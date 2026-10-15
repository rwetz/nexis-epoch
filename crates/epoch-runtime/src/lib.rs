//! Running training jobs and scripts outside the UI process.
//!
//! - [`JobSpec::plan`] turns a job into the exact process it will start (a
//!   [`CommandPlan`]), so the user sees it before anything runs and tests
//!   can assert on it.
//! - [`LocalRuntime`] starts plans as child processes, streams their stdout
//!   and stderr line by line, parses Nexis ML protocol events as they
//!   arrive, and stops them gracefully (`{"cmd":"cancel"}`, then a kill
//!   after a timeout). On Windows every job runs in its own job object, so
//!   its children die with it; on Unix, in its own process group.
//! - [`engine`] finds Axon (the Nexis ML engine) and Python interpreters by
//!   looking at files, never by running them.
//!
//! Two rules are encoded in the types rather than left to callers:
//!
//! - A job can only be described with a [`LaunchConsent`], which is created
//!   from an explicit user action. Opening a project never runs its code.
//! - Engines that speak the Nexis ML protocol are launched with
//!   `--nexis-protocol`, so their stdout is machine-readable and every other
//!   process's stdout is treated as plain log lines.

pub mod engine;
mod lines;
mod local;
pub mod prepare;
mod sys;

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub use epoch_core::nexis_ml::protocol::{Control, Event as ProtocolEvent};
pub use lines::split_args;
pub use local::{JobNotice, LocalRuntime, Observer, RuntimeOptions};

/// Proof that a person asked for this launch. Deliberately has no
/// `Default`: the only constructor names the action that produced it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchConsent {
    action: String,
    at_ms: u64,
}

impl LaunchConsent {
    /// Record that `action` (e.g. "Run button in Experiment Explorer") was
    /// taken by the user at `at_ms`.
    pub fn from_user_action(action: impl Into<String>, at_ms: u64) -> Self {
        Self {
            action: action.into(),
            at_ms,
        }
    }

