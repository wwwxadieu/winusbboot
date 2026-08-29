//! Kiểu lỗi dùng chung, tự serialize được để trả thẳng về frontend.

use std::fmt;

#[derive(Debug, Clone)]
pub struct AppError {
    pub message: String,
    /// Mã ngắn để frontend phân nhánh xử lý (vd: "not_admin", "no_powershell").
    pub code: String,
}

impl AppError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self { code: code.to_string(), message: message.into() }
    }
    pub fn msg(message: impl Into<String>) -> Self {
        Self::new("error", message)
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for AppError {}

impl serde::Serialize for AppError {
    // `Result` ở module này là alias một tham số bên dưới, nên phải ghi đủ đường dẫn.
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("AppError", 2)?;
        st.serialize_field("code", &self.code)?;
        st.serialize_field("message", &self.message)?;
        st.end()
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::new("io", e.to_string())
    }
}
impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::new("json", format!("Không đọc được dữ liệu JSON: {e}"))
    }
}
impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        AppError::new("network", format!("Lỗi mạng: {e}"))
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
