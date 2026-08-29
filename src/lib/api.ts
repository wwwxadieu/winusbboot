/** Lớp bọc mỏng quanh các lệnh Tauri, để component không phải đụng tới invoke. */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  CatalogState, DistroRecommendation, DownloadOption, DownloadProgress, FormatRequest,
  FormatResult, HardwareReport, IsoInfo, RawWriteRequest, Recommendation, ResolvedIso,
  UnattendConfig, UsbDisk, WriteProgress, WriteRequest,
} from "../types";

export const api = {
  listUsbDisks: () => invoke<UsbDisk[]>("list_usb_disks"),
  diskToken: (diskNumber: number) => invoke<string>("disk_token", { diskNumber }),
  scanHardware: () => invoke<HardwareReport>("scan_hardware"),
  getRecommendation: () => invoke<Recommendation>("get_recommendation"),
  memoryTypeName: (code: number) => invoke<string>("memory_type_name", { code }),
  refreshCatalog: () => invoke<CatalogState>("refresh_catalog"),
  catalogState: () => invoke<CatalogState>("catalog_state"),
  isAdmin: () => invoke<boolean>("is_admin"),
  relaunchAsAdmin: () => invoke<void>("relaunch_as_admin"),
  inspectIso: (path: string) => invoke<IsoInfo>("inspect_iso", { path }),
  officialDownloadPage: (releaseId: string) =>
    invoke<string>("official_download_page", { releaseId }),
  fetchDownloadLinks: (releaseId: string, language: string) =>
    invoke<DownloadOption[]>("fetch_download_links", { releaseId, language }),
  downloadIso: (url: string, dest: string) => invoke<string>("download_iso", { url, dest }),
  hashIso: (path: string) => invoke<string>("hash_iso", { path }),
  formatUsb: (request: FormatRequest) => invoke<FormatResult>("format_usb", { request }),
  writeIso: (request: WriteRequest) => invoke<void>("write_iso", { request }),
  recommendDistros: () => invoke<DistroRecommendation>("recommend_distros"),
  resolveDistroIso: (distroId: string) =>
    invoke<ResolvedIso>("resolve_distro_iso", { distroId }),
  writeImageRaw: (request: RawWriteRequest) => invoke<void>("write_image_raw", { request }),
  previewUnattend: (config: UnattendConfig) => invoke<string | null>("preview_unattend", { config }),
};

export const events = {
  onUsbChanged: (cb: (d: UsbDisk[]) => void): Promise<UnlistenFn> =>
    listen<UsbDisk[]>("usb://changed", (e) => cb(e.payload)),
  onWriteProgress: (cb: (p: WriteProgress) => void): Promise<UnlistenFn> =>
    listen<WriteProgress>("write://progress", (e) => cb(e.payload)),
  onFormatProgress: (cb: (p: WriteProgress) => void): Promise<UnlistenFn> =>
    listen<WriteProgress>("format://progress", (e) => cb(e.payload)),
  onDownloadProgress: (cb: (p: DownloadProgress) => void): Promise<UnlistenFn> =>
    listen<DownloadProgress>("download://progress", (e) => cb(e.payload)),
  onHashProgress: (cb: (p: number) => void): Promise<UnlistenFn> =>
    listen<number>("hash://progress", (e) => cb(e.payload)),
  onCatalogUpdated: (cb: (s: CatalogState) => void): Promise<UnlistenFn> =>
    listen<CatalogState>("catalog://updated", (e) => cb(e.payload)),
};

/** Lỗi từ backend là `{ code, message }`; ở đây quy về một chuỗi để hiển thị. */
export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return "Đã xảy ra lỗi không xác định.";
}
