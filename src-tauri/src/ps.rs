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

// ---------------------------------------------------------------------------
// Soát lỗi cú pháp trong các script nhúng
// ---------------------------------------------------------------------------

/// Bắt một lớp lỗi PowerShell mà trình biên dịch Rust không thể thấy.
///
/// Sinh ra từ một lỗi thật: `Test-Path $a -and -not (Test-Path $b)`. PowerShell
/// đọc `-and` là **tham số** của `Test-Path` chứ không phải toán tử logic, và cả
/// lệnh chết với "A parameter cannot be found that matches parameter name 'and'".
/// Đúng dòng đó nằm trong nhánh chạy với mọi ISO không phải ARM64, nghĩa là bước
/// đọc ISO chưa bao giờ chạy được — mà không có gì trong CI phát hiện ra, vì
/// script chỉ là một chuỗi ký tự cho tới lúc chạy trên máy Windows thật.
///
/// Chỉ dùng trong test nên nằm sau `#[cfg(test)]`: mã sản phẩm không cần nó.
#[cfg(test)]
pub mod lint {
    /// Một từ khoá kiểu `Get-Item`, `Test-Path` — tức là một lệnh, không phải
    /// toán tử (`-and`) hay tham số (`-Path`), vì hai loại đó bắt đầu bằng `-`.
    fn looks_like_a_cmdlet(token: &str) -> bool {
        let mut parts = token.splitn(2, '-');
        let (Some(verb), Some(noun)) = (parts.next(), parts.next()) else { return false };
        !verb.is_empty()
            && verb.chars().next().is_some_and(|c| c.is_ascii_uppercase())
            && verb.chars().all(|c| c.is_ascii_alphabetic())
            && !noun.is_empty()
            && noun.chars().all(|c| c.is_ascii_alphabetic())
    }

    /// Tìm những chỗ `-and` / `-or` bị gắn vào một lệnh thay vì nối hai biểu thức.
    ///
    /// Cách nhận biết: đi qua script, đếm độ sâu ngoặc, và ở mỗi độ sâu ghi nhớ
    /// các từ đã gặp trong câu lệnh hiện tại. Gặp `-and` mà **cùng độ sâu** với
    /// một tên lệnh nghĩa là toán tử đó đang nằm trong danh sách tham số của
    /// lệnh — chính là lỗi. Viết đúng thì lệnh phải nằm trong ngoặc của riêng
    /// nó, tức là sâu hơn một bậc, nên không bị bắt.
    pub fn operator_binding_bugs(script: &str) -> Vec<String> {
        let chars: Vec<char> = script.chars().collect();
        let mut stack: Vec<Vec<String>> = vec![Vec::new()];
        let mut found = Vec::new();
        let mut i = 0usize;
        let mut line = 1usize;

        while i < chars.len() {
            let c = chars[i];
            match c {
                '\n' => {
                    line += 1;
                    // Tham số của một lệnh không vắt qua dòng mới, nên sang dòng
                    // là bắt đầu một câu lệnh khác.
                    stack.last_mut().unwrap().clear();
                    i += 1;
                }
                // Chuỗi và chú thích không chứa cú pháp cần soát.
                '\'' | '"' => {
                    let quote = c;
                    i += 1;
                    while i < chars.len() {
                        if chars[i] == '`' && quote == '"' {
                            i += 2;
                            continue;
                        }
                        if chars[i] == quote {
                            i += 1;
                            break;
                        }
                        if chars[i] == '\n' {
                            line += 1;
                        }
                        i += 1;
                    }
                }
                '#' => {
                    while i < chars.len() && chars[i] != '\n' {
                        i += 1;
                    }
                }
                '(' | '{' | '[' => {
                    stack.push(Vec::new());
                    i += 1;
                }
                ')' | '}' | ']' => {
                    if stack.len() > 1 {
                        stack.pop();
                    }
                    i += 1;
                }
                ';' | '|' => {
                    stack.last_mut().unwrap().clear();
                    i += 1;
                }
                c if c.is_whitespace() => i += 1,
                _ => {
                    let start = i;
                    while i < chars.len()
                        && !chars[i].is_whitespace()
                        && !matches!(chars[i], '(' | ')' | '{' | '}' | '[' | ']' | ';' | '|' | ',' | '\'' | '"' | '#')
                    {
                        i += 1;
                    }
                    if i == start {
                        // Ký tự ngăn cách không có arm riêng (vd dấu phẩy). Không
                        // nhảy qua thì vòng lặp đứng yên mãi mãi tại đây.
                        i += 1;
                        continue;
                    }
                    let token: String = chars[start..i].iter().collect();
                    let level = stack.last_mut().unwrap();

                    if token.eq_ignore_ascii_case("-and") || token.eq_ignore_ascii_case("-or") {
                        if let Some(cmd) = level.iter().find(|t| looks_like_a_cmdlet(t)) {
                            found.push(format!(
                                "dòng {line}: `{token}` nằm trong danh sách tham số của `{cmd}` \
                                 — hãy bọc lệnh đó trong ngoặc riêng"
                            ));
                        }
                    }
                    level.push(token);
                }
            }
        }
        found
    }
}

