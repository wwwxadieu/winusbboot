//! Get WinUSB — nhận diện USB, phân tích phần cứng, gợi ý và tạo USB cài Windows.

mod catalog;
mod catalog_sync;
mod checks;
mod cpu;
mod distro;
mod download;
mod drivers;
mod error;
mod hardware;
mod languages;
mod ps;
mod recommend;
mod unattend;
mod usb;
mod verify;
mod writer;

use error::Result;
use tauri::{AppHandle, Emitter, Manager};

// ---------------------------------------------------------------------------
// Nhận diện USB
// ---------------------------------------------------------------------------

#[tauri::command]
async fn list_usb_disks() -> Result<Vec<usb::UsbDisk>> {
    usb::list().await
}

/// Vân tay của ổ tại thời điểm gọi. Frontend lấy giá trị này ngay trước khi ghi
/// và gửi kèm theo yêu cầu; backend so lại để chắc chắn không ghi nhầm ổ khác.
#[tauri::command]
async fn disk_token(disk_number: u32) -> Result<String> {
    let disks = usb::list().await?;
    disks
        .iter()
        .find(|d| d.number == disk_number)
        .map(writer::token_for)
        .ok_or_else(|| error::AppError::new("disk_gone", "Không còn thấy ổ USB này."))
}

// ---------------------------------------------------------------------------
// Phần cứng và gợi ý
// ---------------------------------------------------------------------------

#[tauri::command]
async fn scan_hardware() -> Result<hardware::HardwareReport> {
    hardware::scan().await
}

#[tauri::command]
async fn get_recommendation() -> Result<recommend::Recommendation> {
    let hw = hardware::scan().await?;
    Ok(recommend::analyze(&hw))
}

/// Đồng bộ lại danh mục theo yêu cầu của người dùng.
#[tauri::command]
async fn refresh_catalog() -> Result<catalog::CatalogState> {
    catalog_sync::sync().await
}

#[tauri::command]
fn catalog_state() -> catalog::CatalogState {
    catalog::snapshot()
}

/// Bảng ngôn ngữ bộ cài. Tách thành lệnh riêng để giao diện không phải giữ một
/// bản sao thứ hai của danh sách rồi lệch dần khỏi bản backend dùng để khớp SKU.
#[tauri::command]
fn setup_languages() -> Vec<languages::SetupLanguage> {
    languages::all()
}

#[tauri::command]
fn memory_type_name(code: u32) -> String {
    hardware::memory_type_name(code).to_string()
}

// ---------------------------------------------------------------------------
// Hệ điều hành mã nguồn mở
// ---------------------------------------------------------------------------

/// Chấm điểm các bản Linux theo đúng báo cáo phần cứng đã quét cho Windows —
/// một lần quét máy dùng chung cho cả hai engine.
#[tauri::command]
async fn recommend_distros() -> Result<distro::DistroRecommendation> {
    let hw = hardware::scan().await?;
    Ok(distro::analyze(&hw))
}

/// Tra link tải hiện hành của một bản Linux qua file mã băm chính thức.
#[tauri::command]
async fn resolve_distro_iso(distro_id: String) -> Result<download::ResolvedIso> {
    let release = distro::builtin()
        .into_iter()
        .find(|r| r.id == distro_id)
        .ok_or_else(|| error::AppError::new("no_distro", "Không có bản Linux nào mang mã này."))?;

    let url = release.checksum_url.ok_or_else(|| {
        error::AppError::new(
            "manual_only",
            format!("{} không có link tải ổn định — hãy tải từ trang chính thức rồi chọn file.", release.name),
        )
    })?;

    download::resolve_distro_iso(&url, &release.iso_match).await
}

/// Ghi nguyên khối ảnh đĩa ra USB. Dùng cho ISO Linux, xem `writer::write_image_raw`.
#[tauri::command]
async fn write_image_raw(app: AppHandle, request: writer::RawWriteRequest) -> Result<()> {
    let handle = app.clone();
    writer::write_image_raw(request, move |p| {
        let _ = handle.emit("write://progress", &p);
    })
    .await
}

