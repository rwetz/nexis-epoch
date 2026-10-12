import * as React from "react";
import { StatusBar, StatusDot, StatusItem } from "@nexis/design";
import { api } from "@/lib/api";
import { BACKEND_AVAILABLE } from "@/lib/ipc";
import type { SystemStatus } from "@/types/backend";
import { useApp } from "./store";

const POLL_MS = 3_000;

function useSystemStatus(): SystemStatus | null {
  const [status, setStatus] = React.useState<SystemStatus | null>(null);
  React.useEffect(() => {
    if (!BACKEND_AVAILABLE) return;
    let alive = true;
    const tick = () => api.systemStatus().then((s) => alive && setStatus(s), () => {});
    tick();
    const id = setInterval(tick, POLL_MS);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, []);
  return status;
}

/** Facts about the app's state, never controls (clicks are shortcuts). */
export function StatusFacts() {
  const project = useApp((s) => s.project);
  const settings = useApp((s) => s.settings);
  const goTo = useApp((s) => s.goTo);
  const sys = useSystemStatus();

  const env = project?.environments[0];
  const python = settings?.python.interpreter
    ? { label: "Python (configured)", title: settings.python.interpreter }
    : env
      ? { label: `Python ${env.version ?? ""} · ${env.path || "."}`.replace("  ", " "), title: env.interpreter ?? env.path }
      : { label: "No Python environment", title: "No virtual environment detected in this project. Set an interpreter in Settings." };
  const gpu = sys?.gpus[0];

  return (
    <StatusBar
      left={
        <>
          <StatusItem icon="folder" onClick={() => goTo({ kind: "page", page: "projects" })} title={project?.root ?? "No project open"}>
            {project?.name ?? "No project"}
          </StatusItem>
          {project && (
            <StatusItem icon="brand-python" title={python.title} onClick={() => goTo({ kind: "page", page: "settings" })}>
              {python.label}
            </StatusItem>
          )}
        </>
      }
      right={
        <>
          <StatusItem
            title={
              BACKEND_AVAILABLE
                ? "Running DuckDB queries. Training jobs arrive with the runtime milestone."
                : "No backend in a browser preview"
            }
            live
          >
            {sys ? `${sys.runningQueries} running` : "—"}
          </StatusItem>
          {sys && (
            <StatusItem icon="cpu" live title={`${sys.cpuCount} logical CPUs · ${sys.memoryUsedMb} / ${sys.memoryTotalMb} MB RAM`}>
              {sys.cpuPercent.toFixed(0)}% · {Math.round((sys.memoryUsedMb / Math.max(1, sys.memoryTotalMb)) * 100)}% mem
            </StatusItem>
          )}
          {gpu && (
            <StatusItem icon="flash" live title={`${gpu.name} · ${gpu.memoryUsedMb ?? "?"} / ${gpu.memoryTotalMb ?? "?"} MB`}>
              GPU {gpu.utilizationPercent?.toFixed(0) ?? "?"}% · {gpu.memoryUsedMb ?? "?"} MB
            </StatusItem>
          )}
          <StatusItem title={BACKEND_AVAILABLE ? "Rust backend connected" : "Browser preview: the Rust backend is only available in the desktop app"}>
            <span className="inline-flex items-center gap-1.5">
              <StatusDot tone={BACKEND_AVAILABLE ? "success" : "warning"} />
              {BACKEND_AVAILABLE ? "Backend" : "Preview"}
            </span>
          </StatusItem>
        </>
      }
    />
  );
}
