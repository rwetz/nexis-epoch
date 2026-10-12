import * as React from "react";
import type { PanelHandle, PanelParams, PanelTypeId } from "./types";

export const PanelContext = React.createContext<PanelHandle<object> | null>(null);

/**
 * A panel's own handle: its params, and ways to update them, mark unsaved
 * work, and guard closing. Provided by whichever layout adapter mounted it.
 */
export function usePanel<T extends PanelTypeId>(_type: T): PanelHandle<PanelParams<T>> {
  const handle = React.useContext(PanelContext);
  if (!handle) throw new Error("usePanel must be used inside a workspace panel");
  return handle as unknown as PanelHandle<PanelParams<T>>;
}
