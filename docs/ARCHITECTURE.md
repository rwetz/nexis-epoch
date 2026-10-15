# Architecture

Nexis Epoch is a local-first desktop research studio: a Tauri 2 shell around
a React workspace, over Rust crates that do the real work. This document is
the map; the code comments carry the detail.

```
┌──────────────────────── apps/desktop (webview) ────────────────────────┐
│  app/        shell: title bar, sidebar, status bar, palette, pages     │
│  features/   panels by domain + the workspace system                   │
│  components/ grid, charts, editor, states (on @nexis/design)           │
│  lib/api.ts  one typed function per Tauri command                      │
└──────────────────────────────┬─────────────────────────────────────────┘
                               │ Tauri IPC (invoke → JSON)
┌──────────────────────── apps/desktop/src-tauri ────────────────────────┐
│  thin commands: validate, spawn_blocking, call a crate, map errors     │
└───────┬──────────────┬──────────────┬───────────────┬──────────────────┘
        │              │              │               │
  epoch-core     epoch-config    epoch-store     epoch-data      epoch-runtime
  paths, fs,     TOML parse/     SQLite: recent  DuckDB: schema, job specs,
  detection,     edit, settings  projects, run   pages, stats,   command plans,
  Nexis ML       .toml           index           sandboxed SQL   events (interfaces)
  run store
```

## Two hosts, one source

Epoch runs as its own app and as a window inside Nexis, from the same code:

- **Backend:** every command is in `crates/tauri-plugin-epoch`, a Tauri
  plugin. The standalone app (`apps/desktop/src-tauri`) registers it and adds
  only its window, the OS plugins and the loopback link channel to Nexis
  (`links.rs`). Nexis registers the same plugin, pinned by commit. Both keep
  Epoch's state in the standalone app's folders (`app.nexis.epoch`), so
  recent projects, the run index, layouts and settings are shared.
- **Frontend:** `src/embed.tsx` mounts the whole UI given an `EpochHost`
  (`src/host.ts`). `main.tsx` mounts it with the standalone host; Nexis
  builds `embed.tsx` into a page of its own (`epoch.html`) with a host whose
  links come from the window's launch parameters and which opens Synapse and
  Pulse in place. Hosts type-check against `embed.d.ts`, which a test keeps
  in step with the implementation. Imports inside Epoch use `@epoch/…`,
  because `@` belongs to Nexis's sources in that build.
- Commands are called as `plugin:epoch|<name>` (`lib/ipc.ts`); each host
  grants `epoch:default` to its Epoch window.

## Principles

- **The core knows nothing about the UI.** `epoch-core`, `epoch-config`,
  `epoch-store`, `epoch-data` and `epoch-runtime` have no Tauri, React or
  layout-library dependency and are tested on their own. A native (GPUI)
  Nexis could link them directly.
- **Commands are adapters.** `crates/tauri-plugin-epoch/src/commands/*` validates input, moves
  blocking work off the async runtime (`spawn_blocking`) and calls a crate.
  Logic worth testing lives in the crates.
- **Files stay the source of truth.** Epoch reads projects in place. SQLite
  is an index that can be rebuilt; experiment files stay readable without
  Epoch; large files never enter the database.
- **Nothing runs on open.** Opening a project reads files. Python versions
  come from `pyvenv.cfg`, not from launching an interpreter. The only process
  Epoch starts by itself is `nvidia-smi`, for the status bar, when it is on
  `PATH`.

## Crates

| Crate | Responsibility | Notable choices |
|---|---|---|
| `epoch-core` | `ProjectRoot` (path confinement), bounded listing/walking, binary-safe reads, atomic writes with conflict detection, project detection, the Nexis ML protocol and run-store reader | No async: callers decide where to block. FNV-1a content hashes detect external edits. |
| `epoch-config` | TOML parse → diagnostics (line, UTF-16 column) and a tree; format-preserving value edits; `settings.toml` | `toml_edit`: untouched documents round-trip byte-for-byte; an edited value keeps its surrounding comments and whitespace but is re-spelled (`1_000` → `1000`). Epoch claims no more than that. |
| `epoch-store` | SQLite: recent projects, the experiment index, key/value | Forward-only migrations via `PRAGMA user_version`; refuses a newer schema. Incremental re-index by file mtime; runs whose files vanished are removed. |
| `epoch-data` | DuckDB over CSV/TSV/Parquet/JSON/JSONL: describe, paged preview with sort and filters, `SUMMARIZE`, ad-hoc SQL | The only crate that links DuckDB, so the rest builds without it. See "Data layer". |
| `epoch-runtime` | Job specs, the exact command a job would run, job events, the `Runtime` trait | Interfaces only in milestone 1. A `LaunchConsent` (made only from a user action) is required to describe a job. |

