/**
 * A running job as the panels see it: its output lines and metric curves,
 * built up from the event pages the backend hands out (`job_events`).
 * Pure functions, so the folding rules are tested without a backend.
 */
import type { EventPage, JobInfo, JobRequest, JobStatus } from "@epoch/types/backend";

/** Lines kept per job in the UI (the oldest go first). */
export const MAX_LIVE_LINES = 20_000;
/** Points kept per metric; beyond this a series is thinned to every other point. */
export const MAX_LIVE_POINTS = 20_000;

export type LiveLine = {
  seq: number;
  atMs: number;
  /** out/err: the process's streams; log: a protocol log event; job: status and run notices. */
  stream: "out" | "err" | "log" | "job";
  level: string;
  text: string;
  epoch: number | null;
};

export type LiveSample = { epoch: number | null; output: string };

export type LiveJob = {
  /** Pass back to `job_events` as `after`. */
  cursor: number;
  lines: LiveLine[];
  series: Record<string, [number, number][]>;
  samples: LiveSample[];
  /** Events the backend dropped before we read them, plus lines we let go. */
  dropped: number;
  /** Every event of a finished job has been read. */
  done: boolean;
  /** The last epoch the protocol reported. */
  epoch: number | null;
};

export function emptyLive(): LiveJob {
  return { cursor: 0, lines: [], series: {}, samples: [], dropped: 0, done: false, epoch: null };
}

export function isActive(status: JobStatus): boolean {
  return !["exited", "cancelled", "failed"].includes(status.state);
}

export type Tone = "neutral" | "brand" | "success" | "warning" | "info" | "danger";

/** A status in words, with the tone its dot takes. */
export function describeStatus(status: JobStatus): { label: string; tone: Tone } {
  switch (status.state) {
    case "queued":
      return { label: "Queued", tone: "neutral" };
    case "starting":
      return { label: "Starting", tone: "info" };
    case "running":
      return { label: "Running", tone: "brand" };
    case "paused":
      return { label: "Paused", tone: "warning" };
    case "stopping":
      return { label: "Stopping", tone: "warning" };
    case "exited":
      return status.code === 0
        ? { label: "Finished", tone: "success" }
        : { label: status.code === null ? "Ended by a signal" : `Exited with code ${status.code}`, tone: "danger" };
    case "cancelled":
      return { label: status.forced ? "Stopped (killed)" : "Stopped", tone: "neutral" };
    case "failed":
      return { label: `Failed: ${status.message}`, tone: "danger" };
  }
}

function sampleText(output: unknown): string {
  if (typeof output === "string") return output;
  if (output === null || output === undefined) return "";
  return JSON.stringify(output);
}

/** Keep every other point (and the last), so a long run stays drawable. */
function thin(points: [number, number][]): [number, number][] {
  const last = points[points.length - 1];
  const out = points.filter((_, i) => i % 2 === 0);
  if (out[out.length - 1] !== last) out.push(last);
  return out;
}

