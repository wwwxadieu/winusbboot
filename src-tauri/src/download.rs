//! Lấy link ISO chính thức từ Microsoft và tải về có tiến trình, có resume.

use crate::error::{AppError, Result};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadOption {
    pub label: String,
    pub url: String,
    pub language: String,
    pub architecture: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: u64,
    pub percent: f64,
    pub speed_bps: u64,
    pub eta_secs: u64,
}

/// Trang tải công khai của từng dòng sản phẩm.
pub fn official_page(release_id: &str) -> &'static str {
    match release_id {
        id if id.starts_with("win10") => "https://www.microsoft.com/software-download/windows10",
        _ => "https://www.microsoft.com/software-download/windows11",
    }
}

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                  (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

/// Client cho các request nhỏ: trang HTML, file mã băm, API SKU. Ở đây đặt hạn
/// chót cho cả request là hợp lý — không có cái nào đáng chạy quá một phút.
fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(UA)
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(Into::into)
}

/// Client riêng cho việc tải file ISO.
///
/// **Không** đặt `.timeout()`. Trong reqwest đó là hạn chót cho *toàn bộ*
/// request, tính cả thời gian tải hết body — chứ không phải timeout kết nối.
/// Dùng chung client 60 giây với các request nhỏ nghĩa là mọi file ISO đều bị
/// huỷ ở giây thứ 60, vì không file 3–6 GB nào tải xong trong ngần ấy thời
/// gian. Đó chính là lý do tính năng tải tự động chưa bao giờ chạy được.
///
/// Thay bằng hai mốc đúng nghĩa: `connect_timeout` cho lúc bắt tay, và
/// `read_timeout` cho khoảng lặng giữa hai khối dữ liệu. Mạng chậm vẫn tải
/// được, còn kết nối chết thật thì vẫn bị cắt sau một phút không nhận được gì.
fn download_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(UA)
        .connect_timeout(std::time::Duration::from_secs(30))
        .read_timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(Into::into)
}

