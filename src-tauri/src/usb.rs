//! Nhận diện ổ USB đang cắm và theo dõi thay đổi theo thời gian thực.

use crate::error::Result;
use crate::ps;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UsbVolume {
    pub letter: Option<String>,
    pub label: Option<String>,
    pub fs: Option<String>,
    pub size: u64,
    pub free: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UsbDisk {
    pub number: u32,
    pub model: String,
    pub serial: Option<String>,
    pub size: u64,
    pub partition_style: String,
    pub is_readonly: bool,
    pub is_boot: bool,
    pub is_system: bool,
    pub bus_type: String,
    pub device_path: String,
    pub volumes: Vec<UsbVolume>,
}

impl UsbDisk {
    /// Ổ có an toàn để ghi đè không. Ổ hệ thống/ổ boot bị loại tuyệt đối — đây
    /// là hàng rào cuối cùng ngăn người dùng xoá nhầm ổ cứng đang chạy Windows.
    pub fn is_writable_target(&self) -> bool {
        !self.is_boot && !self.is_system && !self.is_readonly && self.bus_type == "USB"
    }

    /// Dung lượng tối thiểu để chứa được bộ cài Windows 11 hiện nay.
    pub fn is_large_enough(&self) -> bool {
        self.size >= 8 * 1024 * 1024 * 1024
    }
}

/// Đoạn script dựng danh sách ổ USB. Dùng chung cho cả lần quét một lần lẫn
/// vòng lặp theo dõi, nên chỉ cần sửa một chỗ khi muốn lấy thêm thông tin.
const COLLECT: &str = r#"
$disks = @()
foreach ($d in (Get-Disk -ErrorAction SilentlyContinue | Where-Object { $_.BusType -eq 'USB' })) {
  $vols = @()
  foreach ($p in (Get-Partition -DiskNumber $d.Number -ErrorAction SilentlyContinue)) {
    $v = $null
    try { $v = Get-Volume -Partition $p -ErrorAction SilentlyContinue } catch {}
    if ($v) {
      $vols += [pscustomobject]@{
        letter = $(if ($v.DriveLetter) { [string]$v.DriveLetter } else { $null })
        label  = [string]$v.FileSystemLabel
        fs     = [string]$v.FileSystem
        size   = [uint64]$(if ($v.Size) { $v.Size } else { 0 })
        free   = [uint64]$(if ($v.SizeRemaining) { $v.SizeRemaining } else { 0 })
      }
    }
  }
  $phys = Get-CimInstance Win32_DiskDrive -Filter "Index=$($d.Number)" -ErrorAction SilentlyContinue | Select-Object -First 1
  $disks += [pscustomobject]@{
    number          = [int]$d.Number
    model           = [string]$(if ($d.FriendlyName) { $d.FriendlyName } else { 'Ổ USB' })
    serial          = [string]$d.SerialNumber
    size            = [uint64]$(if ($d.Size) { $d.Size } else { 0 })
    partition_style = [string]$d.PartitionStyle
    is_readonly     = [bool]$d.IsReadOnly
    is_boot         = [bool]$d.IsBoot
    is_system       = [bool]$d.IsSystem
    bus_type        = [string]$d.BusType
    device_path     = [string]$(if ($phys) { $phys.DeviceID } else { "\\.\PHYSICALDRIVE$($d.Number)" })
    volumes         = $vols
  }
}
"#;

/// Quét một lần, dùng khi mở app hoặc khi người dùng bấm làm mới.
pub async fn list() -> Result<Vec<UsbDisk>> {
    if cfg!(not(windows)) {
        return Ok(mock_disks());
    }
    let script = format!("{COLLECT}\nConvertTo-Json -InputObject @($disks) -Depth 5 -Compress");
    ps::run_json(&script).await
}

/// Vòng lặp theo dõi cắm/rút.
///
/// Thay vì gọi PowerShell lại mỗi vài giây (tốn ~40MB RAM và 300ms mỗi lần khởi
/// động tiến trình), ta giữ đúng một tiến trình PowerShell chạy suốt vòng đời
/// ứng dụng và để nó tự in JSON theo nhịp. Rust chỉ việc đọc từng dòng.
pub async fn watch<F>(interval_secs: u32, mut on_change: F) -> Result<()>
where
    F: FnMut(Vec<UsbDisk>) + Send,
{
    if cfg!(not(windows)) {
        on_change(mock_disks());
        return Ok(());
    }

    let script = format!(
        r#"
while ($true) {{
{COLLECT}
  Write-Output ('USB ' + (ConvertTo-Json -InputObject @($disks) -Depth 5 -Compress))
  [Console]::Out.Flush()
  Start-Sleep -Seconds {interval_secs}
}}
"#
    );

    let mut last: Option<String> = None;
    ps::run_streaming(&script, |line| {
        let Some(payload) = line.trim().strip_prefix("USB ") else { return };
        // Chỉ báo lên UI khi danh sách thực sự đổi, tránh render lại vô ích.
        if last.as_deref() == Some(payload) {
            return;
        }
        last = Some(payload.to_string());
        if let Ok(disks) = serde_json::from_str::<Vec<UsbDisk>>(payload) {
            on_change(disks);
        }
    })
    .await
}

/// Dữ liệu giả lập để phát triển giao diện trên máy không phải Windows.
fn mock_disks() -> Vec<UsbDisk> {
    vec![UsbDisk {
        number: 2,
        model: "SanDisk Ultra USB 3.0".into(),
        serial: Some("4C530001010101".into()),
        size: 32 * 1024 * 1024 * 1024,
        partition_style: "MBR".into(),
        is_readonly: false,
        is_boot: false,
        is_system: false,
        bus_type: "USB".into(),
        device_path: "\\\\.\\PHYSICALDRIVE2".into(),
        volumes: vec![UsbVolume {
            letter: Some("E".into()),
            label: Some("USB DRIVE".into()),
            fs: Some("FAT32".into()),
            size: 32_000_000_000,
            free: 31_000_000_000,
        }],
    }]
}
