//! Tauri commands. Each is a thin adapter: validate, hop off the async
//! runtime for blocking work, and call into an `epoch-*` crate. Logic that
//! is worth testing lives in those crates, not here.

pub mod config;
pub mod data;
pub mod experiments;
pub mod project;
pub mod settings;
pub mod system;
