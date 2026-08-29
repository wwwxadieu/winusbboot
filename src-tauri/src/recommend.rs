//! Engine gợi ý: từ báo cáo phần cứng, chấm điểm từng phiên bản Windows.

use crate::catalog::{self, CatalogOrigin, WindowsRelease};
use crate::checks::{self, Check, CheckSummary};
use crate::cpu::{self, CpuSupport, CpuVerdict};
use crate::hardware::HardwareReport;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Cài được ngay, đúng chuẩn.
    Recommended,
    /// Cài được sau khi chỉnh vài thiết lập trong BIOS.
    NeedsSetup,
    /// Chỉ cài được khi bỏ qua kiểm tra của Microsoft.
    NeedsBypass,
    /// Không nên hoặc không thể cài.
    Blocked,
}

#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub release: WindowsRelease,
    /// Ngày hết hỗ trợ đã định dạng sẵn cho người đọc.
    pub end_of_support_label: String,
    /// Số ngày còn được hỗ trợ, tính từ hôm nay. Âm nghĩa là đã quá hạn.
    pub days_remaining: i64,
    pub expired: bool,
    pub score: i32,
    pub verdict: Verdict,
    pub pros: Vec<String>,
    pub cons: Vec<String>,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Recommendation {
    pub cpu: CpuVerdict,
    pub checks: Vec<Check>,
    pub check_summary: CheckSummary,
    pub candidates: Vec<Candidate>,
    /// `id` của phiên bản đứng đầu bảng.
    pub best: String,
    pub summary: String,
    pub architecture: String,
    pub edition_hint: String,
    pub language_hint: String,
    /// Danh mục dùng để chấm điểm đến từ đâu, và cập nhật lần cuối lúc nào.
    /// Người dùng cần biết mình đang xem dữ liệu mới hay dữ liệu đóng băng từ
    /// lúc ứng dụng được biên dịch.
    pub catalog_origin: CatalogOrigin,
    pub catalog_synced_on: Option<String>,
    pub catalog_note: Option<String>,
}

pub fn analyze(hw: &HardwareReport) -> Recommendation {
    // Chốt "hôm nay" một lần rồi dùng chung, để mọi phiên bản được chấm trên
    // cùng một mốc thời gian.
    analyze_at(hw, catalog::today())
}

/// Bản có truyền mốc thời gian, dùng cho kiểm thử: kết quả phải phụ thuộc vào
/// ngày, và cách duy nhất để chứng minh điều đó là thử ở nhiều ngày khác nhau.
pub fn analyze_at(hw: &HardwareReport, today: i64) -> Recommendation {
    let cpu_verdict = cpu::analyze(&hw.cpu.name);
    let checks = checks::evaluate(hw, &cpu_verdict);
    let check_summary = checks::summarize(&checks);

    // Danh mục có thể vừa được đồng bộ từ Microsoft, nên phải đọc kho đang hoạt
    // động chứ không phải bảng nhúng.
    let catalog = catalog::snapshot();

    let mut candidates: Vec<Candidate> = catalog
        .releases
        .iter()
        .map(|r| score(r, hw, &cpu_verdict, today))
        .collect();

    // Điểm cao lên đầu; điểm bằng nhau thì bản còn hỗ trợ lâu hơn được ưu tiên.
    candidates.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.expired.cmp(&b.expired))
            .then_with(|| b.days_remaining.cmp(&a.days_remaining))
    });

    let best = candidates.first().map(|c| c.release.id.to_string()).unwrap_or_default();
    let summary = summarize(hw, &cpu_verdict, &candidates);

    Recommendation {
        architecture: architecture(hw),
        edition_hint: edition_hint(hw),
        language_hint: "Tiếng Việt (vi-vn)".into(),
        cpu: cpu_verdict,
        checks,
        check_summary,
        candidates,
        best,
        summary,
        catalog_origin: catalog.origin,
        catalog_synced_on: catalog.synced_on,
        catalog_note: catalog.note,
    }
}

fn architecture(hw: &HardwareReport) -> String {
    if hw.is_arm() {
        "arm64".into()
    } else if hw.is_64bit() {
        "x64".into()
    } else {
        "x86 (32-bit)".into()
    }
}

