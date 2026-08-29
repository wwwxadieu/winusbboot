//! Cầu nối tới PowerShell.
//!
//! Toàn bộ việc đọc phần cứng, liệt kê đĩa USB và thao tác phân vùng đều đi qua
//! đây. Dùng PowerShell thay vì bind trực tiếp WMI/Win32 giúp mã nguồn biên dịch
//! được trên mọi nền tảng (tiện phát triển trên máy không phải Windows) và dễ
//! mở rộng — muốn thêm một truy vấn mới chỉ cần viết thêm một đoạn script.

use crate::error::{AppError, Result};
use base64::Engine;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// Không bật cửa sổ console đen khi chạy tiến trình con trên Windows.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Bao script bằng khối bắt lỗi để lỗi PowerShell trở thành thông báo rõ ràng
/// thay vì một stderr lộn xộn.
fn wrap(script: &str) -> String {
    format!(
        "$ProgressPreference='SilentlyContinue';\r\n\
         $ErrorActionPreference='Stop';\r\n\
         [Console]::OutputEncoding=[System.Text.Encoding]::UTF8;\r\n\
         try {{\r\n{script}\r\n}} catch {{\r\n\
           [Console]::Error.WriteLine('GWU-ERROR: ' + $_.Exception.Message); exit 1\r\n\
         }}"
    )
}

/// PowerShell nhận script dạng base64 của chuỗi UTF-16LE. Cách này tránh hoàn
/// toàn việc escape dấu nháy — vốn là nguồn lỗi kinh điển khi gọi PowerShell.
fn encode(script: &str) -> String {
    let utf16: Vec<u8> = wrap(script)
        .encode_utf16()
        .flat_map(|c| c.to_le_bytes())
        .collect();
    base64::engine::general_purpose::STANDARD.encode(utf16)
}

fn build(script: &str) -> Command {
    // PowerShell 5.1 luôn có sẵn trên Windows 10/11 nên dùng làm mặc định,
    // thay vì pwsh 7 vốn phải cài thêm.
    let mut cmd = Command::new("powershell");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-EncodedCommand",
        &encode(script),
    ]);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    // `cmd` là tokio::process::Command, vốn đã có sẵn creation_flags trên
    // Windows — không cần trait CommandExt của std.
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// Chạy script, trả về toàn bộ stdout.
pub async fn run(script: &str) -> Result<String> {
    let out = build(script).output().await.map_err(|e| {
        AppError::new(
            "no_powershell",
            format!("Không khởi chạy được PowerShell: {e}. Ứng dụng này chỉ chạy trên Windows."),
        )
    })?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let clean = stderr
            .lines()
            .find_map(|l| l.trim().strip_prefix("GWU-ERROR: "))
            .map(str::to_string)
            .unwrap_or_else(|| stderr.trim().to_string());
        return Err(AppError::new(
            "powershell",
            if clean.is_empty() { "PowerShell trả về lỗi không rõ nguyên nhân".into() } else { clean },
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Chạy script rồi parse stdout thành kiểu `T`.
///
/// `ConvertTo-Json` của PowerShell 5.1 trả về `null` cho tập rỗng và trả về một
/// object đơn lẻ thay vì mảng một phần tử, nên mọi script gọi qua đây đều phải
/// dùng `ConvertTo-Json -InputObject @(...)` để giữ nguyên dạng mảng.
pub async fn run_json<T: serde::de::DeserializeOwned>(script: &str) -> Result<T> {
    let raw = run(script).await?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::new("empty", "PowerShell không trả về dữ liệu nào"));
    }
    serde_json::from_str(trimmed).map_err(|e| {
        AppError::new(
            "json",
            format!("Không đọc được kết quả từ PowerShell: {e}\n--- dữ liệu thô ---\n{}",
                trimmed.chars().take(600).collect::<String>()),
        )
    })
}

/// Chạy script dài và gọi `on_line` cho mỗi dòng stdout ngay khi nó xuất hiện.
///
/// Dùng cho các thao tác nhiều phút (copy file, DISM) để đẩy tiến trình lên UI
/// theo thời gian thực thay vì đợi tới lúc kết thúc.
pub async fn run_streaming<F>(script: &str, mut on_line: F) -> Result<()>
where
    F: FnMut(&str) + Send,
{
    let mut child = build(script).spawn().map_err(|e| {
        AppError::new("no_powershell", format!("Không khởi chạy được PowerShell: {e}"))
    })?;

    let stdout = child.stdout.take().ok_or_else(|| AppError::msg("Không đọc được stdout"))?;
    let stderr = child.stderr.take().ok_or_else(|| AppError::msg("Không đọc được stderr"))?;

    // Gom stderr ở luồng riêng để tiến trình con không bị nghẽn khi bộ đệm đầy.
    let stderr_task = tokio::spawn(async move {
        let mut buf = String::new();
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            buf.push_str(&line);
            buf.push('\n');
        }
        buf
    });

    let mut lines = BufReader::new(stdout).lines();
    while let Some(line) = lines.next_line().await? {
        on_line(&line);
    }

    let status = child.wait().await?;
    let stderr_text = stderr_task.await.unwrap_or_default();

    if !status.success() {
        let clean = stderr_text
            .lines()
            .find_map(|l| l.trim().strip_prefix("GWU-ERROR: "))
            .map(str::to_string)
            .unwrap_or_else(|| stderr_text.trim().to_string());
        return Err(AppError::new(
            "powershell",
            if clean.is_empty() { "Thao tác thất bại".into() } else { clean },
        ));
    }
    Ok(())
}

/// Kiểm tra tiến trình hiện tại có quyền Administrator hay không.
///
/// Thư mục `System32\config` chỉ đọc được bởi tài khoản quản trị, nên đây là
/// phép thử tức thời, không tốn một lần gọi PowerShell.
pub fn is_elevated() -> bool {
    #[cfg(windows)]
    {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
        std::fs::read_dir(format!("{root}\\System32\\config")).is_ok()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Khởi động lại chính ứng dụng này với quyền Administrator (hiện hộp thoại UAC).
pub async fn relaunch_elevated() -> Result<()> {
    let exe = std::env::current_exe()?;
    let path = exe.to_string_lossy().replace('\'', "''");
    run(&format!("Start-Process -FilePath '{path}' -Verb RunAs")).await?;
    Ok(())
}
