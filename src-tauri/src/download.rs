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

fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(UA)
        .timeout(std::time::Duration::from_secs(60))
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

    let sku_id = extract_between(&sku_html, "\"id\":\"", "\"")
        .into_iter()
        .next()
        .or_else(|| {
            extract_between(&sku_html, "<option value=\"{&quot;id&quot;:&quot;", "&quot;")
                .into_iter()
                .next()
        })
        .ok_or_else(|| {
            AppError::new(
                "ms_no_sku",
                format!("Microsoft không trả về bản tải cho ngôn ngữ {language}. Hãy tải thủ công từ trang chính thức."),
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
    let http = client()?;

    // Đã tải được bao nhiêu từ lần trước.
    let existing = tokio::fs::metadata(dest).await.map(|m| m.len()).unwrap_or(0);

    let mut req = http.get(url);
    if existing > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={existing}-"));
    }
    let resp = req.send().await?;

    let resuming = resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    if !resp.status().is_success() {
        return Err(AppError::new(
            "http",
            format!("Máy chủ trả về mã {} khi tải file.", resp.status()),
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
