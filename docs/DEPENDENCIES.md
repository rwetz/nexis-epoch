# Dependencies: what was checked, and the trade-offs

Versions were chosen from releases at least two weeks old at scaffold time
(October 2026), except where noted.

## Trellis UI (workspace layout) — licence warning

| | |
|---|---|
| Package | `@danfessler/trellis-react` 0.3.0 + `@danfessler/trellis` 0.3.0 (Dan Fessler, [github.com/DanFessler/trellis](https://github.com/DanFessler/trellis)) |
| What it gives Epoch | Splits, tab groups, docking, floating, hiding, focus/free zoom navigation, a serialisable `LayoutDocument`, content that stays mounted while panels move |
| Maturity | First published 26 Sep 2026; 0.3.0 was ~2 weeks old when adopted (a few hours short of Epoch's usual two-week bar — taken because the docs describe 0.3) |
| **Licence** | **Not open source.** Three tiers: free for **non-commercial** use; **commercial** use by teams of ≤10 developers requires an active GitHub Sponsorship at the required tier; larger organisations need an enterprise licence (`LICENSE.md` in the package). |

**Implications.** Epoch as a personal/non-commercial project may use Trellis
freely. Selling Epoch, using it inside a company, or shipping it as part of a
paid product requires a Trellis licence for as long as it is used. Because of
this the layout is isolated behind an adapter:
`apps/desktop/src/features/workspace/trellis/` is the only code that imports
it. Panels, presets, persistence and commands use Epoch's own types, and the
saved-layout envelope records the adapter, so a layout from another adapter
is never misread. Open-source replacements to evaluate if needed:
`dockview` (MIT), `flexlayout-react` (ISC), `rc-dock` (Apache-2.0); none
offers Trellis's zoomable nesting.

**Found while integrating:** the React package's type declarations say
`export * from '@danfessler/trellis'`, but its JavaScript re-exports only
`createDocument`, `layout` and `formatCombo`. `sanitize` (and other core
functions) must be imported from `@danfessler/trellis` directly, or the build
fails. Worth reporting upstream.

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
