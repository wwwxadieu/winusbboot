//! Danh mục hệ điều hành mã nguồn mở, và engine gợi ý bản phù hợp với máy.
//!
//! Song song với `catalog.rs` + `recommend.rs` của phía Windows chứ không dùng
//! chung: hai họ hệ điều hành đo bằng những thước đo khác hẳn nhau. Windows
//! hỏi "máy có TPM 2.0 và CPU nằm trong danh sách hỗ trợ không"; Linux thì gần
//! như máy nào cũng cài được, câu hỏi thật là "bản nào chạy mượt trên đúng
//! lượng RAM này, và cắm vào máy đang bật Secure Boot có boot thẳng không".
//!
//! Gộp hai bảng vào một struct sẽ tạo ra một đống trường luôn rỗng ở một nửa số
//! dòng (`requires_tpm2` với Ubuntu, `desktop` với Windows 11), nên tách ra.
//! Thứ dùng chung là `HardwareReport` — chỉ quét máy một lần cho cả hai engine.

use crate::catalog;
use crate::hardware::HardwareReport;
use serde::{Deserialize, Serialize};

/// Môi trường desktop nặng hay nhẹ. Đây là yếu tố quyết định nhất trên máy cũ:
/// cùng một nhân Linux, GNOME cần khoảng 4 GB RAM mới thoải mái, còn XFCE hay
/// LXQt chạy được trong 2 GB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopWeight {
    Light,
    Medium,
    Heavy,
}

/// Distro có được Microsoft ký shim hay không.
///
/// Ranh giới này quan trọng hơn vẻ ngoài của nó: bản có ký thì cắm vào máy đang
/// bật Secure Boot là boot thẳng, bản không ký thì người dùng phải vào BIOS tắt
/// Secure Boot — một việc hoàn toàn làm được nhưng phải nói trước, vì triệu
/// chứng khi không biết là "máy không nhận USB" chứ không có thông báo lỗi nào.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecureBootSupport {
    /// Có shim ký sẵn — không cần đụng vào BIOS.
    Signed,
    /// Phải tắt Secure Boot mới boot được.
    Unsigned,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistroRelease {
    pub id: String,
    pub name: String,
    /// Dòng sản phẩm, để gom nhóm khi hiển thị: "Ubuntu", "Debian"…
    pub family: String,
    pub version: String,
    pub desktop: String,
    pub weight: DesktopWeight,
    /// Ngày phát hành, dạng ISO `YYYY-MM-DD`.
    pub released: String,
    /// Ngày hết hỗ trợ, dạng ISO. `None` với bản rolling release (Arch) —
    /// không có mốc hết hạn nào để so.
    pub end_of_support: Option<String>,
    pub lts: bool,
    /// Bản rolling release cập nhật liên tục, không có phiên bản cố định.
    pub rolling: bool,
    /// RAM tối thiểu để cài được, theo tài liệu chính thức của distro.
    pub min_ram_gb: f64,
    /// RAM để dùng thoải mái. Khoảng cách giữa hai con số này mới là chỗ engine
    /// phân biệt "cài được" với "cài được mà dùng thì ức chế".
    pub rec_ram_gb: f64,
    pub min_disk_gb: f64,
    /// Kiến trúc có bản cài sẵn: "x64", "arm64".
    pub architectures: Vec<String>,
    pub secure_boot: SecureBootSupport,
    /// Dung lượng file ISO, xấp xỉ — để người dùng ước lượng thời gian tải.
    pub iso_size: u64,
    /// Ai nên dùng bản này.
    /// Bản không có trình cài đặt đồ hoạ — toàn bộ quá trình cài làm bằng dòng
    /// lệnh. Trước đây suy ra từ chuỗi tên desktop, nhưng so chuỗi thì vừa dễ vỡ
    /// vừa lẫn với khái niệm "desktop nhẹ": Arch không có desktop *nào cả*, đó
    /// là chuyện khác hẳn với XFCE hay LXQt.
    pub needs_cli_install: bool,
    /// Mức độ hợp với người mới chuyển sang Linux, thang 0–10.
    ///
    /// Đây là đánh giá biên tập chứ không phải thông số kỹ thuật — ghi thành một
    /// trường riêng để người đọc mã thấy rõ chỗ nào ứng dụng đang có quan điểm,
    /// thay vì giấu nó trong công thức tính điểm.
    pub newcomer_fit: i32,
    pub audience: String,
    pub tagline: String,
    /// Trang tải chính thức. Luôn có, và luôn là nguồn đúng nhất.
    pub download_page: String,
    /// File `SHA256SUMS` (hoặc tương đương) của distro.
    ///
    /// Ứng dụng không ghi cứng link ISO, vì tên file đổi theo từng bản vá nhỏ:
    /// `ubuntu-24.04.2` thành `24.04.3` là link cũ chết ngay, mà chết im lặng —
    /// người dùng chỉ thấy "tải thất bại" chứ không biết vì sao. Thay vào đó
    /// ứng dụng đọc file mã băm nằm ở thư mục cố định: nó vừa cho biết tên file
    /// ISO hiện hành, vừa cho luôn mã băm chính thức để đối chiếu sau khi tải.
    /// Một lần đọc giải quyết cả hai việc, và tự đúng qua mọi bản vá nhỏ.
    ///
    /// `None` với distro bắt tải qua trang trung gian — lúc đó giao diện chỉ mở
    /// trang chính thức chứ không đoán một URL sẽ hỏng.
    pub checksum_url: Option<String>,
    /// Chuỗi con để nhận ra đúng dòng ISO cần lấy trong file mã băm — một file
    /// thường liệt kê cả bản desktop, bản live và bản netinst.
    pub iso_match: String,
}

