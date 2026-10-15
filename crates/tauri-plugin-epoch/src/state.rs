use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use epoch_config::AppSettings;
use epoch_core::project::ProjectSummary;
use epoch_core::ProjectRoot;
use epoch_data::{CancelHandle, DataEngine, Limits};
use epoch_store::Store;

use crate::error::{CmdResult, CommandError};

pub struct AppPaths {
    /// settings.toml
    pub config_dir: PathBuf,
    /// epoch.db, layouts/
    pub data_dir: PathBuf,
}

/// The project open in this window.
pub struct OpenProject {
    pub root: ProjectRoot,
    pub summary: ProjectSummary,
    /// Created on first use, dropped with the project.
    data: Mutex<Option<Arc<DataEngine>>>,
}

impl OpenProject {
    pub fn new(root: ProjectRoot, summary: ProjectSummary) -> Self {
        Self {
            root,
            summary,
            data: Mutex::new(None),
        }
    }

    pub fn data(&self, settings: &AppSettings) -> CmdResult<Arc<DataEngine>> {
        let mut slot = self.data.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(engine) = slot.as_ref() {
            return Ok(engine.clone());
        }
        let d = &settings.data;
        let engine = Arc::new(DataEngine::open(
            &self.root,
            Limits {
                memory_limit_mb: d.memory_limit_mb,
                threads: d.threads,
                timeout: Duration::from_secs(u64::from(d.query_timeout_secs)),
                max_rows: d.query_row_limit.max(d.page_size) as usize,
            },
        )?);
        *slot = Some(engine.clone());
        Ok(engine)
    }

    /// Settings changed: rebuild the engine (and its limits) on next use.
    pub fn reset_data(&self) {
        *self.data.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

pub struct AppState {
    pub paths: AppPaths,
    pub store: Mutex<Store>,
    pub settings: RwLock<AppSettings>,
    pub project: RwLock<Option<Arc<OpenProject>>>,
    /// Running queries by the id the frontend gave them, for cancellation.
    pub queries: Mutex<HashMap<String, CancelHandle>>,
    pub system: Mutex<crate::commands::system::SystemProbe>,
}

impl AppState {
    pub fn project(&self) -> CmdResult<Arc<OpenProject>> {
        self.project
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or_else(CommandError::no_project)
    }

    pub fn settings(&self) -> AppSettings {
        self.settings
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn with_store<T>(&self, f: impl FnOnce(&mut Store) -> CmdResult<T>) -> CmdResult<T> {
        let mut store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut store)
    }

    /// Register a cancellable query; the returned guard unregisters it.
    pub fn track_query(self: &Arc<Self>, id: Option<String>) -> QueryTracker {
        QueryTracker {
            state: self.clone(),
            id,
        }
    }
}

pub struct QueryTracker {
    state: Arc<AppState>,
    id: Option<String>,
}

impl QueryTracker {
    pub fn on_start(&self) -> impl FnOnce(CancelHandle) + '_ {
        move |handle| {
            if let Some(id) = &self.id {
                self.state
                    .queries
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(id.clone(), handle);
            }
        }
    }
}

impl Drop for QueryTracker {
    fn drop(&mut self) {
        if let Some(id) = &self.id {
            self.state
                .queries
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(id);
        }
    }
}
