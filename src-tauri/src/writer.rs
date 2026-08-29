//! Tạo USB cài Windows: xoá, chia phân vùng, chép bộ cài, ghi mã khởi động.
//!
//! Đây là phần duy nhất trong ứng dụng có thể làm mất dữ liệu, nên mọi thao tác
//! đều đi qua `guard()` — hàm kiểm tra lại ổ đĩa ngay trước khi ghi thay vì tin
//! vào thông tin đã đọc lúc người dùng bấm chọn.

use crate::error::{AppError, Result};
use crate::ps;
use crate::usb;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartitionScheme {
    /// GPT + FAT32 — chuẩn cho mọi máy UEFI đời mới. Mặc định.
    GptFat32,
    /// MBR + FAT32 — khởi động được cả UEFI (chế độ CSM) lẫn BIOS cũ.
    MbrFat32,
    /// MBR + NTFS — chỉ cho máy BIOS đời cũ; không bị giới hạn file 4 GB.
    MbrNtfs,
}

impl PartitionScheme {
    fn style(self) -> &'static str {
        match self {
            PartitionScheme::GptFat32 => "GPT",
            _ => "MBR",
        }
    }
    fn filesystem(self) -> &'static str {
        match self {
            PartitionScheme::MbrNtfs => "NTFS",
            _ => "FAT32",
        }
    }
    /// FAT32 do Windows tạo bị chặn ở 32 GB, nên phân vùng boot phải cắt tại đó
    /// và phần dư được để riêng thành một phân vùng dữ liệu.
    fn max_boot_bytes(self) -> Option<u64> {
        match self.filesystem() {
            "FAT32" => Some(32 * 1024 * 1024 * 1024),
            _ => None,
        }
    }
    /// FAT32 không chứa nổi file quá 4 GB — install.wim thường vượt mốc này.
    fn needs_wim_split(self) -> bool {
        self.filesystem() == "FAT32"
    }
}

