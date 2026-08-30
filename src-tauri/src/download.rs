//! Tải file ảnh đĩa có tiến trình và có nối tiếp, kèm phần giải link ISO Linux.

use crate::error::{AppError, Result};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

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

/// Client cho các request nhỏ: file mã băm, trang chỉ mục của các dự án Linux.
/// Ở đây đặt hạn chót cho cả request là hợp lý — không cái nào đáng chạy quá
/// một phút.
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

// ---------------------------------------------------------------------------
// Thư mục tải của ứng dụng
// ---------------------------------------------------------------------------

/// Nơi ứng dụng tự tải ISO về.
///
/// Trước đây bước tải bắt người dùng chọn thư mục thủ công, rồi để lại một file
/// 3–6 GB nằm đó vĩnh viễn — phần lớn người dùng ghi xong USB là không cần tới
/// nó nữa. Giờ ứng dụng tự quản một thư mục riêng, và chỉ những file nằm trong
/// đó mới được phép xoá tự động.
pub fn managed_dir() -> std::path::PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("GetWinUSB").join("iso")
}

/// File này có phải do ứng dụng tải về thư mục riêng của nó không.
///
/// Đây là hàng rào duy nhất giữa "dọn dẹp file tạm" và "xoá mất file ISO người
/// dùng đã tự tải về từ trước". So sau khi chuẩn hoá cả hai đường dẫn, nên
/// `..` hay dấu phân cách khác kiểu đều không lách qua được.
pub fn is_managed(path: &std::path::Path) -> bool {
    let dir = managed_dir();
    // canonicalize() chỉ chạy được với đường dẫn có thật; file đã xoá rồi thì
    // lùi về so sánh dạng thô.
    let norm = |p: &std::path::Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    norm(path).starts_with(norm(&dir))
}

/// Xoá file ISO ứng dụng đã tải, sau khi ghi xong USB.
///
/// Từ chối mọi đường dẫn nằm ngoài thư mục quản lý. Người dùng chọn file ISO có
/// sẵn trên máy thì file đó là của họ — xoá nhầm một file 6 GB họ đã tải cả
/// buổi là thiệt hại không sửa được.
pub fn discard(path: &std::path::Path) -> Result<bool> {
    if !is_managed(path) {
        return Err(AppError::new(
            "not_managed",
            "Chỉ xoá được file do ứng dụng tự tải về. File bạn tự chọn thì ứng dụng không đụng tới.",
        ));
    }
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        // Không còn ở đó thì coi như đã dọn xong.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
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
    ///
    /// `None` với Windows: Microsoft không công bố mã băm ở đâu trong luồng tải
    /// của họ. Giao diện vì thế chỉ hiện mã băm tính được chứ không nói là
    /// "khớp" hay "không khớp" — không có gì để so thì đừng vờ như có.
    pub sha256: Option<String>,
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
        sha256: Some(sha256),
    })
}

// ---------------------------------------------------------------------------
// Lấy link ISO Windows từ Microsoft
// ---------------------------------------------------------------------------

/// Endpoint hiện hành của trang tải Microsoft.
///
/// Luồng cũ dùng `/api/controls/contentinclude/html` và endpoint đó đã bị gỡ —
/// trả 404 với mọi pageId, nên tính năng tải tự động từng bị bỏ hẳn. Microsoft
/// đã dựng lại luồng mới ở địa chỉ này; hình dạng dưới đây đọc thẳng từ JS của
/// trang tải nên khớp từng tham số với thứ trình duyệt thật gửi đi.
const CONNECTOR: &str = "https://www.microsoft.com/software-download-connector/api";

/// Mã hồ sơ cố định trang tải gắn vào mọi request.
const PROFILE: &str = "606624d44113";

#[derive(Debug, Clone, Deserialize)]
pub struct Sku {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "Language")]
    pub language: String,
    #[serde(rename = "LocalizedLanguage", default)]
    pub localized_language: String,
}

