/**
 * Every action in the app, for the command palette (Mod+Shift+P) — plus
 * the project's files for search (Mod+P).
 */
import * as React from "react";
import { fmtShortcut, listBuiltinThemes, MOD_KEY, type PaletteCommand, SHIFT_KEY, toast, useTheme } from "@nexis/design";
import { PANELS } from "@epoch/features/panels";
import {
  chooseAndOpenProject,
  closeProject,
  openProject,
  rescanProject,
} from "@epoch/features/projects/actions";
import { openFile } from "@epoch/features/projects/ProjectExplorerPanel";
import { useExperiments } from "@epoch/features/experiments/store";
import { showJob, stopJob } from "@epoch/features/jobs/JobsStatus";
import { isActive } from "@epoch/features/jobs/live";
import { requestRun } from "@epoch/features/jobs/RunDialog";
import { useJobs } from "@epoch/features/jobs/store";
import { benchmarkInPulse, NEXIS_TOOLS, openInSynapse } from "@epoch/features/links/links";
import { WORKSPACES } from "@epoch/features/workspace/presets";
import { api } from "@epoch/lib/api";
import { fileIcon } from "@epoch/lib/fileIcons";
import { BACKEND_AVAILABLE, errorMessage } from "@epoch/lib/ipc";
import type { IndexedFile } from "@epoch/types/backend";
import { openPanel, useApp, workspaceApi } from "./store";

