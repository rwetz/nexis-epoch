use serde::Serialize;

/// What every command returns on failure: a stable `code` the frontend can
/// branch on, and a human-readable `message`.
#[derive(Debug, Serialize)]
pub struct CommandError {
    pub code: &'static str,
    pub message: String,
}

pub type CmdResult<T> = Result<T, CommandError>;

impl CommandError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn no_project() -> Self {
        Self::new("no_project", "Open a project first.")
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new("internal", message)
    }
}

impl From<epoch_core::CoreError> for CommandError {
    fn from(e: epoch_core::CoreError) -> Self {
        use epoch_core::ErrorCode as C;
        let code = match e.code() {
            C::NotFound => "not_found",
            C::PermissionDenied => "permission_denied",
            C::InvalidInput => "invalid_input",
            C::TooLarge => "too_large",
            C::Binary => "binary",
            C::Conflict => "conflict",
            C::Io => "io",
        };
        Self::new(code, e.to_string())
    }
}

impl From<epoch_store::StoreError> for CommandError {
    fn from(e: epoch_store::StoreError) -> Self {
        match e {
            epoch_store::StoreError::Core(c) => c.into(),
            other => Self::new("database", other.to_string()),
        }
    }
}

impl From<epoch_data::DataError> for CommandError {
    fn from(e: epoch_data::DataError) -> Self {
        use epoch_data::DataError as D;
        let code = match &e {
            D::Query(_) => "query",
            D::Timeout(_) => "timeout",
            D::Cancelled => "cancelled",
            D::Invalid(_) => "invalid_input",
            D::Core(_) => "io",
        };
        if let D::Core(c) = e {
            return c.into();
        }
        Self::new(code, e.to_string())
    }
}

impl From<epoch_config::EditError> for CommandError {
    fn from(e: epoch_config::EditError) -> Self {
        Self::new("invalid_input", e.to_string())
    }
}

impl From<tauri::Error> for CommandError {
    fn from(e: tauri::Error) -> Self {
        Self::internal(e.to_string())
    }
}

/// Run blocking work (filesystem, SQLite, DuckDB) off the async runtime's
/// worker threads so the UI's other requests keep flowing.
pub async fn blocking<T, F>(f: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> CmdResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| CommandError::internal(format!("background task failed: {e}")))?
}
