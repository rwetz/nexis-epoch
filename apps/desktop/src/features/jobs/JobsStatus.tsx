/**
 * The jobs list in the status bar: a fact while something runs (the newest
 * job and its progress), and on click every job with its controls.
 */
import * as React from "react";
import { Button, CallChip, Icon, Popover, PopoverContent, PopoverTrigger, StatusDot, toast, Tooltip, TooltipContent, TooltipTrigger } from "@nexis/design";
import { openPanel } from "@epoch/app/store";
import { api } from "@epoch/lib/api";
import { errorMessage } from "@epoch/lib/ipc";
import type { JobInfo } from "@epoch/types/backend";
import { describeStatus, isActive, jobProgressLabel } from "./live";
import { useJobs } from "./store";

/** Show a job's output (and curves, for a training run). */
export function showJob(job: JobInfo): void {
  useJobs.getState().focus(job.id);
  openPanel("execution-logs", {}, { reuse: "type" });
  if (job.plan.speaksProtocol) openPanel("training-metrics", {}, { reuse: "type" });
}

async function act(what: string, fn: () => Promise<void>) {
  try {
    await fn();
  } catch (e) {
    toast.error(`Couldn't ${what}`, { description: errorMessage(e) });
  }
  void useJobs.getState().refresh().catch(() => {});
}

export function stopJob(job: JobInfo): void {
  // A second stop while one is pending kills at once.
  const force = job.status.state === "stopping";
  void act(force ? "kill the job" : "stop the job", () => api.jobStop(job.id, force));
}

export function togglePause(job: JobInfo): void {
  if (job.status.state === "paused") void act("resume the job", () => api.jobResume(job.id));
  else void act("pause the job", () => api.jobPause(job.id));
}

function useNow(active: boolean): number {
  const [now, setNow] = React.useState(() => Date.now());
  React.useEffect(() => {
    if (!active) return;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [active]);
  return now;
}

function RowButton({ icon, label, onClick }: { icon: "pause" | "play" | "stop" | "terminal"; label: string; onClick(): void }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant="ghost" size="icon-xs" aria-label={label} onClick={onClick}>
          <Icon name={icon} size="xs" />
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

function JobRow({ job, now }: { job: JobInfo; now: number }) {
  const active = isActive(job.status);
  const { label, tone } = describeStatus(job.status);
  return (
    <li className="flex min-w-0 items-center gap-2 rounded-md px-1.5 py-1 hover:bg-muted/50">
      {active && job.startedAtMs !== null ? (
        <CallChip
          label={job.title}
          startedAtMs={job.startedAtMs}
          onClick={() => showJob(job)}
          onEnd={() => stopJob(job)}
          endLabel={job.status.state === "stopping" ? "Kill" : "Stop"}
          className="min-w-0 shrink"
        />
      ) : (
        <button type="button" onClick={() => showJob(job)} className="flex min-w-0 items-center gap-2 text-left" title={job.plan.program}>
          <StatusDot tone={tone} />
          <span className="truncate text-xs">{job.title}</span>
        </button>
      )}
      <span className="flex-1" />
      <span className="shrink-0 font-mono text-[10.5px] text-muted-foreground tabular-nums" title={label}>
        {jobProgressLabel(job, now)}
      </span>
      {job.controllable && (job.status.state === "running" || job.status.state === "paused") && (
        <RowButton icon={job.status.state === "paused" ? "play" : "pause"} label={job.status.state === "paused" ? "Resume" : "Pause at the next epoch"} onClick={() => togglePause(job)} />
      )}
      {active && job.startedAtMs === null && <RowButton icon="stop" label="Cancel" onClick={() => stopJob(job)} />}
      <RowButton icon="terminal" label="Show output" onClick={() => showJob(job)} />
    </li>
  );
}

export function JobsStatus() {
  const jobs = useJobs((s) => s.jobs);
  const running = jobs.filter((j) => isActive(j.status));
  const now = useNow(running.length > 0);
  if (jobs.length === 0) return null;

  const lead = running[0] ?? jobs[0];
  const { tone } = describeStatus(lead.status);
  const more = running.length > 1 ? ` · +${running.length - 1}` : "";
  const summary = `${lead.title} · ${jobProgressLabel(lead, now)}${more}`;

  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          title={`${running.length} running · ${jobs.length} job${jobs.length === 1 ? "" : "s"} this session`}
          className="inline-flex h-5 max-w-72 items-center gap-1.5 rounded-md px-1.5 hover:bg-muted hover:text-foreground"
        >
          <StatusDot tone={tone} live={running.length > 0} />
          <span className="truncate font-mono tabular-nums">{summary}</span>
        </button>
      </PopoverTrigger>
      <PopoverContent align="end" side="top" className="w-[380px] p-1.5">
        <div className="flex items-center gap-1.5 px-1.5 pt-0.5 pb-1.5 text-xs font-medium">
          <Icon name="play" size="xs" className="text-muted-foreground" />
          Jobs
          <span className="font-mono text-[11px] font-normal text-muted-foreground tabular-nums">
            {running.length} running · {jobs.length} total
          </span>
        </div>
        <ul className="nexis-scrollbar max-h-80 space-y-0.5 overflow-y-auto">
          {jobs.map((job) => (
            <JobRow key={job.id} job={job} now={now} />
          ))}
        </ul>
      </PopoverContent>
    </Popover>
  );
}
