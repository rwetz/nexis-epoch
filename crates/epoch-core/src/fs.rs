//! Filesystem access for the project explorer and editors: bounded listings,
//! text reads that refuse binaries, and atomic writes that refuse to clobber
//! a file that changed on disk since it was read.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;

use crate::error::{CoreError, Result};
use crate::paths::ProjectRoot;

/// What a file is, as far as Epoch's panels care.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Directory,
    Toml,
    Python,
    Notebook,
    Csv,
    Parquet,
    Json,
    Jsonl,
    Sqlite,
    Checkpoint,
    Image,
    Markdown,
    Text,
    Other,
}

impl FileKind {
    pub fn from_path(path: &Path) -> Self {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        match ext.as_str() {
            "toml" => Self::Toml,
            "py" | "pyi" => Self::Python,
            "ipynb" => Self::Notebook,
            "csv" | "tsv" => Self::Csv,
            "parquet" | "pq" => Self::Parquet,
            "json" => Self::Json,
            "jsonl" | "ndjson" => Self::Jsonl,
            "sqlite" | "sqlite3" | "db" | "duckdb" => Self::Sqlite,
            "pt" | "pth" | "ckpt" | "safetensors" | "onnx" | "bin" | "h5" | "gguf" => {
                Self::Checkpoint
            }
            "png" | "jpg" | "jpeg" | "bmp" | "gif" | "webp" | "tif" | "tiff" => Self::Image,
            "md" | "markdown" => Self::Markdown,
            "txt" | "cfg" | "ini" | "yaml" | "yml" | "log" | "sh" | "ps1" | "rs" | "ts" | "js" => {
                Self::Text
            }
            _ => Self::Other,
        }
    }

    /// Whether the dataset explorer can open it.
    pub fn is_tabular(self) -> bool {
        matches!(self, Self::Csv | Self::Parquet | Self::Json | Self::Jsonl)
    }
}