/// Trích mọi giá trị nằm giữa `open` và `close`. Đủ dùng để bóc vài thuộc tính
/// HTML mà không phải kéo theo cả một thư viện phân tích DOM.
fn extract_between(hay: &str, open: &str, close: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = hay[from..].find(open) {
        let start = from + rel + open.len();
        let Some(end_rel) = hay[start..].find(close) else { break };
        out.push(hay[start..start + end_rel].to_string());
        from = start + end_rel + close.len();
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sku {
    pub id: String,
    pub language: String,
}

/// Bóc danh sách SKU (mỗi ngôn ngữ một SKU) từ HTML Microsoft trả về.
///
/// Trước đây đoạn này chỉ lấy `"id"` **đầu tiên** trong cả trang và bỏ qua hoàn
/// toàn ngôn ngữ được yêu cầu — nghĩa là xin tiếng Nhật thì nhận về SKU nào
/// đứng đầu danh sách, rồi được dán nhãn "Nhật". Tải sai ngôn ngữ mà vẫn báo
/// đúng còn tệ hơn là báo lỗi, nên giờ id và ngôn ngữ luôn đi thành cặp.
pub fn parse_skus(html: &str) -> Vec<Sku> {
    // Microsoft nhúng JSON vào thuộc tính value của <option>, nên dấu nháy bị
    // escape thành &quot;. Chuẩn hoá lại rồi mới bóc.
    let flat = html.replace("&quot;", "\"");

    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = flat[from..].find("\"id\":\"") {
        let start = from + rel + 6;
        let Some(end) = flat[start..].find('"') else { break };
        let id = flat[start..start + end].to_string();

        // Ngôn ngữ nằm cùng object với id; giới hạn tầm nhìn để không vớ phải
        // trường của SKU kế tiếp.
        let window_end = flat[start..]
            .find("\"id\":\"")
            .map(|n| start + n)
            .unwrap_or(flat.len());
        let window = &flat[start..window_end];

        let language = ["\"language\":\"", "\"localizedLanguage\":\""]
            .iter()
            .find_map(|key| {
                let at = window.find(key)? + key.len();
                let stop = window[at..].find('"')?;
                Some(window[at..at + stop].to_string())
            })
            .unwrap_or_default();

        if !id.is_empty() && !language.is_empty() {
            out.push(Sku { id, language });
        }
        from = start + end;
    }
    out
}

/// Chọn SKU đúng ngôn ngữ. So không phân biệt hoa thường, và chấp nhận cả tên
/// đầy đủ lẫn tên rút gọn ("English" khớp "English (United States)").
pub fn pick_sku(skus: &[Sku], want: &str) -> Option<String> {
    let want = want.trim();
    skus.iter()
        .find(|s| s.language.eq_ignore_ascii_case(want))
        .or_else(|| {
            skus.iter().find(|s| {
                s.language.to_lowercase().starts_with(&want.to_lowercase())
            })
        })
        .map(|s| s.id.clone())
}

/// Hỏi Microsoft danh sách link tải ISO cho ngôn ngữ đã chọn.
///
/// Đây là chính luồng mà trang tải chính thức dùng: đăng ký một phiên, hỏi mã
/// SKU theo ngôn ngữ, rồi đổi mã SKU lấy link ký sẵn (link chỉ sống 24 giờ).
/// Microsoft có thể đổi luồng này bất cứ lúc nào, nên mọi lỗi ở đây đều được
/// diễn giải thành lời khuyên "tải thủ công" thay vì một stack trace.
pub async fn fetch_official_links(release_id: &str, language: &str) -> Result<Vec<DownloadOption>> {
    let http = client()?;
    let session = uuid::Uuid::new_v4().to_string();
    let page = official_page(release_id);

    // 1. Lấy mã product edition từ trang tải (thay vì hard-code, vì mã đổi theo
    //    từng bản phát hành).
    let html = http.get(page).send().await?.text().await?;
    let product_id = extract_between(&html, "<option value=\"", "\"")
        .into_iter()
        .find(|v| v.chars().all(|c| c.is_ascii_digit()) && v.len() >= 3)
        .ok_or_else(|| {
            AppError::new(
                "ms_layout_changed",
                "Không tìm thấy mã sản phẩm trên trang tải của Microsoft. Hãy tải ISO thủ công rồi chọn file.",
            )
        })?;

    // 2. Đăng ký phiên. Bỏ qua lỗi ở bước này — nó chỉ dùng để chống bot.
    let _ = http
        .get(format!(
            "https://vlscppe.microsoft.com/fp/tags?org_id=y6jn8c31&session_id={session}"
        ))
        .send()
        .await;

    let base = "https://www.microsoft.com/en-US/api/controls/contentinclude/html\
                ?pageId=a8f8f489-4c7f-463a-9ca6-5cff94d8d041&host=www.microsoft.com\
                &segments=software-download,windows11&query=&sdVersion=2";

    // 3. Đổi product edition + ngôn ngữ lấy mã SKU.
    let sku_html = http
        .get(format!(
            "{base}&action=getskuinformationbyproductedition&sessionId={session}&productEditionId={product_id}"
        ))
        .send()
        .await?
        .text()
        .await?;

    let skus = parse_skus(&sku_html);
    let sku_id = pick_sku(&skus, language).ok_or_else(|| {
        // Nói rõ có những ngôn ngữ nào thay vì chỉ báo "không tìm thấy" —
        // Microsoft đổi cách đặt tên là chuyện thường, và người dùng cần biết
        // phải chọn lại cái gì.
        let available: Vec<&str> = skus.iter().map(|s| s.language.as_str()).take(12).collect();
        AppError::new(
            "ms_no_sku",
            if skus.is_empty() {
                format!(
                    "Microsoft không trả về danh sách ngôn ngữ nào. Hãy tải thủ công \
                     bản {language} từ trang chính thức."
                )
            } else {
                format!(
                    "Microsoft không có bản {language}. Những ngôn ngữ đang có: {}.",
                    available.join(", ")
                )
            },
        )
    })?;

    // 4. Đổi mã SKU lấy link tải thật.
    let links_html = http
        .get(format!(
            "{base}&action=GetProductDownloadLinksBySku&sessionId={session}&skuId={sku_id}"
        ))
        .send()
        .await?
        .text()
        .await?;

    let mut options: Vec<DownloadOption> = extract_between(&links_html, "href=\"", "\"")
        .into_iter()
        .filter(|u| {
            u.starts_with("https://")
                && (u.contains("software.download.prss.microsoft.com")
                    || u.contains("software-download.microsoft.com")
                    || u.to_lowercase().contains(".iso"))
        })
        .map(|url| {
            let arch = if url.to_lowercase().contains("arm64") {
                "arm64"
            } else if url.to_lowercase().contains("x86") {
                "x86"
            } else {
                "x64"
            };
            DownloadOption {
                label: format!("ISO {arch} · {language}"),
                url,
                language: language.to_string(),
                architecture: arch.to_string(),
            }
        })
        .collect();

    options.dedup_by(|a, b| a.url == b.url);

    if options.is_empty() {
        return Err(AppError::new(
            "ms_blocked",
            "Microsoft đã từ chối yêu cầu tải tự động (thường do chặn theo khu vực). \
             Hãy bấm \"Mở trang tải chính thức\" để tải thủ công rồi chọn file ISO.",
        ));
    }
    Ok(options)
}

/// Tải file có báo tiến trình, tự nối tiếp nếu file tải dở còn nằm trên đĩa.
pub async fn download<F>(url: &str, dest: &Path, mut on_progress: F) -> Result<PathBuf>
where
    F: FnMut(DownloadProgress) + Send,
{
    let http = download_client()?;

    // Đã tải được bao nhiêu từ lần trước.
    let existing = tokio::fs::metadata(dest).await.map(|m| m.len()).unwrap_or(0);

    let mut req = http.get(url);
    if existing > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={existing}-"));
    }
    let resp = req.send().await?;

    let resuming = resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    if !resp.status().is_success() {
        let code = resp.status().as_u16();
        // Mã trần không nói được gì cho người dùng. Ba trường hợp hay gặp nhất
        // đều có cách xử lý khác nhau, nên tách ra thành lời khuyên cụ thể.
        let hint = match code {
            403 => " Máy chủ từ chối yêu cầu — một số nhà phát hành chặn tải trực tiếp \
                    từ ngoài mạng gia đình. Hãy bấm \"Mở trang tải chính thức\" để tải \
                    qua trình duyệt rồi quay lại chọn file.",
            404 => " Không còn file này trên máy chủ — nhiều khả năng bản vá mới đã ra \
                    và bản cũ bị gỡ. Hãy tải từ trang chính thức.",
            429 | 503 => " Máy chủ đang quá tải hoặc giới hạn lượt tải. Thử lại sau vài \
                          phút, hoặc tải từ trang chính thức.",
            _ => "",
        };
        return Err(AppError::new(
            "http",
            format!("Máy chủ trả về mã {code} khi tải file.{hint}"),
        ));
    }

    // Máy chủ không hỗ trợ nối tiếp thì phải tải lại từ đầu.
    let start = if resuming { existing } else { 0 };
    let total = resp.content_length().unwrap_or(0) + start;

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(!resuming)
        .append(resuming)
        .open(dest)
        .await?;

    let mut downloaded = start;
    let began = std::time::Instant::now();
    let mut last_tick = std::time::Instant::now();
    let mut stream = resp.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;

        // Báo lên UI tối đa 4 lần/giây, đủ mượt mà không làm nghẽn kênh sự kiện.
        if last_tick.elapsed().as_millis() >= 250 {
            last_tick = std::time::Instant::now();
            let elapsed = began.elapsed().as_secs_f64().max(0.001);
            let speed = ((downloaded - start) as f64 / elapsed) as u64;
            on_progress(DownloadProgress {
                downloaded,
                total,
                percent: if total > 0 { downloaded as f64 / total as f64 * 100.0 } else { 0.0 },
                speed_bps: speed,
                eta_secs: if speed > 0 && total > downloaded { (total - downloaded) / speed } else { 0 },
            });
        }
    }
    file.flush().await?;

    on_progress(DownloadProgress {
        downloaded,
        total: total.max(downloaded),
        percent: 100.0,
        speed_bps: 0,
        eta_secs: 0,
    });
    Ok(dest.to_path_buf())
}

