import { describe, expect, it } from "vitest";
import type { EventPage, JobEvent, JobInfo, JobRecord } from "@epoch/types/backend";
import { applyPage, describeStatus, emptyLive, jobForRun, jobProgressLabel, MAX_LIVE_LINES, runDirKey, runRequestFor, trainConfigFor } from "./live";

let seq = 0;
function rec(event: JobEvent): JobRecord {
  seq += 1;
  return { seq, atMs: 1000 + seq, event };
}

function page(records: JobRecord[], extra: Partial<EventPage> = {}): EventPage {
  return { records, next: records.at(-1)?.seq ?? 0, dropped: 0, done: false, ...extra };
}

const metric = (name: string, step: number | null, value: number, epoch: number | null = null): JobEvent => ({
  type: "protocol",
  event: { ev: "metric", run: "r", step, epoch, name, value },
});

describe("applyPage", () => {
  it("turns streams, status and protocol logs into lines", () => {
    const live = applyPage(
      emptyLive(),
      page([
        rec({ type: "status", state: "running", pid: 4 }),
        rec({ type: "stdout", line: "hello" }),
        rec({ type: "stderr", line: "warning: x" }),
        rec({ type: "protocol", event: { ev: "epoch", run: "r", epoch: 2, of: 5 } }),
        rec({ type: "protocol", event: { ev: "log", run: "r", level: "warn", msg: "slow" } }),
      ]),
    );
    expect(live.lines.map((l) => [l.stream, l.level, l.text])).toEqual([
      ["job", "job", "Running"],
      ["out", "out", "hello"],
      ["err", "err", "warning: x"],
      ["log", "warn", "slow"],
    ]);
    expect(live.lines[3].epoch).toBe(2);
    expect(live.epoch).toBe(2);
    expect(live.cursor).toBe(seq);
  });

  it("builds metric series like the run store reads metrics.jsonl", () => {
    let live = applyPage(emptyLive(), page([rec(metric("loss", 1, 0.5, 1)), rec(metric("loss", 2, 0.25, 1)), rec(metric("acc", null, 0.7))]));
    live = applyPage(live, page([rec(metric("acc", null, 0.8)), rec(metric("loss", 3, Number.NaN))]));
    expect(live.series.loss).toEqual([
      [1, 0.5],
      [2, 0.25],
    ]);
    // No step: the point's index.
    expect(live.series.acc).toEqual([
      [0, 0.7],
      [1, 0.8],
    ]);
  });

  it("does not mutate the previous state", () => {
    const first = applyPage(emptyLive(), page([rec(metric("loss", 1, 1))]));
    const second = applyPage(first, page([rec(metric("loss", 2, 2)), rec({ type: "stdout", line: "x" })]));
    expect(first.series.loss).toHaveLength(1);
    expect(first.lines).toHaveLength(0);
    expect(second.series.loss).toHaveLength(2);
  });

  it("keeps the newest lines and counts what was dropped", () => {
    const many = Array.from({ length: MAX_LIVE_LINES + 5 }, (_, i) => rec({ type: "stdout", line: String(i) }));
    const live = applyPage(emptyLive(), page(many, { dropped: 3 }));
    expect(live.lines).toHaveLength(MAX_LIVE_LINES);
    expect(live.lines[0].text).toBe("5");
    expect(live.dropped).toBe(8);
  });

  it("marks a finished job done, even on an empty page", () => {
    const live = applyPage(emptyLive(), page([], { next: 7, done: true }));
    expect(live.done).toBe(true);
    expect(live.cursor).toBe(7);
  });
});

describe("statuses and runs", () => {
  it("describes how a job ended", () => {
    expect(describeStatus({ state: "exited", code: 0 })).toEqual({ label: "Finished", tone: "success" });
    expect(describeStatus({ state: "exited", code: 2 }).tone).toBe("danger");
    expect(describeStatus({ state: "cancelled", forced: true }).label).toBe("Stopped (killed)");
  });

  it("matches a run directory however it is spelled", () => {
    expect(runDirKey("C:\\Proj\\.nexis-ml\\runs\\r1\\")).toBe(runDirKey("c:/proj/.nexis-ml/runs/r1"));
    expect(runDirKey("/home/A/runs/r1")).not.toBe(runDirKey("/home/a/runs/r1"));
    const job = { progress: { runDir: "C:\\p\\.nexis-ml\\runs\\r1" } } as JobInfo;
    expect(jobForRun([job], "c:/p/.nexis-ml/runs/r1")).toBe(job);
    expect(jobForRun([job], "c:/p/.nexis-ml/runs/r2")).toBeUndefined();
  });

  it("labels progress by epoch, else by time", () => {
    const base = {
      status: { state: "running", pid: 1 },
      startedAtMs: 0,
      progress: { epoch: null, totalEpochs: null },
    } as unknown as JobInfo;
    expect(jobProgressLabel(base, 125_000)).toBe("2m 05s");
    expect(jobProgressLabel({ ...base, progress: { ...base.progress, epoch: 3, totalEpochs: 10 } }, 0)).toBe("epoch 3/10");
    expect(jobProgressLabel({ ...base, status: { state: "exited", code: 0 } }, 0)).toBe("Finished");
  });
});

describe("runRequestFor", () => {
  it("runs train.toml with Axon and .py files with Python, nothing else", () => {
    expect(runRequestFor("models/tab/train.toml")).toEqual({ kind: "axon", config: "models/tab/train.toml" });
    expect(runRequestFor("scripts/eval.py")).toEqual({ kind: "python_script", script: "scripts/eval.py", args: "" });
    expect(runRequestFor("pyproject.toml")).toBeNull();
    expect(runRequestFor("notes/train.toml.bak")).toBeNull();
    expect(runRequestFor("data.csv")).toBeNull();
  });
});

describe("trainConfigFor", () => {
  it("prefers the focused run's project, else the first config", () => {
    const configs = ["a/train.toml", "b/train.toml"];
    expect(trainConfigFor("C:\\p", configs, "C:\\p\\b")).toBe("b/train.toml");
    expect(trainConfigFor("C:\\p", configs, "C:\\elsewhere")).toBe("a/train.toml");
    expect(runRequestFor("models\\tab\\train.toml")).toEqual({ kind: "axon", config: "models\\tab\\train.toml" });
    expect(trainConfigFor("/p", ["train.toml"], "/p")).toBe("train.toml");
    expect(trainConfigFor("/p", [], undefined)).toBeUndefined();
  });
});
