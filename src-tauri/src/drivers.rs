//! Kèm driver vào USB cài Windows.
//!
//! Vấn đề cần giải: cài lại Windows xong thì máy mất Wi-Fi, mà muốn tải driver
//! Wi-Fi thì lại cần Wi-Fi. Vòng luẩn quẩn này chỉ phá được bằng cách đưa driver
//! lên USB **trước khi** cài.
//!
//! Cách làm: Windows Setup tự tìm thư mục tên `$WinPEDriver$` ở gốc ổ đĩa rời và
//! cài mọi driver trong đó vào ảnh Windows đang cài. Không cần sửa bộ cài, không
//! cần biết ổ USB sẽ mang chữ cái nào lúc chạy WinPE — đây là lý do module này
//! chọn đường đó thay vì khai `DriverPaths` trong `autounattend.xml`, vốn phải
//! ghi cứng một đường dẫn tuyệt đối mà không ai đoán trước được.
//!
//! Đánh đổi của `$WinPEDriver$`: Setup cài **tất cả** driver trong thư mục, không
//! cần biết máy có thiết bị đó hay không. Vì vậy phần lọc theo nhóm ở đây không
//! phải để tiết kiệm dung lượng mà là để giảm rủi ro — một driver điều khiển đĩa
//! sai có thể làm máy không khởi động được.

use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Kiểu dữ liệu
// ---------------------------------------------------------------------------

/// Một gói driver = một thư mục có chứa file `.inf`.
///
/// Đơn vị là thư mục chứ không phải từng file `.inf`, vì một gói driver còn có
/// `.sys`, `.cat`, `.dll` nằm cạnh; chép thiếu một trong số đó thì Setup từ chối
/// cài vì chữ ký không khớp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverPackage {
    pub folder: String,
    /// Tên hiển thị — tên thư mục, thứ người dùng thấy trong danh sách.
    pub name: String,
    pub infs: Vec<String>,
    /// Nhóm thiết bị, đã khử trùng lặp (`Net`, `SCSIAdapter`…).
    pub classes: Vec<String>,
    pub provider: String,
    pub version: String,
    pub size: u64,
    /// Mã phần cứng gói này khai là hỗ trợ. Chỉ dùng trong Rust để đối chiếu với
    /// thiết bị thật; danh sách này có thể dài hàng nghìn dòng nên không gửi ra
    /// giao diện.
    #[serde(skip)]
    pub hardware_ids: Vec<String>,
}

/// Kết quả quét một thư mục driver.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverSet {
    pub source: String,
    pub packages: Vec<DriverPackage>,
    pub total_size: u64,
    /// Số thư mục chỉ có file cài đặt (`.exe`, `.msi`) mà không có `.inf` nào.
    ///
    /// Đếm riêng vì đây là hiểu lầm phổ biến nhất: người dùng tải "driver Wi-Fi"
    /// từ trang hãng và nhận về một file `.exe` — thứ Windows Setup không nhồi
    /// vào ảnh cài được. Giao diện phải nói ra thay vì im lặng bỏ qua.
    pub installer_only: u32,
}

/// Một thiết bị thật trên máy, kèm kết luận có driver hay không.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceMatch {
    pub name: String,
    /// `wifi`, `ethernet`, `bluetooth`, hoặc `storage`.
    pub kind: String,
    pub hardware_id: String,
    /// Tên gói driver phủ được thiết bị này; `None` nghĩa là chưa có.
    pub covered_by: Option<String>,
}

