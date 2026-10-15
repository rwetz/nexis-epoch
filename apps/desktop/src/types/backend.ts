/**
 * TypeScript mirrors of the Rust types the Tauri commands return. Field
 * names follow the Rust structs' `serde(rename_all = "camelCase")`.
 *
 * Hand-maintained: when a Rust type changes, change it here in the same
 * commit (docs/ARCHITECTURE.md, "Frontend/backend contract").
 */

export type FileKind =
  | "directory"
  | "toml"
  | "python"
  | "notebook"
  | "csv"
  | "parquet"
  | "json"
  | "jsonl"
  | "sqlite"
  | "checkpoint"
  | "image"
  | "markdown"
  | "text"
  | "other";

export type DirEntry = {
  name: string;
  path: string;
  kind: FileKind;
  size: number;
  modifiedMs: number | null;
  hidden: boolean;
};

export type DirListing = { path: string; entries: DirEntry[]; truncated: boolean };

export type TextFile = {
  path: string;
  content: string;
  hash: string;
  size: number;
  modifiedMs: number | null;
};

export type PythonEnv = { path: string; interpreter: string | null; version: string | null };
export type DatasetRef = { path: string; kind: FileKind; size: number };
export type ExperimentStoreRef = {
  projectDir: string;
  runsDir: string;
  format: string;
  runCount: number;
};

export type ProjectSummary = {
  root: string;
  name: string;
  packageName: string | null;
  hasPyproject: boolean;
  requirements: string[];
  trainConfigs: string[];
  tomlFiles: number;
  pythonFiles: number;
  notebooks: number;
  environments: PythonEnv[];
  datasets: DatasetRef[];
  checkpointDirs: string[];
  experimentStores: ExperimentStoreRef[];
  truncated: boolean;
};

export type IndexStats = {
  stores: number;
  runs: number;
  updated: number;
  removed: number;
  errors: string[];
};

export type Opened = { summary: ProjectSummary; index: IndexStats | null };

export type RecentProject = { path: string; name: string; lastOpenedMs: number; exists: boolean };

export type IndexedFile = { path: string; kind: FileKind };
export type FileIndex = { files: IndexedFile[]; truncated: boolean };

// ── TOML ────────────────────────────────────────────────────────────────

export type PathSeg = string | number;

export type ScalarValue =
  | { type: "string"; value: string }
  | { type: "integer"; value: number }
  | { type: "float"; value: number | null }
  | { type: "boolean"; value: boolean }
  | { type: "datetime"; value: string };

export type TomlNodeKind = "table" | "inline_table" | "array" | "array_of_tables" | "scalar";

export type TomlNode = {
  label: string;
  path: PathSeg[];
  kind: TomlNodeKind;
  value: ScalarValue | null;
  line: number | null;
  children: TomlNode[];
};

export type Diagnostic = {
  message: string;
  line: number;
  column: number;
  endLine: number;
  endColumn: number;
};

export type ParseReport = { ok: boolean; diagnostics: Diagnostic[]; tree: TomlNode[] };

// ── Data ────────────────────────────────────────────────────────────────

export type Column = { name: string; dataType: string };
export type CellValue = string | number | boolean | null | CellValue[] | { [k: string]: CellValue };

export type DatasetInfo = {
  path: string;
  kind: FileKind;
  size: number;
  columns: Column[];
  rowCount: number;
};

export type FilterOp =
  | "eq"
  | "ne"
  | "lt"
  | "le"
  | "gt"
  | "ge"
  | "contains"
  | "starts_with"
  | "is_null"
  | "not_null";

export type Filter = { column: string; op: FilterOp; value?: string | null };
export type Sort = { column: string; desc: boolean };

export type PageRequest = {
  offset: number;
  limit: number;
  sort: Sort[];
  filters: Filter[];
  withTotal: boolean;
};

export type Page = { offset: number; columns: Column[]; rows: CellValue[][]; total: number | null };

export type QueryResult = {
  columns: Column[];
  rows: CellValue[][];
  truncated: boolean;
  elapsedMs: number;
};

// ── Experiments ─────────────────────────────────────────────────────────

export type MetricStats = {
  last: number | null;
  min: number | null;
  max: number | null;
  count: number | null;
};

export type IndexedRun = {
  runDir: string;
  projectDir: string;
  runId: string;
  format: string;
  engine: string | null;
  name: string | null;
  status: string;
  startedAt: string | null;
  finishedAt: string | null;
  device: string | null;
  totalEpochs: number | null;
  lastEpoch: number | null;
  metrics: Record<string, MetricStats>;
  note: string;
  tags: string[];
  pinned: boolean;
};

export type RunRecord = {
  id: string;
  dir: string;
  projectDir: string;
  format: string;
  engine: string | null;
  name: string | null;
  status: string;
  startedAt: string | null;
  finishedAt: string | null;
  device: string | null;
  totalEpochs: number | null;
  lastEpoch: number | null;
  metrics: Record<string, MetricStats>;
  notes: { note: string; tags: string[]; pinned: boolean };
  sourceModifiedMs: number;
};

