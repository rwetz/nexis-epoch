//! Finding what to run: Axon (the Nexis ML engine) for a `train.toml`, and
//! a Python interpreter for a script.
//!
//! Only files are looked at. Nothing is launched to ask its version, so
//! detection is safe to do while a dialog is open; the user sees the path
//! that was found in the command plan before anything runs.
//!
//! Engine names follow Nexis (`src-tauri/src/modules/ml.rs`): `axon` since
//! Axon 0.9, and `nexis-ml` for the legacy Python engine and Axon builds
//! before the rename.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// Engine binary names, preferred first.
pub const ENGINE_NAMES: [&str; 2] = ["axon", "nexis-ml"];

/// The folder (under the OS's local data dir) where Nexis keeps the Axon
/// build it downloaded: Nexis's bundle identifier, then `engine`.
pub const NEXIS_IDENTIFIER: &str = "app.nexis.nexis";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// A virtual environment in or above the ML project.
    ProjectVenv,
    Path,
    /// The engine Nexis downloaded and verified.
    NexisManaged,
    /// Epoch's settings (Python only).
    Settings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub path: PathBuf,
    pub source: Source,
    /// The pre-rename `nexis-ml` binary.
    pub legacy: bool,
}

/// Where to look for an engine.
#[derive(Debug, Clone, Default)]
pub struct EngineSearch {
    /// The folder holding `train.toml`.
    pub ml_project_dir: PathBuf,
    /// The open project's root; venvs are looked for from the ML project
    /// up to here.
    pub project_root: PathBuf,
    /// Virtual environments detection found in the project (absolute).
    pub venvs: Vec<PathBuf>,
    /// The `PATH` value to search.
    pub path_var: Option<std::ffi::OsString>,
    /// Nexis's managed engine folder, if known.
    pub managed_dir: Option<PathBuf>,
}

fn exe(stem: &str) -> String {
    if cfg!(windows) {
        format!("{stem}.exe")
    } else {
        stem.to_string()
    }
}

/// A venv's scripts folder.
fn venv_bin(venv: &Path) -> PathBuf {
    venv.join(if cfg!(windows) { "Scripts" } else { "bin" })
}

/// `.venv` and `venv` in `from` and each parent up to `root` (inclusive),
/// nearest first, then `extra`.
fn venv_dirs(from: &Path, root: &Path, extra: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut dir = Some(from);
    while let Some(d) = dir {
        for name in [".venv", "venv"] {
            out.push(d.join(name));
        }
        if d == root || !d.starts_with(root) {
            break;
        }
        dir = d.parent();
    }
    out.extend(extra.iter().cloned());
    out
}

/// Programs on PATH named `file`, in PATH order.
fn on_path(path_var: Option<&OsStr>, file: &str) -> Vec<PathBuf> {
    let Some(var) = path_var else {
        return Vec::new();
    };
    std::env::split_paths(var)
        .filter(|dir| !dir.as_os_str().is_empty())
        .map(|dir| dir.join(file))
        // Windows' Store aliases (`…\WindowsApps\python.exe`) open the
        // Microsoft Store instead of running anything.
        .filter(|p| !p.components().any(|c| c.as_os_str() == "WindowsApps"))
        .filter(|p| p.is_file())
        .collect()
}

fn push_unique(out: &mut Vec<Found>, seen: &mut HashSet<PathBuf>, found: Found) {
    let key = dunce::canonicalize(&found.path).unwrap_or_else(|_| found.path.clone());
    if seen.insert(key) {
        out.push(found);
    }
}

/// Every engine binary that exists, best first: project venvs (nearest
/// first), then `PATH`, then Nexis's managed folder; `axon` before
/// `nexis-ml` within each.
pub fn find_engines(search: &EngineSearch) -> Vec<Found> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for venv in venv_dirs(&search.ml_project_dir, &search.project_root, &search.venvs) {
        for (i, name) in ENGINE_NAMES.iter().enumerate() {
            let path = venv_bin(&venv).join(exe(name));
            if path.is_file() {
                let found = Found {
                    path,
                    source: Source::ProjectVenv,
                    legacy: i > 0,
                };
                push_unique(&mut out, &mut seen, found);
            }
        }
    }
    for (i, name) in ENGINE_NAMES.iter().enumerate() {
        for path in on_path(search.path_var.as_deref(), &exe(name)) {
            let found = Found {
                path,
                source: Source::Path,
                legacy: i > 0,
            };
            push_unique(&mut out, &mut seen, found);
        }
    }
    if let Some(dir) = &search.managed_dir {
        for (i, name) in ENGINE_NAMES.iter().enumerate() {
            let path = dir.join(exe(name));
            if path.is_file() {
                let found = Found {
                    path,
                    source: Source::NexisManaged,
                    legacy: i > 0,
                };
                push_unique(&mut out, &mut seen, found);
            }
        }
    }
    out
}

