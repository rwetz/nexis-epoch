import * as React from "react";
import { Button, Icon, toast, Tree, type TreeNode } from "@nexis/design";
import { PanelToolbar, ToolbarButton, ToolbarTitle } from "@epoch/components/PanelToolbar";
import { EmptyState, ErrorState, LoadingState } from "@epoch/components/states";
import { openPanel, useApp } from "@epoch/app/store";
import { requestRun } from "@epoch/features/jobs/RunDialog";
import { runRequestFor } from "@epoch/features/jobs/live";
import { definePanel, noParams } from "@epoch/features/workspace/registry";
import { api } from "@epoch/lib/api";
import { fileIcon, isTabular } from "@epoch/lib/fileIcons";
import { formatBytes } from "@epoch/lib/format";
import { errorMessage } from "@epoch/lib/ipc";
import type { DirEntry } from "@epoch/types/backend";
import { chooseAndOpenProject, openProject } from "./actions";

type DirState = { status: "loading" } | { status: "ready"; entries: DirEntry[]; truncated: boolean } | { status: "error"; error: string };

const LOADING_SUFFIX = "\u0000loading";

/** Open a project file in the panel that understands it. */
export function openFile(entry: Pick<DirEntry, "path" | "kind">): void {
  if (entry.kind === "toml") openPanel("toml-editor", { path: entry.path }, { reuse: "params" });
  else if (isTabular(entry.kind)) openPanel("dataset-explorer", { path: entry.path }, { reuse: "params" });
  else if (/\.(safetensors|onnx)$/i.test(entry.path)) openPanel("model-inspector", { path: entry.path }, { reuse: "params" });
  else if (entry.kind === "checkpoint")
    toast("Not read by Epoch", {
      description: `${entry.path}: PyTorch and pickle checkpoints can run code when loaded, so Epoch doesn't open them. Safetensors and ONNX files open in the Model panel.`,
    });
  else toast("No viewer for this file yet", { description: `${entry.path}: Epoch opens TOML, CSV, Parquet, JSON, Safetensors and ONNX files.` });
}

function ProjectExplorer() {
  const project = useApp((s) => s.project);
  const recent = useApp((s) => s.recent);
  const [dirs, setDirs] = React.useState<Record<string, DirState>>({});
  const [expanded, setExpanded] = React.useState<Set<string>>(new Set());
  const [selected, setSelected] = React.useState<string | null>(null);

  const load = React.useCallback(async (path: string) => {
    setDirs((d) => ({ ...d, [path]: { status: "loading" } }));
    try {
      const listing = await api.listDir(path);
      setDirs((d) => ({ ...d, [path]: { status: "ready", entries: listing.entries, truncated: listing.truncated } }));
    } catch (e) {
      setDirs((d) => ({ ...d, [path]: { status: "error", error: errorMessage(e) } }));
    }
  }, []);

  // A new project: start from its root.
  React.useEffect(() => {
    setDirs({});
    setExpanded(new Set());
    setSelected(null);
    if (project) void load("");
  }, [project?.root, load]); // eslint-disable-line react-hooks/exhaustive-deps

  const entriesByPath = React.useRef(new Map<string, DirEntry>());

  const nodes = React.useMemo(() => {
    entriesByPath.current.clear();
    const build = (path: string): TreeNode[] => {
      const state = dirs[path];
      if (!state || state.status === "loading") return [{ id: path + LOADING_SUFFIX, label: "Loading…", icon: "loading" }];
      if (state.status === "error") return [{ id: path + LOADING_SUFFIX, label: state.error, icon: "alert-circle" }];
      const out: TreeNode[] = state.entries.map((e) => {
        entriesByPath.current.set(e.path, e);
        const isDir = e.kind === "directory";
        return {
          id: e.path,
          label: e.name,
          icon: fileIcon(e.kind, expanded.has(e.path)),
          meta: isDir ? undefined : <span className="font-mono text-[10px] tabular-nums">{formatBytes(e.size)}</span>,
          // Folders show a chevron before their contents are known.
          children: isDir ? (dirs[e.path] ? build(e.path) : [{ id: e.path + LOADING_SUFFIX, label: "Loading…", icon: "loading" }]) : undefined,
        };
      });
      if (state.truncated) out.push({ id: path + "\u0000more", label: "More entries not shown", icon: "more" });
      return out;
    };
    return dirs[""] ? build("") : [];
  }, [dirs, expanded]);

  const onExpandedChange = (next: Set<string>) => {
    for (const id of next) if (!expanded.has(id) && !dirs[id]) void load(id);
    setExpanded(next);
  };

  const onSelect = (id: string) => {
    if (id.includes("\u0000")) return;
    setSelected(id);
    const entry = entriesByPath.current.get(id);
    if (!entry) return;
    if (entry.kind === "directory") {
      const next = new Set(expanded);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      onExpandedChange(next);
    } else {
      openFile(entry);
    }
  };

  if (!project) {
    return (
      <EmptyState
        icon="folder-open"
        title="No project open"
        description="Open any folder: a Python repo, a Nexis ML project, a directory of datasets. Nothing in it is run."
        action={
          <div className="flex w-full max-w-64 flex-col gap-1.5">
            <Button size="sm" variant="brand" onClick={() => void chooseAndOpenProject()}>
              <Icon name="folder-open" /> Open folder…
            </Button>
            {recent.slice(0, 4).filter((r) => r.exists).map((r) => (
              <Button key={r.path} size="sm" variant="ghost" className="justify-start" title={r.path} onClick={() => void openProject(r.path)}>
                <Icon name="clock" /> <span className="truncate">{r.name}</span>
              </Button>
            ))}
          </div>
        }
      />
    );
  }

  const root = dirs[""];
  const runnable = selected ? runRequestFor(selected) : null;
  return (
    <>
      <PanelToolbar>
        <ToolbarTitle icon="folder">{project.name}</ToolbarTitle>
        <span className="flex-1" />
        <ToolbarButton
          icon="play"
          label={
            runnable?.kind === "axon"
              ? "Train with Axon (shows the command first)"
              : runnable
                ? "Run this script (shows the command first)"
                : "Select a train.toml or a .py file to run it"
          }
          disabled={!runnable}
          onClick={() => runnable && requestRun(runnable, "Run button in the Project panel")}
        />
        <ToolbarButton icon="collapse" label="Collapse all" onClick={() => setExpanded(new Set())} />
        <ToolbarButton
          icon="refresh"
          label="Refresh"
          onClick={() => {
            const open = [...expanded];
            setDirs({});
            void load("");
            open.forEach((p) => void load(p));
          }}
        />
      </PanelToolbar>
      <div className="nexis-scrollbar min-h-0 flex-1 overflow-y-auto py-1">
        {!root || root.status === "loading" ? (
          <LoadingState />
        ) : root.status === "error" ? (
          <ErrorState error={root.error} onRetry={() => void load("")} />
        ) : root.entries.length === 0 ? (
          <EmptyState icon="folder" title="Empty folder" />
        ) : (
          <Tree label="Project files" nodes={nodes} expanded={expanded} onExpandedChange={onExpandedChange} selected={selected} onSelect={onSelect} />
        )}
      </div>
    </>
  );
}

export const projectExplorerPanel = definePanel({
  type: "project-explorer",
  title: "Project",
  icon: "explorer",
  description: "Browse the open project's files. Opens TOML and datasets in their panels.",
  component: ProjectExplorer,
  parseParams: noParams,
  placement: "side",
  singleton: true,
  maturity: "ready",
});