/// Thiết bị đọc được từ máy đang chạy, trước khi đối chiếu.
#[derive(Debug, Clone, Deserialize)]
pub struct Device {
    pub name: String,
    pub kind: String,
    pub hardware_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriverFilter {
    /// Chỉ những nhóm mà thiếu là máy không dùng được: mạng và ổ đĩa.
    Essential,
    /// Thêm chipset, USB, bàn phím/chuột, âm thanh — bỏ card màn hình.
    Recommended,
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageReport {
    pub dest: String,
    pub packages: usize,
    pub bytes: u64,
}

// ---------------------------------------------------------------------------
// Nhóm thiết bị
// ---------------------------------------------------------------------------

/// Nhóm mà thiếu driver là máy coi như hỏng: không mạng, hoặc không thấy ổ đĩa.
const ESSENTIAL: &[&str] = &["net", "bluetooth", "scsiadapter", "hdc", "sdhost"];

/// Thêm vào mức khuyến nghị. Cố tình **không** có `display`: driver card màn
/// hình là nhóm hay gây lỗi nhất khi bị nhồi vào ảnh cài mà không đúng máy, mà
/// thiếu nó thì Windows vẫn chạy được bằng driver cơ bản rồi tự cập nhật sau.
const EXTRA_RECOMMENDED: &[&str] =
    &["system", "usb", "hidclass", "keyboard", "mouse", "media", "monitor"];

/// Vài INF chỉ ghi `ClassGuid` mà không ghi `Class`. Bảng này đủ phủ các nhóm
/// mà bộ lọc quan tâm — nhóm lạ thì để nguyên GUID và chỉ lọt qua mức "tất cả".
const CLASS_BY_GUID: &[(&str, &str)] = &[
    ("{4d36e972-e325-11ce-bfc1-08002be10318}", "Net"),
    ("{e0cbf06c-cd8b-4647-bb8a-263b43f0f974}", "Bluetooth"),
    ("{4d36e97b-e325-11ce-bfc1-08002be10318}", "SCSIAdapter"),
    ("{4d36e96a-e325-11ce-bfc1-08002be10318}", "HDC"),
    ("{a0a588a4-c46f-4b37-b7ea-c82fe89870c6}", "SDHost"),
    ("{4d36e97d-e325-11ce-bfc1-08002be10318}", "System"),
    ("{36fc9e60-c465-11cf-8056-444553540000}", "USB"),
    ("{745a17a0-74d3-11d0-b6fe-00a0c90f57da}", "HIDClass"),
    ("{4d36e96b-e325-11ce-bfc1-08002be10318}", "Keyboard"),
    ("{4d36e96f-e325-11ce-bfc1-08002be10318}", "Mouse"),
    ("{4d36e96c-e325-11ce-bfc1-08002be10318}", "Media"),
    ("{4d36e96e-e325-11ce-bfc1-08002be10318}", "Monitor"),
    ("{4d36e968-e325-11ce-bfc1-08002be10318}", "Display"),
    ("{4d36e978-e325-11ce-bfc1-08002be10318}", "Ports"),
    ("{6bdd1fc6-810f-11d0-bec7-08002be2092f}", "Image"),
    ("{5175d334-c371-4806-b3ba-71fd53c9258d}", "SoftwareComponent"),
];

pub fn class_from_guid(guid: &str) -> Option<&'static str> {
    let g = guid.trim().to_ascii_lowercase();
    CLASS_BY_GUID.iter().find(|(k, _)| *k == g).map(|(_, v)| *v)
}

/// Gói này có lọt qua bộ lọc không. Đủ một nhóm khớp là giữ cả gói: một thư mục
/// có thể chứa nhiều INF khác nhóm, và bỏ đi phần nào cũng làm hỏng gói.
pub fn passes(classes: &[String], filter: DriverFilter) -> bool {
    if filter == DriverFilter::All {
        return true;
    }
    classes.iter().any(|c| {
        let c = c.to_ascii_lowercase();
        ESSENTIAL.contains(&c.as_str())
            || (filter == DriverFilter::Recommended && EXTRA_RECOMMENDED.contains(&c.as_str()))
    })
}

/// Nhóm này có phải driver mạng không — thứ quyết định máy có Wi-Fi sau khi cài.
pub fn is_network(classes: &[String]) -> bool {
    classes
        .iter()
        .any(|c| matches!(c.to_ascii_lowercase().as_str(), "net" | "bluetooth"))
}

// ---------------------------------------------------------------------------
// Đọc file INF
// ---------------------------------------------------------------------------

/// INF có thể là UTF-16LE (bản mới), UTF-8, hoặc một bảng mã 8-bit đời cũ.
///
/// Đoán sai bảng mã thì cả file thành ký tự rác và không đọc ra nổi `Class=` —
/// gói driver đúng sẽ bị loại vì tưởng là không đọc được.
pub fn decode_inf(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    let body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(body) {
        Ok(s) => s.to_string(),
        // Bảng mã 8-bit: chỉ phần chữ có dấu bị sai, còn `Class=` và mã phần
        // cứng đều là ASCII nên vẫn đọc đúng.
        Err(_) => body.iter().map(|b| *b as char).collect(),
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct InfMeta {
    pub class: String,
    pub class_guid: String,
    pub provider: String,
    pub version: String,
    pub date: String,
}

/// Bỏ phần chú thích cuối dòng. Trong INF, `;` bắt đầu chú thích.
fn strip_comment(line: &str) -> &str {
    match line.find(';') {
        Some(i) => &line[..i],
        None => line,
    }
}

fn unquote(v: &str) -> &str {
    v.trim().trim_matches('"')
}

/// Đọc `[Version]` và `[Strings]` của một file INF.
///
/// Chỉ đọc trong đúng section `[Version]` chứ không quét cả file: các khoá như
/// `Provider` cũng xuất hiện ở section khác với ý nghĩa khác.
pub fn parse_inf(text: &str) -> InfMeta {
    let mut meta = InfMeta::default();
    let mut strings: Vec<(String, String)> = Vec::new();
    let mut section = String::new();

    for raw in text.lines() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            section = line
                .trim_start_matches('[')
                .split(']')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        let key = k.trim().to_ascii_lowercase();
        let val = unquote(v);

        match section.as_str() {
            "version" => match key.as_str() {
                "class" => meta.class = val.to_string(),
                "classguid" => meta.class_guid = val.to_string(),
                "provider" => meta.provider = val.to_string(),
                "driverver" => {
                    // Dạng chuẩn: `DriverVer=03/28/2023,22.240.0.4`
                    let mut it = val.splitn(2, ',');
                    meta.date = it.next().unwrap_or("").trim().to_string();
                    meta.version = it.next().unwrap_or("").trim().to_string();
                }
                _ => {}
            },
            "strings" => strings.push((k.trim().to_string(), val.to_string())),
            _ => {}
        }
    }

    // `Provider=%Intel%` phải tra ngược sang `[Strings]` mới ra tên đọc được.
    if let Some(token) = meta.provider.strip_prefix('%').and_then(|s| s.strip_suffix('%')) {
        if let Some((_, v)) = strings
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(token))
        {
            meta.provider = v.clone();
        }
    }
    if meta.class.is_empty() {
        if let Some(name) = class_from_guid(&meta.class_guid) {
            meta.class = name.to_string();
        }
    }
    meta
}

/// Các tiền tố mã phần cứng đáng quan tâm. Bỏ qua `ROOT\` và `SW\` vì đó là
/// thiết bị ảo, không bao giờ là thứ người dùng đang đi tìm driver.
const ID_PREFIXES: &[&str] = &[
    "PCI\\", "USB\\", "HDAUDIO\\", "ACPI\\", "SD\\", "MMC\\", "PCMCIA\\", "BTH\\", "HID\\",
    "USBPRINT\\", "SCSI\\", "IDE\\",
];

fn is_id_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '\\' | '&' | '_' | '.' | '-' | '{' | '}')
}

