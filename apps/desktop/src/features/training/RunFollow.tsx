import { ToolbarButton } from "@epoch/components/PanelToolbar";
import { useExperiments } from "@epoch/features/experiments/store";

/**
 * Pin a panel to the focused run, or let it follow the selection again.
 * Pinning is what makes a second Training Metrics (or Model Inspector)
 * panel useful: one stays on a baseline while the other follows along.
 */
export function RunFollowToggle({ pinned, onChange }: { pinned: string | undefined; onChange(runDir: string | undefined): void }) {
  const focused = useExperiments((s) => s.selected[0]);
  return pinned ? (
    <ToolbarButton icon="pin" active label="Pinned to one run. Click to follow the selection" onClick={() => onChange(undefined)} />
  ) : (
    <ToolbarButton icon="pin" label="Pin to the focused run" disabled={!focused} onClick={() => onChange(focused)} />
  );
}
