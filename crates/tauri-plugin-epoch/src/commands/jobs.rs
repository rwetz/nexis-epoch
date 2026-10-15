//! Running things: plan a job, start it once the user confirmed that exact
//! plan, follow its events, pause, resume, stop.
//!
//! Nothing here runs on its own. `job_plan` only looks at files; `job_start`
//! takes the plan the user confirmed and refuses if it no longer matches.
//! The runtime and the launch logic are in `epoch-runtime`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::State;

use epoch_core::project;
use epoch_runtime::prepare::{self, Context, Prepared, Target};
use epoch_runtime::{
    split_args, CommandPlan, Control, EventPage, JobId, JobInfo, JobNotice, LaunchConsent, Runtime,
    RuntimeError, StopMode, DEFAULT_STOP_TIMEOUT,
};

use crate::error::{blocking, CmdResult, CommandError};
use crate::state::{AppState, OpenProject};

/// What the user asked to run. Paths are relative to the open project.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JobRequest {
    /// Train with Axon from this `train.toml`.
    Axon { config: String },
    /// Run this `.py` file; `args` as typed (split like a shell would,
    /// without expanding anything).
    PythonScript {
        script: String,
        #[serde(default)]
        args: String,
    },
}

/// The confirmation dialog's content.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedJob {
    #[serde(flatten)]
    pub prepared: Prepared,
    pub plan: CommandPlan,
    pub command_line: String,
}

impl From<RuntimeError> for CommandError {
    fn from(e: RuntimeError) -> Self {
        let code = match &e {
            RuntimeError::UnknownJob(_) => "not_found",
            RuntimeError::NotControllable(_) | RuntimeError::Finished(_) => "invalid_input",
            RuntimeError::Spawn { .. } | RuntimeError::Io(_) => "io",
        };
        Self::new(code, e.to_string())
    }
}

fn prepare_request(
    state: &AppState,
    request: &JobRequest,
) -> CmdResult<(Arc<OpenProject>, Prepared)> {
    let project = state.project()?;
    let root = project.root.path().to_path_buf();
    let target = match request {
        JobRequest::Axon { config } => Target::Axon {
            config: project.root.resolve(config)?,
        },
        JobRequest::PythonScript { script, args } => Target::PythonScript {
            script: project.root.resolve(script)?,
            args: split_args(args).map_err(|e| CommandError::new("invalid_input", e))?,
        },
    };
    let interpreter = state.settings().python.interpreter;
    let ctx = Context {
        venvs: project
            .summary
            .environments
            .iter()
            .map(|env| root.join(&env.path))
            .collect(),
        path_var: std::env::var_os("PATH"),
        managed_dir: state.paths.nexis_engine_dir.clone(),
        configured_python: (!interpreter.is_empty()).then(|| PathBuf::from(interpreter)),
        project_root: root,
    };
    let prepared = prepare::prepare(target, &ctx).map_err(|e| CommandError::new("not_found", e))?;
    Ok((project, prepared))
}

/// The exact command a request would run, and where its engine or
/// interpreter was found. Reads files only.
#[tauri::command]
pub async fn job_plan(
    request: JobRequest,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<PlannedJob> {
    let state = state.inner().clone();
    blocking(move || {
        let (_, prepared) = prepare_request(&state, &request)?;
        let plan = prepared.plan();
        Ok(PlannedJob {
            command_line: plan.command_line(),
            plan,
            prepared,
        })
    })
    .await
}

/// Start a job the user confirmed. `confirmed` is the plan the dialog
/// showed; if the request now plans differently (a venv appeared, Settings
/// changed), nothing starts and the user is asked again. `action` names
/// the control the user used, for the job's consent record.
#[tauri::command]
pub async fn job_start(
    request: JobRequest,
    confirmed: CommandPlan,
    action: String,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<JobInfo> {
    let state = state.inner().clone();
    blocking(move || {
        let (project, prepared) = prepare_request(&state, &request)?;
        if prepared.plan() != confirmed {
            return Err(CommandError::new(
                "conflict",
                "The command changed since you confirmed it. Review it and confirm again.",
            ));
        }
        let at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let consent = LaunchConsent::from_user_action(action, at_ms);
        let scope = project.summary.root.clone();
        let id = state.jobs.start(prepared.into_spec(scope, consent));
        Ok(state.jobs.info(id)?)
    })
    .await
}

/// Every job this app session started, newest first (finished ones are
/// kept for a while).
#[tauri::command]
pub fn jobs_list(state: State<'_, Arc<AppState>>) -> Vec<JobInfo> {
    state.jobs.jobs()
}

/// A job's events after the cursor `after` (0 for all), at most `max`.
#[tauri::command]
pub fn job_events(
    id: u64,
    after: u64,
    max: Option<usize>,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<EventPage> {
    Ok(state
        .jobs
        .events(JobId(id), after, max.unwrap_or(2_000).min(10_000))?)
}

/// Stop a job: ask it to finish (protocol engines save and finalise the
/// run), and kill it if it hasn't after the timeout. `force` kills now.
#[tauri::command]
pub fn job_stop(id: u64, force: bool, state: State<'_, Arc<AppState>>) -> CmdResult<()> {
    let mode = if force {
        StopMode::Kill
    } else {
        StopMode::Graceful {
            timeout: DEFAULT_STOP_TIMEOUT,
        }
    };
    Ok(state.jobs.stop(JobId(id), mode)?)
}

/// Pause a protocol engine at its next epoch boundary.
#[tauri::command]
pub fn job_pause(id: u64, state: State<'_, Arc<AppState>>) -> CmdResult<()> {
    Ok(state.jobs.control(JobId(id), Control::Pause)?)
}

#[tauri::command]
pub fn job_resume(id: u64, state: State<'_, Arc<AppState>>) -> CmdResult<()> {
    Ok(state.jobs.control(JobId(id), Control::Resume)?)
}

/// The runtime's observer: keep the run index current while jobs run, so
/// a run is listed as soon as it starts and its final state lands when it
/// finishes, whether or not a window is watching. Only for jobs of the
/// project that is open now.
pub(crate) fn on_notice(state: &AppState, info: &JobInfo, notice: &JobNotice) {
    let Ok(project) = state.project() else {
        return;
    };
    if project.summary.root != info.scope {
        return;
    }
    let result = match notice {
        JobNotice::RunStarted { run_dir, .. } => {
            let project = ensure_store_known(state, project, run_dir);
            super::project::reindex(state, &project)
        }
        JobNotice::Finished(_) if info.kind == "axon" => super::project::reindex(state, &project),
        JobNotice::Finished(_) => return,
    };
    if let Err(e) = result {
        eprintln!("epoch: indexing after job {:?}: {}", info.id, e.message);
    }
}

/// A first run creates its project's `.nexis-ml/runs`; detection hasn't
/// seen that store yet, so detect again (as Rescan does) before indexing.
fn ensure_store_known(
    state: &AppState,
    project: Arc<OpenProject>,
    run_dir: &Path,
) -> Arc<OpenProject> {
    let root = project.root.path();
    let known = project
        .summary
        .experiment_stores
        .iter()
        .any(|s| run_dir.starts_with(root.join(&s.runs_dir)));
    if known {
        return project;
    }
    let summary = project::detect(&project.root);
    let fresh = Arc::new(OpenProject::new(project.root.clone(), summary));
    *state.project.write().unwrap_or_else(|e| e.into_inner()) = Some(fresh.clone());
    fresh
}
