/**
 * The Trellis adapter: the only module that imports Trellis.
 *
 * It turns registered panel definitions into Trellis view types, neutral
 * layout presets into Trellis documents, and Trellis's view handles into
 * Epoch's `PanelHandle`. Swapping the docking library means rewriting this
 * folder and nothing else (docs/ARCHITECTURE.md, "Workspace").
 *
 * Licensing: Trellis is free for non-commercial use only; commercial use
 * needs a sponsorship or enterprise licence (docs/DEPENDENCIES.md).
 */
import * as React from "react";
import {
  createDocument,
  layout as L,
  type LayoutDocument,
  type LayoutSpec,
  useCloseGuard,
  useView,
  useViewBadge,
  ViewType,
  Workspace,
  type WorkspaceHandle,
} from "@danfessler/trellis-react";
// trellis-react's typings re-export the whole core, but its JavaScript only
// re-exports createDocument/layout/formatCombo; take the rest from the core.
import { sanitize } from "@danfessler/trellis";
import { Icon, useTheme } from "@nexis/design";
import { PanelErrorBoundary } from "@/components/PanelErrorBoundary";
import { confirmAction } from "@/components/confirm";
import { useApp } from "@/app/store";
import { getPanel, isPanelType, listPanels } from "../registry";
import { PanelContext } from "../panel-context";
import type { LayoutNode, PanelDefinition, PanelHandle, WorkspaceApi, WorkspacePreset } from "../types";

function toSpec(node: LayoutNode): LayoutSpec {
  switch (node.kind) {
    case "panel":
      return L.panel(
        { selected: node.selected ?? 0 },
        ...node.views.map((v) => L.view(v.type, { params: v.params ?? {}, ...(v.id ? { id: v.id } : {}) })),
      );
    case "stage":
      return L.stage(node.child ? toSpec(node.child) : undefined);
    case "split":
      return L.split(node.axis === "row" ? "x" : "y", node.children.map(toSpec), { weights: node.weights });
  }
}

/** The preset as a Trellis document. View ids are deterministic, so a
 * reset keeps panels that are in both layouts mounted. */
export function presetDocument(preset: WorkspacePreset): LayoutDocument {
  return createDocument(toSpec(preset.layout));
}

/** Repair a stored document: drop unknown panel types, re-validate params. */
export function repairDocument(raw: unknown): LayoutDocument | null {
  if (typeof raw !== "object" || raw === null) return null;
  try {
    const doc = sanitize(raw as LayoutDocument, isPanelType);
    for (const record of Object.values(doc.views)) {
      const def = getPanel(record.type);
      if (def) record.params = def.parseParams(record.params) as Record<string, unknown>;
    }
    return doc;
  } catch {
    return null;
  }
}

function PanelBridge({ def }: { def: PanelDefinition }) {
  const view = useView<object>();
  const setDirtyGlobal = useApp((s) => s.setDirty);
  const [dirty, setDirty] = React.useState(false);
  const guard = React.useRef<(() => boolean | Promise<boolean>) | null>(null);
  useViewBadge(dirty || null);
  useCloseGuard(() => (guard.current ? guard.current() : true));
  React.useEffect(() => () => setDirtyGlobal(view.id, false), [view.id, setDirtyGlobal]);

  const params = React.useMemo(() => def.parseParams(view.params) as object, [def, view.params]);
  const handle = React.useMemo<PanelHandle<object>>(
    () => ({
      id: view.id,
      type: def.type,
      params,
      setParams: (patch) => view.setParams(patch),
      setDirty: (d) => {
        setDirty(d);
        setDirtyGlobal(view.id, d);
      },
      setCloseGuard: (g) => {
        guard.current = g;
      },
      visible: view.visible,
      focused: view.focused,
    }),
    // `view` is a fresh object each render; depend on what we read from it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [view.id, params, view.visible, view.focused, def, setDirtyGlobal],
  );
  const Component = def.component;
  return (
    <PanelContext.Provider value={handle}>
      <div className="epoch-panel">
        <PanelErrorBoundary name={def.title}>
          <Component />
        </PanelErrorBoundary>
      </div>
    </PanelContext.Provider>
  );
}

export type TrellisWorkspaceProps = {
  preset: WorkspacePreset;
  initialDocument: LayoutDocument;
  onDocumentChange(doc: LayoutDocument): void;
  onReady(api: WorkspaceApi | null): void;
  navigation: "focus" | "free";
};

export function TrellisWorkspace({ preset, initialDocument, onDocumentChange, onReady, navigation }: TrellisWorkspaceProps) {
  const ref = React.useRef<WorkspaceHandle>(null);
  const [doc, setDoc] = React.useState(initialDocument);
  const { resolvedMode } = useTheme();
  const defs = React.useMemo(() => listPanels(), []);

  const change = React.useCallback(
    (next: LayoutDocument) => {
      setDoc(next);
      onDocumentChange(next);
    },
    [onDocumentChange],
  );

  // Hand the shell an API once Trellis has created the workspace.
  React.useEffect(() => {
    let cancelled = false;
    let tries = 0;
    const attach = () => {
      if (cancelled) return;
      const ws = ref.current;
      if (!ws) {
        if (tries++ < 50) setTimeout(attach, 20);
        return;
      }
      const dirtyHere = () => {
        const dirty = useApp.getState().dirty;
        return ws.views().filter((v) => dirty[v.id]);
      };
      onReady({
        id: preset.id,
        openPanel(type, params, options) {
          const def = getPanel(type);
          if (!def) return;
          ws.open(type, {
            params: def.parseParams(params ?? {}) as object,
            reuse: options?.reuse ?? (def.singleton ? "type" : "params"),
            placement: options?.placement ?? (def.placement === "stage" ? "stage" : "side"),
            focus: true,
          });
        },
        async resetLayout() {
          const unsaved = dirtyHere();
          if (unsaved.length > 0) {
            const ok = await confirmAction({
              title: "Reset the layout?",
              description: `${unsaved.map((v) => v.title).join(", ")} ${unsaved.length === 1 ? "has" : "have"} unsaved changes that will be lost.`,
              confirmLabel: "Discard and reset",
              destructive: true,
            });
            if (!ok) return false;
          }
          const fresh = presetDocument(preset);
          ws.setDocument(fresh);
          change(fresh);
          return true;
        },
        toggleZoom: () => void ws.navigation.toggle(),
        overview: () => ws.navigation.overview(),
        restoreHidden: () => ws.getSnapshot().hidden.forEach((h) => ws.restore(h.panelId)),
        closeFocused: () => {
          const id = ws.getSnapshot().focusedView;
          if (id) void ws.close(id);
        },
      });
    };
    attach();
    return () => {
      cancelled = true;
      onReady(null);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <Workspace
      ref={ref}
      theme={resolvedMode}
      navigation={navigation}
      floating="overlay"
      label={`${preset.name} workspace`}
      document={doc}
      onDocumentChange={change}
      onMissingType={() => "drop"}
      tabs={{ inset: 4 }}
    >
      {defs.map((def) => (
        <ViewType
          key={def.type}
          id={def.type}
          title={(v) => def.tabTitle?.(def.parseParams(v.params) as never) ?? def.title}
          icon={<Icon name={def.icon} size="xs" />}
          placement={def.placement === "stage" ? "stage" : "side"}
          singleton={def.singleton}
          render={() => <PanelBridge def={def} />}
        />
      ))}
    </Workspace>
  );
}