/// Bóc mọi mã phần cứng mà file INF khai là hỗ trợ.
///
/// Không phân tích đúng ngữ pháp INF mà quét thẳng cả file. Lý do: mã phần cứng
/// nằm rải trong các section models có tên do nhà sản xuất tự đặt, và đi kèm đủ
/// kiểu định dạng. Quét theo tiền tố bắt được hết mà không phải đoán tên section
/// — nhận dư vài mã vô hại hơn nhiều so với bỏ sót đúng cái card Wi-Fi.
pub fn hardware_ids(text: &str) -> Vec<String> {
    let upper = text.to_ascii_uppercase();
    let bytes: Vec<char> = upper.chars().collect();
    let mut out = BTreeSet::new();

    let mut i = 0usize;
    while i < bytes.len() {
        // Mã phần cứng phải đứng đầu từ, không phải đuôi của một chuỗi khác.
        let boundary = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
        if boundary {
            let rest: String = bytes[i..bytes.len().min(i + 10)].iter().collect();
            if let Some(p) = ID_PREFIXES.iter().find(|p| rest.starts_with(**p)) {
                let mut j = i + p.len();
                while j < bytes.len() && is_id_char(bytes[j]) {
                    j += 1;
                }
                let id: String = bytes[i..j].iter().collect();
                // `PCI\` trần hoặc `USB\CLASS_...` không định danh thiết bị nào.
                if id.len() > p.len() + 3 {
                    out.insert(id);
                }
                i = j;
                continue;
            }
        }
        i += 1;
    }
    out.into_iter().collect()
}

/// Gói driver này có phủ được thiết bị không.
///
/// Windows so mã phần cứng của thiết bị với danh sách trong INF. Ở đây so xuôi
/// một chiều: mã của thiết bị bắt đầu bằng mã trong INF. Đủ để bắt trường hợp
/// thường gặp — thiết bị báo `PCI\VEN_8086&DEV_2725&SUBSYS_00108086&REV_1A`
/// còn INF chỉ ghi tới `SUBSYS`. Không so chiều ngược lại: một INF ghi `SUBSYS`
/// khác hẳn thì Windows sẽ không nhận, nên ta cũng không được nói là nhận.
pub fn covers(inf_ids: &[String], device_ids: &[String]) -> bool {
    device_ids.iter().any(|d| {
        let d = d.trim().to_ascii_uppercase();
        inf_ids.iter().any(|i| d == *i || d.starts_with(i.as_str()))
    })
}

/// Đối chiếu từng thiết bị với bộ driver đã chọn.
pub fn match_devices(devices: &[Device], packages: &[DriverPackage]) -> Vec<DeviceMatch> {
    devices
        .iter()
        .map(|dev| DeviceMatch {
            name: dev.name.clone(),
            kind: dev.kind.clone(),
            hardware_id: dev.hardware_ids.first().cloned().unwrap_or_default(),
            covered_by: packages
                .iter()
                .find(|p| covers(&p.hardware_ids, &dev.hardware_ids))
                .map(|p| p.name.clone()),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Quét thư mục
// ---------------------------------------------------------------------------

/// Không đi sâu quá mức này. Thư mục driver thật sâu nhất cũng chỉ vài tầng, còn
/// người dùng hoàn toàn có thể lỡ tay chọn cả ổ C:.
const MAX_DEPTH: usize = 8;

/// Nhiều hơn ngần này thì gần như chắc chắn người dùng chọn nhầm thư mục.
const MAX_PACKAGES: usize = 3000;

fn dir_size(dir: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(dir) else { return 0 };
    let mut total = 0u64;
    for entry in rd.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            total += dir_size(&entry.path());
        } else if let Ok(m) = entry.metadata() {
            total += m.len();
        }
    }
    total
}

fn ext_is(path: &Path, want: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case(want))
        .unwrap_or(false)
}

/// Duyệt cây thư mục, gom lại những thư mục có chứa `.inf`.
fn walk(dir: &Path, depth: usize, infs: &mut Vec<PathBuf>, installer_only: &mut u32) {
    if depth > MAX_DEPTH || infs.len() > MAX_PACKAGES * 4 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };

    let mut here_inf = false;
    let mut here_installer = false;
    let mut subdirs = Vec::new();

    for entry in rd.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        let path = entry.path();
        if ft.is_dir() {
            subdirs.push(path);
        } else if ext_is(&path, "inf") {
            here_inf = true;
            infs.push(path);
        } else if ext_is(&path, "exe") || ext_is(&path, "msi") {
            here_installer = true;
        }
    }
    if here_installer && !here_inf {
        *installer_only += 1;
    }
    for sub in subdirs {
        walk(&sub, depth + 1, infs, installer_only);
    }
}

/// Quét một thư mục và dựng danh sách gói driver.
pub fn scan(root: &Path) -> Result<DriverSet> {
    if !root.is_dir() {
        return Err(AppError::new(
            "not_a_folder",
            format!("Không mở được thư mục {}.", root.display()),
        ));
    }

    let mut inf_paths = Vec::new();
    let mut installer_only = 0u32;
    walk(root, 0, &mut inf_paths, &mut installer_only);

    // Gom theo thư mục cha: một gói driver là cả thư mục, không phải từng file.
    let mut by_folder: std::collections::BTreeMap<PathBuf, Vec<PathBuf>> = Default::default();
    for p in inf_paths {
        if let Some(parent) = p.parent() {
            by_folder.entry(parent.to_path_buf()).or_default().push(p);
        }
    }

    let mut packages = Vec::new();
    for (folder, infs) in by_folder.into_iter().take(MAX_PACKAGES) {
        let mut classes: BTreeSet<String> = BTreeSet::new();
        let mut ids: BTreeSet<String> = BTreeSet::new();
        let mut provider = String::new();
        let mut version = String::new();
        let mut names = Vec::new();

        for inf in &infs {
            let Ok(bytes) = std::fs::read(inf) else { continue };
            let text = decode_inf(&bytes);
            let meta = parse_inf(&text);
            if !meta.class.is_empty() {
                classes.insert(meta.class.clone());
            }
            if provider.is_empty() && !meta.provider.is_empty() {
                provider = meta.provider.clone();
            }
            if version.is_empty() && !meta.version.is_empty() {
                version = meta.version.clone();
            }
            ids.extend(hardware_ids(&text));
            names.push(
                inf.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            );
        }

        // Không đọc ra nhóm nào thì gói này không phân loại được — vẫn giữ, chỉ
        // là chỉ lọt qua mức "tất cả".
        packages.push(DriverPackage {
            name: folder
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| folder.to_string_lossy().to_string()),
            size: dir_size(&folder),
            folder: folder.to_string_lossy().to_string(),
            infs: names,
            classes: classes.into_iter().collect(),
            provider,
            version,
            hardware_ids: ids.into_iter().collect(),
        });
    }

    let total_size = packages.iter().map(|p| p.size).sum();
    Ok(DriverSet {
        source: root.to_string_lossy().to_string(),
        packages,
        total_size,
        installer_only,
    })
}

