//! Nexis Epoch's backend as a Tauri plugin.
//!
//! The standalone Epoch app and Nexis's Epoch window register this same
//! plugin, so both expose one command surface over the `epoch-*` crates and
//! a feature added here reaches both. The webview calls the commands as
//! `plugin:epoch|<command>` (apps/desktop/src/lib/ipc.ts); a window needs the
//! `epoch:default` permission in its capability.
//!
//! The commands stay thin adapters: validate, hop off the async runtime for
//! blocking work, call a crate (docs/ARCHITECTURE.md).

mod commands;
mod error;
mod state;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Manager, Runtime};

pub use error::{CmdResult, CommandError};
use state::{AppPaths, AppState};

/// The standalone app's bundle identifier. Its app-data and config folders
/// are named after it, and Epoch keeps them wherever it is hosted, so the
/// standalone app and the Nexis window share recent projects, the run index,
/// saved layouts and settings.toml.
pub const EPOCH_IDENTIFIER: &str = "app.nexis.epoch";

/// The plugin, with Epoch's state created on setup.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("epoch")
        .setup(|app, _api| {
            let path = app.path();
            let paths = AppPaths {
                config_dir: path.config_dir()?.join(EPOCH_IDENTIFIER),
                data_dir: path.data_dir()?.join(EPOCH_IDENTIFIER),
            };
            let store = epoch_store::Store::open(&paths.data_dir.join("epoch.db"))?;
            let (settings, warnings) = commands::settings::initial(&paths.config_dir);
            for w in warnings {
                eprintln!("epoch settings: {w}");
            }
            app.manage(Arc::new(AppState {
                paths,
                store: Mutex::new(store),
                settings: RwLock::new(settings),
                project: RwLock::new(None),
                queries: Mutex::new(HashMap::new()),
                system: Mutex::new(commands::system::SystemProbe::new()),
            }));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::settings::app_info,
            commands::settings::settings_get,
            commands::settings::settings_save,
            commands::settings::layout_load,
            commands::settings::layout_save,
            commands::settings::layout_delete,
            commands::project::project_open,
            commands::project::project_close,
            commands::project::project_current,
            commands::project::project_rescan,
            commands::project::project_recent,
            commands::project::project_forget,
            commands::project::project_files,
            commands::project::fs_list_dir,
            commands::project::fs_read_text,
            commands::project::fs_write_text,
            commands::config::toml_parse,
            commands::config::toml_set_value,
            commands::data::dataset_describe,
            commands::data::dataset_page,
            commands::data::dataset_stats,
            commands::data::sql_run,
            commands::data::sql_register_view,
            commands::data::query_cancel,
            commands::experiments::experiments_index,
            commands::experiments::experiments_list,
            commands::experiments::experiment_detail,
            commands::experiments::experiment_artifact,
            commands::models::model_inspect,
            commands::system::system_status,
        ])
        .build()
}
