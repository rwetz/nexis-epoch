/**
 * Epoch's neutral layouts ↔ dockview's serialized layout (`SerializedDockview`).
 *
 * Pure functions, no DOM: presets are turned into a dockview document
 * directly (so panel ids are deterministic and a reset is one `fromJSON`),
 * and documents loaded from disk are repaired before dockview sees them.
 */
import type { GroupviewPanelState, Orientation, SerializedDockview, SerializedGridObject } from "dockview-react";
import { getPanel } from "../registry";
import type { LayoutNode, LayoutView, WorkspacePreset } from "../types";

/** A group in the grid: its panels (tabs) and which one shows. dockview's
 * own type for this isn't exported; these are the fields Epoch uses. */
export type GroupPanelViewState = { id: string; views: string[]; activeView?: string };

/**
 * The smallest a panel may be dragged or squeezed to. Below this a tab
 * strip clips and an empty state stacks one word per line, which is what a
 * side column looked like on a small window.
 */
export const PANEL_MIN = { minimumWidth: 220, minimumHeight: 120 } as const;

/** dockview scales sizes to the real container; this is just the unit. */
const CANVAS = 1000;

type Split = Extract<LayoutNode, { kind: "split" }>;

/**
 * Drop stages (dockview has no "stage" region; every group can hold
 * documents) and flatten a split nested directly in a split on the same
 * axis, so axes alternate as dockview's grid requires.
 */
export function normalize(node: LayoutNode): LayoutNode | null {
  if (node.kind === "stage") return node.child ? normalize(node.child) : null;
  if (node.kind === "panel") return node.views.length ? node : null;
  const weights = node.weights ?? node.children.map(() => 1);
  const children: LayoutNode[] = [];
  const childWeights: number[] = [];
  node.children.forEach((raw, i) => {
    const child = normalize(raw);
    if (!child) return;
    const w = weights[i] ?? 1;
    if (child.kind === "split" && child.axis === node.axis) {
      const inner = child.weights ?? child.children.map(() => 1);
      const sum = inner.reduce((a, b) => a + b, 0) || 1;
      child.children.forEach((c, j) => {
        children.push(c);
        childWeights.push((w * (inner[j] ?? 1)) / sum);
      });
    } else {
      children.push(child);
      childWeights.push(w);
    }
  });
  if (children.length === 0) return null;
  if (children.length === 1) return children[0];
  return { kind: "split", axis: node.axis, children, weights: childWeights };
}

/** The preset as a dockview document. */
export function presetDocument(preset: WorkspacePreset): SerializedDockview {
  const panels: Record<string, GroupviewPanelState> = {};
  const perType = new Map<string, number>();
  let groups = 0;
  let firstGroup: string | undefined;

  const panelId = (v: LayoutView) => {
    if (v.id) return v.id;
    const n = (perType.get(v.type) ?? 0) + 1;
    perType.set(v.type, n);
    return `${v.type}-${n}`;
  };

  const leaf = (node: Extract<LayoutNode, { kind: "panel" }>, size: number): SerializedGridObject<GroupPanelViewState> => {
    const views = node.views.map((v) => {
      const id = panelId(v);
      const def = getPanel(v.type);
      const params = (def ? def.parseParams(v.params ?? {}) : (v.params ?? {})) as Record<string, unknown>;
      panels[id] = { id, contentComponent: v.type, title: def?.title ?? v.type, params, ...PANEL_MIN };
      return id;
    });
    const id = `group-${++groups}`;
    firstGroup ??= id;
    return { type: "leaf", size, data: { id, views, activeView: views[node.selected ?? 0] ?? views[0] } };
  };

  const build = (node: LayoutNode, rect: { w: number; h: number }, size: number): SerializedGridObject<GroupPanelViewState> => {
    if (node.kind === "panel") return leaf(node, size);
    if (node.kind === "stage") throw new Error("normalize() removes stages");
    const split = node as Split;
    const weights = split.weights ?? split.children.map(() => 1);
    const sum = weights.reduce((a, b) => a + b, 0) || 1;
    return {
      type: "branch",
      size,
      data: split.children.map((child, i) => {
        const f = (weights[i] ?? 1) / sum;
        return split.axis === "row"
          ? build(child, { w: rect.w * f, h: rect.h }, rect.w * f)
          : build(child, { w: rect.w, h: rect.h * f }, rect.h * f);
      }),
    };
  };

  const normalized = normalize(preset.layout);
  // The grid's root is always a branch; a lone panel becomes a one-group row.
  const root: Split =
    normalized === null
      ? { kind: "split", axis: "row", children: [] }
      : normalized.kind === "split"
        ? (normalized as Split)
        : { kind: "split", axis: "row", children: [normalized] };
  const orientation = (root.axis === "row" ? "HORIZONTAL" : "VERTICAL") as Orientation;
  const grid = build(root, { w: CANVAS, h: CANVAS }, CANVAS);
  return {
    grid: { root: grid as SerializedDockview["grid"]["root"], width: CANVAS, height: CANVAS, orientation },
    panels,
    activeGroup: firstGroup,
  };
}

