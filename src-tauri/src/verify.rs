//! Kiểm tra chiếc USB vừa ghi có thật sự khởi động được không.
//!
//! Ghi xong không có nghĩa là boot được. Ba nhóm nguyên nhân thường gặp, và
//! không nhóm nào báo lỗi lúc ghi:
//!
//! 1. **Chép hụt.** Ổ đầy giữa chừng, hoặc một file bị khoá nên bị bỏ qua —
//!    `robocopy` vẫn kết thúc với mã thành công.
//! 2. **Thiếu đường khởi động.** Có đủ file cài đặt nhưng thiếu `bootmgr` hoặc
//!    `efi\boot\bootx64.efi`, nên firmware không tìm ra thứ gì để chạy.
//! 3. **USB dối.** Ổ khai 128 GB nhưng thật ra chỉ có 8 GB, hoặc flash đã gần
//!    chết. Ghi thì "thành công" vì thiết bị nhận hết dữ liệu rồi vứt đi.
//!
//! Nhóm 1 và 2 phát hiện được bằng cách đọc cấu trúc ổ — vài giây. Nhóm 3 chỉ
//! lộ ra khi **đọc ngược toàn bộ dữ liệu vừa ghi và đối chiếu**, việc này mất
//! gần bằng thời gian ghi nên để người dùng tự bấm.
//!
//! Toàn bộ phần *đánh giá* nằm ở các hàm thuần bên dưới, tách khỏi phần thu
//! thập dữ liệu qua PowerShell. Đây mới là chỗ dễ sai — kết luận "không boot
//! được" cho một chiếc USB hoàn toàn tốt còn tệ hơn là không kiểm tra gì — nên
//! nó phải kiểm thử được mà không cần tới máy Windows.