#[cfg(test)]
mod lint_tests {
    use super::lint::*;

    #[test]
    fn the_bug_that_broke_reading_every_windows_iso_is_caught() {
        let bad = "if (Test-Path (Join-Path $r 'a.efi') -and -not (Test-Path $b)) { $x = 1 }";
        let hits = operator_binding_bugs(bad);
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert!(hits[0].contains("Test-Path"), "{hits:?}");
    }

    #[test]
    fn the_correct_form_with_each_command_in_its_own_parens_passes() {
        let good = "if ((Test-Path $a) -and -not (Test-Path $b)) { $x = 1 }";
        assert!(operator_binding_bugs(good).is_empty());
    }

    #[test]
    fn comparisons_between_plain_values_are_not_flagged() {
        let ok = "$capped = ($maxBoot -gt 0 -and $free -gt $maxBoot)\n\
                  if ($v -and $v.DriveLetter) { $t = 1 }\n\
                  if ($d.Id -notlike 'PCI\\*' -and $d.Id -notlike 'USB\\*') { continue }";
        assert!(operator_binding_bugs(ok).is_empty(), "{:?}", operator_binding_bugs(ok));
    }

    #[test]
    fn a_command_inside_parens_on_the_left_of_the_operator_is_fine() {
        // Dạng này hợp lệ: lệnh đã có ngoặc riêng, `-and` ở bậc ngoài.
        let ok = "if ($capped -and (Get-Disk -Number $n).LargestFreeExtent -gt 1GB) { $x = 1 }";
        assert!(operator_binding_bugs(ok).is_empty(), "{:?}", operator_binding_bugs(ok));
    }

    #[test]
    fn operators_inside_a_script_block_do_not_see_the_outer_command() {
        let ok = "Get-ChildItem | Where-Object { $_.Length -gt 0 -and $_.Name -ne 'x' }";
        assert!(operator_binding_bugs(ok).is_empty(), "{:?}", operator_binding_bugs(ok));
    }

    #[test]
    fn an_operator_word_inside_a_string_is_not_syntax() {
        let ok = "Write-Output 'chạy Test-Path -and xong'";
        assert!(operator_binding_bugs(ok).is_empty());
    }

    /// Quét **toàn bộ** script PowerShell nhúng trong mã nguồn.
    ///
    /// Đọc thẳng file nguồn thay vì liệt kê tên từng hằng số: script mới thêm
    /// vào bất kỳ module nào cũng tự động được soát, không phải nhớ cập nhật
    /// danh sách. Chuỗi thô `r#"…"#` là dạng duy nhất dùng để nhúng script nên
    /// chỉ cần bóc đúng dạng đó.
    #[test]
    fn no_embedded_powershell_script_binds_an_operator_to_a_command() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut checked = 0usize;
        let mut problems: Vec<String> = Vec::new();

        for entry in std::fs::read_dir(&dir).expect("không đọc được thư mục src").flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let src = std::fs::read_to_string(&path).unwrap();
            let name = path.file_name().unwrap().to_string_lossy().to_string();

            let mut rest = src.as_str();
            while let Some(at) = rest.find("r#\"") {
                let body = &rest[at + 3..];
                let Some(end) = body.find("\"#") else { break };
                let script = &body[..end];
                rest = &body[end + 2..];

                // Chỉ soát thứ trông như PowerShell, bỏ qua các chuỗi thô khác.
                if !script.contains('$') && !script.contains("-Path") {
                    continue;
                }
                checked += 1;
                for hit in operator_binding_bugs(script) {
                    problems.push(format!("{name} — {hit}"));
                }
            }
        }

        assert!(checked >= 10, "chỉ soát được {checked} script, có vẻ đã bóc sai");
        assert!(problems.is_empty(), "script PowerShell hỏng cú pháp:\n{}", problems.join("\n"));
    }
}