/// Ngày chốt của bảng nhúng dưới đây. Hiện lên giao diện để người dùng biết dữ
/// liệu cũ tới đâu — distro ra bản mới liên tục, và bảng này không tự cập nhật.
pub const CATALOG_SNAPSHOT: &str = "2026-08-29";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Cài được ngay, không phải đụng vào gì.
    Recommended,
    /// Phần cứng đủ, nhưng phải chỉnh một thiết lập trong BIOS (Secure Boot).
    NeedsSetup,
    /// Cài được nhưng máy sẽ ì vì dưới mức RAM khuyến nghị.
    NeedsBypass,
    /// Không đủ điều kiện tối thiểu.
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistroCandidate {
    pub release: DistroRelease,
    pub score: i32,
    pub verdict: Verdict,
    pub pros: Vec<String>,
    pub cons: Vec<String>,
    pub blockers: Vec<String>,
    /// Nhãn vòng đời đã định dạng sẵn cho người đọc.
    pub support_label: String,
    pub expired: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistroRecommendation {
    pub candidates: Vec<DistroCandidate>,
    /// `id` của bản đứng đầu bảng.
    pub best: String,
    pub summary: String,
    pub architecture: String,
    pub ram_gb: f64,
    /// Ngày chốt của bảng nhúng.
    pub catalog_snapshot: String,
}

// ---------------------------------------------------------------------------
// Bảng nhúng
// ---------------------------------------------------------------------------

const GB: u64 = 1024 * 1024 * 1024;

#[allow(clippy::too_many_arguments)]
fn d(
    id: &str,
    name: &str,
    family: &str,
    version: &str,
    desktop: &str,
    weight: DesktopWeight,
    released: &str,
    eos: Option<&str>,
    lts: bool,
    min_ram: f64,
    rec_ram: f64,
    min_disk: f64,
    secure_boot: SecureBootSupport,
    iso_size: u64,
    needs_cli_install: bool,
    newcomer_fit: i32,
    audience: &str,
    tagline: &str,
    page: &str,
    checksum: Option<&str>,
    iso_match: &str,
) -> DistroRelease {
    DistroRelease {
        id: id.into(),
        name: name.into(),
        family: family.into(),
        version: version.into(),
        desktop: desktop.into(),
        weight,
        released: released.into(),
        end_of_support: eos.map(Into::into),
        lts,
        rolling: eos.is_none(),
        min_ram_gb: min_ram,
        rec_ram_gb: rec_ram,
        min_disk_gb: min_disk,
        architectures: vec!["x64".into()],
        secure_boot,
        iso_size,
        needs_cli_install,
        newcomer_fit,
        audience: audience.into(),
        tagline: tagline.into(),
        download_page: page.into(),
        checksum_url: checksum.map(Into::into),
        iso_match: iso_match.into(),
    }
}

/// Bảng nhúng, chốt ngày `CATALOG_SNAPSHOT`.
///
/// Số hiệu phiên bản và ngày hết hỗ trợ ở đây là ảnh chụp tại thời điểm biên
/// dịch — distro ra bản mới theo nhịp riêng của từng dự án và bảng này không tự
/// đồng bộ. Bù lại, link tải không bao giờ lỗi thời vì được tra qua file mã băm
/// (xem `checksum_url`), nên bản vá nhỏ mới ra là ứng dụng tải đúng file mới.
pub fn builtin() -> Vec<DistroRelease> {
    use DesktopWeight::*;
    use SecureBootSupport::*;

    vec![
        d("ubuntu-2404-lts", "Ubuntu 24.04 LTS", "Ubuntu", "24.04", "GNOME", Medium,
          "2024-04-25", Some("2029-05-31"), true, 4.0, 8.0, 25.0, Signed, 6 * GB, false, 9,
          "Người dùng phổ thông muốn một bản có cộng đồng lớn nhất",
          "Bản Linux thông dụng nhất — hỏi gì trên mạng cũng có người đã trả lời.",
          "https://ubuntu.com/download/desktop",
          Some("https://releases.ubuntu.com/24.04/SHA256SUMS"), "desktop-amd64.iso"),

        d("mint-22-cinnamon", "Linux Mint 22 Cinnamon", "Linux Mint", "22", "Cinnamon", Medium,
          "2024-07-25", Some("2029-04-30"), true, 2.0, 4.0, 20.0, Signed, 3 * GB, false, 10,
          "Người vừa chuyển từ Windows sang",
          "Giao diện gần Windows nhất: có menu Start, thanh tác vụ, khay hệ thống ở đúng chỗ quen thuộc.",
          "https://linuxmint.com/download.php",
          Some("https://mirrors.edge.kernel.org/linuxmint/stable/22/sha256sum.txt"),
          "cinnamon-64bit.iso"),

        d("mint-22-xfce", "Linux Mint 22 XFCE", "Linux Mint", "22", "XFCE", Light,
          "2024-07-25", Some("2029-04-30"), true, 2.0, 3.0, 20.0, Signed, 3 * GB, false, 9,
          "Máy cũ, RAM 2–4 GB, muốn giao diện kiểu Windows",
          "Cùng bộ Mint nhưng desktop nhẹ hơn hẳn — cứu tinh của máy đời cũ.",
          "https://linuxmint.com/download.php",
          Some("https://mirrors.edge.kernel.org/linuxmint/stable/22/sha256sum.txt"),
          "xfce-64bit.iso"),

        d("debian-13", "Debian 13 \"Trixie\"", "Debian", "13", "GNOME", Medium,
          "2025-08-09", Some("2030-06-30"), true, 2.0, 4.0, 20.0, Signed, 4 * GB, false, 6,
          "Người cần một máy chạy nhiều năm không phải nâng cấp",
          "Ổn định là mục tiêu số một; vòng đời hỗ trợ dài nhất trong danh sách này.",
          "https://www.debian.org/CD/live/",
          Some("https://cdimage.debian.org/debian-cd/current-live/amd64/iso-hybrid/SHA256SUMS"),
          "gnome.iso"),

        d("fedora-43", "Fedora Workstation 43", "Fedora", "43", "GNOME", Heavy,
          "2025-10-28", Some("2026-12-01"), false, 4.0, 8.0, 20.0, Signed, 2 * GB, false, 6,
          "Lập trình viên muốn nhân và bộ công cụ mới nhất",
          "Luôn đi trước một nhịp về nhân Linux và thư viện — đổi lại phải nâng cấp mỗi năm.",
          "https://fedoraproject.org/workstation/download", None, ""),

        d("popos-2204-lts", "Pop!_OS 22.04 LTS", "Pop!_OS", "22.04", "COSMIC", Medium,
          "2022-04-25", Some("2027-04-30"), true, 4.0, 8.0, 20.0, Signed, 3 * GB, false, 7,
          "Máy có card đồ hoạ NVIDIA rời",
          "Có sẵn bản cài kèm driver NVIDIA — đỡ hẳn phần cài driver thủ công sau khi vào máy.",
          "https://system76.com/pop/download", None, ""),

        d("zorin-17-core", "Zorin OS 17 Core", "Zorin OS", "17", "GNOME (tuỳ biến)", Medium,
          "2024-01-30", Some("2027-04-30"), true, 2.0, 4.0, 15.0, Signed, 4 * GB, false, 9,
          "Người muốn giao diện đổi được sang kiểu Windows hoặc macOS",
          "Có sẵn công cụ đổi bố cục desktop sang kiểu Windows 11, Windows 7 hay macOS.",
          "https://zorin.com/os/download/", None, ""),

        d("lubuntu-2404-lts", "Lubuntu 24.04 LTS", "Lubuntu", "24.04", "LXQt", Light,
          "2024-04-25", Some("2027-04-30"), true, 1.0, 2.0, 15.0, Signed, 3 * GB, false, 7,
          "Máy rất yếu — RAM 1–2 GB, ổ cứng cơ",
          "Bản nhẹ nhất trong danh sách; hồi sinh được cả máy netbook đời 2010.",
          "https://lubuntu.me/downloads/",
          Some("https://cdimage.ubuntu.com/lubuntu/releases/24.04/release/SHA256SUMS"),
          "desktop-amd64.iso"),

        d("archlinux", "Arch Linux", "Arch", "rolling", "Không có sẵn", Light,
          "2002-03-11", None, false, 1.0, 2.0, 10.0, Unsigned, 1 * GB, true, 1,
          "Người đã quen dòng lệnh và muốn tự dựng hệ thống từ đầu",
          "Rolling release, không có bản cài đồ hoạ — bạn tự chọn từng thành phần.",
          "https://archlinux.org/download/",
          Some("https://geo.mirror.pkgbuild.com/iso/latest/sha256sums.txt"),
          "x86_64.iso"),
    ]
}

// ---------------------------------------------------------------------------
// Engine gợi ý
// ---------------------------------------------------------------------------

const GB_F: f64 = 1024.0 * 1024.0 * 1024.0;

/// Kiến trúc bộ xử lý, quy về đúng chuỗi dùng trong `architectures`.
fn arch_of(hw: &HardwareReport) -> &'static str {
    // 12 là mã ARM64 trong Win32_Processor.
    if hw.cpu.architecture == 12 {
        "arm64"
    } else if hw.cpu.address_width >= 64 {
        "x64"
    } else {
        "x86"
    }
}

