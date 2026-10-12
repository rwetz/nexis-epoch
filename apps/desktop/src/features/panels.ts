/**
 * Every panel type Epoch ships, registered once at startup. Adding a panel
 * is: write its component and `definePanel(...)`, add a `PanelTypeId` and
 * its params in workspace/types.ts, and list it here.
 */
import { tomlEditorPanel } from "./configurations/TomlEditorPanel";
import { datasetExplorerPanel } from "./datasets/DatasetExplorerPanel";
import { sqlEditorPanel } from "./datasets/SqlEditorPanel";
import { experimentExplorerPanel } from "./experiments/ExperimentExplorerPanel";
import { modelInspectorPanel } from "./models/ModelInspectorPanel";
import { projectExplorerPanel } from "./projects/ProjectExplorerPanel";
import { executionLogsPanel } from "./training/ExecutionLogsPanel";
import { trainingMetricsPanel } from "./training/TrainingMetricsPanel";
import { registerPanels } from "./workspace/registry";
import type { PanelDefinition, PanelTypeId } from "./workspace/types";

export const PANELS = [
  projectExplorerPanel,
  datasetExplorerPanel,
  sqlEditorPanel,
  tomlEditorPanel,
  experimentExplorerPanel,
  trainingMetricsPanel,
  executionLogsPanel,
  modelInspectorPanel,
] as unknown as readonly PanelDefinition<PanelTypeId>[];

registerPanels(PANELS);