/// Tính SHA-256 của file ISO để đối chiếu với giá trị Microsoft công bố.
pub async fn sha256<F>(path: &Path, mut on_progress: F) -> Result<String>
where
    F: FnMut(f64) + Send,
{
    use tokio::io::AsyncReadExt;

    let total = tokio::fs::metadata(path).await?.len().max(1);
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 4 * 1024 * 1024];
    let mut read_total = 0u64;
    let mut last_tick = std::time::Instant::now();

    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        read_total += n as u64;
        if last_tick.elapsed().as_millis() >= 250 {
            last_tick = std::time::Instant::now();
            on_progress(read_total as f64 / total as f64 * 100.0);
        }
    }
    on_progress(100.0);
    Ok(hex::encode(hasher.finalize()))
}

// ---------------------------------------------------------------------------
// Giải link tải của distro qua file mã băm
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedIso {
    pub url: String,
    pub filename: String,
    /// Mã băm chính thức do chính dự án công bố, để đối chiếu sau khi tải xong.
    pub sha256: String,
}

/// Tách một dòng trong file mã băm thành `(mã băm, tên file)`.
///
/// Hai định dạng cùng tồn tại trong thực tế và phải nhận được cả hai:
/// GNU coreutils viết `<mã băm>  <tên file>` (dấu `*` trước tên file nghĩa là
/// chế độ nhị phân), còn BSD viết `SHA256 (<tên file>) = <mã băm>`.
fn parse_checksum_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();

    // Dạng BSD.
    if let Some(rest) = line.strip_prefix("SHA256 (") {
        let (name, hash) = rest.split_once(") = ")?;
        return Some((hash.trim().to_string(), name.trim().to_string()));
    }

    // Dạng GNU: mã băm, khoảng trắng, rồi tên file.
    let (hash, name) = line.split_once(char::is_whitespace)?;
    let name = name.trim_start().trim_start_matches('*').trim();
    if name.is_empty() {
        return None;
    }
    Some((hash.trim().to_string(), name.to_string()))
}

fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Chọn đúng dòng ISO trong nội dung file mã băm.
///
/// Tách riêng khỏi phần mạng để kiểm thử được: đây mới là chỗ dễ chọn nhầm.
/// Một file mã băm thường liệt kê nhiều thứ cùng lúc — bản desktop, bản live,
/// bản netinst, kèm cả `.zsync` và `.torrent` — nên phải lọc đúng file `.iso`
/// rồi mới khớp chuỗi nhận dạng.
pub fn pick_iso(body: &str, want: &str) -> Option<(String, String)> {
    let mut matches: Vec<(String, String)> = body
        .lines()
        .filter_map(parse_checksum_line)
        .filter(|(hash, name)| {
            is_sha256(hash) && name.to_lowercase().ends_with(".iso") && name.contains(want)
        })
        .collect();

    // Nhiều dòng cùng khớp (ví dụ bản có và không có gói ngôn ngữ) thì lấy tên
    // ngắn nhất — đó gần như luôn là bản tiêu chuẩn.
    matches.sort_by_key(|(_, name)| (name.len(), name.clone()));
    matches.into_iter().next()
}

/// Đọc file mã băm của distro để biết tên file ISO hiện hành và mã băm của nó.
///
/// Không ghi cứng link ISO vào danh mục vì tên file đổi theo từng bản vá nhỏ.
/// Thư mục chứa thì cố định, nên đọc file mã băm trong đó là cách duy nhất vừa
/// luôn ra đúng file mới nhất, vừa có sẵn mã băm để đối chiếu.
pub async fn resolve_distro_iso(checksum_url: &str, want: &str) -> Result<ResolvedIso> {
    let (dir, _) = checksum_url.rsplit_once('/').ok_or_else(|| {
        AppError::new("bad_checksum_url", "Địa chỉ file mã băm không hợp lệ.")
    })?;

    let body = client()?
        .get(checksum_url)
        .send()
        .await?
        .error_for_status()
        .map_err(|e| {
            AppError::new(
                "checksum_unreachable",
                format!("Không tải được danh sách mã băm của bản này: {e}"),
            )
        })?
        .text()
        .await?;

    let (sha256, filename) = pick_iso(&body, want).ok_or_else(|| {
        AppError::new(
            "iso_not_listed",
            "Không tìm thấy file ISO nào trong danh sách mã băm chính thức. \
             Nhiều khả năng dự án đã đổi cách đặt tên file — hãy tải thủ công từ trang chính thức.",
        )
    })?;

    Ok(ResolvedIso {
        url: format!("{dir}/{filename}"),
        filename,
        sha256,
    })
}