    pub fn action(&self) -> &str {
        &self.action
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct JobId(pub u64);

/// What to run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JobKind {
    /// Axon (or the legacy `nexis-ml` engine) training the ML project in
    /// the job's working directory from its `train.toml`, the way Nexis's
    /// Synapse starts it.
    Axon { engine: PathBuf },
    /// Any Python script, with the interpreter the user chose.
    PythonScript {
        interpreter: PathBuf,
        script: PathBuf,
        args: Vec<String>,
    },
    /// A program and its arguments as given. For other runtimes and tests;
    /// the app only builds the two kinds above.
    Process {
        program: PathBuf,
        args: Vec<String>,
        speaks_protocol: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSpec {
    /// Shown in the jobs list ("Train tabular", "Run scripts/eval.py").
    pub title: String,
    /// Who the job belongs to; the app uses the project root.
    pub scope: String,
    pub kind: JobKind,
    pub cwd: PathBuf,
    /// Extra environment variables on top of the app's own. Never
    /// credentials in plain text.
    pub env: Vec<(String, String)>,
    pub consent: LaunchConsent,
}

/// The exact process a job will start, as data, so it can be shown to the
/// user before it runs and asserted on in tests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandPlan {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    /// Set in addition to the inherited environment.
    pub env: Vec<(String, String)>,
    /// stdout carries Nexis ML protocol events, and stdin takes control
    /// messages (cancel, pause, resume).
    pub speaks_protocol: bool,
}

impl CommandPlan {
    /// The plan as one line a shell user would recognise, quoting
    /// arguments that contain spaces or quotes. For display only: the
    /// runtime passes `args` to the OS as a list, never through a shell.
    pub fn command_line(&self) -> String {
        std::iter::once(self.program.display().to_string())
            .chain(self.args.iter().cloned())
            .map(|a| quote_for_display(&a))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn quote_for_display(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '"', '\'']) {
        return arg.to_string();
    }
    format!("\"{}\"", arg.replace('"', "\\\""))
}

impl JobSpec {
    pub fn plan(&self) -> CommandPlan {
        plan_for(&self.kind, &self.cwd, &self.env)
    }
}

/// The process a job of `kind` started in `cwd` with `extra_env` will be.
pub fn plan_for(
    kind: &JobKind,
    cwd: &std::path::Path,
    extra_env: &[(String, String)],
) -> CommandPlan {
    {
        let (program, args, speaks_protocol) = match kind {
            // `<engine> --nexis-protocol train .` from the ML project's
            // folder: what Nexis runs (src/modules/ml/lib/engine-bridge.ts).
            JobKind::Axon { engine } => (
                engine.clone(),
                vec!["--nexis-protocol".into(), "train".into(), ".".into()],
                true,
            ),
            JobKind::PythonScript {
                interpreter,
                script,
                args,
            } => {
                // -u: unbuffered, so output streams as it is printed.
                let mut all = vec!["-u".to_string(), script.display().to_string()];
                all.extend(args.iter().cloned());
                (interpreter.clone(), all, false)
            }
            JobKind::Process {
                program,
                args,
                speaks_protocol,
            } => (program.clone(), args.clone(), *speaks_protocol),
        };
        let mut env = extra_env.to_vec();
        match kind {
            JobKind::Axon { .. } => {
                env.push(("NEXIS_ML_PROTOCOL".into(), "1".into()));
                // The legacy engine is Python: keep its output UTF-8 when
                // stdout is a pipe on Windows.
                env.push(("PYTHONIOENCODING".into(), "utf-8".into()));
            }
            JobKind::PythonScript { .. } => {
                env.push(("PYTHONUNBUFFERED".into(), "1".into()));
                env.push(("PYTHONIOENCODING".into(), "utf-8".into()));
            }
            JobKind::Process { .. } => {}
        }
        CommandPlan {
            program,
            args,
            cwd: cwd.to_path_buf(),
            env,
            speaks_protocol,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum JobStatus {
    /// Waiting for a free slot (the concurrency limit).
    Queued,
    Starting,
    Running {
        pid: u32,
    },
    /// Pause was requested; protocol engines pause at the next epoch
    /// boundary.
    Paused {
        pid: u32,
    },
    /// Stop was requested; the process has not exited yet.
    Stopping {
        pid: u32,
    },
    Exited {
        /// `None` when the process was ended by a signal.
        code: Option<i32>,
    },
    /// Stopped at the user's request. `forced`: it had to be killed.
    Cancelled {
        forced: bool,
    },
    Failed {
        message: String,
    },
}

impl JobStatus {
    pub fn is_finished(&self) -> bool {
        matches!(
            self,
            Self::Exited { .. } | Self::Cancelled { .. } | Self::Failed { .. }
        )
    }

    /// A process exists (or is being started) for the job.
    pub fn holds_slot(&self) -> bool {
        matches!(
            self,
            Self::Starting | Self::Running { .. } | Self::Paused { .. } | Self::Stopping { .. }
        )
    }

    pub fn pid(&self) -> Option<u32> {
        match self {
            Self::Running { pid } | Self::Paused { pid } | Self::Stopping { pid } => Some(*pid),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JobEvent {
    Status(JobStatus),
    Stdout { line: String },
    Stderr { line: String },
    Protocol { event: ProtocolEvent },
}

/// One event in a job's log, numbered so a reader can resume after the
/// last one it saw.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobRecord {
    pub seq: u64,
    pub at_ms: u64,
    pub event: JobEvent,
}

/// Events after a cursor.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventPage {
    pub records: Vec<JobRecord>,
    /// Pass back as `after` to continue.
    pub next: u64,
    /// Events that were dropped from the job's buffer before they could be
    /// read (the buffer keeps the most recent ones).
    pub dropped: u64,
    /// The job has finished and every event has been returned.
    pub done: bool,
}

/// What the protocol has told us about a job's run so far.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobProgress {
    pub run_id: Option<String>,
    /// The run's directory, when the engine reported one (`run.started`'s
    /// `dir`) or it could be derived from the working directory.
    pub run_dir: Option<PathBuf>,
    pub device: Option<String>,
    pub epoch: Option<u64>,
    pub total_epochs: Option<u64>,
    /// `run.finished`'s status ("completed", "cancelled", "failed").
    pub outcome: Option<String>,
    pub metric_events: u64,
}

/// A job as the jobs list shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobInfo {
    pub id: JobId,
    pub title: String,
    pub scope: String,
    /// "axon", "python_script" or "process".
    pub kind: &'static str,
    pub plan: CommandPlan,
    pub consent: LaunchConsent,
    pub status: JobStatus,
    /// Accepts pause/resume (protocol engines only).
    pub controllable: bool,
    pub queued_at_ms: u64,
    pub started_at_ms: Option<u64>,
    pub finished_at_ms: Option<u64>,
    pub progress: JobProgress,
    /// stdout + stderr lines so far.
    pub output_lines: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopMode {
    /// Ask nicely (`{"cmd":"cancel"}` for protocol engines, which checkpoint
    /// and finalise the run; SIGTERM to the process group on Unix), then
    /// kill after the timeout. Windows has no polite signal for a
    /// windowless process, so a plain script is killed at once there.
    Graceful {
        timeout: Duration,
    },
    Kill,
}

pub const DEFAULT_STOP_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("no job {0:?}")]
    UnknownJob(JobId),
    #[error("{0:?} does not accept control messages")]
    NotControllable(JobId),
    #[error("{0:?} has already finished")]
    Finished(JobId),
    #[error("could not start {program}: {message}")]
    Spawn { program: String, message: String },
    #[error("could not reach the process: {0}")]
    Io(String),
}

/// What any runtime — local processes today, perhaps a remote worker or an
/// in-process Rust trainer later — must provide.
pub trait Runtime: Send + Sync {
    /// Queue a job; it starts as soon as the concurrency limit allows.
    fn start(&self, spec: JobSpec) -> JobId;
    fn control(&self, id: JobId, message: Control) -> Result<(), RuntimeError>;
    fn stop(&self, id: JobId, mode: StopMode) -> Result<(), RuntimeError>;
    fn status(&self, id: JobId) -> Result<JobStatus, RuntimeError>;
    fn jobs(&self) -> Vec<JobInfo>;
    fn events(&self, id: JobId, after: u64, max: usize) -> Result<EventPage, RuntimeError>;
}

/// Turn one stdout line into an event: protocol events for engines that
/// speak the protocol, plain lines otherwise (or when a line isn't one).
pub fn classify_stdout(line: &str, speaks_protocol: bool) -> JobEvent {
    if speaks_protocol {
        if let Some(event) = epoch_core::nexis_ml::protocol::parse_line(line) {
            return JobEvent::Protocol { event };
        }
    }
    JobEvent::Stdout {
        line: line.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn consent() -> LaunchConsent {
        LaunchConsent::from_user_action("test", 0)
    }

    fn spec(kind: JobKind, cwd: &str) -> JobSpec {
        JobSpec {
            title: "t".into(),
            scope: String::new(),
            kind,
            cwd: cwd.into(),
            env: vec![],
            consent: consent(),
        }
    }

    #[test]
    fn axon_jobs_request_the_protocol_like_nexis_does() {
        let plan = spec(
            JobKind::Axon {
                engine: "axon".into(),
            },
            "proj",
        )
        .plan();
        assert_eq!(plan.args, ["--nexis-protocol", "train", "."]);
        assert_eq!(plan.cwd, PathBuf::from("proj"));
        assert!(plan.speaks_protocol);
        assert!(plan.env.contains(&("NEXIS_ML_PROTOCOL".into(), "1".into())));
    }

    #[test]
    fn python_scripts_run_unbuffered_with_their_args() {
        let plan = spec(
            JobKind::PythonScript {
                interpreter: "python".into(),
                script: "train.py".into(),
                args: vec!["--epochs".into(), "3".into()],
            },
            ".",
        )
        .plan();
        assert_eq!(plan.args, ["-u", "train.py", "--epochs", "3"]);
        assert!(!plan.speaks_protocol);
        assert!(plan.env.contains(&("PYTHONUNBUFFERED".into(), "1".into())));
    }

    #[test]
    fn command_lines_quote_what_needs_it() {
        let plan = spec(
            JobKind::PythonScript {
                interpreter: "C:/My Envs/python.exe".into(),
                script: "a.py".into(),
                args: vec!["--name".into(), "say \"hi\"".into(), String::new()],
            },
            ".",
        )
        .plan();
        assert_eq!(
            plan.command_line(),
            r#""C:/My Envs/python.exe" -u a.py --name "say \"hi\"" """#
        );
    }

    #[test]
    fn stdout_is_classified_by_engine_kind() {
        let line = r#"{"ev":"epoch","run":"r","epoch":2,"of":5}"#;
        assert!(matches!(
            classify_stdout(line, true),
            JobEvent::Protocol { .. }
        ));
        assert!(matches!(
            classify_stdout(line, false),
            JobEvent::Stdout { .. }
        ));
        assert!(matches!(
            classify_stdout("loss 0.3", true),
            JobEvent::Stdout { .. }
        ));
    }

    #[test]
    fn events_serialize_as_the_frontend_expects() {
        let status = serde_json::to_value(JobEvent::Status(JobStatus::Running { pid: 7 })).unwrap();
        assert_eq!(
            status,
            serde_json::json!({"type": "status", "state": "running", "pid": 7})
        );
        let line = serde_json::to_value(JobEvent::Stderr { line: "x".into() }).unwrap();
        assert_eq!(line, serde_json::json!({"type": "stderr", "line": "x"}));
        assert_eq!(serde_json::to_value(JobId(3)).unwrap(), 3);
    }
}
