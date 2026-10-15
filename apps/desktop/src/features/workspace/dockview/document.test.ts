import type { SerializedGridObject } from "dockview-react";
import { beforeAll, describe, expect, it } from "vitest";
import { getWorkspace, layoutViews, WORKSPACES } from "../presets";
import { definePanel, registerPanels, resetRegistryForTests, stringParams } from "../registry";
import type { LayoutNode, PanelDefinition, PanelTypeId } from "../types";
import { type GroupPanelViewState, normalize, presetDocument, repairDocument } from "./document";

const TYPES: PanelTypeId[] = [
  "project-explorer",
  "dataset-explorer",
  "sql-editor",
  "toml-editor",
  "experiment-explorer",
  "training-metrics",
  "execution-logs",
  "model-inspector",
];

beforeAll(() => {
  resetRegistryForTests();
  registerPanels(
    TYPES.map(
      (type) =>
        definePanel({
          type,
          title: type,
          icon: "file",
          description: "",
          component: () => null,
          parseParams: (raw) => stringParams(raw, ["path"] as const) as never,
          placement: "side",
          maturity: "ready",
        }) as unknown as PanelDefinition<PanelTypeId>,
    ),
  );
});

type Node = SerializedGridObject<GroupPanelViewState>;
const leaves = (n: unknown): GroupPanelViewState[] => {
  const node = n as Node;
  return node.type === "leaf" ? [node.data as GroupPanelViewState] : (node.data as Node[]).flatMap(leaves);
};

describe("presetDocument", () => {
  it("puts every preset view in a group, with deterministic ids", () => {
    for (const preset of WORKSPACES) {
      const doc = presetDocument(preset);
      const ids = leaves(doc.grid.root).flatMap((g) => g.views);
      expect(ids.length).toBe(layoutViews(preset.layout).length);
      expect(Object.keys(doc.panels).sort()).toEqual([...ids].sort());
      expect(presetDocument(preset)).toEqual(doc);
    }
  });

  it("keeps tabs together and sizes groups by weight", () => {
    const doc = presetDocument(getWorkspace("studio"));
    expect(doc.grid.orientation).toBe("HORIZONTAL");
    const columns = doc.grid.root.data as unknown as Node[];
    expect(columns).toHaveLength(3);
    // Weights 1.15 : 3 : 1.6 across the canvas.
    expect(columns[1].size).toBeCloseTo((3 / 5.75) * 1000, 3);
    const stage = leaves(columns[1]).find((g) => g.views.length === 3)!;
    expect(stage.views).toEqual(["dataset-explorer-1", "toml-editor-1", "sql-editor-1"]);
    expect(stage.activeView).toBe("dataset-explorer-1");
    expect(doc.panels["toml-editor-1"].contentComponent).toBe("toml-editor");
  });

  it("wraps a single panel in a branch", () => {
    const doc = presetDocument({ ...getWorkspace("config"), layout: { kind: "panel", views: [{ type: "sql-editor" }] } });
    expect(doc.grid.root.type).toBe("branch");
    expect(leaves(doc.grid.root)[0].views).toEqual(["sql-editor-1"]);
  });
});

describe("normalize", () => {
  it("drops stages and flattens same-axis splits, keeping proportions", () => {
    const p = (type: PanelTypeId): LayoutNode => ({ kind: "panel", views: [{ type }] });
    const tree: LayoutNode = {
      kind: "split",
      axis: "row",
      weights: [1, 2],
      children: [p("sql-editor"), { kind: "stage", child: { kind: "split", axis: "row", weights: [1, 1], children: [p("toml-editor"), p("model-inspector")] } }],
    };
    const out = normalize(tree) as Extract<LayoutNode, { kind: "split" }>;
    expect(out.children).toHaveLength(3);
    expect(out.weights).toEqual([1, 1, 1]);
  });
});

describe("repairDocument", () => {
  it("passes a valid document through", () => {
    const doc = presetDocument(getWorkspace("data"));
    expect(repairDocument(doc)).toEqual(doc);
  });

  it("drops unknown panel types and the groups they empty", () => {
    const doc = presetDocument(getWorkspace("data"));
    doc.panels["project-explorer-1"].contentComponent = "retired-panel";
    doc.popoutGroups = [];
    const fixed = repairDocument(doc)!;
    expect(fixed.panels["project-explorer-1"]).toBeUndefined();
    expect(leaves(fixed.grid.root).flatMap((g) => g.views)).not.toContain("project-explorer-1");
    expect(fixed.popoutGroups).toBeUndefined();
  });

  it("repairs params and active views", () => {
    const doc = presetDocument(getWorkspace("data"));
    doc.panels["dataset-explorer-1"].params = { path: 7, junk: true };
    const group = leaves(doc.grid.root).find((g) => g.views.includes("dataset-explorer-1"))!;
    group.activeView = "gone";
    const fixed = repairDocument(doc)!;
    expect(fixed.panels["dataset-explorer-1"].params).toEqual({});
    expect(leaves(fixed.grid.root).find((g) => g.views.includes("dataset-explorer-1"))!.activeView).toBe("dataset-explorer-1");
  });

  it("returns null for garbage or an empty result", () => {
    expect(repairDocument(null)).toBeNull();
    expect(repairDocument({ root: {}, views: {} })).toBeNull();
    const doc = presetDocument(getWorkspace("config"));
    for (const p of Object.values(doc.panels)) p.contentComponent = "nope";
    expect(repairDocument(doc)).toBeNull();
  });
});
