/**
 * The workspace contract, independent of the docking library.
 *
 * Panels, layouts and commands are described here in Epoch's own terms.
 * Only `features/workspace/dockview/` knows about dockview; replacing it
 * means writing another adapter against these types, not touching any panel.
 */
import type * as React from "react";
import type { IconName } from "@nexis/design";

/** Stable panel type ids. Saved layouts refer to these: never rename one. */
export type PanelTypeId =
  | "project-explorer"
  | "dataset-explorer"
  | "sql-editor"
  | "toml-editor"
  | "experiment-explorer"
  | "training-metrics"
  | "execution-logs"
  | "model-inspector";

/** Each panel type's params: small, serialisable, saved with the layout. */
export interface PanelParamsMap {
  "project-explorer": Record<string, never>;
  "dataset-explorer": { path?: string };
  "sql-editor": { sql?: string };
  "toml-editor": { path?: string };
  "experiment-explorer": Record<string, never>;
  /** With `runDir`, pinned to that run; without, follows the selection. */
  "training-metrics": { runDir?: string };
  "execution-logs": { runDir?: string };
  "model-inspector": { runDir?: string };
}

export type PanelParams<T extends PanelTypeId> = PanelParamsMap[T];

export interface PanelDefinition<T extends PanelTypeId = PanelTypeId> {
  type: T;
  title: string;
  icon: IconName;
  description: string;
  component: React.ComponentType;
  /** Repair params from a saved layout or a caller. Never throws. */
  parseParams(raw: unknown): PanelParams<T>;
  /** A tab title from params, when it should say more than `title`. */
  tabTitle?(params: PanelParams<T>): string | null;
  /** Opened in the central stage (documents) or docked beside it (tools). */
  placement: "stage" | "side";
  /** At most one open per workspace. */
  singleton?: boolean;
  /**
   * `ready`: works on real data. `partial`: works, with features still to
   * come (named in `limitations`).
   */
  maturity: "ready" | "partial";
  limitations?: string;
}

/** What a panel's component can read and do about itself. */
export interface PanelHandle<P> {
  id: string;
  type: PanelTypeId;
  params: P;
  setParams(patch: Partial<P>): void;
  /** Mark unsaved changes (shown as a dot on the tab, guards close). */
  setDirty(dirty: boolean): void;
  /** Return false (or resolve false) to keep the panel open. */
  setCloseGuard(guard: (() => boolean | Promise<boolean>) | null): void;
  visible: boolean;
  focused: boolean;
}

/**
 * The neutral layout description used for workspace defaults. A `stage`
 * marks where documents go; adapters without such a region treat it as a
 * plain group.
 */
export type LayoutNode =
  | { kind: "split"; axis: "row" | "column"; weights?: number[]; children: LayoutNode[] }
  | { kind: "panel"; views: LayoutView[]; selected?: number }
  | { kind: "stage"; child: LayoutNode | null };

export type LayoutView = { type: PanelTypeId; params?: object; id?: string };

export type WorkspaceId = "studio" | "data" | "experiments" | "models" | "config";

export interface WorkspacePreset {
  id: WorkspaceId;
  name: string;
  icon: IconName;
  description: string;
  /** Bump when `layout` changes enough that "Reset layout" should say so. */
  version: number;
  layout: LayoutNode;
}

export interface OpenOptions {
  /** Focus an existing panel instead of opening another: of the same type, or the same params. */
  reuse?: "none" | "type" | "params";
  placement?: "stage" | "side" | "float";
}

/** Imperative control of one mounted workspace. */
export interface WorkspaceApi {
  id: WorkspaceId;
  openPanel<T extends PanelTypeId>(type: T, params?: PanelParams<T>, options?: OpenOptions): void;
  /** Animate back to the preset layout. Resolves false if the user kept unsaved work. */
  resetLayout(): Promise<boolean>;
  /** Maximize the focused panel, or restore it. */
  toggleZoom(): void;
  /** Leave any zoom. */
  overview(): void;
  /** Undock the focused panel into a floating window over the workspace. */
  floatFocused(): void;
  closeFocused(): void;
}
