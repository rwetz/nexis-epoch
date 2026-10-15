/**
 * The host contract's types, with no imports, so a host can type-check
 * against them without compiling Epoch (embed.d.ts re-exports them; Nexis's
 * tsc reads only that). host.ts documents what each host does.
 */

/** Open a project, then maybe a run or a dataset in it. */
export type EpochLink = { project: string; run: string | null; dataset: string | null };

export type NexisOpenArgs = { tool: "ml-lab" | "benchmark"; project?: string; run?: string; model?: string };

export interface EpochHost {
  readonly kind: "standalone" | "nexis";
  /** Show Synapse or Pulse. `launched` means Nexis had to be started. */
  openInNexis(args: NexisOpenArgs): Promise<"sent" | "launched" | "opened">;
  /** Hand every link (the one Epoch opened with, and later ones) to
   * `follow`, in order. Returns the cleanup. */
  subscribeLinks(follow: (link: EpochLink) => void): () => void;
}
