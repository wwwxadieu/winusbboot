//! Đọc bảng vòng đời sản phẩm trên trang release-health của Microsoft.
//!
//! Microsoft không phát hành API JSON ổn định cho dữ liệu này, nên cách duy nhất
//! là bóc tách HTML — và HTML thì có thể đổi bất cứ lúc nào. Toàn bộ module này
//! vì thế được viết theo tinh thần: thà không cập nhật còn hơn cập nhật sai.
//!
//! Ba nguyên tắc chống vỡ:
//!
//! 1. **Ánh xạ cột theo tiêu đề, không theo vị trí.** Microsoft chèn thêm hay
//!    đổi chỗ một cột là chuyện thường; đọc theo thứ tự cột thì sẽ lấy nhầm
//!    ngày mà không hề báo lỗi — kiểu hỏng nguy hiểm nhất.
//! 2. **Hợp nhất chứ không thay thế.** Trang của Microsoft chỉ có số build và
//!    ngày tháng; những thứ như yêu cầu TPM hay cách lấy ISO vẫn lấy từ bảng
//!    nhúng. Đọc hụt một dòng thì mất một bản cập nhật, không mất cả danh mục.
//! 3. **Nghi ngờ thì bỏ qua.** Dòng thiếu ngày hết hỗ trợ, dòng của bản chỉ
//!    dành cho máy mới, hay bản đã quá hạn mà ứng dụng chưa từng biết — đều bị
//!    loại thay vì đoán bừa.

use crate::catalog::{self, CatalogOrigin, CatalogState, SourceKind, WindowsRelease};
use crate::error::{AppError, Result};

const WIN11_URL: &str =
    "https://learn.microsoft.com/en-us/windows/release-health/windows11-release-information";
const WIN10_URL: &str =
    "https://learn.microsoft.com/en-us/windows/release-health/release-information";

/// Phiên bản chỉ giao cho máy mới xuất xưởng, không nâng cấp tại chỗ được và
/// cũng không có ISO công khai — đưa vào danh mục chỉ khiến người dùng đi vào
/// ngõ cụt.
const SKIP_VERSIONS: &[&str] = &["26H1"];

/// Một dòng đọc được từ bảng.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveRow {
    pub version: String,
    pub build: Option<String>,
    pub released: Option<String>,
    pub end_of_support: Option<String>,
}

// ---------------------------------------------------------------------------
// Bóc tách HTML
// ---------------------------------------------------------------------------

/// Lấy phần bên trong của mọi thẻ `<tag>…</tag>` ở mức ngoài cùng.
fn blocks<'a>(hay: &'a str, tag: &str) -> Vec<&'a str> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut i = 0usize;

    while let Some(rel) = hay[i..].find(&open) {
        let start = i + rel;
        // Chặn khớp nhầm: "<td" không được khớp với "<tdx".
        let next = hay[start + open.len()..].chars().next();
        if !matches!(next, Some(c) if c == '>' || c == '/' || c.is_whitespace()) {
            i = start + open.len();
            continue;
        }
        let Some(gt) = hay[start..].find('>') else { break };
        let body = start + gt + 1;
        let Some(end) = hay[body..].find(&close) else { break };
        out.push(&hay[body..body + end]);
        i = body + end + close.len();
    }
    out
}

