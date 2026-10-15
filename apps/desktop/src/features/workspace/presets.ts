import type { LayoutNode, LayoutView, PanelTypeId, WorkspaceId, WorkspacePreset } from "./types";

const view = (type: PanelTypeId, params?: object): LayoutView => ({ type, params });
const panel = (...views: LayoutView[]): LayoutNode => ({ kind: "panel", views });
const row = (children: LayoutNode[], weights?: number[]): LayoutNode => ({ kind: "split", axis: "row", children, weights });
const column = (children: LayoutNode[], weights?: number[]): LayoutNode => ({ kind: "split", axis: "column", children, weights });
const stage = (child: LayoutNode | null): LayoutNode => ({ kind: "stage", child });

/**
 * The default layout of each workspace. Users rearrange freely from here;
 * "Reset layout" comes back to these.
 */
export const WORKSPACES: readonly WorkspacePreset[] = [
  {
    id: "studio",
    name: "Studio",
    icon: "grid",
    description: "Everything at once: files, data, configuration, runs and their curves.",
    version: 1,
    layout: row(
      [
        column([panel(view("project-explorer")), panel(view("experiment-explorer"))], [3, 2]),
        column(
          [stage(panel(view("dataset-explorer"), view("toml-editor"), view("sql-editor"))), panel(view("execution-logs"))],
          [3, 1.1],
        ),
        column([panel(view("training-metrics")), panel(view("model-inspector"))], [1, 1]),
      ],
      [1.15, 3, 1.6],
    ),
  },
  {
    id: "data",
    name: "Data",
    icon: "database",
    description: "Browse datasets, inspect schemas and statistics, and query them with SQL.",
    version: 1,
    layout: row([panel(view("project-explorer")), column([stage(panel(view("dataset-explorer"))), panel(view("sql-editor"))], [3, 2])], [1, 4]),
  },
  {
    id: "experiments",
    name: "Experiments",
    icon: "layers",
    description: "Runs side by side: metrics, comparisons and logs.",
    version: 1,
    layout: row(
      [panel(view("experiment-explorer")), column([stage(panel(view("training-metrics"))), panel(view("execution-logs"))], [3, 1.3])],
      [1.3, 3],
    ),
  },
  {
    id: "models",
    name: "Models",
    icon: "brain",
    description: "What a run trained: architecture, hyperparameters, checkpoints, evaluation.",
    version: 1,
    layout: row([panel(view("experiment-explorer")), stage(panel(view("model-inspector"))), panel(view("training-metrics"))], [1.2, 2.4, 1.8]),
  },
  {
    id: "config",
    name: "Config",
    icon: "file-code",
    description: "Edit TOML configuration with validation and a structured view.",
    version: 1,
    layout: row([panel(view("project-explorer")), stage(panel(view("toml-editor")))], [1, 3.5]),
  },
];

export function getWorkspace(id: WorkspaceId): WorkspacePreset {
  const preset = WORKSPACES.find((w) => w.id === id);
  if (!preset) throw new Error(`unknown workspace ${id}`);
  return preset;
}

export function isWorkspaceId(id: string): id is WorkspaceId {
  return WORKSPACES.some((w) => w.id === id);
}

/** Every view a layout contains, depth-first. */
export function layoutViews(node: LayoutNode | null): LayoutView[] {
  if (!node) return [];
  switch (node.kind) {
    case "panel":
      return node.views;
    case "stage":
      return layoutViews(node.child);
    case "split":
      return node.children.flatMap(layoutViews);
  }
}
