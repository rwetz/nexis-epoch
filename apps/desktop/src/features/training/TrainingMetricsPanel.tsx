import * as React from "react";
import { Badge, cn, Icon } from "@nexis/design";
import { SeriesChart, type XYSeries } from "@epoch/components/charts/SeriesChart";
import { PanelToolbar, ToolbarTitle } from "@epoch/components/PanelToolbar";
import { EmptyState, ErrorState, LoadingState } from "@epoch/components/states";
import { SeriesSwatch } from "@epoch/features/experiments/ExperimentExplorerPanel";
import { runLabel, useExperiments } from "@epoch/features/experiments/store";
import { useRunViews } from "@epoch/features/experiments/useRunDetails";
import { usePanel } from "@epoch/features/workspace/panel-context";
import { definePanel, stringParams } from "@epoch/features/workspace/registry";
import { basename } from "@epoch/lib/fileIcons";
import { RunFollowToggle } from "./RunFollow";

/** loss first, then accuracy, then everything else alphabetically. */
function metricOrder(a: string, b: string): number {
  const rank = (m: string) => (m.startsWith("loss") ? 0 : m.startsWith("acc") ? 1 : m.startsWith("perplexity") ? 2 : m.startsWith("mem") ? 4 : 3);
  return rank(a) - rank(b) || a.localeCompare(b);
}

function TrainingMetrics() {
  const panel = usePanel("training-metrics");
  const views = useRunViews(panel.params.runDir, "all");
  const runs = useExperiments((s) => s.runs);
  const [hidden, setHidden] = React.useState<Set<string>>(new Set());

  const ready = views.flatMap((v) => (v.state.status === "ready" ? [{ ...v, detail: v.state.detail }] : []));
  const metrics = React.useMemo(
    () => [...new Set(ready.flatMap((r) => r.detail.series.map((s) => s.name)))].sort(metricOrder),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [ready.map((r) => r.runDir).join("\n"), ready.length],
  );

  const toolbar = (
    <PanelToolbar>
      <ToolbarTitle icon="chart-line" meta={panel.params.runDir ? "pinned" : undefined}>
        {views.length === 1 ? basename(views[0].runDir) : `${views.length} runs`}
      </ToolbarTitle>
      <span className="flex-1" />
      <RunFollowToggle pinned={panel.params.runDir} onChange={(runDir) => panel.setParams({ runDir })} />
    </PanelToolbar>
  );

  if (views.length === 0) {
    return (
      <>
        <EmptyState icon="chart-line" title="No run selected" description="Select runs in Experiments. Tick several to compare their curves." />
      </>
    );
  }
  const failed = views.find((v) => v.state.status === "error");
  if (ready.length === 0) {
    return (
      <>
        {toolbar}
        {failed && failed.state.status === "error" ? <ErrorState error={failed.state.error} /> : <LoadingState label="Reading metrics…" />}
      </>
    );
  }

  const labelOf = (runDir: string) => {
    const run = runs.find((r) => r.runDir === runDir);
    return run ? runLabel(run) : basename(runDir);
  };

  return (
    <>
      {toolbar}
      <div className="flex shrink-0 flex-wrap items-center gap-1.5 border-b border-border/60 px-2 py-1.5">
        {ready.map((r) => (
          <span key={r.runDir} className="flex items-center gap-1.5 rounded-md bg-muted/60 px-1.5 py-0.5 font-mono text-[10.5px]" title={r.runDir}>
            <SeriesSwatch slot={r.slot} />
            {labelOf(r.runDir)}
          </span>
        ))}
        <span className="flex-1" />
        {metrics.map((m) => {
          const on = !hidden.has(m);
          return (
            <button
              key={m}
              type="button"
              aria-pressed={on}
              onClick={() =>
                setHidden((h) => {
                  const n = new Set(h);
                  if (n.has(m)) n.delete(m);
                  else n.add(m);
                  return n;
                })
              }
              className={cn(
                "rounded-md border px-1.5 py-0.5 font-mono text-[10.5px]",
                on ? "border-border bg-card text-foreground" : "border-dashed border-border/60 text-muted-foreground",
              )}
            >
              {m}
            </button>
          );
        })}
      </div>
      {metrics.length === 0 ? (
        <EmptyState icon="chart-line" title="No metrics recorded" description="These runs' metrics.jsonl has no metric events." />
      ) : (
        // Small multiples: as many columns as fit, every chart the same
        // landscape shape. Rows used to stretch to the panel's height, which
        // on a large screen made each chart a tall strip; auto-fill also
        // left empty tracks to the right once the panel was wide.
        <div className="epoch-content-enter nexis-scrollbar grid min-h-0 flex-1 grid-cols-[repeat(auto-fit,minmax(min(100%,320px),1fr))] content-start gap-2 overflow-y-auto p-2">
          {metrics
            .filter((m) => !hidden.has(m))
            .map((metric) => {
              const series: XYSeries[] = ready.flatMap((r) => {
                const s = r.detail.series.find((x) => x.name === metric);
                return s ? [{ label: labelOf(r.runDir), points: s.points, slot: r.slot }] : [];
              });
              const decimated = ready.some((r) => r.detail.series.find((x) => x.name === metric)?.decimated);
              return (
                <section
                  key={metric}
                  className="flex aspect-[16/10] max-h-[480px] min-h-[200px] flex-col rounded-lg border border-border/60 bg-card p-2"
                  aria-label={metric}
                >
                  <header className="flex items-center gap-1.5 px-1 pb-1">
                    <span className="font-mono text-[11.5px] font-medium">{metric}</span>
                    {decimated && (
                      <Badge variant="outline" className="h-4 px-1 text-[9px]" title="Long series are reduced to per-bucket min/max, so spikes survive.">
                        decimated
                      </Badge>
                    )}
                  </header>
                  <SeriesChart title={metric} series={series} legend={false} className="min-h-0 flex-1" />
                </section>
              );
            })}
        </div>
      )}
      {views.some((v) => v.state.status === "loading") && (
        <div className="flex shrink-0 items-center gap-1.5 border-t border-border/60 px-2 py-1 text-[10.5px] text-muted-foreground">
          <Icon name="loading" size="xs" /> Loading more runs…
        </div>
      )}
    </>
  );
}

export const trainingMetricsPanel = definePanel({
  type: "training-metrics",
  title: "Metrics",
  icon: "chart-line",
  description: "Every recorded metric as a curve, overlaid across the selected runs.",
  component: TrainingMetrics,
  parseParams: (raw) => stringParams(raw, ["runDir"] as const),
  tabTitle: (p) => (p.runDir ? `Metrics · ${basename(p.runDir)}` : null),
  placement: "side",
  maturity: "partial",
  limitations: "Historical runs from their metric logs. Live streaming from a running job comes with training execution.",
});