/// Bỏ hết thẻ, gộp khoảng trắng, giải mã vài thực thể HTML hay gặp.
fn text(html: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for c in html.chars() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    let out = out
        .replace("&nbsp;", " ")
        .replace("&#160;", " ")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#8217;", "'");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Vị trí các cột cần dùng, tra từ hàng tiêu đề.
#[derive(Debug, Default, PartialEq)]
struct Columns {
    version: Option<usize>,
    build: Option<usize>,
    released: Option<usize>,
    end_of_support: Option<usize>,
}

fn map_columns(headers: &[String]) -> Columns {
    let mut c = Columns::default();
    for (i, h) in headers.iter().enumerate() {
        let h = h.to_lowercase();
        if c.version.is_none() && h.contains("version") {
            c.version = Some(i);
        } else if c.build.is_none() && h.contains("build") {
            c.build = Some(i);
        } else if c.released.is_none() && h.contains("availability") {
            c.released = Some(i);
        // Microsoft đã đổi cách gọi cột này ít nhất một lần ("End of servicing"
        // thành "End of updates") mà không đổi ý nghĩa, nên nhận cả ba cách viết.
        } else if h.contains("end of servicing")
            || h.contains("end of support")
            || h.contains("end of updates")
        {
            // Bản Home/Pro hết hỗ trợ sớm hơn bản Enterprise. Người dùng phổ
            // thông dùng Home/Pro nên đó mới là mốc cần lấy; lấy nhầm cột
            // Enterprise sẽ khiến ứng dụng nói máy còn được hỗ trợ thêm một năm.
            if h.contains("home") || h.contains("pro") {
                c.end_of_support = Some(i);
            } else if c.end_of_support.is_none()
                && !h.contains("enterprise")
                && !h.contains("education")
            {
                c.end_of_support = Some(i);
            }
        }
    }
    c
}

fn iso_date(s: &str) -> Option<String> {
    let s = s.trim();
    catalog::parse_date(s).map(|_| s.to_string())
}

/// Đọc mọi bảng trong trang, trả về các dòng hiểu được.
pub fn parse_tables(html: &str) -> Vec<LiveRow> {
    let mut rows = Vec::new();

    for table in blocks(html, "table") {
        let trs = blocks(table, "tr");
        let Some(header_row) = trs.iter().find(|r| !blocks(r, "th").is_empty()) else {
            continue;
        };
        let headers: Vec<String> = blocks(header_row, "th").iter().map(|c| text(c)).collect();
        let cols = map_columns(&headers);

        let (Some(vi), Some(ei)) = (cols.version, cols.end_of_support) else {
            continue; // Bảng không phải bảng vòng đời — bỏ qua.
        };

        for tr in &trs {
            let cells: Vec<String> = blocks(tr, "td").iter().map(|c| text(c)).collect();
            if cells.len() <= vi.max(ei) {
                continue;
            }

            let version = cells[vi].trim().to_string();
            // Mã phiên bản luôn có dạng "24H2", "25H2" — lọc sớm để không nhặt
            // phải dòng chú thích hay dòng tổng hợp.
            if !is_version_code(&version) {
                continue;
            }
            // Bản chỉ dành cho máy mới xuất xưởng.
            if text(tr).to_lowercase().contains("new devices") {
                continue;
            }

            rows.push(LiveRow {
                version,
                build: cols
                    .build
                    .and_then(|i| cells.get(i))
                    .map(|b| b.split('.').next().unwrap_or(b).trim().to_string())
                    .filter(|b| !b.is_empty()),
                released: cols.released.and_then(|i| cells.get(i)).and_then(|d| iso_date(d)),
                end_of_support: iso_date(&cells[ei]),
            });
        }
    }
    rows
}

/// `25H2` đúng dạng; `Version`, `21H2 (xem chú thích)` hay ô trống thì không.
fn is_version_code(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 4
        && b[0].is_ascii_digit()
        && b[1].is_ascii_digit()
        && (b[2] == b'H' || b[2] == b'h')
        && b[3].is_ascii_digit()
}

// ---------------------------------------------------------------------------
// Hợp nhất với bảng nhúng
// ---------------------------------------------------------------------------

/// Gộp dữ liệu vừa đọc vào danh mục nền.
///
/// Trả về danh mục mới cùng số mục đã cập nhật và số phiên bản mới phát hiện.
pub fn merge(
    base: &[WindowsRelease],
    rows: &[LiveRow],
    family: &str,
    today: i64,
) -> (Vec<WindowsRelease>, usize, usize) {
    let mut out = base.to_vec();
    let mut updated = 0usize;
    let mut discovered = 0usize;

    for row in rows {
        if SKIP_VERSIONS.iter().any(|v| v.eq_ignore_ascii_case(&row.version)) {
            continue;
        }
        let Some(eos) = row.end_of_support.clone() else { continue };

        // Khớp theo mã phiên bản trong tên, vd "Windows 11 25H2" ↔ "25H2".
        // Các bản LTSC không mang mã này nên không bao giờ khớp — đúng ý, vì
        // trang release-health không nói về chúng.
        let hit = out
            .iter_mut()
            .find(|r| r.family == family && r.name.to_uppercase().ends_with(&row.version.to_uppercase()));

        if let Some(existing) = hit {
            let changed = existing.end_of_support != eos
                || row.build.as_ref().is_some_and(|b| &existing.build != b);
            existing.end_of_support = eos;
            if let Some(b) = &row.build {
                existing.build = b.clone();
            }
            if let Some(d) = &row.released {
                existing.released = d.clone();
            }
            if changed {
                updated += 1;
            }
            continue;
        }

        // Phiên bản ứng dụng chưa từng biết. Chỉ nhận nếu còn được hỗ trợ —
        // trang của Microsoft liệt kê cả những bản đã chết từ lâu, thêm vào chỉ
        // làm rối danh sách.
        if catalog::parse_date(&eos).is_none_or(|d| d < today) {
            continue;
        }

        // Yêu cầu phần cứng lấy theo bản mới nhất cùng dòng đã biết: Microsoft
        // chưa từng nới lỏng yêu cầu giữa hai bản Windows 11 liên tiếp, nên đây
        // là phỏng đoán an toàn — và mục được đánh dấu `discovered` để giao diện
        // nói rõ là suy ra chứ không phải đã xác nhận.
        let Some(template) = newest_of_family(&out, family, today) else { continue };

        out.push(WindowsRelease {
            id: format!("{}-{}", family_slug(family), row.version.to_lowercase()),
            name: format!("{family} {}", row.version.to_uppercase()),
            family: family.to_string(),
            build: row.build.clone().unwrap_or_else(|| template.build.clone()),
            released: row.released.clone().unwrap_or_else(|| catalog::to_iso(today)),
            end_of_support: eos,
            requires_tpm2: template.requires_tpm2,
            requires_secure_boot: template.requires_secure_boot,
            requires_uefi: template.requires_uefi,
            requires_cpu_list: template.requires_cpu_list,
            min_ram_gb: template.min_ram_gb,
            min_disk_gb: template.min_disk_gb,
            tagline: format!(
                "Bản mới nhất, ứng dụng phát hiện từ trang của Microsoft. Yêu cầu phần cứng suy theo {} — hãy đối chiếu lại nếu máy bạn nằm sát ngưỡng.",
                template.name
            ),
            source: SourceKind::MicrosoftConsumer,
            discovered: true,
        });
        discovered += 1;
    }

    (out, updated, discovered)
}

/// Bản còn hỗ trợ, phát hành gần nhất trong cùng dòng sản phẩm.
fn newest_of_family(list: &[WindowsRelease], family: &str, today: i64) -> Option<WindowsRelease> {
    list.iter()
        .filter(|r| r.family == family && r.source == SourceKind::MicrosoftConsumer && !r.is_expired(today))
        .max_by_key(|r| catalog::parse_date(&r.released).unwrap_or(0))
        .cloned()
}

fn family_slug(family: &str) -> &'static str {
    if family.contains("10") {
        "win10"
    } else {
        "win11"
    }
}

