/**
 * Runs in the open project, which ones are selected, and their details.
 *
 * The selection is shared by the experiment-aware panels: Training Metrics
 * overlays every selected run; Execution Logs and Model Inspector follow the
 * first (the "focused" run) unless pinned to one by their params.
 */
import { create } from "zustand";
import { api } from "@/lib/api";
import type { IndexedRun, RunDetail } from "@/types/backend";

/** Most runs overlaid at once: one per categorical colour slot. */
export const MAX_SELECTED = 5;

type DetailState = { status: "loading" } | { status: "ready"; detail: RunDetail } | { status: "error"; error: unknown };

interface ExperimentStore {
  runs: IndexedRun[];
  status: "idle" | "loading" | "ready" | "error";
  error: unknown;
  selected: string[];
  details: Record<string, DetailState>;

  reload(): Promise<void>;
  reindex(): Promise<string>;
  select(runDir: string, mode: "replace" | "toggle"): void;
  loadDetail(runDir: string, force?: boolean): void;
  reset(): void;
}

export const useExperiments = create<ExperimentStore>((set, get) => ({
  runs: [],
  status: "idle",
  error: null,
  selected: [],
  details: {},

  async reload() {
    set({ status: "loading", error: null });
    try {
      const runs = await api.experimentsList();
      const known = new Set(runs.map((r) => r.runDir));
      set((s) => ({
        runs,
        status: "ready",
        selected: s.selected.filter((d) => known.has(d)),
      }));
    } catch (error) {
      set({ status: "error", error });
    }
  },

  async reindex() {
    const stats = await api.experimentsIndex();
    // Files may have changed; drop cached details.
    set({ details: {} });
    await get().reload();
    const parts = [`${stats.runs} run${stats.runs === 1 ? "" : "s"}`];
    if (stats.updated) parts.push(`${stats.updated} updated`);
    if (stats.removed) parts.push(`${stats.removed} removed`);
    if (stats.errors.length) parts.push(`${stats.errors.length} store${stats.errors.length === 1 ? "" : "s"} unreadable`);
    return parts.join(" · ");
  },

  select(runDir, mode) {
    set((s) => {
      if (mode === "replace") return { selected: [runDir] };
      if (s.selected.includes(runDir)) return { selected: s.selected.filter((d) => d !== runDir) };
      if (s.selected.length >= MAX_SELECTED) return s;
      return { selected: [...s.selected, runDir] };
    });
  },

  loadDetail(runDir, force = false) {
    const current = get().details[runDir];
    if (current && !force && current.status !== "error") return;
    set((s) => ({ details: { ...s.details, [runDir]: { status: "loading" } } }));
    api.experimentDetail(runDir).then(
      (detail) => set((s) => ({ details: { ...s.details, [runDir]: { status: "ready", detail } } })),
      (error: unknown) => set((s) => ({ details: { ...s.details, [runDir]: { status: "error", error } } })),
    );
  },

  reset() {
    set({ runs: [], status: "idle", error: null, selected: [], details: {} });
  },
}));

/** A run's short display name: the run id without its timestamp prefix. */
export function runLabel(run: Pick<IndexedRun, "runId">): string {
  const m = /^\d{4}-\d{2}-\d{2}-\d{4}-(.+)$/.exec(run.runId);
  return m ? `${m[1]} · ${run.runId.slice(5, 10)} ${run.runId.slice(11, 13)}:${run.runId.slice(13, 15)}` : run.runId;
}
