/**
 * The one way anything runs: `requestRun(request, action)` opens a dialog
 * showing the exact command (`job_plan`), and only its Run button starts
 * it (`job_start`, which refuses if the plan changed in between).
 * <RunDialogHost/> is rendered once at the root, like <ConfirmHost/>.
 */
import * as React from "react";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Badge,
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Field,
  Icon,
  Input,
  Spinner,
  toast,
} from "@nexis/design";
import { create } from "zustand";
import { openPanel } from "@epoch/app/store";
import { api } from "@epoch/lib/api";
import { BackendError, errorMessage } from "@epoch/lib/ipc";
import type { FoundProgram, JobRequest, PlannedJob, ProgramSource } from "@epoch/types/backend";
import { useJobs } from "./store";

type Pending = { request: JobRequest; action: string };

const useRunDialog = create<{ pending: Pending | null }>(() => ({ pending: null }));

/** Ask to run something. Nothing starts until the user confirms the plan. */
export function requestRun(request: JobRequest, action: string): void {
  useRunDialog.setState({ pending: { request, action } });
}

const SOURCE_LABEL: Record<ProgramSource, string> = {
  project_venv: "project environment",
  path: "PATH",
  nexis_managed: "installed by Nexis",
  settings: "Settings",
};

function sourceLabel(found: FoundProgram): string {
  return `${SOURCE_LABEL[found.source]}${found.legacy ? " · legacy nexis-ml" : ""}`;
}

type PlanState = { status: "loading" } | { status: "ready"; planned: PlannedJob } | { status: "error"; message: string };

export function RunDialogHost() {
  const pending = useRunDialog((s) => s.pending);
  const close = () => useRunDialog.setState({ pending: null });
  return (
    <Dialog open={pending !== null} onOpenChange={(open) => !open && close()}>
      {pending && <RunDialogBody key={JSON.stringify(pending.request)} pending={pending} onClose={close} />}
    </Dialog>
  );
}