/// Đoán phiên bản (Home/Pro) từ hệ điều hành đang chạy để giữ nguyên loại bản quyền.
fn edition_hint(hw: &HardwareReport) -> String {
    let c = hw.os.caption.to_lowercase();
    if c.contains("enterprise") {
        "Enterprise".into()
    } else if c.contains("education") {
        "Education".into()
    } else if c.contains("pro") {
        "Pro".into()
    } else {
        "Home".into()
    }
}

fn score(r: &WindowsRelease, hw: &HardwareReport, cpu_v: &CpuVerdict, today: i64) -> Candidate {
    let mut score: i32 = 100;
    let mut pros: Vec<String> = Vec::new();
    let mut cons: Vec<String> = Vec::new();
    let mut blockers: Vec<String> = Vec::new();
    let mut needs_bypass = false;
    let mut needs_setup = false;

    let ram = hw.ram_gb();
    let free = hw.free_disk_gb();

    // ---- Điều kiện cứng, không cách nào lách ----
    if !hw.is_64bit() && r.family == "Windows 11" {
        blockers.push("Windows 11 không có bản 32-bit.".into());
        score -= 100;
    }
    if ram < r.min_ram_gb {
        blockers.push(format!(
            "RAM {ram:.1} GB thấp hơn mức tối thiểu {:.0} GB.",
            r.min_ram_gb
        ));
        score -= 60;
    }
    if free < r.min_disk_gb {
        blockers.push(format!(
            "Chỉ còn {free:.0} GB trống, cần tối thiểu {:.0} GB.",
            r.min_disk_gb
        ));
        score -= 40;
    }

    // ---- Điều kiện Windows 11 kiểm tra khi cài ----
    if r.requires_tpm2 && !hw.tpm_ready() {
        if hw.tpm.present && hw.tpm_major() >= 2 {
            needs_setup = true;
            score -= 8;
            cons.push("TPM 2.0 đang tắt — cần bật trong BIOS.".into());
        } else {
            needs_bypass = true;
            score -= 35;
            cons.push("Máy không có TPM 2.0 — phải bỏ qua kiểm tra khi cài.".into());
        }
    } else if r.requires_tpm2 {
        pros.push("TPM 2.0 sẵn sàng.".into());
    }

    if r.requires_uefi && !hw.is_uefi() {
        needs_bypass = true;
        score -= 25;
        cons.push("Máy đang chạy BIOS cũ, cần chuyển ổ sang GPT trước.".into());
    }

    if r.requires_secure_boot {
        match hw.secure_boot {
            Some(true) => pros.push("Secure Boot đang bật.".into()),
            Some(false) => {
                needs_setup = true;
                score -= 8;
                cons.push("Secure Boot đang tắt — chỉ cần bật lại trong BIOS.".into());
            }
            None => {
                score -= 15;
                cons.push("Không xác định được Secure Boot.".into());
            }
        }
    }

    if r.requires_cpu_list {
        match cpu_v.support {
            CpuSupport::Supported => pros.push(format!("{} nằm trong danh sách CPU hỗ trợ.", cpu_v.family)),
            CpuSupport::Unsupported => {
                needs_bypass = true;
                score -= 30;
                cons.push(format!("{} không nằm trong danh sách CPU hỗ trợ.", cpu_v.family));
            }
            CpuSupport::Unknown => {
                score -= 10;
                cons.push("Chưa xác định chắc chắn CPU có được hỗ trợ hay không.".into());
            }
        }
    } else if cpu_v.support == CpuSupport::Unsupported {
        pros.push("Không đòi hỏi CPU nằm trong danh sách hỗ trợ.".into());
    }

    // ---- Vòng đời sản phẩm ----
    let days = r.days_remaining(today);
    let expired = days < 0;
    let label = r.end_of_support_label();

    if expired {
        score -= 45;
        cons.push(format!("Đã hết hỗ trợ từ {label} — không còn bản vá bảo mật."));
    } else if days <= 120 {
        // Cài một bản sắp hết vòng đời nghĩa là vài tuần nữa phải cài lại. Trừ
        // điểm theo mức độ cấp bách thay vì ghi cứng tên phiên bản nào đó.
        score -= 25;
        cons.push(format!("Chỉ còn {days} ngày hỗ trợ (đến {label}) rồi ngừng nhận bản vá."));
    } else if days <= 365 {
        score -= 10;
        cons.push(format!("Hỗ trợ đến {label}, còn chưa đầy một năm."));
    } else {
        pros.push(format!("Còn nhận bản vá bảo mật đến {label}."));
    }

    // ---- Ưu tiên mềm ----
    if r.id == "win11-25h2" {
        score += 10; // bản phát hành chính thức hiện hành
    }
    if r.discovered {
        cons.push(
            "Phiên bản này ứng dụng phát hiện từ trang của Microsoft; yêu cầu phần cứng là suy theo bản trước đó, chưa được xác nhận.".into(),
        );
    }
    if r.source == catalog::SourceKind::VolumeLicense {
        score -= 6;
        cons.push("Chỉ phát hành qua kênh doanh nghiệp — bạn cần tự chuẩn bị file ISO.".into());
    }
    if ram < 8.0 && r.family == "Windows 11" {
        score -= 10;
        cons.push("Dưới 8 GB RAM, Windows 11 sẽ chạy khá nặng.".into());
    }
    if hw.system_disk.media_type.as_deref() == Some("HDD") && r.family == "Windows 11" {
        score -= 8;
        cons.push("Máy đang chạy ổ cứng cơ (HDD) — Windows 11 sẽ rất chậm, nên thay SSD.".into());
    }

    let verdict = if !blockers.is_empty() {
        Verdict::Blocked
    } else if needs_bypass {
        Verdict::NeedsBypass
    } else if needs_setup {
        Verdict::NeedsSetup
    } else {
        Verdict::Recommended
    };

    Candidate {
        release: r.clone(),
        end_of_support_label: label,
        days_remaining: days,
        expired,
        score: score.clamp(0, 100),
        verdict,
        pros,
        cons,
        blockers,
    }
}

