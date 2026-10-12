#!/usr/bin/env node
// `pnpm tauri:dev`: run the desktop app in development without compiling
// DuckDB from source.
//
// libduckdb-sys downloads DuckDB's official prebuilt library for the pinned
// version (DUCKDB_DOWNLOAD_LIB=1) and copies it next to the test binaries in
// target/debug/deps. Linux and macOS find it through the rpath the build
// script sets; Windows looks next to the .exe and on PATH, so the deps
// folder goes on PATH for the app process this command starts.
//
// Release builds compile DuckDB in instead: `--features bundled-duckdb` (CI).
import { spawn } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const deps = join(root, "target", "debug", "deps");
const sep = process.platform === "win32" ? ";" : ":";

const env = {
  ...process.env,
  DUCKDB_DOWNLOAD_LIB: "1",
  PATH: `${deps}${sep}${process.env.PATH ?? ""}`,
  // Keep the machine responsive while the first build runs.
  CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS ?? "4",
};

const child = spawn("pnpm", ["--dir", join(root, "apps", "desktop"), "tauri", "dev", ...process.argv.slice(2)], {
  env,
  stdio: "inherit",
  shell: process.platform === "win32",
});
child.on("exit", (code) => process.exit(code ?? 1));