pub fn analyze(hw: &HardwareReport) -> DistroRecommendation {
    analyze_at(hw, catalog::today())
}

/// Bản có truyền mốc thời gian, dùng cho kiểm thử — vòng đời của distro cũng
/// phải tính từ đồng hồ chứ không ghi cứng, y như phía Windows.
pub fn analyze_at(hw: &HardwareReport, today: i64) -> DistroRecommendation {
    let arch = arch_of(hw);
    let ram_gb = hw.total_ram as f64 / GB_F;
    let free_gb = hw.system_disk.free as f64 / GB_F;

    let mut candidates: Vec<DistroCandidate> = builtin()
        .into_iter()
        .map(|r| score(r, hw, arch, ram_gb, free_gb, today))
        .collect();

    // Điểm cao lên đầu. Bằng điểm thì bản còn hỗ trợ lâu hơn được ưu tiên;
    // bản rolling coi như còn hỗ trợ vô hạn.
    candidates.sort_by(|a, b| {
        b.score.cmp(&a.score).then_with(|| {
            remaining(&b.release, today).cmp(&remaining(&a.release, today))
        })
    });

    let best = candidates
        .iter()
        .find(|c| c.verdict != Verdict::Blocked)
        .or_else(|| candidates.first())
        .map(|c| c.release.id.clone())
        .unwrap_or_default();

    let summary = summarise(hw, &candidates, ram_gb);

    DistroRecommendation {
        candidates,
        best,
        summary,
        architecture: arch.to_string(),
        ram_gb,
        catalog_snapshot: CATALOG_SNAPSHOT.to_string(),
    }
}