// ---------------------------------------------------------------------------
// Tải về và lưu đệm
// ---------------------------------------------------------------------------

fn cache_path() -> Option<std::path::PathBuf> {
    let base = std::env::var("LOCALAPPDATA")
        .or_else(|_| std::env::var("HOME"))
        .ok()?;
    Some(std::path::Path::new(&base).join("GetWinUSB").join("catalog.json"))
}

/// Đọc bản lưu đệm của lần đồng bộ trước. Dùng ngay lúc khởi động để người dùng
/// không phải chờ mạng mới thấy dữ liệu mới nhất mình từng có.
pub fn load_cache() -> Option<CatalogState> {
    let raw = std::fs::read_to_string(cache_path()?).ok()?;
    let mut state: CatalogState = serde_json::from_str(&raw).ok()?;
    if state.releases.is_empty() {
        return None;
    }
    state.origin = CatalogOrigin::Cache;
    Some(state)
}

fn save_cache(state: &CatalogState) {
    let Some(path) = cache_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = std::fs::write(path, json);
    }
}

async fn fetch(url: &str) -> Result<String> {
    let client = reqwest::Client::builder()
        .user_agent("GetWinUSB/0.1 (+https://github.com/)")
        .timeout(std::time::Duration::from_secs(25))
        .build()?;
    let resp = client.get(url).send().await?;
    if !resp.status().is_success() {
        return Err(AppError::new(
            "http",
            format!("Trang của Microsoft trả về mã {}.", resp.status()),
        ));
    }
    Ok(resp.text().await?)
}

