import { Button, cn, Icon, type IconName, SceneEnter, SectionLabel, Stat } from "@nexis/design";
import { AppLogo } from "@epoch/AppLogo";
import { openFile } from "@epoch/features/projects/ProjectExplorerPanel";
import { chooseAndOpenProject, openProject } from "@epoch/features/projects/actions";
import { useExperiments } from "@epoch/features/experiments/store";
import { WORKSPACES } from "@epoch/features/workspace/presets";
import { fileIcon } from "@epoch/lib/fileIcons";
import { formatBytes, formatRelativeTime } from "@epoch/lib/format";
import { useApp } from "../store";

function Card({ children, className }: { children: React.ReactNode; className?: string }) {
  return <section className={cn("rounded-xl border border-border/60 bg-card p-4", className)}>{children}</section>;
}

function ListRow({ icon, label, meta, onClick }: { icon: IconName; label: string; meta?: string; onClick(): void }) {
  return (
    <button type="button" onClick={onClick} className="flex w-full min-w-0 items-center gap-2 rounded-md px-2 py-1.5 text-left text-xs hover:bg-muted/60">
      <Icon name={icon} size="xs" className="shrink-0 text-muted-foreground" />
      <span className="truncate font-mono">{label}</span>
      {meta && <span className="ml-auto shrink-0 font-mono text-[10.5px] text-muted-foreground">{meta}</span>}
    </button>
  );
}

