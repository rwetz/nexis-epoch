# Development

## Prerequisites

- Node 22+ and pnpm 11 (`corepack enable`)
- Rust stable (1.85+)
- Tauri 2 platform prerequisites: WebView2 (Windows, preinstalled on 11),
  `libwebkit2gtk-4.1-dev` and friends (Linux), Xcode CLT (macOS) —
  see https://v2.tauri.app/start/prerequisites/

## Commands

```bash
pnpm install              # also syncs the design system's cursors into public/
pnpm tauri:dev            # the desktop app, with DuckDB's prebuilt library
pnpm dev                  # frontend only, in a browser (no backend: panels say so)
pnpm typecheck            # tsc, strict
pnpm test                 # vitest: layout migrations, page cache, registry, formatting
pnpm build                # typecheck + production bundle
pnpm check                # all of the above + cargo fmt/clippy/test (library crates)
```

Rust on its own:

```bash
cargo test                                         # core, config, store, runtime
DUCKDB_DOWNLOAD_LIB=1 cargo test -p epoch-data     # DuckDB layer, prebuilt library
EPOCH_NEXIS_ML_PROJECT=/path/to/project cargo test -p epoch-core --test real_run_store
```

### Builds and your CPU

The first `pnpm tauri:dev` compiles Tauri and its dependencies (a few
minutes; later runs are incremental). It caps Cargo at 4 parallel jobs
(`CARGO_BUILD_JOBS`, override it if you like). DuckDB itself is never
compiled locally: `DUCKDB_DOWNLOAD_LIB=1` fetches DuckDB's official prebuilt
library for the pinned version from DuckDB's GitHub releases into
`target/duckdb-download/`.

Release builds compile DuckDB in (`--features bundled-duckdb`) so the app has
no runtime library to ship; CI does that (`.github/workflows/ci.yml`, job
`desktop`).

## Layout of the repo

```
apps/desktop/            the Tauri app (React frontend + src-tauri)
  src/app/               shell, pages, palette commands, app store
  src/features/          panels by domain; workspace/ is the layout system
  src/components/        grid, charts, editor, shared states
  src-tauri/src/         commands (thin) + state
crates/                  epoch-core, -config, -store, -data, -runtime
docs/                    architecture, compatibility, dependencies, roadmap
scripts/tauri-dev.mjs    dev launcher (DuckDB linking, job cap)
```

## Adding a panel

1. Add its id and params to `PanelParamsMap` in
   `src/features/workspace/types.ts`. Ids are stable: saved layouts use them.
2. Write the component; read params with `usePanel("<id>")`, mark unsaved
   work with `setDirty`, guard closing with `setCloseGuard`.
3. Export a `definePanel({...})` with `parseParams` that never throws.
4. List it in `src/features/panels.ts`. It appears in the palette
   ("Open … panel") and can be placed in presets (`presets.ts`).

## Changing the layout format

Bump `LAYOUT_SCHEMA` in `src/features/workspace/persistence.ts`, add a
migration from the previous schema to `MIGRATIONS`, and a test. Old layouts
upgrade on load; nothing is lost.

## Design rules

Epoch follows @nexis/design's AGENTS.md: colours only from tokens, one
accent (`brand`), state colours only for state, icons only via `<Icon>`, no
emoji, motion only from the house vocabulary, every action in the command
palette. Chart series use Epoch's validated `--series-*` tokens
(ARCHITECTURE.md, "Charts").