fn summarize(hw: &HardwareReport, cpu_v: &CpuVerdict, candidates: &[Candidate]) -> String {
    let Some(top) = candidates.first() else {
        return "Không đánh giá được cấu hình máy.".into();
    };

    let machine = format!("{} {}", hw.manufacturer.trim(), hw.model.trim());
    let machine = machine.trim();

    match top.verdict {
        Verdict::Recommended => format!(
            "{machine} đáp ứng đầy đủ yêu cầu của {}. {} và TPM 2.0 đều đạt, nên cài bản này là gọn nhất.",
            top.release.name, cpu_v.family
        ),
        Verdict::NeedsSetup => format!(
            "{machine} cài được {} nhưng cần chỉnh vài thiết lập trong BIOS trước: {}.",
            top.release.name,
            top.cons.join("; ").trim_end_matches('.')
        ),
        Verdict::NeedsBypass => format!(
            "{machine} không qua được bộ kiểm tra của Windows 11 ({}). Bản an toàn nhất là {} — vẫn còn bản vá bảo mật đến {}.",
            cpu_v.reason.trim_end_matches('.'),
            top.release.name,
            top.end_of_support_label
        ),
        Verdict::Blocked => format!(
            "Cấu hình hiện tại của {machine} chưa đủ để cài bất kỳ bản Windows nào một cách trọn vẹn: {}.",
            top.blockers.join("; ").trim_end_matches('.')
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::*;

    /// Mọi test dưới đây chạy ở mốc 28/08/2026 cho kết quả ổn định.
    const NOW: i64 = 20693;

    fn base() -> HardwareReport {
        HardwareReport {
            manufacturer: "Dell".into(),
            model: "Latitude 7490".into(),
            bios_version: "1.20.0".into(),
            cpu: CpuInfo {
                name: "Intel(R) Core(TM) i5-8350U CPU @ 1.70GHz".into(),
                manufacturer: "GenuineIntel".into(),
                cores: 4,
                threads: 8,
                max_clock_mhz: 1700,
                address_width: 64,
                architecture: 9,
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
                version: "10.0.19045".into(),
                build: "19045".into(),
                architecture: "64-bit".into(),
            },
        }
    }

    #[test]
    fn modern_machine_gets_current_release() {
        let rec = analyze_at(&base(), NOW);
        assert_eq!(rec.best, "win11-25h2");
        assert_eq!(rec.candidates[0].verdict, Verdict::Recommended);
        assert_eq!(rec.edition_hint, "Pro");
    }

    #[test]
    fn machine_without_tpm_falls_back_to_ltsc() {
        let mut hw = base();
        hw.tpm = TpmInfo { present: false, version: None, enabled: false, activated: false, source: "none".into() };
        hw.cpu.name = "Intel(R) Core(TM) i5-4200U CPU @ 1.60GHz".into();
        hw.secure_boot = None;
        hw.firmware = "Legacy".into();

        let rec = analyze_at(&hw, NOW);
        assert_eq!(rec.best, "win10-ltsc-2021",
                   "máy cũ không TPM nên được gợi ý LTSC còn hỗ trợ tới 2032");
        // Windows 11 vẫn xuất hiện nhưng phải được đánh dấu là cần bỏ qua kiểm tra.
        let w11 = rec.candidates.iter().find(|c| c.release.id == "win11-25h2").unwrap();
        assert_eq!(w11.verdict, Verdict::NeedsBypass);
    }

    #[test]
    fn tpm_present_but_disabled_is_only_a_bios_change() {
        let mut hw = base();
        hw.tpm.enabled = false;
        let rec = analyze_at(&hw, NOW);
        let w11 = rec.candidates.iter().find(|c| c.release.id == "win11-25h2").unwrap();
        assert_eq!(w11.verdict, Verdict::NeedsSetup,
                   "TPM 2.0 có sẵn mà đang tắt thì chỉ cần vào BIOS, không phải lách");
    }

    #[test]
    fn tpm_read_through_device_manager_still_counts_as_ready() {
        // Chạy ở quyền thường thì Win32_Tpm bị chặn, thông tin đến từ Device
        // Manager. Kết luận phải y hệt như khi đọc được bằng quyền quản trị —
        // nếu không, máy đủ điều kiện lại bị đẩy sang Windows 10.
        let mut hw = base();
        hw.elevated = false;
        hw.tpm.source = "device".into();
        hw.secure_boot_source = "registry".into();

        let rec = analyze_at(&hw, NOW);
        assert_eq!(rec.best, "win11-25h2");
        assert_eq!(rec.candidates[0].verdict, Verdict::Recommended);
    }

    #[test]
    fn a_release_drops_in_rank_once_its_support_date_passes() {
        // Đây là điều quan trọng nhất: danh mục không tự biết bản Windows mới,
        // nhưng ít nhất nó phải tự biết bản cũ đã hết hạn — không cần ai sửa mã.
        let hw = base();
        let before = analyze_at(&hw, catalog::parse_date("2026-06-01").unwrap());
        let after = analyze_at(&hw, catalog::parse_date("2026-11-01").unwrap());

        let pick = |r: &Recommendation| {
            r.candidates.iter().find(|c| c.release.id == "win11-24h2").unwrap().clone()
        };
        let (b, a) = (pick(&before), pick(&after));

        assert!(!b.expired, "01/06/2026 thì 24H2 vẫn còn hỗ trợ");
        assert!(a.expired, "01/11/2026 thì 24H2 đã quá hạn 13/10/2026");
        assert!(a.score < b.score, "hết hạn thì điểm phải tụt");
        assert_eq!(a.end_of_support_label, "13/10/2026");
    }

    #[test]
    fn a_release_nearing_end_of_life_is_flagged_before_it_expires() {
        // 46 ngày trước mốc: chưa hết hạn nhưng phải cảnh báo, vì cài xong là
        // vài tuần sau đã phải cài lại.
        let hw = base();
        let rec = analyze_at(&hw, NOW);
        let c = rec.candidates.iter().find(|x| x.release.id == "win11-24h2").unwrap();

        assert!(!c.expired);
        assert_eq!(c.days_remaining, 46);
        assert!(c.cons.iter().any(|t| t.contains("46 ngày")), "phải nói rõ còn bao nhiêu ngày");
    }

    #[test]
    fn expired_release_is_never_the_top_pick_when_alternatives_exist() {
        let rec = analyze_at(&base(), NOW);
        assert_ne!(rec.best, "win10-22h2");
    }

    #[test]
    fn low_ram_blocks_windows_11() {
        let mut hw = base();
        hw.total_ram = 2 * 1024 * 1024 * 1024;
        let rec = analyze_at(&hw, NOW);
        let w11 = rec.candidates.iter().find(|c| c.release.id == "win11-25h2").unwrap();
        assert_eq!(w11.verdict, Verdict::Blocked);
    }
}
