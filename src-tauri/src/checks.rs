//! Đối chiếu từng thành phần phần cứng với yêu cầu chính thức của Windows 11.
//!
//! Đây là nguồn sự thật duy nhất cho cả bước "Phần cứng" (hiện dấu tích từng
//! mục) lẫn bước "Gợi ý" (chấm điểm). Tách riêng ra để hai màn hình không bao
//! giờ nói khác nhau về cùng một chiếc máy.
//!
//! Danh sách bám theo yêu cầu Microsoft công bố: bộ xử lý 64-bit từ 1 GHz và 2
//! nhân trở lên nằm trong danh sách hỗ trợ, 4 GB RAM, 64 GB ổ cứng, firmware
//! UEFI có Secure Boot, TPM 2.0, card đồ hoạ DirectX 12 với driver WDDM 2.0, và
//! màn hình chéo trên 9 inch, độ phân giải từ 720p, 8 bit mỗi kênh màu.

use crate::cpu::{CpuSupport, CpuVerdict};
use crate::hardware::HardwareReport;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    /// Đạt yêu cầu.
    Pass,
    /// Chưa đạt nhưng sửa được bằng cách đổi thiết lập, không phải thay phần cứng.
    Fixable,
    /// Không đạt.
    Fail,
    /// Không đủ dữ liệu để kết luận.
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub id: String,
    /// Nhóm để gom lại khi hiển thị.
    pub group: String,
    pub label: String,
    /// Giá trị đọc được trên máy này.
    pub value: String,
    /// Ngưỡng mà Windows 11 đòi hỏi.
    pub requirement: String,
    pub status: CheckStatus,
    pub hint: Option<String>,
    /// Không đạt thì trình cài đặt Windows 11 chặn lại.
    pub blocking: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckSummary {
    pub passed: usize,
    pub fixable: usize,
    pub failed: usize,
    pub unknown: usize,
    pub total: usize,
    /// Máy qua được toàn bộ các mục bắt buộc hay chưa.
    pub windows11_ready: bool,
}

pub fn summarize(checks: &[Check]) -> CheckSummary {
    let count = |s: CheckStatus| checks.iter().filter(|c| c.status == s).count();
    CheckSummary {
        passed: count(CheckStatus::Pass),
        fixable: count(CheckStatus::Fixable),
        failed: count(CheckStatus::Fail),
        unknown: count(CheckStatus::Unknown),
        total: checks.len(),
        windows11_ready: !checks
            .iter()
            .any(|c| c.blocking && matches!(c.status, CheckStatus::Fail | CheckStatus::Fixable)),
    }
}

/// Dựng một mục kiểm tra. Gom vào một hàm để 13 mục dưới đây đọc như một bảng
/// dữ liệu chứ không phải 13 khối khởi tạo struct giống hệt nhau.
#[allow(clippy::too_many_arguments)]
fn check(
    id: &str,
    group: &str,
    label: &str,
    value: String,
    requirement: &str,
    status: CheckStatus,
    blocking: bool,
    hint: Option<String>,
) -> Check {
    Check {
        id: id.into(),
        group: group.into(),
        label: label.into(),
        value,
        requirement: requirement.into(),
        status,
        hint,
        blocking,
    }
}

const G_CPU: &str = "Bộ xử lý";
const G_MEM: &str = "Bộ nhớ và lưu trữ";
const G_SEC: &str = "Bảo mật nền tảng";
const G_GFX: &str = "Đồ hoạ và màn hình";