/** Fold one page of events into a job's live state. Returns a new object. */
export function applyPage(live: LiveJob, page: EventPage): LiveJob {
  if (page.records.length === 0 && page.dropped === 0) {
    return page.done === live.done && page.next === live.cursor ? live : { ...live, cursor: page.next, done: page.done };
  }
  const lines = [...live.lines];
  const series: Record<string, [number, number][]> = { ...live.series };
  const samples = [...live.samples];
  let epoch = live.epoch;
  const touched = new Set<string>();
  const line = (seq: number, atMs: number, stream: LiveLine["stream"], level: string, text: string) =>
    lines.push({ seq, atMs, stream, level, text, epoch });

  for (const { seq, atMs, event } of page.records) {
    switch (event.type) {
      case "stdout":
        line(seq, atMs, "out", "out", event.line);
        break;
      case "stderr":
        line(seq, atMs, "err", "err", event.line);
        break;
      case "status": {
        const { type: _type, ...status } = event;
        line(seq, atMs, "job", "job", describeStatus(status as JobStatus).label);
        break;
      }
      case "protocol": {
        const ev = event.event;
        switch (ev.ev) {
          case "metric": {
            if (!Number.isFinite(ev.value)) break;
            if (ev.epoch !== null) epoch = Math.max(epoch ?? ev.epoch, ev.epoch);
            if (!touched.has(ev.name)) {
              series[ev.name] = [...(series[ev.name] ?? [])];
              touched.add(ev.name);
            }
            const points = series[ev.name];
            // As the run store reads metrics.jsonl: x is the step, else the
            // point's index.
            points.push([ev.step ?? points.length, ev.value]);
            if (points.length > MAX_LIVE_POINTS) series[ev.name] = thin(points);
            break;
          }
          case "epoch":
            epoch = ev.epoch;
            break;
          case "log":
            line(seq, atMs, "log", ev.level ?? "info", ev.msg);
            break;
          case "sample":
            samples.push({ epoch, output: sampleText(ev.output) });
            break;
          case "run.started":
            line(seq, atMs, "job", "run", `Run ${ev.run} started${ev.device ? ` on ${ev.device}` : ""}`);
            break;
          case "run.finished":
            line(seq, atMs, "job", "run", `Run ${ev.run} ${ev.status}`);
            break;
          case "artifact":
            line(seq, atMs, "job", "run", `Saved ${ev.kind}: ${ev.path}`);
            break;
        }
        break;
      }
    }
  }
  let dropped = live.dropped + page.dropped;
  if (lines.length > MAX_LIVE_LINES) {
    const excess = lines.length - MAX_LIVE_LINES;
    lines.splice(0, excess);
    dropped += excess;
  }
  return { cursor: page.next, lines, series, samples, dropped, done: page.done, epoch };
}

/**
 * A run directory in a form two spellings of the same folder share:
 * forward slashes, no trailing slash, and on Windows drive paths, case
 * folded.
 */
export function runDirKey(path: string): string {
  const slashes = path.replace(/\\/g, "/").replace(/\/+$/, "");
  return /^[a-z]:\//i.test(slashes) ? slashes.toLowerCase() : slashes;
}

/** The job writing a run, if any. */
export function jobForRun(jobs: readonly JobInfo[], runDir: string): JobInfo | undefined {
  const key = runDirKey(runDir);
  return jobs.find((j) => j.progress.runDir !== null && runDirKey(j.progress.runDir) === key);
}

/** "epoch 3/10", "3 min", or the exit — what a job's row says after its title. */
export function jobProgressLabel(job: JobInfo, nowMs: number): string {
  if (!isActive(job.status)) return describeStatus(job.status).label;
  const { epoch, totalEpochs } = job.progress;
  if (epoch !== null) return totalEpochs ? `epoch ${epoch}/${totalEpochs}` : `epoch ${epoch}`;
  if (job.startedAtMs === null) return describeStatus(job.status).label;
  return formatElapsed(nowMs - job.startedAtMs);
}

export function formatElapsed(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ${String(s % 60).padStart(2, "0")}s`;
  return `${Math.floor(m / 60)}h ${String(m % 60).padStart(2, "0")}m`;
}

/** What "Run" means for a project file: Axon for a train.toml, Python for a .py; else nothing. */
export function runRequestFor(path: string): JobRequest | null {
  const name = path.split(/[\\/]/).pop() ?? path;
  if (name === "train.toml") return { kind: "axon", config: path };
  if (/\.py$/i.test(name)) return { kind: "python_script", script: path, args: "" };
  return null;
}

/**
 * The train.toml to train from the Experiments panel: the one in the
 * focused run's ML project when there is one, else the project's first.
 * `configs` are relative to `root`; `projectDir` is absolute.
 */
export function trainConfigFor(root: string, configs: readonly string[], projectDir: string | undefined): string | undefined {
  if (projectDir) {
    const want = runDirKey(projectDir);
    const match = configs.find((c) => runDirKey(`${root}/${c.replace(/\/?train\.toml$/, "")}`) === want);
    if (match) return match;
  }
  return configs[0];
}
