/**
 * Jobs started from Epoch (training with Axon, Python scripts): the list,
 * each job's live output and curves, and which job the Logs panel follows.
 *
 * The backend keeps every job's events with sequence numbers; this store
 * polls `jobs_list` and `job_events` with a cursor while anything is
 * running, so a reloaded window (or Nexis's Epoch window opened later)
 * picks up where things are. Polling stops when every job is finished and
 * read.
 */
import { create } from "zustand";
import { toast } from "@nexis/design";
import { useApp } from "@epoch/app/store";
import { useExperiments } from "@epoch/features/experiments/store";
import { api } from "@epoch/lib/api";
import { BACKEND_AVAILABLE } from "@epoch/lib/ipc";
import type { JobInfo } from "@epoch/types/backend";
import { applyPage, describeStatus, emptyLive, isActive, type LiveJob, runDirKey } from "./live";

const POLL_MS = 700;
const PAGE = 2_000;
/** Pages read per job per poll, so a chatty job can't starve the others. */
const PAGES_PER_POLL = 4;

interface JobsStore {
  jobs: JobInfo[];
  live: Record<number, LiveJob>;
  /** The job the Logs panel follows, until a run is selected instead. */
  focused: number | null;
  /** Jobs started from this window: their runs get selected, their ends announced. */
  mine: number[];

  refresh(): Promise<void>;
  focus(id: number | null): void;
  started(job: JobInfo): void;
}

export const useJobs = create<JobsStore>((set, get) => ({
  jobs: [],
  live: {},
  focused: null,
  mine: [],

  async refresh() {
    const previous = get().jobs;
    const jobs = await api.jobsList();
    const live = { ...get().live };
    for (const job of jobs) {
      let state = live[job.id] ?? emptyLive();
      for (let i = 0; i < PAGES_PER_POLL && !state.done; i++) {
        const page = await api.jobEvents(job.id, state.cursor, PAGE);
        state = applyPage(state, page);
        if (page.records.length < PAGE) break;
      }
      live[job.id] = state;
    }
    // Forget what the backend forgot.
    const known = new Set(jobs.map((j) => j.id));
    for (const id of Object.keys(live).map(Number)) if (!known.has(id)) delete live[id];
    set((s) => ({ jobs, live, focused: s.focused !== null && known.has(s.focused) ? s.focused : null }));
    react(previous, jobs);
  },

  focus(id) {
    set({ focused: id });
  },

  started(job) {
    set((s) => ({ jobs: [job, ...s.jobs.filter((j) => j.id !== job.id)], focused: job.id, mine: [...s.mine, job.id] }));
    schedule();
  },
}));

/** What changed since the last poll that the rest of the app should hear. */
function react(previous: JobInfo[], jobs: JobInfo[]) {
  const before = new Map(previous.map((j) => [j.id, j]));
  const { mine } = useJobs.getState();
  for (const job of jobs) {
    const old = before.get(job.id);
    const runDir = job.progress.runDir;
    // A run started: the backend has indexed it (and detected a new run
    // store) before reporting it, so list it, and select the ones started
    // here so Metrics and Logs follow them.
    if (runDir && !old?.progress.runDir) {
      void refreshProjectSummary();
      void useExperiments
        .getState()
        .reload()
        .then(() => {
          if (!mine.includes(job.id)) return;
          const run = useExperiments.getState().runs.find((r) => runDirKey(r.runDir) === runDirKey(runDir));
          if (run) useExperiments.getState().select(run.runDir, "replace");
        });
    }
    if (old && isActive(old.status) && !isActive(job.status)) {
      // The run's files now hold its final state.
      if (runDir) {
        const exp = useExperiments.getState();
        void exp.reload().then(() => {
          const run = useExperiments.getState().runs.find((r) => runDirKey(r.runDir) === runDirKey(runDir));
          if (run && useExperiments.getState().details[run.runDir]) useExperiments.getState().loadDetail(run.runDir, true);
        });
      }
      if (mine.includes(job.id)) announce(job);
    }
  }
}

function announce(job: JobInfo) {
  const { label, tone } = describeStatus(job.status);
  if (tone === "success") toast.success(`${job.title} finished`);
  else if (tone === "danger") toast.error(`${job.title}: ${label}`);
  else toast(`${job.title}: ${label}`);
}

async function refreshProjectSummary() {
  try {
    const summary = await api.projectCurrent();
    if (summary && summary.root === useApp.getState().project?.root) useApp.setState({ project: summary });
  } catch {
    // The next rescan will catch up.
  }
}

let timer: ReturnType<typeof setTimeout> | null = null;

function needsPolling(): boolean {
  const { jobs, live } = useJobs.getState();
  return jobs.some((j) => isActive(j.status) || !live[j.id]?.done);
}

/** Poll until every job is finished and read. Safe to call repeatedly. */
export function schedule(): void {
  if (!BACKEND_AVAILABLE || timer !== null) return;
  timer = setTimeout(() => {
    void useJobs
      .getState()
      .refresh()
      .catch(() => {})
      .finally(() => {
        timer = null;
        if (needsPolling()) schedule();
      });
  }, POLL_MS);
}

/** Once at startup: jobs may already be running (a reloaded window, or Nexis). */
export function bootstrapJobs(): void {
  if (!BACKEND_AVAILABLE) return;
  void useJobs
    .getState()
    .refresh()
    .then(() => needsPolling() && schedule())
    .catch(() => {});
}

// Selecting a run in Experiments hands Logs back to the selection, unless
// the run is the focused job's own.
useExperiments.subscribe((s, prev) => {
  if (s.selected === prev.selected) return;
  const { focused, jobs } = useJobs.getState();
  if (focused === null) return;
  const job = jobs.find((j) => j.id === focused);
  const runDir = job?.progress.runDir;
  if (!runDir || !s.selected[0] || runDirKey(s.selected[0]) !== runDirKey(runDir)) useJobs.setState({ focused: null });
});

/** The job a Logs panel shows when it follows the selection. */
export function useFocusedJob(): JobInfo | undefined {
  const focused = useJobs((s) => s.focused);
  const jobs = useJobs((s) => s.jobs);
  return focused === null ? undefined : jobs.find((j) => j.id === focused);
}
