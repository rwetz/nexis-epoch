import * as React from "react";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  fmtShortcut,
  Icon,
  MOD_KEY,
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
  Spinner,
  toast,
} from "@nexis/design";
import { DataGrid } from "@epoch/components/DataGrid";
import { CodeEditor } from "@epoch/components/editor/CodeEditor";
import { PanelToolbar, ToolbarTitle } from "@epoch/components/PanelToolbar";
import { EmptyState } from "@epoch/components/states";
import { useApp } from "@epoch/app/store";
import { usePanel } from "@epoch/features/workspace/panel-context";
import { definePanel, stringParams } from "@epoch/features/workspace/registry";
import { api } from "@epoch/lib/api";
import { basename, fileIcon } from "@epoch/lib/fileIcons";
import { formatCount, formatDuration } from "@epoch/lib/format";
import { BackendError, errorMessage, newQueryId } from "@epoch/lib/ipc";
import type { QueryResult } from "@epoch/types/backend";

const STARTER = `-- Ctrl/⌘+Enter runs the statement.
-- Pick a dataset from "Datasets" to query it by name.
SELECT 42 AS answer;
`;

function SqlEditor() {
  const panel = usePanel("sql-editor");
  const project = useApp((s) => s.project);
  const settings = useApp((s) => s.settings);
  const [sql, setSql] = React.useState(panel.params.sql ?? STARTER);
  const [result, setResult] = React.useState<QueryResult | null>(null);
  const [error, setError] = React.useState<unknown>(null);
  const [running, setRunning] = React.useState<string | null>(null);
  // View name → column names, for completion.
  const [schema, setSchema] = React.useState<Record<string, string[]>>({});

  // The query text is small and saved with the layout, so it survives a restart.
  React.useEffect(() => {
    const id = setTimeout(() => panel.setParams({ sql }), 500);
    return () => clearTimeout(id);
  }, [sql]); // eslint-disable-line react-hooks/exhaustive-deps

  // A different project means a different database: forget its views.
  React.useEffect(() => setSchema({}), [project?.root]);

  const run = async () => {
    if (running || !sql.trim()) return;
    const id = newQueryId("sql");
    setRunning(id);
    setError(null);
    try {
      setResult(await api.sqlRun(sql, id));
    } catch (e) {
      setResult(null);
      setError(e);
    } finally {
      setRunning(null);
    }
  };

  const addDataset = async (path: string) => {
    try {
      const name = await api.sqlRegisterView(path, basename(path).replace(/\.[^.]+$/, ""));
      const info = await api.datasetDescribe(path);
      setSchema((s) => ({ ...s, [name]: info.columns.map((c) => c.name) }));
      if (!sql.includes(name)) setSql((s) => `${s.trimEnd()}\n\nSELECT *\nFROM ${name}\nLIMIT 100;\n`);
      toast.success(`${name} is queryable`, { description: `A view over ${path}` });
    } catch (e) {
      toast.error("Couldn't add the dataset", { description: errorMessage(e) });
    }
  };

  if (!project) return <EmptyState icon="code" title="No project open" description="SQL runs against the open project's datasets." />;

  return (
    <>
      <PanelToolbar>
        <ToolbarTitle icon="code">SQL</ToolbarTitle>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button size="xs" variant="ghost">
              <Icon name="database" /> Datasets <Icon name="chevron-down" size={10} />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="max-h-80 w-72 overflow-y-auto">
            <DropdownMenuLabel className="text-[11px]">Query a project file by name</DropdownMenuLabel>
            <DropdownMenuSeparator />
            {project.datasets.length === 0 && <DropdownMenuItem disabled>No datasets found</DropdownMenuItem>}
            {project.datasets.slice(0, 60).map((d) => (
              <DropdownMenuItem key={d.path} onSelect={() => void addDataset(d.path)}>
                <Icon name={fileIcon(d.kind)} /> <span className="truncate font-mono text-xs">{d.path}</span>
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
        <span className="flex-1" />
        <span className="hidden text-[10.5px] text-muted-foreground md:inline">
          up to {formatCount(settings?.data.queryRowLimit ?? 10_000)} rows · {settings?.data.queryTimeoutSecs ?? 60}s timeout
        </span>
        {running ? (
          <Button size="xs" variant="outline" onClick={() => void api.queryCancel(running)}>
            <Icon name="stop" /> Cancel
          </Button>
        ) : (
          <Button size="xs" variant="brand" onClick={() => void run()} title={fmtShortcut(MOD_KEY, "Enter")}>
            <Icon name="play" /> Run
          </Button>
        )}
      </PanelToolbar>
      <ResizablePanelGroup orientation="vertical" className="min-h-0 flex-1">
        <ResizablePanel defaultSize="45%" minSize="15%" className="flex flex-col">
          <CodeEditor label="SQL query" language="sql" value={sql} onChange={setSql} onRun={() => void run()} schema={schema} />
        </ResizablePanel>
        <ResizableHandle />
        <ResizablePanel defaultSize="55%" minSize="15%" className="flex flex-col">
          <Results result={result} error={error} running={running !== null} />
        </ResizablePanel>
      </ResizablePanelGroup>
    </>
  );
}

function Results({ result, error, running }: { result: QueryResult | null; error: unknown; running: boolean }) {
  if (running) {
    return (
      <div className="flex h-full items-center justify-center gap-2 text-xs text-muted-foreground">
        <Spinner className="size-4" /> Running…
      </div>
    );
  }
  if (error) {
    const cancelled = error instanceof BackendError && error.code === "cancelled";
    return (
      <div className="p-3">
        <Alert variant={cancelled ? "default" : "destructive"}>
          <Icon name={cancelled ? "stop" : "alert-circle"} />
          <AlertTitle>{cancelled ? "Cancelled" : "Query failed"}</AlertTitle>
          <AlertDescription className="font-mono text-[11.5px] whitespace-pre-wrap">{errorMessage(error)}</AlertDescription>
        </Alert>
      </div>
    );
  }
  if (!result) return <EmptyState icon="table" title="No results yet" description="Run a statement to see its rows here." />;
  if (result.columns.length === 0) {
    return <EmptyState icon="check" title="Done" description={`The statement ran in ${formatDuration(result.elapsedMs)} and returned no rows.`} />;
  }
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <DataGrid label="Query results" columns={result.columns} rowCount={result.rows.length} getRow={(i) => result.rows[i]} />
      <div className="flex shrink-0 items-center gap-2 border-t border-border/60 px-3 py-1 font-mono text-[10.5px] text-muted-foreground tabular-nums">
        {formatCount(result.rows.length)} rows · {formatDuration(result.elapsedMs)}
        {result.truncated && <span className="text-warning">· truncated at the row limit (Settings → Data)</span>}
      </div>
    </div>
  );
}

export const sqlEditorPanel = definePanel({
  type: "sql-editor",
  title: "SQL",
  icon: "code",
  description: "Ad-hoc DuckDB SQL over the project's datasets, sandboxed to the project folder.",
  component: SqlEditor,
  parseParams: (raw) => stringParams(raw, ["sql"] as const),
  placement: "stage",
  maturity: "ready",
});
