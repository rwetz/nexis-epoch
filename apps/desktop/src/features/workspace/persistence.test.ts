import { describe, expect, it } from "vitest";
import { LAYOUT_SCHEMA, makeEnvelope, migrateLayout } from "./persistence";

const dockviewDoc = {
  grid: {
    root: { type: "branch", size: 1000, data: [{ type: "leaf", size: 1000, data: { id: "group-1", views: ["sql-editor-1"], activeView: "sql-editor-1" } }] },
    width: 1000,
    height: 1000,
    orientation: "HORIZONTAL",
  },
  panels: { "sql-editor-1": { id: "sql-editor-1", contentComponent: "sql-editor", params: {} } },
};

describe("migrateLayout", () => {
  it("passes a current envelope through untouched", () => {
    const env = makeEnvelope("data", 1, dockviewDoc);
    const result = migrateLayout(JSON.stringify(env), "data");
    expect(result).toEqual({ ok: true, envelope: env, migratedFrom: null });
  });

  it("upgrades a bare dockview document (schema 0)", () => {
    const result = migrateLayout(dockviewDoc, "studio");
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.migratedFrom).toBe(0);
    expect(result.envelope.schema).toBe(LAYOUT_SCHEMA);
    expect(result.envelope.workspace).toBe("studio");
    expect(result.envelope.adapter).toBe("dockview");
    expect(result.envelope.document).toEqual(dockviewDoc);
  });

  it("refuses a layout saved by Epoch 0.1.0's Trellis adapter, saying why", () => {
    const old = { ...makeEnvelope("studio", 1, { schema: 1, root: null, views: {} }), adapter: "trellis" };
    const result = migrateLayout(old, "studio");
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.reason).toMatch(/made for trellis, not dockview/);
  });

  it("refuses layouts from a newer Epoch rather than guessing", () => {
    const future = { ...makeEnvelope("data", 1, dockviewDoc), schema: LAYOUT_SCHEMA + 1 };
    const result = migrateLayout(future, "data");
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.reason).toMatch(/newer Epoch/);
  });

  it("refuses another workspace's layout and garbage", () => {
    expect(migrateLayout(makeEnvelope("data", 1, dockviewDoc), "models").ok).toBe(false);
    expect(migrateLayout("{not json", "data").ok).toBe(false);
    expect(migrateLayout([1, 2], "data").ok).toBe(false);
    expect(migrateLayout({ hello: "world" }, "data").ok).toBe(false);
    expect(migrateLayout({ ...makeEnvelope("data", 1, dockviewDoc), document: null }, "data").ok).toBe(false);
  });
});