// ---------------------------------------------------------------------------
// Thư mục xuất của ứng dụng
// ---------------------------------------------------------------------------

/// Nơi ứng dụng xuất driver của máy đang chạy.
pub fn export_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("GetWinUSB").join("drivers")
}

/// Đúng như `download::discard`: chỉ xoá được thứ do ứng dụng tự tạo ra.
pub fn discard_export() -> Result<bool> {
    let dir = export_dir();
    if !dir.is_dir() {
        return Ok(false);
    }
    let norm = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    // Hàng rào chống một biến môi trường LOCALAPPDATA bị đặt bậy thành `C:\`.
    if !norm(&dir).ends_with("GetWinUSB\\drivers") && !norm(&dir).ends_with("GetWinUSB/drivers") {
        return Err(AppError::new(
            "not_managed",
            "Đường dẫn thư mục xuất driver không hợp lệ nên ứng dụng không xoá.",
        ));
    }
    std::fs::remove_dir_all(&dir)?;
    Ok(true)
}

// ---------------------------------------------------------------------------
// Chép sang USB
// ---------------------------------------------------------------------------

/// Tên thư mục Windows Setup tự tìm trên ổ đĩa rời.
pub const WINPE_DIR: &str = "$WinPEDriver$";

fn copy_dir(from: &Path, to: &Path) -> Result<u64> {
    std::fs::create_dir_all(to)?;
    let mut bytes = 0u64;
    for entry in std::fs::read_dir(from)?.flatten() {
        let ft = entry.file_type()?;
        if ft.is_symlink() {
            continue;
        }
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if ft.is_dir() {
            bytes += copy_dir(&src, &dst)?;
        } else {
            bytes += std::fs::copy(&src, &dst)?;
        }
    }
    Ok(bytes)
}

/// Tên thư mục đích cho một gói, đảm bảo không trùng nhau.
///
/// Nhiều gói cùng tên `Drivers` hay `x64` là chuyện thường khi người dùng gộp
/// nhiều bộ driver lại; ghi đè lên nhau thì gói sau nuốt mất gói trước.
fn unique_name(base: &str, used: &mut BTreeSet<String>) -> String {
    let clean: String = base
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '_' })
        .collect();
    let clean = if clean.trim_matches('_').is_empty() { "driver".to_string() } else { clean };
    let mut name = clean.clone();
    let mut n = 2;
    while used.contains(&name.to_ascii_lowercase()) {
        name = format!("{clean}_{n}");
        n += 1;
    }
    used.insert(name.to_ascii_lowercase());
    name
}

/// Chép các gói đã chọn vào `<ổ>:\$WinPEDriver$`.
///
/// `on_progress` nhận (số gói đã xong, tổng số, tên gói) để giao diện báo tiến
/// trình — chép vài trăm MB sang USB 2.0 mất hàng phút.
pub fn stage<F>(
    drive_letter: &str,
    packages: &[DriverPackage],
    mut on_progress: F,
) -> Result<StageReport>
where
    F: FnMut(usize, usize, &str),
{
    let letter = drive_letter.trim().trim_end_matches(':');
    if letter.len() != 1 || !letter.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(AppError::new("bad_drive", "Chữ cái ổ đĩa không hợp lệ."));
    }
    let root = PathBuf::from(format!("{letter}:\\")).join(WINPE_DIR);
    std::fs::create_dir_all(&root)?;

    let mut used: BTreeSet<String> = BTreeSet::new();
    let mut bytes = 0u64;
    let total = packages.len();

    for (i, pkg) in packages.iter().enumerate() {
        on_progress(i, total, &pkg.name);
        let dest = root.join(unique_name(&pkg.name, &mut used));
        bytes += copy_dir(Path::new(&pkg.folder), &dest)?;
    }
    on_progress(total, total, "");

    Ok(StageReport {
        dest: root.to_string_lossy().to_string(),
        packages: total,
        bytes,
    })
}

// ---------------------------------------------------------------------------
// PowerShell
// ---------------------------------------------------------------------------

/// Xuất driver của chính máy đang chạy.
///
/// `Export-WindowsDriver -Online` lấy đúng các gói driver bên thứ ba đã cài —
/// tức là bộ driver đang chạy được trên chính chiếc máy này. Với người cài lại
/// máy của mình thì đây là nguồn tin cậy nhất: không phải đoán model, không phụ
/// thuộc trang tải nào còn sống hay không.
const SCRIPT_EXPORT: &str = r#"
$dest = '%%DEST%%'
$all = @(Get-WindowsDriver -Online)
Write-Output ('GWU:TOTAL ' + $all.Count)
if (Test-Path -LiteralPath $dest) { Remove-Item -LiteralPath $dest -Recurse -Force }
New-Item -ItemType Directory -Force -Path $dest | Out-Null
Export-WindowsDriver -Online -Destination $dest | ForEach-Object {
  Write-Output ('GWU:DRV ' + $_.Driver)
}
Write-Output 'GWU:DONE'
"#;

/// Số driver đã xuất xong / tổng số, đọc từ các dòng `GWU:` ở trên.
pub fn parse_export_line(line: &str) -> Option<(&'static str, String)> {
    let line = line.trim();
    if let Some(rest) = line.strip_prefix("GWU:TOTAL ") {
        return Some(("total", rest.trim().to_string()));
    }
    if let Some(rest) = line.strip_prefix("GWU:DRV ") {
        return Some(("driver", rest.trim().to_string()));
    }
    if line == "GWU:DONE" {
        return Some(("done", String::new()));
    }
    None
}

