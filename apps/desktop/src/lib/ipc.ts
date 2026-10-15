import { invoke } from "@tauri-apps/api/core";
import { IN_TAURI } from "@nexis/design";

/** Stable error codes from the Rust `CommandError`, plus `unavailable`. */
export type ErrorCode =
  | "unavailable"
  | "no_project"
  | "not_found"
  | "permission_denied"
  | "invalid_input"
  | "too_large"
  | "binary"
  | "conflict"
  | "io"
  | "database"
  | "query"
  | "timeout"
  | "cancelled"
  | "internal";

export class BackendError extends Error {
  readonly code: ErrorCode;
  constructor(code: ErrorCode, message: string) {
    super(message);
    this.name = "BackendError";
    this.code = code;
  }
}

/** The desktop backend is reachable. False in a plain browser (vite dev). */
export const BACKEND_AVAILABLE = IN_TAURI;

function toBackendError(e: unknown): BackendError {
  if (e instanceof BackendError) return e;
  if (typeof e === "object" && e !== null && "code" in e && "message" in e) {
    const { code, message } = e as { code: unknown; message: unknown };
    return new BackendError(String(code) as ErrorCode, String(message));
  }
  return new BackendError("internal", e instanceof Error ? e.message : String(e));
}

/**
 * Invoke an Epoch command; failures always reject with a `BackendError`.
 * The commands live in `tauri-plugin-epoch`, which the standalone app and
 * Nexis's Epoch window both register, hence the plugin prefix.
 */
export async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!BACKEND_AVAILABLE) {
    throw new BackendError(
      "unavailable",
      "The Epoch backend runs inside the desktop app. This browser preview can't reach your files.",
    );
  }
  try {
    return await invoke<T>(`plugin:epoch|${command}`, args);
  } catch (e) {
    throw toBackendError(e);
  }
}

export function errorMessage(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

let nextQuery = 0;
/** A fresh id for a cancellable query (`query_cancel`). */
export function newQueryId(prefix: string): string {
  nextQuery += 1;
  return `${prefix}-${Date.now().toString(36)}-${nextQuery}`;
}
