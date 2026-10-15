/**
 * Where Epoch is running, and the few things only its host can do.
 *
 * Epoch's UI runs in two places from this one source: the standalone app
 * (this repo's src-tauri) and an Epoch window inside Nexis, which builds
 * `embed.tsx` into its own bundle. Both register `tauri-plugin-epoch`, so
 * every Epoch command is the same in both (`lib/ipc.ts`). What differs is
 * how Epoch talks to Nexis:
 *
 * - **Standalone**: links arrive over the loopback channel (src-tauri
 *   `links.rs`), and "Open in Synapse" hands Nexis a `nexis://` link,
 *   starting Nexis if it isn't running.
 * - **Inside Nexis**: links are the window's launch parameters, and
 *   Synapse and Pulse are windows of the same app, so Nexis opens them
 *   directly.
 *
 * Nothing else may branch on the host. A feature that works standalone
 * works in Nexis.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { IN_TAURI } from "@nexis/design";

import type { EpochHost, EpochLink } from "./host-types";

export type { EpochHost, EpochLink, NexisOpenArgs } from "./host-types";

/** The standalone app: the loopback link channel in src-tauri/src/links.rs. */
export const standaloneHost: EpochHost = {
  kind: "standalone",
  openInNexis: (args) =>
    invoke<"sent" | "launched">("nexis_open", {
      tool: args.tool,
      project: args.project ?? null,
      run: args.run ?? null,
      model: args.model ?? null,
    }),
  subscribeLinks(follow) {
    if (!IN_TAURI) return () => {};
    const drain = () => {
      void invoke<EpochLink[]>("links_take_pending")
        .then((links) => links.forEach(follow))
        .catch(() => {});
    };
    drain();
    const unlisten = listen("epoch://link", drain);
    return () => void unlisten.then((fn) => fn()).catch(() => {});
  },
};

let current: EpochHost = standaloneHost;

/** Set once, before the first render (main.tsx or embed.tsx). */
export function setHost(host: EpochHost): void {
  current = host;
}

export function host(): EpochHost {
  return current;
}