/// Đồng bộ danh mục từ Microsoft và ghi kết quả vào kho đang hoạt động.
pub async fn sync() -> Result<CatalogState> {
    let today = catalog::today();
    let mut releases = catalog::builtin();
    let mut updated = 0usize;
    let mut discovered = 0usize;
    let mut problems: Vec<String> = Vec::new();

    for (url, family) in [(WIN11_URL, "Windows 11"), (WIN10_URL, "Windows 10")] {
        match fetch(url).await {
            Ok(html) => {
                let rows = parse_tables(&html);
                if rows.is_empty() {
                    problems.push(format!(
                        "Không nhận ra bảng phiên bản trên trang {family} — nhiều khả năng Microsoft đã đổi bố cục trang."
                    ));
                    continue;
                }
                let (merged, u, d) = merge(&releases, &rows, family, today);
                releases = merged;
                updated += u;
                discovered += d;
            }
            Err(e) => problems.push(format!("{family}: {}", e.message)),
        }
    }

    // Không đọc được gì cả thì giữ nguyên thứ đang có, đừng ghi đè bằng bảng
    // nhúng — bản lưu đệm cũ vẫn mới hơn.
    if updated == 0 && discovered == 0 && !problems.is_empty() {
        return Err(AppError::new("sync_failed", problems.join(" · ")));
    }

    let state = CatalogState {
        releases,
        origin: CatalogOrigin::Live,
        synced_on: Some(catalog::to_iso(today)),
        note: (!problems.is_empty()).then(|| problems.join(" · ")),
    };

    save_cache(&state);
    catalog::replace(state.clone());
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bảng rút gọn theo đúng kiểu Microsoft đang dùng, kể cả cột "Latest
    /// revision date" nằm chen giữa — cái bẫy khiến việc đọc theo vị trí cột
    /// lấy nhầm ngày.
    const SAMPLE: &str = r##"
    <table>
      <thead><tr>
        <th>Version</th><th>Servicing option</th><th>Availability date</th>
        <th>OS build</th><th>Latest revision date</th>
        <th>End of servicing: Home, Pro, Pro Education, Pro for Workstations</th>
        <th>End of servicing: Enterprise, Education, IoT Enterprise</th>
      </tr></thead>
      <tbody>
        <tr><td><a href="#">26H2</a></td><td>General Availability Channel</td><td>2026-10-13</td>
            <td>28100.1000</td><td>2026-10-13</td><td>2028-10-10</td><td>2029-10-09</td></tr>
        <tr><td>26H1</td><td>General Availability Channel (new devices only)</td><td>2026-02-10</td>
            <td>28000.1000</td><td>2026-02-10</td><td>2028-03-14</td><td>2029-03-13</td></tr>
        <tr><td>25H2</td><td>General Availability Channel</td><td>2025-09-30</td>
            <td>26200.1000</td><td>2026-08-11</td><td>2027-10-12</td><td>2028-10-10</td></tr>
        <tr><td>22H2</td><td>General Availability Channel</td><td>2022-10-18</td>
            <td>22621.1000</td><td>2024-06-11</td><td>End of servicing</td><td>2025-10-14</td></tr>
      </tbody>
    </table>"##;

    #[test]
    fn columns_are_mapped_by_header_not_position() {
        let headers: Vec<String> = vec![
            "Version".into(),
            "Servicing option".into(),
            "Availability date".into(),
            "OS build".into(),
            "Latest revision date".into(),
            "End of servicing: Home, Pro".into(),
            "End of servicing: Enterprise".into(),
        ];
        let c = map_columns(&headers);
        assert_eq!(c.version, Some(0));
        assert_eq!(c.released, Some(2));
        assert_eq!(c.build, Some(3));
        assert_eq!(
            c.end_of_support,
            Some(5),
            "phải lấy cột Home/Pro, không phải cột Enterprise vốn dài hơn một năm"
        );
    }

    /// Tên cột trên trang của Microsoft (tháng 8/2026). Đổi cách gọi cột mà
    /// không đổi ý nghĩa là kiểu thay đổi im lặng nhất: đồng bộ ngừng chạy
    /// nhưng ứng dụng vẫn hiện danh mục nhúng như không có chuyện gì.
    #[test]
    fn the_renamed_end_of_updates_column_is_still_recognised() {
        let headers: Vec<String> = vec![
            "Version".into(),
            "Servicing option".into(),
            "Availability date".into(),
            "End of updates: Home, Pro, Pro Education, and Pro for Workstations".into(),
            "End of updates: Enterprise, Education, IoT Enterprise, and Enterprise multi-session"
                .into(),
            "Latest update".into(),
            "Latest revision date".into(),
            "Latest build".into(),
        ];
        let c = map_columns(&headers);
        assert_eq!(c.version, Some(0));
        assert_eq!(c.released, Some(2));
        assert_eq!(
            c.end_of_support,
            Some(3),
            "phải lấy cột Home/Pro, không phải cột Enterprise vốn dài hơn một năm"
        );
    }

    #[test]
    fn rows_are_read_from_the_table() {
        let rows = parse_tables(SAMPLE);
        let v: Vec<&str> = rows.iter().map(|r| r.version.as_str()).collect();

        assert!(v.contains(&"26H2"));
        assert!(v.contains(&"25H2"));
        assert!(!v.contains(&"26H1"), "bản chỉ dành cho máy mới phải bị loại");

        let r = rows.iter().find(|r| r.version == "26H2").unwrap();
        assert_eq!(r.build.as_deref(), Some("28100"), "phải cắt phần sau dấu chấm");
        assert_eq!(r.released.as_deref(), Some("2026-10-13"));
        assert_eq!(r.end_of_support.as_deref(), Some("2028-10-10"));

        // Ô ghi chữ thay vì ngày thì phải thành None, không được đoán bừa.
        let old = rows.iter().find(|r| r.version == "22H2").unwrap();
        assert_eq!(old.end_of_support, None);
    }

    #[test]
    fn a_brand_new_release_is_added_to_the_catalog() {
        let today = catalog::parse_date("2026-11-01").unwrap();
        let rows = parse_tables(SAMPLE);
        let (merged, _, discovered) = merge(&catalog::builtin(), &rows, "Windows 11", today);

        assert_eq!(discovered, 1, "26H2 phải được thêm mới");
        let new = merged.iter().find(|r| r.id == "win11-26h2").unwrap();
        assert_eq!(new.name, "Windows 11 26H2");
        assert_eq!(new.end_of_support, "2028-10-10");
        assert!(new.discovered, "phải đánh dấu là do phát hiện, không phải dữ liệu đã xác nhận");
        assert!(new.requires_tpm2, "yêu cầu phần cứng kế thừa từ bản Windows 11 trước đó");
    }

    #[test]
    fn known_releases_are_updated_not_duplicated() {
        let today = catalog::parse_date("2026-11-01").unwrap();
        let rows = parse_tables(SAMPLE);
        let (merged, _, _) = merge(&catalog::builtin(), &rows, "Windows 11", today);

        let hits: Vec<_> = merged.iter().filter(|r| r.name.ends_with("25H2")).collect();
        assert_eq!(hits.len(), 1, "không được nhân bản mục đã có");
        assert_eq!(hits[0].build, "26200");
        assert!(!hits[0].discovered);
    }

    #[test]
    fn ltsc_entries_are_left_untouched() {
        // Trang release-health không nói về LTSC. Nếu khớp nhầm thì ngày hết hỗ
        // trợ 2032 của LTSC 2021 sẽ bị ghi đè thành một mốc sớm hơn nhiều.
        let today = catalog::parse_date("2026-11-01").unwrap();
        let rows = parse_tables(SAMPLE);
        let (merged, _, _) = merge(&catalog::builtin(), &rows, "Windows 11", today);

        let ltsc = merged.iter().find(|r| r.id == "win10-ltsc-2021").unwrap();
        assert_eq!(ltsc.end_of_support, "2032-01-13");
    }

    #[test]
    fn expired_versions_we_never_knew_about_are_ignored() {
        let today = catalog::parse_date("2026-11-01").unwrap();
        let rows = vec![LiveRow {
            version: "21H2".into(),
            build: Some("22000".into()),
            released: Some("2021-10-04".into()),
            end_of_support: Some("2023-10-10".into()),
        }];
        let (merged, _, discovered) = merge(&catalog::builtin(), &rows, "Windows 11", today);
        assert_eq!(discovered, 0);
        assert!(!merged.iter().any(|r| r.id == "win11-21h2"));
    }

    #[test]
    fn garbage_html_yields_nothing_rather_than_nonsense() {
        assert!(parse_tables("<html><body>Trang đã đổi hoàn toàn</body></html>").is_empty());
        assert!(parse_tables("").is_empty());
    }
}
