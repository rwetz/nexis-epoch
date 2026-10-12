# Nexis Epoch — notes for coding agents

Read `docs/ARCHITECTURE.md` first; it is short.

- **UI rules** come from @nexis/design: read
  `apps/desktop/node_modules/@nexis/design/AGENTS.md` (tokens only, one accent,
  `<Icon name>` only, no emoji, every action in the command palette).
- **Only `apps/desktop/src/features/workspace/trellis/` may import Trellis.**
  Panels use `usePanel()`; new panels follow docs/DEVELOPMENT.md "Adding a panel".
- **Logic goes in the crates**, not in `src-tauri/src/commands` (thin adapters).
  Keep crates free of Tauri/React dependencies.
- **Never run project code** as a side effect of opening or reading a project.
- Changing a Rust type returned by a command → update `src/types/backend.ts`
  in the same change. Changing the saved-layout shape → bump
  `LAYOUT_SCHEMA` and add a migration + test.
- Verify: `pnpm check` (typecheck, vitest, fmt, clippy, cargo test). The data
  crate needs `DUCKDB_DOWNLOAD_LIB=1`. Don't compile DuckDB locally
  (`bundled-duckdb` is for CI); the first Tauri build is slow, so say so
  before starting one.