// ---------------------------------------------------------------------------
// Quyền quản trị
// ---------------------------------------------------------------------------

#[tauri::command]
fn is_admin() -> bool {
    ps::is_elevated()
}

#[tauri::command]
async fn relaunch_as_admin(app: AppHandle) -> Result<()> {
    ps::relaunch_elevated().await?;
    app.exit(0);
    Ok(())
}

// ---------------------------------------------------------------------------
// Nguồn cài đặt
// ---------------------------------------------------------------------------

#[tauri::command]
async fn inspect_iso(path: String) -> Result<writer::IsoInfo> {
    writer::inspect_iso(&path).await
}

#[tauri::command]
fn official_download_page(release_id: String) -> String {
    download::official_page(&release_id).to_string()
}

#[tauri::command]
async fn download_iso(app: AppHandle, url: String, dest: String) -> Result<String> {
    let path = std::path::PathBuf::from(&dest);
    let handle = app.clone();
    download::download(&url, &path, move |p| {
        let _ = handle.emit("download://progress", &p);
    })
    .await?;
    Ok(dest)
}

/// Thư mục ứng dụng tự tải ISO về. Giao diện hiện đường dẫn này thay vì bắt
/// người dùng chọn thư mục mỗi lần tải.
#[tauri::command]
fn iso_download_dir() -> String {
    download::managed_dir().to_string_lossy().to_string()
}

/// Dọn file ISO sau khi ghi xong USB. Chỉ xoá được file nằm trong thư mục ứng
/// dụng tự quản — file người dùng tự chọn thì bị từ chối.
#[tauri::command]
fn discard_iso(path: String) -> Result<bool> {
    download::discard(std::path::Path::new(&path))
}

#[tauri::command]
async fn hash_iso(app: AppHandle, path: String) -> Result<String> {
    let handle = app.clone();
    download::sha256(std::path::Path::new(&path), move |pct| {
        let _ = handle.emit("hash://progress", pct);
    })
    .await
}

// ---------------------------------------------------------------------------
// Ghi USB
// ---------------------------------------------------------------------------

/// Bước format: xoá và chia lại phân vùng. Tách riêng khỏi bước ghi vì đây là
/// thao tác duy nhất làm mất dữ liệu, và người dùng cần thấy rõ điều đó.
#[tauri::command]
async fn format_usb(app: AppHandle, request: writer::FormatRequest) -> Result<writer::FormatResult> {
    let handle = app.clone();
    writer::format_usb(request, move |p| {
        let _ = handle.emit("format://progress", &p);
    })
    .await
}

/// Bước ghi: chép bộ cài lên ổ USB đã format.
#[tauri::command]
async fn write_iso(app: AppHandle, request: writer::WriteRequest) -> Result<()> {
    let handle = app.clone();
    writer::write_iso(request, move |p| {
        let _ = handle.emit("write://progress", &p);
    })
    .await
}

// ---------------------------------------------------------------------------
// Kiểm tra USB sau khi ghi
// ---------------------------------------------------------------------------

/// Kiểm tra cấu trúc — vài giây, không đọc lại dữ liệu.
#[tauri::command]
async fn check_usb_boot(request: verify::BootCheckRequest) -> Result<verify::BootReport> {
    verify::check_boot(request).await
}

/// Đọc lại toàn bộ dữ liệu vừa ghi và đối chiếu. Chậm, nên do người dùng bấm.
#[tauri::command]
async fn verify_usb_readback(
    app: AppHandle,
    request: verify::BootCheckRequest,
) -> Result<verify::ReadbackResult> {
    let handle = app.clone();
    verify::readback(request, move |p| {
        let _ = handle.emit("verify://progress", &p);
    })
    .await
}

/// Xem trước nội dung autounattend.xml trước khi ghi.
#[tauri::command]
fn preview_unattend(config: unattend::UnattendConfig) -> Option<String> {
    unattend::generate(&config)
}

// ---------------------------------------------------------------------------
// Driver kèm theo USB
// ---------------------------------------------------------------------------