## Data layer

Files are queried where they are (`read_csv`, `read_parquet`, `read_json`);
only the requested page crosses to the UI.

**Sandbox.** Each project gets an in-memory DuckDB configured, in order:
`allowed_directories = [<project>]`, `enable_external_access = false` (only
the allowed folder stays readable), extension autoload/autoinstall off, then
`lock_configuration = true` so SQL cannot undo it. Tests prove a file outside
the project can't be read and the lock can't be lifted.

**The SQL editor reads; it doesn't write.** DuckDB's allowlist permits writes
inside the project, so ad-hoc SQL is limited to read-type statements by
leading keyword (`SELECT`, `WITH`, `FROM`, `DESCRIBE`, `SUMMARIZE`, …; DDL/DML
only touch the in-memory session). `COPY`, `ATTACH`, `INSTALL`, `LOAD`, `SET`
and `EXPLAIN ANALYZE` are refused, and so is more than one statement: the
DuckDB Rust binding runs every statement but the last when given several,
which would otherwise let `SELECT 1; COPY …` through. The statement counter
understands quotes, `$$` strings and comments.

**Relative paths.** DuckDB checks the sandbox against the raw path text, so
`FROM 'data/x.csv'` is refused. Datasets are exposed by name instead: the
SQL editor's *Datasets* menu creates a view over the file's absolute path.

**Limits.** Every request runs on its own cloned connection with a timeout
watchdog and a cancel handle (`query_cancel`), under a memory limit and a
row cap (Settings → Data). Values that a JavaScript number would round
(64-bit integers beyond 2^53, decimals) arrive as strings.

**The grid never holds a dataset.** `DataGrid` takes `getRow(i)` and reports
its visible range; `PageCache` fetches pages on demand and keeps at most 24
(least recently used dropped). TanStack Table provides the column model
(sizing, resizing, sort state) and TanStack Virtual renders visible rows.

**Polars.** Not in milestone 1. A transform step would sit beside
`epoch-data` (DuckDB and Polars both speak Arrow), behind the same
`DatasetInfo`/`Page` shapes.

## Workspace

The workspace is Epoch's own model, with [dockview](https://dockview.dev)
(MIT) as the current renderer. Epoch 0.1.0 used Trellis; the swap touched
only the adapter folder, as this design intends.

- `features/workspace/types.ts`: panel type ids (stable; saved layouts refer
  to them), typed params per panel, `PanelHandle` (what a panel can do about
  itself), the neutral `LayoutNode`, `WorkspaceApi`.
- `registry.ts`: every panel type is registered once (`features/panels.ts`);
  the shell, palette and adapter read the registry. Each definition owns
  `parseParams`, which repairs params from saved layouts.
- `dockview/`: **the only code that imports dockview.** `document.ts` turns
  presets into dockview documents and repairs saved ones (pure, unit-tested);
  `DockviewWorkspace.tsx` maps definitions to dockview components, dockview's
  panel API to `PanelHandle`, and draws Epoch's tabs (icon, unsaved-changes
  dot, a close button that runs the panel's close guard). Replacing dockview
  means rewriting this folder.
- Panels use `usePanel(type)` and never see the layout library. dockview's
  `defaultRenderer: "always"` keeps every panel's DOM alive as a hidden tab,
  and panels keep their React state while they move between groups or float,
  so state survives rearranging; workspaces stay mounted after first use.
- What the layout offers: splits, tab groups, drag-to-dock, floating
  windows, maximize (zoom one group), close, reset. Only dockview's free
  core is used; its paid `dockview-enterprise` modules are not installed.
