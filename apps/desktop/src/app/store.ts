/**
 * App-wide state: where the user is, the open project, settings, and the
 * mounted workspaces. Panel-specific state lives in the panels (and
 * survives rearranging, because the layout never remounts them).
 */
import { create } from "zustand";
import type { AppSettings, ProjectSummary, RecentProject } from "@/types/backend";
import type { OpenOptions, PanelParams, PanelTypeId, WorkspaceApi, WorkspaceId } from "@/features/workspace/types";

export type PageId = "overview" | "projects" | "pipelines" | "settings";
export type Location = { kind: "page"; page: PageId } | { kind: "workspace"; workspace: WorkspaceId };

type PendingOpen = (api: WorkspaceApi) => void;

interface AppStore {
  location: Location;
  /** Workspaces opened at least once this session (kept mounted). */
  mounted: WorkspaceId[];
  lastWorkspace: WorkspaceId;
  sidebarCollapsed: boolean;

  project: ProjectSummary | null;
  projectBusy: boolean;
  recent: RecentProject[];

  settings: AppSettings | null;
  settingsWarnings: string[];

  workspaceApis: Partial<Record<WorkspaceId, WorkspaceApi>>;
  pending: Partial<Record<WorkspaceId, PendingOpen[]>>;
  /** Panels with unsaved changes, by panel id. */
  dirty: Record<string, true>;

  goTo(location: Location): void;
  toggleSidebar(): void;
  registerWorkspace(api: WorkspaceApi | null, id: WorkspaceId): void;
  setDirty(panelId: string, dirty: boolean): void;
}

export const useApp = create<AppStore>((set, get) => ({
  location: { kind: "page", page: "overview" },
  mounted: [],
  lastWorkspace: "studio",
  sidebarCollapsed: false,
  project: null,
  projectBusy: false,
  recent: [],
  settings: null,
  settingsWarnings: [],
  workspaceApis: {},
  pending: {},
  dirty: {},

  goTo(location) {
    if (location.kind === "workspace") {
      const mounted = get().mounted.includes(location.workspace) ? get().mounted : [...get().mounted, location.workspace];
      set({ location, mounted, lastWorkspace: location.workspace });
    } else {
      set({ location });
    }
  },

  toggleSidebar() {
    set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed }));
  },

  registerWorkspace(api, id) {
    set((s) => ({ workspaceApis: { ...s.workspaceApis, [id]: api ?? undefined } }));
    if (!api) return;
    const queued = get().pending[id];
    if (queued?.length) {
      set((s) => ({ pending: { ...s.pending, [id]: [] } }));
      for (const run of queued) run(api);
    }
  },

  setDirty(panelId, dirty) {
    set((s) => {
      if (dirty === !!s.dirty[panelId]) return s;
      const next = { ...s.dirty };
      if (dirty) next[panelId] = true;
      else delete next[panelId];
      return { dirty: next };
    });
  },
}));

/** The workspace the user is in, or the last one they were in. */
export function currentWorkspace(): WorkspaceId {
  const { location, lastWorkspace } = useApp.getState();
  return location.kind === "workspace" ? location.workspace : lastWorkspace;
}

/**
 * Open a panel in a workspace (default: the current one), switching to it
 * and waiting for it to mount if needed.
 */
export function openPanel<T extends PanelTypeId>(
  type: T,
  params?: PanelParams<T>,
  options?: OpenOptions & { workspace?: WorkspaceId },
): void {
  const id = options?.workspace ?? currentWorkspace();
  const store = useApp.getState();
  store.goTo({ kind: "workspace", workspace: id });
  const run: PendingOpen = (api) => api.openPanel(type, params, options);
  const api = useApp.getState().workspaceApis[id];
  if (api) run(api);
  else useApp.setState((s) => ({ pending: { ...s.pending, [id]: [...(s.pending[id] ?? []), run] } }));
}

export function workspaceApi(id: WorkspaceId = currentWorkspace()): WorkspaceApi | undefined {
  return useApp.getState().workspaceApis[id];
}
