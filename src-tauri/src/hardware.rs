//! Quét cấu hình máy: CPU, RAM, đĩa, GPU, TPM, Secure Boot, chế độ firmware.

use crate::error::Result;
use crate::ps;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuInfo {
    pub name: String,
    pub manufacturer: String,
    pub cores: u32,
    pub threads: u32,
    pub max_clock_mhz: u32,
    pub address_width: u32,
    pub architecture: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryModule {
    pub capacity: u64,
    pub speed: u32,
    #[serde(rename = "type")]
    pub kind: u32,
    pub slot: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub name: String,
    pub vram: u64,
    pub driver: String,
    /// Ngày driver dạng ISO. Đây là căn cứ khả dĩ nhất để đoán GPU có hỗ trợ
    /// WDDM 2.0 hay không mà không phải chạy dxdiag mất hàng chục giây.
    pub driver_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayInfo {
    pub width: u32,
    pub height: u32,
    /// Đường chéo tính bằng inch, đọc từ EDID của màn hình.
    pub diagonal_inches: Option<f64>,
    pub bits_per_pixel: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TpmInfo {
    pub present: bool,
    pub version: Option<String>,
    pub enabled: bool,
    pub activated: bool,
    /// Nguồn đọc được thông tin:
    /// `wmi` — đọc thẳng từ Win32_Tpm, đầy đủ nhất nhưng đòi quyền quản trị;
    /// `device` — suy từ thiết bị trong Device Manager, chạy được ở quyền thường;
    /// `none` — không thấy chip TPM nào đang hoạt động.
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemDiskInfo {
    pub size: u64,
    pub free: u64,
    pub media_type: Option<String>,
    pub partition_style: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsInfo {
    pub caption: String,
    pub version: String,
    pub build: String,
    pub architecture: String,
}

/// Ảnh chụp toàn bộ phần cứng — đầu vào duy nhất của engine gợi ý.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareReport {
    pub manufacturer: String,
    pub model: String,
    pub bios_version: String,
    pub cpu: CpuInfo,
    pub total_ram: u64,
    pub memory_modules: Vec<MemoryModule>,
    pub memory_slots: u32,
    pub gpus: Vec<GpuInfo>,
    pub display: DisplayInfo,
    pub tpm: TpmInfo,
    /// `None` nghĩa là không truy vấn được — thường vì máy đang chạy chế độ BIOS cũ.
    pub secure_boot: Option<bool>,
    /// `uefi-api`, `registry`, hoặc `none`.
    pub secure_boot_source: String,
    pub firmware: String,
    /// Ứng dụng có đang chạy với quyền quản trị lúc quét hay không. Giao diện
    /// dùng cờ này để giải thích vì sao một số mục chỉ đọc được gián tiếp.
    #[serde(default)]
    pub elevated: bool,
    pub system_disk: SystemDiskInfo,
    pub os: OsInfo,
}

impl HardwareReport {
    pub fn ram_gb(&self) -> f64 {
        self.total_ram as f64 / 1024.0 / 1024.0 / 1024.0
    }
    pub fn free_disk_gb(&self) -> f64 {
        self.system_disk.free as f64 / 1024.0 / 1024.0 / 1024.0
    }
    pub fn is_uefi(&self) -> bool {
        self.firmware.eq_ignore_ascii_case("UEFI")
    }
    pub fn is_64bit(&self) -> bool {
        self.cpu.address_width >= 64
    }
    pub fn is_arm(&self) -> bool {
        // Mã kiến trúc của Win32_Processor: 5 = ARM, 12 = ARM64.
        matches!(self.cpu.architecture, 5 | 12)
    }
    pub fn tpm_major(&self) -> u32 {
        self.tpm
            .version
            .as_deref()
            .and_then(|v| v.split('.').next())
            .and_then(|v| v.trim().parse::<u32>().ok())
            .unwrap_or(0)
    }
    /// TPM 2.0 có mặt *và* đã bật — chỉ "có chip" là chưa đủ để Windows 11 chấp nhận.
    pub fn tpm_ready(&self) -> bool {
        self.tpm.present && self.tpm_major() >= 2 && self.tpm.enabled
    }
}

const SCRIPT: &str = r#"
$cs   = Get-CimInstance Win32_ComputerSystem
$cpu  = Get-CimInstance Win32_Processor | Select-Object -First 1
$os   = Get-CimInstance Win32_OperatingSystem
$bios = Get-CimInstance Win32_BIOS | Select-Object -First 1

# --- TPM ---------------------------------------------------------------
# Win32_Tpm nằm trong namespace bảo mật và CHỈ truy vấn được khi chạy với quyền
# quản trị — chạy quyền thường sẽ ném Access Denied. Vì vậy khi cách này hỏng,
# ta hỏi Device Manager: thiết bị TPM hiện tên kèm luôn phiên bản
# ("Trusted Platform Module 2.0") và mọi tài khoản đều đọc được.
$tpm = [pscustomobject]@{ present=$false; version=$null; enabled=$false; activated=$false; source='none' }

try {
  $t = Get-CimInstance -Namespace 'root\CIMV2\Security\MicrosoftTpm' -ClassName Win32_Tpm -ErrorAction Stop | Select-Object -First 1
  if ($t) {
    $ver = ([string]$t.SpecVersion -split ',')[0].Trim()
    $tpm = [pscustomobject]@{
      present   = $true
      version   = $ver
      enabled   = [bool]$t.IsEnabled_InitialValue
      activated = [bool]$t.IsActivated_InitialValue
      source    = 'wmi'
    }
  }
} catch {}

if ($tpm.source -eq 'none') {
  try {
    $dev = Get-CimInstance Win32_PnPEntity -Filter "Service='TPM'" -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $dev) {
      $dev = Get-CimInstance Win32_PnPEntity -ErrorAction SilentlyContinue |
             Where-Object { $_.Name -like '*Trusted Platform Module*' } | Select-Object -First 1
    }
    if ($dev) {
      # Lấy cụm số cuối trong tên thiết bị, vd "Trusted Platform Module 2.0" -> "2.0"
      $ver = $null
      if ([string]$dev.Name -match '(\d+\.\d+)\s*$') { $ver = $Matches[1] }
      # Thiết bị bị tắt trong BIOS thì không xuất hiện ở đây, nên có mặt nghĩa là đang bật.
      $ok = ([string]$dev.Status -eq 'OK')
      $tpm = [pscustomobject]@{
        present   = $true
        version   = $ver
        enabled   = $ok
        activated = $ok
        source    = 'device'
      }
    }
  } catch {}
}

# --- Secure Boot -------------------------------------------------------
# Confirm-SecureBootUEFI cũng đòi quyền quản trị. Khoá registry dưới đây thì mọi
# tài khoản đều đọc được, và chỉ tồn tại trên máy chạy UEFI — nên không đọc được
# khoá này cũng đồng nghĩa máy đang ở chế độ BIOS cũ.
$sb = $null
$sbSrc = 'none'
try {
  $sb = [bool](Confirm-SecureBootUEFI -ErrorAction Stop)
  $sbSrc = 'uefi-api'
} catch {
  try {
    $k = Get-ItemProperty -Path 'HKLM:\SYSTEM\CurrentControlSet\Control\SecureBoot\State' `
                          -Name 'UEFISecureBootEnabled' -ErrorAction Stop
    $sb = [bool]$k.UEFISecureBootEnabled
    $sbSrc = 'registry'
  } catch { $sb = $null; $sbSrc = 'none' }
}

$firmware = [string]$env:firmware_type
if ([string]::IsNullOrWhiteSpace($firmware)) { $firmware = 'Unknown' }

# --- Đĩa hệ thống ------------------------------------------------------
$letter  = $env:SystemDrive.Substring(0,1)
$dsize=0; $dfree=0; $dmedia=$null; $dstyle=$null
try {
  $v = Get-Volume -DriveLetter $letter -ErrorAction Stop
  $dsize = [uint64]$v.Size; $dfree = [uint64]$v.SizeRemaining
  $p  = Get-Partition -DriveLetter $letter -ErrorAction Stop
  $dk = Get-Disk -Number $p.DiskNumber -ErrorAction Stop
  $dstyle = [string]$dk.PartitionStyle
  $pd = Get-PhysicalDisk -DeviceNumber $p.DiskNumber -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($pd) { $dmedia = [string]$pd.MediaType }
} catch {}

# --- RAM ---------------------------------------------------------------
$mods = @()
foreach ($m in (Get-CimInstance Win32_PhysicalMemory -ErrorAction SilentlyContinue)) {
  $spd = $m.ConfiguredClockSpeed; if (-not $spd) { $spd = $m.Speed }
  $mods += [pscustomobject]@{
    capacity = [uint64]$(if ($m.Capacity) { $m.Capacity } else { 0 })
    speed    = [uint32]$(if ($spd) { $spd } else { 0 })
    type     = [uint32]$(if ($m.SMBIOSMemoryType) { $m.SMBIOSMemoryType } else { 0 })
    slot     = [string]$m.DeviceLocator
  }
}
$slots = 0
try { $slots = [uint32]((Get-CimInstance Win32_PhysicalMemoryArray | Select-Object -First 1).MemoryDevices) } catch {}

# --- GPU và màn hình ---------------------------------------------------
$gpus = @()
$cards = @(Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue)
foreach ($g in $cards) {
  $vram = 0
  if ($g.AdapterRAM -and $g.AdapterRAM -gt 0) { $vram = [uint64]$g.AdapterRAM }
  $dd = $null
  try { if ($g.DriverDate) { $dd = ([datetime]$g.DriverDate).ToString('yyyy-MM-dd') } } catch {}
  $gpus += [pscustomobject]@{
    name        = [string]$g.Name
    vram        = $vram
    driver      = [string]$g.DriverVersion
    driver_date = $dd
  }
}

$disp = [pscustomobject]@{ width=[uint32]0; height=[uint32]0; diagonal_inches=$null; bits_per_pixel=[uint32]0 }
$active = $cards | Where-Object { $_.CurrentHorizontalResolution -gt 0 } | Select-Object -First 1
if ($active) {
  $disp.width          = [uint32]$active.CurrentHorizontalResolution
  $disp.height         = [uint32]$active.CurrentVerticalResolution
  $disp.bits_per_pixel = [uint32]$(if ($active.CurrentBitsPerPixel) { $active.CurrentBitsPerPixel } else { 0 })
}
# Kích thước vật lý nằm trong EDID của màn hình, đơn vị centimet.
try {
  $m = Get-CimInstance -Namespace 'root\wmi' -ClassName WmiMonitorBasicDisplayParams -ErrorAction Stop |
       Where-Object { $_.MaxHorizontalImageSize -gt 0 } | Select-Object -First 1
  if ($m) {
    $w = [double]$m.MaxHorizontalImageSize / 2.54
    $h = [double]$m.MaxVerticalImageSize / 2.54
    $disp.diagonal_inches = [math]::Round([math]::Sqrt($w * $w + $h * $h), 1)
  }
} catch {}

$report = [pscustomobject]@{
  manufacturer   = [string]$cs.Manufacturer
  model          = [string]$cs.Model
  bios_version   = [string]$bios.SMBIOSBIOSVersion
  cpu = [pscustomobject]@{
    name           = ([string]$cpu.Name).Trim()
    manufacturer   = [string]$cpu.Manufacturer
    cores          = [uint32]$(if ($cpu.NumberOfCores) { $cpu.NumberOfCores } else { 0 })
    threads        = [uint32]$(if ($cpu.NumberOfLogicalProcessors) { $cpu.NumberOfLogicalProcessors } else { 0 })
    max_clock_mhz  = [uint32]$(if ($cpu.MaxClockSpeed) { $cpu.MaxClockSpeed } else { 0 })
    address_width  = [uint32]$(if ($cpu.AddressWidth) { $cpu.AddressWidth } else { 0 })
    architecture   = [uint32]$(if ($null -ne $cpu.Architecture) { $cpu.Architecture } else { 0 })
  }
  total_ram       = [uint64]$(if ($cs.TotalPhysicalMemory) { $cs.TotalPhysicalMemory } else { 0 })
  memory_modules  = $mods
  memory_slots    = [uint32]$slots
  gpus            = $gpus
  display         = $disp
  tpm             = $tpm
  secure_boot        = $sb
  secure_boot_source = $sbSrc
  firmware           = $firmware
  system_disk = [pscustomobject]@{
    size            = [uint64]$dsize
    free            = [uint64]$dfree
    media_type      = $dmedia
    partition_style = $dstyle
  }
  os = [pscustomobject]@{
    caption      = ([string]$os.Caption).Trim()
    version      = [string]$os.Version
    build        = [string]$os.BuildNumber
    architecture = [string]$os.OSArchitecture
  }
}
ConvertTo-Json -InputObject $report -Depth 6 -Compress
"#;

pub async fn scan() -> Result<HardwareReport> {
    if cfg!(not(windows)) {
        return Ok(mock_report());
    }
    let mut report: HardwareReport = ps::run_json(SCRIPT).await?;
    report.elevated = crate::ps::is_elevated();
    Ok(report)
}

impl DisplayInfo {
    /// Windows 11 yêu cầu 8 bit cho mỗi kênh màu. Độ sâu 24 hoặc 32 bit trên
    /// mỗi điểm ảnh đều tương ứng 8 bit/kênh (32 bit có thêm kênh alpha).
    pub fn bits_per_channel(&self) -> Option<u32> {
        match self.bits_per_pixel {
            0 => None,
            bpp if bpp >= 24 => Some(8),
            bpp => Some(bpp / 3),
        }
    }
}

impl GpuInfo {
    /// Driver từ 2016 trở đi gần như chắc chắn là WDDM 2.0 — WDDM 2.0 ra cùng
    /// Windows 10 năm 2015, nên driver phát hành sau đó cho một máy đang chạy
    /// Windows 10/11 đều đã đạt. Cách xác định chính xác là chạy dxdiag, nhưng
    /// nó tốn hàng chục giây nên không hợp với một lần quét tức thời.
    pub fn likely_wddm2(&self) -> Option<bool> {
        let year: i32 = self.driver_date.as_deref()?.get(..4)?.parse().ok()?;
        Some(year >= 2016)
    }
}

/// Tên dễ đọc cho mã SMBIOSMemoryType.
pub fn memory_type_name(code: u32) -> &'static str {
    match code {
        20 => "DDR",
        21 => "DDR2",
        24 => "DDR3",
        26 => "DDR4",
        34 | 35 => "DDR5",
        _ => "RAM",
    }
}

fn mock_report() -> HardwareReport {
    HardwareReport {
        manufacturer: "ASUS".into(),
        model: "VivoBook_ASUSLaptop X515EA".into(),
        bios_version: "X515EA.308".into(),
        cpu: CpuInfo {
            name: "11th Gen Intel(R) Core(TM) i5-1135G7 @ 2.40GHz".into(),
            manufacturer: "GenuineIntel".into(),
            cores: 4,
            threads: 8,
            max_clock_mhz: 2400,
            address_width: 64,
            architecture: 9,
        },
        total_ram: 8 * 1024 * 1024 * 1024,
        memory_modules: vec![MemoryModule {
            capacity: 8 * 1024 * 1024 * 1024,
            speed: 3200,
            kind: 26,
            slot: "DIMM 0".into(),
        }],
        memory_slots: 2,
        gpus: vec![GpuInfo {
            name: "Intel(R) Iris(R) Xe Graphics".into(),
            vram: 0,
            driver: "31.0.101.4502".into(),
            driver_date: Some("2024-03-18".into()),
        }],
        display: DisplayInfo {
            width: 1920,
            height: 1080,
            diagonal_inches: Some(15.6),
            bits_per_pixel: 32,
        },
        tpm: TpmInfo {
            present: true,
            version: Some("2.0".into()),
            enabled: true,
            activated: true,
            source: "device".into(),
        },
        secure_boot: Some(true),
        secure_boot_source: "registry".into(),
        firmware: "UEFI".into(),
        elevated: false,
        system_disk: SystemDiskInfo {
            size: 512 * 1024 * 1024 * 1024,
            free: 180 * 1024 * 1024 * 1024,
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