use crate::error::{AppError, Result};
use crate::ps;
use crate::usb;
use crate::writer;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckLevel {
    Pass,
    /// Không đúng chuẩn nhưng không chặn khởi động.
    Warn,
    Fail,
    /// Không đọc được — **không phải** là không đạt.
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootCheck {
    pub id: String,
    pub group: String,
    pub label: String,
    /// Giá trị đọc được trên chiếc USB này.
    pub value: String,
    /// Điều kiện để khởi động được.
    pub expectation: String,
    pub level: CheckLevel,
    pub hint: Option<String>,
    /// Không đạt thì firmware chắc chắn không khởi động được từ ổ này.
    pub blocking: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BootVerdict {
    /// Đủ điều kiện khởi động, không có cảnh báo nào.
    Ready,
    /// Khởi động được nhưng có điểm cần biết trước.
    ReadyWithWarnings,
    /// Chắc chắn không khởi động được.
    NotBootable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootReport {
    pub checks: Vec<BootCheck>,
    pub passed: u32,
    pub warned: u32,
    pub failed: u32,
    pub skipped: u32,
    /// Khởi động được trên máy UEFI đời mới.
    pub bootable_uefi: bool,
    /// Khởi động được trên máy BIOS đời cũ / chế độ CSM.
    pub bootable_legacy: bool,
    pub verdict: BootVerdict,
    pub summary: String,
}

fn check(
    id: &str,
    group: &str,
    label: &str,
    value: impl Into<String>,
    expectation: &str,
    level: CheckLevel,
    hint: Option<&str>,
    blocking: bool,
) -> BootCheck {
    BootCheck {
        id: id.into(),
        group: group.into(),
        label: label.into(),
        value: value.into(),
        expectation: expectation.into(),
        level,
        hint: hint.map(Into::into),
        blocking,
    }
}

/// Gộp danh sách mục thành một kết luận.
///
/// Chỉ mục `blocking` mới hạ được kết luận xuống "không khởi động được". Một
/// mục `Fail` không chặn (ví dụ thiếu `autounattend.xml`) làm hỏng trải nghiệm
/// chứ không làm hỏng việc khởi động, và nói quá lên thì người dùng sẽ ghi lại
/// một chiếc USB vốn đã dùng được.
pub fn summarise(checks: Vec<BootCheck>, uefi: bool, legacy: bool) -> BootReport {
    let mut passed = 0;
    let mut warned = 0;
    let mut failed = 0;
    let mut skipped = 0;
    for c in &checks {
        match c.level {
            CheckLevel::Pass => passed += 1,
            CheckLevel::Warn => warned += 1,
            CheckLevel::Fail => failed += 1,
            CheckLevel::Skipped => skipped += 1,
        }
    }

    let blocked = checks
        .iter()
        .any(|c| c.blocking && c.level == CheckLevel::Fail);

    let verdict = if blocked || (!uefi && !legacy) {
        BootVerdict::NotBootable
    } else if warned > 0 || failed > 0 {
        BootVerdict::ReadyWithWarnings
    } else {
        BootVerdict::Ready
    };

    let ways = match (uefi, legacy) {
        (true, true) => "cả máy UEFI đời mới lẫn máy BIOS đời cũ",
        (true, false) => "máy UEFI đời mới",
        (false, true) => "máy BIOS đời cũ hoặc UEFI ở chế độ CSM",
        (false, false) => "",
    };

    // "Không khởi động được" và "khởi động được nhưng bộ cài hỏng" là hai
    // chuyện khác nhau, và gộp chúng lại thì kết luận tự mâu thuẫn với chính
    // bảng ngay bên dưới nó: chữ đỏ "chưa khởi động được" trong khi cả hai
    // đường khởi động đều xanh. Người dùng cần biết mình đang gặp cái nào —
    // thiếu mã khởi động thì máy không thấy USB, còn thiếu file cài đặt thì
    // máy vẫn boot rồi mới dừng giữa chừng.
    let summary = match verdict {
        BootVerdict::NotBootable if !uefi && !legacy => {
            "Máy sẽ không thấy USB này trong menu khởi động — trên ổ không có mã khởi động nào.              Xem các mục đỏ bên dưới rồi ghi lại."
                .to_string()
        }
        BootVerdict::NotBootable => format!(
            "USB khởi động được trên {ways}, nhưng bộ cài trên đó chưa hoàn chỉnh nên quá trình              cài sẽ dừng giữa chừng. Xem các mục đỏ bên dưới rồi ghi lại."
        ),
        BootVerdict::ReadyWithWarnings => {
            format!("USB khởi động được trên {ways}, nhưng có {} điểm cần biết trước.", warned + failed)
        }
        BootVerdict::Ready => format!("USB sẵn sàng khởi động trên {ways}."),
    };

    BootReport { checks, passed, warned, failed, skipped, bootable_uefi: uefi, bootable_legacy: legacy, verdict, summary }
}

// ---------------------------------------------------------------------------
// Luồng Windows: ổ đã format rồi chép file lên
// ---------------------------------------------------------------------------

/// Những đường dẫn quyết định việc khởi động. Giữ thành hằng số để script
/// PowerShell và phần đánh giá không bao giờ lệch nhau.
pub const WINDOWS_PROBES: &[&str] = &[
    "bootmgr",
    "boot\\bcd",
    "efi\\boot\\bootx64.efi",
    "efi\\boot\\bootia32.efi",
    "efi\\microsoft\\boot\\bcd",
    "sources\\boot.wim",
    "sources\\install.wim",
    "sources\\install.esd",
    "sources\\install.swm",
    "autounattend.xml",
];

/// FAT32 không chứa nổi file từ 4 GiB trở lên.
const FAT32_MAX_FILE: u64 = 4 * 1024 * 1024 * 1024 - 1;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProbedFile {
    pub path: String,
    pub exists: bool,
    pub size: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WindowsUsbSnapshot {
    pub partition_style: String,
    pub filesystem: String,
    pub drive_letter: String,
    pub volume_label: String,
    /// Cờ active của phân vùng, chỉ có nghĩa với ổ MBR.
    pub is_active: bool,
    pub files: Vec<ProbedFile>,
    pub file_count: u64,
    pub total_bytes: u64,
    pub largest_path: String,
    pub largest_bytes: u64,
}

impl WindowsUsbSnapshot {
    fn probe(&self, path: &str) -> Option<&ProbedFile> {
        self.files.iter().find(|f| f.path.eq_ignore_ascii_case(path))
    }
    fn has(&self, path: &str) -> bool {
        self.probe(path).map(|f| f.exists && f.size > 0).unwrap_or(false)
    }
}

fn human(bytes: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{:.1} {}", v, U[i]).replace('.', ",")
}

/// Chấm một chiếc USB cài Windows. Hàm thuần — không đụng tới hệ thống.
pub fn evaluate_windows(s: &WindowsUsbSnapshot, expect_unattend: bool) -> BootReport {
    let mut out: Vec<BootCheck> = Vec::new();
    let fat32 = s.filesystem.eq_ignore_ascii_case("FAT32");
    let mbr = s.partition_style.eq_ignore_ascii_case("MBR");

    // --- Cấu trúc ổ -------------------------------------------------------
    out.push(check(
        "layout", "Cấu trúc ổ", "Kiểu phân vùng và hệ thống file",
        format!("{} + {} · ổ {}:", s.partition_style, s.filesystem, s.drive_letter),
        "GPT + FAT32 cho máy UEFI, MBR cho máy BIOS cũ",
        if s.partition_style.is_empty() { CheckLevel::Skipped } else { CheckLevel::Pass },
        None, false,
    ));

    if mbr {
        out.push(check(
            "active", "Cấu trúc ổ", "Cờ active của phân vùng",
            if s.is_active { "Đã bật" } else { "Chưa bật" },
            "Ổ MBR phải có phân vùng active thì BIOS mới nạp mã khởi động",
            if s.is_active { CheckLevel::Pass } else { CheckLevel::Fail },
            (!s.is_active).then_some(
                "Hãy ghi lại. BIOS đời cũ bỏ qua ổ MBR không có phân vùng active."
            ),
            true,
        ));
    }

    // Giới hạn 4 GB của FAT32 là lỗi âm thầm điển hình: file bị cắt hoặc bị bỏ
    // qua lúc chép, Windows Setup chạy tới giữa chừng mới báo thiếu.
    let oversize = fat32 && s.largest_bytes > FAT32_MAX_FILE;
    out.push(check(
        "fat32_limit", "Cấu trúc ổ", "Giới hạn 4 GB của FAT32",
        if s.largest_bytes == 0 {
            "Không đọc được".to_string()
        } else {
            format!("File lớn nhất: {} ({})", human(s.largest_bytes), s.largest_path)
        },
        "Trên FAT32 không file nào được từ 4 GB trở lên",
        if s.largest_bytes == 0 {
            CheckLevel::Skipped
        } else if oversize {
            CheckLevel::Fail
        } else {
            CheckLevel::Pass
        },
        oversize.then_some(
            "File này vượt giới hạn FAT32 nên chắc chắn chưa được chép trọn vẹn. \
             Hãy ghi lại và để ứng dụng tách install.wim, hoặc chọn kiểu MBR + NTFS."
        ),
        true,
    ));

    // --- Đường khởi động --------------------------------------------------
    let efi64 = s.has("efi\\boot\\bootx64.efi");
    let efi32 = s.has("efi\\boot\\bootia32.efi");
    let has_efi_loader = efi64 || efi32;

    // Gần như mọi firmware UEFI chỉ đọc được FAT32. Có bootx64.efi trên phân
    // vùng NTFS thì file đó nằm ở nơi firmware không với tới được.
    let uefi_ok = has_efi_loader && fat32;

    out.push(check(
        "uefi_loader", "Đường khởi động", "Mã khởi động UEFI",
        match (efi64, efi32) {
            (true, true) => "Có bootx64.efi và bootia32.efi".to_string(),
            (true, false) => "Có efi\\boot\\bootx64.efi".to_string(),
            (false, true) => "Chỉ có efi\\boot\\bootia32.efi".to_string(),
            (false, false) => "Không tìm thấy".to_string(),
        },
        "Máy UEFI tìm file efi\\boot\\bootx64.efi để khởi động",
        if uefi_ok { CheckLevel::Pass } else if has_efi_loader { CheckLevel::Warn } else { CheckLevel::Fail },
        if has_efi_loader && !fat32 {
            Some("Có mã khởi động UEFI nhưng nó nằm trên phân vùng NTFS, mà hầu hết \
                  firmware UEFI không đọc được NTFS. Ổ này chỉ dùng được ở chế độ BIOS/CSM.")
        } else if !has_efi_loader {
            Some("Thiếu file này thì máy đời mới sẽ không thấy USB trong menu boot.")
        } else {
            None
        },
        false,
    ));

    let legacy_ok = s.has("bootmgr") && s.has("boot\\bcd");
    out.push(check(
        "legacy_loader", "Đường khởi động", "Mã khởi động BIOS đời cũ",
        match (s.has("bootmgr"), s.has("boot\\bcd")) {
            (true, true) => "Có bootmgr và boot\\bcd",
            (true, false) => "Có bootmgr nhưng thiếu boot\\bcd",
            (false, true) => "Có boot\\bcd nhưng thiếu bootmgr",
            (false, false) => "Không tìm thấy",
        },
        "Máy BIOS cũ cần bootmgr ở thư mục gốc và boot\\bcd",
        if legacy_ok { CheckLevel::Pass } else { CheckLevel::Warn },
        (!legacy_ok).then_some(
            "Máy UEFI vẫn khởi động được bình thường; chỉ máy BIOS đời cũ là không."
        ),
        false,
    ));

    // --- File cài đặt -----------------------------------------------------
    let boot_wim = s.probe("sources\\boot.wim");
    out.push(check(
        "boot_wim", "File cài đặt", "Ảnh khởi động (sources\\boot.wim)",
        boot_wim.filter(|f| f.exists).map(|f| human(f.size)).unwrap_or_else(|| "Không tìm thấy".into()),
        "Bắt buộc — đây là môi trường Windows Setup chạy trong đó",
        if s.has("sources\\boot.wim") { CheckLevel::Pass } else { CheckLevel::Fail },
        (!s.has("sources\\boot.wim")).then_some(
            "Thiếu file này thì máy nạp được mã khởi động rồi dừng lại ngay sau đó."
        ),
        true,
    ));

    let install = ["sources\\install.wim", "sources\\install.esd", "sources\\install.swm"]
        .iter()
        .find(|p| s.has(p));
    out.push(check(
        "install_image", "File cài đặt", "Ảnh cài đặt Windows",
        install
            .and_then(|p| s.probe(p))
            .map(|f| format!("{} · {}", f.path, human(f.size)))
            .unwrap_or_else(|| "Không tìm thấy".into()),
        "Cần install.wim, install.esd, hoặc bộ install.swm đã tách",
        if install.is_some() { CheckLevel::Pass } else { CheckLevel::Fail },
        install.is_none().then_some(
            "Setup sẽ chạy được tới bước chọn phiên bản rồi báo không tìm thấy file cài đặt."
        ),
        true,
    ));

    if expect_unattend {
        let ok = s.has("autounattend.xml");
        out.push(check(
            "unattend", "File cài đặt", "File trả lời tự động",
            if ok { "Có autounattend.xml ở thư mục gốc" } else { "Không tìm thấy" },
            "Bạn đã bật bỏ qua màn hình hỏi đáp nên file này phải nằm ở gốc ổ",
            if ok { CheckLevel::Pass } else { CheckLevel::Fail },
            // Thiếu file này thì Setup vẫn chạy, chỉ là hỏi lại từng bước — làm
            // hỏng trải nghiệm chứ không làm hỏng việc khởi động.
            (!ok).then_some("USB vẫn khởi động và cài được, chỉ là bạn phải tự trả lời các màn hình ban đầu."),
            false,
        ));
    }

    out.push(check(
        "content", "File cài đặt", "Tổng lượng đã chép",
        if s.file_count == 0 {
            "Không đọc được".to_string()
        } else {
            format!("{} file · {}", s.file_count, human(s.total_bytes))
        },
        "Bộ cài Windows đầy đủ có hàng trăm file",
        if s.file_count == 0 {
            CheckLevel::Skipped
        } else if s.file_count < 50 {
            CheckLevel::Fail
        } else {
            CheckLevel::Pass
        },
        (s.file_count > 0 && s.file_count < 50).then_some(
            "Số file quá ít so với một bộ cài hoàn chỉnh — nhiều khả năng quá trình chép bị đứt giữa chừng."
        ),
        true,
    ));

    summarise(out, uefi_ok, legacy_ok)
}

// ---------------------------------------------------------------------------
// Luồng Linux: ảnh đĩa ghi nguyên khối
// ---------------------------------------------------------------------------

/// Số byte đầu ổ cần đọc để kiểm tra. Đủ chứa cả sector 17 của ISO9660
/// (offset 0x8800) lẫn phần đuôi của nó.
pub const HEAD_BYTES: usize = 64 * 1024;

const ISO_PVD: usize = 0x8000; // sector 16 — Primary Volume Descriptor
const ISO_BOOT_RECORD: usize = 0x8800; // sector 17 — bản ghi El Torito
const MBR_PART_TABLE: usize = 0x1BE;
const MBR_SIG: usize = 0x1FE;
const GPT_HEADER: usize = 0x200; // LBA 1

#[derive(Debug, Clone, Copy)]
struct MbrEntry {
    active: bool,
    kind: u8,
}

fn mbr_entries(head: &[u8]) -> Vec<MbrEntry> {
    (0..4)
        .filter_map(|i| {
            let at = MBR_PART_TABLE + i * 16;
            let e = head.get(at..at + 16)?;
            (e[4] != 0).then(|| MbrEntry { active: e[0] == 0x80, kind: e[4] })
        })
        .collect()
}

/// Nhãn volume của ảnh ISO, nằm ở offset 0x8028 và được đệm bằng dấu cách.
fn iso_label(head: &[u8]) -> Option<String> {
    let raw = head.get(ISO_PVD + 40..ISO_PVD + 72)?;
    let s: String = raw
        .iter()
        .map(|&b| if (0x20..0x7f).contains(&b) { b as char } else { ' ' })
        .collect();
    let s = s.trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn has_at(head: &[u8], at: usize, want: &[u8]) -> bool {
    head.get(at..at + want.len()).map(|s| s == want).unwrap_or(false)
}

/// Chấm một chiếc USB đã ghi nguyên khối. Hàm thuần — chỉ đọc mảng byte.
pub fn evaluate_linux(head: &[u8], disk_size: u64, iso_size: u64) -> BootReport {
    let mut out: Vec<BootCheck> = Vec::new();

    // --- Ảnh đĩa có nằm đúng chỗ không -----------------------------------
    //
    // Đây là mục quan trọng nhất và cũng là mục rẻ nhất. Chữ ký ISO9660 phải
    // nằm đúng offset 0x8000 tính từ **byte đầu của ổ**. Lệch một chút nghĩa là
    // ảnh đã bị ghi vào một phân vùng thay vì vào cả thiết bị — lỗi kinh điển
    // khi dùng nhầm công cụ, và triệu chứng của nó là máy lặng lẽ bỏ qua USB.
    let iso_ok = has_at(head, ISO_PVD, &[0x01]) && has_at(head, ISO_PVD + 1, b"CD001");
    let label = iso_label(head);
    out.push(check(
        "iso9660", "Ảnh đĩa", "Chữ ký ISO9660",
        if iso_ok {
            match &label {
                Some(l) => format!("Đúng vị trí · nhãn \"{l}\""),
                None => "Đúng vị trí".to_string(),
            }
        } else {
            "Không tìm thấy ở offset 0x8000".to_string()
        },
        "Ảnh đĩa phải bắt đầu từ byte đầu tiên của ổ",
        if iso_ok { CheckLevel::Pass } else { CheckLevel::Fail },
        (!iso_ok).then_some(
            "Ảnh đĩa không nằm đúng chỗ — nhiều khả năng nó đã được ghi vào một phân vùng \
             thay vì vào cả thiết bị. Hãy ghi lại từ bước trước."
        ),
        true,
    ));

    // --- Bảng phân vùng ---------------------------------------------------
    let mbr_sig = has_at(head, MBR_SIG, &[0x55, 0xAA]);
    let gpt = has_at(head, GPT_HEADER, b"EFI PART");
    let parts = mbr_entries(head);
    // Hybrid ISO mang sẵn mã khởi động trong 446 byte đầu; toàn số 0 nghĩa là
    // BIOS đời cũ không có gì để chạy.
    let bootstrap = head.get(..MBR_PART_TABLE).map(|b| b.iter().any(|&x| x != 0)).unwrap_or(false);

    out.push(check(
        "part_table", "Bảng phân vùng", "Bảng phân vùng trên ổ",
        match (mbr_sig, gpt) {
            (true, true) => format!("MBR + GPT lai · {} phân vùng", parts.len()),
            (true, false) => format!("MBR · {} phân vùng", parts.len()),
            (false, true) => "Chỉ có GPT".to_string(),
            (false, false) => "Không tìm thấy".to_string(),
        },
        "Firmware cần một bảng phân vùng hợp lệ để nhận ra ổ khởi động được",
        if mbr_sig || gpt { CheckLevel::Pass } else { CheckLevel::Fail },
        (!mbr_sig && !gpt).then_some(
            "Không có chữ ký bảng phân vùng nào ở đầu ổ. Ảnh đĩa có thể chưa được ghi trọn vẹn."
        ),
        true,
    ));

    // Phân vùng EFI (kiểu 0xEF) là thứ máy đời mới tìm. Hybrid ISO thường có
    // một phân vùng như vậy; bản chỉ có GPT thì ESP nằm trong bảng GPT.
    let esp = parts.iter().any(|p| p.kind == 0xEF);
    let uefi_ok = esp || gpt;
    out.push(check(
        "efi_part", "Bảng phân vùng", "Phân vùng khởi động UEFI",
        if esp {
            "Có phân vùng EFI (kiểu 0xEF)".to_string()
        } else if gpt {
            "Có bảng GPT chứa phân vùng khởi động".to_string()
        } else {
            "Không tìm thấy".to_string()
        },
        "Máy UEFI đời mới cần một phân vùng EFI trên ổ",
        if uefi_ok { CheckLevel::Pass } else { CheckLevel::Warn },
        (!uefi_ok).then_some(
            "Ảnh đĩa này có vẻ chỉ khởi động được ở chế độ BIOS/CSM. Nếu máy đích chỉ hỗ trợ \
             UEFI thì hãy chọn một bản phân phối khác."
        ),
        false,
    ));

    let legacy_ok = mbr_sig && bootstrap;
    out.push(check(
        "legacy_boot", "Bảng phân vùng", "Mã khởi động BIOS đời cũ",
        match (bootstrap, parts.iter().any(|p| p.active)) {
            (true, true) => "Có mã khởi động và phân vùng active",
            (true, false) => "Có mã khởi động",
            (false, _) => "Vùng mã khởi động trống",
        },
        "Máy BIOS cũ nạp 446 byte đầu ổ rồi chạy đoạn mã trong đó",
        if legacy_ok { CheckLevel::Pass } else { CheckLevel::Warn },
        (!legacy_ok).then_some("Ổ này chỉ khởi động được ở chế độ UEFI."),
        false,
    ));

    // El Torito là bản ghi khởi động dành cho đĩa quang. Nó không quyết định
    // việc boot từ USB, nhưng có mặt là dấu hiệu ảnh đĩa còn nguyên vẹn.
    let el_torito = has_at(head, ISO_BOOT_RECORD, &[0x00])
        && has_at(head, ISO_BOOT_RECORD + 1, b"CD001")
        && has_at(head, ISO_BOOT_RECORD + 7, b"EL TORITO SPECIFICATION");
    out.push(check(
        "el_torito", "Ảnh đĩa", "Bản ghi khởi động El Torito",
        if el_torito { "Có" } else { "Không có" },
        "Có mặt trong hầu hết ISO khởi động được (không bắt buộc khi boot từ USB)",
        if el_torito { CheckLevel::Pass } else { CheckLevel::Skipped },
        None,
        false,
    ));

    // --- Dung lượng -------------------------------------------------------
    let fits = disk_size == 0 || iso_size == 0 || disk_size >= iso_size;
    out.push(check(
        "capacity", "Dung lượng", "Ổ chứa đủ ảnh đĩa",
        if disk_size == 0 || iso_size == 0 {
            "Không đọc được".to_string()
        } else {
            format!("Ổ {} · ảnh đĩa {}", human(disk_size), human(iso_size))
        },
        "Ghi nguyên khối cần ổ lớn hơn hoặc bằng file ảnh",
        if disk_size == 0 || iso_size == 0 {
            CheckLevel::Skipped
        } else if fits {
            CheckLevel::Pass
        } else {
            CheckLevel::Fail
        },
        (!fits).then_some("Ảnh đĩa lớn hơn ổ nên phần đuôi đã bị cắt mất."),
        true,
    ));

    summarise(out, uefi_ok, legacy_ok)
}

// ---------------------------------------------------------------------------
// Thu thập dữ liệu từ ổ thật
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootCheckRequest {
    pub disk_number: u32,
    /// "windows" hoặc "linux".
    pub family: String,
    pub iso_path: String,
    /// Nhãn đã đặt lúc format — dùng để tìm đúng phân vùng khởi động.
    pub label: String,
    /// Người dùng có bật cài đặt tự động không (chỉ có nghĩa với Windows).
    pub expect_unattend: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadbackResult {
    pub matched: bool,
    /// Số file đã đối chiếu (Windows) hoặc số byte đã đọc lại (Linux).
    pub compared: u64,
    pub mismatched: Vec<String>,
    pub missing: Vec<String>,
    pub expected_sha: Option<String>,
    pub actual_sha: Option<String>,
    pub message: String,
}

fn escape(s: &str) -> String {
    s.replace('\'', "''")
}

/// Ổ đích vẫn phải là đúng chiếc USB đã ghi. Không kiểm tra lại thì một cái rút
/// ra cắm lại là đang đọc nhầm ổ khác mà báo kết quả cho ổ này.
async fn locate(disk_number: u32) -> Result<usb::UsbDisk> {
    usb::list()
        .await?
        .into_iter()
        .find(|d| d.number == disk_number)
        .ok_or_else(|| {
            AppError::new("disk_gone", "Không còn thấy ổ USB vừa ghi. Hãy cắm lại rồi kiểm tra lại.")
        })
}

/// Kiểm tra cấu trúc — vài giây, không đọc lại dữ liệu.
pub async fn check_boot(req: BootCheckRequest) -> Result<BootReport> {
    let disk = locate(req.disk_number).await?;

    if req.family == "linux" {
        let iso_size = std::fs::metadata(&req.iso_path).map(|m| m.len()).unwrap_or(0);
        let b64 = ps::run(
            &SCRIPT_READ_HEAD
                .replace("%%DISK%%", &req.disk_number.to_string())
                .replace("%%LEN%%", &HEAD_BYTES.to_string()),
        )
        .await?;

        use base64::Engine;
        let head = base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .map_err(|e| AppError::new("head_decode", format!("Không đọc được đầu ổ USB: {e}")))?;

        return Ok(evaluate_linux(&head, disk.size, iso_size));
    }

    let probes = WINDOWS_PROBES
        .iter()
        .map(|p| format!("'{}'", escape(p)))
        .collect::<Vec<_>>()
        .join(",");

    let snap: WindowsUsbSnapshot = ps::run_json(
        &SCRIPT_WINDOWS_SNAPSHOT
            .replace("%%DISK%%", &req.disk_number.to_string())
            .replace("%%LABEL%%", &escape(&req.label))
            .replace("%%PROBES%%", &probes),
    )
    .await?;

    Ok(evaluate_windows(&snap, req.expect_unattend))
}

/// Đọc lại toàn bộ dữ liệu vừa ghi và đối chiếu.
///
/// Đây là bước duy nhất phát hiện được USB khai khống dung lượng và flash sắp
/// chết: cả hai loại đều nhận hết dữ liệu lúc ghi rồi âm thầm vứt đi, nên mọi
/// kiểm tra cấu trúc đều báo xanh.
pub async fn readback<F>(req: BootCheckRequest, mut on_progress: F) -> Result<ReadbackResult>
where
    F: FnMut(writer::WriteProgress) + Send,
{
    let _ = locate(req.disk_number).await?;

    // Cùng cách làm với bước ghi: chặng nào đếm được byte thì đặt số liệu vào
    // ô nhớ này ngay trước khi gọi `emit`, chặng nào không thì để trống.
    let tp = crate::rate::Slot::default();
    let mut emit = |pct: f64, msg: String, detail: Option<String>| {
        on_progress(writer::WriteProgress {
            stage: "readback".into(),
            stage_index: 1,
            total_stages: 1,
            percent: pct,
            message: msg,
            detail,
            rate: tp.take(),
        });
    };

    if req.family == "linux" {
        return readback_raw(&req, &mut emit, &tp).await;
    }
    // Đối chiếu theo file thì đơn vị là số file chứ không phải byte, nên không
    // có tốc độ nào để báo.
    readback_files(&req, &mut emit).await
}

/// Luồng Linux: băm lại đúng số byte đã ghi rồi so với mã băm của file ảnh.
async fn readback_raw<F>(
    req: &BootCheckRequest,
    emit: &mut F,
    tp: &crate::rate::Slot,
) -> Result<ReadbackResult>
where
    F: FnMut(f64, String, Option<String>) + Send,
{
    let iso_size = std::fs::metadata(&req.iso_path)?.len();
    if iso_size == 0 {
        return Err(AppError::new("no_iso", "Không đọc được file ảnh đĩa để đối chiếu."));
    }

    emit(0.0, "Đang đọc lại dữ liệu trên USB…".into(), None);

    let script = SCRIPT_READBACK_RAW
        .replace("%%DISK%%", &req.disk_number.to_string())
        .replace("%%LEN%%", &iso_size.to_string());

    let mut actual: Option<String> = None;
    let mut last = std::time::Instant::now();
    let mut rate = crate::rate::Rate::new();
    ps::run_streaming(&script, |line| {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("GWU:HASH ") {
            actual = Some(rest.trim().to_lowercase());
        } else if let Some(rest) = line.strip_prefix("GWU:READ ") {
            let done: u64 = rest.trim().parse().unwrap_or(0);
            if last.elapsed().as_millis() >= 150 {
                let now = std::time::Instant::now();
                last = now;
                tp.set(rate.sample(now, done, iso_size));
                emit(
                    done as f64 / iso_size as f64 * 100.0,
                    format!("Đang đọc lại · {} / {}", human(done), human(iso_size)),
                    None,
                );
            }
        }
    })
    .await?;

    let actual = actual.ok_or_else(|| {
        AppError::new("readback_failed", "Đọc lại được dữ liệu nhưng không tính được mã băm.")
    })?;

    emit(100.0, "Đang tính mã băm của file ảnh gốc…".into(), None);
    let expected = crate::download::sha256(std::path::Path::new(&req.iso_path), |_| {}).await?;
    let expected = expected.to_lowercase();

    let matched = actual == expected;
    Ok(ReadbackResult {
        matched,
        compared: iso_size,
        mismatched: Vec::new(),
        missing: Vec::new(),
        expected_sha: Some(expected),
        actual_sha: Some(actual),
        message: if matched {
            format!("Đã đọc lại {} và đối chiếu khớp từng byte với file ảnh gốc.", human(iso_size))
        } else {
            "Dữ liệu đọc lại từ USB khác với file ảnh gốc. Ổ này không dùng được — \
             thường là do USB khai khống dung lượng hoặc bộ nhớ flash đã hỏng."
                .to_string()
        },
    })
}

/// Luồng Windows: đối chiếu từng file giữa ảnh ISO gắn tạm và bản trên USB.
async fn readback_files<F>(req: &BootCheckRequest, emit: &mut F) -> Result<ReadbackResult>
where
    F: FnMut(f64, String, Option<String>) + Send,
{
    emit(0.0, "Đang gắn file ISO để đối chiếu…".into(), None);

    let script = SCRIPT_READBACK_FILES
        .replace("%%ISO%%", &escape(&req.iso_path))
        .replace("%%DISK%%", &req.disk_number.to_string())
        .replace("%%LABEL%%", &escape(&req.label));

    let mut compared: u64 = 0;
    let mut mismatched: Vec<String> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    let mut last = std::time::Instant::now();

    ps::run_streaming(&script, |line| {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("GWU:DIFF ") {
            // Chỉ giữ vài cái tên đầu: danh sách dài không giúp gì thêm mà lại
            // đẩy một khối văn bản khổng lồ lên giao diện.
            if mismatched.len() < 12 {
                mismatched.push(rest.to_string());
            }
        } else if let Some(rest) = line.strip_prefix("GWU:MISSING ") {
            if missing.len() < 12 {
                missing.push(rest.to_string());
            }
        } else if let Some(rest) = line.strip_prefix("GWU:CMP ") {
            let mut parts = rest.splitn(3, ' ');
            let done: u64 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            let total: u64 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(1);
            let name = parts.next().unwrap_or("").to_string();
            compared = done;
            if last.elapsed().as_millis() >= 150 {
                last = std::time::Instant::now();
                emit(
                    done as f64 / total.max(1) as f64 * 100.0,
                    format!("Đang đối chiếu · {done} / {total} file"),
                    Some(name),
                );
            }
        }
    })
    .await?;

    let matched = mismatched.is_empty() && missing.is_empty();
    Ok(ReadbackResult {
        matched,
        compared,
        message: if matched {
            format!("Đã đối chiếu {compared} file giữa ảnh ISO và bản trên USB — khớp toàn bộ.")
        } else {
            format!(
                "{} file khác nội dung và {} file thiếu so với ảnh ISO. USB này chưa dùng được.",
                mismatched.len(),
                missing.len()
            )
        },
        mismatched,
        missing,
        expected_sha: None,
        actual_sha: None,
    })
}

// ---------------------------------------------------------------------------
// Script PowerShell
// ---------------------------------------------------------------------------

/// Đọc `%%LEN%%` byte đầu ổ và trả về dạng base64.
///
/// Đọc thô nên phải mở ổ ở chế độ chia sẻ đầy đủ — sau khi ghi nguyên khối,
/// Windows vẫn đang giữ handle trên ổ để dò phân vùng.
const SCRIPT_READ_HEAD: &str = r#"
$ErrorActionPreference = 'Stop'
$n = %%DISK%%
$len = %%LEN%%
$fs = $null
try {
  $fs = New-Object System.IO.FileStream(
    "\\.\PHYSICALDRIVE$n",
    [System.IO.FileMode]::Open,
    [System.IO.FileAccess]::Read,
    [System.IO.FileShare]::ReadWrite)
  $buf = New-Object byte[] $len
  $got = 0
  while ($got -lt $len) {
    $r = $fs.Read($buf, $got, $len - $got)
    if ($r -le 0) { break }
    $got += $r
  }
  [Convert]::ToBase64String($buf, 0, $got)
}
finally { if ($fs) { $fs.Dispose() } }
"#;

/// Thu thập trạng thái phân vùng khởi động của ổ cài Windows.
///
/// Chỉ đếm và đo, không kết luận gì — mọi phán xét nằm ở `evaluate_windows`.
const SCRIPT_WINDOWS_SNAPSHOT: &str = r#"
$ErrorActionPreference = 'Stop'
$n = %%DISK%%
$label = '%%LABEL%%'
$probes = @(%%PROBES%%)

$disk = Get-Disk -Number $n -ErrorAction Stop
$parts = @(Get-Partition -DiskNumber $n -ErrorAction SilentlyContinue)

# Ưu tiên phân vùng đúng nhãn: ổ FAT32 lớn còn có thêm phân vùng DATA.
$target = $null
foreach ($p in $parts) {
  $v = $null
  try { $v = Get-Volume -Partition $p -ErrorAction SilentlyContinue } catch {}
  if ($v -and $v.FileSystemLabel -eq $label) { $target = $p; break }
}
if (-not $target) {
  foreach ($p in $parts) {
    $v = $null
    try { $v = Get-Volume -Partition $p -ErrorAction SilentlyContinue } catch {}
    if ($v -and $v.DriveLetter) { $target = $p; break }
  }
}
if (-not $target) { throw 'Không tìm thấy phân vùng nào có ký tự ổ đĩa trên USB.' }

$vol = Get-Volume -Partition $target -ErrorAction SilentlyContinue
$letter = [string]$vol.DriveLetter
$root = "${letter}:\"

$files = @()
foreach ($rel in $probes) {
  $full = Join-Path $root $rel
  $item = $null
  try { $item = Get-Item -LiteralPath $full -Force -ErrorAction SilentlyContinue } catch {}
  $files += [pscustomobject]@{
    path   = [string]$rel
    exists = [bool]($item -ne $null)
    size   = [uint64]$(if ($item -and -not $item.PSIsContainer) { $item.Length } else { 0 })
  }
}

$count = 0
$total = [uint64]0
$maxLen = [uint64]0
$maxPath = ''
foreach ($f in (Get-ChildItem -LiteralPath $root -Recurse -File -Force -ErrorAction SilentlyContinue)) {
  $count++
  $total += [uint64]$f.Length
  if ([uint64]$f.Length -gt $maxLen) {
    $maxLen = [uint64]$f.Length
    $maxPath = $f.FullName.Substring($root.Length)
  }
}

$out = [pscustomobject]@{
  partition_style = [string]$disk.PartitionStyle
  filesystem      = [string]$vol.FileSystem
  drive_letter    = $letter
  volume_label    = [string]$vol.FileSystemLabel
  is_active       = [bool]$(if ($target.PSObject.Properties['IsActive']) { $target.IsActive } else { $false })
  files           = $files
  file_count      = [uint64]$count
  total_bytes     = $total
  largest_path    = $maxPath
  largest_bytes   = $maxLen
}
ConvertTo-Json -InputObject $out -Depth 4 -Compress
"#;

/// Đọc lại `%%LEN%%` byte đầu ổ rồi băm SHA-256 theo dòng chảy.
///
/// Không nạp cả file vào bộ nhớ: ảnh đĩa thường vài GB, mà cách này chỉ giữ một
/// khối 4 MB tại một thời điểm.
const SCRIPT_READBACK_RAW: &str = r#"
$ErrorActionPreference = 'Stop'
$n = %%DISK%%
$len = [uint64]%%LEN%%

$fs = $null
$sha = $null
try {
  $fs = New-Object System.IO.FileStream(
    "\\.\PHYSICALDRIVE$n",
    [System.IO.FileMode]::Open,
    [System.IO.FileAccess]::Read,
    [System.IO.FileShare]::ReadWrite)
  $sha = [System.Security.Cryptography.SHA256]::Create()

  $size = 4194304
  $buf = New-Object byte[] $size
  $done = [uint64]0
  $mark = [uint64]0

  while ($done -lt $len) {
    $want = [int][math]::Min([uint64]$size, $len - $done)
    $r = $fs.Read($buf, 0, $want)
    if ($r -le 0) { break }
    [void]$sha.TransformBlock($buf, 0, $r, $null, 0)
    $done += [uint64]$r
    if (($done - $mark) -ge 16777216) {
      Write-Output "GWU:READ $done"
      $mark = $done
    }
  }
  [void]$sha.TransformFinalBlock((New-Object byte[] 0), 0, 0)
  Write-Output "GWU:READ $done"

  if ($done -lt $len) {
    throw "Chỉ đọc lại được $done trên $len byte — ổ USB báo hết dung lượng sớm hơn dung lượng nó khai."
  }
  $hex = ($sha.Hash | ForEach-Object { $_.ToString('x2') }) -join ''
  Write-Output "GWU:HASH $hex"
}
finally {
  if ($sha) { $sha.Dispose() }
  if ($fs) { $fs.Dispose() }
}
"#;

/// Gắn lại ảnh ISO rồi đối chiếu SHA-256 từng file với bản trên USB.
///
/// Bỏ qua `install.wim` khi nó đã được tách thành `.swm` — hai bên khi đó không
/// còn là cùng một file nữa, và so chúng sẽ luôn báo sai.
const SCRIPT_READBACK_FILES: &str = r#"
$ErrorActionPreference = 'Stop'
$iso = '%%ISO%%'
$n = %%DISK%%
$label = '%%LABEL%%'

$parts = @(Get-Partition -DiskNumber $n -ErrorAction SilentlyContinue)
$target = $null
foreach ($p in $parts) {
  $v = $null
  try { $v = Get-Volume -Partition $p -ErrorAction SilentlyContinue } catch {}
  if ($v -and $v.FileSystemLabel -eq $label) { $target = $p; break }
}
if (-not $target) {
  foreach ($p in $parts) {
    $v = $null
    try { $v = Get-Volume -Partition $p -ErrorAction SilentlyContinue } catch {}
    if ($v -and $v.DriveLetter) { $target = $p; break }
  }
}
if (-not $target) { throw 'Không tìm thấy phân vùng khởi động trên USB.' }
$dstRoot = ([string](Get-Volume -Partition $target).DriveLetter) + ':\'

$img = Mount-DiskImage -ImagePath $iso -PassThru -ErrorAction Stop
try {
  $srcLetter = ($img | Get-Volume).DriveLetter
  if (-not $srcLetter) { throw 'Không gắn được file ISO để đối chiếu.' }
  $srcRoot = "${srcLetter}:\"

  # USB dùng install.swm thì bản .wim trong ISO không có đối ứng để so.
  $split = Test-Path -LiteralPath (Join-Path $dstRoot 'sources\install.swm')

  $all = @(Get-ChildItem -LiteralPath $srcRoot -Recurse -File -Force -ErrorAction SilentlyContinue)
  $total = $all.Count
  $i = 0
  foreach ($f in $all) {
    $i++
    $rel = $f.FullName.Substring($srcRoot.Length)
    Write-Output "GWU:CMP $i $total $rel"

    if ($split -and ($rel -ieq 'sources\install.wim' -or $rel -ieq 'sources\install.esd')) { continue }

    $dst = Join-Path $dstRoot $rel
    if (-not (Test-Path -LiteralPath $dst)) { Write-Output "GWU:MISSING $rel"; continue }

    $d = Get-Item -LiteralPath $dst -Force
    if ($d.Length -ne $f.Length) { Write-Output "GWU:DIFF $rel"; continue }

    $a = (Get-FileHash -LiteralPath $f.FullName -Algorithm SHA256).Hash
    $b = (Get-FileHash -LiteralPath $dst -Algorithm SHA256).Hash
    if ($a -ne $b) { Write-Output "GWU:DIFF $rel" }
  }
}
finally {
  Dismount-DiskImage -ImagePath $iso -ErrorAction SilentlyContinue | Out-Null
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    // --- Windows ----------------------------------------------------------

    fn f(path: &str, size: u64) -> ProbedFile {
        ProbedFile { path: path.into(), exists: size > 0, size }
    }

    /// Một chiếc USB cài Windows hoàn chỉnh: GPT + FAT32, đủ cả hai đường khởi động.
    fn good_windows() -> WindowsUsbSnapshot {
        WindowsUsbSnapshot {
            partition_style: "GPT".into(),
            filesystem: "FAT32".into(),
            drive_letter: "E".into(),
            volume_label: "WINSETUP".into(),
            is_active: false,
            files: vec![
                f("bootmgr", 430_000),
                f("boot\\bcd", 32_768),
                f("efi\\boot\\bootx64.efi", 1_500_000),
                f("efi\\boot\\bootia32.efi", 0),
                f("efi\\microsoft\\boot\\bcd", 32_768),
                f("sources\\boot.wim", 520_000_000),
                f("sources\\install.wim", 0),
                f("sources\\install.esd", 0),
                f("sources\\install.swm", 3_800_000_000),
                f("autounattend.xml", 2_400),
            ],
            file_count: 1_042,
            total_bytes: 6_100_000_000,
            largest_path: "sources\\install.swm".into(),
            largest_bytes: 3_800_000_000,
        }
    }

    fn find<'a>(r: &'a BootReport, id: &str) -> &'a BootCheck {
        r.checks.iter().find(|c| c.id == id).unwrap_or_else(|| panic!("thiếu mục {id}"))
    }

    #[test]
    fn a_complete_windows_usb_is_ready() {
        let r = evaluate_windows(&good_windows(), true);
        assert_eq!(r.verdict, BootVerdict::Ready, "{}", r.summary);
        assert!(r.bootable_uefi && r.bootable_legacy);
        assert_eq!(r.failed, 0);
        assert_eq!(r.warned, 0);
    }

    #[test]
    fn every_check_has_a_unique_id_and_a_stated_expectation() {
        let r = evaluate_windows(&good_windows(), true);
        let mut ids: Vec<&str> = r.checks.iter().map(|c| c.id.as_str()).collect();
        let before = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(before, ids.len(), "id kiểm tra bị trùng");
        for c in &r.checks {
            assert!(!c.expectation.is_empty(), "{} không nói rõ điều kiện", c.id);
            assert!(!c.value.is_empty(), "{} không nói rõ đọc được gì", c.id);
        }
    }

    /// Thiếu một đường khởi động không phải là hỏng: máy UEFI vẫn chạy được ổ
    /// chỉ có mã UEFI. Báo "không boot được" ở đây sẽ khiến người dùng ghi lại
    /// một chiếc USB vốn đã dùng tốt.
    #[test]
    fn missing_legacy_boot_still_leaves_a_usable_uefi_stick() {
        let mut s = good_windows();
        s.files.retain(|x| x.path != "bootmgr");

        let r = evaluate_windows(&s, false);
        assert!(r.bootable_uefi);
        assert!(!r.bootable_legacy);
        assert_eq!(r.verdict, BootVerdict::ReadyWithWarnings, "{}", r.summary);
        assert_eq!(find(&r, "legacy_loader").level, CheckLevel::Warn);
    }

    #[test]
    fn missing_both_boot_paths_is_not_bootable() {
        let mut s = good_windows();
        s.files.retain(|x| !["bootmgr", "efi\\boot\\bootx64.efi"].contains(&x.path.as_str()));

        let r = evaluate_windows(&s, false);
        assert_eq!(r.verdict, BootVerdict::NotBootable);
        assert!(!r.bootable_uefi && !r.bootable_legacy);
    }

    #[test]
    fn a_missing_boot_wim_is_fatal() {
        let mut s = good_windows();
        s.files.retain(|x| x.path != "sources\\boot.wim");

        let r = evaluate_windows(&s, false);
        assert_eq!(r.verdict, BootVerdict::NotBootable);
        assert_eq!(find(&r, "boot_wim").level, CheckLevel::Fail);
        assert!(find(&r, "boot_wim").blocking);
    }

    #[test]
    fn a_missing_install_image_is_fatal() {
        let mut s = good_windows();
        s.files.retain(|x| !x.path.starts_with("sources\\install"));

        let r = evaluate_windows(&s, false);
        assert_eq!(r.verdict, BootVerdict::NotBootable);
        assert_eq!(find(&r, "install_image").level, CheckLevel::Fail);
    }

    /// File 4 GB trên FAT32 là lỗi âm thầm: quá trình chép báo xong, Setup chạy
    /// tới giữa chừng mới hỏng.
    /// Thiếu file cài đặt thì máy **vẫn** boot vào Windows Setup rồi mới hỏng.
    /// Nói "máy sẽ không thấy USB" ở đây là mô tả sai triệu chứng, và người dùng
    /// sẽ đi tìm nhầm nguyên nhân — trong khi chính bảng bên dưới đang báo cả
    /// hai đường khởi động đều tốt.
    #[test]
    fn a_bootable_stick_with_broken_content_is_described_as_such() {
        let mut s = good_windows();
        s.files.retain(|x| !x.path.starts_with("sources\\install"));

        let r = evaluate_windows(&s, false);
        assert_eq!(r.verdict, BootVerdict::NotBootable);
        // Mã khởi động vẫn còn nguyên, nên không được phủ nhận điều đó.
        assert!(r.bootable_uefi && r.bootable_legacy);
        assert!(
            !r.summary.contains("không thấy USB"),
            "kết luận mâu thuẫn với bảng đường khởi động: {}", r.summary
        );
        assert!(r.summary.contains("dừng giữa chừng"), "{}", r.summary);
    }

    /// Ngược lại: không có mã khởi động nào thì đúng là máy sẽ bỏ qua USB.
    #[test]
    fn a_stick_with_no_boot_path_says_the_machine_will_not_see_it() {
        let mut s = good_windows();
        s.files.retain(|x| !["bootmgr", "efi\\boot\\bootx64.efi"].contains(&x.path.as_str()));

        let r = evaluate_windows(&s, false);
        assert!(r.summary.contains("không thấy USB"), "{}", r.summary);
    }

    #[test]
    fn a_file_over_the_fat32_limit_is_caught() {
        let mut s = good_windows();
        s.largest_path = "sources\\install.wim".into();
        s.largest_bytes = 5 * 1024 * 1024 * 1024;

        let r = evaluate_windows(&s, false);
        assert_eq!(find(&r, "fat32_limit").level, CheckLevel::Fail);
        assert_eq!(r.verdict, BootVerdict::NotBootable);
    }

    /// …nhưng cùng dung lượng đó trên NTFS thì hoàn toàn bình thường.
    #[test]
    fn the_same_large_file_is_fine_on_ntfs() {
        let mut s = good_windows();
        s.partition_style = "MBR".into();
        s.filesystem = "NTFS".into();
        s.is_active = true;
        s.largest_bytes = 5 * 1024 * 1024 * 1024;

        let r = evaluate_windows(&s, false);
        assert_eq!(find(&r, "fat32_limit").level, CheckLevel::Pass);
    }

    /// Firmware UEFI hầu như chỉ đọc được FAT32, nên bootx64.efi nằm trên NTFS
    /// là nằm ở nơi firmware không với tới.
    #[test]
    fn a_uefi_loader_on_ntfs_does_not_count_as_uefi_bootable() {
        let mut s = good_windows();
        s.partition_style = "MBR".into();
        s.filesystem = "NTFS".into();
        s.is_active = true;

        let r = evaluate_windows(&s, false);
        assert!(!r.bootable_uefi, "NTFS không được tính là khởi động UEFI được");
        assert!(r.bootable_legacy);
        assert_eq!(find(&r, "uefi_loader").level, CheckLevel::Warn);
        assert_eq!(r.verdict, BootVerdict::ReadyWithWarnings);
    }

    #[test]
    fn an_mbr_disk_without_an_active_partition_cannot_boot() {
        let mut s = good_windows();
        s.partition_style = "MBR".into();
        s.is_active = false;

        let r = evaluate_windows(&s, false);
        assert_eq!(find(&r, "active").level, CheckLevel::Fail);
        assert_eq!(r.verdict, BootVerdict::NotBootable);
    }

    /// Ổ GPT không có khái niệm phân vùng active — không được bịa ra một mục
    /// kiểm tra rồi đánh trượt nó.
    #[test]
    fn a_gpt_disk_is_not_asked_about_the_active_flag() {
        let r = evaluate_windows(&good_windows(), true);
        assert!(r.checks.iter().all(|c| c.id != "active"));
    }

    /// Thiếu file trả lời tự động làm hỏng trải nghiệm, không làm hỏng khởi động.
    #[test]
    fn a_missing_answer_file_never_makes_the_stick_unbootable() {
        let mut s = good_windows();
        s.files.retain(|x| x.path != "autounattend.xml");

        let r = evaluate_windows(&s, true);
        assert_eq!(find(&r, "unattend").level, CheckLevel::Fail);
        assert!(!find(&r, "unattend").blocking);
        assert_eq!(r.verdict, BootVerdict::ReadyWithWarnings, "{}", r.summary);
    }

    #[test]
    fn the_answer_file_is_not_checked_when_it_was_never_requested() {
        let mut s = good_windows();
        s.files.retain(|x| x.path != "autounattend.xml");
        let r = evaluate_windows(&s, false);
        assert!(r.checks.iter().all(|c| c.id != "unattend"));
    }

    #[test]
    fn a_half_copied_stick_is_caught_by_the_file_count() {
        let mut s = good_windows();
        s.file_count = 9;
        s.total_bytes = 4_000_000;

        let r = evaluate_windows(&s, false);
        assert_eq!(find(&r, "content").level, CheckLevel::Fail);
        assert_eq!(r.verdict, BootVerdict::NotBootable);
    }

    /// Không đọc được là **không đọc được**, không phải là không đạt — cùng
    /// nguyên tắc mà phần quét phần cứng đã theo.
    #[test]
    fn unreadable_values_are_skipped_never_failed() {
        let mut s = good_windows();
        s.partition_style = String::new();
        s.file_count = 0;
        s.total_bytes = 0;
        s.largest_bytes = 0;

        let r = evaluate_windows(&s, false);
        for id in ["layout", "content", "fat32_limit"] {
            assert_eq!(find(&r, id).level, CheckLevel::Skipped, "{id} phải là xám");
        }
        assert_ne!(r.verdict, BootVerdict::NotBootable, "không đọc được thì không kết luận hỏng");
    }

    // --- Linux ------------------------------------------------------------

    /// Dựng phần đầu của một hybrid ISO đúng chuẩn.
    fn hybrid_head(label: &str) -> Vec<u8> {
        let mut h = vec![0u8; HEAD_BYTES];

        // Mã khởi động MBR (chỉ cần khác 0) và chữ ký 0x55AA.
        h[0..64].fill(0xEB);
        h[MBR_SIG] = 0x55;
        h[MBR_SIG + 1] = 0xAA;

        // Phân vùng 1: chính ảnh ISO, có cờ active.
        h[MBR_PART_TABLE] = 0x80;
        h[MBR_PART_TABLE + 4] = 0x17;
        h[MBR_PART_TABLE + 12..MBR_PART_TABLE + 16].copy_from_slice(&5_000_000u32.to_le_bytes());
        // Phân vùng 2: phân vùng EFI.
        h[MBR_PART_TABLE + 16 + 4] = 0xEF;
        h[MBR_PART_TABLE + 16 + 12..MBR_PART_TABLE + 16 + 16].copy_from_slice(&20_000u32.to_le_bytes());

        // Primary Volume Descriptor ở sector 16.
        h[ISO_PVD] = 0x01;
        h[ISO_PVD + 1..ISO_PVD + 6].copy_from_slice(b"CD001");
        let mut name = [b' '; 32];
        name[..label.len()].copy_from_slice(label.as_bytes());
        h[ISO_PVD + 40..ISO_PVD + 72].copy_from_slice(&name);

        // Bản ghi khởi động El Torito ở sector 17.
        h[ISO_BOOT_RECORD] = 0x00;
        h[ISO_BOOT_RECORD + 1..ISO_BOOT_RECORD + 6].copy_from_slice(b"CD001");
        h[ISO_BOOT_RECORD + 7..ISO_BOOT_RECORD + 7 + 23]
            .copy_from_slice(b"EL TORITO SPECIFICATION");

        h
    }

    const DISK: u64 = 64 * 1024 * 1024 * 1024;
    const ISO: u64 = 3 * 1024 * 1024 * 1024;

    #[test]
    fn a_correctly_written_hybrid_iso_is_ready() {
        let r = evaluate_linux(&hybrid_head("Ubuntu 24.04.3 LTS amd64"), DISK, ISO);
        assert_eq!(r.verdict, BootVerdict::Ready, "{}", r.summary);
        assert!(r.bootable_uefi && r.bootable_legacy);
        assert!(find(&r, "iso9660").value.contains("Ubuntu 24.04.3 LTS amd64"));
    }

    /// Lỗi kinh điển: ảnh được ghi vào một phân vùng chứ không vào cả thiết bị.
    /// Cấu trúc trông vẫn hợp lệ, chỉ có chữ ký ISO9660 là không ở đúng offset.
    #[test]
    fn an_image_written_to_the_wrong_offset_is_caught() {
        let mut h = hybrid_head("Ubuntu");
        h[ISO_PVD..ISO_PVD + 6].fill(0);

        let r = evaluate_linux(&h, DISK, ISO);
        assert_eq!(find(&r, "iso9660").level, CheckLevel::Fail);
        assert_eq!(r.verdict, BootVerdict::NotBootable);
    }

    #[test]
    fn a_blank_disk_is_not_bootable() {
        let r = evaluate_linux(&vec![0u8; HEAD_BYTES], DISK, ISO);
        assert_eq!(r.verdict, BootVerdict::NotBootable);
        assert!(!r.bootable_uefi && !r.bootable_legacy);
    }

    /// Ảnh chỉ dùng GPT vẫn khởi động được trên máy UEFI, chỉ là không boot được
    /// máy BIOS cũ — cảnh báo, không phải lỗi.
    #[test]
    fn a_gpt_only_image_boots_uefi_and_only_warns_about_legacy() {
        let mut h = hybrid_head("Fedora");
        h[..MBR_PART_TABLE].fill(0);
        h[MBR_SIG] = 0;
        h[MBR_SIG + 1] = 0;
        h[MBR_PART_TABLE + 16 + 4] = 0;
        h[GPT_HEADER..GPT_HEADER + 8].copy_from_slice(b"EFI PART");

        let r = evaluate_linux(&h, DISK, ISO);
        assert!(r.bootable_uefi);
        assert!(!r.bootable_legacy);
        assert_eq!(find(&r, "legacy_boot").level, CheckLevel::Warn);
        assert_eq!(r.verdict, BootVerdict::ReadyWithWarnings);
    }

    #[test]
    fn an_image_larger_than_the_disk_is_fatal() {
        let r = evaluate_linux(&hybrid_head("Ubuntu"), 2 * 1024 * 1024 * 1024, 6 * 1024 * 1024 * 1024);
        assert_eq!(find(&r, "capacity").level, CheckLevel::Fail);
        assert_eq!(r.verdict, BootVerdict::NotBootable);
    }

    /// Không có El Torito là chuyện bình thường khi boot từ USB — xám, không đỏ.
    #[test]
    fn a_missing_el_torito_record_is_not_a_failure() {
        let mut h = hybrid_head("Debian");
        h[ISO_BOOT_RECORD..ISO_BOOT_RECORD + 64].fill(0);

        let r = evaluate_linux(&h, DISK, ISO);
        assert_eq!(find(&r, "el_torito").level, CheckLevel::Skipped);
        assert_eq!(r.verdict, BootVerdict::Ready, "{}", r.summary);
    }

    /// Đầu ổ đọc hụt không được làm chương trình hoảng loạn.
    #[test]
    fn a_short_read_does_not_panic() {
        for len in [0usize, 1, 512, 4096, ISO_PVD + 3] {
            let r = evaluate_linux(&vec![0u8; len], DISK, ISO);
            assert_eq!(r.verdict, BootVerdict::NotBootable);
        }
    }
}
