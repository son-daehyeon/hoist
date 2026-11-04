use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error, specta::Type)]
#[specta(type = ApiError, inline)]
pub enum Error {
    #[error(transparent)]
    Core(#[from] banner_core::CoreError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Opener(#[from] tauri_plugin_opener::Error),
    #[error(transparent)]
    Updater(#[from] tauri_plugin_updater::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
    #[error("설정에서 OpenAI API 키를 먼저 넣어 주세요")]
    NoApiKey,
    #[error("{0}")]
    Lock(String),
    #[error("{0}")]
    Invalid(String),
}

#[derive(Serialize, specta::Type)]
#[serde(tag = "kind", content = "message", rename_all = "snake_case")]
pub enum ApiError {
    Core(String),
    Io(String),
    Opener(String),
    Updater(String),
    Other(String),
    NoApiKey(String),
    Lock(String),
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl<T> From<std::sync::PoisonError<T>> for Error {
    fn from(e: std::sync::PoisonError<T>) -> Self {
        Error::Lock(e.to_string())
    }
}

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let message = self.to_string();
        match self {
            Error::Core(_) => ApiError::Core(message),
            Error::Io(_) => ApiError::Io(message),
            Error::Opener(_) => ApiError::Opener(message),
            Error::Updater(_) => ApiError::Updater(message),
            Error::Other(_) => ApiError::Other(message),
            Error::NoApiKey => ApiError::NoApiKey(message),
            Error::Lock(_) => ApiError::Lock(message),
            Error::Invalid(_) => ApiError::Invalid(message),
        }
        .serialize(serializer)
    }
}