/// Xuất driver máy hiện tại vào thư mục riêng của ứng dụng.
pub async fn export_this_pc<F>(mut on_progress: F) -> Result<PathBuf>
where
    F: FnMut(u32, u32) + Send,
{
    if !crate::ps::is_elevated() {
        return Err(AppError::new(
            "not_admin",
            "Xuất driver của máy cần quyền quản trị. Hãy bấm \"Chạy lại với quyền quản trị\" rồi thử lại.",
        ));
    }
    let dir = export_dir();
    let script = SCRIPT_EXPORT.replace("%%DEST%%", &dir.to_string_lossy().replace('\'', "''"));

    let mut total = 0u32;
    let mut done = 0u32;
    crate::ps::run_streaming(&script, |line| {
        match parse_export_line(line) {
            Some(("total", v)) => total = v.parse().unwrap_or(0),
            Some(("driver", _)) => {
                done += 1;
                on_progress(done, total);
            }
            Some(("done", _)) => on_progress(total.max(done), total.max(done)),
            _ => {}
        }
    })
    .await?;

    Ok(dir)
}

/// Đọc các thiết bị mạng và ổ đĩa đang có trên máy, kèm mã phần cứng.
///
/// Lọc theo `InstanceId` bắt đầu bằng `PCI\` hoặc `USB\` để bỏ qua card ảo
/// (VPN, loopback, WAN Miniport) — chúng không cần driver rời và chỉ làm nhiễu
/// danh sách người dùng phải đọc.
const SCRIPT_DEVICES: &str = r#"
$wifi = @{}
try {
  foreach ($a in Get-NetAdapter -Physical -ErrorAction Stop) {
    $wifi[$a.PnPDeviceID] = [string]$a.PhysicalMediaType
  }
} catch {}

$out = @()
foreach ($d in Get-PnpDevice -PresentOnly -Class Net,Bluetooth,SCSIAdapter,HDC -ErrorAction SilentlyContinue) {
  if ($d.InstanceId -notlike 'PCI\*' -and $d.InstanceId -notlike 'USB\*') { continue }
  $ids = @()
  try {
    $ids = @((Get-PnpDeviceProperty -InstanceId $d.InstanceId -KeyName 'DEVPKEY_Device_HardwareIds' -ErrorAction Stop).Data)
  } catch {}
  if ($ids.Count -eq 0) { $ids = @($d.InstanceId) }

  $media = [string]$wifi[$d.InstanceId]
  $name = [string]$d.FriendlyName
  $kind = switch -Regex ($d.Class) {
    'Bluetooth'   { 'bluetooth'; break }
    'SCSIAdapter' { 'storage'; break }
    'HDC'         { 'storage'; break }
    default {
      if ($media -match '802\.11' -or $name -match 'Wi-?Fi|Wireless|WLAN|802\.11') { 'wifi' }
      else { 'ethernet' }
    }
  }
  $out += [pscustomobject]@{ name = $name; kind = $kind; hardware_ids = @($ids) }
}
ConvertTo-Json -InputObject @($out) -Depth 4 -Compress
"#;

pub async fn list_devices() -> Result<Vec<Device>> {
    crate::ps::run_json(SCRIPT_DEVICES).await
}

/// Dung lượng trống còn lại của một ổ, để biết bộ driver có nằm vừa không.
pub async fn free_space(drive_letter: &str) -> Result<u64> {
    let letter = drive_letter.trim().trim_end_matches(':');
    if letter.len() != 1 || !letter.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(AppError::new("bad_drive", "Chữ cái ổ đĩa không hợp lệ."));
    }
    let script = format!("[long](Get-PSDrive -Name '{letter}').Free");
    let out = crate::ps::run(&script).await?;
    out.trim()
        .parse::<u64>()
        .map_err(|_| AppError::new("no_free_space", "Không đọc được dung lượng trống của ổ USB."))
}

// ---------------------------------------------------------------------------
// Ghép các mảnh lại
// ---------------------------------------------------------------------------

/// Kết quả phân tích một thư mục driver, đã đối chiếu với phần cứng máy.
#[derive(Debug, Clone, Serialize)]
pub struct DriverAnalysis {
    pub set: DriverSet,
    /// Tên các gói sẽ được chép theo bộ lọc đang chọn.
    pub selected: Vec<String>,
    pub selected_size: u64,
    /// Thiết bị mạng/ổ đĩa của máy, kèm kết luận đã có driver hay chưa.
    ///
    /// Rỗng nghĩa là không đọc được danh sách thiết bị (chạy ngoài Windows,
    /// hoặc PowerShell từ chối) — giao diện phải nói rõ là *không kiểm tra
    /// được*, chứ không phải *không có thiết bị nào*.
    pub devices: Vec<DeviceMatch>,
    pub devices_read: bool,
}

/// Lọc ra đúng những gói sẽ được chép.
pub fn select(set: &DriverSet, filter: DriverFilter) -> Vec<&DriverPackage> {
    set.packages.iter().filter(|p| passes(&p.classes, filter)).collect()
}

/// Quét thư mục, lọc theo mức đã chọn, rồi đối chiếu với thiết bị thật của máy.
///
/// Đối chiếu chạy trên **bộ đã lọc** chứ không phải toàn bộ thư mục: người dùng
/// cần biết chiếc USB sắp ghi có driver Wi-Fi hay không, chứ không phải cái thư
/// mục trên đĩa có hay không.
pub async fn analyse(path: &Path, filter: DriverFilter) -> Result<DriverAnalysis> {
    let root = path.to_path_buf();
    let set = tokio::task::spawn_blocking(move || scan(&root))
        .await
        .map_err(|e| AppError::new("scan_failed", format!("Quét thư mục driver hỏng: {e}")))??;

    let chosen: Vec<DriverPackage> = select(&set, filter).into_iter().cloned().collect();
    let selected_size = chosen.iter().map(|p| p.size).sum();
    let selected = chosen.iter().map(|p| p.name.clone()).collect();

    let (devices, devices_read) = match list_devices().await {
        Ok(d) => (match_devices(&d, &chosen), true),
        Err(_) => (Vec::new(), false),
    };

    Ok(DriverAnalysis { set, selected, selected_size, devices, devices_read })
}

