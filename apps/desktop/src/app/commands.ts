/**
 * Every action in the app, for the command palette (Mod+Shift+P) — plus
 * the project's files for search (Mod+P).
 */
import * as React from "react";
import { fmtShortcut, listBuiltinThemes, MOD_KEY, type PaletteCommand, SHIFT_KEY, toast, useTheme } from "@nexis/design";
import { PANELS } from "@/features/panels";
import {
  chooseAndOpenProject,
  closeProject,
  openProject,
  rescanProject,
} from "@/features/projects/actions";
import { openFile } from "@/features/projects/ProjectExplorerPanel";
import { useExperiments } from "@/features/experiments/store";
import { WORKSPACES } from "@/features/workspace/presets";
import { useWorkspacePrefs } from "@/features/workspace/prefs";
import { api } from "@/lib/api";
import { fileIcon } from "@/lib/fileIcons";
import { BACKEND_AVAILABLE, errorMessage } from "@/lib/ipc";
import type { IndexedFile } from "@/types/backend";
import { openPanel, useApp, workspaceApi } from "./store";

export function useCommands(): PaletteCommand[] {
  const project = useApp((s) => s.project);
  const recent = useApp((s) => s.recent);
  const goTo = useApp((s) => s.goTo);
  const toggleSidebar = useApp((s) => s.toggleSidebar);
  const theme = useTheme();
  const navigation = useWorkspacePrefs((s) => s.navigation);
  const setNavigation = useWorkspacePrefs((s) => s.setNavigation);

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
      { id: "ws.overview", label: "Show the whole workspace", group: "Workspace", icon: "zoom-out", run: () => workspaceApi()?.overview() },
      { id: "ws.restore", label: "Restore hidden panels", group: "Workspace", icon: "visible", run: () => workspaceApi()?.restoreHidden() },
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
        id: "ws.navigation",
        label: navigation === "free" ? "Use focus navigation (double-click to zoom)" : "Use free navigation (pinch and gesture zoom)",
        group: "Workspace",
        icon: "map",
        run: () => setNavigation(navigation === "free" ? "focus" : "free"),
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
  }, [project, recent, goTo, toggleSidebar, theme, navigation, setNavigation]);
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
