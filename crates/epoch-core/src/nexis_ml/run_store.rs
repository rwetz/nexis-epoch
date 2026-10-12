//! Reading a Nexis ML run store: `<project>/.nexis-ml/runs/<run-id>/`.
//!
//! ```text
//! config.json     resolved hyperparameters (+ "engine", "derived" on nexis-ml-rs)
//! metrics.jsonl   every protocol event, one per line
//! summary.json    final status and per-metric last/min/max/count
//! checkpoints/    best.pt / last.pt (Python), best.json + best-weights.json (Rust)
//! artifacts/      confusion matrices, weights, sample grids
//! notes.json      note, tags, pinned (written by Nexis; optional)
//! ```
//!
//! Epoch only reads these files. Two facts verified against real stores
//! rather than the docs: artifact paths are recorded as *absolute* paths
//! (Windows paths on Windows), and `notes.json` is usually absent. Recorded
//! paths are rebased onto the run directory, so a store that was moved or
//! copied to another machine still resolves.

use std::collections::BTreeMap;
use std::io::BufRead as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::protocol::{parse_line, Event};
use crate::error::{CoreError, Result};

/// For display; use [`runs_dir`] to build paths (native separators).
pub const RUNS_SUBDIR: &str = ".nexis-ml/runs";
pub const FORMAT: &str = "nexis-ml/1";

/// `<project_dir>/.nexis-ml/runs`, with the platform's separators.
pub fn runs_dir(project_dir: &Path) -> PathBuf {
    project_dir.join(".nexis-ml").join("runs")
}

