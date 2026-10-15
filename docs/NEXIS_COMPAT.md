# Nexis ML Lab compatibility

Epoch imports experiments produced by the Nexis ML Lab engines
(`nexis-ml`, Python/PyTorch, and `nexis-ml-rs`, Rust/burn) **without
modifying their files**. Compatibility is by data format, not by sharing
frontend code with Nexis.

## Sources checked

| Source | What it says |
|---|---|
| Nexis `docs/ML_LAB_GUIDE.md`, `docs/ML_SUITE.md` | Canonical protocol (v1) and run-store layout |
| Nexis `src/modules/ml/lib/{protocol,projects,notes,toml-edit}.ts` | How the panel actually parses events, lists runs, reads `notes.json`, edits `train.toml` |
| `rwetz/nexis-ml` `PROTOCOL.md`, `src/nexis_ml/run_store.py` | Python engine's writer: atomic JSON, append-only `metrics.jsonl`, run id allocation |
| Real stores: `my-first-model` (18 runs, nexis-ml-rs), `stock-ml/stock-predict` | What the files contain in practice |

## The format, as implemented in `epoch-core::nexis_ml`

```
<ml-project>/train.toml                       the project marker (Rust projects have no train.py)
<ml-project>/.nexis-ml/runs/<run-id>/
  config.json     resolved hyperparameters; nexis-ml-rs adds "engine" and "derived"
  metrics.jsonl   one protocol event per line (run.started, metric, epoch,
                  artifact, sample, log, run.finished)
  summary.json    status, startedAt/finishedAt, totalEpochs, lastEpoch, device,
                  metrics: { name: { last, min, max, count } }, artifacts
  checkpoints/    best.pt/last.pt (Python); best.json + best-weights.json (Rust)
  artifacts/      cm-epochN.json {labels, matrix}, weights-epochN.json, sample PNGs
  notes.json      { note, tags, pinned } — optional, written by Nexis
```

Run ids are `YYYY-MM-DD-HHMM-<slug>[-N]`, so name order is time order.
Unknown event types and fields are ignored (the protocol's forward-compat
rule); malformed lines are counted and skipped, never fatal.

## Where the docs and the files disagree

Verified against real data, as the brief asked:

1. **Artifact paths are absolute.** `ML_SUITE.md`'s example shows
   `.nexis-ml/runs/…/cm-epoch1.json`; the engines' `PROTOCOL.md` says
   absolute, and real files contain `C:\Users\…\artifacts\cm-epoch1.json`.
   Epoch keeps the part after `<run-id>/` and resolves it against the run's
   current location, so a store copied to another folder or machine still
   opens. A recorded path that would leave the run directory is refused.
2. **`notes.json` is usually absent.** Only Nexis writes it (on annotate or
   pin); runs read as unpinned with no note or tags.
3. **`engine` is recorded only by nexis-ml-rs.** Epoch shows "not recorded"
   rather than guessing the Python engine.
4. **A diverging run can write `NaN`.** The Python engine's `protocol.py`
   calls `json.dumps` without `allow_nan=False` or a finiteness check, and
   `NaN` is not JSON. Such lines are
   skipped by Epoch, as they are by the Nexis panel (`JSON.parse`); the
   skipped count is shown in the Logs panel.

## Where it lives in Epoch

| Concern | Code |
|---|---|
| Event parsing (shared with the future runtime) | `crates/epoch-core/src/nexis_ml/protocol.rs` |
| Listing runs, reading details, rebasing artifacts, decimation | `crates/epoch-core/src/nexis_ml/run_store.rs` |
| Finding run stores in a project (root + nested ML projects) | `crates/epoch-core/src/project.rs` |
| SQLite index (rebuildable from the files) | `crates/epoch-store/src/lib.rs` |
| A check against a real project | `crates/epoch-core/tests/real_run_store.rs` (`EPOCH_NEXIS_ML_PROJECT=…`) |

## Running Axon

Epoch starts the engine the way Synapse does (Nexis
`src/modules/ml/lib/engine-bridge.ts`, `src-tauri/src/modules/ml.rs`):
`<engine> --nexis-protocol train .` in the ML project's folder, with
`NEXIS_ML_PROTOCOL=1` and stdin piped for `{"cmd":"cancel|pause|resume"}`.
The binary is `axon` (Axon 0.9+) or `nexis-ml` (the Python engine and older
Axon). Epoch finds it by file, never by running `--version`: the project's
venvs, `PATH`, then the folder Nexis downloads it to
(`<local data>/app.nexis.nexis/engine`), so an engine installed from Synapse
is used by Epoch too. Epoch never downloads or installs an engine.

## Not yet

- **Writing `notes.json`.** Epoch reads notes/tags/pins; editing them would be
  Epoch's first write into a run store and is deliberately left for later.
- **`serve`/Playground, HTML report, ONNX export.** Nexis features with no
  Epoch equivalent yet.
- **The `weights` artifact** (per-epoch dense weights) is listed but not drawn;
  a network-graph view belongs with the Model Inspector's architecture work.

## Links with Nexis

Nexis names its ML tools **Synapse** (the ML Lab, tool id `ml-lab`) and
**Pulse** (Benchmark, tool id `benchmark`). Since 0.4.0 Nexis also has an
**Epoch window** that runs this repo's UI and backend (docs/ARCHITECTURE.md,
"Two hosts, one source"). Inside Nexis, links between the three are window
launch parameters, handled by the Epoch window's host (`project`, `run`,
`dataset`; Nexis `src/windows/epoch.tsx`). The table below is the
**standalone** app's channel, which still links Epoch and a running Nexis
both ways:

| From | Action | Link |
|---|---|---|
| Synapse | Open in Epoch / Inspect run in Epoch | `nexis-epoch://open?project=<dir>[&run=<id>]` |
| Pulse | Open in Epoch (models exported by Synapse) | `nexis-epoch://open?project=<dir>` |
| Epoch, Runs toolbar and palette | Open in Synapse | `nexis://ml-lab?project=<dir>[&run=<id>]` |
| Epoch, Runs toolbar and palette | Benchmark in Pulse | `nexis://benchmark?model=<dir>/model.onnx` |

The links don't use an OS URL scheme. Each running app listens on a
loopback port and writes `{port, token, pid}` to
`<local data dir>/nexis/links/<app>.json` (`epoch` or `nexis`). A sender
writes one JSON line, `{token, url}`. If the receiver isn't running, the
sender launches it with `--link <url>`. A launch carrying a link hands it to
the running copy and exits, so plain launches are unchanged.

Code: `apps/desktop/src-tauri/src/links.rs` (validation, channel,
`nexis_open`) and `apps/desktop/src/features/links/links.ts` (following a
link: open the project, focus the run in Experiments, or open the dataset in
Data). Nexis's half is `src-tauri/src/modules/links.rs` in the Nexis repo.
Keep the two in step.

Links only select. They need an absolute project path, and may add a run id
or a dataset path inside the project. Anything else is rejected.