function isObject(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

/**
 * Repair a document from disk: drop panels whose type is no longer
 * registered (and the groups they leave empty), re-validate params, and
 * drop popout windows (not offered in a desktop shell). Returns null when
 * nothing usable is left, so the caller falls back to the preset.
 */
export function repairDocument(raw: unknown): SerializedDockview | null {
  if (!isObject(raw) || !isObject(raw.grid) || !isObject(raw.panels) || !isObject(raw.grid.root)) return null;
  const doc = structuredClone(raw) as unknown as SerializedDockview;
  for (const [id, state] of Object.entries(doc.panels)) {
    const def = state && typeof state.contentComponent === "string" ? getPanel(state.contentComponent) : undefined;
    if (!def) {
      delete doc.panels[id];
      continue;
    }
    state.params = def.parseParams(state.params ?? {}) as Record<string, unknown>;
    // Layouts saved before minimum sizes existed get them too.
    state.minimumWidth ??= PANEL_MIN.minimumWidth;
    state.minimumHeight ??= PANEL_MIN.minimumHeight;
  }
  const known = (id: string) => id in doc.panels;
  const groupIds = new Set<string>();

  const prune = (node: SerializedGridObject<GroupPanelViewState>): SerializedGridObject<GroupPanelViewState> | null => {
    if (node.type === "leaf") {
      const data = node.data as GroupPanelViewState;
      if (!isObject(data) || !Array.isArray(data.views)) return null;
      const views = data.views.filter(known);
      if (views.length === 0) return null;
      if (typeof data.id === "string") groupIds.add(data.id);
      return {
        ...node,
        data: { ...data, views, activeView: data.activeView && views.includes(data.activeView) ? data.activeView : views[0] },
      };
    }
    const children = Array.isArray(node.data)
      ? (node.data as SerializedGridObject<GroupPanelViewState>[]).map(prune).filter((c): c is SerializedGridObject<GroupPanelViewState> => c !== null)
      : [];
    return children.length ? { ...node, data: children } : null;
  };

  const root = prune(doc.grid.root as unknown as SerializedGridObject<GroupPanelViewState>);
  if (!root) return null;
  // The root must stay a branch.
  const fixedRoot = root.type === "branch" ? root : { type: "branch" as const, size: root.size, data: [root] };
  doc.grid.root = fixedRoot as unknown as SerializedDockview["grid"]["root"];
  if (Array.isArray(doc.floatingGroups)) {
    // A floating group holds one group's tabs; drop missing panels, then
    // groups left empty. Shapes this code doesn't recognise are dropped.
    doc.floatingGroups = doc.floatingGroups.filter((f) => {
      const data = (f as { data?: GroupPanelViewState }).data;
      if (!isObject(data) || !Array.isArray(data.views)) return false;
      data.views = data.views.filter(known);
      if (data.activeView && !data.views.includes(data.activeView)) data.activeView = data.views[0];
      return data.views.length > 0;
    });
  }
  delete doc.popoutGroups;
  if (doc.activeGroup && !groupIds.has(doc.activeGroup)) delete doc.activeGroup;
  return doc;
}
