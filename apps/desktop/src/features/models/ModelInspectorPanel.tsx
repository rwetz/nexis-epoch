import * as React from "react";
import { PropertyList, SectionLabel, Slider, StatusDot } from "@nexis/design";
import { MatrixChart } from "@epoch/components/charts/MatrixChart";
import { PanelToolbar, ToolbarTitle } from "@epoch/components/PanelToolbar";
import { EmptyState, ErrorState, LoadingState, PlannedNote } from "@epoch/components/states";
import { statusLabel, statusTone } from "@epoch/features/experiments/ExperimentExplorerPanel";
import { useRunViews } from "@epoch/features/experiments/useRunDetails";
import { RunFollowToggle } from "@epoch/features/training/RunFollow";
import { usePanel } from "@epoch/features/workspace/panel-context";
import { definePanel, stringParams } from "@epoch/features/workspace/registry";
import { api } from "@epoch/lib/api";
import { basename } from "@epoch/lib/fileIcons";
import { ModelFileView } from "./ModelFileView";
import { formatBytes, formatTimestamp } from "@epoch/lib/format";
import type { ArtifactInfo, RunDetail } from "@epoch/types/backend";

type Json = null | boolean | number | string | Json[] | { [k: string]: Json };

function scalar(v: Json): string {
  if (v === null) return "null";
  if (Array.isArray(v)) return v.every((x) => typeof x !== "object" || x === null) ? `[${v.join(", ")}]` : JSON.stringify(v);
  if (typeof v === "object") return JSON.stringify(v);
  return String(v);
}

/** A config object as labelled sections: one per top-level table. */
function ConfigSections({ config }: { config: Json }) {
  if (typeof config !== "object" || config === null || Array.isArray(config)) {
    return <p className="px-1 text-xs text-muted-foreground">No configuration snapshot.</p>;
  }
  const entries = Object.entries(config);
  const flat = entries.filter(([, v]) => typeof v !== "object" || v === null || Array.isArray(v));
  const tables = entries.filter(([, v]) => typeof v === "object" && v !== null && !Array.isArray(v)) as [string, { [k: string]: Json }][];
  return (
    <div className="flex flex-col gap-3">
      {flat.length > 0 && <PropertyList items={flat.map(([k, v]) => ({ label: k, value: scalar(v), mono: true }))} />}
      {tables.map(([name, table]) => (
        <div key={name} className="flex flex-col gap-1">
          <SectionLabel>{name}</SectionLabel>
          <PropertyList items={Object.entries(table).map(([k, v]) => ({ label: k, value: scalar(v), mono: true }))} />
        </div>
      ))}
    </div>
  );
}

type Matrix = { labels: string[]; matrix: number[][] };

function isMatrix(v: unknown): v is Matrix {
  const m = v as Matrix;
  return !!m && Array.isArray(m.labels) && Array.isArray(m.matrix) && m.matrix.every((r) => Array.isArray(r) && r.every((x) => typeof x === "number"));
}

/** The per-epoch confusion matrices, scrubbable by epoch. */
function ConfusionMatrices({ detail }: { detail: RunDetail }) {
  const matrices = React.useMemo(
    () => detail.artifacts.filter((a): a is ArtifactInfo & { path: string } => a.kind === "confusion-matrix" && a.exists && a.path !== null),
    [detail],
  );
  const [index, setIndex] = React.useState(matrices.length - 1);
  React.useEffect(() => setIndex(matrices.length - 1), [matrices.length]);
  const [state, setState] = React.useState<{ key: string; matrix?: Matrix; error?: unknown } | null>(null);
  const current = matrices[index];

  React.useEffect(() => {
    if (!current) return;
    const key = current.path;
    let cancelled = false;
    api.experimentArtifact(detail.record.dir, current.path).then(
      (v) => !cancelled && setState(isMatrix(v) ? { key, matrix: v } : { key, error: new Error("not a confusion matrix ({labels, matrix})") }),
      (error: unknown) => !cancelled && setState({ key, error }),
    );
    return () => {
      cancelled = true;
    };
  }, [current, detail.record.dir]);

  if (matrices.length === 0) return null;
  const accuracy = state?.matrix
    ? state.matrix.matrix.reduce((s, row, i) => s + (row[i] ?? 0), 0) / Math.max(1, state.matrix.matrix.flat().reduce((a, b) => a + b, 0))
    : null;
  return (
    <div className="flex flex-col gap-1.5">
      <SectionLabel>Confusion matrix</SectionLabel>
      <div className="flex items-center gap-3 px-1 text-[11px] text-muted-foreground">
        <span className="shrink-0 font-mono tabular-nums">epoch {current?.epoch ?? "?"}</span>
        {matrices.length > 1 && (
          <Slider value={[index]} min={0} max={matrices.length - 1} step={1} onValueChange={([v]) => setIndex(v)} aria-label="Epoch" className="flex-1" />
        )}
        {accuracy !== null && <span className="shrink-0 font-mono tabular-nums">acc {(accuracy * 100).toFixed(1)}%</span>}
      </div>
      <div className="h-64 rounded-lg border border-border/60 p-1">
        {state?.key !== current?.path ? (
          <LoadingState />
        ) : state?.error ? (
          <ErrorState error={state.error} />
        ) : state?.matrix ? (
          <MatrixChart
            label={`Confusion matrix at epoch ${current?.epoch ?? "?"}`}
            rows={state.matrix.labels}
            columns={state.matrix.labels}
            values={state.matrix.matrix}
            rowName="actual"
            columnName="predicted"
            className="h-full"
          />
        ) : null}
      </div>
    </div>
  );
}