function RunDialogBody({ pending, onClose }: { pending: Pending; onClose(): void }) {
  const isScript = pending.request.kind === "python_script";
  const [args, setArgs] = React.useState(pending.request.kind === "python_script" ? (pending.request.args ?? "") : "");
  const request = React.useMemo<JobRequest>(
    () => (pending.request.kind === "python_script" ? { ...pending.request, args } : pending.request),
    [pending.request, args],
  );
  const [plan, setPlan] = React.useState<PlanState>({ status: "loading" });
  const [starting, setStarting] = React.useState(false);
  const [notice, setNotice] = React.useState<string | null>(null);
  const [replan, setReplan] = React.useState(0);

  // Re-plan as the arguments change (debounced); the plan shown is always
  // the backend's own, never one assembled here.
  React.useEffect(() => {
    let alive = true;
    const id = setTimeout(
      () => {
        api.jobPlan(request).then(
          (planned) => alive && setPlan({ status: "ready", planned }),
          (e: unknown) => alive && setPlan({ status: "error", message: errorMessage(e) }),
        );
      },
      isScript ? 250 : 0,
    );
    return () => {
      alive = false;
      clearTimeout(id);
    };
  }, [request, isScript, replan]);

  const run = async () => {
    if (plan.status !== "ready") return;
    setStarting(true);
    setNotice(null);
    try {
      const job = await api.jobStart(request, plan.planned.plan, pending.action);
      useJobs.getState().started(job);
      onClose();
      toast.success(`Started ${job.title}`, { description: job.status.state === "queued" ? "Queued until a running job finishes." : undefined });
      openPanel("execution-logs", {}, { reuse: "type" });
      if (job.plan.speaksProtocol) openPanel("training-metrics", {}, { reuse: "type" });
    } catch (e) {
      if (e instanceof BackendError && e.code === "conflict") {
        setNotice(e.message);
        setPlan({ status: "loading" });
        setReplan((n) => n + 1);
      } else {
        setNotice(errorMessage(e));
      }
    } finally {
      setStarting(false);
    }
  };

  const planned = plan.status === "ready" ? plan.planned : null;
  return (
    <DialogContent className="sm:max-w-xl">
      <DialogHeader>
        <DialogTitle>{planned?.title ?? (isScript ? "Run a Python script" : "Train with Axon")}</DialogTitle>
        <DialogDescription>
          Epoch will start this process on your computer, with your permissions. Nothing runs until you press Run.
        </DialogDescription>
      </DialogHeader>

      <div className="flex min-w-0 flex-col gap-3">
        {isScript && (
          <Field label="Arguments" hint="Split like a shell would, without expanding anything: no variables, globs or pipes.">
            <Input
              value={args}
              onChange={(e) => {
                setArgs(e.target.value);
                setPlan({ status: "loading" });
              }}
              placeholder="--epochs 3"
              className="font-mono text-xs"
              spellCheck={false}
            />
          </Field>
        )}

        {plan.status === "error" ? (
          <Alert variant="destructive">
            <Icon name="alert-circle" />
            <AlertTitle>Can't run this</AlertTitle>
            <AlertDescription>{plan.message}</AlertDescription>
          </Alert>
        ) : (
          <section aria-label="Command" className="flex min-w-0 flex-col gap-2">
            <div className="text-xs font-medium text-muted-foreground">Command</div>
            <pre
              className={`nexis-scrollbar max-h-32 overflow-auto rounded-lg border border-border/60 bg-muted/40 p-2.5 font-mono text-[11.5px] break-all whitespace-pre-wrap select-text ${planned ? "epoch-content-enter" : "text-muted-foreground"}`}
            >
              {planned?.commandLine ?? "Working out the command…"}
            </pre>
            {planned && (
              <dl className="epoch-content-enter grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1.5 text-xs">
                <dt className="text-muted-foreground">Program</dt>
                <dd className="flex min-w-0 items-center gap-1.5">
                  <span className="truncate font-mono text-[11px]" title={planned.program.path}>
                    {planned.program.path}
                  </span>
                  <Badge variant="outline" className="h-4 shrink-0 px-1 text-[9.5px]">
                    {sourceLabel(planned.program)}
                  </Badge>
                </dd>
                <dt className="text-muted-foreground">Folder</dt>
                <dd className="truncate font-mono text-[11px]" title={planned.plan.cwd}>
                  {planned.plan.cwd}
                </dd>
                <dt className="text-muted-foreground">Environment</dt>
                <dd className="min-w-0 font-mono text-[11px]">
                  {planned.plan.env.length === 0 ? (
                    <span className="text-muted-foreground">inherited</span>
                  ) : (
                    <span className="break-all">
                      {planned.plan.env.map(([k, v]) => `${k}=${v}`).join("  ")}
                      <span className="text-muted-foreground"> + inherited</span>
                    </span>
                  )}
                </dd>
                <dt className="text-muted-foreground">Output</dt>
                <dd>
                  {planned.plan.speaksProtocol
                    ? "Nexis ML protocol: live metrics, pause, and a graceful stop that saves the run."
                    : "Plain output, shown in Logs. Stopping ends the process."}
                </dd>
                {planned.alternatives.length > 0 && (
                  <>
                    <dt className="text-muted-foreground">Also found</dt>
                    <dd className="min-w-0 space-y-0.5">
                      {planned.alternatives.map((a) => (
                        <div key={a.path} className="truncate font-mono text-[11px] text-muted-foreground" title={a.path}>
                          {a.path} <span className="font-sans">({sourceLabel(a)})</span>
                        </div>
                      ))}
                    </dd>
                  </>
                )}
              </dl>
            )}
          </section>
        )}

        {notice && (
          <Alert>
            <Icon name="info" />
            <AlertDescription>{notice}</AlertDescription>
          </Alert>
        )}
      </div>

      <DialogFooter>
        <Button variant="ghost" onClick={onClose}>
          Cancel
        </Button>
        <Button variant="brand" disabled={!planned || starting} onClick={() => void run()}>
          {starting ? <Spinner /> : <Icon name="play" />} Run
        </Button>
      </DialogFooter>
    </DialogContent>
  );
}
