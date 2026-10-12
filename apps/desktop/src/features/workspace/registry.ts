/**
 * The central panel registry. Panel types are registered once, at startup
 * (`features/panels.ts`); the shell, the palette and the layout adapter all
 * read them from here instead of hardcoding panels.
 */
import type { PanelDefinition, PanelTypeId } from "./types";

const registry = new Map<PanelTypeId, PanelDefinition>();

/** Type-checks a definition against its params type; returns it unchanged. */
export function definePanel<T extends PanelTypeId>(def: PanelDefinition<T>): PanelDefinition<T> {
  return def;
}

/**
 * Register panel types. A type listed twice is a bug and throws; registering
 * the list again (a hot reload in development) replaces the definitions.
 */
export function registerPanels(defs: readonly PanelDefinition<PanelTypeId>[]): void {
  const seen = new Set<PanelTypeId>();
  for (const def of defs) {
    if (seen.has(def.type)) throw new Error(`panel type listed twice: ${def.type}`);
    seen.add(def.type);
    registry.set(def.type, def);
  }
}

export function getPanel(type: string): PanelDefinition | undefined {
  return registry.get(type as PanelTypeId);
}

export function listPanels(): PanelDefinition[] {
  return [...registry.values()];
}

export function isPanelType(type: string): type is PanelTypeId {
  return registry.has(type as PanelTypeId);
}

// ── Param helpers for parseParams ─────────────────────────────────────────

function record(raw: unknown): Record<string, unknown> {
  return typeof raw === "object" && raw !== null && !Array.isArray(raw) ? (raw as Record<string, unknown>) : {};
}

/** Keep only the listed string fields of a params object. */
export function stringParams<K extends string>(raw: unknown, keys: readonly K[]): { [P in K]?: string } {
  const r = record(raw);
  const out: { [P in K]?: string } = {};
  for (const k of keys) {
    const v = r[k];
    if (typeof v === "string" && v.length > 0 && v.length < 100_000) out[k] = v;
  }
  return out;
}

export function noParams(): Record<string, never> {
  return {};
}

/** Test-only: forget registrations. */
export function resetRegistryForTests(): void {
  registry.clear();
}