#[derive(Debug, Deserialize)]
struct SkuResponse {
    #[serde(rename = "Skus", default)]
    skus: Vec<Sku>,
    #[serde(rename = "ValidationContainer", default)]
    validation: Option<ErrorContainer>,
    #[serde(rename = "Errors", default)]
    errors: Vec<MsError>,
}

#[derive(Debug, Default, Deserialize)]
struct ErrorContainer {
    #[serde(rename = "Errors", default)]
    errors: Vec<MsError>,
}

#[derive(Debug, Clone, Deserialize)]
struct MsError {
    #[serde(rename = "Key", default)]
    key: String,
    #[serde(rename = "Value", default)]
    value: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DownloadLink {
    #[serde(rename = "Uri")]
    pub uri: String,
    #[serde(rename = "DownloadType", default)]
    pub download_type: String,
}

#[derive(Debug, Deserialize)]
struct LinksResponse {
    #[serde(rename = "ProductDownloadOptions", default)]
    options: Vec<DownloadLink>,
    #[serde(rename = "Errors", default)]
    errors: Vec<MsError>,
}

/// Bóc mã product edition từ trang tải.
///
/// Mã này đổi theo từng bản phát hành (bản 25H2 mang mã khác 24H2), nên đọc từ
/// trang thay vì ghi cứng — ghi cứng thì tới bản sau là hỏng mà không ai biết.
pub fn parse_product_edition(html: &str) -> Option<String> {
    let mut from = 0usize;
    while let Some(rel) = html[from..].find("<option value=\"") {
        let start = from + rel + 15;
        let end = html[start..].find('"')? + start;
        let value = &html[start..end];
        if value.len() >= 3 && value.chars().all(|c| c.is_ascii_digit()) {
            return Some(value.to_string());
        }
        from = end;
    }
    None
}

/// Chọn SKU đúng ngôn ngữ.
///
/// So bằng dấu bằng chứ **không** so kiểu "bắt đầu bằng": Microsoft có cả
/// "English" lẫn "English International", nên so tiền tố sẽ trả về bản quốc tế
/// cho người chọn bản Mỹ. Tải sai ngôn ngữ mà vẫn báo đúng còn tệ hơn báo lỗi.
pub fn pick_sku<'a>(skus: &'a [Sku], want: &str) -> Option<&'a Sku> {
    let want = want.trim();
    skus.iter().find(|s| {
        s.language.eq_ignore_ascii_case(want) || s.localized_language.eq_ignore_ascii_case(want)
    })
}

/// Chọn link x64 trong danh sách Microsoft trả về.
pub fn pick_link(links: &[DownloadLink]) -> Option<&DownloadLink> {
    links
        .iter()
        .find(|l| {
            let hay = format!("{} {}", l.uri, l.download_type).to_lowercase();
            hay.contains("x64") || hay.contains("64-bit")
        })
        .or_else(|| links.first())
}

/// Tên file từ một link đã ký. Link của Microsoft luôn kèm chuỗi truy vấn dài.
pub fn filename_from_url(url: &str) -> String {
    url.split('?')
        .next()
        .unwrap_or(url)
        .rsplit('/')
        .next()
        .filter(|n| n.to_lowercase().ends_with(".iso"))
        .unwrap_or("windows.iso")
        .to_string()
}

