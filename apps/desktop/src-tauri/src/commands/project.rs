use std::path::Path;
use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use epoch_core::fs::{self, DirListing, FileKind, ListOptions, TextFile};
use epoch_core::project::{self, ProjectSummary};
use epoch_core::ProjectRoot;
use epoch_store::{IndexStats, RecentProject};

use crate::error::{blocking, CmdResult};
use crate::state::{AppState, OpenProject};

/// Text files larger than this are not opened in an editor.
const MAX_TEXT_BYTES: u64 = 5 * 1024 * 1024;
const MAX_INDEXED_FILES: usize = 20_000;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Opened {
    pub summary: ProjectSummary,
    /// Present when experiments were indexed as part of opening.
    pub index: Option<IndexStats>,
}

/// Re-read every experiment store the summary lists into the index.
pub(crate) fn reindex(state: &AppState, project: &OpenProject) -> CmdResult<IndexStats> {
    let base = project.root.path();
    let dirs: Vec<_> = project
        .summary
        .experiment_stores
        .iter()
        .map(|s| {
            if s.project_dir.is_empty() {
                base.to_path_buf()
            } else {
                base.join(&s.project_dir)
            }
        })
        .collect();
    let refs: Vec<&Path> = dirs.iter().map(|d| d.as_path()).collect();
    let root = base.display().to_string();
    state.with_store(|store| Ok(store.reindex_project(&root, &refs)?))
}

/// Open a folder as the project. Reads it; never runs anything in it.
#[tauri::command]
pub async fn project_open(path: String, state: State<'_, Arc<AppState>>) -> CmdResult<Opened> {
    let state = state.inner().clone();
    blocking(move || {
        let root = ProjectRoot::open(&path)?;
        let summary = project::detect(&root);
        let project = Arc::new(OpenProject::new(root, summary.clone()));
        state.with_store(|s| Ok(s.touch_project(&summary.root, &summary.name)?))?;
        let index = if state.settings().experiments.index_on_open
            && !summary.experiment_stores.is_empty()
        {
            Some(reindex(&state, &project)?)
        } else {
            None
        };
        *state.project.write().unwrap_or_else(|e| e.into_inner()) = Some(project);
        state
            .queries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        Ok(Opened { summary, index })
    })
    .await
}

#[tauri::command]
pub fn project_close(state: State<'_, Arc<AppState>>) {
    *state.project.write().unwrap_or_else(|e| e.into_inner()) = None;
}

#[tauri::command]
pub fn project_current(state: State<'_, Arc<AppState>>) -> Option<ProjectSummary> {
    state
        .project
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|p| p.summary.clone())
}

/// Detect again (files changed on disk) without reopening.
#[tauri::command]
pub async fn project_rescan(state: State<'_, Arc<AppState>>) -> CmdResult<ProjectSummary> {
    let state = state.inner().clone();
    blocking(move || {
        let current = state.project()?;
        let summary = project::detect(&current.root);
        let fresh = Arc::new(OpenProject::new(current.root.clone(), summary.clone()));
        *state.project.write().unwrap_or_else(|e| e.into_inner()) = Some(fresh);
        Ok(summary)
    })
    .await
}

#[tauri::command]
pub async fn project_recent(state: State<'_, Arc<AppState>>) -> CmdResult<Vec<RecentProject>> {
    let state = state.inner().clone();
    blocking(move || state.with_store(|s| Ok(s.recent_projects(12)?))).await
}

#[tauri::command]
pub async fn project_forget(path: String, state: State<'_, Arc<AppState>>) -> CmdResult<()> {
    let state = state.inner().clone();
    blocking(move || state.with_store(|s| Ok(s.forget_project(&path)?))).await
}

#[tauri::command]
pub async fn fs_list_dir(path: String, state: State<'_, Arc<AppState>>) -> CmdResult<DirListing> {
    let state = state.inner().clone();
    blocking(move || {
        let project = state.project()?;
        let opts = ListOptions {
            show_hidden: state.settings().explorer.show_hidden,
            ..Default::default()
        };
        Ok(fs::list_dir(&project.root, &path, opts)?)
    })
    .await
}

#[tauri::command]
pub async fn fs_read_text(path: String, state: State<'_, Arc<AppState>>) -> CmdResult<TextFile> {
    let state = state.inner().clone();
    blocking(move || {
        Ok(fs::read_text(
            &state.project()?.root,
            &path,
            MAX_TEXT_BYTES,
        )?)
    })
    .await
}

/// Save atomically; refuses with `conflict` if the file changed on disk
/// since `expected_hash` was read.
#[tauri::command]
pub async fn fs_write_text(
    path: String,
    content: String,
    expected_hash: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<TextFile> {
    let state = state.inner().clone();
    blocking(move || {
        Ok(fs::write_text(
            &state.project()?.root,
            &path,
            &content,
            expected_hash.as_deref(),
        )?)
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileIndex {
    pub files: Vec<IndexedFile>,
    pub truncated: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexedFile {
    pub path: String,
    pub kind: FileKind,
}

/// Every file path in the project (bounded), for global search.
#[tauri::command]
pub async fn project_files(state: State<'_, Arc<AppState>>) -> CmdResult<FileIndex> {
    let state = state.inner().clone();
    blocking(move || {
        let project = state.project()?;
        let mut files = Vec::new();
        let truncated = fs::walk(
            project.root.path(),
            12,
            MAX_INDEXED_FILES * 2,
            |p, meta, _| {
                if meta.is_file() && files.len() < MAX_INDEXED_FILES {
                    if let Some(rel) = project.root.relative(p) {
                        files.push(IndexedFile {
                            kind: FileKind::from_path(p),
                            path: rel,
                        });
                    }
                }
            },
        );
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(FileIndex {
            truncated: truncated || files.len() >= MAX_INDEXED_FILES,
            files,
        })
    })
    .await
}