function Welcome() {
  const recent = useApp((s) => s.recent);
  const busy = useApp((s) => s.projectBusy);
  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-8 px-8 py-14">
      <div className="flex items-center gap-4">
        <AppLogo className="size-12 text-brand" />
        <div>
          <h1 className="font-heading text-3xl font-semibold tracking-tight">Nexis Epoch</h1>
          <p className="text-sm text-muted-foreground">Build. Train. Discover.</p>
        </div>
      </div>
      <p className="max-w-xl text-sm leading-relaxed text-muted-foreground">
        A local research studio for your own models and data. Open any repository as it is: Epoch reads its datasets, configuration and Nexis ML runs,
        and never runs its code unless you ask.
      </p>
      <div className="flex gap-2">
        <Button variant="brand" onClick={() => void chooseAndOpenProject()} disabled={busy}>
          <Icon name="folder-open" /> {busy ? "Opening…" : "Open project folder"}
        </Button>
      </div>
      {recent.length > 0 && (
        <div className="flex flex-col gap-1">
          <SectionLabel>Recent</SectionLabel>
          {recent.map((r) => (
            <button
              key={r.path}
              type="button"
              disabled={!r.exists || busy}
              onClick={() => void openProject(r.path)}
              className="flex items-center gap-3 rounded-lg px-3 py-2 text-left hover:bg-muted/60 disabled:opacity-50"
            >
              <Icon name="folder" className="text-muted-foreground" />
              <span className="flex min-w-0 flex-col">
                <span className="text-sm">{r.name}</span>
                <span className="truncate font-mono text-[11px] text-muted-foreground">{r.path}</span>
              </span>
              <span className="ml-auto shrink-0 text-[11px] text-muted-foreground">{r.exists ? formatRelativeTime(r.lastOpenedMs) : "missing"}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

export function OverviewPage() {
  const project = useApp((s) => s.project);
  const goTo = useApp((s) => s.goTo);
  const runs = useExperiments((s) => s.runs);
  if (!project) {
    return (
      <SceneEnter replayKey="welcome" className="h-full overflow-y-auto">
        <Welcome />
      </SceneEnter>
    );
  }
  const finished = runs.filter((r) => r.status === "ok").length;
  return (
    <SceneEnter replayKey={project.root} className="nexis-scrollbar h-full overflow-y-auto">
      <div className="mx-auto flex w-full max-w-6xl flex-col gap-5 px-6 py-6">
        <header className="flex flex-wrap items-end gap-3">
          <div className="min-w-0">
            <h1 className="font-heading text-2xl font-semibold tracking-tight">{project.packageName ?? project.name}</h1>
            <p className="truncate font-mono text-xs text-muted-foreground">{project.root}</p>
          </div>
          <span className="flex-1" />
          <Button size="sm" variant="outline" onClick={() => goTo({ kind: "workspace", workspace: "studio" })}>
            <Icon name="grid" /> Open Studio
          </Button>
        </header>

        <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
          <Card><Stat label="Datasets" value={String(project.datasets.length)} caption="CSV · Parquet · JSON" /></Card>
          <Card><Stat label="Runs" value={String(runs.length)} caption={`${finished} finished`} /></Card>
          <Card><Stat label="Configs" value={String(project.trainConfigs.length)} caption={`${project.tomlFiles} TOML files`} /></Card>
          <Card><Stat label="Python" value={String(project.pythonFiles)} caption={`${project.notebooks} notebooks · ${project.environments.length} envs`} /></Card>
        </div>

        <div className="grid gap-3 lg:grid-cols-3">
          <Card className="flex min-h-0 flex-col gap-1">
            <SectionLabel>Datasets</SectionLabel>
            {project.datasets.length === 0 && <p className="px-2 text-xs text-muted-foreground">None found within four levels of the root.</p>}
            {project.datasets.slice(0, 8).map((d) => (
              <ListRow key={d.path} icon={fileIcon(d.kind)} label={d.path} meta={formatBytes(d.size)} onClick={() => openFile(d)} />
            ))}
          </Card>
          <Card className="flex min-h-0 flex-col gap-1">
            <SectionLabel>Configuration</SectionLabel>
            {project.hasPyproject && <ListRow icon="settings" label="pyproject.toml" onClick={() => openFile({ path: "pyproject.toml", kind: "toml" })} />}
            {project.trainConfigs.slice(0, 7).map((p) => (
              <ListRow key={p} icon="settings" label={p} onClick={() => openFile({ path: p, kind: "toml" })} />
            ))}
            {project.requirements.map((p) => (
              <div key={p} className="flex items-center gap-2 px-2 py-1.5 text-xs text-muted-foreground">
                <Icon name="list-bullet" size="xs" /> <span className="truncate font-mono">{p}</span>
                <span className="ml-auto text-[10.5px]">dependencies</span>
              </div>
            ))}
            {!project.hasPyproject && project.trainConfigs.length === 0 && project.requirements.length === 0 && (
              <p className="px-2 text-xs text-muted-foreground">No pyproject.toml, train.toml or requirements files.</p>
            )}
          </Card>
          <Card className="flex min-h-0 flex-col gap-1">
            <SectionLabel>Experiments</SectionLabel>
            {project.experimentStores.length === 0 && <p className="px-2 text-xs text-muted-foreground">No Nexis ML run stores found.</p>}
            {project.experimentStores.map((s) => (
              <ListRow
                key={s.runsDir}
                icon="chart-line"
                label={s.projectDir || "."}
                meta={`${s.runCount} runs`}
                onClick={() => goTo({ kind: "workspace", workspace: "experiments" })}
              />
            ))}
            {project.environments.length > 0 && <SectionLabel className="pt-2">Environments</SectionLabel>}
            {project.environments.map((e) => (
              <ListRow key={e.path} icon="brand-python" label={e.path} meta={e.version ?? undefined} onClick={() => goTo({ kind: "page", page: "projects" })} />
            ))}
          </Card>
        </div>

        <div className="flex flex-col gap-2">
          <SectionLabel>Workspaces</SectionLabel>
          <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-5">
            {WORKSPACES.map((w) => (
              <button
                key={w.id}
                type="button"
                onClick={() => goTo({ kind: "workspace", workspace: w.id })}
                className="flex flex-col gap-1.5 rounded-xl border border-border/60 bg-card p-3 text-left hover:border-border hover:bg-muted/40"
              >
                <Icon name={w.icon} className="text-brand" />
                <span className="text-sm font-medium">{w.name}</span>
                <span className="text-[11.5px] leading-snug text-muted-foreground">{w.description}</span>
              </button>
            ))}
          </div>
        </div>
        {project.truncated && (
          <p className="text-[11px] text-muted-foreground">
            This project is large: detection stopped after 20,000 entries or four folder levels. Everything is still reachable from the project explorer.
          </p>
        )}
      </div>
    </SceneEnter>
  );
}