/// The interpreter for a script: the configured one if set (it must
/// exist), else a venv in or above the script's folder or one detection
/// found, else `python3`/`python` on `PATH`.
pub fn find_python(
    configured: Option<&Path>,
    script_dir: &Path,
    project_root: &Path,
    venvs: &[PathBuf],
    path_var: Option<&OsStr>,
) -> Result<Found, String> {
    if let Some(path) = configured.filter(|p| !p.as_os_str().is_empty()) {
        return if path.is_file() {
            Ok(Found {
                path: path.to_path_buf(),
                source: Source::Settings,
                legacy: false,
            })
        } else {
            Err(format!(
                "the interpreter in Settings does not exist: {}",
                path.display()
            ))
        };
    }
    let names: &[&str] = if cfg!(windows) {
        &["python.exe"]
    } else {
        &["python3", "python"]
    };
    for venv in venv_dirs(script_dir, project_root, venvs) {
        for name in names {
            let path = venv_bin(&venv).join(name);
            if path.is_file() {
                return Ok(Found {
                    path,
                    source: Source::ProjectVenv,
                    legacy: false,
                });
            }
        }
    }
    for name in names {
        if let Some(path) = on_path(path_var, name).into_iter().next() {
            return Ok(Found {
                path,
                source: Source::Path,
                legacy: false,
            });
        }
    }
    Err("no Python interpreter found: create a virtual environment in the project, put Python on PATH, or set one in Settings".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"").unwrap();
    }

    fn bin(venv: &Path, stem: &str) -> PathBuf {
        venv_bin(venv).join(exe(stem))
    }

    #[test]
    fn prefers_the_nearest_venv_then_path_then_nexis() {
        let tmp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(tmp.path()).unwrap();
        let ml = root.join("models").join("tabular");
        std::fs::create_dir_all(&ml).unwrap();
        let near = bin(&ml.join(".venv"), "nexis-ml");
        let far = bin(&root.join("venv"), "axon");
        let on_path_dir = root.join("tools");
        let path_axon = on_path_dir.join(exe("axon"));
        let managed = root.join("managed");
        let managed_axon = managed.join(exe("axon"));
        for p in [&near, &far, &path_axon, &managed_axon] {
            touch(p);
        }
        let found = find_engines(&EngineSearch {
            ml_project_dir: ml.clone(),
            project_root: root.clone(),
            venvs: vec![root.join("venv")], // also found by the walk: listed once
            path_var: Some(std::env::join_paths([&on_path_dir]).unwrap()),
            managed_dir: Some(managed.clone()),
        });
        let got: Vec<_> = found
            .iter()
            .map(|f| (&f.path, f.source, f.legacy))
            .collect();
        assert_eq!(
            got,
            [
                (&near, Source::ProjectVenv, true),
                (&far, Source::ProjectVenv, false),
                (&path_axon, Source::Path, false),
                (&managed_axon, Source::NexisManaged, false),
            ]
        );
    }

    #[test]
    fn does_not_walk_above_the_project() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = dunce::canonicalize(tmp.path()).unwrap();
        let root = outside.join("project");
        std::fs::create_dir_all(&root).unwrap();
        touch(&bin(&outside.join(".venv"), "axon"));
        let found = find_engines(&EngineSearch {
            ml_project_dir: root.clone(),
            project_root: root,
            ..Default::default()
        });
        assert!(found.is_empty());
    }

    #[test]
    fn python_comes_from_settings_then_venvs_then_path() {
        let tmp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(tmp.path()).unwrap();
        let missing = root.join("nope").join("python");
        assert!(find_python(Some(&missing), &root, &root, &[], None).is_err());

        let name = if cfg!(windows) {
            "python.exe"
        } else {
            "python3"
        };
        let venv_python = venv_bin(&root.join(".venv")).join(name);
        touch(&venv_python);
        let found = find_python(None, &root.join("scripts"), &root, &[], None).unwrap();
        assert_eq!(found.path, venv_python);
        assert_eq!(found.source, Source::ProjectVenv);

        let configured = root.join("custom").join(name);
        touch(&configured);
        let found = find_python(Some(&configured), &root, &root, &[], None).unwrap();
        assert_eq!(found.source, Source::Settings);

        let empty = tempfile::tempdir().unwrap();
        let path_dir = empty.path().join("bin");
        touch(&path_dir.join(name));
        let var = std::env::join_paths([&path_dir]).unwrap();
        let found = find_python(None, empty.path(), empty.path(), &[], Some(&var)).unwrap();
        assert_eq!(found.source, Source::Path);
    }
}
