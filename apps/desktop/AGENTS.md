# Epoch desktop app

The Tauri app for Nexis Epoch. Repository-level guidance is in `../../AGENTS.md`;
UI rules are @nexis/design's AGENTS.md (`node_modules/@nexis/design/AGENTS.md`).

- `src/app/` — shell (title bar, sidebar, status bar), pages, palette commands, app store
- `src/features/<domain>/` — panels; `src/features/workspace/` — the layout system
- `src/components/` — DataGrid, charts, CodeEditor, loading/empty/error states
- `src/lib/api.ts` — one typed function per Tauri command
- `src-tauri/src/` — thin commands over the `epoch-*` crates