/// Vân tay của ổ ghi lại lúc người dùng bấm chọn. Nếu không khớp nữa nghĩa là
/// ổ đã bị rút ra cắm lại (hoặc đổi ổ khác ở cùng vị trí), phải dừng.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatRequest {
    pub disk_number: u32,
    pub scheme: PartitionScheme,
    pub label: String,
    pub confirm_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatResult {
    pub drive_letter: String,
    pub filesystem: String,
    pub partition_style: String,
    /// Ổ lớn hơn 32 GB dùng FAT32 sẽ có thêm một phân vùng NTFS chứa phần dư.
    pub has_data_partition: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteRequest {
    pub disk_number: u32,
    pub iso_path: String,
    /// Vẫn cần ở bước ghi để biết có phải tách install.wim và có ghi mã khởi
    /// động MBR hay không — phải khớp với kiểu đã format ở bước trước.
    pub scheme: PartitionScheme,
    /// Nhãn đã đặt lúc format, dùng để tìm đúng phân vùng khởi động khi ổ có
    /// thêm phân vùng dữ liệu.
    pub label: String,
    pub confirm_token: String,
    pub unattend: crate::unattend::UnattendConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteProgress {
    pub stage: String,
    pub stage_index: u32,
    pub total_stages: u32,
    pub percent: f64,
    pub message: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsoInfo {
    pub path: String,
    pub size: u64,
    pub install_image: Option<String>,
    pub install_image_size: u64,
    pub editions: Vec<String>,
    pub architecture: String,
    pub needs_split: bool,
    pub bootable_uefi: bool,
}

/// Bước format chỉ có hai chặng; bước ghi Windows có sáu; ghi raw có ba.
pub const FORMAT_STAGES: u32 = 2;
pub const WRITE_STAGES: u32 = 6;
pub const RAW_WRITE_STAGES: u32 = 3;

/// Bộ cài Windows 11 không nằm vừa ổ dưới 8 GB.
const WINDOWS_MIN_USB: u64 = 8 * 1024 * 1024 * 1024;

pub fn token_for(disk: &usb::UsbDisk) -> String {
    format!(
        "{}|{}|{}",
        disk.model.trim(),
        disk.serial.clone().unwrap_or_default().trim(),
        disk.size
    )
}

/// Hàng rào an toàn cuối cùng trước mọi thao tác ghi lên ổ.
/// `min_size` là dung lượng tối thiểu của ổ USB cho luồng đang gọi. Bộ cài
/// Windows cần 8 GB, còn ISO Linux thì nhiều bản nằm gọn trong 2 GB — ghi cứng
/// một ngưỡng chung sẽ từ chối oan những chiếc USB hoàn toàn dùng được.
async fn guard(
    disk_number: u32,
    confirm_token: &str,
    iso_path: Option<&str>,
    min_size: u64,
) -> Result<usb::UsbDisk> {
    let disks = usb::list().await?;
    let disk = disks
        .into_iter()
        .find(|d| d.number == disk_number)
        .ok_or_else(|| {
            AppError::new("disk_gone", "Không còn thấy ổ USB đã chọn. Hãy cắm lại và chọn lại.")
        })?;

    if !disk.is_writable_target() {
        return Err(AppError::new(
            "unsafe_target",
            format!(
                "Ổ đĩa {disk_number} không phải ổ USB rời an toàn để ghi (hệ thống/boot/chỉ đọc). Đã dừng để bảo vệ dữ liệu."
            ),
        ));
    }
    if token_for(&disk) != confirm_token {
        return Err(AppError::new(
            "disk_changed",
            "Ổ USB ở vị trí này đã thay đổi kể từ lúc bạn chọn. Hãy chọn lại để chắc chắn ghi đúng ổ.",
        ));
    }
    if disk.size < min_size {
        return Err(AppError::new(
            "too_small",
            format!(
                "Ổ chỉ có {:.1} GB, cần tối thiểu {:.1} GB cho bộ cài này.",
                disk.size as f64 / 1024.0 / 1024.0 / 1024.0,
                min_size as f64 / 1024.0 / 1024.0 / 1024.0
            ),
        ));
    }

    if let Some(path) = iso_path {
        let iso = std::path::Path::new(path);
        if !iso.is_file() {
            return Err(AppError::new("no_iso", "Không tìm thấy file ISO đã chọn."));
        }
        if std::fs::metadata(iso)?.len() + 512 * 1024 * 1024 > disk.size {
            return Err(AppError::new("too_small", "Ổ USB không đủ chỗ cho file ISO này."));
        }
    }
    if !ps::is_elevated() {
        return Err(AppError::new(
            "not_admin",
            "Cần quyền Administrator để chia lại phân vùng ổ USB.",
        ));
    }
    Ok(disk)
}

/// Đọc thông tin bên trong file ISO: bản Windows nào, có cần tách wim không.
pub async fn inspect_iso(path: &str) -> Result<IsoInfo> {
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);

    if cfg!(not(windows)) {
        return Ok(IsoInfo {
            path: path.to_string(),
            size,
            install_image: Some("sources\\install.wim".into()),
            install_image_size: 5_100_000_000,
            editions: vec!["Windows 11 Home".into(), "Windows 11 Pro".into()],
            architecture: "x64".into(),
            needs_split: true,
            bootable_uefi: true,
        });
    }

    let script = SCRIPT_INSPECT.replace("%%ISO%%", &escape(path));
    let info: IsoInfoRaw = ps::run_json(&script).await?;

    Ok(IsoInfo {
        path: path.to_string(),
        size,
        install_image: info.install_image,
        install_image_size: info.install_image_size,
        editions: info.editions,
        architecture: info.architecture,
        needs_split: info.install_image_size > 4 * 1024 * 1024 * 1024 - 1,
        bootable_uefi: info.bootable_uefi,
    })
}

#[derive(Deserialize)]
struct IsoInfoRaw {
    install_image: Option<String>,
    install_image_size: u64,
    editions: Vec<String>,
    architecture: String,
    bootable_uefi: bool,
}

/// Chuỗi đưa vào script PowerShell luôn nằm trong nháy đơn, nên chỉ cần nhân đôi
/// dấu nháy đơn là an toàn tuyệt đối trước mọi ký tự đặc biệt khác.
fn escape(s: &str) -> String {
    s.replace('\'', "''")
}

/// Tìm ký tự ổ đĩa của phân vùng khởi động trên ổ USB đích.
///
/// Đọc lại từ hệ thống thay vì nhớ giá trị từ bước format: giữa hai bước người
/// dùng có thể đã rút ra cắm lại, và Windows hoàn toàn có thể gán một ký tự
/// khác cho cùng phân vùng đó.
fn boot_letter(disk: &usb::UsbDisk, label: &str) -> Result<String> {
    // Ưu tiên khớp theo nhãn vì ổ có thể có thêm phân vùng DATA.
    let by_label = disk
        .volumes
        .iter()
        .find(|v| v.label.as_deref().map(|l| l.eq_ignore_ascii_case(label)).unwrap_or(false))
        .and_then(|v| v.letter.clone());

    by_label
        .or_else(|| disk.volumes.iter().find_map(|v| v.letter.clone()))
        .ok_or_else(|| {
            AppError::new(
                "no_letter",
                "Không tìm thấy phân vùng có ký tự ổ đĩa trên USB. Hãy chạy lại bước Format.",
            )
        })
}

/// Bước 1: xoá và chia lại phân vùng ổ USB.
///
/// Tách hẳn khỏi bước ghi vì đây là thao tác *duy nhất* làm mất dữ liệu. Gộp
/// chung vào một nút "tạo USB" khiến người dùng khó thấy chính xác lúc nào dữ
/// liệu bị xoá; tách ra thì việc xoá có màn hình xác nhận của riêng nó.
pub async fn format_usb<F>(req: FormatRequest, mut on_progress: F) -> Result<FormatResult>
where
    F: FnMut(WriteProgress) + Send,
{
    let mut emit = |stage: &'static str, idx: u32, pct: f64, msg: String| {
        on_progress(WriteProgress {
            stage: stage.to_string(),
            stage_index: idx,
            total_stages: FORMAT_STAGES,
            percent: pct,
            message: msg,
            detail: None,
        });
    };

    emit("check", 1, 0.0, "Đang kiểm tra ổ đĩa…".into());
    let disk = guard(req.disk_number, &req.confirm_token, None, WINDOWS_MIN_USB).await?;
    emit(
        "check",
        1,
        100.0,
        format!(
            "Sẽ xoá sạch {} ({:.1} GB).",
            disk.model,
            disk.size as f64 / 1024.0 / 1024.0 / 1024.0
        ),
    );

    emit("partition", 2, 0.0, "Đang xoá và chia lại phân vùng…".into());

    let max_boot = req
        .scheme
        .max_boot_bytes()
        .map(|b| b.to_string())
        .unwrap_or_else(|| "0".into());

    let script = SCRIPT_PARTITION
        .replace("%%DISK%%", &req.disk_number.to_string())
        .replace("%%STYLE%%", req.scheme.style())
        .replace("%%FS%%", req.scheme.filesystem())
        .replace("%%LABEL%%", &escape(&req.label))
        .replace("%%MAXBOOT%%", &max_boot);

    let letter = ps::run(&script).await?.trim().to_string();
    if letter.is_empty() {
        return Err(AppError::new(
            "format_failed",
            "Chia phân vùng xong nhưng Windows không gán được ký tự ổ đĩa.",
        ));
    }

    // Đọc lại trạng thái ổ để biết có phân vùng dữ liệu phụ hay không.
    let after = usb::list().await?;
    let has_data = after
        .iter()
        .find(|d| d.number == req.disk_number)
        .map(|d| d.volumes.len() > 1)
        .unwrap_or(false);

    emit(
        "done",
        2,
        100.0,
        format!("Đã format xong. Phân vùng {} nằm ở ổ {letter}:", req.scheme.filesystem()),
    );

    Ok(FormatResult {
        drive_letter: letter,
        filesystem: req.scheme.filesystem().to_string(),
        partition_style: req.scheme.style().to_string(),
        has_data_partition: has_data,
    })
}

/// Bước 2: chép bộ cài lên ổ USB đã format.
pub async fn write_iso<F>(req: WriteRequest, mut on_progress: F) -> Result<()>
where
    F: FnMut(WriteProgress) + Send,
{
    let mut emit = |stage: &'static str, idx: u32, pct: f64, msg: String, detail: Option<String>| {
        on_progress(WriteProgress {
            stage: stage.to_string(),
            stage_index: idx,
            total_stages: WRITE_STAGES,
            percent: pct,
            message: msg,
            detail,
        });
    };

    // --- 1. Kiểm tra an toàn -------------------------------------------
    emit("check", 1, 0.0, "Đang kiểm tra ổ đĩa và file ISO…".into(), None);
    let disk = guard(req.disk_number, &req.confirm_token, Some(&req.iso_path), WINDOWS_MIN_USB).await?;
    let dst_letter = boot_letter(&disk, &req.label)?;
    let iso = inspect_iso(&req.iso_path).await?;

    if req.scheme.needs_wim_split()
        && iso.install_image.as_deref().map(|p| p.ends_with(".esd")).unwrap_or(false)
        && iso.needs_split
    {
        return Err(AppError::new(
            "esd_too_big",
            "File install.esd trong ISO này lớn hơn 4 GB nên không nằm vừa phân vùng FAT32, \
             và DISM không tách được file .esd. Hãy quay lại bước Format và chọn \"MBR + NTFS\".",
        ));
    }

    emit("check", 1, 100.0, format!("Sẽ chép lên ổ {dst_letter}:"), None);

    // --- 2. Gắn file ISO -----------------------------------------------
    emit("mount", 2, 0.0, "Đang gắn file ISO…".into(), None);
    let src_letter: String = ps::run(&SCRIPT_MOUNT.replace("%%ISO%%", &escape(&req.iso_path)))
        .await?
        .trim()
        .to_string();
    if src_letter.is_empty() {
        return Err(AppError::new("mount_failed", "Không gắn được file ISO. File có thể bị hỏng."));
    }

    // Từ đây trở đi mọi lỗi đều phải tháo ISO ra trước khi thoát.
    let result = copy_inner(&req, &iso, &src_letter, &dst_letter, &mut emit).await;
    let _ = ps::run(&SCRIPT_DISMOUNT.replace("%%ISO%%", &escape(&req.iso_path))).await;
    result
}

