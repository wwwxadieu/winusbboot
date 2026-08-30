//! Danh mục các phiên bản Windows mà ứng dụng có thể gợi ý.
//!
//! Danh mục có hai tầng. Tầng nền là bảng nhúng sẵn trong `builtin()`, luôn có
//! mặt kể cả khi máy không nối mạng. Tầng trên là dữ liệu đồng bộ từ trang
//! release-health của Microsoft (xem `catalog_sync.rs`) — nó cập nhật số build,
//! ngày hết hỗ trợ, và bổ sung những phiên bản ra đời sau khi ứng dụng được
//! biên dịch.
//!
//! Vì vậy mọi nơi cần danh mục phải gọi `snapshot()` chứ không đọc thẳng bảng
//! nhúng, nếu không sẽ bỏ qua dữ liệu vừa cập nhật.

use serde::{Deserialize, Serialize};
use std::sync::{OnceLock, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowsRelease {
    pub id: String,
    pub name: String,
    pub family: String,
    pub build: String,
    /// Ngày phát hành, dạng ISO `YYYY-MM-DD`.
    pub released: String,
    /// Ngày hết hỗ trợ bản Home/Pro (hoặc bản tương ứng của dòng LTSC),
    /// dạng ISO `YYYY-MM-DD` để còn so sánh được với ngày hiện tại.
    pub end_of_support: String,
    pub requires_tpm2: bool,
    pub requires_secure_boot: bool,
    pub requires_uefi: bool,
    pub requires_cpu_list: bool,
    pub min_ram_gb: f64,
    pub min_disk_gb: f64,
    pub tagline: String,
    /// Cách lấy bộ cài — quyết định giao diện hiện nút nào ở bước chọn nguồn.
    pub source: SourceKind,
    /// `true` nếu mục này do đồng bộ từ Microsoft phát hiện ra chứ không có sẵn
    /// trong bản nhúng. Giao diện đánh dấu để người dùng biết các thông số yêu
    /// cầu phần cứng là suy ra từ phiên bản liền trước, chưa được xác nhận.
    #[serde(default)]
    pub discovered: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// Tải trực tiếp từ trang tải chính thức của Microsoft.
    MicrosoftConsumer,
    /// Chỉ phát hành qua kênh doanh nghiệp — người dùng tự cung cấp ISO.
    VolumeLicense,
}

/// Nguồn của danh mục đang dùng.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogOrigin {
    /// Bảng nhúng trong ứng dụng.
    Builtin,
    /// Bản lưu đệm của lần đồng bộ trước.
    Cache,
    /// Vừa đọc được từ trang của Microsoft.
    Live,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogState {
    pub releases: Vec<WindowsRelease>,
    pub origin: CatalogOrigin,
    /// Ngày đồng bộ gần nhất, dạng ISO. `None` nếu chưa từng đồng bộ được.
    pub synced_on: Option<String>,
    /// Vì sao lần đồng bộ gần nhất không thành công, để giao diện nói thật thay
    /// vì im lặng hiện dữ liệu cũ.
    pub note: Option<String>,
}

// ---------------------------------------------------------------------------
// Bảng nhúng
// ---------------------------------------------------------------------------

fn r(
    id: &str,
    name: &str,
    family: &str,
    build: &str,
    released: &str,
    eos: &str,
    win11_rules: bool,
    min_ram: f64,
    min_disk: f64,
    tagline: &str,
    source: SourceKind,
) -> WindowsRelease {
    WindowsRelease {
        id: id.into(),
        name: name.into(),
        family: family.into(),
        build: build.into(),
        released: released.into(),
        end_of_support: eos.into(),
        requires_tpm2: win11_rules,
        requires_secure_boot: win11_rules,
        requires_uefi: win11_rules,
        requires_cpu_list: win11_rules,
        min_ram_gb: min_ram,
        min_disk_gb: min_disk,
        tagline: tagline.into(),
        source,
        discovered: false,
    }
}

/// Bảng nhúng, cập nhật tới tháng 8/2026.
pub fn builtin() -> Vec<WindowsRelease> {
    use SourceKind::*;
    vec![
        r("win11-25h2", "Windows 11 25H2", "Windows 11", "26200",
          "2025-09-30", "2027-10-12", true, 4.0, 64.0,
          "Bản phát hành chính thức hiện hành — lựa chọn mặc định cho máy đời mới.",
          MicrosoftConsumer),
        r("win11-24h2", "Windows 11 24H2", "Windows 11", "26100",
          "2024-10-01", "2026-10-13", true, 4.0, 64.0,
          "Bản cũ hơn một nhịp — chỉ nên chọn nếu bạn cần đúng build này.",
          MicrosoftConsumer),
        r("win11-ltsc-2024", "Windows 11 IoT Enterprise LTSC 2024", "Windows 11", "26100",
          "2024-10-01", "2034-10-10", true, 4.0, 64.0,
          "Không có Store và ứng dụng kèm theo, chỉ nhận bản vá bảo mật — hợp với máy dùng cố định một mục đích.",
          VolumeLicense),
        r("win10-ltsc-2021", "Windows 10 IoT Enterprise LTSC 2021", "Windows 10", "19044",
          "2021-11-16", "2032-01-13", false, 2.0, 32.0,
          "Lối thoát tốt nhất cho máy không có TPM 2.0: vẫn còn bản vá bảo mật tới năm 2032.",
          VolumeLicense),
        r("win10-22h2", "Windows 10 22H2", "Windows 10", "19045",
          "2022-10-18", "2025-10-14", false, 2.0, 32.0,
          "Đã ngừng nhận bản vá bảo mật từ tháng 10/2025 — chỉ dùng khi thật sự không còn cách khác.",
          MicrosoftConsumer),
    ]
}

// ---------------------------------------------------------------------------
// Danh mục đang hoạt động
// ---------------------------------------------------------------------------

fn store() -> &'static RwLock<CatalogState> {
    static ACTIVE: OnceLock<RwLock<CatalogState>> = OnceLock::new();
    ACTIVE.get_or_init(|| {
        RwLock::new(CatalogState {
            releases: builtin(),
            origin: CatalogOrigin::Builtin,
            synced_on: None,
            note: None,
        })
    })
}

