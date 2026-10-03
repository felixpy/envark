use std::path::Path;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    InvalidInput(String),
    #[error("{0}")]
    UnsafePath(String),
    #[error("The operation was cancelled.")]
    Cancelled,
    #[error("{0}")]
    Process(String),
    #[error("{0}")]
    Unavailable(String),
    #[error("{0}")]
    Conflict(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl Error {
    pub fn unsafe_path(path: &Path, reason: &str) -> Self {
        Self::UnsafePath(format!("{}: {reason}", path.display()))
    }
}
