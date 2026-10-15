/**
 * The dockview adapter: the only module that imports dockview.
 *
 * It maps registered panel definitions to dockview components, presets to
 * dockview documents (document.ts), and dockview's panel API to Epoch's
 * `PanelHandle`. Swapping the layout library means rewriting this folder
 * and nothing else (docs/ARCHITECTURE.md, "Workspace").
 *
 * dockview (MIT) — https://dockview.dev. Only its free core is used; the
 * `dockview-enterprise` modules (auto-hide edges, keyboard docking, undo
 * history, pinned/multi-row tabs, smart guides) are not installed.
 */
import * as React from "react";
import {
  type DockviewApi,
  type DockviewPanelApi,
  DockviewReact,
  type DockviewReadyEvent,
  type DockviewTheme,
  type IDockviewPanelHeaderProps,
  type IDockviewPanelProps,
  type SerializedDockview,
} from "dockview-react";
import "dockview-react/dist/styles/dockview.css";
import { cn, Icon, useTheme } from "@nexis/design";
import { PanelErrorBoundary } from "@epoch/components/PanelErrorBoundary";
import { confirmAction } from "@epoch/components/confirm";
import { useApp } from "@epoch/app/store";
import { getPanel, listPanels } from "../registry";
import { PanelContext } from "../panel-context";
import type { PanelDefinition, PanelHandle, WorkspaceApi, WorkspacePreset } from "../types";
import { PANEL_MIN, presetDocument } from "./document";

/**
 * Panel ids from presets are only unique within a workspace (Studio and
 * Config each have a `toml-editor-1`), so app-wide state about a panel —
 * unsaved changes, close guards — is keyed by workspace and panel.
 */
const WorkspaceIdContext = React.createContext<string>("");
export function panelKey(workspace: string, panelId: string): string {
  return `${workspace}:${panelId}`;
}

/** Close guards by panel key, consulted by every close path. */
const guards = new Map<string, () => boolean | Promise<boolean>>();

async function requestClose(workspace: string, api: DockviewPanelApi): Promise<boolean> {
  const guard = guards.get(panelKey(workspace, api.id));
  if (guard && !(await guard())) return false;
  api.close();
  return true;
}

function useDockviewEvent<T>(subscribe: (listener: (e: T) => void) => { dispose(): void }, listener: (e: T) => void) {
  const latest = React.useRef(listener);
  latest.current = listener;
  React.useEffect(() => {
    const d = subscribe((e) => latest.current(e));
    return () => d.dispose();
  }, [subscribe]);
}

function PanelBridge({ def, props }: { def: PanelDefinition; props: IDockviewPanelProps }) {
  const { api } = props;
  const key = panelKey(React.useContext(WorkspaceIdContext), api.id);
  const setDirtyGlobal = useApp((s) => s.setDirty);
  const [visible, setVisible] = React.useState(api.isVisible);
  const [focused, setFocused] = React.useState(api.isActive);
  const [rawParams, setRawParams] = React.useState<object>(() => props.params ?? {});
  useDockviewEvent(api.onDidVisibilityChange, (e) => setVisible(e.isVisible));
  useDockviewEvent(api.onDidActiveChange, (e) => setFocused(e.isActive));
  useDockviewEvent(api.onDidParametersChange, (p) => setRawParams({ ...(p ?? {}) }));

  const params = React.useMemo(() => def.parseParams(rawParams) as object, [def, rawParams]);

  // The tab says what the panel shows (a file name, a run).
  React.useEffect(() => {
    api.setTitle(def.tabTitle?.(params as never) ?? def.title);
  }, [api, def, params]);

  React.useEffect(
    () => () => {
      guards.delete(key);
      setDirtyGlobal(key, false);
    },
    [key, setDirtyGlobal],
  );

  const handle = React.useMemo<PanelHandle<object>>(
    () => ({
      id: api.id,
      type: def.type,
      params,
      setParams: (patch) => api.updateParameters({ ...api.getParameters(), ...patch }),
      setDirty: (d) => setDirtyGlobal(key, d),
      setCloseGuard: (g) => {
        if (g) guards.set(key, g);
        else guards.delete(key);
      },
      visible,
      focused,
    }),
    [api, key, def, params, visible, focused, setDirtyGlobal],
  );

  const Component = def.component;
  return (
    <PanelContext.Provider value={handle}>
      <div className="nexis-dock-panel">
        <PanelErrorBoundary name={def.title}>
          <Component />
        </PanelErrorBoundary>
      </div>
    </PanelContext.Provider>
  );
}