/// Bản sao của danh mục đang dùng.
///
/// Trả về bản sao thay vì khoá đọc: danh mục chỉ có dăm mục nên sao chép gần
/// như miễn phí, đổi lại người gọi không phải giữ khoá qua các điểm `await` —
/// vốn là cách dễ nhất để tự khoá chết chính mình.
pub fn snapshot() -> CatalogState {
    match store().read() {
        Ok(g) => g.clone(),
        // Khoá hỏng chỉ xảy ra khi một luồng khác panic khi đang giữ nó. Lúc đó
        // quay về bảng nhúng vẫn tốt hơn là kéo cả ứng dụng sập theo.
        Err(_) => CatalogState {
            releases: builtin(),
            origin: CatalogOrigin::Builtin,
            synced_on: None,
            note: Some("Không đọc được danh mục đang dùng, tạm quay về bảng nhúng.".into()),
        },
    }
}

pub fn replace(state: CatalogState) {
    if let Ok(mut g) = store().write() {
        *g = state;
    }
}

// ---------------------------------------------------------------------------
// Ngày tháng
//
// Vòng đời của một bản Windows là thứ trôi theo thời gian: hôm nay còn được hỗ
// trợ, sáu tuần nữa thì không. Ghi cứng cờ "đã hết hạn" vào mã nguồn thì ứng
// dụng sẽ lặng lẽ nói sai ngay khi qua mốc — nên mọi kết luận về vòng đời đều
// tính từ đồng hồ hệ thống. Chỉ cần ngày, không cần giờ, nên tự tính thay vì
// kéo thêm một thư viện lịch.
// ---------------------------------------------------------------------------