async fn copy_inner<F>(
    req: &WriteRequest,
    iso: &IsoInfo,
    src_letter: &str,
    dst_letter: &str,
    emit: &mut F,
) -> Result<()>
where
    F: FnMut(&'static str, u32, f64, String, Option<String>) + Send,
{
    emit("mount", 2, 100.0, format!("Đã gắn ISO vào ổ {src_letter}:"), None);

    // --- 3. Chép bộ cài -------------------------------------------------
    emit("copy", 3, 0.0, "Đang chép bộ cài sang USB…".into(), None);

    let skip_wim = req.scheme.needs_wim_split() && iso.needs_split;
    let copy_script = SCRIPT_COPY
        .replace("%%SRC%%", src_letter)
        .replace("%%DST%%", dst_letter)
        .replace("%%SKIPWIM%%", if skip_wim { "$true" } else { "$false" });

    let mut copied: u64 = 0;
    let mut total: u64 = 1;
    let mut last_emit = std::time::Instant::now();

    ps::run_streaming(&copy_script, |line| {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("GWU:TOTAL ") {
            total = rest.trim().parse().unwrap_or(1).max(1);
        } else if let Some(rest) = line.strip_prefix("GWU:P ") {
            let mut parts = rest.splitn(2, ' ');
            copied = parts.next().and_then(|v| v.parse().ok()).unwrap_or(copied);
            let name = parts.next().unwrap_or("").to_string();
            // Giới hạn nhịp báo để không dội hàng nghìn sự kiện lên giao diện.
            if last_emit.elapsed().as_millis() >= 120 {
                last_emit = std::time::Instant::now();
                emit(
                    "copy",
                    3,
                    copied as f64 / total as f64 * 100.0,
                    format!("Đang chép bộ cài · {} / {}", human(copied), human(total)),
                    Some(name),
                );
            }
        }
    })
    .await?;

    emit("copy", 3, 100.0, "Đã chép xong bộ cài.".into(), None);

    // --- 4. Tách install.wim nếu vượt giới hạn FAT32 --------------------
    if skip_wim {
        emit("split", 4, 0.0, "File install.wim lớn hơn 4 GB — đang tách nhỏ…".into(), None);
        let split_script = SCRIPT_SPLIT
            .replace("%%SRC%%", src_letter)
            .replace("%%DST%%", dst_letter);

        let mut last_split = std::time::Instant::now();
        ps::run_streaming(&split_script, |line| {
            let Some((written, total)) = parse_pair(line, "GWU:SPLIT ") else { return };
            if last_split.elapsed().as_millis() >= 300 {
                last_split = std::time::Instant::now();
                emit(
                    "split",
                    4,
                    written as f64 / total.max(1) as f64 * 100.0,
                    format!("Đang tách install.wim · {} / {}", human(written), human(total)),
                    None,
                );
            }
        })
        .await?;
        emit("split", 4, 100.0, "Đã tách xong install.wim.".into(), None);
    } else {
        emit("split", 4, 100.0, "Không cần tách install.wim.".into(), None);
    }

    // --- 5. Ghi mã khởi động --------------------------------------------
    emit("boot", 5, 0.0, "Đang ghi mã khởi động…".into(), None);
    if req.scheme.style() == "MBR" {
        let boot_script = SCRIPT_BOOTSECT
            .replace("%%SRC%%", src_letter)
            .replace("%%DST%%", dst_letter);
        // Máy chỉ khởi động UEFI vẫn dùng được USB dù bước này thất bại, nên lỗi
        // ở đây chỉ là cảnh báo chứ không huỷ cả quá trình.
        match ps::run(&boot_script).await {
            Ok(_) => emit("boot", 5, 100.0, "Đã ghi mã khởi động cho BIOS cũ.".into(), None),
            Err(e) => emit(
                "boot",
                5,
                100.0,
                "USB đã sẵn sàng cho máy UEFI, nhưng không ghi được mã khởi động BIOS cũ.".into(),
                Some(e.message),
            ),
        }
    } else {
        emit("boot", 5, 100.0, "Chuẩn GPT/UEFI không cần ghi mã khởi động riêng.".into(), None);
    }

    // --- 6. File trả lời tự động ----------------------------------------
    //
    // Windows Setup tự tìm autounattend.xml ở thư mục gốc của thiết bị rời, nên
    // chỉ cần đặt file vào đúng chỗ là xong — không phải sửa gì trong bộ cài.
    emit("unattend", 6, 0.0, "Đang ghi thiết lập cài đặt tự động…".into(), None);
    match crate::unattend::generate(&req.unattend) {
        Some(xml) => {
            let path = format!("{dst_letter}:\\autounattend.xml");
            match std::fs::write(&path, xml) {
                Ok(_) => emit(
                    "unattend",
                    6,
                    100.0,
                    "Đã ghi autounattend.xml — máy sẽ bỏ qua các màn hình hỏi đáp ban đầu.".into(),
                    None,
                ),
                // Không ghi được file này thì USB vẫn cài được bình thường, chỉ
                // là phải bấm qua các bước thủ công. Không đáng huỷ cả quá trình.
                Err(e) => emit(
                    "unattend",
                    6,
                    100.0,
                    "USB đã sẵn sàng, nhưng không ghi được file cài đặt tự động.".into(),
                    Some(e.to_string()),
                ),
            }
        }
        None => emit("unattend", 6, 100.0, "Không dùng cài đặt tự động.".into(), None),
    }

    emit("done", 6, 100.0, "USB cài Windows đã sẵn sàng.".into(), Some(dst_letter.to_string()));
    Ok(())
}

/// Đọc dòng dạng `<tiền tố><số> <số>` mà các script tiến trình in ra.
fn parse_pair(line: &str, prefix: &str) -> Option<(u64, u64)> {
    let rest = line.trim().strip_prefix(prefix)?;
    let mut it = rest.split_whitespace();
    Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
}

/// Dung lượng dạng chuỗi ngắn để hiện kèm thanh tiến trình.
fn human(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.1} {}", UNITS[i]).replace('.', ",")
}

