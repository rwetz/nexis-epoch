//! Recognising what an existing repository contains, without asking it to
//! change shape and without running any of its code.
//!
//! Detection only reads files: interpreter versions come from `pyvenv.cfg`,
//! never from launching the interpreter.

use std::path::Path;

use serde::Serialize;

use crate::fs::{walk, FileKind};
use crate::nexis_ml;
use crate::paths::ProjectRoot;

/// How far detection looks. A project bigger than this is still opened;
/// the summary just says it was cut short.
pub const DETECT_MAX_DEPTH: usize = 4;
pub const DETECT_MAX_ENTRIES: usize = 20_000;
const MAX_LISTED: usize = 200;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub root: String,
    pub name: String,
    /// `[project] name` (or `[tool.poetry] name`) from pyproject.toml.
    pub package_name: Option<String>,
    pub has_pyproject: bool,
    pub requirements: Vec<String>,
    pub train_configs: Vec<String>,
    pub toml_files: usize,
    pub python_files: usize,
    pub notebooks: usize,
    pub environments: Vec<PythonEnv>,
    pub datasets: Vec<DatasetRef>,
    pub checkpoint_dirs: Vec<String>,
    pub experiment_stores: Vec<ExperimentStoreRef>,
    /// Detection hit its depth or entry limit.
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PythonEnv {
    /// Relative to the project root.
    pub path: String,
    /// Absolute path to the interpreter, if it exists.
    pub interpreter: Option<String>,
    /// From `pyvenv.cfg`.
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetRef {
    pub path: String,
    pub kind: FileKind,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentStoreRef {
    /// The ML project directory that owns the store, relative to the root.
    pub project_dir: String,
    /// The runs directory, relative to the root.
    pub runs_dir: String,
    pub format: &'static str,
    pub run_count: usize,
}

/// Inspect a project. Cheap enough to run on open: bounded by
/// [`DETECT_MAX_DEPTH`] and [`DETECT_MAX_ENTRIES`].
pub fn detect(root: &ProjectRoot) -> ProjectSummary {
    let base = root.path();
    let pyproject = base.join("pyproject.toml");
    let mut summary = ProjectSummary {
        root: base.display().to_string(),
        name: root.name(),
        package_name: read_package_name(&pyproject),
        has_pyproject: pyproject.is_file(),
        requirements: Vec::new(),
        train_configs: Vec::new(),
        toml_files: 0,
        python_files: 0,
        notebooks: 0,
        environments: Vec::new(),
        datasets: Vec::new(),
        checkpoint_dirs: Vec::new(),
        experiment_stores: Vec::new(),
        truncated: false,
    };
    let rel = |p: &Path| root.relative(p).unwrap_or_default();

    check_experiment_store(base, &rel, &mut summary);
    summary.truncated = walk(
        base,
        DETECT_MAX_DEPTH,
        DETECT_MAX_ENTRIES,
        |path, meta, _| {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default();
            if meta.is_dir() {
                if crate::fs::is_venv(path) {
                    summary.environments.push(python_env(path, rel(path)));
                } else {
                    check_experiment_store(path, &rel, &mut summary);
                    let lower = name.to_ascii_lowercase();
                    if (lower == "checkpoints" || lower == "ckpt" || lower == "weights")
                        && summary.checkpoint_dirs.len() < MAX_LISTED
                    {
                        summary.checkpoint_dirs.push(rel(path));
                    }
                }
                return;
            }
            let kind = FileKind::from_path(path);
            match kind {
                FileKind::Python => summary.python_files += 1,
                FileKind::Notebook => summary.notebooks += 1,
                FileKind::Toml => {
                    summary.toml_files += 1;
                    if name == "train.toml" && summary.train_configs.len() < MAX_LISTED {
                        summary.train_configs.push(rel(path));
                    }
                }
                k if k.is_tabular() && summary.datasets.len() < MAX_LISTED => {
                    summary.datasets.push(DatasetRef {
                        path: rel(path),
                        kind,
                        size: meta.len(),
                    });
                }
                _ => {}
            }
            if name.starts_with("requirements") && name.ends_with(".txt") {
                summary.requirements.push(rel(path));
            }
        },
    );
    summary.train_configs.sort();
    summary.requirements.sort();
    summary.datasets.sort_by(|a, b| a.path.cmp(&b.path));
    summary.environments.sort_by(|a, b| a.path.cmp(&b.path));
    summary
}

fn check_experiment_store(
    dir: &Path,
    rel: &impl Fn(&Path) -> String,
    summary: &mut ProjectSummary,
) {
    let runs = nexis_ml::runs_dir(dir);
    if runs.is_dir() {
        let run_count = std::fs::read_dir(&runs)
            .map(|r| r.flatten().filter(|e| e.path().is_dir()).count())
            .unwrap_or(0);
        summary.experiment_stores.push(ExperimentStoreRef {
            project_dir: rel(dir),
            runs_dir: rel(&runs),
            format: nexis_ml::FORMAT,
            run_count,
        });
    }
}

fn python_env(dir: &Path, rel: String) -> PythonEnv {
    let candidates: &[&str] = if cfg!(windows) {
        &["Scripts/python.exe", "python.exe"]
    } else {
        &["bin/python3", "bin/python"]
    };
    let interpreter = candidates
        .iter()
        .map(|c| dir.join(c))
        .find(|p| p.is_file())
        .map(|p| p.display().to_string());
    let version = std::fs::read_to_string(dir.join("pyvenv.cfg"))
        .ok()
        .and_then(|cfg| {
            cfg.lines().find_map(|line| {
                let (key, value) = line.split_once('=')?;
                matches!(key.trim(), "version" | "version_info").then(|| value.trim().to_string())
            })
        });
    PythonEnv {
        path: rel,
        interpreter,
        version,
    }
}

fn read_package_name(pyproject: &Path) -> Option<String> {
    let text = std::fs::read_to_string(pyproject).ok()?;
    let doc: toml_edit::DocumentMut = text.parse().ok()?;
    let from = |table: &str, sub: Option<&str>| {
        let mut item = doc.get(table)?;
        if let Some(sub) = sub {
            item = item.get(sub)?;
        }
        item.get("name")?.as_str().map(str::to_string)
    };
    from("project", None).or_else(|| from("tool", Some("poetry")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_a_typical_ml_repository() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        std::fs::write(
            p.join("pyproject.toml"),
            "[project]\nname = \"goat-ranker\"\n",
        )
        .unwrap();
        std::fs::write(p.join("requirements.txt"), "torch\n").unwrap();
        std::fs::write(p.join("train.py"), "print('hi')\n").unwrap();
        std::fs::create_dir_all(p.join("data")).unwrap();
        std::fs::write(p.join("data/players.csv"), "a,b\n1,2\n").unwrap();
        std::fs::create_dir_all(p.join("models/mlp/.nexis-ml/runs/2026-01-01-0000-tabular"))
            .unwrap();
        std::fs::write(p.join("models/mlp/train.toml"), "[train]\n").unwrap();
        std::fs::create_dir_all(p.join("checkpoints")).unwrap();
        std::fs::create_dir_all(p.join(".venv/Scripts")).unwrap();
        std::fs::write(p.join(".venv/pyvenv.cfg"), "home = x\nversion = 3.12.4\n").unwrap();

        let summary = detect(&ProjectRoot::open(p).unwrap());
        assert_eq!(summary.package_name.as_deref(), Some("goat-ranker"));
        assert!(summary.has_pyproject);
        assert_eq!(summary.requirements, ["requirements.txt"]);
        assert_eq!(summary.train_configs, ["models/mlp/train.toml"]);
        assert_eq!(summary.python_files, 1);
        assert_eq!(summary.datasets.len(), 1);
        assert_eq!(summary.datasets[0].path, "data/players.csv");
        assert_eq!(summary.checkpoint_dirs, ["checkpoints"]);
        assert_eq!(summary.environments.len(), 1);
        assert_eq!(summary.environments[0].version.as_deref(), Some("3.12.4"));
        assert_eq!(summary.experiment_stores.len(), 1);
        assert_eq!(summary.experiment_stores[0].project_dir, "models/mlp");
        assert_eq!(summary.experiment_stores[0].run_count, 1);
        assert!(!summary.truncated);
    }
}