/// Directories that are never interesting to browse into automatically.
pub const SKIPPED_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "__pycache__",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".tox",
    ".ipynb_checkpoints",
    "site-packages",
    "target",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    pub name: String,
    /// `/`-separated, relative to the project root.
    pub path: String,
    pub kind: FileKind,
    pub size: u64,
    pub modified_ms: Option<u64>,
    pub hidden: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirListing {
    pub path: String,
    pub entries: Vec<DirEntry>,
    /// More entries existed than `limit`.
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct ListOptions {
    pub show_hidden: bool,
    pub limit: usize,
}

impl Default for ListOptions {
    fn default() -> Self {
        Self {
            show_hidden: false,
            limit: 5_000,
        }
    }
}

fn modified_ms(meta: &std::fs::Metadata) -> Option<u64> {
    let t = meta.modified().ok()?;
    Some(t.duration_since(UNIX_EPOCH).ok()?.as_millis() as u64)
}

fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

/// List one directory: folders first, then files, each alphabetically.
pub fn list_dir(root: &ProjectRoot, rel: &str, opts: ListOptions) -> Result<DirListing> {
    let dir = root.resolve(rel)?;
    let read = std::fs::read_dir(&dir).map_err(|e| CoreError::io(&dir, e))?;
    let mut entries = Vec::new();
    let mut truncated = false;
    for item in read {
        let item = item.map_err(|e| CoreError::io(&dir, e))?;
        let name = item.file_name().to_string_lossy().into_owned();
        let hidden = is_hidden(&name);
        if hidden && !opts.show_hidden {
            continue;
        }
        if entries.len() >= opts.limit {
            truncated = true;
            break;
        }
        // Metadata follows symlinks; a dangling link is skipped rather than
        // failing the whole listing.
        let Ok(meta) = std::fs::metadata(item.path()) else {
            continue;
        };
        let path = root.relative(&item.path()).unwrap_or_else(|| name.clone());
        let kind = if meta.is_dir() {
            FileKind::Directory
        } else {
            FileKind::from_path(Path::new(&name))
        };
        entries.push(DirEntry {
            name,
            path,
            kind,
            size: if meta.is_dir() { 0 } else { meta.len() },
            modified_ms: modified_ms(&meta),
            hidden,
        });
    }
    entries.sort_by(|a, b| {
        let dir_a = a.kind == FileKind::Directory;
        let dir_b = b.kind == FileKind::Directory;
        dir_b
            .cmp(&dir_a)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(DirListing {
        path: root.relative(&dir).unwrap_or_default(),
        entries,
        truncated,
    })
}

/// A text file plus what is needed to save it back safely.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextFile {
    pub path: String,
    pub content: String,
    /// Hash of the bytes on disk; pass it back to [`write_text`] to detect
    /// edits made by another program in the meantime.
    pub hash: String,
    pub size: u64,
    pub modified_ms: Option<u64>,
}

/// FNV-1a 64 over the bytes, as hex. A change detector, not a security hash.
pub fn content_hash(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// Read a UTF-8 text file of at most `max_bytes`.
pub fn read_text(root: &ProjectRoot, rel: &str, max_bytes: u64) -> Result<TextFile> {
    let path = root.resolve(rel)?;
    let meta = std::fs::metadata(&path).map_err(|e| CoreError::io(&path, e))?;
    if meta.is_dir() {
        return Err(CoreError::Invalid(format!(
            "{}: is a directory",
            path.display()
        )));
    }
    if meta.len() > max_bytes {
        return Err(CoreError::TooLarge {
            path,
            size: meta.len(),
            limit: max_bytes,
        });
    }
    let bytes = std::fs::read(&path).map_err(|e| CoreError::io(&path, e))?;
    let sniff = &bytes[..bytes.len().min(8192)];
    if sniff.contains(&0) {
        return Err(CoreError::Binary { path });
    }
    let hash = content_hash(&bytes);
    let content = String::from_utf8(bytes).map_err(|_| CoreError::Binary { path: path.clone() })?;
    Ok(TextFile {
        path: root.relative(&path).unwrap_or_else(|| rel.to_string()),
        content,
        hash,
        size: meta.len(),
        modified_ms: modified_ms(&meta),
    })
}

/// Save text atomically. With `expected_hash`, refuse when the file on disk
/// no longer matches what the caller read (or has been deleted).
pub fn write_text(
    root: &ProjectRoot,
    rel: &str,
    content: &str,
    expected_hash: Option<&str>,
) -> Result<TextFile> {
    let path = root.resolve(rel)?;
    if let Some(expected) = expected_hash {
        match std::fs::read(&path) {
            Ok(current) if content_hash(&current) == expected => {}
            Ok(_) => return Err(CoreError::Conflict { path }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(CoreError::Conflict { path })
            }
            Err(e) => return Err(CoreError::io(&path, e)),
        }
    }
    atomic_write(&path, content.as_bytes())?;
    let meta = std::fs::metadata(&path).map_err(|e| CoreError::io(&path, e))?;
    Ok(TextFile {
        path: root.relative(&path).unwrap_or_else(|| rel.to_string()),
        content: content.to_string(),
        hash: content_hash(content.as_bytes()),
        size: meta.len(),
        modified_ms: modified_ms(&meta),
    })
}

/// Write `bytes` to `path` via a temporary sibling and a rename, so readers
/// see either the old file or the new one, never half of either.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| CoreError::InvalidPath(path.display().to_string()))?;
    std::fs::create_dir_all(parent).map_err(|e| CoreError::io(parent, e))?;
    let name = path
        .file_name()
        .ok_or_else(|| CoreError::InvalidPath(path.display().to_string()))?
        .to_string_lossy();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let tmp: PathBuf = parent.join(format!(".{name}.{}-{nanos}.tmp", std::process::id()));
    let result = (|| {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        // std's rename replaces an existing target on every platform.
        std::fs::rename(&tmp, path)
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(CoreError::io(path, e));
    }
    Ok(())
}

/// Visit files under `start` breadth-first, skipping [`SKIPPED_DIRS`],
/// hidden directories and Python virtual environments, up to `max_depth`
/// levels and `max_entries` entries. Returns whether the walk was cut short.
pub fn walk(
    start: &Path,
    max_depth: usize,
    max_entries: usize,
    mut visit: impl FnMut(&Path, &std::fs::Metadata, usize),
) -> bool {
    let mut queue = std::collections::VecDeque::from([(start.to_path_buf(), 0usize)]);
    let mut seen = 0usize;
    while let Some((dir, depth)) = queue.pop_front() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for item in read.flatten() {
            seen += 1;
            if seen > max_entries {
                return true;
            }
            let path = item.path();
            // Don't follow symlinks while walking: a link to `/` would make
            // the walk unbounded in practice.
            let Ok(file_type) = item.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            let Ok(meta) = item.metadata() else { continue };
            visit(&path, &meta, depth);
            if meta.is_dir() && depth < max_depth && should_descend(&path) {
                queue.push_back((path, depth + 1));
            }
        }
    }
    false
}

fn should_descend(dir: &Path) -> bool {
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    !(is_hidden(&name) || SKIPPED_DIRS.contains(&name.as_ref()) || is_venv(dir))
}

/// A Python virtual environment (it has a `pyvenv.cfg`).
pub fn is_venv(dir: &Path) -> bool {
    dir.join("pyvenv.cfg").is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> (tempfile::TempDir, ProjectRoot) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("data")).unwrap();
        std::fs::write(dir.path().join("train.toml"), "[train]\nepochs = 3\n").unwrap();
        std::fs::write(dir.path().join("data/a.csv"), "x\n1\n").unwrap();
        std::fs::write(dir.path().join(".env"), "SECRET=1").unwrap();
        std::fs::write(dir.path().join("blob.bin"), [0u8, 1, 2]).unwrap();
        let root = ProjectRoot::open(dir.path()).unwrap();
        (dir, root)
    }

    #[test]
    fn lists_dirs_first_and_hides_dotfiles() {
        let (_d, root) = project();
        let listing = list_dir(&root, "", ListOptions::default()).unwrap();
        let names: Vec<_> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["data", "blob.bin", "train.toml"]);
        assert_eq!(listing.entries[2].kind, FileKind::Toml);
        let all = list_dir(
            &root,
            "",
            ListOptions {
                show_hidden: true,
                limit: 10,
            },
        )
        .unwrap();
        assert!(all.entries.iter().any(|e| e.name == ".env" && e.hidden));
    }

    #[test]
    fn listing_respects_limit() {
        let (_d, root) = project();
        let listing = list_dir(
            &root,
            "",
            ListOptions {
                show_hidden: false,
                limit: 1,
            },
        )
        .unwrap();
        assert_eq!(listing.entries.len(), 1);
        assert!(listing.truncated);
    }

    #[test]
    fn reads_text_and_refuses_binaries_and_large_files() {
        let (_d, root) = project();
        let file = read_text(&root, "train.toml", 1024).unwrap();
        assert!(file.content.contains("epochs"));
        assert!(matches!(
            read_text(&root, "blob.bin", 1024),
            Err(CoreError::Binary { .. })
        ));
        assert!(matches!(
            read_text(&root, "train.toml", 4),
            Err(CoreError::TooLarge { .. })
        ));
    }

    #[test]
    fn write_detects_external_changes() {
        let (_d, root) = project();
        let file = read_text(&root, "train.toml", 1024).unwrap();
        let saved = write_text(
            &root,
            "train.toml",
            "[train]\nepochs = 4\n",
            Some(&file.hash),
        )
        .unwrap();
        // The first hash is now stale.
        let stale = write_text(&root, "train.toml", "x", Some(&file.hash));
        assert!(matches!(stale, Err(CoreError::Conflict { .. })));
        write_text(
            &root,
            "train.toml",
            "[train]\nepochs = 5\n",
            Some(&saved.hash),
        )
        .unwrap();
        assert!(read_text(&root, "train.toml", 1024)
            .unwrap()
            .content
            .contains('5'));
    }

    #[test]
    fn atomic_write_leaves_no_temp_files() {
        let (dir, _root) = project();
        atomic_write(&dir.path().join("data/out.json"), b"{}").unwrap();
        let names: Vec<_> = std::fs::read_dir(dir.path().join("data"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().all(|n| !n.ends_with(".tmp")), "{names:?}");
    }

    #[test]
    fn walk_skips_venvs_and_stops_at_limits() {
        let (dir, _root) = project();
        std::fs::create_dir_all(dir.path().join(".venv/lib")).unwrap();
        std::fs::write(dir.path().join(".venv/pyvenv.cfg"), "version = 3.12.1").unwrap();
        std::fs::create_dir_all(dir.path().join("env2/lib")).unwrap();
        std::fs::write(dir.path().join("env2/pyvenv.cfg"), "").unwrap();
        std::fs::write(dir.path().join("env2/lib/x.py"), "").unwrap();
        let mut seen = Vec::new();
        let cut = walk(dir.path(), 8, 1000, |p, _, _| seen.push(p.to_path_buf()));
        assert!(!cut);
        assert!(seen.iter().any(|p| p.ends_with("a.csv")));
        assert!(!seen.iter().any(|p| p.ends_with("x.py")));
        assert!(walk(dir.path(), 8, 2, |_, _, _| {}));
    }
}