// ---------------------------------------------------------------------------
// Các đoạn script PowerShell. Tham số được thay bằng %%TÊN%% thay vì format!
// để không phải nhân đôi mọi dấu ngoặc nhọn của PowerShell.
// ---------------------------------------------------------------------------

const SCRIPT_MOUNT: &str = r#"
$img = Mount-DiskImage -ImagePath '%%ISO%%' -PassThru -ErrorAction Stop
Start-Sleep -Milliseconds 700
$vol = $img | Get-Volume
if (-not $vol -or -not $vol.DriveLetter) { throw 'Không lấy được ký tự ổ đĩa của file ISO' }
Write-Output ([string]$vol.DriveLetter)
"#;

const SCRIPT_DISMOUNT: &str = r#"
Dismount-DiskImage -ImagePath '%%ISO%%' -ErrorAction SilentlyContinue | Out-Null
"#;

const SCRIPT_INSPECT: &str = r#"
$img = Mount-DiskImage -ImagePath '%%ISO%%' -PassThru -ErrorAction Stop
try {
  Start-Sleep -Milliseconds 700
  $l = ($img | Get-Volume).DriveLetter
  $root = "$l`:\"

  $img_path = $null; $img_size = 0
  foreach ($n in @('sources\install.wim','sources\install.esd')) {
    $p = Join-Path $root $n
    if (Test-Path $p) { $img_path = $n; $img_size = (Get-Item $p).Length; break }
  }

  $editions = @()
  if ($img_path) {
    try {
      $editions = @(Get-WindowsImage -ImagePath (Join-Path $root $img_path) -ErrorAction Stop |
                    ForEach-Object { [string]$_.ImageName })
    } catch {}
  }

  $arch = 'x64'
  if (Test-Path (Join-Path $root 'efi\boot\bootaa64.efi')) { $arch = 'arm64' }
  elseif (Test-Path (Join-Path $root 'efi\boot\bootia32.efi') -and -not (Test-Path (Join-Path $root 'efi\boot\bootx64.efi'))) { $arch = 'x86' }

  $uefi = Test-Path (Join-Path $root 'efi\boot')

  $out = [pscustomobject]@{
    install_image      = $img_path
    install_image_size = [uint64]$img_size
    editions           = $editions
    architecture       = $arch
    bootable_uefi      = [bool]$uefi
  }
  ConvertTo-Json -InputObject $out -Depth 4 -Compress
} finally {
  Dismount-DiskImage -ImagePath '%%ISO%%' -ErrorAction SilentlyContinue | Out-Null
}
"#;