/** A tab: the panel's icon and title, a dot for unsaved work, a guarded close. */
function EpochTab({ api }: IDockviewPanelHeaderProps) {
  const workspace = React.useContext(WorkspaceIdContext);
  const def = getPanel(api.component);
  const [title, setTitle] = React.useState(api.title ?? def?.title ?? "");
  useDockviewEvent(api.onDidTitleChange, (e) => setTitle(e.title));
  const dirty = useApp((s) => !!s.dirty[panelKey(workspace, api.id)]);
  return (
    <div className="nexis-dock-tab" title={title} onAuxClick={(e) => e.button === 1 && void requestClose(workspace, api)}>
      {def && <Icon name={def.icon} size="xs" className="nexis-dock-tab-icon" />}
      <span className="truncate">{title}</span>
      <button
        type="button"
        aria-label={dirty ? `${title} has unsaved changes. Close` : `Close ${title}`}
        className={cn("nexis-dock-tab-close", dirty && "is-dirty")}
        // Keep a press on the button from starting a tab drag.
        onPointerDown={(e) => e.stopPropagation()}
        onClick={(e) => {
          e.stopPropagation();
          void requestClose(workspace, api);
        }}
      >
        <span aria-hidden className="nexis-dock-tab-dot" />
        <Icon name="close" size={10} className="nexis-dock-tab-x" />
      </button>
    </div>
  );
}

export type DockviewWorkspaceProps = {
  preset: WorkspacePreset;
  initialDocument: SerializedDockview;
  onDocumentChange(doc: SerializedDockview): void;
  onReady(api: WorkspaceApi | null): void;
};

let nextPanel = 0;

