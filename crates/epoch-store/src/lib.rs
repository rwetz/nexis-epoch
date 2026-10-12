//! Epoch's application metadata in SQLite.
//!
//! The database is an index, never the source of truth: experiment files
//! stay where the engine wrote them, readable without Epoch, and
//! [`Store::reindex_project`] rebuilds the index from them at any time.
//! Large files (checkpoints, datasets) are never stored here.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use epoch_core::nexis_ml::run_store::{self, MetricStats, RunRecord};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error(transparent)]
    Core(#[from] epoch_core::CoreError),
    #[error("database schema {found} is newer than this build understands ({supported})")]
    TooNew { found: i64, supported: i64 },
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// Forward-only migrations; `PRAGMA user_version` records how many ran.
const MIGRATIONS: &[&str] = &[
    // 1: recent projects, the experiment index, small key/value state.
    "CREATE TABLE recent_projects (
        path TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        last_opened_ms INTEGER NOT NULL
     );
     CREATE TABLE experiment_runs (
        run_dir TEXT PRIMARY KEY,
        project_root TEXT NOT NULL,
        project_dir TEXT NOT NULL,
        run_id TEXT NOT NULL,
        format TEXT NOT NULL,
        engine TEXT,
        name TEXT,
        status TEXT NOT NULL,
        started_at TEXT,
        finished_at TEXT,
        device TEXT,
        total_epochs INTEGER,
        last_epoch INTEGER,
        metrics_json TEXT NOT NULL,
        note TEXT NOT NULL,
        tags_json TEXT NOT NULL,
        pinned INTEGER NOT NULL,
        source_modified_ms INTEGER NOT NULL,
        indexed_at_ms INTEGER NOT NULL
     );
     CREATE INDEX experiment_runs_by_project ON experiment_runs (project_root, run_id);
     CREATE TABLE kv (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
];

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentProject {
    pub path: String,
    pub name: String,
    pub last_opened_ms: i64,
    /// The folder still exists.
    pub exists: bool,
}

/// A run as stored in the index.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexedRun {
    pub run_dir: String,
    pub project_dir: String,
    pub run_id: String,
    pub format: String,
    pub engine: Option<String>,
    pub name: Option<String>,
    pub status: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub device: Option<String>,
    pub total_epochs: Option<i64>,
    pub last_epoch: Option<i64>,
    pub metrics: BTreeMap<String, MetricStats>,
    pub note: String,
    pub tags: Vec<String>,
    pub pinned: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStats {
    pub stores: usize,
    pub runs: usize,
    pub updated: usize,
    pub removed: usize,
    /// Stores that could not be read, with the reason.
    pub errors: Vec<String>,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| epoch_core::CoreError::io(parent, e))?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", true)?;
        let mut store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    pub fn schema_version(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?)
    }

    fn migrate(&mut self) -> Result<()> {
        let current = self.schema_version()?;
        let supported = MIGRATIONS.len() as i64;
        if current > supported {
            return Err(StoreError::TooNew {
                found: current,
                supported,
            });
        }
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", i as i64 + 1)?;
            tx.commit()?;
        }
        Ok(())
    }

    // ── Recent projects ───────────────────────────────────────────────────

    pub fn touch_project(&self, path: &str, name: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO recent_projects (path, name, last_opened_ms) VALUES (?1, ?2, ?3)
             ON CONFLICT(path) DO UPDATE SET name = excluded.name, last_opened_ms = excluded.last_opened_ms",
            params![path, name, now_ms()],
        )?;
        Ok(())
    }

    pub fn recent_projects(&self, limit: usize) -> Result<Vec<RecentProject>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, name, last_opened_ms FROM recent_projects ORDER BY last_opened_ms DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit as i64], |r| {
            let path: String = r.get(0)?;
            Ok(RecentProject {
                exists: Path::new(&path).is_dir(),
                path,
                name: r.get(1)?,
                last_opened_ms: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn forget_project(&self, path: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM recent_projects WHERE path = ?1", [path])?;
        self.conn.execute(
            "DELETE FROM experiment_runs WHERE project_root = ?1",
            [path],
        )?;
        Ok(())
    }

    // ── Key/value ─────────────────────────────────────────────────────────

    pub fn get_kv(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM kv WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    pub fn set_kv(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO kv (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    // ── Experiment index ──────────────────────────────────────────────────

    /// Re-read every store under `project_dirs` (each a directory holding
    /// `.nexis-ml/runs`) and make the index match. Runs whose files have not
    /// changed since the last pass are skipped; runs that disappeared are
    /// removed.
    pub fn reindex_project(
        &mut self,
        project_root: &str,
        project_dirs: &[&Path],
    ) -> Result<IndexStats> {
        let mut stats = IndexStats {
            stores: project_dirs.len(),
            ..Default::default()
        };
        let mut records: Vec<RunRecord> = Vec::new();
        for dir in project_dirs {
            match run_store::list_runs(dir) {
                Ok(runs) => records.extend(runs),
                Err(e) => stats.errors.push(e.to_string()),
            }
        }
        stats.runs = records.len();
        let tx = self.conn.transaction()?;
        let known: BTreeMap<String, i64> = {
            let mut stmt = tx.prepare(
                "SELECT run_dir, source_modified_ms FROM experiment_runs WHERE project_root = ?1",
            )?;
            let rows = stmt.query_map([project_root], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let now = now_ms();
        let mut seen = HashSet::new();
        {
            let mut upsert = tx.prepare(
                "INSERT INTO experiment_runs (run_dir, project_root, project_dir, run_id, format, engine,
                    name, status, started_at, finished_at, device, total_epochs, last_epoch,
                    metrics_json, note, tags_json, pinned, source_modified_ms, indexed_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
                 ON CONFLICT(run_dir) DO UPDATE SET
                    project_root = excluded.project_root, project_dir = excluded.project_dir,
                    run_id = excluded.run_id, format = excluded.format, engine = excluded.engine,
                    name = excluded.name, status = excluded.status, started_at = excluded.started_at,
                    finished_at = excluded.finished_at, device = excluded.device,
                    total_epochs = excluded.total_epochs, last_epoch = excluded.last_epoch,
                    metrics_json = excluded.metrics_json, note = excluded.note,
                    tags_json = excluded.tags_json, pinned = excluded.pinned,
                    source_modified_ms = excluded.source_modified_ms,
                    indexed_at_ms = excluded.indexed_at_ms",
            )?;
            for r in &records {
                seen.insert(r.dir.clone());
                let modified = r.source_modified_ms as i64;
                if known.get(&r.dir) == Some(&modified) {
                    continue;
                }
                upsert.execute(params![
                    r.dir,
                    project_root,
                    r.project_dir,
                    r.id,
                    r.format,
                    r.engine,
                    r.name,
                    r.status,
                    r.started_at,
                    r.finished_at,
                    r.device,
                    r.total_epochs.map(|v| v as i64),
                    r.last_epoch.map(|v| v as i64),
                    serde_json::to_string(&r.metrics).unwrap_or_else(|_| "{}".into()),
                    r.notes.note,
                    serde_json::to_string(&r.notes.tags).unwrap_or_else(|_| "[]".into()),
                    r.notes.pinned,
                    modified,
                    now,
                ])?;
                stats.updated += 1;
            }
            let mut delete = tx.prepare("DELETE FROM experiment_runs WHERE run_dir = ?1")?;
            for dir in known.keys().filter(|d| !seen.contains(*d)) {
                delete.execute([dir])?;
                stats.removed += 1;
            }
        }
        tx.commit()?;
        Ok(stats)
    }

    /// Indexed runs for a project: pinned first, then newest first.
    pub fn runs_for_project(&self, project_root: &str) -> Result<Vec<IndexedRun>> {
        let mut stmt = self.conn.prepare(
            "SELECT run_dir, project_dir, run_id, format, engine, name, status, started_at,
                    finished_at, device, total_epochs, last_epoch, metrics_json, note, tags_json, pinned
             FROM experiment_runs WHERE project_root = ?1
             ORDER BY pinned DESC, run_id DESC",
        )?;
        let rows = stmt.query_map([project_root], |r| {
            let metrics: String = r.get(12)?;
            let tags: String = r.get(14)?;
            Ok(IndexedRun {
                run_dir: r.get(0)?,
                project_dir: r.get(1)?,
                run_id: r.get(2)?,
                format: r.get(3)?,
                engine: r.get(4)?,
                name: r.get(5)?,
                status: r.get(6)?,
                started_at: r.get(7)?,
                finished_at: r.get(8)?,
                device: r.get(9)?,
                total_epochs: r.get(10)?,
                last_epoch: r.get(11)?,
                metrics: serde_json::from_str(&metrics).unwrap_or_default(),
                note: r.get(13)?,
                tags: serde_json::from_str(&tags).unwrap_or_default(),
                pinned: r.get(15)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_run(project: &Path, id: &str, status: &str) {
        let run = project.join(".nexis-ml/runs").join(id);
        std::fs::create_dir_all(&run).unwrap();
        std::fs::write(
            run.join("summary.json"),
            format!(r#"{{"status":"{status}","metrics":{{"acc/val":{{"last":0.9,"min":0.5,"max":0.9,"count":3}}}}}}"#),
        )
        .unwrap();
    }

    #[test]
    fn migrates_to_the_latest_schema_once() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("epoch.db");
        assert_eq!(
            Store::open(&db).unwrap().schema_version().unwrap(),
            MIGRATIONS.len() as i64
        );
        // Re-opening runs nothing again.
        assert_eq!(
            Store::open(&db).unwrap().schema_version().unwrap(),
            MIGRATIONS.len() as i64
        );
    }

    #[test]
    fn refuses_a_newer_schema() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("epoch.db");
        Connection::open(&db)
            .unwrap()
            .pragma_update(None, "user_version", 99)
            .unwrap();
        assert!(matches!(
            Store::open(&db),
            Err(StoreError::TooNew { found: 99, .. })
        ));
    }

    #[test]
    fn recent_projects_are_most_recent_first() {
        let store = Store::open_in_memory().unwrap();
        store.touch_project("/a", "a").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        store.touch_project("/b", "b").unwrap();
        let recent = store.recent_projects(10).unwrap();
        assert_eq!(
            recent.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            ["b", "a"]
        );
        store.forget_project("/b").unwrap();
        assert_eq!(store.recent_projects(10).unwrap().len(), 1);
    }

    #[test]
    fn index_can_be_rebuilt_from_files() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path();
        make_run(project, "2026-01-01-0000-a", "ok");
        make_run(project, "2026-01-02-0000-b", "error");
        let root = project.display().to_string();
        let mut store = Store::open_in_memory().unwrap();

        let stats = store.reindex_project(&root, &[project]).unwrap();
        assert_eq!((stats.runs, stats.updated, stats.removed), (2, 2, 0));
        let runs = store.runs_for_project(&root).unwrap();
        assert_eq!(runs[0].run_id, "2026-01-02-0000-b");
        assert_eq!(runs[1].metrics["acc/val"].last, Some(0.9));

        // Unchanged files are skipped; deleted runs leave the index.
        let stats = store.reindex_project(&root, &[project]).unwrap();
        assert_eq!(stats.updated, 0);
        std::fs::remove_dir_all(project.join(".nexis-ml/runs/2026-01-02-0000-b")).unwrap();
        let stats = store.reindex_project(&root, &[project]).unwrap();
        assert_eq!(stats.removed, 1);
        assert_eq!(store.runs_for_project(&root).unwrap().len(), 1);
    }

    #[test]
    fn unreadable_stores_are_reported_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open_in_memory().unwrap();
        let missing = dir.path().join("nope");
        let stats = store.reindex_project("/p", &[missing.as_path()]).unwrap();
        assert_eq!(stats.errors.len(), 1);
    }
}