const SCRIPT_PARTITION: &str = r#"
$n = %%DISK%%
$disk = Get-Disk -Number $n -ErrorAction Stop

# Kiểm tra lại ngay trong PowerShell: nếu có gì đó vừa thay đổi giữa hai lần
# kiểm tra thì thà dừng còn hơn xoá nhầm ổ cứng.
if ($disk.IsSystem)   { throw 'Ổ này là ổ hệ thống — từ chối ghi.' }
if ($disk.IsBoot)     { throw 'Ổ này là ổ khởi động — từ chối ghi.' }
if ($disk.BusType -ne 'USB') { throw ('Ổ này không phải USB (BusType=' + $disk.BusType + ') — từ chối ghi.') }

Set-Disk -Number $n -IsReadOnly $false -ErrorAction SilentlyContinue
Set-Disk -Number $n -IsOffline  $false -ErrorAction SilentlyContinue

Get-Partition -DiskNumber $n -ErrorAction SilentlyContinue |
  Remove-Partition -Confirm:$false -ErrorAction SilentlyContinue

Clear-Disk -Number $n -RemoveData -RemoveOEM -Confirm:$false -ErrorAction Stop
Initialize-Disk -Number $n -PartitionStyle '%%STYLE%%' -ErrorAction Stop
Start-Sleep -Milliseconds 500

$maxBoot = [uint64]%%MAXBOOT%%
$free = (Get-Disk -Number $n).LargestFreeExtent
$capped = ($maxBoot -gt 0 -and $free -gt $maxBoot)

if ($capped) {
  $part = New-Partition -DiskNumber $n -Size $maxBoot -AssignDriveLetter -ErrorAction Stop
} else {
  $part = New-Partition -DiskNumber $n -UseMaximumSize -AssignDriveLetter -ErrorAction Stop
}