export type MetricSeries = { name: string; points: [number, number][]; decimated: boolean };
export type LogLine = { level: string; msg: string; epoch: number | null };
export type SampleOutput = { epoch: number | null; output: string };
export type ArtifactInfo = {
  kind: string;
  path: string | null;
  recordedPath: string;
  epoch: number | null;
  exists: boolean;
};
export type FileInfo = { name: string; size: number };

export type RunDetail = {
  record: RunRecord;
  config: unknown;
  series: MetricSeries[];
  logs: LogLine[];
  samples: SampleOutput[];
  artifacts: ArtifactInfo[];
  checkpoints: FileInfo[];
  eventCount: number;
  skippedLines: number;
  logsTruncated: boolean;
};

// ── Settings / app ──────────────────────────────────────────────────────

export type AppSettings = {
  python: { interpreter: string };
  data: {
    pageSize: number;
    queryRowLimit: number;
    queryTimeoutSecs: number;
    memoryLimitMb: number;
    threads: number;
  };
  explorer: { showHidden: boolean };
  experiments: { indexOnOpen: boolean };
};

export type LoadedSettings = { settings: AppSettings; warnings: string[]; path: string };

export type AppInfo = { version: string; configDir: string; dataDir: string; settingsFile: string };

export type GpuStatus = {
  name: string;
  utilizationPercent: number | null;
  memoryUsedMb: number | null;
  memoryTotalMb: number | null;
};

export type SystemStatus = {
  cpuPercent: number;
  cpuCount: number;
  memoryUsedMb: number;
  memoryTotalMb: number;
  gpus: GpuStatus[];
  gpuAvailable: boolean;
  runningQueries: number;
};

// epoch-core::model_file

export type TensorInfo = {
  name: string;
  dtype: string;
  /** Numbers, symbolic names (`batch`) or `?`. */
  shape: string[];
  elements: number | null;
};

export type OnnxInfo = {
  irVersion: number | null;
  opsets: [string, number][];
  producer: string | null;
  graphName: string | null;
  inputs: TensorInfo[];
  outputs: TensorInfo[];
  nodeCount: number;
  ops: { op: string; count: number }[];
};

export type ModelFileInfo = {
  path: string;
  format: "safetensors" | "onnx";
  size: number;
  tensors: TensorInfo[];
  tensorCount: number;
  parameters: number;
  dtypes: [string, number][];
  metadata: Record<string, string>;
  onnx: OnnxInfo | null;
  truncated: boolean;
};

// ── Jobs (epoch-runtime, plugin commands/jobs.rs) ────────────────────────

/** What to run; paths are relative to the open project. */
export type JobRequest = { kind: "axon"; config: string } | { kind: "python_script"; script: string; args?: string };

export type CommandPlan = {
  program: string;
  args: string[];
  cwd: string;
  /** Set on top of the inherited environment. */
  env: [string, string][];
  speaksProtocol: boolean;
};

export type ProgramSource = "project_venv" | "path" | "nexis_managed" | "settings";
export type FoundProgram = { path: string; source: ProgramSource; legacy: boolean };

export type PlannedJob = {
  title: string;
  program: FoundProgram;
  alternatives: FoundProgram[];
  plan: CommandPlan;
  commandLine: string;
};

export type JobStatus =
  | { state: "queued" }
  | { state: "starting" }
  | { state: "running"; pid: number }
  | { state: "paused"; pid: number }
  | { state: "stopping"; pid: number }
  | { state: "exited"; code: number | null }
  | { state: "cancelled"; forced: boolean }
  | { state: "failed"; message: string };

/** epoch-core::nexis_ml::protocol::Event. Absent optionals arrive as null. */
export type ProtocolEvent =
  | {
      ev: "run.started";
      run: string;
      name: string | null;
      dir: string | null;
      config: unknown;
      totalEpochs: number | null;
      device: string | null;
      protocol: number | null;
      startedAt: string | null;
    }
  | { ev: "metric"; run: string; step: number | null; epoch: number | null; name: string; value: number }
  | { ev: "epoch"; run: string; epoch: number; of: number | null }
  | { ev: "artifact"; run: string; kind: string; path: string }
  | { ev: "sample"; run: string; input: unknown; output: unknown }
  | { ev: "log"; run: string; level: string | null; msg: string }
  | { ev: "run.finished"; run: string; status: string; summary: unknown };

export type JobEvent =
  | ({ type: "status" } & JobStatus)
  | { type: "stdout"; line: string }
  | { type: "stderr"; line: string }
  | { type: "protocol"; event: ProtocolEvent };

export type JobRecord = { seq: number; atMs: number; event: JobEvent };

export type EventPage = { records: JobRecord[]; next: number; dropped: number; done: boolean };

export type JobProgress = {
  runId: string | null;
  runDir: string | null;
  device: string | null;
  epoch: number | null;
  totalEpochs: number | null;
  outcome: string | null;
  metricEvents: number;
};

export type JobInfo = {
  id: number;
  title: string;
  /** The project root the job was started from. */
  scope: string;
  kind: "axon" | "python_script" | "process";
  plan: CommandPlan;
  consent: { action: string; atMs: number };
  status: JobStatus;
  /** Accepts pause/resume. */
  controllable: boolean;
  queuedAtMs: number;
  startedAtMs: number | null;
  finishedAtMs: number | null;
  progress: JobProgress;
  outputLines: number;
};
