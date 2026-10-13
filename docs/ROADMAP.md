# Roadmap

Milestones are vertical slices: each ends with something a user can do on
real data, verified in the running app.

## M1 — Foundation (this scaffold)

Shell, workspace system, projects, TOML studio, dataset preview, SQL, Nexis
ML import, settings. See the README's status table for what is verified.

## M2 — Run things

The point at which Epoch stops being read-only.

- `epoch-runtime` local process runtime: spawn outside the UI process, stream
  stdout/stderr line by line, parse Nexis ML protocol events live, graceful
  stop (`{"cmd":"cancel"}`, then kill after a timeout), pause/resume, exit
  status, a concurrency limit. Windows: no console window; job objects so
  children die with the job.
- "Run" for a `train.toml` (Nexis ML engine detection: the project's venv,
  `PATH`, the Nexis managed engine dir) and for an arbitrary Python script,
  each behind an explicit confirmation that shows the exact command
  (`CommandPlan`).
- Live Training Metrics and Execution Logs for running jobs; a jobs list in
  the status bar; runs land in the index as they finish.
- Execution metadata per run: interpreter, package versions (`pip freeze`
  output captured at launch), git commit and dirty state, hardware.

## M3 — Configuration and models

- Schema-driven forms for known configs: Nexis ML `train.toml` keys first
  (the catalogue in Nexis's `hyperparams.ts`), then JSON-Schema files a
  project provides. Form and source stay in sync through the
  format-preserving editor.
- Edit run notes, tags and pins (writes `notes.json`, the Nexis format).
- Model Inspector: Safetensors and ONNX header reading (tensor names, shapes,
  dtypes, parameter counts) in Rust; architecture diagram from config or
  from the `weights` artifact.

## M4 — Data, deeper

- SQLite databases and remote connections (credentials in the OS keychain).
- Image and text datasets (folder-per-class, sample grids).
- Distributions, correlation heatmaps, per-column histograms from DuckDB.
- Polars transforms as a pipeline step; the Pipelines page.

## M5 — Comparison and reproducibility

- Run comparison table (hyperparameter diff + final metrics), parallel
  coordinates, run grouping.
- Re-run with the exact snapshot of a past run.
- Generated frontend types from Rust (`ts-rs`/`specta`), replacing the
  hand-kept mirror.

## Ecosystem

- `epoch://` URL scheme so Nexis can open a run or dataset in Epoch (the
  family links apps by URL scheme, not a launcher).
- GPUI: the `epoch-*` crates have no UI dependency, so a native Nexis can
  reuse them directly when it wants these views.

## Housekeeping

- Choose a licence for Epoch itself (it has none yet).
- macOS and Linux hand-testing; code signing for releases.