if ('%%STYLE%%' -eq 'MBR') {
  try { $part | Set-Partition -IsActive $true -ErrorAction Stop } catch {}
}

Start-Sleep -Milliseconds 500
Format-Volume -Partition $part -FileSystem '%%FS%%' -NewFileSystemLabel '%%LABEL%%' `
              -Confirm:$false -Force -ErrorAction Stop | Out-Null

# Phần dung lượng dư sau khi cắt FAT32 ở mốc 32 GB được để thành ổ dữ liệu,
# tránh lãng phí vài chục GB trên các USB dung lượng lớn.
if ($capped -and (Get-Disk -Number $n).LargestFreeExtent -gt 1GB) {
  try {
    $p2 = New-Partition -DiskNumber $n -UseMaximumSize -AssignDriveLetter -ErrorAction Stop
    Format-Volume -Partition $p2 -FileSystem NTFS -NewFileSystemLabel 'DATA' `
                  -Confirm:$false -Force -ErrorAction Stop | Out-Null
  } catch {}
}

Write-Output ([string]$part.DriveLetter)
"#;

const SCRIPT_COPY: &str = r#"
$src = '%%SRC%%:\'
$dst = '%%DST%%:\'
$skipWim = %%SKIPWIM%%

# File nhỏ hơn ngưỡng này chép một phát cho nhanh; file lớn hơn phải chép theo
# từng khối, nếu không thanh tiến trình sẽ đứng im hàng phút khi gặp install.wim
# 5 GB — người dùng tưởng ứng dụng treo.
$BIG   = 64MB
$CHUNK = 4MB

$files = @(Get-ChildItem -Path $src -Recurse -File -Force -ErrorAction SilentlyContinue)
$total = 0
foreach ($f in $files) { if (-not ($skipWim -and $f.Name -ieq 'install.wim')) { $total += $f.Length } }
Write-Output ("GWU:TOTAL " + $total)

$done = 0
$sw = [System.Diagnostics.Stopwatch]::StartNew()
$lastTick = 0

foreach ($f in $files) {
  $rel = $f.FullName.Substring($src.Length)
  if ($skipWim -and $f.Name -ieq 'install.wim') { continue }

  $target = Join-Path $dst $rel
  $dir = Split-Path $target -Parent
  if (-not (Test-Path -LiteralPath $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }

  if ($f.Length -lt $BIG) {
    [System.IO.File]::Copy($f.FullName, $target, $true)
    $done += $f.Length
  } else {
    $in  = [System.IO.File]::OpenRead($f.FullName)
    $out = [System.IO.File]::Create($target)
    try {
      $buf = New-Object byte[] $CHUNK
      while (($n = $in.Read($buf, 0, $buf.Length)) -gt 0) {
        $out.Write($buf, 0, $n)
        $done += $n
        if (($sw.ElapsedMilliseconds - $lastTick) -ge 400) {
          $lastTick = $sw.ElapsedMilliseconds
          Write-Output ("GWU:P " + $done + " " + $rel)
          [Console]::Out.Flush()
        }
      }
      $out.Flush()
    } finally {
      $out.Dispose()
      $in.Dispose()
    }
  }
  Write-Output ("GWU:P " + $done + " " + $rel)
}
[Console]::Out.Flush()
"#;

const SCRIPT_SPLIT: &str = r#"
$wim = '%%SRC%%:\sources\install.wim'
$dir = '%%DST%%:\sources'
$out = Join-Path $dir 'install.swm'
if (-not (Test-Path -LiteralPath $wim)) { throw 'Không tìm thấy sources\install.wim trong file ISO.' }

$total = (Get-Item -LiteralPath $wim).Length

# DISM in tiến trình bằng ký tự về đầu dòng (\r) chứ không xuống dòng, nên đọc
# stdout theo dòng sẽ không nhận được gì cho tới lúc nó chạy xong. Thay vì cố
# bóc tách con số từ đó, ta chạy DISM ở nền rồi tự đo bằng tổng dung lượng các
# mảnh .swm đã ghi ra — vừa chính xác hơn, vừa không phụ thuộc vào cách DISM
# trình bày.
# 3800 MB mỗi mảnh: dưới trần 4 GB của FAT32, và Windows Setup tự nhận diện
# chuỗi install.swm / install2.swm / …
$argLine = '/English /Split-Image /ImageFile:"{0}" /SWMFile:"{1}" /FileSize:3800' -f $wim, $out
$p = Start-Process -FilePath 'dism.exe' -ArgumentList $argLine -NoNewWindow -PassThru

while (-not $p.HasExited) {
  Start-Sleep -Milliseconds 700
  $written = 0
  Get-ChildItem -LiteralPath $dir -Filter 'install*.swm' -ErrorAction SilentlyContinue |
    ForEach-Object { $written += $_.Length }
  Write-Output ("GWU:SPLIT " + $written + " " + $total)
  [Console]::Out.Flush()
}
$p.WaitForExit()

if ($p.ExitCode -ne 0) { throw ('DISM tách file install.wim thất bại, mã lỗi ' + $p.ExitCode) }
Write-Output ("GWU:SPLIT " + $total + " " + $total)
"#;

const SCRIPT_BOOTSECT: &str = r#"
$bs = '%%SRC%%:\boot\bootsect.exe'
if (-not (Test-Path -LiteralPath $bs)) { throw 'ISO không kèm bootsect.exe nên bỏ qua bước này.' }
& $bs /nt60 '%%DST%%:' /mbr 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) { throw ('bootsect trả về mã lỗi ' + $LASTEXITCODE) }
Write-Output 'ok'
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fat32_schemes_cap_the_boot_partition() {
        assert_eq!(
            PartitionScheme::GptFat32.max_boot_bytes(),
            Some(32 * 1024 * 1024 * 1024)
        );
        assert_eq!(PartitionScheme::MbrNtfs.max_boot_bytes(), None);
    }

    #[test]
    fn only_fat32_needs_the_wim_split() {
        assert!(PartitionScheme::GptFat32.needs_wim_split());
        assert!(PartitionScheme::MbrFat32.needs_wim_split());
        assert!(!PartitionScheme::MbrNtfs.needs_wim_split());
    }

    #[test]
    fn progress_lines_are_parsed() {
        assert_eq!(parse_pair("GWU:SPLIT 1024 4096", "GWU:SPLIT "), Some((1024, 4096)));
        assert_eq!(parse_pair("  GWU:SPLIT 0 500  ", "GWU:SPLIT "), Some((0, 500)));
        assert_eq!(parse_pair("Deployment Image Servicing", "GWU:SPLIT "), None);
        // Dòng của bước chép có tên file ở vị trí thứ hai, không phải số —
        // không được nhận nhầm thành tiến trình tách.
        assert_eq!(parse_pair("GWU:SPLIT 10 sources\\boot.wim", "GWU:SPLIT "), None);
    }

    #[test]
    fn sizes_are_formatted_for_vietnamese_readers() {
        assert_eq!(human(0), "0,0 B");
        assert_eq!(human(5 * 1024 * 1024 * 1024), "5,0 GB");
    }

    #[test]
    fn quotes_in_paths_cannot_break_out_of_the_script() {
        assert_eq!(escape("D:\\it's here\\win.iso"), "D:\\it''s here\\win.iso");
    }
}

