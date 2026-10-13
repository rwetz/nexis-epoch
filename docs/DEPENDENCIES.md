# Dependencies: what was checked, and the trade-offs

Versions were chosen from releases at least two weeks old at scaffold time
(October 2026), except where noted.

## dockview (workspace layout)

| | |
|---|---|
| Package | `dockview-react` 8.3.1 (with `dockview` and `dockview-core` held to 8.3.1 by a pnpm override), [dockview.dev](https://dockview.dev) |
| Licence | **MIT** for everything Epoch uses |
| What it gives Epoch | Splits, tab groups, drag-to-dock, floating windows, maximize, serialisable layouts, panels kept alive while they move |
| Not used | `dockview-enterprise` (separately licensed): auto-hide edge groups, keyboard docking/navigation, undo/redo history, pinned and multi-row tabs, smart guides, DnD compass, advanced overflow. None is installed; the free core loads its free modules (floating groups, popouts, context menu, …) by default. |

`dockview-react` really does re-export the core at runtime (verified in its
bundle, unlike Trellis below), so all imports come from one package.

### History: Trellis (0.1.0)

Epoch 0.1.0 used Trellis (`@danfessler/trellis-react` 0.3.0), which is
**source-available, not open source**: free for non-commercial use only,
with commercial use requiring a GitHub Sponsorship or an enterprise licence.
0.2.0 replaced it with dockview so Epoch carries no use restriction. What
was given up: zooming into nested *groups* (dockview maximizes one group at
a time), hiding panels, and free/gesture navigation. Found while
integrating it: its React package's typings claim to re-export the core but
its JavaScript doesn't.

## Deviations from the brief's stack, and why

| Brief | Epoch | Reason |
|---|---|---|
| shadcn/ui + Tailwind | `@nexis/design` (git-pinned) on Tailwind v4 | "Scaffold Epoch like a Nexis app." @nexis/design *is* the family's shadcn-derived system: tokens, 22 themes, chrome, motion, ~70 components. Adding raw shadcn alongside would fork it. |
| Lucide icons | `<Icon name>` from @nexis/design (Phosphor underneath) | The family rule is one icon choke point; importing a second vendor is forbidden by its tripwire tests. |
| TanStack Table | v8.21 (not v9) | v9 is an API rewrite; v8 is the long-stable line. Only the headless column model is used. |
| DuckDB | `duckdb` crate 1.10505 (DuckDB 1.5.5), linked two ways | `bundled` compiles DuckDB's C++ (minutes of full CPU); used in CI/release. Locally, `DUCKDB_DOWNLOAD_LIB=1` links DuckDB's official prebuilt library (Parquet and JSON included), so development never compiles DuckDB. |
| Tokio | via Tauri's runtime (`tauri::async_runtime`) | Commands are async and hand blocking work to `spawn_blocking`. `epoch-runtime` uses `tokio::sync` for its event channel. |
| `toml` | `toml_edit` 0.25 | Comment- and format-preserving edits need `toml_edit`; one TOML library for parse and edit. |
| `packages/ui`, `python/epoch-sdk` | not created | The design system already is a package; an SDK has nothing to do until jobs run. No empty packages. |

## The rest

| Area | Package | Version | Licence |
|---|---|---|---|
| Shell | `tauri`, `tauri-build` | 2.12 / 2.7 | MIT/Apache-2.0 |
| | `tauri-plugin-dialog`, `-os` | 2.8 / 2.4 | MIT/Apache-2.0 |
| Data | `duckdb` | =1.10505.0 | MIT |
| Metadata | `rusqlite` (bundled SQLite) | 0.40 | MIT |
| Config | `toml_edit` | 0.25 | MIT/Apache-2.0 |
| System | `sysinfo` (system feature only) | 0.39 | MIT |
| Paths | `dunce` | 1.0 | CC0/MIT/Apache-2.0 |
| Frontend | React 19, Vite 7, TypeScript 5.9 (strict), Tailwind 4 | via @nexis/design | MIT |
| Editor | CodeMirror 6 (`@codemirror/*`, legacy TOML mode, `lang-sql`) | 6.x | MIT |
| Grid | `@tanstack/react-table` 8, `@tanstack/react-virtual` 3 | | MIT |
| Charts | `echarts` (modular import: line, bar, heatmap) | 6.1 | Apache-2.0 |
| State | `zustand` | 5.0 | MIT |

Not installed, by design: PyTorch, NumPy, Polars, scikit-learn, Safetensors,
ONNX Runtime. Python support will run through managed subprocesses
(`epoch-runtime`), not an embedded interpreter.
