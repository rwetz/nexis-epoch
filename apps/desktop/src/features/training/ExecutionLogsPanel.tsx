import * as React from "react";
import { cn, Input, Segmented, VirtualList } from "@nexis/design";
import { PanelToolbar, ToolbarTitle } from "@epoch/components/PanelToolbar";
import { EmptyState, ErrorState, LoadingState } from "@epoch/components/states";
import { useRunViews } from "@epoch/features/experiments/useRunDetails";
import { JobOutput } from "@epoch/features/jobs/JobOutput";
import { isActive, jobForRun } from "@epoch/features/jobs/live";
import { useFocusedJob, useJobs } from "@epoch/features/jobs/store";
import { usePanel } from "@epoch/features/workspace/panel-context";
import { definePanel, stringParams } from "@epoch/features/workspace/registry";
import { basename } from "@epoch/lib/fileIcons";
import { RunFollowToggle } from "./RunFollow";

type Level = "all" | "warn" | "error";

const LEVEL_CLASS: Record<string, string> = {
  error: "text-destructive",
  warn: "text-warning",
  warning: "text-warning",
  debug: "text-muted-foreground",
};

function ExecutionLogs() {
  const panel = usePanel("execution-logs");
  const [view] = useRunViews(panel.params.runDir, "focused");
  const focusedJob = useFocusedJob();
  const jobs = useJobs((s) => s.jobs);
  const [level, setLevel] = React.useState<Level>("all");
  const [query, setQuery] = React.useState("");
  const [tab, setTab] = React.useState<"log" | "samples">("log");

  const detail = view?.state.status === "ready" ? view.state.detail : null;
  const lines = React.useMemo(() => {
    if (!detail) return [];
    const q = query.trim().toLowerCase();
    return detail.logs.filter((l) => {
      const lv = l.level.toLowerCase();
      if (level === "error" && lv !== "error") return false;
      if (level === "warn" && !["warn", "warning", "error"].includes(lv)) return false;
      return !q || l.msg.toLowerCase().includes(q);
    });
  }, [detail, level, query]);

  // Live output: the job the user is following (unless this panel is
  // pinned to a run), or the job writing the shown run right now.
  const runJob = view ? jobForRun(jobs, view.runDir) : undefined;
  const following = !panel.params.runDir ? focusedJob : undefined;
  const job = following ?? (runJob && isActive(runJob.status) ? runJob : undefined);
  if (job) {
    return <JobOutput job={job} onLeave={following ? () => useJobs.getState().focus(null) : undefined} />;
  }

  if (!view) {
    return <EmptyState icon="terminal" title="No run focused" description="Select a run in Experiments to read its log, or run something to follow its output." />;
  }

  return (
    <>
      <PanelToolbar>
        <ToolbarTitle icon="terminal" meta={detail ? lines.length : undefined}>
          {basename(view.runDir)}
        </ToolbarTitle>
        <span className="flex-1" />
        {detail && detail.samples.length > 0 && (
          <Segmented
            label="Log view"
            size="sm"
            value={tab}
            onChange={setTab}
            segments={[
              { id: "log", label: "Log" },
              { id: "samples", label: `Samples (${detail.samples.length})` },
            ]}
          />
        )}
        <RunFollowToggle pinned={panel.params.runDir} onChange={(runDir) => panel.setParams({ runDir })} />
      </PanelToolbar>
      {view.state.status === "loading" ? (
        <LoadingState />
      ) : view.state.status === "error" ? (
        <ErrorState error={view.state.error} />
      ) : tab === "samples" && detail ? (
        <div className="nexis-scrollbar min-h-0 flex-1 space-y-2 overflow-y-auto p-2">
          {detail.samples.map((s, i) => (
            <figure key={i} className="rounded-lg border border-border/60 bg-muted/30 p-2">
              <figcaption className="pb-1 font-mono text-[10px] text-muted-foreground">after epoch {s.epoch ?? "?"}</figcaption>
              <pre className="font-mono text-[11.5px] whitespace-pre-wrap">{s.output}</pre>
            </figure>
          ))}
        </div>
      ) : (
        <>
          <div className="flex shrink-0 items-center gap-1.5 border-b border-border/60 p-1.5">
            <Segmented
              label="Level"
              size="sm"
              value={level}
              onChange={setLevel}
              segments={[
                { id: "all", label: "All" },
                { id: "warn", label: "Warnings" },
                { id: "error", label: "Errors" },
              ]}
            />
            <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search log" aria-label="Search log" className="h-7 flex-1 text-xs" />
          </div>
          {lines.length === 0 ? (
            <EmptyState icon="terminal" title={detail?.logs.length ? "No lines match" : "No log lines"} description={detail?.logs.length ? undefined : "The run recorded no log events."} />
          ) : (
            <VirtualList
              label="Run log"
              count={lines.length}
              rowHeight={22}
              className="min-h-0 flex-1 font-mono text-[11.5px]"
              renderRow={(i) => {
                const l = lines[i];
                return (
                  <div className="flex h-full items-center gap-2 px-2">
                    <span className="w-10 shrink-0 text-right text-muted-foreground/70 tabular-nums">{l.epoch ?? ""}</span>
                    <span className={cn("w-11 shrink-0 uppercase", LEVEL_CLASS[l.level.toLowerCase()] ?? "text-info")}>{l.level}</span>
                    <span className="truncate" title={l.msg}>
                      {l.msg}
                    </span>
                  </div>
                );
              }}
            />
          )}
          {detail && (
            <div className="shrink-0 border-t border-border/60 px-2 py-1 font-mono text-[10.5px] text-muted-foreground tabular-nums">
              {detail.eventCount} events
              {detail.skippedLines > 0 && ` · ${detail.skippedLines} unreadable lines skipped`}
              {detail.logsTruncated && " · log truncated"}
            </div>
          )}
        </>
      )}
    </>
  );
}

export const executionLogsPanel = definePanel({
  type: "execution-logs",
  title: "Logs",
  icon: "terminal",
  description: "A run's log and generated samples, or a running job's live output.",
  component: ExecutionLogs,
  parseParams: (raw) => stringParams(raw, ["runDir"] as const),
  tabTitle: (p) => (p.runDir ? `Logs · ${basename(p.runDir)}` : null),
  placement: "side",
  maturity: "ready",
});
