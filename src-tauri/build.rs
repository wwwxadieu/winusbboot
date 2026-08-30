/// Manifest nhúng vào file .exe trên Windows.
///
/// Điểm khác duy nhất so với manifest mặc định của `tauri-build` là khối
/// `trustInfo`: ứng dụng khai luôn là cần quyền quản trị, nên Windows hiện hộp
/// thoại UAC **trước khi** app khởi động và biểu tượng lối tắt có khiên nhỏ.
///
/// Vì sao khai ở manifest thay vì tự khởi động lại với quyền cao hơn: gần như
/// mọi thứ ứng dụng này làm — chia lại phân vùng, ghi thẳng ra
/// `\\.\PHYSICALDRIVE`, xuất driver khỏi kho của Windows — đều đòi quyền quản
/// trị. Mở ở quyền thường thì người dùng đi được năm bước rồi mới bị chặn.
/// Cách tự khởi động lại thì phải mở app một lần, tắt đi, mở lại — chớp nháy và
/// dễ thành vòng lặp nếu người dùng bấm "No".
///
/// Đánh đổi: tài khoản chuẩn không có mật khẩu quản trị sẽ không mở được app.
/// Đó là đánh đổi đúng — không có quyền đó thì cũng không tạo được USB.
///
/// Phần `Microsoft.Windows.Common-Controls` chép nguyên từ manifest mặc định
/// của `tauri-build`: khai manifest riêng là thay thế hẳn cái mặc định, bỏ sót
/// khối này thì các hộp thoại hệ thống mất kiểu dáng đời mới.
const APP_MANIFEST: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="requireAdministrator" uiAccess="false" />
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>
"#;

fn main() {
    let windows = tauri_build::WindowsAttributes::new().app_manifest(APP_MANIFEST);
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("không dựng được phần build của Tauri");
}
