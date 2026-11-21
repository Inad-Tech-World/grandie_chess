use serde::{Deserialize, Serialize, Serializer};
use specta::Type;

/// Error type returned by every command. It crosses into TypeScript as an
/// [`ErrorPayload`]: a kind the UI can act on plus a readable message.
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
    /// No OpenRouter key is saved, or OpenRouter rejected it.
    #[error("{0}")]
    NoApiKey(String),
    /// The AI account is out of credits or rate limited.
    #[error("{0}")]
    AiQuota(String),
    /// Any other failure from the AI service.
    #[error("AI error: {0}")]
    Ai(String),
    /// The operation was cancelled by the user.
    #[error("cancelled")]
    Cancelled,
    #[error("{0}")]
    Other(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    pub fn other(msg: impl Into<String>) -> Self {
        Error::Other(msg.into())
    }

    pub fn kind(&self) -> ErrorKind {
        match self {
            Error::Chess(_) => ErrorKind::Chess,
            Error::Engine(_) => ErrorKind::Engine,
            Error::NotFound(_) => ErrorKind::NotFound,
            Error::Io(_) => ErrorKind::Io,
            Error::Http(_) => ErrorKind::Network,
            Error::Db(_) | Error::Pool(_) => ErrorKind::Db,
            Error::Json(_) => ErrorKind::Data,
            Error::Tauri(_) => ErrorKind::App,
            Error::NoApiKey(_) => ErrorKind::NoApiKey,
            Error::AiQuota(_) => ErrorKind::AiQuota,
            Error::Ai(_) => ErrorKind::Ai,
            Error::Cancelled => ErrorKind::Cancelled,
            Error::Other(_) => ErrorKind::Other,
        }
    }
}

/// What kind of failure a command hit, so the UI can offer the right action
/// (for example "Open AI settings" for [`ErrorKind::NoApiKey`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind {
    Chess,
    Engine,
    NotFound,
    Io,
    Network,
    Db,
    Data,
    App,
    NoApiKey,
    AiQuota,
    Ai,
    Cancelled,
    Other,
}

/// How an [`Error`] looks in TypeScript.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ErrorPayload {
    pub kind: ErrorKind,
    pub message: String,
}

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        ErrorPayload { kind: self.kind(), message: self.to_string() }.serialize(serializer)
    }
}

impl specta::Type for Error {
    fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
        <ErrorPayload as specta::Type>::definition(types)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialises_kind_and_message() {
        let json = serde_json::to_value(Error::NoApiKey("Add an OpenRouter key in Settings".into())).unwrap();
        assert_eq!(json, serde_json::json!({"kind": "noApiKey", "message": "Add an OpenRouter key in Settings"}));
        let json = serde_json::to_value(Error::Engine("gone".into())).unwrap();
        assert_eq!(json["kind"], "engine");
        assert_eq!(json["message"], "engine: gone");
    }
}
