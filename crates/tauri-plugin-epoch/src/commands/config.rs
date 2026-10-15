use epoch_config::{ParseReport, PathSeg, ScalarValue};

use crate::error::CmdResult;

/// Parse TOML text into diagnostics and a tree. Pure; no file access.
#[tauri::command]
pub fn toml_parse(text: String) -> ParseReport {
    epoch_config::parse(&text)
}

/// Replace one scalar, preserving formatting; returns the new text for the
/// editor (unsaved until the user saves).
#[tauri::command]
pub fn toml_set_value(text: String, path: Vec<PathSeg>, value: ScalarValue) -> CmdResult<String> {
    Ok(epoch_config::set_value(&text, &path, &value)?)
}
