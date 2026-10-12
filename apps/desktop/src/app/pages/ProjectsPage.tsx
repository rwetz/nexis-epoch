import { Button, Icon, PropertyList, SectionLabel, toast } from "@nexis/design";
import { EmptyState } from "@/components/states";
import { chooseAndOpenProject, closeProject, openProject, refreshRecent, rescanProject } from "@/features/projects/actions";
import { api } from "@/lib/api";
import { formatRelativeTime } from "@/lib/format";
import { errorMessage } from "@/lib/ipc";
import { useApp } from "../store";

export function ProjectsPage() {
  const project = useApp((s) => s.project);
  const recent = useApp((s) => s.recent);
  const busy = useApp((s) => s.projectBusy);

  return (
    <div className="nexis-scrollbar h-full overflow-y-auto">
      <div className="mx-auto flex w-full max-w-4xl flex-col gap-6 px-6 py-6">
        <header className="flex items-center gap-2">
          <h1 className="font-heading text-xl font-semibold tracking-tight">Projects</h1>
          <span className="flex-1" />
          <Button size="sm" variant="brand" disabled={busy} onClick={() => void chooseAndOpenProject()}>
            <Icon name="folder-open" /> Open folder…
          </Button>
        </header>

        {project && (
          <section className="flex flex-col gap-3 rounded-xl border border-border/60 bg-card p-4">
            <div className="flex items-center gap-2">
              <Icon name="folder-open" className="text-brand" />
              <span className="font-medium">{project.name}</span>
              <span className="text-xs text-muted-foreground">open</span>
              <span className="flex-1" />
              <Button size="xs" variant="ghost" onClick={() => void rescanProject()}>
                <Icon name="refresh" /> Rescan
              </Button>
              <Button size="xs" variant="ghost" onClick={() => void closeProject()}>
                <Icon name="close" /> Close
              </Button>
            </div>
            <PropertyList
              items={[
                { label: "Folder", value: project.root, mono: true },
                { label: "Package", value: project.packageName ?? "—", mono: true },
                { label: "pyproject.toml", value: project.hasPyproject ? "yes" : "no" },
                { label: "Requirements", value: project.requirements.join(", ") || "—", mono: true },
                {
                  label: "Environments",
                  value: project.environments.map((e) => `${e.path} (${e.version ?? "unknown version"})`).join(", ") || "none detected",
                  mono: true,
                },
                { label: "Checkpoint folders", value: project.checkpointDirs.join(", ") || "—", mono: true },
                { label: "Run stores", value: project.experimentStores.map((s) => `${s.runsDir} · ${s.runCount}`).join(", ") || "—", mono: true },
              ]}
            />
            <p className="text-[11px] text-muted-foreground">
              Detection only reads files. Interpreter versions come from <span className="font-mono">pyvenv.cfg</span>; nothing in the project is executed.
            </p>
          </section>
        )}

        <section className="flex flex-col gap-1">
          <SectionLabel>Recent projects</SectionLabel>
          {recent.length === 0 ? (
            <EmptyState icon="clock" title="No recent projects" className="min-h-28" />
          ) : (
            recent.map((r) => (
              <div key={r.path} className="group flex items-center gap-3 rounded-lg px-3 py-2 hover:bg-muted/50">
                <Icon name="folder" className="text-muted-foreground" />
                <div className="flex min-w-0 flex-col">
                  <span className="text-sm">{r.name}</span>
                  <span className="truncate font-mono text-[11px] text-muted-foreground">{r.path}</span>
                </div>
                <span className="ml-auto shrink-0 text-[11px] text-muted-foreground">{r.exists ? formatRelativeTime(r.lastOpenedMs) : "folder missing"}</span>
                <Button size="xs" variant="outline" disabled={!r.exists || busy || r.path === project?.root} onClick={() => void openProject(r.path)}>
                  Open
                </Button>
                <Button
                  size="icon-xs"
                  variant="ghost"
                  aria-label={`Forget ${r.name}`}
                  onClick={async () => {
                    try {
                      await api.projectForget(r.path);
                      await refreshRecent();
                    } catch (e) {
                      toast.error("Couldn't forget the project", { description: errorMessage(e) });
                    }
                  }}
                >
                  <Icon name="close" />
                </Button>
              </div>
            ))
          )}
          <p className="px-3 pt-1 text-[11px] text-muted-foreground">Forgetting removes Epoch's record and index of a project. Its files are untouched.</p>
        </section>
      </div>
    </div>
  );
}