/// Diễn giải lỗi Microsoft trả về thành lời khuyên cụ thể.
fn explain(errors: &[MsError]) -> Option<AppError> {
    let first = errors.first()?;
    let key = first.key.to_lowercase();
    // Đây là lỗi hay gặp nhất và cũng dễ hiểu lầm nhất: Microsoft chặn theo
    // mạng và khu vực, không phải app hỏng hay ISO hỏng.
    if key.contains("sentinel") {
        return Some(AppError::new(
            "ms_blocked",
            "Microsoft từ chối cấp link tải cho kết nối mạng này. Họ chặn theo địa chỉ IP — \
             thường gặp với VPN, mạng công ty, hoặc máy chủ. Hãy tắt VPN rồi thử lại, hoặc bấm \
             \"Mở trang tải chính thức\" để tải bằng trình duyệt.",
        ));
    }
    Some(AppError::new(
        "ms_error",
        format!(
            "Microsoft từ chối yêu cầu tải ({}). Hãy tải bằng trình duyệt qua nút \"Mở trang tải \
             chính thức\".",
            if first.value.is_empty() { first.key.clone() } else { first.value.clone() }
        ),
    ))
}

/// Hỏi Microsoft link tải ISO Windows cho ngôn ngữ đã chọn.
///
/// Ba bước, đúng như trình duyệt làm: đọc mã product edition từ trang tải, đổi
/// mã đó lấy danh sách SKU theo ngôn ngữ, rồi đổi SKU lấy link ký sẵn (link chỉ
/// sống 24 giờ). Bước đăng ký phiên ở giữa chỉ để chống bot nên lỗi thì bỏ qua.
pub async fn resolve_windows_iso(release_id: &str, ms_language: &str) -> Result<ResolvedIso> {
    if ms_language.trim().is_empty() {
        return Err(AppError::new(
            "no_language",
            "Chưa chọn ngôn ngữ bộ cài. Hãy quay lại bước Phiên bản để chọn.",
        ));
    }

    let http = client()?;
    let session = uuid::Uuid::new_v4().to_string();
    let page = official_page(release_id);

    let html = http.get(page).send().await?.text().await?;
    let product_id = parse_product_edition(&html).ok_or_else(|| {
        AppError::new(
            "ms_layout_changed",
            "Không tìm thấy mã sản phẩm trên trang tải của Microsoft. Hãy tải ISO thủ công rồi \
             chọn file.",
        )
    })?;

    let _ = http
        .get(format!(
            "https://vlscppe.microsoft.com/fp/tags?org_id=y6jn8c31&session_id={session}"
        ))
        .send()
        .await;

    let sku_url = format!(
        "{CONNECTOR}/getskuinformationbyproductedition?profile={PROFILE}\
         &ProductEditionId={product_id}&SKU=undefined&friendlyFileName=undefined\
         &Locale=en-US&sessionID={session}"
    );
    let sku_body = http.get(sku_url).send().await?.text().await?;
    let skus: SkuResponse = serde_json::from_str(&sku_body).map_err(|_| {
        AppError::new(
            "ms_bad_reply",
            "Microsoft trả về dữ liệu không đọc được ở bước chọn ngôn ngữ. Hãy tải thủ công.",
        )
    })?;

    if let Some(e) = explain(&skus.errors)
        .or_else(|| explain(&skus.validation.unwrap_or_default().errors))
    {
        return Err(e);
    }

    let sku = pick_sku(&skus.skus, ms_language).ok_or_else(|| {
        let available: Vec<&str> = skus.skus.iter().map(|s| s.language.as_str()).take(12).collect();
        AppError::new(
            "ms_no_sku",
            if skus.skus.is_empty() {
                "Microsoft không trả về ngôn ngữ nào. Hãy tải thủ công từ trang chính thức.".into()
            } else {
                format!(
                    "Microsoft không phát hành bản {ms_language}. Những ngôn ngữ đang có: {}.",
                    available.join(", ")
                )
            },
        )
    })?;

    let links_url = format!(
        "{CONNECTOR}/GetProductDownloadLinksBySku?profile={PROFILE}\
         &ProductEditionId=undefined&SKU={}&friendlyFileName=undefined\
         &Locale=en-US&sessionID={session}",
        sku.id
    );
    let links_body = http.get(links_url).send().await?.text().await?;
    let links: LinksResponse = serde_json::from_str(&links_body).map_err(|_| {
        AppError::new(
            "ms_bad_reply",
            "Microsoft trả về dữ liệu không đọc được ở bước lấy link. Hãy tải thủ công.",
        )
    })?;

    if let Some(e) = explain(&links.errors) {
        return Err(e);
    }

    let link = pick_link(&links.options).ok_or_else(|| {
        AppError::new(
            "ms_no_link",
            "Microsoft không trả về link tải nào cho bản này. Hãy tải thủ công từ trang chính thức.",
        )
    })?;

    Ok(ResolvedIso {
        filename: filename_from_url(&link.uri),
        url: link.uri.clone(),
        // Microsoft không công bố mã băm cho ISO tải qua luồng này.
        sha256: None,
    })
}