/** A weights file opened from the project explorer. */
function ModelFile({ path }: { path: string }) {
  return (
    <>
      <PanelToolbar>
        <ToolbarTitle icon="brain">{basename(path)}</ToolbarTitle>
      </PanelToolbar>
      <ModelFileView key={path} path={path} />
    </>
  );
}

function ModelInspector() {
  const panel = usePanel("model-inspector");
  if (panel.params.path) return <ModelFile path={panel.params.path} />;
  return <RunModel />;
}

function RunModel() {
  const panel = usePanel("model-inspector");
  const [view] = useRunViews(panel.params.runDir, "focused");
  const toolbar = (
    <PanelToolbar>
      <ToolbarTitle icon="brain">{view ? basename(view.runDir) : ""}</ToolbarTitle>
      <span className="flex-1" />
      <RunFollowToggle pinned={panel.params.runDir} onChange={(runDir) => panel.setParams({ runDir })} />
    </PanelToolbar>
  );
  if (!view) {
    return (
      <EmptyState
        className="epoch-empty"
        icon="brain"
        title="No run focused"
        description="Select a run in Experiments, or open a .safetensors or .onnx file from the project."
      />
    );
  }
  if (view.state.status !== "ready") {
    return (
      <>
        {toolbar}
        {view.state.status === "error" ? <ErrorState error={view.state.error} /> : <LoadingState />}
      </>
    );
  }
  const d = view.state.detail;
  const r = d.record;
  return (
    <>
      {toolbar}
      <div className="nexis-scrollbar flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-3">
        <div className="flex flex-col gap-1">
          <SectionLabel>Run</SectionLabel>
          <PropertyList
            items={[
              { label: "Status", value: <span className="inline-flex items-center gap-1.5"><StatusDot tone={statusTone(r.status)} />{statusLabel(r.status)}</span> },
              { label: "Engine", value: r.engine ?? "not recorded", mono: true },
              { label: "Device", value: r.device ?? "—", mono: true },
              { label: "Epochs", value: `${r.lastEpoch ?? "?"} / ${r.totalEpochs ?? "?"}`, mono: true },
              { label: "Started", value: formatTimestamp(r.startedAt) },
              { label: "Finished", value: formatTimestamp(r.finishedAt) },
              { label: "Format", value: r.format, mono: true },
            ]}
          />
        </div>
        <ConfusionMatrices detail={d} />
        <div className="flex flex-col gap-1">
          <SectionLabel>Configuration snapshot</SectionLabel>
          <ConfigSections config={(d.config ?? null) as Json} />
        </div>
        <div className="flex flex-col gap-1">
          <SectionLabel>Checkpoints</SectionLabel>
          {d.checkpoints.length === 0 ? (
            <p className="px-1 text-xs text-muted-foreground">No checkpoint files.</p>
          ) : (
            <PropertyList items={d.checkpoints.map((c) => ({ label: c.name, value: formatBytes(c.size), mono: true }))} />
          )}
        </div>
        <PlannedNote>Architecture diagrams come with the model tooling milestone. Weight files (.safetensors, .onnx) already open here from the project explorer.</PlannedNote>
      </div>
    </>
  );
}

export const modelInspectorPanel = definePanel({
  type: "model-inspector",
  title: "Model",
  icon: "brain",
  description: "What a run trained (configuration, checkpoints, evaluation) or what a weights file holds.",
  component: ModelInspector,
  parseParams: (raw) => stringParams(raw, ["runDir", "path"] as const),
  tabTitle: (p) => (p.path ? basename(p.path) : p.runDir ? `Model · ${basename(p.runDir)}` : null),
  placement: "stage",
  maturity: "partial",
  limitations: "Reads run snapshots, confusion matrices and Safetensors/ONNX headers. Architecture diagrams are planned.",
});