#[cfg(test)]
mod timeout_tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    /// Máy chủ tí hon nhả `n` byte, mỗi byte cách nhau `gap`. Tổng thời gian
    /// truyền cố ý dài hơn hạn chót tổng mà test đặt ra, để tái hiện đúng cảnh
    /// một file ISO vài GB tải lâu hơn một phút.
    async fn trickle(n: usize, gap: std::time::Duration) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let Ok((mut sock, _)) = listener.accept().await else { return };
            // Đọc cho hết dòng request rồi mới trả lời.
            let mut buf = [0u8; 1024];
            let _ = tokio::io::AsyncReadExt::read(&mut sock, &mut buf).await;

            let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {n}\r\n\r\n");
            if sock.write_all(head.as_bytes()).await.is_err() {
                return;
            }
            for _ in 0..n {
                if sock.write_all(b"x").await.is_err() {
                    return;
                }
                let _ = sock.flush().await;
                tokio::time::sleep(gap).await;
            }
        });

        format!("http://{addr}/iso")
    }

    const GAP: std::time::Duration = std::time::Duration::from_millis(120);
    const CHUNKS: usize = 25; // ~3 giây, dài hơn hạn chót 1 giây bên dưới

    /// Tái hiện lỗi: `.timeout()` của reqwest là hạn chót cho **toàn bộ**
    /// request, nên một lần truyền dài hơn nó luôn bị huỷ giữa chừng — dù dữ
    /// liệu vẫn đang chảy đều. Đây đúng là thứ đã xảy ra với mọi file ISO.
    #[tokio::test]
    async fn a_total_deadline_kills_a_transfer_that_is_still_flowing() {
        let url = trickle(CHUNKS, GAP).await;
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(1))
            .build()
            .unwrap();

        let err = async {
            let resp = http.get(&url).send().await?;
            resp.bytes().await
        }
        .await
        .expect_err("hạn chót tổng phải cắt ngang lần truyền này");

        assert!(err.is_timeout(), "phải là lỗi timeout, nhận được: {err}");
    }

    /// Và bản sửa: cùng lần truyền đó, nhưng hạn chót đặt vào **khoảng lặng
    /// giữa hai khối** thay vì vào tổng thời gian, thì chạy trọn vẹn.
    #[tokio::test]
    async fn a_read_timeout_lets_a_slow_but_healthy_transfer_finish() {
        let url = trickle(CHUNKS, GAP).await;
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .read_timeout(std::time::Duration::from_secs(1))
            .build()
            .unwrap();

        let body = http.get(&url).send().await.unwrap().bytes().await
            .expect("truyền chậm nhưng đều thì phải xong");
        assert_eq!(body.len(), CHUNKS, "phải nhận đủ số byte");
    }

    /// Client dùng để tải phải là loại thứ hai. Kiểm tra bằng chính nó thay vì
    /// tin vào cấu hình: đây là chỗ đã hỏng một lần rồi.
    #[tokio::test]
    async fn the_real_download_client_has_no_total_deadline() {
        let url = trickle(CHUNKS, GAP).await;
        let http = download_client().unwrap();

        let body = http.get(&url).send().await.unwrap().bytes().await
            .expect("download_client() không được đặt hạn chót cho cả request");
        assert_eq!(body.len(), CHUNKS);
    }
}

