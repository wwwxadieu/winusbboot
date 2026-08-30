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
  /**
   * File do ứng dụng tự tải về thư mục riêng của nó, nên dọn dẹp được. File
   * người dùng tự chọn thì `false` và ứng dụng không bao giờ xoá.
   */
  managed: boolean;
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
  /**
   * Ngôn ngữ **hiển thị** của Windows, vd `en-US`. Bị giới hạn bởi những gì có
   * trong file ISO — Microsoft không phát hành ISO tiếng Việt nên trường này
   * không bao giờ là `vi-VN`.
   */
  ui_language: string;
  /**
   * Locale cho **định dạng vùng**: ngày tháng, tiền tệ, số. Locale nào cũng
   * được, kể cả `vi-VN` trên một bản Windows tiếng Anh.
   */
  locale: string;
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
  /** Số byte đã xong và tổng số của chặng đang chạy; 0 nếu chặng không đếm byte. */
  done: number;
  total: number;
  /** `0` nghĩa là chưa đo được — giao diện ẩn phần tốc độ đi thay vì hiện "0 B/s". */
  speed_bps: number;
  eta_secs: number;
}

export interface DownloadProgress {
  downloaded: number;
  total: number;
  percent: number;
  speed_bps: number;
  eta_secs: number;
}

// ---------------------------------------------------------------------------
// Driver kèm theo USB
// ---------------------------------------------------------------------------

/** Mức lọc theo nhóm thiết bị. Hẹp hơn thì an toàn hơn, rộng hơn thì đủ hơn. */
export type DriverFilter = "essential" | "recommended" | "all";

/** Một gói driver — đơn vị là cả thư mục chứa file .inf, không phải từng file. */
export interface DriverPackage {
  folder: string;
  name: string;
  infs: string[];
  classes: string[];
  provider: string;
  version: string;
  size: number;
}

export interface DriverSet {
  source: string;
  packages: DriverPackage[];
  total_size: number;
  /** Thư mục chỉ có .exe/.msi — Windows Setup không nhồi được vào ảnh cài. */
  installer_only: number;
}

/** Một thiết bị của máy, kèm kết luận bộ driver đã chọn có phủ được hay không. */
export interface DeviceMatch {
  name: string;
  kind: "wifi" | "ethernet" | "bluetooth" | "storage";
  hardware_id: string;
  covered_by: string | null;
}

export interface DriverAnalysis {
  set: DriverSet;
  selected: string[];
  selected_size: number;
  devices: DeviceMatch[];
  /** `false` nghĩa là không đọc được danh sách thiết bị, không phải máy không có. */
  devices_read: boolean;
}

export interface StageReport {
  dest: string;
  packages: number;
  bytes: number;
}

export interface AppError { code: string; message: string }

// ---------------------------------------------------------------------------
// Hệ điều hành mã nguồn mở
// ---------------------------------------------------------------------------

/** Họ hệ điều hành người dùng chọn ở bước đầu; quyết định hình dạng cả luồng sau đó. */
export type OsFamily = "windows" | "linux";

export type DesktopWeight = "light" | "medium" | "heavy";

/** "signed" = có shim ký sẵn, cắm vào máy đang bật Secure Boot là boot thẳng. */
export type SecureBootSupport = "signed" | "unsigned";

export interface DistroRelease {
  id: string;
  name: string;
  family: string;
  version: string;
  desktop: string;
  weight: DesktopWeight;
  /** Dạng ISO YYYY-MM-DD. */
  released: string;
  /** `null` với bản rolling release. */
  end_of_support: string | null;
  lts: boolean;
  rolling: boolean;
  min_ram_gb: number;
  /** RAM để dùng thoải mái — khác với mức tối thiểu chỉ đủ cài. */
  rec_ram_gb: number;
  min_disk_gb: number;
  architectures: string[];
  secure_boot: SecureBootSupport;
  iso_size: number;
  audience: string;
  tagline: string;
  download_page: string;
  /** `null` nghĩa là phải tải thủ công qua trang chính thức. */
  checksum_url: string | null;
  iso_match: string;
}

export interface DistroCandidate {
  release: DistroRelease;
  score: number;
  /** Dùng chung tên với Verdict của Windows nên tái dùng được đúng bộ nhãn màu. */
  verdict: Verdict;
  pros: string[];
  cons: string[];
  blockers: string[];
  support_label: string;
  expired: boolean;
}

export interface DistroRecommendation {
  candidates: DistroCandidate[];
  best: string;
  summary: string;
  architecture: string;
  ram_gb: number;
  /** Ngày chốt của bảng nhúng — danh mục distro không tự đồng bộ. */
  catalog_snapshot: string;
}

/** Kết quả tra link tải qua file mã băm chính thức của distro. */
export interface ResolvedIso {
  url: string;
  filename: string;
  sha256: string;
}

export interface RawWriteRequest {
  disk_number: number;
  iso_path: string;
  confirm_token: string;
}

// ---------------------------------------------------------------------------
// Kiểm tra khởi động sau khi ghi
// ---------------------------------------------------------------------------

/** `skipped` là "không đọc được" — **không phải** là không đạt. */
export type CheckLevel = "pass" | "warn" | "fail" | "skipped";

export interface BootCheck {
  id: string;
  group: string;
  label: string;
  /** Giá trị đọc được trên chiếc USB này. */
  value: string;
  /** Điều kiện để khởi động được. */
  expectation: string;
  level: CheckLevel;
  hint: string | null;
  /** Không đạt thì firmware chắc chắn không khởi động được từ ổ này. */
  blocking: boolean;
}

export type BootVerdict = "ready" | "ready_with_warnings" | "not_bootable";

export interface BootReport {
  checks: BootCheck[];
  passed: number;
  warned: number;
  failed: number;
  skipped: number;
  bootable_uefi: boolean;
  bootable_legacy: boolean;
  verdict: BootVerdict;
  summary: string;
}

export interface BootCheckRequest {
  disk_number: number;
  family: OsFamily;
  iso_path: string;
  label: string;
  expect_unattend: boolean;
}

export interface ReadbackResult {
  matched: boolean;
  /** Số file đã đối chiếu (Windows) hoặc số byte đã đọc lại (Linux). */
  compared: number;
  mismatched: string[];
  missing: string[];
  expected_sha: string | null;
  actual_sha: string | null;
  message: string;
}

// ---------------------------------------------------------------------------
// Ngôn ngữ bộ cài
// ---------------------------------------------------------------------------

export interface SetupLanguage {
  /** Đúng tên Microsoft dùng trên trang tải. Rỗng nghĩa là không có ISO. */
  ms_name: string;
  /** Mã locale cho autounattend.xml. */
  locale: string;
  label: string;
  keyboard: string;
  /**
   * Chỉ dùng được cho định dạng vùng và bàn phím, không có ISO tương ứng.
   * Hiện chỉ tiếng Việt rơi vào nhóm này — Microsoft chưa từng phát hành ISO
   * Windows tiếng Việt.
   */
  region_only: boolean;
}
