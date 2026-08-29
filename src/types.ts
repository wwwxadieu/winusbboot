/** Các kiểu dữ liệu khớp 1-1 với struct Rust ở src-tauri/src. */

export interface UsbVolume {
  letter: string | null;
  label: string | null;
  fs: string | null;
  size: number;
  free: number;
}

export interface UsbDisk {
  number: number;
  model: string;
  serial: string | null;
  size: number;
  partition_style: string;
  is_readonly: boolean;
  is_boot: boolean;
  is_system: boolean;
  bus_type: string;
  device_path: string;
  volumes: UsbVolume[];
}

export interface CpuInfo {
  name: string;
  manufacturer: string;
  cores: number;
  threads: number;
  max_clock_mhz: number;
  address_width: number;
  architecture: number;
}

export interface MemoryModule { capacity: number; speed: number; type: number; slot: string }
export interface GpuInfo {
  name: string;
  vram: number;
  driver: string;
  /** ISO YYYY-MM-DD. Căn cứ để đoán driver có đạt WDDM 2.0 không. */
  driver_date: string | null;
}

export interface DisplayInfo {
  width: number;
  height: number;
  diagonal_inches: number | null;
  bits_per_pixel: number;
}
export interface TpmInfo {
  present: boolean;
  version: string | null;
  enabled: boolean;
  activated: boolean;
  /** "wmi" (cần quyền quản trị) | "device" (Device Manager) | "none" */
  source: string;
}
export interface SystemDiskInfo {
  size: number;
  free: number;
  media_type: string | null;
  partition_style: string | null;
}
export interface OsInfo { caption: string; version: string; build: string; architecture: string }

export interface HardwareReport {
  manufacturer: string;
  model: string;
  bios_version: string;
  cpu: CpuInfo;
  total_ram: number;
  memory_modules: MemoryModule[];
  memory_slots: number;
  gpus: GpuInfo[];
  display: DisplayInfo;
  tpm: TpmInfo;
  secure_boot: boolean | null;
  /** "uefi-api" | "registry" | "none" */
  secure_boot_source: string;
  firmware: string;
  elevated: boolean;
  system_disk: SystemDiskInfo;
  os: OsInfo;
}

export type CpuSupport = "supported" | "unsupported" | "unknown";
export interface CpuVerdict {
  vendor: string;
  family: string;
  generation: string | null;
  support: CpuSupport;
  reason: string;
}

export type CheckStatus = "pass" | "fixable" | "fail" | "unknown";
export interface Check {
  id: string;
  /** Nhóm để gom lại khi hiển thị. */
  group: string;
  label: string;
  /** Giá trị đọc được trên máy này. */
  value: string;
  /** Ngưỡng mà Windows 11 đòi hỏi. */
  requirement: string;
  status: CheckStatus;
  hint: string | null;
  /** Không đạt thì trình cài đặt Windows 11 chặn lại. */
  blocking: boolean;
}

export interface CheckSummary {
  passed: number;
  fixable: number;
  failed: number;
  unknown: number;
  total: number;
  windows11_ready: boolean;
}

export type Verdict = "recommended" | "needs_setup" | "needs_bypass" | "blocked";

export interface WindowsRelease {
  id: string;
  name: string;
  family: string;
  build: string;
  /** Dạng ISO YYYY-MM-DD. */
  released: string;
  /** Dạng ISO YYYY-MM-DD. Dùng `Candidate.end_of_support_label` để hiển thị. */
  end_of_support: string;
  /** Phiên bản do đồng bộ từ Microsoft phát hiện, yêu cầu phần cứng là suy ra. */
  discovered: boolean;
  requires_tpm2: boolean;
  requires_secure_boot: boolean;
  requires_uefi: boolean;
  requires_cpu_list: boolean;
  min_ram_gb: number;
  min_disk_gb: number;
  tagline: string;
  source: "microsoft_consumer" | "volume_license";
}

export interface Candidate {
  release: WindowsRelease;
  end_of_support_label: string;
  /** Số ngày còn hỗ trợ tính từ hôm nay; số âm là đã quá hạn. */
  days_remaining: number;
  expired: boolean;
  score: number;
  verdict: Verdict;
  pros: string[];
  cons: string[];
  blockers: string[];
}

export type CatalogOrigin = "builtin" | "cache" | "live";

export interface CatalogState {
  releases: WindowsRelease[];
  origin: CatalogOrigin;
  synced_on: string | null;
  note: string | null;
}

export interface Recommendation {
  cpu: CpuVerdict;
  checks: Check[];
  check_summary: CheckSummary;
  candidates: Candidate[];
  best: string;
  summary: string;
  architecture: string;
  edition_hint: string;
  language_hint: string;
  catalog_origin: CatalogOrigin;
  catalog_synced_on: string | null;
  catalog_note: string | null;
}

export interface IsoInfo {
  path: string;
  size: number;
  install_image: string | null;
  install_image_size: number;
  editions: string[];
  architecture: string;
  needs_split: boolean;
  bootable_uefi: boolean;
}

export type PartitionScheme = "gpt_fat32" | "mbr_fat32" | "mbr_ntfs";

export interface FormatRequest {
  disk_number: number;
  scheme: PartitionScheme;
  label: string;
  confirm_token: string;
}

export interface FormatResult {
  drive_letter: string;
  filesystem: string;
  partition_style: string;
  has_data_partition: boolean;
}

export interface LocalAccount {
  name: string;
  password: string;
  auto_logon: boolean;
}

export interface UnattendConfig {
  enabled: boolean;
  language: string;
  keyboard: string;
  timezone: string;
  computer_name: string;
  local_account: LocalAccount | null;
  skip_oobe: boolean;
  bypass_requirements: boolean;
  arch: string;
}

export interface WriteRequest {
  disk_number: number;
  iso_path: string;
  scheme: PartitionScheme;
  label: string;
  confirm_token: string;
  unattend: UnattendConfig;
}

export interface WriteProgress {
  stage: string;
  stage_index: number;
  total_stages: number;
  percent: number;
  message: string;
  detail: string | null;
}

export interface DownloadProgress {
  downloaded: number;
  total: number;
  percent: number;
  speed_bps: number;
  eta_secs: number;
}

export interface DownloadOption {
  label: string;
  url: string;
  language: string;
  architecture: string;
}

export interface AppError { code: string; message: string }
