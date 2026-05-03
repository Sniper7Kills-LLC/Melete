use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("bincode error: {0}")]
    Codec(#[from] bincode::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("uuid error: {0}")]
    Uuid(#[from] uuid::Error),

    #[error("invalid uuid blob: expected 16 bytes, got {0}")]
    InvalidUuid(usize),

    #[error("unsupported stroke blob version: {0}")]
    UnsupportedBlobVersion(u8),

    #[error("empty stroke blob")]
    EmptyBlob,

    #[error("not found")]
    NotFound,

    #[error("invalid data: {0}")]
    InvalidData(String),
}

pub type Result<T> = std::result::Result<T, StorageError>;
