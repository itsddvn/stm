use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("invalid path: {0}")]
    InvalidPath(String),
    #[error("path escapes approved root: {0}")]
    PathEscape(PathBuf),
    #[error("project-local root rejected: {0}")]
    ProjectRootRejected(PathBuf),
    #[error("duplicate physical root rejected: {0}")]
    DuplicatePhysicalRoot(PathBuf),
    #[error("unsupported fixture or schema: {0}")]
    UnsupportedSchema(String),
    #[error("malformed fixture or config: {0}")]
    MalformedInput(String),
    #[error("lifecycle plan not found: {0}")]
    LifecyclePlanNotFound(String),
    #[error("lifecycle operation not found: {0}")]
    LifecycleOperationNotFound(String),
    #[error("lifecycle consent denied: {0}")]
    LifecycleConsentDenied(String),
    #[error("lifecycle evidence changed: {0}")]
    LifecycleEvidenceChanged(String),
    #[error("command denied by allowlist: {0}")]
    CommandDenied(String),
    #[error("argument denied by allowlist: {0}")]
    ArgumentDenied(String),
    #[error("process spawn failed: {0}")]
    ProcessSpawn(String),
    #[error("process execution failed: {0}")]
    ProcessExecution(String),
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("toml error: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("url error: {0}")]
    Url(#[from] url::ParseError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
