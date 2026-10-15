import * as React from "react";
import { Badge, Checkbox, cn, Icon, Input, MOD_KEY, StatusDot, toast, Tooltip, TooltipContent, TooltipTrigger } from "@nexis/design";
import { PanelToolbar, ToolbarButton, ToolbarTitle } from "@epoch/components/PanelToolbar";
import { EmptyState, ErrorState, LoadingState } from "@epoch/components/states";
import { useApp } from "@epoch/app/store";
import { definePanel, noParams } from "@epoch/features/workspace/registry";
import { formatMetric } from "@epoch/lib/format";
import { errorMessage } from "@epoch/lib/ipc";
import type { IndexedRun } from "@epoch/types/backend";
import { MAX_SELECTED, runLabel, useExperiments } from "./store";
import { benchmarkInPulse, NEXIS_TOOLS, openInSynapse } from "@epoch/features/links/links";

export function statusTone(status: string): "success" | "danger" | "warning" | "neutral" {
  if (status === "ok") return "success";
  if (status === "error") return "danger";
  if (status === "cancelled") return "warning";
  return "neutral";
}

export function statusLabel(status: string): string {
  return { ok: "Finished", error: "Failed", cancelled: "Cancelled", unknown: "No summary" }[status] ?? status;
}

/** The metrics worth showing in a row: validation accuracy and loss first. */
function headline(run: IndexedRun): { name: string; value: string }[] {
  const pick = (name: string, key: "last" | "min") => {
    const m = run.metrics[name];
    return m ? { name: name.replace("/", " "), value: formatMetric(m[key]) } : null;
  };
  const out = [pick("acc/val", "last"), pick("loss/val", "min") ?? pick("loss/train", "min")].filter(Boolean) as {
    name: string;
    value: string;
  }[];
  if (out.length === 0) {
    const first = Object.keys(run.metrics)[0];
    if (first) out.push({ name: first, value: formatMetric(run.metrics[first].last) });
  }
  return out;
}

/** A small swatch in the run's series colour. */
export function SeriesSwatch({ slot }: { slot: number }) {
  return <span aria-hidden className="inline-block h-[3px] w-3 shrink-0 rounded-full" style={{ background: `var(--series-${(slot % 5) + 1})` }} />;
}