#[cfg(test)]
mod sku_tests {
    use super::*;

    /// Dạng Microsoft thật sự trả về: JSON nhúng trong thuộc tính value của
    /// <option>, dấu nháy bị escape thành &quot;.
    const SKU_HTML: &str = r#"
      <select id="product-languages">
        <option value="">Chọn một</option>
        <option value="{&quot;id&quot;:&quot;11111&quot;,&quot;language&quot;:&quot;Arabic&quot;,&quot;localizedLanguage&quot;:&quot;العربية&quot;}">Arabic</option>
        <option value="{&quot;id&quot;:&quot;22222&quot;,&quot;language&quot;:&quot;English (United States)&quot;,&quot;localizedLanguage&quot;:&quot;English (United States)&quot;}">English</option>
        <option value="{&quot;id&quot;:&quot;33333&quot;,&quot;language&quot;:&quot;Japanese&quot;,&quot;localizedLanguage&quot;:&quot;日本語&quot;}">Japanese</option>
      </select>"#;

    #[test]
    fn every_sku_keeps_its_id_paired_with_its_language() {
        let skus = parse_skus(SKU_HTML);
        assert_eq!(skus.len(), 3);
        assert_eq!(skus[0], Sku { id: "11111".into(), language: "Arabic".into() });
        assert_eq!(skus[2], Sku { id: "33333".into(), language: "Japanese".into() });
    }

    /// Lỗi cũ: lấy `"id"` đầu tiên trong cả trang rồi bỏ qua ngôn ngữ, nên xin
    /// tiếng Nhật lại nhận về SKU tiếng Ả Rập mà vẫn báo là tiếng Nhật.
    #[test]
    fn the_requested_language_decides_the_sku_not_the_page_order() {
        let skus = parse_skus(SKU_HTML);
        assert_eq!(pick_sku(&skus, "Japanese").as_deref(), Some("33333"));
        assert_eq!(pick_sku(&skus, "English (United States)").as_deref(), Some("22222"));
        assert_ne!(
            pick_sku(&skus, "Japanese").as_deref(),
            Some("11111"),
            "không được rơi về SKU đứng đầu danh sách"
        );
    }

    #[test]
    fn matching_ignores_case_and_accepts_a_shortened_name() {
        let skus = parse_skus(SKU_HTML);
        assert_eq!(pick_sku(&skus, "japanese").as_deref(), Some("33333"));
        assert_eq!(pick_sku(&skus, "English").as_deref(), Some("22222"));
    }

    /// Tiếng Việt không có trong danh sách của Microsoft. Trả về None để bên
    /// gọi báo lỗi kèm danh sách ngôn ngữ đang có, thay vì tải nhầm bản khác.
    #[test]
    fn a_language_microsoft_does_not_publish_yields_nothing() {
        let skus = parse_skus(SKU_HTML);
        assert_eq!(pick_sku(&skus, "Vietnamese"), None);
    }

