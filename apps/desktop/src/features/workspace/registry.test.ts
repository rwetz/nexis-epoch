import { beforeEach, describe, expect, it } from "vitest";
import { definePanel, getPanel, isPanelType, listPanels, registerPanels, resetRegistryForTests, stringParams } from "./registry";
import { layoutViews, WORKSPACES } from "./presets";
import type { PanelDefinition, PanelTypeId } from "./types";

const fake = (type: PanelTypeId) =>
  definePanel({
    type,
    title: type,
    icon: "file",
    description: "",
    component: () => null,
    parseParams: (raw) => stringParams(raw, ["path"] as const) as never,
    placement: "side",
    maturity: "ready",
  }) as unknown as PanelDefinition<PanelTypeId>;

describe("panel registry", () => {
  beforeEach(resetRegistryForTests);

  it("registers, looks up and lists panel types", () => {
    registerPanels([fake("sql-editor"), fake("toml-editor")]);
    expect(getPanel("sql-editor")?.type).toBe("sql-editor");
    expect(isPanelType("toml-editor")).toBe(true);
    expect(isPanelType("not-a-panel")).toBe(false);
    expect(listPanels()).toHaveLength(2);
  });

  it("rejects a type listed twice but allows re-registration", () => {
    expect(() => registerPanels([fake("sql-editor"), fake("sql-editor")])).toThrow(/listed twice/);
    registerPanels([fake("toml-editor")]);
    registerPanels([fake("toml-editor")]);
    expect(listPanels()).toHaveLength(2);
  });

  it("keeps only valid string params", () => {
    expect(stringParams({ path: "a.csv", extra: 1, sql: 3 }, ["path", "sql"] as const)).toEqual({ path: "a.csv" });
    expect(stringParams(null, ["path"] as const)).toEqual({});
    expect(stringParams({ path: "" }, ["path"] as const)).toEqual({});
  });
});

describe("workspace presets", () => {
  it("have unique ids and only reference known panel types", () => {
    const known: PanelTypeId[] = [
      "project-explorer",
      "dataset-explorer",
      "sql-editor",
      "toml-editor",
      "experiment-explorer",
      "training-metrics",
      "execution-logs",
      "model-inspector",
    ];
    expect(new Set(WORKSPACES.map((w) => w.id)).size).toBe(WORKSPACES.length);
    for (const w of WORKSPACES) {
      const views = layoutViews(w.layout);
      expect(views.length).toBeGreaterThan(0);
      for (const v of views) expect(known).toContain(v.type);
    }
  });

  it("include every panel type in the Studio workspace", () => {
    const studio = WORKSPACES.find((w) => w.id === "studio")!;
    expect(new Set(layoutViews(studio.layout).map((v) => v.type)).size).toBe(8);
  });
});