/// Số ngày từ 1970-01-01 tới hôm nay theo đồng hồ máy.
pub fn today() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| (d.as_secs() / 86_400) as i64)
        .unwrap_or(0)
}

/// Đổi chuỗi `YYYY-MM-DD` thành số ngày từ 1970-01-01.
///
/// Dùng thuật toán `days_from_civil` của Howard Hinnant — xử lý đúng năm nhuận
/// và quy tắc thế kỷ mà không cần bảng tra.
pub fn parse_date(iso: &str) -> Option<i64> {
    let mut parts = iso.trim().split('-');
    let y: i64 = parts.next()?.trim().parse().ok()?;
    let m: i64 = parts.next()?.trim().parse().ok()?;
    let d: i64 = parts.next()?.trim().parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

/// Đổi số ngày từ 1970-01-01 về chuỗi ISO.
pub fn to_iso(days: i64) -> String {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// `2027-10-12` → `12/10/2027`, dạng người Việt quen đọc.
pub fn format_date(iso: &str) -> String {
    let parts: Vec<&str> = iso.split('-').collect();
    match parts.as_slice() {
        [y, m, d] => format!("{d}/{m}/{y}"),
        _ => iso.to_string(),
    }
}

impl WindowsRelease {
    /// Số ngày còn được hỗ trợ. Số âm nghĩa là đã quá hạn bấy nhiêu ngày.
    pub fn days_remaining(&self, today: i64) -> i64 {
        parse_date(&self.end_of_support).map_or(i64::MAX, |eos| eos - today)
    }

    pub fn is_expired(&self, today: i64) -> bool {
        self.days_remaining(today) < 0
    }

    pub fn end_of_support_label(&self) -> String {
        format_date(&self.end_of_support)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_convert_correctly() {
        assert_eq!(parse_date("1970-01-01"), Some(0));
        assert_eq!(parse_date("2026-08-28"), Some(20693));
        assert_eq!(parse_date("2027-10-12"), Some(21103));
        assert_eq!(parse_date("khong-phai-ngay"), None);
        assert_eq!(parse_date("2026-13-01"), None, "tháng 13 phải bị từ chối");
        assert_eq!(format_date("2027-10-12"), "12/10/2027");
    }

    #[test]
    fn iso_round_trips() {
        for iso in ["1970-01-01", "2026-08-28", "2032-01-13", "2034-10-10"] {
            assert_eq!(to_iso(parse_date(iso).unwrap()), iso);
        }
    }

    #[test]
    fn lifecycle_is_read_from_the_clock_not_hardcoded() {
        let all = builtin();
        let w10 = all.iter().find(|r| r.id == "win10-22h2").unwrap();
        let w11 = all.iter().find(|r| r.id == "win11-25h2").unwrap();
        let today = parse_date("2026-08-28").unwrap();

        assert!(w10.is_expired(today), "Windows 10 22H2 hết hỗ trợ từ 14/10/2025");
        assert!(!w11.is_expired(today));
        assert_eq!(w11.days_remaining(today), 410);
    }

    #[test]
    fn a_release_expires_on_its_own_once_the_date_passes() {
        // 24H2 hết hỗ trợ 13/10/2026. Trước mốc thì còn hạn, sau mốc thì hết —
        // không cần ai sửa mã nguồn.
        let all = builtin();
        let r = all.iter().find(|r| r.id == "win11-24h2").unwrap();
        assert!(!r.is_expired(parse_date("2026-10-12").unwrap()));
        assert!(r.is_expired(parse_date("2026-10-14").unwrap()));
    }

    #[test]
    fn every_builtin_date_is_parseable() {
        for r in builtin() {
            assert!(parse_date(&r.end_of_support).is_some(), "ngày hết hỗ trợ hỏng ở {}", r.id);
            assert!(parse_date(&r.released).is_some(), "ngày phát hành hỏng ở {}", r.id);
        }
    }
}