/// Thư mục ứng dụng xuất driver của máy hiện tại.
#[tauri::command]
fn driver_export_dir() -> String {
    drivers::export_dir().to_string_lossy().to_string()
}

/// Xuất driver của chính máy đang chạy. Cần quyền quản trị và mất vài phút.
#[tauri::command]
async fn export_system_drivers(app: AppHandle) -> Result<String> {
    let handle = app.clone();
    let dir = drivers::export_this_pc(move |done, total| {
        let _ = handle.emit("drivers://export", (done, total));
    })
    .await?;
    Ok(dir.to_string_lossy().to_string())
}

/// Quét một thư mục driver rồi đối chiếu với thiết bị thật của máy.
#[tauri::command]
async fn analyse_drivers(
    path: String,
    filter: drivers::DriverFilter,
) -> Result<drivers::DriverAnalysis> {
    drivers::analyse(std::path::Path::new(&path), filter).await
}

/// Chép các gói driver hợp lệ vào `<ổ>:\$WinPEDriver$` trên USB.
#[tauri::command]
async fn stage_drivers(
    app: AppHandle,
    path: String,
    filter: drivers::DriverFilter,
    drive_letter: String,
) -> Result<drivers::StageReport> {
    let handle = app.clone();
    drivers::stage_to_usb(
        std::path::Path::new(&path),
        filter,
        &drive_letter,
        move |done, total, name| {
            let _ = handle.emit("drivers://copy", (done, total, name.to_string()));
        },
    )
    .await
}

/// Xoá thư mục xuất tạm sau khi đã chép xong sang USB.
#[tauri::command]
fn discard_driver_export() -> Result<bool> {
    drivers::discard_export()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();

            // Danh mục phiên bản Windows: dùng ngay bản lưu đệm của lần trước
            // (không cần mạng), rồi đồng bộ lại ở nền. Đồng bộ hỏng thì im lặng
            // giữ nguyên dữ liệu đang có — người dùng vẫn thấy nguồn dữ liệu và
            // ngày cập nhật trên màn hình gợi ý.
            if let Some(cached) = catalog_sync::load_cache() {
                catalog::replace(cached);
            }
            let catalog_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                match catalog_sync::sync().await {
                    Ok(state) => {
                        let _ = catalog_handle.emit("catalog://updated", &state);
                    }
                    Err(e) => {
                        let _ = catalog_handle.emit("catalog://error", &e.message);
                    }
                }
            });

            // Vòng theo dõi cắm/rút chạy suốt vòng đời ứng dụng. Nếu tiến trình
            // PowerShell chết vì lý do nào đó, chờ vài giây rồi dựng lại thay vì
            // để tính năng nhận diện tự động im lặng ngừng hoạt động.
            tauri::async_runtime::spawn(async move {
                loop {
                    let h = handle.clone();
                    let outcome = usb::watch(3, move |disks| {
                        let _ = h.emit("usb://changed", &disks);
                    })
                    .await;

                    if let Err(e) = outcome {
                        let _ = handle.emit("usb://error", &e.message);
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

                    // Trên máy không phải Windows, watch() trả về ngay lập tức —
                    // lặp vô hạn ở đó chỉ tổ đốt CPU.
                    if cfg!(not(windows)) {
                        break;
                    }
                }
            });

            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_usb_disks,
            disk_token,
            scan_hardware,
            get_recommendation,
            recommend_distros,
            resolve_distro_iso,
            memory_type_name,
            setup_languages,
            refresh_catalog,
            catalog_state,
            is_admin,
            relaunch_as_admin,
            inspect_iso,
            official_download_page,
            download_iso,
            hash_iso,
            iso_download_dir,
            discard_iso,
            format_usb,
            write_iso,
            write_image_raw,
            check_usb_boot,
            verify_usb_readback,
            preview_unattend,
            driver_export_dir,
            export_system_drivers,
            analyse_drivers,
            stage_drivers,
            discard_driver_export,
        ])
        .run(tauri::generate_context!())
        .expect("không khởi chạy được ứng dụng");
}
