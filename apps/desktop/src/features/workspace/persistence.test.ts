import { describe, expect, it } from "vitest";
import { LAYOUT_SCHEMA, makeEnvelope, migrateLayout } from "./persistence";

const trellisDoc = { schema: 1, root: { kind: "panel", id: "p1", views: ["v1"] }, floating: [], hidden: [], views: { v1: { type: "sql-editor" } } };

describe("migrateLayout", () => {
  it("passes a current envelope through untouched", () => {
    const env = makeEnvelope("data", 1, trellisDoc);
    const result = migrateLayout(JSON.stringify(env), "data");
    expect(result).toEqual({ ok: true, envelope: env, migratedFrom: null });
  });

  it("upgrades a bare Trellis document (schema 0)", () => {
    const result = migrateLayout(trellisDoc, "studio");
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.migratedFrom).toBe(0);
    expect(result.envelope.schema).toBe(LAYOUT_SCHEMA);
    expect(result.envelope.workspace).toBe("studio");
    expect(result.envelope.document).toEqual(trellisDoc);
  });

  it("refuses layouts from a newer Epoch rather than guessing", () => {
    const future = { ...makeEnvelope("data", 1, trellisDoc), schema: LAYOUT_SCHEMA + 1 };
    const result = migrateLayout(future, "data");
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.reason).toMatch(/newer Epoch/);
  });

  it("refuses another workspace's layout, another adapter's, and garbage", () => {
    expect(migrateLayout(makeEnvelope("data", 1, trellisDoc), "models").ok).toBe(false);
    expect(migrateLayout({ ...makeEnvelope("data", 1, trellisDoc), adapter: "golden-layout" }, "data").ok).toBe(false);
    expect(migrateLayout("{not json", "data").ok).toBe(false);
    expect(migrateLayout([1, 2], "data").ok).toBe(false);
    expect(migrateLayout({ hello: "world" }, "data").ok).toBe(false);
    expect(migrateLayout({ ...makeEnvelope("data", 1, trellisDoc), document: null }, "data").ok).toBe(false);
  });
});
