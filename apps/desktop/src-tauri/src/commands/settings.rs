use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use epoch_config::settings::{self, LoadedSettings};
use epoch_config::AppSettings;

use crate::error::{blocking, CmdResult, CommandError};
use crate::state::AppState;

#[tauri::command]
pub async fn settings_get(state: State<'_, Arc<AppState>>) -> CmdResult<LoadedSettings> {
    let state = state.inner().clone();
    blocking(move || Ok(settings::load(&state.paths.config_dir))).await
}

#[tauri::command]
pub async fn settings_save(
    settings: AppSettings,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<AppSettings> {
    let state = state.inner().clone();
    blocking(move || {
        let saved = settings::save(&state.paths.config_dir, &settings)?;
        *state.settings.write().unwrap_or_else(|e| e.into_inner()) = saved.clone();
        // New limits apply to the next query.
        if let Ok(project) = state.project() {
            project.reset_data();
        }
        Ok(saved)
    })
    .await
}

const MAX_LAYOUT_BYTES: usize = 2 * 1024 * 1024;

fn layout_file(state: &AppState, workspace: &str) -> CmdResult<std::path::PathBuf> {
    let valid = !workspace.is_empty()
        && workspace.len() <= 64
        && workspace
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !valid {
        return Err(CommandError::new(
            "invalid_input",
            format!("bad workspace id {workspace:?}"),
        ));
    }
    Ok(state
        .paths
        .data_dir
        .join("layouts")
        .join(format!("{workspace}.json")))
}

/// A saved workspace layout (the frontend's versioned JSON envelope), or
/// `None` if this workspace was never saved.
#[tauri::command]
pub async fn layout_load(
    workspace: String,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<Option<String>> {
    let state = state.inner().clone();
    blocking(move || {
        let path = layout_file(&state, &workspace)?;
        match std::fs::read_to_string(&path) {
            Ok(text) => Ok(Some(text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(epoch_core::CoreError::io(&path, e).into()),
        }
    })
    .await
}

#[tauri::command]
pub async fn layout_save(
    workspace: String,
    json: String,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<()> {
    if json.len() > MAX_LAYOUT_BYTES {
        return Err(CommandError::new(
            "too_large",
            "layout document is too large",
        ));
    }
    serde_json::from_str::<serde_json::Value>(&json)
        .map_err(|e| CommandError::new("invalid_input", format!("layout is not JSON: {e}")))?;
    let state = state.inner().clone();
    blocking(move || {
        let path = layout_file(&state, &workspace)?;
        Ok(epoch_core::fs::atomic_write(&path, json.as_bytes())?)
    })
    .await
}

#[tauri::command]
pub async fn layout_delete(workspace: String, state: State<'_, Arc<AppState>>) -> CmdResult<()> {
    let state = state.inner().clone();
    blocking(move || {
        let path = layout_file(&state, &workspace)?;
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(epoch_core::CoreError::io(&path, e).into()),
        }
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: &'static str,
    pub config_dir: String,
    pub data_dir: String,
    pub settings_file: String,
}

#[tauri::command]
pub fn app_info(state: State<'_, Arc<AppState>>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        config_dir: state.paths.config_dir.display().to_string(),
        data_dir: state.paths.data_dir.display().to_string(),
        settings_file: state
            .paths
            .config_dir
            .join(settings::SETTINGS_FILE)
            .display()
            .to_string(),
    }
}

/// Settings were loaded before the window existed; expose the loaded copy.
pub fn initial(config_dir: &std::path::Path) -> (AppSettings, Vec<String>) {
    let loaded = settings::load(config_dir);
    (loaded.settings, loaded.warnings)
}
