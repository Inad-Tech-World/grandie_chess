use serde::{Serialize, Serializer};

/// Error type returned by every command. It crosses into TypeScript as a
/// plain message string.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Chess(String),
    #[error("engine: {0}")]
    Engine(String),
    #[error("{0}")]
    NotFound(String),
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("database pool error: {0}")]
    Pool(#[from] r2d2::Error),
    #[error("data error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("app error: {0}")]
    Tauri(#[from] tauri::Error),
    #[error("{0}")]
    Other(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    pub fn other(msg: impl Into<String>) -> Self {
        Error::Other(msg.into())
    }
}

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl specta::Type for Error {
    fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
        <String as specta::Type>::definition(types)
    }
}