    #[test]
    fn garbage_html_yields_no_skus_rather_than_nonsense() {
        assert!(parse_skus("").is_empty());
        assert!(parse_skus("<html>404 Not Found</html>").is_empty());
        // Có id nhưng không có ngôn ngữ đi kèm thì không dùng được để chọn.
        assert!(parse_skus(r#"{"id":"9999"}"#).is_empty());
    }
}

#[cfg(test)]
mod distro_tests {
    use super::*;

    #[test]
    fn both_checksum_formats_are_understood() {
        assert_eq!(
            parse_checksum_line(
                "e240e4b8c3b1a0f00d5c1e1b6b0a4f0e2d3c4b5a6978877665544332211aabbc *ubuntu-24.04.3-desktop-amd64.iso"
            ),
            Some((
                "e240e4b8c3b1a0f00d5c1e1b6b0a4f0e2d3c4b5a6978877665544332211aabbc".into(),
                "ubuntu-24.04.3-desktop-amd64.iso".into()
            ))
        );
        assert_eq!(
            parse_checksum_line(
                "SHA256 (Fedora-Workstation-Live-x86_64-43-1.1.iso) = 1111111111111111111111111111111111111111111111111111111111111111"
            ),
            Some((
                "1111111111111111111111111111111111111111111111111111111111111111".into(),
                "Fedora-Workstation-Live-x86_64-43-1.1.iso".into()
            ))
        );
    }

    const UBUNTU: &str = "\
e240e4b8c3b1a0f00d5c1e1b6b0a4f0e2d3c4b5a6978877665544332211aabbc *ubuntu-24.04.3-desktop-amd64.iso
1234567890123456789012345678901234567890123456789012345678901234 *ubuntu-24.04.3-desktop-amd64.iso.zsync
abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd *ubuntu-24.04.3-live-server-amd64.iso
";

    #[test]
    fn the_desktop_iso_is_picked_not_the_server_or_the_zsync() {
        let (hash, name) = pick_iso(UBUNTU, "desktop-amd64.iso").unwrap();
        assert_eq!(name, "ubuntu-24.04.3-desktop-amd64.iso");
        assert!(is_sha256(&hash));
    }

    /// Đọc hụt còn hơn tải nhầm một file rồi ghi đè cả ổ USB bằng nó.
    #[test]
    fn nothing_is_returned_when_no_line_matches() {
        assert_eq!(pick_iso(UBUNTU, "arm64.iso"), None);
        assert_eq!(pick_iso("", "desktop-amd64.iso"), None);
        assert_eq!(pick_iso("<html>404 Not Found</html>", "desktop-amd64.iso"), None);
    }

    /// Mã băm cụt hoặc không phải hex là dấu hiệu file đã đổi định dạng — bỏ qua
    /// thay vì trả về một giá trị không dùng để đối chiếu được.
    #[test]
    fn malformed_hashes_are_rejected() {
        let body = "deadbeef *ubuntu-24.04.3-desktop-amd64.iso\n";
        assert_eq!(pick_iso(body, "desktop-amd64.iso"), None);
    }
}

#[cfg(test)]
mod live_probe {
    use super::*;

    /// Giải link thật rồi tải thử vài KB đầu, để chắc URL dựng ra tải được.
    ///
    /// Có gọi mạng nên không chạy trong CI — đây là công cụ bảo trì, chạy tay
    /// mỗi khi nghi một địa chỉ trong danh mục đã hỏng:
    ///
    /// ```text
    /// cargo test --manifest-path src-tauri/Cargo.toml live_probe -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore = "gọi mạng thật"]
    async fn resolved_urls_are_actually_downloadable() {
        for r in crate::distro::builtin().into_iter().filter(|r| r.checksum_url.is_some()) {
            let url = r.checksum_url.clone().unwrap();
            match resolve_distro_iso(&url, &r.iso_match).await {
                Ok(iso) => {
                    let head = client().unwrap()
                        .get(&iso.url)
                        .header(reqwest::header::RANGE, "bytes=0-2047")
                        .send().await;
                    match head {
                        Ok(resp) => {
                            let code = resp.status().as_u16();
                            let n = resp.bytes().await.map(|b| b.len()).unwrap_or(0);
                            eprintln!("{:<20} http={code} {n} byte  {}", r.id, iso.filename);
                        }
                        Err(e) => eprintln!("{:<20} TẢI HỎNG  {e}", r.id),
                    }
                }
                Err(e) => eprintln!("{:<20} GIẢI HỎNG  {}", r.id, e.message),
            }
        }
    }
}