export function DockviewWorkspace({ preset, initialDocument, onDocumentChange, onReady }: DockviewWorkspaceProps) {
  const { resolvedMode } = useTheme();
  const changeRef = React.useRef(onDocumentChange);
  changeRef.current = onDocumentChange;

  const components = React.useMemo(
    () =>
      Object.fromEntries(
        listPanels().map((def) => [def.type, (props: IDockviewPanelProps) => <PanelBridge def={def} props={props} />]),
      ),
    [],
  );

  const theme = React.useMemo<DockviewTheme>(
    () => ({ name: "epoch", className: "dockview-theme-nexis", colorScheme: resolvedMode, gap: 5 }),
    [resolvedMode],
  );

  const apiRef = React.useRef<DockviewApi | null>(null);
  const mounted = React.useRef(false);
  const attached = React.useRef<{ dispose(): void } | null>(null);

  const makeWorkspaceApi = (api: DockviewApi): WorkspaceApi => {
    const dirtyHere = () => {
      const dirty = useApp.getState().dirty;
      return api.panels.filter((p) => dirty[panelKey(preset.id, p.id)]);
    };
    return {
      id: preset.id,
      openPanel(type, params, options) {
        const def = getPanel(type);
        if (!def) return;
        const parsed = def.parseParams(params ?? {}) as Record<string, unknown>;
        const reuse = options?.reuse ?? (def.singleton ? "type" : "params");
        const existing = api.panels.find(
          (p) =>
            p.api.component === type &&
            (reuse === "type" || (reuse === "params" && JSON.stringify(def.parseParams(p.params ?? {})) === JSON.stringify(parsed))),
        );
        if (existing && reuse !== "none") {
          existing.api.setActive();
          return;
        }
        const id = `${type}-${Date.now().toString(36)}-${++nextPanel}`;
        const placement = options?.placement ?? def.placement;
        const base = { id, component: type, title: def.tabTitle?.(parsed as never) ?? def.title, params: parsed, ...PANEL_MIN };
        if (placement === "float") {
          api.addPanel({ ...base, floating: true });
          return;
        }
        // Documents join a group that already holds documents; tools open
        // beside the active group.
        const holdsDocs = (g: { panels: { api: { component: string } }[] }) =>
          g.panels.some((p) => getPanel(p.api.component)?.placement === "stage");
        const docGroup =
          placement === "stage" ? (api.activeGroup && holdsDocs(api.activeGroup) ? api.activeGroup : api.groups.find(holdsDocs)) : undefined;
        if (docGroup) api.addPanel({ ...base, position: { referenceGroup: docGroup } });
        else if (api.activeGroup)
          api.addPanel({ ...base, position: { referenceGroup: api.activeGroup, direction: placement === "stage" ? "within" : "right" } });
        else api.addPanel(base);
      },
      async resetLayout() {
        const unsaved = dirtyHere();
        if (unsaved.length > 0) {
          const ok = await confirmAction({
            title: "Reset the layout?",
            description: `${unsaved.map((p) => p.title ?? p.id).join(", ")} ${unsaved.length === 1 ? "has" : "have"} unsaved changes that will be lost.`,
            confirmLabel: "Discard and reset",
            destructive: true,
          });
          if (!ok) return false;
        }
        api.fromJSON(presetDocument(preset));
        changeRef.current(api.toJSON());
        return true;
      },
      toggleZoom() {
        if (api.hasMaximizedGroup()) api.exitMaximizedGroup();
        else if (api.activePanel) api.maximizeGroup(api.activePanel);
      },
      overview() {
        if (api.hasMaximizedGroup()) api.exitMaximizedGroup();
      },
      floatFocused() {
        if (api.activePanel) api.addFloatingGroup(api.activePanel);
      },
      closeFocused() {
        if (api.activePanel) void requestClose(preset.id, api.activePanel.api);
      },
    };
  };

  // Subscribe and hand the shell its API once both dockview and this
  // component are mounted — in either order, and again after React's
  // development-mode remount, which keeps the dockview instance.
  const attach = () => {
    const api = apiRef.current;
    if (!api || !mounted.current || attached.current) return;
    attached.current = api.onDidLayoutChange(() => changeRef.current(api.toJSON()));
    onReady(makeWorkspaceApi(api));
  };

  React.useEffect(() => {
    mounted.current = true;
    attach();
    return () => {
      mounted.current = false;
      attached.current?.dispose();
      attached.current = null;
      onReady(null);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const ready = React.useCallback(
    (event: DockviewReadyEvent) => {
      apiRef.current = event.api;
      // Development only: lets an automated test drive the layout directly.
      if (import.meta.env.DEV) {
        const w = window as unknown as { __epochDockview?: Record<string, DockviewApi> };
        w.__epochDockview = { ...w.__epochDockview, [preset.id]: event.api };
      }
      try {
        event.api.fromJSON(initialDocument);
      } catch (e) {
        console.error("saved layout could not be restored; using the default", e);
        event.api.fromJSON(presetDocument(preset));
      }
      attach();
    },
    // Read once, when dockview is created.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [],
  );

  return (
    <WorkspaceIdContext.Provider value={preset.id}>
    <DockviewReact
      className="h-full w-full"
      components={components}
      defaultTabComponent={EpochTab}
      onReady={ready}
      theme={theme}
      // Keep every panel's DOM alive when it is a hidden tab, so editors,
      // grids and charts don't lose state (or scroll) on a tab switch.
      defaultRenderer="always"
    />
    </WorkspaceIdContext.Provider>
  );
}