#[cfg(test)]
mod microsoft_tests {
    use super::*;

    /// Cắt từ đúng phản hồi thật của Microsoft, lấy ngày 30/08/2026.
    const SKUS: &str = r#"{"ValidationContainer":{"Errors":[]},"Skus":[
      {"Id":"20046","Language":"English","LocalizedProductDisplayName":"Windows 11  English",
       "LocalizedLanguage":"English (United States)","ProductDisplayName":"Windows 11 25H2__V2"},
      {"Id":"20047","Language":"English International",
       "LocalizedProductDisplayName":"Windows 11  English International",
       "LocalizedLanguage":"English International","ProductDisplayName":"Windows 11 25H2__V2"},
      {"Id":"20057","Language":"Japanese","LocalizedProductDisplayName":"Windows 11  Japanese",
       "LocalizedLanguage":"Japanese","ProductDisplayName":"Windows 11 25H2__V2"}]}"#;

    fn skus() -> Vec<Sku> {
        serde_json::from_str::<SkuResponse>(SKUS).unwrap().skus
    }

    #[test]
    fn the_english_sku_is_never_confused_with_english_international() {
        // Đây là lý do khâu chọn SKU so bằng dấu bằng chứ không so tiền tố:
        // "English" là tiền tố của "English International", nên so tiền tố sẽ
        // đưa người chọn bản Mỹ sang bản quốc tế mà vẫn báo là đúng.
        assert_eq!(pick_sku(&skus(), "English").unwrap().id, "20046");
        assert_eq!(pick_sku(&skus(), "English International").unwrap().id, "20047");
    }

    #[test]
    fn the_older_microsoft_name_still_matches_through_the_localized_field() {
        // Microsoft gọi bản này là "English" ở trường Language nhưng
        // "English (United States)" ở trường LocalizedLanguage. Nhận cả hai thì
        // đổi cách đặt tên một lần nữa cũng không làm hỏng.
        assert_eq!(pick_sku(&skus(), "English (United States)").unwrap().id, "20046");
    }

    #[test]
    fn a_language_microsoft_does_not_publish_yields_nothing() {
        assert!(pick_sku(&skus(), "Vietnamese").is_none());
    }

    #[test]
    fn the_product_edition_id_is_read_from_the_download_page() {
        let html = r#"<select id="product-edition"><option value="">Select</option>
          <option value="3321">Windows 11 (multi-edition ISO for x64 devices)</option></select>"#;
        assert_eq!(parse_product_edition(html).as_deref(), Some("3321"));
    }

    #[test]
    fn a_page_without_any_edition_gives_up_instead_of_guessing() {
        assert!(parse_product_edition("<html>bảo trì</html>").is_none());
    }

    #[test]
    fn a_sentinel_rejection_is_explained_as_a_network_block() {
        // Phản hồi thật khi gọi từ một địa chỉ IP bị Microsoft chặn. Không diễn
        // giải thì người dùng chỉ thấy một chuỗi tiếng Anh khó hiểu và tưởng
        // ứng dụng hỏng.
        let body = r#"{"Errors":[{"Key":"ErrorSettings.SentinelReject",
                       "Value":"Sentinel marked this request as rejected.","Type":8}]}"#;
        let links: LinksResponse = serde_json::from_str(body).unwrap();
        let e = explain(&links.errors).expect("phải có lời giải thích");
        assert_eq!(e.code, "ms_blocked");
        assert!(e.message.contains("VPN"), "{}", e.message);
    }

    #[test]
    fn no_errors_means_no_explanation() {
        let ok: LinksResponse = serde_json::from_str(r#"{"ProductDownloadOptions":[]}"#).unwrap();
        assert!(explain(&ok.errors).is_none());
    }

    #[test]
    fn the_x64_link_is_chosen_over_the_arm_one() {
        let links = vec![
            DownloadLink { uri: "https://x/Win11_arm64.iso?t=1".into(), download_type: "ARM64 Download".into() },
            DownloadLink { uri: "https://x/Win11_x64.iso?t=2".into(), download_type: "64-bit Download".into() },
        ];
        assert!(pick_link(&links).unwrap().uri.contains("x64"));
    }

    #[test]
    fn the_file_name_comes_from_the_signed_url_without_its_query_string() {
        let url = "https://software.download.prss.microsoft.com/dbazure/\
                   Win11_25H2_EnglishInternational_x64v2.iso?t=abc&P1=123&P2=456";
        assert_eq!(filename_from_url(url), "Win11_25H2_EnglishInternational_x64v2.iso");
    }

    #[test]
    fn a_url_that_is_not_an_iso_falls_back_to_a_safe_name() {
        // Không bao giờ dựng tên file từ một chuỗi lạ: tên đó sẽ thành đường dẫn
        // ghi vào thư mục của ứng dụng.
        assert_eq!(filename_from_url("https://example.com/"), "windows.iso");
        assert_eq!(filename_from_url("https://example.com/tai-ve.php?id=9"), "windows.iso");
    }
}

