/**
 * Saved workspace layouts.
 *
 * Each workspace's layout is stored as a versioned envelope:
 *
 *   { schema, workspace, adapter, presetVersion, savedAt, document }
 *
 * `document` belongs to the layout adapter (today dockview's SerializedDockview);
 * the envelope is Epoch's. When the envelope changes shape, bump
 * `LAYOUT_SCHEMA` and add a migration from the previous schema to
 * `MIGRATIONS`. Loading runs every migration from the stored schema up, so
 * a layout saved by any older version still opens. A layout saved by a
 * newer Epoch is not guessed at: the preset is used and the file is left
 * alone until the user changes the layout.
 */
import { api } from "@epoch/lib/api";
import { BACKEND_AVAILABLE } from "@epoch/lib/ipc";
import type { WorkspaceId } from "./types";

export const LAYOUT_SCHEMA = 1;
/** Layouts from another adapter (Epoch 0.1.0 used Trellis) are not misread:
 * the preset is used instead, with a notice. */
export const LAYOUT_ADAPTER = "dockview";

export interface LayoutEnvelope {
  schema: typeof LAYOUT_SCHEMA;
  workspace: WorkspaceId;
  adapter: string;
  /** The preset version the user started from. */
  presetVersion: number;
  savedAt: string;
  document: unknown;
}

type Json = Record<string, unknown>;
type Migration = (input: Json, workspace: WorkspaceId) => Json;

/** `MIGRATIONS[n]` turns schema `n` into schema `n + 1`. */
const MIGRATIONS: Record<number, Migration> = {
  // 0 → 1. Schema 0 is a bare dockview document (`api.toJSON()`) with no
  // envelope. Accepting it lets a layout exported from dockview be imported.
  0: (doc, workspace) => ({
    schema: 1,
    workspace,
    adapter: LAYOUT_ADAPTER,
    presetVersion: 0,
    savedAt: new Date(0).toISOString(),
    document: doc,
  }),
};

export type MigrateResult =
  | { ok: true; envelope: LayoutEnvelope; migratedFrom: number | null }
  | { ok: false; reason: string };

function isObject(v: unknown): v is Json {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function schemaOf(raw: Json): number | null {
  if ("document" in raw && typeof raw.schema === "number") return raw.schema;
  // A bare dockview document: a grid plus a panel table.
  if ("grid" in raw && "panels" in raw) return 0;
  return null;
}

/** Bring any saved layout up to the current envelope, or say why not. */
export function migrateLayout(raw: unknown, workspace: WorkspaceId): MigrateResult {
  let value: unknown = raw;
  if (typeof raw === "string") {
    try {
      value = JSON.parse(raw);
    } catch {
      return { ok: false, reason: "the saved layout is not valid JSON" };
    }
  }
  if (!isObject(value)) return { ok: false, reason: "the saved layout is not an object" };
  const from = schemaOf(value);
  if (from === null) return { ok: false, reason: "the saved layout has no recognisable schema" };
  if (from > LAYOUT_SCHEMA) {
    return { ok: false, reason: `the saved layout was written by a newer Epoch (schema ${from})` };
  }
  let current: Json = value;
  for (let v = from; v < LAYOUT_SCHEMA; v++) {
    const step = MIGRATIONS[v];
    if (!step) return { ok: false, reason: `no migration from layout schema ${v}` };
    current = step(current, workspace);
  }
  if (current.workspace !== workspace) {
    return { ok: false, reason: `the saved layout belongs to workspace ${String(current.workspace)}` };
  }
  if (current.adapter !== LAYOUT_ADAPTER) {
    return { ok: false, reason: `the saved layout was made for ${String(current.adapter)}, not ${LAYOUT_ADAPTER}` };
  }
  if (!isObject(current.document)) return { ok: false, reason: "the saved layout has no document" };
  return {
    ok: true,
    envelope: current as unknown as LayoutEnvelope,
    migratedFrom: from === LAYOUT_SCHEMA ? null : from,
  };
}

export function makeEnvelope(workspace: WorkspaceId, presetVersion: number, document: unknown): LayoutEnvelope {
  return {
    schema: LAYOUT_SCHEMA,
    workspace,
    adapter: LAYOUT_ADAPTER,
    presetVersion,
    savedAt: new Date().toISOString(),
    document,
  };
}

// ── Storage: the app data folder in the desktop app, localStorage in a
//    browser preview ─────────────────────────────────────────────────────

const localKey = (id: WorkspaceId) => `nexis-epoch:layout:${id}`;

async function readRaw(id: WorkspaceId): Promise<string | null> {
  if (BACKEND_AVAILABLE) return api.layoutLoad(id);
  try {
    return localStorage.getItem(localKey(id));
  } catch {
    return null;
  }
}

export type LoadedLayout = { document: unknown | null; notice: string | null };

export async function loadLayout(id: WorkspaceId): Promise<LoadedLayout> {
  let raw: string | null;
  try {
    raw = await readRaw(id);
  } catch (e) {
    return { document: null, notice: `Couldn't read the saved layout: ${e instanceof Error ? e.message : String(e)}` };
  }
  if (raw === null) return { document: null, notice: null };
  const result = migrateLayout(raw, id);
  if (!result.ok) return { document: null, notice: `Using the default layout because ${result.reason}.` };
  return { document: result.envelope.document, notice: null };
}

export async function saveLayout(id: WorkspaceId, presetVersion: number, document: unknown): Promise<void> {
  const json = JSON.stringify(makeEnvelope(id, presetVersion, document));
  if (BACKEND_AVAILABLE) return api.layoutSave(id, json);
  try {
    localStorage.setItem(localKey(id), json);
  } catch {
    // Storage unavailable (private window): the layout lives for this session.
  }
}

export async function deleteLayout(id: WorkspaceId): Promise<void> {
  if (BACKEND_AVAILABLE) return api.layoutDelete(id);
  try {
    localStorage.removeItem(localKey(id));
  } catch {
    // nothing to remove
  }
}
