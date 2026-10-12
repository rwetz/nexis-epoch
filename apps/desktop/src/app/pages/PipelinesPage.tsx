import { EmptyState } from "@/components/states";

/** Clearly a placeholder, and kept to one screen on purpose. */
export function PipelinesPage() {
  return (
    <EmptyState
      icon="nodes"
      title="Pipelines are planned"
      description="Chained data and training steps (load → transform with Polars → train → evaluate) will live here once the training runtime lands. Nothing on this page is functional yet."
    />
  );
}
