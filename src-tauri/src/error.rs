use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("SMB: {0}")]
    Smb(#[from] smb::Error),
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("HTTP: {0}")]
    Http(#[from] http::Error),
    #[error("{0}")]
    Msg(String),
}

impl AppError {
    pub fn msg(message: impl Into<String>) -> Self {
        Self::Msg(message.into())
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<String> for AppError {
    fn from(value: String) -> Self {
        Self::Msg(value)
    }
}

impl From<&str> for AppError {
    fn from(value: &str) -> Self {
        Self::Msg(value.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