/// Số ngày còn hỗ trợ. Bản rolling không có mốc hết hạn nên trả về một giá trị
/// lớn để nó luôn đứng trên bản sắp hết hạn khi bằng điểm.
fn remaining(r: &DistroRelease, today: i64) -> i64 {
    match r.end_of_support.as_deref().and_then(catalog::parse_date) {
        Some(day) => day - today,
        None => i64::MAX / 2,
    }
}

fn score(
    r: DistroRelease,
    hw: &HardwareReport,
    arch: &str,
    ram_gb: f64,
    free_gb: f64,
    today: i64,
) -> DistroCandidate {
    // Điểm nền cố tình để thấp: mọi thứ phía dưới đều cộng trừ vào đây, và nếu
    // nền quá cao thì bản nào cũng chạm trần 100 rồi thứ hạng thật lại do
    // tie-break quyết định — tức là engine không còn chấm điểm gì nữa.
    let mut score: f64 = 45.0;
    let mut pros: Vec<String> = Vec::new();
    let mut cons: Vec<String> = Vec::new();
    let mut blockers: Vec<String> = Vec::new();
    let mut verdict = Verdict::Recommended;

    // --- Rào chặn cứng: không lách được bằng bất kỳ thiết lập nào ---------
    if !r.architectures.iter().any(|a| a == arch) {
        blockers.push(format!(
            "Bản này không có bộ cài cho kiến trúc {arch} của máy bạn."
        ));
    }
    // Nới 0,15 GB vì phần RAM dành cho đồ hoạ tích hợp bị trừ khỏi con số hệ
    // điều hành đọc được — máy 4 GB thật thường báo 3,87 GB.
    if ram_gb + 0.15 < r.min_ram_gb {
        blockers.push(format!(
            "Máy chỉ có {ram_gb:.1} GB RAM, bản này cần tối thiểu {:.0} GB.",
            r.min_ram_gb
        ));
    }
    if free_gb + 0.5 < r.min_disk_gb {
        blockers.push(format!(
            "Chỉ còn {free_gb:.0} GB trống, bản này cần tối thiểu {:.0} GB.",
            r.min_disk_gb
        ));
    }

    // --- RAM so với mức khuyến nghị --------------------------------------
    //
    // Ranh giới đáng giá nhất của engine này. "Đủ RAM tối thiểu" và "đủ RAM để
    // dùng thoải mái" là hai chuyện khác nhau, và gộp chúng lại thì máy 4 GB sẽ
    // được gợi ý GNOME rồi người dùng kết luận "Linux chạy chậm".
    //
    // Tính theo tỉ lệ chứ không theo bậc: máy 7,5 GB và máy 4 GB đều "dưới mức
    // khuyến nghị 8 GB", nhưng trải nghiệm của hai máy đó khác nhau rất xa.
    let ratio = ram_gb / r.rec_ram_gb.max(0.1);
    if blockers.is_empty() {
        if ratio >= 1.5 {
            pros.push(format!("RAM {ram_gb:.1} GB dư dả so với mức bản này cần."));
            score += 18.0;
        } else if ratio >= 0.98 {
            pros.push(format!("RAM {ram_gb:.1} GB đủ cho desktop này."));
            score += 14.0;
        } else {
            // Càng thiếu càng trừ nặng, tối đa 25 điểm.
            score -= (25.0 * (1.0 - ratio)).min(25.0);
            cons.push(format!(
                "RAM {ram_gb:.1} GB dưới mức khuyến nghị {:.0} GB — mở nhiều cửa sổ sẽ thấy ì.",
                r.rec_ram_gb
            ));
            verdict = Verdict::NeedsBypass;
        }
    }

    // Desktop nhẹ trên máy ít RAM là điểm cộng thật. Điều kiện `!needs_cli_install`
    // mới là chỗ quan trọng: Arch được xếp Light vì *không có desktop nào cả*,
    // thưởng cho nó ở đây là gợi ý Arch cho đúng nhóm người dùng ít khả năng
    // cài được nó nhất.
    match r.weight {
        DesktopWeight::Light if ram_gb < 4.5 && !r.needs_cli_install => {
            pros.push("Desktop nhẹ — đúng thứ máy cấu hình này cần.".into());
            score += 16.0;
        }
        DesktopWeight::Heavy if ram_gb < 6.0 => {
            cons.push("Môi trường desktop nặng, máy ít RAM sẽ vất vả.".into());
            score -= 14.0;
        }
        _ => {}
    }

    // --- Vòng đời --------------------------------------------------------
    let days = remaining(&r, today);
    let expired = r.end_of_support.is_some() && days < 0;
    let support_label = match r.end_of_support.as_deref() {
        None => "Rolling release — cập nhật liên tục".to_string(),
        Some(iso) if expired => format!("Hết hỗ trợ {}", catalog::format_date(iso)),
        Some(iso) => format!("Đến {}", catalog::format_date(iso)),
    };

    if expired {
        cons.push(format!(
            "Đã hết hỗ trợ từ {} — không còn bản vá bảo mật.",
            catalog::format_date(r.end_of_support.as_deref().unwrap_or_default())
        ));
        score -= 40.0;
    } else if r.rolling {
        // Không có mốc hết hạn, nhưng cũng không có mốc nào để hứa hẹn.
        score += 8.0;
    } else {
        if days <= 180 {
            cons.push(format!("Chỉ còn {days} ngày hỗ trợ rồi phải nâng cấp lên bản mới."));
        }
        if r.lts {
            pros.push(format!("Hỗ trợ dài hạn, {}.", support_label.to_lowercase()));
            score += 10.0;
        }
        // Mỗi năm còn lại thêm 2 điểm, trần 12 — đây là thứ tách được những bản
        // ngang nhau về mọi mặt khác.
        score += (days as f64 / 365.0 * 2.0).clamp(0.0, 12.0);
    }

    // --- Secure Boot ------------------------------------------------------
    //
    // Chỉ trừ điểm khi máy *đang bật* Secure Boot. Máy đang tắt (hoặc chạy BIOS
    // cũ, đọc ra `None`) thì distro không ký chẳng vướng gì cả — phạt ở đó là
    // phạt oan cho mọi máy đời cũ.
    if r.secure_boot == SecureBootSupport::Unsigned {
        if hw.secure_boot == Some(true) {
            cons.push(
                "Không có shim ký sẵn — phải vào BIOS tắt Secure Boot thì máy mới boot được USB."
                    .into(),
            );
            score -= 16.0;
            if verdict == Verdict::Recommended {
                verdict = Verdict::NeedsSetup;
            }
        }
    } else if hw.secure_boot == Some(true) {
        pros.push("Có shim ký sẵn — boot thẳng dù Secure Boot đang bật.".into());
        score += 5.0;
    }

    // --- Bản phải cài bằng dòng lệnh --------------------------------------
    //
    // Trừ nặng, và cố ý nặng: một bản không có trình cài đặt đồ hoạ không phải
    // là "hơi bất tiện hơn" mà là một loại việc khác hẳn. Người mở ứng dụng này
    // để tạo USB cài Windows gần như chắc chắn không muốn phân vùng ổ đĩa bằng
    // fdisk. Ai thật sự muốn Arch thì vẫn chọn được — chỉ là không bao giờ bị
    // ứng dụng tự đẩy vào.
    if r.needs_cli_install {
        cons.push("Không có trình cài đặt đồ hoạ — toàn bộ quá trình cài làm bằng dòng lệnh.".into());
        score -= 40.0;
    }

    // Đánh giá biên tập về mức độ hợp với người mới.
    score += r.newcomer_fit as f64;

    if !blockers.is_empty() {
        verdict = Verdict::Blocked;
        score = score.min(20.0);
    }

    DistroCandidate {
        release: r,
        score: score.round().clamp(0.0, 100.0) as i32,
        verdict,
        pros,
        cons,
        blockers,
        support_label,
        expired,
    }
}

