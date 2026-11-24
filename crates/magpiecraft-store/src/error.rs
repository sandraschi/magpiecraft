//! Store error type. Kept separate from the domain error so the SQL layer can be
//! used (and tested) without the rest of the workspace.

use std::path::Path;

pub type StoreResult<T> = Result<T, StoreError>;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database: {0}")]
    Sqlite(String),
    #[error("io: {0}: {1}")]
    Io(String, String),
}

impl StoreError {
    pub fn sqlite(e: rusqlite::Error) -> Self {
        StoreError::Sqlite(e.to_string())
    }

    pub fn io(e: std::io::Error) -> Self {
        StoreError::Io(String::new(), e.to_string())
    }

    pub fn io_at(path: &Path, e: std::io::Error) -> Self {
        StoreError::Io(path.display().to_string(), e.to_string())
    }

    /// Stable machine-readable kind for JSON output.
    pub fn kind(&self) -> &'static str {
        match self {
            StoreError::Sqlite(_) => "db",
            StoreError::Io(_, _) => "io",
        }
    }
}
