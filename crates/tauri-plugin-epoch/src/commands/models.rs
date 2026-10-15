use std::sync::Arc;

use tauri::State;

use epoch_core::model_file::{self, ModelFileInfo};

use crate::error::{blocking, CmdResult};
use crate::state::AppState;

/// What a `.safetensors` or `.onnx` file in the project declares: tensors,
/// shapes, dtypes, parameter count and (ONNX) the graph's interface and
/// operators. Headers only; the weights are never loaded.
#[tauri::command]
pub async fn model_inspect(
    path: String,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<ModelFileInfo> {
    let state = state.inner().clone();
    blocking(move || Ok(model_file::inspect(&state.project()?.root, &path)?)).await
}
