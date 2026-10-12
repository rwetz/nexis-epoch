//! Epoch's framework-free core.
//!
//! Nothing here knows about Tauri, React, the workspace layout library or
//! DuckDB, so the same crate can sit under a native (GPUI) frontend later.

pub mod error;
pub mod fs;
pub mod nexis_ml;
pub mod paths;
pub mod project;

pub use error::{CoreError, ErrorCode, Result};
pub use paths::ProjectRoot;