#[cfg(test)]
mod discard_tests {
    use super::*;

    /// Hàng rào quan trọng nhất của tính năng dọn dẹp: file người dùng tự chọn
    /// là của họ. Xoá nhầm một file ISO 6 GB họ đã tải cả buổi là thiệt hại
    /// không sửa được, nên đây phải là một từ chối thẳng thừng.
    #[test]
    fn a_file_the_user_chose_themselves_is_never_touched() {
        for outside in [
            "D:\\ISO\\Win11.iso",
            "/home/nguoidung/Downloads/ubuntu.iso",
            "C:\\Users\\An\\Desktop\\mint.iso",
        ] {
            let p = std::path::Path::new(outside);
            assert!(!is_managed(p), "{outside} không nằm trong thư mục quản lý");
            let err = discard(p).expect_err("phải từ chối");
            assert_eq!(err.code, "not_managed");
        }
    }

    #[test]
    fn a_file_the_app_downloaded_is_recognised() {
        let inside = managed_dir().join("ubuntu-24.04.3-desktop-amd64.iso");
        assert!(is_managed(&inside));
    }

    /// Thư mục cha trùng tiền tố nhưng không phải thư mục quản lý thì không
    /// được lọt — "GetWinUSB-cu" bắt đầu bằng "GetWinUSB".
    #[test]
    fn a_lookalike_sibling_directory_does_not_count_as_managed() {
        let sneaky = managed_dir()
            .parent().unwrap()
            .parent().unwrap()
            .join("GetWinUSB-cu").join("iso").join("x.iso");
        assert!(!is_managed(&sneaky), "{} không được coi là thư mục quản lý", sneaky.display());
    }

    /// Xoá một file đã không còn ở đó là chuyện bình thường — người dùng có thể
    /// đã tự xoá tay. Không được coi là lỗi.
    #[test]
    fn discarding_an_already_gone_file_is_not_an_error() {
        let gone = managed_dir().join("khong-ton-tai-12345.iso");
        assert_eq!(discard(&gone).unwrap(), false);
    }
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
