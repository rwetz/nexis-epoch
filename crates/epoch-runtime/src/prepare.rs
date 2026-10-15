//! From "run this file" to the exact job: which engine or interpreter, which
//! folder, which arguments. The result is shown to the user as a
//! [`CommandPlan`] before anything starts; starting needs a
//! [`LaunchConsent`] on top.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::engine::{find_engines, find_python, EngineSearch, Found};
use crate::{plan_for, CommandPlan, JobKind, JobSpec, LaunchConsent};

/// What the user asked to run. Paths are absolute and already confined to
/// the project by the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// Train the ML project whose `train.toml` this is, with Axon.
    Axon { config: PathBuf },
    /// Run a Python script with these arguments.
    PythonScript { script: PathBuf, args: Vec<String> },
}

/// What the app knows about where things are.
#[derive(Debug, Clone, Default)]
pub struct Context {
    pub project_root: PathBuf,
    /// Virtual environments detection found (absolute).
    pub venvs: Vec<PathBuf>,
    pub path_var: Option<OsString>,
    /// Nexis's managed engine folder.
    pub managed_dir: Option<PathBuf>,
    /// The interpreter from Settings, if one is set.
    pub configured_python: Option<PathBuf>,
}

/// A job ready to confirm.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prepared {
    pub title: String,
    #[serde(skip)]
    pub kind: JobKind,
    #[serde(skip)]
    pub cwd: PathBuf,
    #[serde(skip)]
    pub env: Vec<(String, String)>,
    /// The engine or interpreter that will run, and where it was found.
    pub program: Found,
    /// Other engines that were found, for the user's information.
    pub alternatives: Vec<Found>,
}

impl Prepared {
    pub fn plan(&self) -> CommandPlan {
        plan_for(&self.kind, &self.cwd, &self.env)
    }

    pub fn into_spec(self, scope: String, consent: LaunchConsent) -> JobSpec {
        JobSpec {
            title: self.title,
            scope,
            kind: self.kind,
            cwd: self.cwd,
            env: self.env,
            consent,
        }
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub fn prepare(target: Target, ctx: &Context) -> Result<Prepared, String> {
    match target {
        Target::Axon { config } => {
            if file_name(&config) != "train.toml" {
                return Err(format!(
                    "{} is not a train.toml; Axon trains from a project's train.toml",
                    config.display()
                ));
            }
            if !config.is_file() {
                return Err(format!("{} does not exist", config.display()));
            }
            let ml_dir = config
                .parent()
                .ok_or("train.toml has no folder")?
                .to_path_buf();
            let mut found = find_engines(&EngineSearch {
                ml_project_dir: ml_dir.clone(),
                project_root: ctx.project_root.clone(),
                venvs: ctx.venvs.clone(),
                path_var: ctx.path_var.clone(),
                managed_dir: ctx.managed_dir.clone(),
            });
            if found.is_empty() {
                return Err(
                    "Axon was not found. Install it from Synapse in Nexis, put `axon` on PATH, \
                     or install it into the project's virtual environment."
                        .into(),
                );
            }
            let program = found.remove(0);
            let name = if ml_dir == ctx.project_root {
                file_name(&ctx.project_root)
            } else {
                file_name(&ml_dir)
            };
            Ok(Prepared {
                title: format!("Train {name}"),
                kind: JobKind::Axon {
                    engine: program.path.clone(),
                },
                cwd: ml_dir,
                env: Vec::new(),
                program,
                alternatives: found,
            })
        }
        Target::PythonScript { script, args } => {
            if script.extension().is_none_or(|e| e != "py") {
                return Err(format!("{} is not a Python script", script.display()));
            }
            if !script.is_file() {
                return Err(format!("{} does not exist", script.display()));
            }
            let script_dir = script.parent().unwrap_or(&ctx.project_root);
            let program = find_python(
                ctx.configured_python.as_deref(),
                script_dir,
                &ctx.project_root,
                &ctx.venvs,
                ctx.path_var.as_deref(),
            )?;
            // Run from the project root, naming the script relative to it,
            // as a person would type it there.
            let relative = script
                .strip_prefix(&ctx.project_root)
                .map(Path::to_path_buf)
                .unwrap_or_else(|_| script.clone());
            let shown = relative.to_string_lossy().replace('\\', "/");
            Ok(Prepared {
                title: format!("Run {shown}"),
                kind: JobKind::PythonScript {
                    interpreter: program.path.clone(),
                    script: relative,
                    args,
                },
                cwd: ctx.project_root.clone(),
                env: Vec::new(),
                program,
                alternatives: Vec::new(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Source;

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"").unwrap();
    }

    fn bin(dir: &Path, stem: &str) -> PathBuf {
        let sub = if cfg!(windows) { "Scripts" } else { "bin" };
        let name = if cfg!(windows) {
            format!("{stem}.exe")
        } else {
            stem.to_string()
        };
        dir.join(sub).join(name)
    }

    #[test]
    fn a_train_toml_plans_axon_in_its_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(tmp.path()).unwrap();
        let config = root.join("models").join("tabular").join("train.toml");
        touch(&config);
        let ctx = Context {
            project_root: root.clone(),
            ..Default::default()
        };
        let err = prepare(
            Target::Axon {
                config: config.clone(),
            },
            &ctx,
        )
        .unwrap_err();
        assert!(err.contains("Axon was not found"), "{err}");

        let axon = bin(&root.join(".venv"), "axon");
        touch(&axon);
        let prepared = prepare(Target::Axon { config }, &ctx).unwrap();
        assert_eq!(prepared.title, "Train tabular");
        assert_eq!(prepared.program.source, Source::ProjectVenv);
        let plan = prepared.plan();
        assert_eq!(plan.program, axon);
        assert_eq!(plan.args, ["--nexis-protocol", "train", "."]);
        assert_eq!(plan.cwd, root.join("models").join("tabular"));
        assert!(plan.speaks_protocol);
    }

    #[test]
    fn only_train_toml_and_python_files_are_accepted() {
        let tmp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(tmp.path()).unwrap();
        let other = root.join("pyproject.toml");
        let text = root.join("notes.txt");
        touch(&other);
        touch(&text);
        let ctx = Context {
            project_root: root.clone(),
            ..Default::default()
        };
        assert!(prepare(Target::Axon { config: other }, &ctx).is_err());
        let not_python = Target::PythonScript {
            script: text,
            args: vec![],
        };
        assert!(prepare(not_python, &ctx).is_err());
        let missing = Target::PythonScript {
            script: root.join("gone.py"),
            args: vec![],
        };
        assert!(prepare(missing, &ctx).is_err());
    }

    #[test]
    fn a_script_runs_from_the_project_root_with_the_venv_python() {
        let tmp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(tmp.path()).unwrap();
        let script = root.join("scripts").join("eval.py");
        touch(&script);
        let python = bin(
            &root.join("venv"),
            if cfg!(windows) { "python" } else { "python3" },
        );
        touch(&python);
        let ctx = Context {
            project_root: root.clone(),
            ..Default::default()
        };
        let target = Target::PythonScript {
            script,
            args: vec!["--split".into(), "test".into()],
        };
        let prepared = prepare(target, &ctx).unwrap();
        assert_eq!(prepared.title, "Run scripts/eval.py");
        let plan = prepared.plan();
        assert_eq!(plan.program, python);
        assert_eq!(plan.cwd, root);
        let script_arg = Path::new("scripts").join("eval.py").display().to_string();
        assert_eq!(plan.args, ["-u", script_arg.as_str(), "--split", "test"]);
        assert!(!plan.speaks_protocol);
    }
}