- Five workspaces (Studio, Data, Experiments, Models, Config), each with its
  own saved layout. Multiple instances of a panel type are allowed; the
  experiment panels can be pinned to a run or follow the selection.

**Persistence and migrations.** Layouts are saved per workspace as
`<app data>/layouts/<id>.json`:

```json
{ "schema": 1, "workspace": "studio", "adapter": "dockview",
  "presetVersion": 1, "savedAt": "…", "document": { …dockview SerializedDockview… } }
```

Loading runs `migrateLayout`: each schema bump adds a function to
`MIGRATIONS` that upgrades the previous shape, and the chain runs from the
stored version up. Schema 0 (a bare dockview document, as `api.toJSON()`
returns it) is accepted, which also lets an exported dockview layout be
imported. A layout from a newer Epoch, another workspace or another adapter
(0.1.0's `trellis` layouts) is refused with a notice, and the preset is used
instead. Unknown panel types are dropped and params repaired
(`repairDocument`). Saves are debounced (400 ms) and atomic.

## Frontend/backend contract

`src/types/backend.ts` mirrors the Rust `serde` types by hand, and
`src/lib/api.ts` has one function per command. When a Rust type changes, its
mirror changes in the same commit. Errors arrive as `{ code, message }` with a
stable `code` (`not_found`, `permission_denied`, `conflict`, `timeout`,
`cancelled`, …); `BackendError` carries it. In a plain browser (`pnpm dev`)
every call fails with `unavailable` and panels show that state; there is no
mock backend to mistake for real data.

Generating these types (`ts-rs` or `specta`) is on the roadmap once the
surface settles.

## Nexis ML compatibility

See [NEXIS_COMPAT.md](NEXIS_COMPAT.md). In short: Epoch reads
`<project>/.nexis-ml/runs/<id>/` from either engine without modifying it,
indexes runs into SQLite, and renders metrics, logs, samples, configuration,
checkpoints and confusion matrices from the files. The protocol types in
`epoch-core::nexis_ml::protocol` are shared with `epoch-runtime`, so live
streaming will use the same parser.

## Charts

`components/charts/` takes generic data (`[x, y]` series, labelled matrices)
and knows nothing about losses or architectures. Colours come from tokens:

- Series identity uses `--series-1…5` (styles.css): the dataviz reference
  palette's first five slots in its CVD-safe order, validated on Epoch's card
  surfaces in both modes (all checks pass; light mode needs visible labels for
  contrast, which the legend and end-of-line values provide). The theme's own
  `--chart-*` are a neutral ramp and its saturated hues are status colours,
  so neither suits run identity. A run keeps its colour across themes and
  when other runs are deselected.
- One measure per chart, one y axis. Several metrics → small multiples.
- Matrices use a single-hue sequential scale and label every cell.
- Charts resize with their panel (ResizeObserver) and re-read colours when the
  palette changes.

## Security

- **Paths.** Every file command takes a path relative to the open project;
  `ProjectRoot::resolve` rejects absolute paths, `..`, NUL, Windows drive and
  stream syntax, and re-checks after following symlinks. Run directories
  passed back from the UI must be inside the project and inside a
  `.nexis-ml/runs` folder.
- **Capabilities.** The webview has window chrome, OS facts and the folder
  picker; no general filesystem, shell or HTTP permission. A strict CSP is set
  for production builds.
- **Writes** are atomic (temp file + rename) and refuse to clobber a file that
  changed since it was read (`conflict`), unless the user chooses to.
- **Secrets.** Nothing Epoch writes contains credentials, and settings.toml
  says so. Remote database connections, when they come, will keep secrets in
  the OS keychain (Credential Manager, Keychain, Secret Service), never in TOML.

## Platforms

- Windows: borderless window with native resize borders; `nvidia-smi` is
  started without a console window. Paths are compared after
  canonicalisation (`dunce`, so no `\\?\` prefixes reach the UI).
- Linux: the NVIDIA/Wayland WebKitGTK workaround and edge-resize overlay from
  @nexis/design (`PITFALLS.md` §1–§4) are in place.
- macOS: overlay title bar with native traffic lights.
- Only Windows has been run by hand so far; see the README's status table.
