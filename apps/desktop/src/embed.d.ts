/**
 * What a host sees of `embed.tsx`: declarations only, so the host's
 * TypeScript doesn't compile Epoch's source under its own settings (Epoch's
 * CI type-checks it). `embed-types.test.ts` keeps this in step with
 * embed.tsx.
 */
import type { EpochHost } from "./host-types";

export type { EpochHost, EpochLink, NexisOpenArgs } from "./host-types";

/** Render Epoch into `element`, talking to its host through `host`. */
export declare function mountEpoch(element: HTMLElement, host: EpochHost): void;
