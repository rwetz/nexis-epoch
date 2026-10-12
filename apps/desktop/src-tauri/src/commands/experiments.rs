use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::State;

use epoch_core::nexis_ml::run_store::{self, RunDetail};
use epoch_store::{IndexStats, IndexedRun};

use crate::error::{blocking, CmdResult, CommandError};
use crate::state::{AppState, OpenProject};

/// Accept a run directory only if it is inside the open project and sits in
/// a `.nexis-ml/runs` folder; return it with its ML project directory.
fn locate_run(project: &OpenProject, run_dir: &str) -> CmdResult<(PathBuf, PathBuf)> {
    let run = project.root.contain(Path::new(run_dir))?;
    let runs = run.parent();
    let store = runs.and_then(Path::parent);
    let ok = runs.and_then(Path::file_name).is_some_and(|n| n == "runs")
        && store
            .and_then(Path::file_name)
            .is_some_and(|n| n == ".nexis-ml");
    match (ok, store.and_then(Path::parent)) {
        (true, Some(project_dir)) => Ok((run.clone(), project_dir.to_path_buf())),
        _ => Err(CommandError::new(
            "invalid_input",
            format!("{run_dir} is not a Nexis ML run directory"),
        )),
    }
}

#[tauri::command]
pub async fn experiments_index(state: State<'_, Arc<AppState>>) -> CmdResult<IndexStats> {
    let state = state.inner().clone();
    blocking(move || {
        let project = state.project()?;
        super::project::reindex(&state, &project)
    })
    .await
}

#[tauri::command]
pub async fn experiments_list(state: State<'_, Arc<AppState>>) -> CmdResult<Vec<IndexedRun>> {
    let state = state.inner().clone();
    blocking(move || {
        let root = state.project()?.root.path().display().to_string();
        state.with_store(|s| Ok(s.runs_for_project(&root)?))
    })
    .await
}

/// Metrics, logs, artifacts and checkpoints for one run, read from its files.
#[tauri::command]
pub async fn experiment_detail(
    run_dir: String,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<RunDetail> {
    let state = state.inner().clone();
    blocking(move || {
        let (run, project_dir) = locate_run(&state.project()?, &run_dir)?;
        Ok(run_store::read_run_detail(&project_dir, &run)?)
    })
    .await
}

/// A JSON artifact (confusion matrix, weights) by its run-relative path.
#[tauri::command]
pub async fn experiment_artifact(
    run_dir: String,
    path: String,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<serde_json::Value> {
    let state = state.inner().clone();
    blocking(move || {
        let (run, _) = locate_run(&state.project()?, &run_dir)?;
        Ok(run_store::read_artifact_json(&run, &path)?)
    })
    .await
}
