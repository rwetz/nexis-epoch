# Nexis Epoch

```
Epoch   build. train. discover.   a local research studio for your own models and data
```

Epoch is a local-first desktop application for data science and machine
learning work: open any Python/ML repository as it is, explore its datasets
with DuckDB, edit its TOML configuration safely, and study its training runs
in a dockable workspace you arrange yourself. No cloud account, no required
project structure, and nothing in your project runs unless you ask.

It belongs to the [Nexis](https://github.com/rwetz/Nexis) family, built on
[@nexis/design](https://github.com/rwetz/nexis-design), and reads Nexis ML
Lab experiments directly, but it is a standalone app.

> **Status: milestone 1 (foundation).** Read the table below before relying
> on anything; it says what has been verified and how.

## What works

| Area | What you can do | Verified by |
|---|---|---|
| **Projects** | Open any folder; Epoch recognises `pyproject.toml`, `train.toml`, requirements files, virtual environments (version from `pyvenv.cfg`), datasets, checkpoint folders and Nexis ML run stores. Recent projects, rescan, close. | Rust tests on fixture repos |
| **Project explorer** | Lazy file tree; TOML opens in the configuration editor, CSV/Parquet/JSON in the dataset explorer. Paths are confined to the project (no `..`, no symlink escapes). | Rust tests (incl. traversal and symlink cases) |
| **TOML studio** | CodeMirror editor with live validation (error underline + jump to line), a structured tree of tables/arrays/values, in-place value edits that keep comments and layout, unsaved-change tracking and close guard, atomic save that refuses to overwrite a file changed on disk. | Rust tests (byte-exact round trip, comment preservation, conflict detection) |
| **Dataset explorer** | Schema, row count, a virtualised paged grid with server-side sort and filters (values bound as parameters), DuckDB `SUMMARIZE` statistics, cancel for slow reads. CSV, TSV, Parquet, JSON, JSONL. Never loads a whole file into the UI. | Rust tests against real CSV/Parquet/JSONL via DuckDB 1.5.5; page-cache unit tests |
| **SQL editor** | Ad-hoc DuckDB SQL (Ctrl/⌘+Enter), datasets exposed as named views with column completion, row cap, timeout, cancel. Sandboxed: reads only inside the project, cannot write files, change settings or load extensions. | Rust tests: sandbox escape attempts, `SELECT 1; COPY …` smuggling, timeout, cross-thread cancel |
| **Experiments** | Imports Nexis ML runs (`.nexis-ml/runs`, both engines) read-only into a rebuildable SQLite index; list, filter, focus and compare up to 5 runs. | Rust tests + a test run against two real projects (19 runs) |
| **Training metrics** | Every recorded metric as its own chart (small multiples), overlaid across selected runs, spike-preserving decimation; pin a panel to one run. | Typecheck + browser render; charts not yet seen with real data in the app |
| **Logs / Model inspector** | A run's log (level filter, search), generated-text samples, configuration snapshot, checkpoints, per-epoch confusion matrices with an epoch slider. | Same as above |
| **Workspace** | Five workspaces (Studio, Data, Experiments, Models, Config) built from a central panel registry; split, tab, dock, float, hide, zoom; multiple instances of a panel; layouts saved per workspace in a versioned format with migrations; reset to default. | Unit tests (migrations, registry, presets); save → reload → restore checked in a browser |
| **Shell** | Title bar with project, workspace switcher, search (Ctrl/⌘+P: files + commands), command palette (Ctrl/⌘+Shift+P, ~60 commands), collapsible sidebar, status bar (project, Python env, backend, running queries, CPU/memory, NVIDIA GPU when `nvidia-smi` exists), 22 themes × light/dark × high contrast. | Browser render in two themes and both modes |
| **Settings** | Data limits (page size, row limit, timeout, memory, threads), Python interpreter, hidden files, index-on-open; saved to `settings.toml` keeping your comments; appearance; workspace navigation; forget saved layouts. | Rust tests (round trip, per-field fallback, comment preservation) |

**Not yet verified in the running desktop app.** Everything above was
checked by tests and by rendering the frontend in a browser (where the Rust
backend is unavailable). The first desktop build runs in CI; see "Status of
the desktop build" below.

## What doesn't exist yet

Clearly labelled in the app where it shows at all:

- **Running training.** `epoch-runtime` defines jobs, the exact commands and
  their events, but nothing launches a process. Metrics and logs are read from
  finished runs' files, not streamed live.
- **Schema-driven config forms**, editing run notes/tags, architecture
  diagrams, Safetensors/ONNX inspection.
- **More data sources**: SQLite databases, remote databases, image and text
  datasets, Polars transforms, the Pipelines page.
- **Generated TS types** from Rust (the mirror is hand-kept).

The [roadmap](docs/ROADMAP.md) orders these.

## Quick start

```bash
pnpm install
pnpm tauri:dev        # desktop app; first run compiles Tauri (DuckDB is prebuilt)
```

Then **Open project folder** and point it at any repository. Try a Nexis ML
project (one with `.nexis-ml/runs`) and the **Experiments** workspace.

Frontend-only (`pnpm dev`) works in a browser for UI work; panels say the
backend is unavailable rather than showing invented data.

More: [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

## Status of the desktop build

| Platform | Built | Run by hand |
|---|---|---|
| Windows | CI | not yet |
| Linux | CI | not yet |
| macOS | CI | not yet |

## How it is built

Tauri 2 · React 19 · TypeScript (strict) · Vite · Tailwind 4 ·
@nexis/design · Trellis (workspace) · CodeMirror 6 · TanStack Table/Virtual ·
ECharts · Rust (DuckDB, SQLite, toml_edit).

```
apps/desktop/        Tauri app: React frontend + thin Rust commands
crates/epoch-core    paths, files, project detection, Nexis ML format
crates/epoch-config  TOML parsing/editing, settings.toml
crates/epoch-store   SQLite: recent projects, experiment index
crates/epoch-data    DuckDB: datasets and sandboxed SQL
crates/epoch-runtime job interfaces (no execution yet)
```

The crates know nothing about the UI, so a native (GPUI) frontend can reuse
them. Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the design,
[docs/NEXIS_COMPAT.md](docs/NEXIS_COMPAT.md) for what was verified about
the Nexis ML format, and [docs/DEPENDENCIES.md](docs/DEPENDENCIES.md) for
the dependency checks and trade-offs.

## Licensing note

The workspace layout library, **Trellis, is free for non-commercial use
only**; commercial use requires a sponsorship or enterprise licence from its
author. Epoch isolates it behind an adapter so it can be replaced. Details in
[docs/DEPENDENCIES.md](docs/DEPENDENCIES.md). Epoch itself has no licence
file yet.
