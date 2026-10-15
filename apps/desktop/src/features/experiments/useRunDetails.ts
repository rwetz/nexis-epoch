import * as React from "react";
import type { RunDetail } from "@epoch/types/backend";
import { useExperiments } from "./store";

export type RunView = {
  runDir: string;
  /** Categorical colour slot: the run's position in the selection. */
  slot: number;
  state: { status: "loading" } | { status: "ready"; detail: RunDetail } | { status: "error"; error: unknown };
};

/**
 * The runs a panel shows: the one it is pinned to, or the current
 * selection. Details load on demand and are cached in the store.
 */
export function useRunViews(pinned: string | undefined, mode: "all" | "focused"): RunView[] {
  const selected = useExperiments((s) => s.selected);
  const details = useExperiments((s) => s.details);
  const loadDetail = useExperiments((s) => s.loadDetail);
  const dirs = pinned ? [pinned] : mode === "focused" ? selected.slice(0, 1) : selected;
  const key = dirs.join("\n");
  React.useEffect(() => {
    for (const d of dirs) loadDetail(d);
  }, [key, loadDetail]); // eslint-disable-line react-hooks/exhaustive-deps
  return dirs.map((runDir) => ({
    runDir,
    slot: Math.max(0, selected.indexOf(runDir)),
    state: details[runDir] ?? { status: "loading" },
  }));
}