fn summarise(hw: &HardwareReport, candidates: &[DistroCandidate], ram_gb: f64) -> String {
    let machine = [hw.manufacturer.as_str(), hw.model.as_str()]
        .join(" ")
        .trim()
        .to_string();
    let machine = if machine.is_empty() { "Máy này".into() } else { machine };

    let Some(top) = candidates.iter().find(|c| c.verdict != Verdict::Blocked) else {
        return format!(
            "{machine} chỉ có {ram_gb:.1} GB RAM và dung lượng trống hiện tại, chưa đủ cho bản nào trong danh sách."
        );
    };

    let why = if top.release.needs_cli_install {
        "Bản này cài bằng dòng lệnh — chỉ nên chọn nếu bạn đã quen với việc đó.".to_string()
    } else if ram_gb < 4.5 {
        format!(
            "Với {ram_gb:.1} GB RAM thì desktop nhẹ như {} là lựa chọn chạy mượt nhất.",
            top.release.desktop
        )
    } else {
        format!(
            "Cấu hình này thoải mái cho {} — chọn bản nào cũng chạy tốt, đây là bản cân bằng nhất.",
            top.release.desktop
        )
    };

    format!("{machine} hợp nhất với {}. {why}", top.release.name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::*;

    /// Mọi test dưới đây chạy ở mốc 29/08/2026 cho kết quả ổn định.
    const NOW: i64 = 20694;

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
            gpus: vec![],
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

    fn find<'a>(rec: &'a DistroRecommendation, id: &str) -> &'a DistroCandidate {
        rec.candidates
            .iter()
            .find(|c| c.release.id == id)
            .unwrap_or_else(|| panic!("không thấy {id} trong danh mục"))
    }

    #[test]
    fn every_entry_has_a_unique_id_and_an_official_page() {
        let all = builtin();
        let mut ids: Vec<&str> = all.iter().map(|r| r.id.as_str()).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(before, ids.len(), "id bị trùng trong danh mục");

        for r in &all {
            assert!(r.download_page.starts_with("https://"), "{} thiếu trang tải", r.id);
            assert!(r.rec_ram_gb >= r.min_ram_gb, "{}: RAM khuyến nghị thấp hơn tối thiểu", r.id);
            // Có link mã băm thì phải có chuỗi nhận dạng file, không thì lúc tải
            // sẽ không biết lấy dòng nào trong file mã băm.
            if r.checksum_url.is_some() {
                assert!(!r.iso_match.is_empty(), "{} có link mã băm nhưng thiếu iso_match", r.id);
            }
        }
    }

    #[test]
    fn a_modern_laptop_gets_a_full_desktop() {
        let rec = analyze_at(&base(), NOW);
        let top = &rec.candidates[0];
        assert_eq!(top.verdict, Verdict::Recommended);
        assert_ne!(top.release.weight, DesktopWeight::Light,
            "máy 16 GB RAM không có lý do gì phải dùng desktop tối giản");
    }

    /// Ranh giới quan trọng nhất của engine này.
    #[test]
    fn a_two_gigabyte_machine_is_steered_to_a_light_desktop() {
        let mut hw = base();
        hw.total_ram = 2 * 1024 * 1024 * 1024;

        let rec = analyze_at(&hw, NOW);
        let top = &rec.candidates[0];
        assert_eq!(top.release.weight, DesktopWeight::Light,
            "máy 2 GB RAM phải được gợi ý desktop nhẹ, đang gợi ý {}", top.release.name);

        // Ubuntu GNOME vẫn cài được về mặt kỹ thuật, nhưng phải bị hạ xuống mức
        // "chạy được thôi" chứ không được nói là cài được ngay.
        let ubuntu = find(&rec, "ubuntu-2404-lts");
        assert_ne!(ubuntu.verdict, Verdict::Recommended);
    }

    #[test]
    fn ram_below_the_minimum_is_a_hard_block() {
        let mut hw = base();
        hw.total_ram = 1536 * 1024 * 1024; // 1,5 GB

        let rec = analyze_at(&hw, NOW);
        let ubuntu = find(&rec, "ubuntu-2404-lts");
        assert_eq!(ubuntu.verdict, Verdict::Blocked);
        assert!(!ubuntu.blockers.is_empty());

        // Lubuntu chỉ cần 1 GB nên vẫn phải cài được — chặn hết mọi bản là sai.
        assert_ne!(find(&rec, "lubuntu-2404-lts").verdict, Verdict::Blocked);
    }

    /// Secure Boot đang bật chỉ là một thiết lập trong BIOS, không phải rào chặn.
    #[test]
    fn secure_boot_makes_arch_a_bios_change_not_a_block() {
        let mut hw = base();
        hw.secure_boot = Some(true);

        let arch = find(&analyze_at(&hw, NOW), "archlinux").clone();
        assert_eq!(arch.verdict, Verdict::NeedsSetup);
        assert!(arch.blockers.is_empty(), "tắt Secure Boot được thì không phải rào chặn cứng");
        assert!(arch.cons.iter().any(|c| c.contains("Secure Boot")));
    }

    /// Và ngược lại: máy đã tắt Secure Boot thì distro không ký chẳng vướng gì.
    #[test]
    fn an_unsigned_distro_is_not_penalised_when_secure_boot_is_off() {
        let mut hw = base();
        hw.secure_boot = Some(false);

        let rec = analyze_at(&hw, NOW);
        let arch = find(&rec, "archlinux");
        assert!(!arch.cons.iter().any(|c| c.contains("Secure Boot")));
        assert_ne!(arch.verdict, Verdict::NeedsSetup);
    }

    /// Máy chạy BIOS cũ không đọc được trạng thái Secure Boot (`None`). Coi
    /// `None` như "đang bật" sẽ cảnh báo sai cho mọi máy đời cũ.
    #[test]
    fn unreadable_secure_boot_is_not_treated_as_enabled() {
        let mut hw = base();
        hw.secure_boot = None;
        hw.firmware = "Legacy".into();

        let arch = find(&analyze_at(&hw, NOW), "archlinux").clone();
        assert!(!arch.cons.iter().any(|c| c.contains("Secure Boot")));
    }

    /// Điểm nền quá cao thì mọi bản đều chạm trần 100, và lúc đó thứ hạng thật
    /// lại do tie-break quyết định chứ không phải do chấm điểm. Lỗi này không
    /// làm hỏng test nào khác vì bản đứng đầu vẫn "đúng" một cách tình cờ.
    #[test]
    fn scores_actually_separate_instead_of_all_hitting_the_ceiling() {
        let rec = analyze_at(&base(), NOW);
        let capped = rec.candidates.iter().filter(|c| c.score >= 100).count();
        assert!(capped <= 1, "{capped} bản cùng chạm trần điểm — engine không phân biệt được gì");

        let usable: Vec<i32> = rec
            .candidates
            .iter()
            .filter(|c| c.verdict != Verdict::Blocked)
            .map(|c| c.score)
            .collect();
        let spread = usable.iter().max().unwrap() - usable.iter().min().unwrap();
        assert!(spread >= 15, "điểm chỉ chênh nhau {spread} — quá sát để xếp hạng có nghĩa");
    }

    /// Arch được xếp `Light` vì nó không có desktop nào cả, không phải vì desktop
    /// của nó nhẹ. Thưởng điểm "desktop nhẹ" cho nó sẽ đẩy đúng bản khó cài nhất
    /// lên đầu bảng cho đúng nhóm máy của người dùng ít kinh nghiệm nhất.
    #[test]
    fn a_command_line_only_distro_is_never_the_top_pick() {
        for (label, hw) in [("máy mạnh", base()), ("máy yếu", {
            let mut h = base();
            h.total_ram = 2 * 1024 * 1024 * 1024;
            h.secure_boot = None;
            h
        })] {
            let rec = analyze_at(&hw, NOW);
            assert_ne!(rec.best, "archlinux", "{label}: không được tự gợi ý Arch");
        }

        let arch = find(&analyze_at(&base(), NOW), "archlinux").clone();
        assert!(arch.release.needs_cli_install);
        assert!(arch.cons.iter().any(|c| c.contains("dòng lệnh")));
    }

    /// Và câu tóm tắt cũng không được gọi tên một desktop không tồn tại.
    #[test]
    fn the_summary_never_names_a_desktop_that_is_not_there() {
        for r in builtin().iter().filter(|r| r.needs_cli_install) {
            assert!(
                !r.desktop.is_empty(),
                "{}: trường desktop rỗng sẽ lọt vào câu tóm tắt", r.id
            );
        }
        let rec = analyze_at(&base(), NOW);
        assert!(!rec.summary.contains("Không có sẵn"), "câu tóm tắt: {}", rec.summary);
    }

    #[test]
    fn a_release_past_its_date_drops_down_the_ranking() {
        let hw = base();
        let fedora_now = find(&analyze_at(&hw, NOW), "fedora-43").score;
        // Cùng cấu hình máy, chỉ khác ngày: hai năm sau Fedora 43 đã quá hạn.
        let fedora_later = find(&analyze_at(&hw, NOW + 730), "fedora-43").clone();

        assert!(fedora_later.expired);
        assert!(fedora_later.score < fedora_now,
            "bản hết hỗ trợ phải tụt điểm: {} so với {fedora_now}", fedora_later.score);
    }

    /// Bản rolling không có ngày hết hỗ trợ — không được coi là đã quá hạn.
    #[test]
    fn a_rolling_release_never_expires() {
        let arch = find(&analyze_at(&base(), NOW + 3650), "archlinux").clone();
        assert!(!arch.expired);
        assert!(arch.release.rolling);
        assert!(arch.support_label.contains("Rolling"));
    }

    #[test]
    fn a_32_bit_machine_is_blocked_everywhere_rather_than_offered_a_bad_iso() {
        let mut hw = base();
        hw.cpu.address_width = 32;

        let rec = analyze_at(&hw, NOW);
        assert_eq!(rec.architecture, "x86");
        assert!(rec.candidates.iter().all(|c| c.verdict == Verdict::Blocked));
    }

    #[test]
    fn a_full_disk_blocks_installation() {
        let mut hw = base();
        hw.system_disk.free = 5 * 1024 * 1024 * 1024;

        let ubuntu = find(&analyze_at(&hw, NOW), "ubuntu-2404-lts").clone();
        assert_eq!(ubuntu.verdict, Verdict::Blocked);
        assert!(ubuntu.blockers.iter().any(|b| b.contains("trống")));
    }
}
