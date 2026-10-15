/**
 * A job's live output in the Logs panel: stdout, stderr, the protocol's log
 * events and the job's own status changes, following the tail.
 */
import * as React from "react";
import { Badge, cn, Input, Segmented, StatusDot, VirtualList } from "@nexis/design";
import { PanelToolbar, ToolbarButton, ToolbarTitle } from "@epoch/components/PanelToolbar";
import { EmptyState } from "@epoch/components/states";
import type { JobInfo } from "@epoch/types/backend";
import { stopJob, togglePause } from "./JobsStatus";
import { describeStatus, emptyLive, isActive, type LiveLine } from "./live";
import { useJobs } from "./store";

type Filter = "all" | "problems";

const STREAM_CLASS: Record<string, string> = {
  err: "text-warning",
  error: "text-destructive",
  warn: "text-warning",
  warning: "text-warning",
  debug: "text-muted-foreground",
  out: "text-muted-foreground",
  job: "text-brand",
  run: "text-brand",
};

function isProblem(l: LiveLine): boolean {
  const lv = l.level.toLowerCase();
  return l.stream === "err" || lv === "error" || lv === "warn" || lv === "warning";
}

export function JobOutput({ job, onLeave }: { job: JobInfo; onLeave?: () => void }) {
  const live = useJobs((s) => s.live[job.id]) ?? emptyLive();
  const [filter, setFilter] = React.useState<Filter>("all");
  const [query, setQuery] = React.useState("");
  const lines = React.useMemo(() => {
    const q = query.trim().toLowerCase();
    return live.lines.filter((l) => (filter === "all" || isProblem(l)) && (!q || l.text.toLowerCase().includes(q)));
  }, [live.lines, filter, query]);
  const { label, tone } = describeStatus(job.status);
  const active = isActive(job.status);
  const canPause = job.controllable && (job.status.state === "running" || job.status.state === "paused");

  return (
    <>
      <PanelToolbar>
        <ToolbarTitle icon="terminal" meta={lines.length}>
          {job.title}
        </ToolbarTitle>
        <span className="flex shrink-0 items-center gap-1.5 text-[11px] text-muted-foreground" title={label}>
          <StatusDot tone={tone} live={active} />
          <span className="max-w-40 truncate">{label}</span>
        </span>
        <span className="flex-1" />
        {canPause && (
          <ToolbarButton
            icon={job.status.state === "paused" ? "play" : "pause"}
            label={job.status.state === "paused" ? "Resume" : "Pause at the next epoch"}
            onClick={() => togglePause(job)}
          />
        )}
        {active && (
          <ToolbarButton
            icon="stop"
            label={job.status.state === "stopping" ? "Kill now" : job.controllable ? "Stop (the run is saved)" : "Stop"}
            onClick={() => stopJob(job)}
          />
        )}
        {onLeave && <ToolbarButton icon="close" label="Back to the selected run's log" onClick={onLeave} />}
      </PanelToolbar>
      <div className="flex shrink-0 items-center gap-1.5 border-b border-border/60 p-1.5">
        <Segmented
          label="Lines"
          size="sm"
          value={filter}
          onChange={setFilter}
          segments={[
            { id: "all", label: "All" },
            { id: "problems", label: "Warnings & errors" },
          ]}
        />
        <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search output" aria-label="Search output" className="h-7 flex-1 text-xs" />
      </div>
      {lines.length === 0 ? (
        <EmptyState
          icon="terminal"
          title={live.lines.length ? "No lines match" : active ? "Waiting for output" : "No output"}
          description={live.lines.length || !active ? undefined : "Lines appear here as the process prints them."}
        />
      ) : (
        <VirtualList
          label="Job output"
          count={lines.length}
          rowHeight={22}
          follow={active}
          className="min-h-0 flex-1 font-mono text-[11.5px]"
          renderRow={(i) => {
            const l = lines[i];
            return (
              <div className="flex h-full items-center gap-2 px-2">
                <span className="w-10 shrink-0 text-right text-muted-foreground/70 tabular-nums">{l.epoch ?? ""}</span>
                <span className={cn("w-11 shrink-0 uppercase", STREAM_CLASS[l.level.toLowerCase()] ?? "text-info")}>{l.level}</span>
                <span className={cn("truncate", l.stream === "job" && "text-muted-foreground")} title={l.text}>
                  {l.text}
                </span>
              </div>
            );
          }}
        />
      )}
      <div className="flex shrink-0 items-center gap-1.5 border-t border-border/60 px-2 py-1 font-mono text-[10.5px] text-muted-foreground tabular-nums">
        <span className="truncate" title={job.plan.program}>
          {job.outputLines} lines
          {live.dropped > 0 && ` · ${live.dropped} oldest not kept`}
          {job.progress.runId && ` · run ${job.progress.runId}`}
        </span>
        <span className="flex-1" />
        {job.status.state === "exited" && (
          <Badge variant={job.status.code === 0 ? "success" : "destructive"} className="h-4 px-1 text-[9.5px]">
            exit {job.status.code ?? "signal"}
          </Badge>
        )}
      </div>
    </>
  );
}
