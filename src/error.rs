use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    InvalidInput(String),
    #[error("{path}: {message}")]
    Parse { path: String, message: String },
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Ambiguous(String),
    #[error("{0}")]
    Unsupported(String),
    #[error("{0}")]
    Conflict(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Yaml(#[from] serde_yaml::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error("{0}")]
    Data(String),
}
impl Error {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "usage_error",
            Self::Parse { .. } => "parse_error",
            Self::Validation(_) => "validation_error",
            Self::NotFound(_) => "not_found",
            Self::Ambiguous(_) => "ambiguous_reference",
            Self::Unsupported(_) => "unsupported",
            Self::Conflict(_) => "conflict",
            _ => "internal_error",
        }
    }
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({"code":self.code(),"message":self.to_string(),"details":{}})
    }
}
pub type Result<T> = std::result::Result<T, Error>;