/// Quét lại rồi chép các gói hợp lệ sang USB.
///
/// Quét lại thay vì nhận danh sách gói từ giao diện: chỉ có một nguồn sự thật,
/// và thứ được chép luôn đúng bằng thứ vừa hiển thị cho người dùng xem.
pub async fn stage_to_usb<F>(
    path: &Path,
    filter: DriverFilter,
    drive_letter: &str,
    on_progress: F,
) -> Result<StageReport>
where
    F: FnMut(usize, usize, &str) + Send + 'static,
{
    let root = path.to_path_buf();
    let set = tokio::task::spawn_blocking({
        let root = root.clone();
        move || scan(&root)
    })
    .await
    .map_err(|e| AppError::new("scan_failed", format!("Quét thư mục driver hỏng: {e}")))??;

    let chosen: Vec<DriverPackage> = select(&set, filter).into_iter().cloned().collect();
    if chosen.is_empty() {
        return Err(AppError::new(
            "no_drivers",
            "Không có gói driver nào hợp với mức lọc đang chọn.",
        ));
    }

    // Ổ USB đã chứa nguyên bộ cài Windows rồi, nên chỗ trống còn lại thường
    // không nhiều. Phát hiện trước còn hơn để việc chép chết giữa chừng và bỏ
    // lại một thư mục driver dở dang mà Setup vẫn sẽ cố cài.
    let need: u64 = chosen.iter().map(|p| p.size).sum();
    if let Ok(free) = free_space(drive_letter).await {
        if free < need + 16 * 1024 * 1024 {
            return Err(AppError::new(
                "usb_full",
                format!(
                    "Ổ USB chỉ còn trống {:.1} GB, không đủ cho {:.1} GB driver. \
                     Hãy chọn mức lọc hẹp hơn, hoặc dùng ổ USB lớn hơn.",
                    free as f64 / 1024.0 / 1024.0 / 1024.0,
                    need as f64 / 1024.0 / 1024.0 / 1024.0,
                ),
            ));
        }
    }

    let letter = drive_letter.to_string();
    tokio::task::spawn_blocking(move || {
        let mut cb = on_progress;
        stage(&letter, &chosen, &mut cb)
    })
    .await
    .map_err(|e| AppError::new("copy_failed", format!("Chép driver hỏng: {e}")))?
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const INTEL_WIFI: &str = r#"
; Intel(R) Wireless driver
[Version]
Signature   = "$WINDOWS NT$"
Class       = Net
ClassGUID   = {4d36e972-e325-11ce-bfc1-08002be10318}
Provider    = %Intel%
CatalogFile = Netwtw08.cat
DriverVer   = 03/28/2023,22.240.0.4

[Manufacturer]
%Intel% = Intel, NTamd64.10.0

[Intel.NTamd64.10.0]
%AX211.DeviceDesc% = Install, PCI\VEN_8086&DEV_51F0&SUBSYS_00748086
%AX201.DeviceDesc% = Install, PCI\VEN_8086&DEV_A0F0&SUBSYS_00748086

[Strings]
Intel = "Intel Corporation"
AX211.DeviceDesc = "Intel(R) Wi-Fi 6E AX211 160MHz"
"#;

    fn pkg(name: &str, classes: &[&str], ids: &[&str]) -> DriverPackage {
        DriverPackage {
            folder: format!("C:\\{name}"),
            name: name.to_string(),
            infs: vec![format!("{name}.inf")],
            classes: classes.iter().map(|s| s.to_string()).collect(),
            provider: String::new(),
            version: String::new(),
            size: 0,
            hardware_ids: ids.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn version_section_is_read_and_provider_resolved_from_strings() {
        let m = parse_inf(INTEL_WIFI);
        assert_eq!(m.class, "Net");
        assert_eq!(m.version, "22.240.0.4");
        assert_eq!(m.date, "03/28/2023");
        // `%Intel%` phải thành tên thật, không thì danh sách toàn dấu phần trăm.
        assert_eq!(m.provider, "Intel Corporation");
    }

    #[test]
    fn a_missing_class_is_recovered_from_the_class_guid() {
        // Không ít INF chỉ ghi ClassGuid. Bỏ qua chúng nghĩa là loại nhầm cả gói
        // driver đúng chỉ vì thiếu một dòng.
        let inf = "[Version]\nClassGuid={4d36e972-e325-11ce-bfc1-08002be10318}\n";
        assert_eq!(parse_inf(inf).class, "Net");
    }

    #[test]
    fn keys_outside_the_version_section_are_ignored() {
        let inf = "[Version]\nClass=Net\n\n[SomeInstall.Services]\nClass=Display\n";
        assert_eq!(parse_inf(inf).class, "Net");
    }

    #[test]
    fn comments_do_not_leak_into_values() {
        let inf = "[Version]\nClass=Net ; nhóm mạng\n";
        assert_eq!(parse_inf(inf).class, "Net");
    }

    #[test]
    fn hardware_ids_are_pulled_out_of_the_models_section() {
        let ids = hardware_ids(INTEL_WIFI);
        assert!(ids.contains(&"PCI\\VEN_8086&DEV_51F0&SUBSYS_00748086".to_string()), "{ids:?}");
        assert!(ids.contains(&"PCI\\VEN_8086&DEV_A0F0&SUBSYS_00748086".to_string()), "{ids:?}");
        // Chuỗi trong [Strings] không phải mã phần cứng.
        assert!(!ids.iter().any(|i| i.contains("DEVICEDESC")), "{ids:?}");
    }

    #[test]
    fn a_device_id_with_a_revision_suffix_still_matches() {
        // Windows báo mã đầy đủ kèm &REV_, còn INF thường chỉ ghi tới SUBSYS.
        // So bằng dấu bằng thuần thì không bao giờ khớp.
        let inf = vec!["PCI\\VEN_8086&DEV_51F0&SUBSYS_00748086".to_string()];
        let dev = vec!["PCI\\VEN_8086&DEV_51F0&SUBSYS_00748086&REV_01".to_string()];
        assert!(covers(&inf, &dev));
    }

    #[test]
    fn a_driver_for_another_subsystem_is_not_reported_as_covering() {
        // Đây là chỗ dễ nói dối nhất: cùng chip, khác hãng lắp máy. Windows sẽ
        // không nhận, nên ứng dụng cũng không được báo là đã có driver.
        let inf = vec!["PCI\\VEN_8086&DEV_51F0&SUBSYS_11112222".to_string()];
        let dev = vec!["PCI\\VEN_8086&DEV_51F0&SUBSYS_00748086&REV_01".to_string()];
        assert!(!covers(&inf, &dev));
    }

    #[test]
    fn utf16_inf_files_are_decoded() {
        let text = "[Version]\r\nClass=Net\r\n";
        let mut bytes = vec![0xFF, 0xFE];
        for u in text.encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        assert_eq!(parse_inf(&decode_inf(&bytes)).class, "Net");
    }

    #[test]
    fn a_high_byte_ansi_file_still_yields_its_class() {
        // INF đời cũ hay là CP1252. Chỉ cần `Class=` đọc đúng là đủ.
        let mut bytes = b"[Version]\r\nClass=Net\r\nProvider=\"".to_vec();
        bytes.push(0xE9); // 'é' trong CP1252
        bytes.extend_from_slice(b"\"\r\n");
        assert_eq!(parse_inf(&decode_inf(&bytes)).class, "Net");
    }

    #[test]
    fn the_essential_filter_keeps_network_and_drops_graphics() {
        let net = vec!["Net".to_string()];
        let display = vec!["Display".to_string()];
        assert!(passes(&net, DriverFilter::Essential));
        assert!(!passes(&display, DriverFilter::Essential));
        // Card màn hình cố tình nằm ngoài cả mức khuyến nghị.
        assert!(!passes(&display, DriverFilter::Recommended));
        assert!(passes(&display, DriverFilter::All));
    }

    #[test]
    fn a_folder_mixing_classes_is_kept_whole() {
        // Bỏ bớt file trong một thư mục gói là làm hỏng chữ ký của cả gói.
        let mixed = vec!["Display".to_string(), "Net".to_string()];
        assert!(passes(&mixed, DriverFilter::Essential));
    }

    #[test]
    fn matching_reports_the_package_that_covers_each_device() {
        let devices = vec![
            Device {
                name: "Intel(R) Wi-Fi 6E AX211".into(),
                kind: "wifi".into(),
                hardware_ids: vec!["PCI\\VEN_8086&DEV_51F0&SUBSYS_00748086&REV_01".into()],
            },
            Device {
                name: "Realtek Gaming GbE".into(),
                kind: "ethernet".into(),
                hardware_ids: vec!["PCI\\VEN_10EC&DEV_8168&SUBSYS_00011025".into()],
            },
        ];
        let packages = vec![pkg(
            "netwtw08",
            &["Net"],
            &["PCI\\VEN_8086&DEV_51F0&SUBSYS_00748086"],
        )];

        let m = match_devices(&devices, &packages);
        assert_eq!(m[0].covered_by.as_deref(), Some("netwtw08"));
        // Không có driver cho card mạng dây thì phải nói là chưa có.
        assert_eq!(m[1].covered_by, None);
    }

    #[test]
    fn duplicate_folder_names_do_not_overwrite_each_other() {
        let mut used = BTreeSet::new();
        assert_eq!(unique_name("x64", &mut used), "x64");
        assert_eq!(unique_name("x64", &mut used), "x64_2");
        assert_eq!(unique_name("x64", &mut used), "x64_3");
    }

    #[test]
    fn folder_names_unsafe_for_fat32_are_cleaned_up() {
        let mut used = BTreeSet::new();
        assert_eq!(unique_name("Wi-Fi: Intel®", &mut used), "Wi-Fi__Intel_");
    }

    #[test]
    fn export_progress_lines_are_parsed() {
        assert_eq!(parse_export_line("GWU:TOTAL 42"), Some(("total", "42".into())));
        assert_eq!(parse_export_line("GWU:DRV oem12.inf"), Some(("driver", "oem12.inf".into())));
        assert_eq!(parse_export_line("GWU:DONE"), Some(("done", String::new())));
        assert_eq!(parse_export_line("chuyện gì đó khác"), None);
    }

    #[test]
    fn scanning_a_folder_groups_infs_by_their_directory() {
        let tmp = std::env::temp_dir().join(format!("gwu-drv-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let wifi = tmp.join("Netwtw08");
        let gpu = tmp.join("igfx");
        std::fs::create_dir_all(&wifi).unwrap();
        std::fs::create_dir_all(&gpu).unwrap();
        std::fs::write(wifi.join("netwtw08.inf"), INTEL_WIFI).unwrap();
        std::fs::write(wifi.join("netwtw08.sys"), vec![0u8; 2048]).unwrap();
        std::fs::write(gpu.join("igdlh64.inf"), "[Version]\nClass=Display\n").unwrap();
        // Thư mục chỉ có file cài đặt: phải được đếm riêng chứ không im lặng bỏ.
        let vendor = tmp.join("SetupOnly");
        std::fs::create_dir_all(&vendor).unwrap();
        std::fs::write(vendor.join("Setup.exe"), b"MZ").unwrap();

        let set = scan(&tmp).unwrap();
        assert_eq!(set.packages.len(), 2, "{:?}", set.packages);
        assert_eq!(set.installer_only, 1);

        let wifi_pkg = set.packages.iter().find(|p| p.name == "Netwtw08").unwrap();
        assert_eq!(wifi_pkg.classes, vec!["Net".to_string()]);
        assert!(wifi_pkg.size >= 2048);
        assert!(!wifi_pkg.hardware_ids.is_empty());

        let kept: Vec<&DriverPackage> = set
            .packages
            .iter()
            .filter(|p| passes(&p.classes, DriverFilter::Recommended))
            .collect();
        assert_eq!(kept.len(), 1, "chỉ gói mạng được giữ");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Dựng một cây thư mục driver giống thật rồi in ra JSON của `analyse`.
    ///
    /// Dùng để nuôi bộ chụp màn hình: ảnh trong tài liệu là kết quả thật của bộ
    /// phân tích này, không phải dữ liệu bịa bằng tay.
    ///
    /// ```text
    /// cargo test --manifest-path src-tauri/Cargo.toml dump_driver_fixture -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "công cụ sinh dữ liệu mẫu"]
    fn dump_driver_fixture() {
        let tmp = std::env::temp_dir().join("gwu-drv-fixture");
        let _ = std::fs::remove_dir_all(&tmp);

        let make = |name: &str, inf: &str, body: &str, bulk: usize| {
            let dir = tmp.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(inf), body).unwrap();
            std::fs::write(dir.join(inf.replace(".inf", ".sys")), vec![0u8; bulk]).unwrap();
        };

        make("Netwtw08", "netwtw08.inf", INTEL_WIFI, 5_400_000);
        make(
            "rtl8168",
            "rt640x64.inf",
            "[Version]\nClass=Net\nClassGuid={4d36e972-e325-11ce-bfc1-08002be10318}\n\
             Provider=%Realtek%\nDriverVer=05/09/2023,10.62.0503.2023\n\
             [Realtek.NTamd64]\n%RTL%=Install, PCI\\VEN_10EC&DEV_8168&SUBSYS_00011025\n\
             [Strings]\nRealtek=\"Realtek Semiconductor Corp.\"\n",
            1_200_000,
        );
        make(
            "iaStorVD",
            "iastorvd.inf",
            "[Version]\nClass=SCSIAdapter\nClassGuid={4d36e97b-e325-11ce-bfc1-08002be10318}\n\
             Provider=%Intel%\nDriverVer=02/14/2023,19.5.1.1040\n\
             [Intel.NTamd64]\n%VMD%=Install, PCI\\VEN_8086&DEV_A77F\n\
             [Strings]\nIntel=\"Intel Corporation\"\n",
            900_000,
        );
        make(
            "ibtusb",
            "ibtusb.inf",
            "[Version]\nClass=Bluetooth\nClassGuid={e0cbf06c-cd8b-4647-bb8a-263b43f0f974}\n\
             Provider=%Intel%\nDriverVer=03/28/2023,23.20.0.3\n\
             [Intel.NTamd64]\n%BT%=Install, USB\\VID_8087&PID_0033\n\
             [Strings]\nIntel=\"Intel Corporation\"\n",
            700_000,
        );
        make(
            "SmbusChipset",
            "smbus.inf",
            "[Version]\nClass=System\nClassGuid={4d36e97d-e325-11ce-bfc1-08002be10318}\n\
             Provider=%Intel%\nDriverVer=01/10/2023,10.1.19444.8378\n\
             [Strings]\nIntel=\"Intel Corporation\"\n",
            300_000,
        );
        make(
            "RealtekAudio",
            "hdxrt.inf",
            "[Version]\nClass=MEDIA\nClassGuid={4d36e96c-e325-11ce-bfc1-08002be10318}\n\
             Provider=%Realtek%\nDriverVer=04/03/2023,6.0.9522.1\n\
             [Strings]\nRealtek=\"Realtek Semiconductor Corp.\"\n",
            2_800_000,
        );
        make(
            "igfx",
            "iigd_dch.inf",
            "[Version]\nClass=Display\nClassGuid={4d36e968-e325-11ce-bfc1-08002be10318}\n\
             Provider=%Intel%\nDriverVer=06/20/2023,31.0.101.4502\n\
             [Strings]\nIntel=\"Intel Corporation\"\n",
            48_000_000,
        );
        // Thư mục chỉ có bộ cài .exe — phải bị đếm riêng chứ không im lặng bỏ.
        let vendor = tmp.join("MyASUS-Setup");
        std::fs::create_dir_all(&vendor).unwrap();
        std::fs::write(vendor.join("Setup.exe"), b"MZ").unwrap();

        let devices = vec![
            Device {
                name: "Intel(R) Wi-Fi 6E AX211 160MHz".into(),
                kind: "wifi".into(),
                hardware_ids: vec!["PCI\\VEN_8086&DEV_51F0&SUBSYS_00748086&REV_01".into()],
            },
            Device {
                name: "Realtek PCIe GbE Family Controller".into(),
                kind: "ethernet".into(),
                hardware_ids: vec!["PCI\\VEN_10EC&DEV_8168&SUBSYS_00011025&REV_15".into()],
            },
            Device {
                name: "Intel(R) Wireless Bluetooth(R)".into(),
                kind: "bluetooth".into(),
                hardware_ids: vec!["USB\\VID_8087&PID_0033&REV_0002".into()],
            },
            Device {
                name: "Intel RST VMD Controller 9A0B".into(),
                kind: "storage".into(),
                hardware_ids: vec!["PCI\\VEN_8086&DEV_9A0B&SUBSYS_15E71043".into()],
            },
        ];

        let set = scan(&tmp).unwrap();
        let mut out = serde_json::Map::new();
        for filter in [DriverFilter::Essential, DriverFilter::Recommended, DriverFilter::All] {
            let chosen: Vec<DriverPackage> =
                select(&set, filter).into_iter().cloned().collect();
            let analysis = DriverAnalysis {
                set: set.clone(),
                selected: chosen.iter().map(|p| p.name.clone()).collect(),
                selected_size: chosen.iter().map(|p| p.size).sum(),
                devices: match_devices(&devices, &chosen),
                devices_read: true,
            };
            let key = match filter {
                DriverFilter::Essential => "essential",
                DriverFilter::Recommended => "recommended",
                DriverFilter::All => "all",
            };
            out.insert(key.into(), serde_json::to_value(&analysis).unwrap());
        }
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn scanning_something_that_is_not_a_folder_fails_clearly() {
        let missing = std::env::temp_dir().join("gwu-khong-ton-tai-12345");
        let e = scan(&missing).unwrap_err();
        assert_eq!(e.code, "not_a_folder");
    }
}