export function useCommands(): PaletteCommand[] {
  const project = useApp((s) => s.project);
  const recent = useApp((s) => s.recent);
  const goTo = useApp((s) => s.goTo);
  const toggleSidebar = useApp((s) => s.toggleSidebar);
  const theme = useTheme();
  const jobs = useJobs((s) => s.jobs);

  return React.useMemo<PaletteCommand[]>(() => {
    const cmds: PaletteCommand[] = [
      { id: "project.open", label: "Open project folder…", group: "Project", icon: "folder-open", shortcut: fmtShortcut(MOD_KEY, "O"), run: () => void chooseAndOpenProject() },
      { id: "project.rescan", label: "Rescan project", group: "Project", icon: "refresh", disabled: !project, run: () => void rescanProject() },
      { id: "project.close", label: "Close project", group: "Project", icon: "close", disabled: !project, run: () => void closeProject() },
      ...recent
        .filter((r) => r.exists && r.path !== project?.root)
        .slice(0, 6)
        .map<PaletteCommand>((r) => ({
          id: `project.recent.${r.path}`,
          label: `Open ${r.name}`,
          group: "Recent projects",
          icon: "clock",
          keywords: [r.path],
          run: () => void openProject(r.path),
        })),
      { id: "go.overview", label: "Overview", group: "Go to", icon: "home", run: () => goTo({ kind: "page", page: "overview" }) },
      { id: "go.projects", label: "Projects", group: "Go to", icon: "folder-open", run: () => goTo({ kind: "page", page: "projects" }) },
      ...WORKSPACES.map<PaletteCommand>((w) => ({
        id: `go.ws.${w.id}`,
        label: `${w.name} workspace`,
        group: "Go to",
        icon: w.icon,
        keywords: [w.description],
        run: () => goTo({ kind: "workspace", workspace: w.id }),
      })),
      { id: "go.settings", label: "Settings", group: "Go to", icon: "settings", shortcut: fmtShortcut(MOD_KEY, ","), run: () => goTo({ kind: "page", page: "settings" }) },
      ...PANELS.map<PaletteCommand>((p) => ({
        id: `panel.${p.type}`,
        label: `Open ${p.title} panel`,
        group: "Panels",
        icon: p.icon,
        keywords: [p.description, p.type],
        run: () => openPanel(p.type, {}, { reuse: p.singleton ? "type" : "none" }),
      })),
      { id: "ws.zoom", label: "Zoom focused panel", group: "Workspace", icon: "zoom-in", shortcut: fmtShortcut(MOD_KEY, SHIFT_KEY, "Enter"), run: () => workspaceApi()?.toggleZoom() },
      { id: "ws.overview", label: "Leave zoom", group: "Workspace", icon: "zoom-out", run: () => workspaceApi()?.overview() },
      { id: "ws.float", label: "Float focused panel", group: "Workspace", icon: "external", keywords: ["undock", "window"], run: () => workspaceApi()?.floatFocused() },
      { id: "ws.close", label: "Close focused panel", group: "Workspace", icon: "close", run: () => workspaceApi()?.closeFocused() },
      {
        id: "ws.reset",
        label: "Reset layout",
        group: "Workspace",
        icon: "revert",
        run: async () => {
          const api = workspaceApi();
          if (api && (await api.resetLayout())) toast.success("Layout reset");
        },
      },
      {
        id: "experiments.reindex",
        label: "Re-index experiments",
        group: "Experiments",
        icon: "refresh",
        disabled: !project,
        run: async () => {
          try {
            toast.success("Experiments re-indexed", { description: await useExperiments.getState().reindex() });
          } catch (e) {
            toast.error("Re-index failed", { description: errorMessage(e) });
          }
        },
      },
      // Running things: each opens the confirmation with the exact command.
      ...(project?.trainConfigs ?? []).map<PaletteCommand>((config) => ({
        id: `run.axon.${config}`,
        label: `Train ${config} with Axon…`,
        group: "Run",
        icon: "play",
        keywords: ["run", "train", "axon", "nexis-ml", config],
        run: () => requestRun({ kind: "axon", config }, "Command palette"),
      })),
      ...jobs.flatMap<PaletteCommand>((job) => [
        {
          id: `job.show.${job.id}`,
          label: `Show output: ${job.title}`,
          group: "Run",
          icon: "terminal",
          keywords: ["job", "logs", "output"],
          run: () => showJob(job),
        },
        ...(isActive(job.status)
          ? [
              {
                id: `job.stop.${job.id}`,
                label: `Stop ${job.title}`,
                group: "Run",
                icon: "stop" as const,
                keywords: ["job", "cancel", "kill"],
                run: () => stopJob(job),
              },
            ]
          : []),
      ]),
      ...(project?.experimentStores ?? []).flatMap<PaletteCommand>((store) => {
        const name = store.projectDir.split(/[\\/]/).pop() ?? store.projectDir;
        return [
          {
            id: `nexis.synapse.${store.projectDir}`,
            label: `Open ${name} in ${NEXIS_TOOLS.mlLab}`,
            group: "Nexis",
            icon: "brain",
            keywords: ["nexis", "synapse", "ml lab", "train", store.projectDir],
            run: () => {
              const focused = useExperiments.getState().selected[0];
              const run = useExperiments.getState().runs.find((r) => r.runDir === focused && r.projectDir === store.projectDir);
              void openInSynapse(store.projectDir, run?.runId);
            },
          },
          {
            id: `nexis.pulse.${store.projectDir}`,
            label: `Benchmark ${name}'s model in ${NEXIS_TOOLS.benchmark}`,
            group: "Nexis",
            icon: "activity",
            keywords: ["nexis", "pulse", "benchmark", "onnx", "latency", store.projectDir],
            run: () => void benchmarkInPulse(store.projectDir),
          },
        ];
      }),
      { id: "view.sidebar", label: "Toggle sidebar", group: "View", icon: "sidebar-left", shortcut: fmtShortcut(MOD_KEY, "B"), run: toggleSidebar },
      {
        id: "view.mode",
        label: "Toggle light / dark",
        group: "View",
        icon: "contrast",
        keywords: ["theme"],
        run: () => theme.setMode(theme.resolvedMode === "dark" ? "light" : "dark"),
      },
      {
        id: "view.contrast",
        label: theme.highContrast ? "Standard contrast" : "High contrast",
        group: "View",
        icon: "contrast",
        run: () => theme.setContrast(theme.highContrast ? "standard" : "high"),
      },
      ...listBuiltinThemes().map<PaletteCommand>((t) => ({
        id: `theme.${t.id}`,
        label: `Theme: ${t.name}`,
        group: "Themes",
        icon: "theme",
        run: () => theme.setThemeId(t.id),
      })),
    ];
    return cmds;
  }, [project, recent, goTo, toggleSidebar, theme, jobs]);
}

/** Files in the open project, as palette entries (loaded on demand). */
export function useFileCommands(enabled: boolean): PaletteCommand[] {
  const project = useApp((s) => s.project);
  const [files, setFiles] = React.useState<IndexedFile[]>([]);
  React.useEffect(() => setFiles([]), [project?.root]);
  React.useEffect(() => {
    if (!enabled || !project || !BACKEND_AVAILABLE || files.length > 0) return;
    api.projectFiles().then(
      (index) => {
        setFiles(index.files);
        if (index.truncated) toast("Search covers the first 20,000 files");
      },
      (e: unknown) => toast.error("Couldn't index files", { description: errorMessage(e) }),
    );
  }, [enabled, project, files.length]);
  return React.useMemo(
    () =>
      files.map<PaletteCommand>((f) => ({
        id: `file.${f.path}`,
        label: f.path,
        group: "Files",
        icon: fileIcon(f.kind),
        run: () => openFile(f),
      })),
    [files],
  );
}
