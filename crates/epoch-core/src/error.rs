use std::path::PathBuf;

/// Errors shared by every Epoch crate that touches the filesystem.
///
/// Each variant maps to a stable [`ErrorCode`] so a frontend can branch on
/// the kind of failure without parsing messages.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("{path}: not found")]
    NotFound { path: PathBuf },

    #[error("{path}: outside the project root")]
    OutsideRoot { path: PathBuf },

    #[error("{0}: invalid path")]
    InvalidPath(String),

    #[error("{path}: {size} bytes exceeds the {limit} byte limit")]
    TooLarge {
        path: PathBuf,
        size: u64,
        limit: u64,
    },

    #[error("{path}: binary file")]
    Binary { path: PathBuf },

    #[error("{path}: changed on disk since it was opened")]
    Conflict { path: PathBuf },

    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{0}")]
    Invalid(String),
}

/// A stable, serializable error category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    NotFound,
    PermissionDenied,
    InvalidInput,
    TooLarge,
    Binary,
    Conflict,
    Io,
}

impl CoreError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        let path = path.into();
        if source.kind() == std::io::ErrorKind::NotFound {
            return Self::NotFound { path };
        }
        Self::Io { path, source }
    }

    pub fn code(&self) -> ErrorCode {
        match self {
            Self::NotFound { .. } => ErrorCode::NotFound,
            Self::OutsideRoot { .. } => ErrorCode::PermissionDenied,
            Self::InvalidPath(_) | Self::Invalid(_) => ErrorCode::InvalidInput,
            Self::TooLarge { .. } => ErrorCode::TooLarge,
            Self::Binary { .. } => ErrorCode::Binary,
            Self::Conflict { .. } => ErrorCode::Conflict,
            Self::Io { source, .. } if source.kind() == std::io::ErrorKind::PermissionDenied => {
                ErrorCode::PermissionDenied
            }
            Self::Io { .. } => ErrorCode::Io,
        }
    }
}

pub type Result<T, E = CoreError> = std::result::Result<T, E>;
