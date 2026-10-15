/**
 * The loading, empty and error states every panel uses, so a panel waiting
 * on DuckDB looks like one waiting on the filesystem.
 */
import * as React from "react";
import {
  Button,
  cn,
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
  Icon,
  type IconName,
  Spinner,
} from "@nexis/design";
import { BackendError } from "@epoch/lib/ipc";

export function LoadingState({ label = "Loading…", className }: { label?: string; className?: string }) {
  return (
    <div role="status" className={cn("flex h-full min-h-24 items-center justify-center gap-2 text-xs text-muted-foreground", className)}>
      <Spinner className="size-4" />
      {label}
    </div>
  );
}

export function EmptyState({
  icon,
  title,
  description,
  action,
  className,
}: {
  icon: IconName;
  title: string;
  description?: React.ReactNode;
  action?: React.ReactNode;
  className?: string;
}) {
  return (
    <Empty className={cn("epoch-empty h-full min-h-40 gap-3 border-0 p-6", className)}>
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <Icon name={icon} />
        </EmptyMedia>
        <EmptyTitle className="font-heading text-sm">{title}</EmptyTitle>
        {description && <EmptyDescription className="text-xs">{description}</EmptyDescription>}
      </EmptyHeader>
      {action && <EmptyContent>{action}</EmptyContent>}
    </Empty>
  );
}

/** Errors from the backend, with a retry when there is something to retry. */
export function ErrorState({
  error,
  onRetry,
  title,
  className,
}: {
  error: unknown;
  onRetry?: () => void;
  title?: string;
  className?: string;
}) {
  const unavailable = error instanceof BackendError && error.code === "unavailable";
  const message = error instanceof Error ? error.message : String(error);
  return (
    <EmptyState
      className={className}
      icon={unavailable ? "computer" : "alert-circle"}
      title={title ?? (unavailable ? "Desktop backend not connected" : "Something went wrong")}
      description={<span className="break-words font-mono text-[11px]">{message}</span>}
      action={
        onRetry && !unavailable ? (
          <Button size="sm" variant="outline" onClick={onRetry}>
            <Icon name="refresh" /> Try again
          </Button>
        ) : undefined
      }
    />
  );
}

/** A clearly labelled not-yet-built capability. Kept small on purpose. */
export function PlannedNote({ children, className }: { children: React.ReactNode; className?: string }) {
  return (
    <div className={cn("flex items-start gap-2 rounded-lg border border-dashed border-border px-3 py-2 text-[11.5px] text-muted-foreground", className)}>
      <Icon name="clock" size="xs" className="mt-0.5 shrink-0" />
      <span>
        <span className="font-medium text-foreground/80">Planned · </span>
        {children}
      </span>
    </div>
  );
}

type State<T> = { status: "idle" | "loading" } | { status: "ready"; data: T } | { status: "error"; error: unknown };

/**
 * Run an async loader when `deps` change, dropping results from loads that
 * were superseded. `reload()` runs it again.
 */
export function useAsync<T>(load: (() => Promise<T>) | null, deps: React.DependencyList) {
  const [state, setState] = React.useState<State<T>>({ status: load ? "loading" : "idle" });
  const generation = React.useRef(0);
  const run = React.useCallback(() => {
    const gen = ++generation.current;
    if (!load) {
      setState({ status: "idle" });
      return;
    }
    setState((s) => (s.status === "ready" ? s : { status: "loading" }));
    load().then(
      (data) => gen === generation.current && setState({ status: "ready", data }),
      (error: unknown) => gen === generation.current && setState({ status: "error", error }),
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  React.useEffect(run, [run]);
  return { state, reload: run };
}
