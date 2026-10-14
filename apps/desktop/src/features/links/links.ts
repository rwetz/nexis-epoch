/**
 * Links between Epoch and Nexis's two ML tools: Synapse (trains and tries
 * models; Nexis's ML lab) and Pulse (times them; Nexis's benchmark).
 *
 * Inbound, from Synapse's "Inspect run in Epoch" and Pulse's model links:
 * `nexis-epoch://open?project=<dir>[&run=<id>][&dataset=<file>]`. The
 * backend validates every link (src-tauri/src/links.rs) and queues it; this
 * side opens the project, then focuses the run or the dataset.
 *
 * Outbound: `openInSynapse` and `benchmarkInPulse` hand Nexis a `nexis://`
 * link, through the running Nexis or by starting it.
 */
import * as React from "react";
import { listen } from "@tauri-apps/api/event";
import { toast } from "@nexis/design";
import { openPanel, useApp } from "@/app/store";
import { useExperiments } from "@/features/experiments/store";
import { openProject } from "@/features/projects/actions";
import { api } from "@/lib/api";
import { BACKEND_AVAILABLE, errorMessage } from "@/lib/ipc";

export type EpochLink = { project: string; run: string | null; dataset: string | null };

/** Nexis's names for its tools, as its title bar shows them. */
export const NEXIS_TOOLS = { mlLab: "Synapse", benchmark: "Pulse" } as const;

const norm = (p: string) => {
  const s = p.replace(/\\/g, "/").replace(/\/+$/, "");
  return /^[a-z]:\//i.test(s) ? s.toLowerCase() : s;
};

export const samePath = (a: string, b: string) => norm(a) === norm(b);

/** `file` relative to `root` with `/` separators, or null when outside it. */
export function relativeTo(root: string, file: string): string | null {
  const r = norm(root);
  const f = file.replace(/\\/g, "/");
  return norm(f).startsWith(`${r}/`) ? f.slice(r.length + 1) : null;
}

async function follow(link: EpochLink): Promise<void> {
  const current = useApp.getState().project;
  if (!current || !samePath(current.root, link.project)) {
    if (!(await openProject(link.project))) return;
  }
  const root = useApp.getState().project?.root ?? link.project;

  if (link.dataset) {
    const rel = relativeTo(root, link.dataset);
    if (rel) openPanel("dataset-explorer", { path: rel }, { reuse: "params", workspace: "data" });
    else toast.warning("Dataset is outside the project", { description: link.dataset });
  }

  if (link.run) {
    const experiments = useExperiments.getState();
    if (experiments.status !== "ready") await experiments.reload();
    const run = useExperiments
      .getState()
      .runs.find((r) => r.runId === link.run && samePath(r.projectDir, link.project));
    if (run) {
      useApp.getState().goTo({ kind: "workspace", workspace: "experiments" });
      useExperiments.getState().select(run.runDir, "replace");
    } else {
      toast.warning("Run not found", { description: `${link.run} isn't indexed yet — try Rescan project.` });
    }
  } else if (!link.dataset) {
    useApp.getState().goTo({ kind: "workspace", workspace: "experiments" });
  }
}

/** Follow links for the life of the app: one that launched Epoch, and any
 * that arrive while it runs. */
export function useNexisLinks(): void {
  React.useEffect(() => {
    if (!BACKEND_AVAILABLE) return;
    let queue = Promise.resolve();
    const drain = () => {
      void api
        .linksTakePending()
        .then((links) => {
          for (const link of links) queue = queue.then(() => follow(link)).catch(() => {});
        })
        .catch(() => {});
    };
    drain();
    const unlisten = listen("epoch://link", drain);
    return () => void unlisten.then((fn) => fn()).catch(() => {});
  }, []);
}

async function toNexis(args: Parameters<typeof api.nexisOpen>[0], what: string): Promise<void> {
  try {
    const how = await api.nexisOpen(args);
    if (how === "launched") toast(`Starting Nexis for ${what}…`);
  } catch (e) {
    const message = errorMessage(e);
    toast.error(message.includes("not-found") ? "Nexis isn't installed here" : `Couldn't open ${what}`, {
      description: message.includes("not-found")
        ? "Install Nexis, or set NEXIS_EXE to its path, to open runs in Synapse and models in Pulse."
        : message,
    });
  }
}

/** Show a Nexis ML project (and maybe one run) in Synapse. */
export function openInSynapse(project: string, run?: string): Promise<void> {
  return toNexis({ tool: "ml-lab", project, run }, NEXIS_TOOLS.mlLab);
}

/** Time a project's exported `model.onnx` in Pulse. */
export function benchmarkInPulse(project: string): Promise<void> {
  const sep = project.includes("\\") && !project.includes("/") ? "\\" : "/";
  return toNexis({ tool: "benchmark", model: `${project.replace(/[\\/]+$/, "")}${sep}model.onnx` }, NEXIS_TOOLS.benchmark);
}
