//! Project-scoped paths.
//!
//! Every file the frontend asks for is named relative to an opened project
//! root, and resolved here. A relative path may not be absolute, may not use
//! `..`, and must still be inside the root after symlinks are followed — so
//! a webview that is compromised or simply buggy can't read or write outside
//! the folder the user opened.

use std::path::{Component, Path, PathBuf};

use crate::error::{CoreError, Result};

/// A canonical, existing directory that file access is confined to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRoot {
    root: PathBuf,
}

impl ProjectRoot {
    /// Open an existing directory as a project root.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let root = dunce::canonicalize(path).map_err(|e| CoreError::io(path, e))?;
        let meta = std::fs::metadata(&root).map_err(|e| CoreError::io(&root, e))?;
        if !meta.is_dir() {
            return Err(CoreError::Invalid(format!(
                "{}: not a directory",
                root.display()
            )));
        }
        Ok(Self { root })
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    /// The directory's own name, for display.
    pub fn name(&self) -> String {
        self.root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.root.display().to_string())
    }

    /// Resolve a `/`-separated path relative to the root. The empty string
    /// and `"."` are the root itself. The target need not exist yet.
    pub fn resolve(&self, rel: &str) -> Result<PathBuf> {
        let rel = rel.trim();
        if rel.contains('\0') || rel.starts_with(['/', '\\']) {
            return Err(CoreError::InvalidPath(rel.into()));
        }
        // Alternate data streams (`file:stream`) and drive prefixes.
        if cfg!(windows) && rel.contains(':') {
            return Err(CoreError::InvalidPath(rel.into()));
        }
        let mut joined = self.root.clone();
        for part in rel.split(['/', '\\']) {
            if part.is_empty() || part == "." {
                continue;
            }
            let mut components = Path::new(part).components();
            match (components.next(), components.next()) {
                (Some(Component::Normal(name)), None) => joined.push(name),
                _ => return Err(CoreError::InvalidPath(rel.into())),
            }
        }
        self.confine(joined)
    }

    /// Accept an absolute path only if it lies inside the root.
    pub fn contain(&self, abs: &Path) -> Result<PathBuf> {
        if !abs.is_absolute() {
            return Err(CoreError::InvalidPath(abs.display().to_string()));
        }
        self.confine(abs.to_path_buf())
    }

    /// The `/`-separated path of `abs` relative to the root, or `None` when
    /// `abs` is outside it.
    pub fn relative(&self, abs: &Path) -> Option<String> {
        let rest = abs.strip_prefix(&self.root).ok()?;
        let parts: Vec<_> = rest
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect();
        Some(parts.join("/"))
    }

    /// Follow symlinks on the longest existing prefix of `path` and check the
    /// result is still under the root.
    fn confine(&self, path: PathBuf) -> Result<PathBuf> {
        let mut existing = path.as_path();
        let mut tail = Vec::new();
        loop {
            match dunce::canonicalize(existing) {
                Ok(real) => {
                    if !real.starts_with(&self.root) {
                        return Err(CoreError::OutsideRoot { path });
                    }
                    let mut out = real;
                    out.extend(tail.iter().rev());
                    return Ok(out);
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    let (Some(parent), Some(name)) = (existing.parent(), existing.file_name())
                    else {
                        return Err(CoreError::OutsideRoot { path });
                    };
                    tail.push(name.to_owned());
                    existing = parent;
                }
                Err(e) => return Err(CoreError::io(existing, e)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> (tempfile::TempDir, ProjectRoot) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data/raw")).unwrap();
        std::fs::write(dir.path().join("data/raw/a.csv"), "x\n1\n").unwrap();
        let root = ProjectRoot::open(dir.path()).unwrap();
        (dir, root)
    }

    #[test]
    fn resolves_inside_root() {
        let (_d, root) = root();
        let p = root.resolve("data/raw/a.csv").unwrap();
        assert!(p.ends_with("a.csv"));
        assert_eq!(root.relative(&p).as_deref(), Some("data/raw/a.csv"));
        assert_eq!(root.resolve("").unwrap(), root.path());
        assert_eq!(root.resolve("./data/").unwrap(), root.path().join("data"));
    }

    #[test]
    fn resolves_paths_that_do_not_exist_yet() {
        let (_d, root) = root();
        let p = root.resolve("data/new/file.toml").unwrap();
        assert_eq!(p, root.path().join("data").join("new").join("file.toml"));
    }

    #[test]
    fn rejects_traversal_and_absolute_paths() {
        let (_d, root) = root();
        for bad in ["../secret", "data/../../x", "/etc/passwd", "data/\0x"] {
            assert!(root.resolve(bad).is_err(), "{bad} should be rejected");
        }
        if cfg!(windows) {
            assert!(root.resolve("C:\\Windows").is_err());
            assert!(root.resolve("a.csv:stream").is_err());
        }
    }

    #[test]
    fn contain_rejects_outside_absolute_paths() {
        let (_d, root) = root();
        let other = tempfile::tempdir().unwrap();
        assert!(matches!(
            root.contain(other.path()),
            Err(CoreError::OutsideRoot { .. })
        ));
        assert!(root.contain(&root.path().join("data")).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape() {
        let (_d, root) = root();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("link")).unwrap();
        assert!(matches!(
            root.resolve("link/x"),
            Err(CoreError::OutsideRoot { .. })
        ));
    }
}