pub fn evaluate(hw: &HardwareReport, cpu_v: &CpuVerdict) -> Vec<Check> {
    let mut out = Vec::with_capacity(13);

    // ---------------------------------------------------------------- CPU
    out.push(check(
        "cpu-model", G_CPU, "Kiểu bộ xử lý",
        hw.cpu.name.trim().to_string(),
        "Nằm trong danh sách CPU Microsoft hỗ trợ",
        match cpu_v.support {
            CpuSupport::Supported => CheckStatus::Pass,
            CpuSupport::Unsupported => CheckStatus::Fail,
            CpuSupport::Unknown => CheckStatus::Unknown,
        },
        true,
        match cpu_v.support {
            CpuSupport::Unsupported => Some(format!(
                "{} Vẫn cài Windows 11 được nếu bỏ qua kiểm tra, nhưng Microsoft không đảm bảo cập nhật.",
                cpu_v.reason
            )),
            CpuSupport::Unknown => Some(format!(
                "{} Hãy đối chiếu tên CPU với danh sách hỗ trợ chính thức của Microsoft.",
                cpu_v.reason
            )),
            CpuSupport::Supported => None,
        },
    ));

    out.push(check(
        "cpu-cores", G_CPU, "Số nhân",
        format!("{} nhân · {} luồng", hw.cpu.cores, hw.cpu.threads),
        "Từ 2 nhân trở lên",
        if hw.cpu.cores == 0 { CheckStatus::Unknown }
        else if hw.cpu.cores >= 2 { CheckStatus::Pass }
        else { CheckStatus::Fail },
        true,
        (hw.cpu.cores == 1).then(|| "Windows 11 yêu cầu tối thiểu 2 nhân.".to_string()),
    ));

    out.push(check(
        "cpu-clock", G_CPU, "Xung nhịp",
        if hw.cpu.max_clock_mhz > 0 {
            format!("{:.2} GHz", hw.cpu.max_clock_mhz as f64 / 1000.0).replace('.', ",")
        } else { "Không đọc được".into() },
        "Từ 1 GHz trở lên",
        if hw.cpu.max_clock_mhz == 0 { CheckStatus::Unknown }
        else if hw.cpu.max_clock_mhz >= 1000 { CheckStatus::Pass }
        else { CheckStatus::Fail },
        true,
        None,
    ));

    out.push(check(
        "cpu-arch", G_CPU, "Kiến trúc",
        format!(
            "{} · hệ điều hành {}",
            if hw.is_arm() { "ARM64" } else if hw.is_64bit() { "x64" } else { "x86 (32-bit)" },
            hw.os.architecture
        ),
        "Bộ xử lý 64-bit",
        if hw.cpu.address_width == 0 { CheckStatus::Unknown }
        else if hw.is_64bit() { CheckStatus::Pass }
        else { CheckStatus::Fail },
        true,
        (!hw.is_64bit() && hw.cpu.address_width > 0)
            .then(|| "Windows 11 chỉ có bản 64-bit. CPU 32-bit bắt buộc dùng Windows 10.".to_string()),
    ));

    // ------------------------------------------------- Bộ nhớ và lưu trữ
    let ram = hw.ram_gb();
    out.push(check(
        "ram", G_MEM, "Bộ nhớ RAM",
        format!("{ram:.1} GB").replace('.', ","),
        "Từ 4 GB trở lên",
        if ram >= 8.0 { CheckStatus::Pass }
        else if ram >= 4.0 { CheckStatus::Fixable }
        else { CheckStatus::Fail },
        true,
        if ram < 4.0 {
            Some("Dưới mức tối thiểu 4 GB của Windows 11.".into())
        } else if ram < 8.0 {
            Some("Đủ mức tối thiểu nhưng máy sẽ chậm khi mở nhiều ứng dụng. Nâng lên 8 GB nếu có thể.".into())
        } else { None },
    ));

    let free = hw.free_disk_gb();
    out.push(check(
        "disk", G_MEM, "Dung lượng trống ổ hệ thống",
        format!(
            "{free:.0} GB trống trên {:.0} GB{}",
            hw.system_disk.size as f64 / 1024.0_f64.powi(3),
            hw.system_disk.media_type.as_deref().map(|m| format!(" · {m}")).unwrap_or_default()
        ),
        "Từ 64 GB trở lên",
        if free >= 64.0 { CheckStatus::Pass }
        else if free >= 32.0 { CheckStatus::Fixable }
        else { CheckStatus::Fail },
        true,
        (free < 64.0).then(|| "Windows 11 cần tối thiểu 64 GB. Hãy dọn bớt dung lượng trước khi cài.".to_string()),
    ));

    // -------------------------------------------------- Bảo mật nền tảng
    out.push(check(
        "firmware", G_SEC, "Chế độ khởi động",
        hw.firmware.clone(),
        "UEFI",
        if hw.is_uefi() { CheckStatus::Pass }
        else if hw.firmware.eq_ignore_ascii_case("Legacy") { CheckStatus::Fixable }
        else { CheckStatus::Unknown },
        true,
        (!hw.is_uefi()).then(|| {
            "Máy đang chạy chế độ BIOS cũ. Cần chuyển ổ sang GPT (lệnh mbr2gpt) rồi bật UEFI trong BIOS.".to_string()
        }),
    ));

    out.push(check(
        "secure-boot", G_SEC, "Secure Boot",
        match hw.secure_boot {
            Some(true) => "Đang bật".to_string(),
            Some(false) => "Đang tắt".to_string(),
            None => "Không đọc được (máy chạy BIOS cũ)".to_string(),
        },
        "Máy hỗ trợ và đang bật",
        match hw.secure_boot {
            Some(true) => CheckStatus::Pass,
            Some(false) => CheckStatus::Fixable,
            None => CheckStatus::Unknown,
        },
        true,
        match hw.secure_boot {
            Some(false) => Some("Vào BIOS bật Secure Boot. Máy đã hỗ trợ sẵn, chỉ đang tắt.".into()),
            None => Some("Chuyển sang chế độ UEFI thì mới bật được Secure Boot.".into()),
            Some(true) => None,
        },
    ));

    let tpm_major = hw.tpm_major();
    out.push(check(
        "tpm", G_SEC, "TPM",
        if !hw.tpm.present { "Không tìm thấy".to_string() } else {
            format!(
                "Phiên bản {} · {}{}",
                hw.tpm.version.clone().unwrap_or_else(|| "?".into()),
                if hw.tpm.enabled { "đang bật" } else { "đang tắt" },
                if hw.tpm.source == "device" { " · đọc qua Device Manager" } else { "" }
            )
        },
        "TPM 2.0 đang bật",
        if hw.tpm_ready() { CheckStatus::Pass }
        else if hw.tpm.present && tpm_major >= 2 { CheckStatus::Fixable }
        else { CheckStatus::Fail },
        true,
        if !hw.tpm.present {
            Some("Nhiều máy có TPM dạng firmware nhưng mặc định tắt: tìm mục fTPM (AMD) hoặc PTT / Intel Platform Trust Technology trong BIOS.".into())
        } else if tpm_major < 2 {
            Some(format!("Máy chỉ có TPM {tpm_major}.x, Windows 11 yêu cầu 2.0."))
        } else if !hw.tpm.enabled {
            Some("Đã có chip TPM 2.0, chỉ cần bật lên trong BIOS.".into())
        } else if hw.tpm.source == "device" && !hw.elevated {
            Some("Đọc gián tiếp từ Device Manager vì Windows chỉ cho truy vấn chi tiết TPM khi chạy quyền quản trị. Đủ để kết luận.".into())
        } else { None },
    ));

    // -------------------------------------------------- Đồ hoạ và màn hình
    let gpu = hw.gpus.first();
    let wddm = gpu.and_then(|g| g.likely_wddm2());
    out.push(check(
        "gpu", G_GFX, "Card đồ hoạ",
        gpu.map(|g| {
            let d = g.driver_date.as_deref().map(|d| format!(" · driver {}", crate::catalog::format_date(d))).unwrap_or_default();
            format!("{}{d}", g.name)
        }).unwrap_or_else(|| "Không đọc được".into()),
        "DirectX 12 với driver WDDM 2.0",
        match wddm {
            Some(true) => CheckStatus::Pass,
            Some(false) => CheckStatus::Unknown,
            None => CheckStatus::Unknown,
        },
        false,
        match wddm {
            Some(true) => None,
            _ => Some(
                "Không xác định chắc chắn được từ thông tin driver. Gõ dxdiag vào ô tìm kiếm của Windows để xem mục \"Driver Model\" — cần WDDM 2.0 trở lên.".into(),
            ),
        },
    ));

    let d = &hw.display;
    let px_ok = d.height >= 720 && d.width > 0;
    out.push(check(
        "display-res", G_GFX, "Độ phân giải màn hình",
        if d.width > 0 { format!("{} × {}", d.width, d.height) } else { "Không đọc được".into() },
        "Từ 720p trở lên",
        if d.width == 0 { CheckStatus::Unknown } else if px_ok { CheckStatus::Pass } else { CheckStatus::Fail },
        true,
        (d.width > 0 && !px_ok).then(|| "Windows 11 yêu cầu chiều cao tối thiểu 720 điểm ảnh.".to_string()),
    ));

    out.push(check(
        "display-size", G_GFX, "Kích thước màn hình",
        d.diagonal_inches
            .map(|v| format!("{v:.1} inch").replace('.', ","))
            .unwrap_or_else(|| "Không đọc được".into()),
        "Đường chéo trên 9 inch",
        match d.diagonal_inches {
            Some(v) if v > 9.0 => CheckStatus::Pass,
            Some(_) => CheckStatus::Fail,
            None => CheckStatus::Unknown,
        },
        true,
        d.diagonal_inches.is_none().then(|| {
            "Màn hình không báo kích thước qua EDID — thường gặp ở màn nối bằng cáp chuyển đổi hoặc máy ảo.".to_string()
        }),
    ));

    out.push(check(
        "display-color", G_GFX, "Độ sâu màu",
        d.bits_per_channel()
            .map(|b| format!("{b} bit mỗi kênh ({} bit mỗi điểm ảnh)", d.bits_per_pixel))
            .unwrap_or_else(|| "Không đọc được".into()),
        "8 bit mỗi kênh màu",
        match d.bits_per_channel() {
            Some(b) if b >= 8 => CheckStatus::Pass,
            Some(_) => CheckStatus::Fail,
            None => CheckStatus::Unknown,
        },
        true,
        None,
    ));

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu;
    use crate::hardware::*;

    fn good() -> HardwareReport {
        HardwareReport {
            manufacturer: "Dell".into(),
            model: "Latitude 7490".into(),
            bios_version: "1.20.0".into(),
            cpu: CpuInfo {
                name: "Intel(R) Core(TM) i5-8350U CPU @ 1.70GHz".into(),
                manufacturer: "GenuineIntel".into(),
                cores: 4, threads: 8, max_clock_mhz: 1700,
                address_width: 64, architecture: 9,
            },
            total_ram: 16 * 1024 * 1024 * 1024,
            memory_modules: vec![],
            memory_slots: 2,
            gpus: vec![GpuInfo {
                name: "Intel(R) UHD Graphics 620".into(),
                vram: 0,
                driver: "31.0.101.2111".into(),
                driver_date: Some("2023-05-10".into()),
            }],
            display: DisplayInfo { width: 1920, height: 1080, diagonal_inches: Some(14.0), bits_per_pixel: 32 },
            tpm: TpmInfo { present: true, version: Some("2.0".into()), enabled: true, activated: true, source: "wmi".into() },
            secure_boot: Some(true),
            secure_boot_source: "uefi-api".into(),
            firmware: "UEFI".into(),
            elevated: true,
            system_disk: SystemDiskInfo {
                size: 256 * 1024 * 1024 * 1024,
                free: 120 * 1024 * 1024 * 1024,
                media_type: Some("SSD".into()),
                partition_style: Some("GPT".into()),
            },
            os: OsInfo {
                caption: "Microsoft Windows 10 Pro".into(),
                version: "10.0.19045".into(), build: "19045".into(),
                architecture: "64-bit".into(),
            },
        }
    }

    fn run(hw: &HardwareReport) -> Vec<Check> {
        evaluate(hw, &cpu::analyze(&hw.cpu.name))
    }

    fn status_of<'a>(checks: &'a [Check], id: &str) -> &'a Check {
        checks.iter().find(|c| c.id == id).unwrap_or_else(|| panic!("thiếu mục {id}"))
    }

    #[test]
    fn a_fully_compliant_machine_passes_everything() {
        let checks = run(&good());
        assert_eq!(checks.len(), 13, "phải quét đủ 13 mục");
        let s = summarize(&checks);
        assert_eq!(s.failed, 0);
        assert_eq!(s.fixable, 0);
        assert!(s.windows11_ready);
    }

    #[test]
    fn every_check_has_a_unique_id_and_a_stated_requirement() {
        let checks = run(&good());
        let mut ids: Vec<&str> = checks.iter().map(|c| c.id.as_str()).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before, "id bị trùng thì giao diện sẽ render sai");
        assert!(checks.iter().all(|c| !c.requirement.is_empty()),
                "mỗi mục phải nói rõ ngưỡng yêu cầu, không chỉ đạt hay không");
    }

    #[test]
    fn disabled_tpm_is_amber_not_red() {
        // Phân biệt "sửa được trong BIOS" với "hỏng hẳn" là điểm cốt lõi: gắn
        // dấu đỏ cho một chiếc máy chỉ cần bật TPM sẽ đẩy người dùng đi cài
        // nhầm phiên bản Windows.
        let mut hw = good();
        hw.tpm.enabled = false;
        let c = run(&hw);
        assert_eq!(status_of(&c, "tpm").status, CheckStatus::Fixable);
        assert!(!summarize(&c).windows11_ready);
    }

    #[test]
    fn missing_tpm_is_red() {
        let mut hw = good();
        hw.tpm = TpmInfo { present: false, version: None, enabled: false, activated: false, source: "none".into() };
        assert_eq!(status_of(&run(&hw), "tpm").status, CheckStatus::Fail);
    }

    #[test]
    fn unreadable_values_are_grey_never_red() {
        // Không đọc được không đồng nghĩa với không đạt. Báo đỏ cho thứ mình
        // chưa biết là nói dối người dùng.
        let mut hw = good();
        hw.display = DisplayInfo { width: 0, height: 0, diagonal_inches: None, bits_per_pixel: 0 };
        hw.cpu.max_clock_mhz = 0;
        hw.gpus.clear();

        let c = run(&hw);
        for id in ["display-res", "display-size", "display-color", "cpu-clock", "gpu"] {
            assert_eq!(status_of(&c, id).status, CheckStatus::Unknown, "mục {id}");
        }
        assert_eq!(summarize(&c).failed, 0);
    }

    #[test]
    fn a_small_low_res_screen_fails() {
        let mut hw = good();
        hw.display = DisplayInfo { width: 1024, height: 600, diagonal_inches: Some(7.0), bits_per_pixel: 16 };
        let c = run(&hw);
        assert_eq!(status_of(&c, "display-res").status, CheckStatus::Fail);
        assert_eq!(status_of(&c, "display-size").status, CheckStatus::Fail);
        assert_eq!(status_of(&c, "display-color").status, CheckStatus::Fail);
    }

    #[test]
    fn old_graphics_driver_is_unknown_not_failed() {
        // Driver cũ chỉ nghĩa là không suy ra được WDDM, chứ không chứng minh
        // được card không hỗ trợ — và mục này không phải rào chặn.
        let mut hw = good();
        hw.gpus[0].driver_date = Some("2012-08-01".into());
        let c = run(&hw);
        assert_eq!(status_of(&c, "gpu").status, CheckStatus::Unknown);
        assert!(!status_of(&c, "gpu").blocking);
        assert!(summarize(&c).windows11_ready, "GPU không xác định thì không được chặn cả máy");
    }

    #[test]
    fn old_cpu_fails_the_model_check() {
        let mut hw = good();
        hw.cpu.name = "Intel(R) Core(TM) i5-4200U CPU @ 1.60GHz".into();
        assert_eq!(status_of(&run(&hw), "cpu-model").status, CheckStatus::Fail);
    }
}