/// Most runs listed per project, newest first (matches Nexis).
pub const MAX_RUNS: usize = 200;
/// Points kept per metric series before min/max decimation.
pub const MAX_POINTS: usize = 4_000;
const MAX_LOG_LINES: usize = 20_000;
const MAX_SAMPLES: usize = 200;
const MAX_ARTIFACT_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MetricStats {
    #[serde(default)]
    pub last: Option<f64>,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub count: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SummaryFile {
    status: Option<String>,
    name: Option<String>,
    started_at: Option<String>,
    finished_at: Option<String>,
    total_epochs: Option<u64>,
    last_epoch: Option<u64>,
    device: Option<String>,
    #[serde(default)]
    metrics: BTreeMap<String, MetricStats>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct RunNotes {
    pub note: String,
    pub tags: Vec<String>,
    pub pinned: bool,
}

impl RunNotes {
    /// Defensive, like Nexis's `parseRunMeta`: garbage becomes empty.
    fn from_value(v: &serde_json::Value) -> Self {
        Self {
            note: v
                .get("note")
                .and_then(|n| n.as_str())
                .unwrap_or_default()
                .to_string(),
            tags: v
                .get("tags")
                .and_then(|t| t.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|t| t.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            pinned: v.get("pinned").and_then(|p| p.as_bool()).unwrap_or(false),
        }
    }
}

/// One run, as listed in the experiment explorer and indexed in SQLite.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRecord {
    pub id: String,
    pub dir: String,
    pub project_dir: String,
    pub format: &'static str,
    /// `config.json` → `engine` (`nexis-ml-rs`), when recorded.
    pub engine: Option<String>,
    pub name: Option<String>,
    /// `ok`, `error`, `cancelled`, or `unknown` (no summary: running or crashed).
    pub status: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub device: Option<String>,
    pub total_epochs: Option<u64>,
    pub last_epoch: Option<u64>,
    pub metrics: BTreeMap<String, MetricStats>,
    pub notes: RunNotes,
    /// Latest modification time of the files the record was built from,
    /// for incremental re-indexing.
    pub source_modified_ms: u64,
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn mtime_ms(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Build the record for one run directory from its small files.
pub fn read_run(project_dir: &Path, run_dir: &Path) -> RunRecord {
    let id = run_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let config = read_json(&run_dir.join("config.json"));
    let summary: SummaryFile = read_json(&run_dir.join("summary.json"))
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    let notes = read_json(&run_dir.join("notes.json"))
        .map(|v| RunNotes::from_value(&v))
        .unwrap_or_default();
    let source_modified_ms = ["config.json", "summary.json", "notes.json", "metrics.jsonl"]
        .iter()
        .map(|f| mtime_ms(&run_dir.join(f)))
        .max()
        .unwrap_or(0);
    RunRecord {
        dir: run_dir.display().to_string(),
        project_dir: project_dir.display().to_string(),
        format: FORMAT,
        engine: config
            .as_ref()
            .and_then(|c| c.get("engine"))
            .and_then(|e| e.as_str())
            .map(str::to_string),
        name: summary.name,
        status: summary.status.unwrap_or_else(|| "unknown".into()),
        started_at: summary.started_at,
        finished_at: summary.finished_at,
        device: summary.device,
        total_epochs: summary.total_epochs,
        last_epoch: summary.last_epoch,
        metrics: summary.metrics,
        notes,
        source_modified_ms,
        id,
    }
}

/// Every run under `<project_dir>/.nexis-ml/runs`, pinned first, then newest
/// first (run ids start with a timestamp, so name order is time order).
pub fn list_runs(project_dir: &Path) -> Result<Vec<RunRecord>> {
    let runs_dir = runs_dir(project_dir);
    let read = std::fs::read_dir(&runs_dir).map_err(|e| CoreError::io(&runs_dir, e))?;
    let mut dirs: Vec<PathBuf> = read
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs.reverse();
    dirs.truncate(MAX_RUNS);
    let mut runs: Vec<_> = dirs.iter().map(|d| read_run(project_dir, d)).collect();
    runs.sort_by_key(|r| !r.notes.pinned); // stable: keeps newest-first within groups
    Ok(runs)
}

/// One metric's values over training steps.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricSeries {
    pub name: String,
    /// `[step, value]` pairs in event order.
    pub points: Vec<[f64; 2]>,
    /// Points were reduced to per-bucket min/max to stay under [`MAX_POINTS`].
    pub decimated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub level: String,
    pub msg: String,
    pub epoch: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleOutput {
    pub epoch: Option<u64>,
    pub output: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactInfo {
    pub kind: String,
    /// Relative to the run directory, after rebasing; `None` if it could
    /// not be placed inside the run.
    pub path: Option<String>,
    pub recorded_path: String,
    pub epoch: Option<u64>,
    pub exists: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub name: String,
    pub size: u64,
}

/// Everything the metrics, logs and model panels need for one run.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunDetail {
    pub record: RunRecord,
    pub config: Option<serde_json::Value>,
    pub series: Vec<MetricSeries>,
    pub logs: Vec<LogLine>,
    pub samples: Vec<SampleOutput>,
    pub artifacts: Vec<ArtifactInfo>,
    pub checkpoints: Vec<FileInfo>,
    pub event_count: u64,
    /// Lines that were blank, not JSON, or not a known event.
    pub skipped_lines: u64,
    /// The event log was longer than the reader is willing to keep in full.
    pub logs_truncated: bool,
}

/// Read a run's event log and files. Streams `metrics.jsonl` line by line,
/// so a long run costs memory proportional to what is kept, not file size.
pub fn read_run_detail(project_dir: &Path, run_dir: &Path) -> Result<RunDetail> {
    if !run_dir.is_dir() {
        return Err(CoreError::NotFound {
            path: run_dir.to_path_buf(),
        });
    }
    let record = read_run(project_dir, run_dir);
    let config = read_json(&run_dir.join("config.json"));
    let mut series: BTreeMap<String, Vec<[f64; 2]>> = BTreeMap::new();
    let mut logs = Vec::new();
    let mut samples = Vec::new();
    let mut artifacts = Vec::new();
    let (mut event_count, mut skipped_lines) = (0u64, 0u64);
    let mut logs_truncated = false;
    let mut epoch: Option<u64> = None;

    let metrics_path = run_dir.join("metrics.jsonl");
    if let Ok(file) = std::fs::File::open(&metrics_path) {
        for line in std::io::BufReader::new(file).lines() {
            let Ok(line) = line else {
                skipped_lines += 1;
                continue;
            };
            let Some(ev) = parse_line(&line) else {
                if !line.trim().is_empty() {
                    skipped_lines += 1;
                }
                continue;
            };
            event_count += 1;
            match ev {
                Event::Metric {
                    step,
                    epoch: e,
                    name,
                    value,
                    ..
                } => {
                    if e.is_some() {
                        epoch = e;
                    }
                    let points = series.entry(name).or_default();
                    let x = step.unwrap_or(points.len() as f64);
                    if value.is_finite() {
                        points.push([x, value]);
                    }
                }
                Event::Epoch { epoch: e, .. } => epoch = Some(e),
                Event::Log { level, msg, .. } => {
                    if logs.len() < MAX_LOG_LINES {
                        logs.push(LogLine {
                            level: level.unwrap_or_else(|| "info".into()),
                            msg,
                            epoch,
                        });
                    } else {
                        logs_truncated = true;
                    }
                }
                Event::Sample { output, .. } => {
                    let output = match output {
                        Some(serde_json::Value::String(s)) => s,
                        Some(other) => other.to_string(),
                        None => String::new(),
                    };
                    samples.push(SampleOutput { epoch, output });
                    if samples.len() > MAX_SAMPLES {
                        samples.remove(0);
                    }
                }
                Event::Artifact { kind, path, .. } => {
                    let resolved = rebase_artifact(run_dir, &path);
                    artifacts.push(ArtifactInfo {
                        kind,
                        exists: resolved.as_ref().is_some_and(|p| run_dir.join(p).is_file()),
                        path: resolved,
                        recorded_path: path,
                        epoch,
                    });
                }
                Event::RunStarted { .. } | Event::RunFinished { .. } => {}
            }
        }
    }

    let series = series
        .into_iter()
        .map(|(name, points)| {
            let decimated = points.len() > MAX_POINTS;
            let points = if decimated {
                decimate(&points, MAX_POINTS)
            } else {
                points
            };
            MetricSeries {
                name,
                points,
                decimated,
            }
        })
        .collect();

    let mut checkpoints: Vec<FileInfo> = std::fs::read_dir(run_dir.join("checkpoints"))
        .map(|r| {
            r.flatten()
                .filter_map(|e| {
                    let meta = e.metadata().ok()?;
                    meta.is_file().then(|| FileInfo {
                        name: e.file_name().to_string_lossy().into_owned(),
                        size: meta.len(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    checkpoints.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(RunDetail {
        record,
        config,
        series,
        logs,
        samples,
        artifacts,
        checkpoints,
        event_count,
        skipped_lines,
        logs_truncated,
    })
}

/// Place a recorded artifact path inside `run_dir`, returning it relative to
/// the run. Engines record absolute paths; when the store has moved those
/// no longer exist, so the part after `<run-id>/` is kept. `..` is refused.
pub fn rebase_artifact(run_dir: &Path, recorded: &str) -> Option<String> {
    let run_id = run_dir.file_name()?.to_string_lossy().into_owned();
    let parts: Vec<&str> = recorded
        .split(['/', '\\'])
        .filter(|p| !p.is_empty())
        .collect();
    let tail: &[&str] = match parts.iter().rposition(|p| *p == run_id) {
        Some(i) => &parts[i + 1..],
        // `/x` is not "absolute" to Windows' Path (no drive), but it is
        // still rooted somewhere else.
        None if !Path::new(recorded).is_absolute()
            && !recorded.starts_with(['/', '\\'])
            && !recorded.contains(':') =>
        {
            &parts
        }
        None => return None,
    };
    if tail.is_empty()
        || tail
            .iter()
            .any(|p| *p == ".." || *p == "." || p.contains(':'))
    {
        return None;
    }
    Some(tail.join("/"))
}

/// Read a JSON artifact (confusion matrix, weights) by its rebased path.
pub fn read_artifact_json(run_dir: &Path, rel: &str) -> Result<serde_json::Value> {
    let rel = rebase_artifact(run_dir, rel).ok_or_else(|| CoreError::InvalidPath(rel.into()))?;
    let path = run_dir.join(&rel);
    let meta = std::fs::metadata(&path).map_err(|e| CoreError::io(&path, e))?;
    if meta.len() > MAX_ARTIFACT_BYTES {
        return Err(CoreError::TooLarge {
            path,
            size: meta.len(),
            limit: MAX_ARTIFACT_BYTES,
        });
    }
    let text = std::fs::read_to_string(&path).map_err(|e| CoreError::io(&path, e))?;
    serde_json::from_str(&text).map_err(|e| CoreError::Invalid(format!("{rel}: {e}")))
}

/// Keep the first and last point and, per bucket, the min and max — so a
/// spike survives decimation (the same trade-off the Nexis panel makes).
pub fn decimate(points: &[[f64; 2]], max: usize) -> Vec<[f64; 2]> {
    if points.len() <= max || max < 4 {
        return points.to_vec();
    }
    let buckets = (max - 2) / 2;
    let inner = &points[1..points.len() - 1];
    let size = inner.len().div_ceil(buckets);
    let mut out = Vec::with_capacity(max);
    out.push(points[0]);
    for chunk in inner.chunks(size) {
        let (mut lo, mut hi) = (0, 0);
        for (i, p) in chunk.iter().enumerate() {
            if p[1] < chunk[lo][1] {
                lo = i;
            }
            if p[1] > chunk[hi][1] {
                hi = i;
            }
        }
        let (a, b) = if lo <= hi { (lo, hi) } else { (hi, lo) };
        out.push(chunk[a]);
        if a != b {
            out.push(chunk[b]);
        }
    }
    out.push(points[points.len() - 1]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A run laid out exactly like the nexis-ml-rs v0.8 store this was
    /// verified against, but recorded on a different machine.
    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("my-first-model");
        let run = runs_dir(&project).join("2026-09-18-1640-tabular-5");
        std::fs::create_dir_all(run.join("artifacts")).unwrap();
        std::fs::create_dir_all(run.join("checkpoints")).unwrap();
        let elsewhere = r"C:\\Users\\someone\\Dev\\my-first-model\\.nexis-ml\\runs\\2026-09-18-1640-tabular-5\\artifacts\\cm-epoch1.json";
        std::fs::write(
            run.join("config.json"),
            r#"{"engine":"nexis-ml-rs","model":{"hidden":[16]},"train":{"epochs":2}}"#,
        )
        .unwrap();
        std::fs::write(
            run.join("summary.json"),
            r#"{"status":"ok","name":"tabular","lastEpoch":2,"totalEpochs":2,"device":"cpu",
                "metrics":{"loss/train":{"last":0.1,"min":0.1,"max":0.7,"count":2}}}"#,
        )
        .unwrap();
        let events = [
            r#"{"ev":"run.started","run":"r","name":"tabular","totalEpochs":2,"protocol":1}"#
                .to_string(),
            r#"{"ev":"log","level":"info","msg":"burn MLP","run":"r"}"#.into(),
            r#"{"epoch":1,"ev":"metric","name":"loss/train","run":"r","step":1,"value":0.7}"#
                .into(),
            format!(
                r#"{{"ev":"artifact","kind":"confusion-matrix","path":"{elsewhere}","run":"r"}}"#
            ),
            r#"{"epoch":1,"ev":"epoch","of":2,"run":"r"}"#.into(),
            "garbage".into(),
            r#"{"epoch":2,"ev":"metric","name":"loss/train","run":"r","step":2,"value":0.1}"#
                .into(),
            r#"{"ev":"run.finished","run":"r","status":"ok"}"#.into(),
        ];
        std::fs::write(run.join("metrics.jsonl"), events.join("\n")).unwrap();
        std::fs::write(
            run.join("artifacts/cm-epoch1.json"),
            r#"{"labels":["0","1"],"matrix":[[45,0],[0,51]]}"#,
        )
        .unwrap();
        std::fs::write(run.join("checkpoints/best.json"), "{}").unwrap();
        (dir, project, run)
    }

    #[test]
    fn lists_runs_with_summary_fields() {
        let (_d, project, _run) = fixture();
        let runs = list_runs(&project).unwrap();
        assert_eq!(runs.len(), 1);
        let r = &runs[0];
        assert_eq!(r.id, "2026-09-18-1640-tabular-5");
        assert_eq!(r.status, "ok");
        assert_eq!(r.engine.as_deref(), Some("nexis-ml-rs"));
        assert_eq!(r.metrics["loss/train"].min, Some(0.1));
        assert_eq!(r.notes, RunNotes::default());
    }

    #[test]
    fn pinned_runs_sort_first() {
        let (_d, project, run) = fixture();
        let older = runs_dir(&project).join("2026-01-01-0000-tabular");
        std::fs::create_dir_all(&older).unwrap();
        std::fs::write(
            older.join("notes.json"),
            r#"{"note":"baseline","tags":["a",1],"pinned":true}"#,
        )
        .unwrap();
        let runs = list_runs(&project).unwrap();
        assert_eq!(runs[0].id, "2026-01-01-0000-tabular");
        assert_eq!(runs[0].notes.tags, ["a"]);
        assert_eq!(runs[0].status, "unknown");
        assert_eq!(runs[1].dir, run.display().to_string());
    }

    #[test]
    fn reads_events_and_rebases_foreign_artifact_paths() {
        let (_d, project, run) = fixture();
        let detail = read_run_detail(&project, &run).unwrap();
        assert_eq!(detail.series.len(), 1);
        assert_eq!(detail.series[0].points, vec![[1.0, 0.7], [2.0, 0.1]]);
        assert_eq!(detail.logs.len(), 1);
        assert_eq!(detail.skipped_lines, 1);
        assert_eq!(detail.event_count, 7);
        let cm = &detail.artifacts[0];
        assert_eq!(cm.path.as_deref(), Some("artifacts/cm-epoch1.json"));
        assert_eq!(cm.epoch, Some(1));
        assert!(cm.exists);
        assert_eq!(detail.checkpoints[0].name, "best.json");
        let matrix = read_artifact_json(&run, cm.path.as_deref().unwrap()).unwrap();
        assert_eq!(matrix["matrix"][1][1], 51);
    }

    #[test]
    fn rebase_refuses_escapes() {
        let run = Path::new("/p/.nexis-ml/runs/r1");
        assert_eq!(
            rebase_artifact(run, "/x/runs/r1/artifacts/a.json").as_deref(),
            Some("artifacts/a.json")
        );
        assert_eq!(
            rebase_artifact(run, "artifacts/a.json").as_deref(),
            Some("artifacts/a.json")
        );
        assert_eq!(rebase_artifact(run, "/x/runs/r1/../../etc/passwd"), None);
        assert_eq!(rebase_artifact(run, "/somewhere/else.json"), None);
        assert_eq!(rebase_artifact(run, "/x/runs/r1"), None);
    }

    #[test]
    fn decimation_keeps_spikes_and_ends() {
        let mut points: Vec<[f64; 2]> = (0..10_000).map(|i| [i as f64, 1.0]).collect();
        points[5_000][1] = 99.0;
        let out = decimate(&points, 100);
        assert!(out.len() <= 100);
        assert_eq!(out[0], [0.0, 1.0]);
        assert_eq!(out[out.len() - 1], [9_999.0, 1.0]);
        assert!(out.iter().any(|p| p[1] == 99.0));
        assert!(out.windows(2).all(|w| w[0][0] <= w[1][0]));
    }
}
