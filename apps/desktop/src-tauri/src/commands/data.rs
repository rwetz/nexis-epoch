use std::sync::Arc;

use tauri::State;

use epoch_data::{DatasetInfo, Page, PageRequest, QueryResult};

use crate::error::{blocking, CmdResult};
use crate::state::AppState;

/// Every data command takes an optional `query_id`; passing the same id to
/// [`query_cancel`] interrupts it.
#[tauri::command]
pub async fn dataset_describe(
    path: String,
    query_id: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<DatasetInfo> {
    let state = state.inner().clone();
    blocking(move || {
        let engine = state.project()?.data(&state.settings())?;
        let tracker = state.track_query(query_id);
        Ok(engine.describe(&path, tracker.on_start())?)
    })
    .await
}

#[tauri::command]
pub async fn dataset_page(
    path: String,
    request: PageRequest,
    query_id: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<Page> {
    let state = state.inner().clone();
    blocking(move || {
        let engine = state.project()?.data(&state.settings())?;
        let tracker = state.track_query(query_id);
        Ok(engine.page(&path, &request, tracker.on_start())?)
    })
    .await
}

#[tauri::command]
pub async fn dataset_stats(
    path: String,
    query_id: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<QueryResult> {
    let state = state.inner().clone();
    blocking(move || {
        let engine = state.project()?.data(&state.settings())?;
        let tracker = state.track_query(query_id);
        Ok(engine.stats(&path, tracker.on_start())?)
    })
    .await
}

#[tauri::command]
pub async fn sql_run(
    sql: String,
    query_id: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<QueryResult> {
    let state = state.inner().clone();
    blocking(move || {
        let settings = state.settings();
        let engine = state.project()?.data(&settings)?;
        let tracker = state.track_query(query_id);
        Ok(engine.run_sql(
            &sql,
            settings.data.query_row_limit as usize,
            tracker.on_start(),
        )?)
    })
    .await
}

/// Expose a project dataset to the SQL editor as a view; returns its name.
#[tauri::command]
pub async fn sql_register_view(
    path: String,
    name: String,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<String> {
    let state = state.inner().clone();
    blocking(move || {
        Ok(state
            .project()?
            .data(&state.settings())?
            .register_view(&path, &name)?)
    })
    .await
}

/// Interrupt a running query. Unknown ids (already finished) are ignored.
#[tauri::command]
pub fn query_cancel(query_id: String, state: State<'_, Arc<AppState>>) {
    if let Some(handle) = state
        .queries
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&query_id)
    {
        handle.cancel();
    }
}
