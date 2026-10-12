mod commands;
mod error;
mod state;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use tauri::Manager;

use crate::state::{AppPaths, AppState};

#[cfg(target_os = "linux")]
fn is_nvidia() -> bool {
    std::path::Path::new("/proc/driver/nvidia/version").exists()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "linux")]
    {
        // NVIDIA + Wayland: WebKitGTK's DMA-BUF renderer crashes at first
        // paint. Forcing Mesa's EGL keeps it alive *and* keeps window alpha
        // (the 12px transparent corners); disabling DMA-BUF is the fallback
        // that keeps it alive but paints the corners black.
        // @nexis/design docs/PITFALLS.md §2 — do not re-litigate.
        if is_nvidia()
            && std::env::var_os("APP_KEEP_HW_ACCEL").is_none()
            && std::env::var_os("__EGL_VENDOR_LIBRARY_FILENAMES").is_none()
        {
            const MESA: &str = "/usr/share/glvnd/egl_vendor.d/50_mesa.json";
            if std::path::Path::new(MESA).exists() {
                std::env::set_var("__EGL_VENDOR_LIBRARY_FILENAMES", MESA);
            } else {
                std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1"); // alive > pretty
            }
        }
        // KWin otherwise draws a server-side title bar over our own.
        if std::env::var_os("GTK_CSD").is_none() {
            std::env::set_var("GTK_CSD", "1");
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let paths = AppPaths {
                config_dir: app.path().app_config_dir()?,
                data_dir: app.path().app_data_dir()?,
            };
            let store = epoch_store::Store::open(&paths.data_dir.join("epoch.db"))?;
            let (settings, warnings) = commands::settings::initial(&paths.config_dir);
            for w in warnings {
                eprintln!("settings: {w}");
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
            commands::system::system_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Nexis Epoch");
}