// ---------------------------------------------------------------------------
// Ghi ảnh đĩa nguyên khối (cho ISO Linux)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawWriteRequest {
    pub disk_number: u32,
    pub iso_path: String,
    pub confirm_token: String,
}

/// Ghi nguyên khối file ISO ra ổ USB — tương đương `dd` trên Linux.
///
/// ISO của các distro là *hybrid ISO*: bảng phân vùng và mã khởi động nằm ngay
/// trong chính file ảnh đĩa. Chép từng file ra một phân vùng FAT32 như cách làm
/// với bộ cài Windows sẽ hỏng, vì bootloader (isolinux/GRUB) trông chờ đúng bố
/// cục ISO9660 và đúng nhãn volume mà nó được dựng cùng — máy sẽ báo không tìm
/// thấy thiết bị khởi động, hoặc dừng giữa chừng ở initramfs.
///
/// Vì ghi từ byte 0 nên thao tác này xoá luôn bảng phân vùng cũ: không cần và
/// cũng không được format trước, bước Format vì thế không có trong luồng Linux.
pub async fn write_image_raw<F>(req: RawWriteRequest, mut on_progress: F) -> Result<()>
where
    F: FnMut(WriteProgress) + Send,
{
    let mut emit = |stage: &'static str, idx: u32, pct: f64, msg: String, detail: Option<String>| {
        on_progress(WriteProgress {
            stage: stage.to_string(),
            stage_index: idx,
            total_stages: RAW_WRITE_STAGES,
            percent: pct,
            message: msg,
            detail,
        });
    };

    // --- 1. Kiểm tra an toàn ---------------------------------------------
    emit("check", 1, 0.0, "Đang kiểm tra ổ đĩa và file ảnh…".into(), None);

    let iso_size = std::fs::metadata(&req.iso_path).map(|m| m.len()).unwrap_or(0);
    if iso_size == 0 {
        return Err(AppError::new("no_iso", "Không đọc được file ISO đã chọn."));
    }
    // Ghi nguyên khối nên ổ chỉ cần chứa vừa đúng file ảnh, không cần dư ra như
    // luồng Windows (vốn còn phải tách install.wim ngay trên ổ).
    let disk = guard(req.disk_number, &req.confirm_token, None, iso_size).await?;

    emit(
        "check",
        1,
        100.0,
        format!(
            "Sẽ ghi đè toàn bộ {} ({:.1} GB).",
            disk.model,
            disk.size as f64 / 1024.0 / 1024.0 / 1024.0
        ),
        None,
    );

    // --- 2. Ghi ------------------------------------------------------------
    emit("raw", 2, 0.0, "Đang ghi ảnh đĩa ra USB…".into(), None);

    let script = SCRIPT_RAW_WRITE
        .replace("%%DISK%%", &req.disk_number.to_string())
        .replace("%%ISO%%", &escape(&req.iso_path));

    let mut last_emit = std::time::Instant::now();
    let mut written: u64 = 0;

    ps::run_streaming(&script, |line| {
        let Some((done, total)) = parse_pair(line, "GWU:RAW ") else { return };
        written = done;
        // Giới hạn nhịp báo để không dội hàng nghìn sự kiện lên giao diện.
        if last_emit.elapsed().as_millis() >= 150 {
            last_emit = std::time::Instant::now();
            emit(
                "raw",
                2,
                done as f64 / total.max(1) as f64 * 100.0,
                format!("Đang ghi ảnh đĩa · {} / {}", human(done), human(total)),
                None,
            );
        }
    })
    .await?;

    if written == 0 {
        return Err(AppError::new(
            "raw_write_failed",
            "Không ghi được byte nào ra ổ USB. Hãy chắc chắn ứng dụng đang chạy với quyền quản trị \
             và không có cửa sổ Explorer nào đang mở ổ này.",
        ));
    }

    emit("raw", 2, 100.0, format!("Đã ghi {}.", human(written)), None);

    // --- 3. Đẩy hết bộ đệm ra thiết bị ------------------------------------
    //
    // Windows đệm ghi rất mạnh: rút USB ngay sau khi thanh tiến trình đầy có
    // thể mất vài trăm MB cuối. Bước này gọi lại ổ về trạng thái online, buộc
    // hệ điều hành đọc lại bảng phân vùng mới và xả bộ đệm.
    emit("flush", 3, 0.0, "Đang đẩy dữ liệu còn trong bộ đệm ra USB…".into(), None);
    let _ = ps::run(&SCRIPT_RAW_FLUSH.replace("%%DISK%%", &req.disk_number.to_string())).await;
    emit("flush", 3, 100.0, "USB đã sẵn sàng để rút ra.".into(), None);

    Ok(())
}