function ExperimentExplorer() {
  const project = useApp((s) => s.project);
  const { runs, status, error, selected, select, reload, reindex } = useExperiments();
  const [query, setQuery] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  // The first selected run is the focused one (see store.ts).
  const focused = runs.find((r) => r.runDir === selected[0]) ?? null;

  const shown = React.useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return runs;
    return runs.filter((r) =>
      [r.runId, r.name, r.status, r.engine, r.device, r.note, ...r.tags].some((f) => f?.toLowerCase().includes(q)),
    );
  }, [runs, query]);

  if (!project) return <EmptyState icon="chart-line" title="No project open" description="Runs appear here once a project with experiments is open." />;

  const doReindex = async () => {
    setBusy(true);
    try {
      toast.success("Experiments re-indexed", { description: await reindex() });
    } catch (e) {
      toast.error("Re-index failed", { description: errorMessage(e) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <PanelToolbar>
        <ToolbarTitle icon="chart-line" meta={runs.length || undefined}>
          Runs
        </ToolbarTitle>
        <span className="flex-1" />
        <ToolbarButton
          icon="brain"
          label={`Open the focused run in ${NEXIS_TOOLS.mlLab} (Nexis)`}
          onClick={() => focused && void openInSynapse(focused.projectDir, focused.runId)}
          disabled={!focused}
        />
        <ToolbarButton
          icon="activity"
          label={`Benchmark the focused run's project model in ${NEXIS_TOOLS.benchmark} (Nexis)`}
          onClick={() => focused && void benchmarkInPulse(focused.projectDir)}
          disabled={!focused}
        />
        <ToolbarButton icon="refresh" label="Re-index from run files" onClick={() => void doReindex()} disabled={busy} />
      </PanelToolbar>
      <div className="shrink-0 border-b border-border/60 p-1.5">
        <Input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Filter by id, status, device, tag…"
          aria-label="Filter runs"
          className="h-7 text-xs"
        />
      </div>
      <div className="nexis-scrollbar min-h-0 flex-1 overflow-y-auto">
        {status === "loading" && runs.length === 0 ? (
          <LoadingState />
        ) : status === "error" ? (
          <ErrorState error={error} onRetry={() => void reload()} />
        ) : runs.length === 0 ? (
          <EmptyState
            icon="chart-line"
            title="No runs found"
            description={
              <>
                Epoch reads Nexis ML run stores (<span className="font-mono">.nexis-ml/runs</span>) in this project and its subfolders. Train
                with <span className="font-mono">nexis-ml train</span>, then re-index.
              </>
            }
          />
        ) : shown.length === 0 ? (
          <EmptyState icon="search" title="No runs match" />
        ) : (
          <ul role="listbox" aria-label="Runs" aria-multiselectable className="flex flex-col p-1">
            {shown.map((run) => {
              const slot = selected.indexOf(run.runDir);
              const isSelected = slot >= 0;
              const full = !isSelected && selected.length >= MAX_SELECTED;
              return (
                <li
                  key={run.runDir}
                  role="option"
                  aria-selected={isSelected}
                  onClick={(e) => select(run.runDir, e.ctrlKey || e.metaKey ? "toggle" : "replace")}
                  className={cn(
                    "group flex cursor-pointer items-start gap-2 rounded-lg px-2 py-1.5 text-xs",
                    isSelected ? "bg-muted" : "hover:bg-muted/50",
                    slot === 0 && "ring-1 ring-brand/50",
                  )}
                >
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <span onClick={(e) => e.stopPropagation()} className="pt-0.5">
                        <Checkbox
                          checked={isSelected}
                          disabled={full}
                          onCheckedChange={() => select(run.runDir, "toggle")}
                          aria-label={`Compare ${run.runId}`}
                        />
                      </span>
                    </TooltipTrigger>
                    <TooltipContent>{full ? `Compare up to ${MAX_SELECTED} runs` : "Compare"}</TooltipContent>
                  </Tooltip>
                  <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                    <div className="flex min-w-0 items-center gap-1.5">
                      {isSelected && <SeriesSwatch slot={slot} />}
                      <StatusDot tone={statusTone(run.status)} />
                      <span className="truncate font-mono text-[11.5px] text-foreground" title={run.runDir}>
                        {runLabel(run)}
                      </span>
                      {run.pinned && <Icon name="pin" size={11} active className="shrink-0 text-brand" />}
                    </div>
                    <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-0.5 text-[10.5px] text-muted-foreground">
                      <span>{statusLabel(run.status)}</span>
                      {run.lastEpoch !== null && (
                        <span className="font-mono tabular-nums">
                          {run.lastEpoch}/{run.totalEpochs ?? "?"} ep
                        </span>
                      )}
                      {run.device && <span className="font-mono">{run.device}</span>}
                      {headline(run).map((m) => (
                        <span key={m.name} className="font-mono tabular-nums">
                          {m.name} <span className="text-foreground/80">{m.value}</span>
                        </span>
                      ))}
                    </div>
                    {(run.tags.length > 0 || run.note) && (
                      <div className="flex min-w-0 flex-wrap items-center gap-1">
                        {run.tags.map((t) => (
                          <Badge key={t} variant="outline" className="h-4 px-1 text-[9.5px]">
                            {t}
                          </Badge>
                        ))}
                        {run.note && <span className="truncate text-[10.5px] text-muted-foreground italic">{run.note}</span>}
                      </div>
                    )}
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </div>
      {runs.length > 0 && (
        <div className="shrink-0 border-t border-border/60 px-2.5 py-1 text-[10.5px] text-muted-foreground">
          Click to focus · {MOD_KEY}-click or tick to compare (up to {MAX_SELECTED})
        </div>
      )}
    </>
  );
}


export const experimentExplorerPanel = definePanel({
  type: "experiment-explorer",
  title: "Experiments",
  icon: "layers",
  description: "Runs imported from Nexis ML run stores, indexed in SQLite. Select runs to compare.",
  component: ExperimentExplorer,
  parseParams: noParams,
  placement: "side",
  singleton: true,
  maturity: "partial",
  limitations: "Reads Nexis ML run stores. Launching runs and other experiment formats are planned.",
});
