//! Interfaces for running training jobs outside the UI process.
//!
//! Milestone 1 defines the shapes only: what a job is, the exact command it
//! would run, the events it produces and the operations a runtime offers.
//! Nothing here spawns a process, and nothing in the app calls it yet; see
//! docs/ROADMAP.md, "Training execution".
//!
//! Two rules are encoded in the types rather than left to callers:
//!
//! - A job can only be described with a [`LaunchConsent`], which is created
//!   from an explicit user action. Opening a project never runs its code.
//! - Engines that speak the Nexis ML protocol are launched with
//!   `--nexis-protocol`, so their stdout is machine-readable and every other
//!   engine's stdout is treated as plain log lines.

use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;

pub use epoch_core::nexis_ml::protocol::{Control, Event as ProtocolEvent};

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
pub struct JobId(pub u64);

/// What to run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JobKind {
    /// A Nexis ML engine (`nexis-ml` Python or `nexis-ml-rs`) training the
    /// project in `project_dir` from its `train.toml`.
    NexisMl {
        engine: PathBuf,
        project_dir: PathBuf,
    },
    /// Any Python script, with the interpreter the user chose.
    PythonScript {
        interpreter: PathBuf,
        script: PathBuf,
        args: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSpec {
    pub id: JobId,
    pub kind: JobKind,
    pub cwd: PathBuf,
    /// Extra environment variables. Never credentials in plain text.
    pub env: Vec<(String, String)>,
    pub consent: LaunchConsent,
}

/// The exact process a job would start, as data, so it can be shown to the
/// user before it runs and asserted on in tests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandPlan {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    /// stdout carries Nexis ML protocol events.
    pub speaks_protocol: bool,
}

impl JobSpec {
    pub fn plan(&self) -> CommandPlan {
        let (program, args, speaks_protocol) = match &self.kind {
            JobKind::NexisMl {
                engine,
                project_dir,
            } => (
                engine.clone(),
                vec![
                    "train".to_string(),
                    project_dir.display().to_string(),
                    "--nexis-protocol".to_string(),
                ],
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
        };
        let mut env = self.env.clone();
        if speaks_protocol {
            env.push(("NEXIS_ML_PROTOCOL".into(), "1".into()));
        }
        CommandPlan {
            program,
            args,
            cwd: self.cwd.clone(),
            env,
            speaks_protocol,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum JobStatus {
    Starting,
    Running { pid: u32 },
    Paused,
    Stopping,
    Exited { code: Option<i32> },
    Cancelled,
    Failed { message: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JobEvent {
    Status(JobStatus),
    Stdout { line: String },
    Stderr { line: String },
    Protocol { event: ProtocolEvent },
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopMode {
    /// Ask nicely (`{"cmd":"cancel"}` for protocol engines, which checkpoint
    /// and finalise the run), then kill after the timeout.
    Graceful {
        timeout: Duration,
    },
    Kill,
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("no job {0:?}")]
    UnknownJob(JobId),
    #[error("{0:?} does not accept control messages")]
    NotControllable(JobId),
    #[error("could not start {program}: {message}")]
    Spawn { program: String, message: String },
    #[error("limit reached: {0}")]
    Limit(String),
}

/// A started job: its id and its event stream.
pub struct JobHandle {
    pub id: JobId,
    pub events: tokio::sync::mpsc::Receiver<JobEvent>,
}

/// What any runtime — local processes today, perhaps a remote worker or an
/// in-process Rust trainer later — must provide.
pub trait Runtime: Send + Sync {
    fn start(&self, spec: JobSpec) -> Result<JobHandle, RuntimeError>;
    fn control(&self, id: JobId, message: Control) -> Result<(), RuntimeError>;
    fn stop(&self, id: JobId, mode: StopMode) -> Result<(), RuntimeError>;
    fn status(&self, id: JobId) -> Result<JobStatus, RuntimeError>;
    fn jobs(&self) -> Vec<(JobId, JobStatus)>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn consent() -> LaunchConsent {
        LaunchConsent::from_user_action("test", 0)
    }

    #[test]
    fn nexis_ml_jobs_request_the_protocol() {
        let spec = JobSpec {
            id: JobId(1),
            kind: JobKind::NexisMl {
                engine: "nexis-ml".into(),
                project_dir: "proj".into(),
            },
            cwd: "proj".into(),
            env: vec![],
            consent: consent(),
        };
        let plan = spec.plan();
        assert_eq!(plan.args, ["train", "proj", "--nexis-protocol"]);
        assert!(plan.speaks_protocol);
        assert!(plan.env.contains(&("NEXIS_ML_PROTOCOL".into(), "1".into())));
    }

    #[test]
    fn python_scripts_run_unbuffered_with_their_args() {
        let spec = JobSpec {
            id: JobId(2),
            kind: JobKind::PythonScript {
                interpreter: "python".into(),
                script: "train.py".into(),
                args: vec!["--epochs".into(), "3".into()],
            },
            cwd: ".".into(),
            env: vec![],
            consent: consent(),
        };
        let plan = spec.plan();
        assert_eq!(plan.args, ["-u", "train.py", "--epochs", "3"]);
        assert!(!plan.speaks_protocol);
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
}