/// Ghi nguyên khối file ISO ra `\\.\PHYSICALDRIVE<n>`.
///
/// Ba việc phải làm đúng thứ tự, thiếu bước nào cũng hỏng:
///
/// 1. `Clear-Disk` để Windows nhả mọi khoá volume đang giữ trên ổ. Không xoá
///    trước thì lệnh mở thiết bị bị từ chối truy cập.
/// 2. Đưa ổ về offline, nếu không Windows sẽ thấy phân vùng mới xuất hiện giữa
///    chừng và gắn nó vào trong lúc đang ghi dở.
/// 3. Ghi theo bội số sector. Ghi thẳng ra thiết bị không cho phép độ dài lẻ,
///    nên khối cuối phải được đệm 0 cho tròn — với hybrid ISO thì phần đuôi vốn
///    đã là vùng đệm nên đệm thêm không ảnh hưởng gì.
const SCRIPT_RAW_WRITE: &str = r#"
$ErrorActionPreference = 'Stop'
$n = %%DISK%%
$iso = '%%ISO%%'

$disk = Get-Disk -Number $n -ErrorAction Stop
if ($disk.BusType -ne 'USB') { throw 'Ổ đĩa này không phải USB — đã dừng.' }
if ($disk.IsSystem -or $disk.IsBoot) { throw 'Ổ đĩa này là ổ hệ thống — đã dừng.' }

Clear-Disk -Number $n -RemoveData -RemoveOEM -Confirm:$false -ErrorAction SilentlyContinue
Set-Disk -Number $n -IsOffline $true -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500

$src = $null
$dst = $null
try {
  $src = [System.IO.File]::OpenRead($iso)
  $total = $src.Length
  $dst = New-Object System.IO.FileStream(
    "\\.\PHYSICALDRIVE$n",
    [System.IO.FileMode]::Open,
    [System.IO.FileAccess]::Write,
    [System.IO.FileShare]::ReadWrite)

  $size = 4194304
  $buf = New-Object byte[] $size
  $done = 0L
  $mark = 0L

  while (($read = $src.Read($buf, 0, $size)) -gt 0) {
    $len = $read
    if ($len % 512 -ne 0) {
      $len = [int]([math]::Ceiling($len / 512.0)) * 512
      [Array]::Clear($buf, $read, $len - $read)
    }
    $dst.Write($buf, 0, $len)
    $done += $read
    if (($done - $mark) -ge 16777216) {
      Write-Output "GWU:RAW $done $total"
      $mark = $done
    }
  }
  $dst.Flush($true)
  Write-Output "GWU:RAW $total $total"
}
finally {
  if ($dst) { $dst.Dispose() }
  if ($src) { $src.Dispose() }
}
"#;

/// Đưa ổ về online và buộc Windows đọc lại bảng phân vùng vừa ghi.
const SCRIPT_RAW_FLUSH: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
$n = %%DISK%%
Set-Disk -Number $n -IsOffline $false
Start-Sleep -Milliseconds 400
Update-Disk -Number $n
"#;
