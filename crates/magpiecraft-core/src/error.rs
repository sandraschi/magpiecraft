//! One error type for the whole engine, with a stable `kind()` for JSON output.

use magpiecraft_comics::ComicError;
use magpiecraft_health::HealthError;
use magpiecraft_scan::ScanError;
use magpiecraft_store::StoreError;
use magpiecraft_tools::ToolError;

pub type MagpieResult<T> = Result<T, MagpieError>;

#[derive(Debug, thiserror::Error)]
pub enum MagpieError {
    #[error("{0}")]
    Store(#[from] StoreError),
    #[error("{0}")]
    Scan(#[from] ScanError),
    #[error("{0}")]
    Comic(#[from] ComicError),
    #[error("{0}")]
    Health(#[from] HealthError),
    #[error("{0}")]
    Tool(#[from] ToolError),
    #[error("io: {0}: {1}")]
    Io(String, String),
    #[error("invalid: {0}")]
    Invalid(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("not implemented: {0}")]
    NotImplemented(String),
    #[error("config: {0}")]
    Config(String),
    #[error("confirmation required: {0}")]
    ConfirmRequired(String),
    #[error("llm: {0}")]
    Llm(String),
}

impl MagpieError {
    /// Stable machine-readable kind. Callers switch on this, never on the message.
    pub fn kind(&self) -> &'static str {
        match self {
            MagpieError::Store(e) => e.kind(),
            MagpieError::Scan(e) => e.kind(),
            MagpieError::Comic(e) => e.kind(),
            MagpieError::Health(e) => e.kind(),
            MagpieError::Tool(e) => e.kind(),
            MagpieError::Io(_, _) => "io",
            MagpieError::Invalid(_) => "invalid",
            MagpieError::NotFound(_) => "not_found",
            MagpieError::NotImplemented(_) => "not_implemented",
            MagpieError::Config(_) => "config",
            MagpieError::ConfirmRequired(_) => "confirm_required",
            MagpieError::Llm(_) => "llm",
        }
    }

    pub fn io(path: &std::path::Path, e: std::io::Error) -> Self {
        MagpieError::Io(path.display().to_string(), e.to_string())
    }

    pub fn invalid(msg: impl Into<String>) -> Self {
        MagpieError::Invalid(msg.into())
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        MagpieError::NotFound(msg.into())
    }

    pub fn not_implemented(msg: impl Into<String>) -> Self {
        MagpieError::NotImplemented(msg.into())
    }

    pub fn llm(msg: impl Into<String>) -> Self {
        MagpieError::Llm(msg.into())
    }
}
