import * as React from "react";
import {
  Badge,
  Button,
  GlidingTabs,
  Icon,
  Input,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  toast,
} from "@nexis/design";
import { DataGrid } from "@/components/DataGrid";
import { PanelToolbar, ToolbarButton, ToolbarTitle } from "@/components/PanelToolbar";
import { EmptyState, ErrorState, LoadingState } from "@/components/states";
import { openPanel, useApp } from "@/app/store";
import { usePanel } from "@/features/workspace/panel-context";
import { definePanel, stringParams } from "@/features/workspace/registry";
import { api } from "@/lib/api";
import { basename, fileIcon } from "@/lib/fileIcons";
import { formatBytes, formatCount, formatDuration } from "@/lib/format";
import { errorMessage, newQueryId } from "@/lib/ipc";
import type { Column, DatasetInfo, Filter, FilterOp, QueryResult, Sort } from "@/types/backend";
import { usePageCache } from "./pageCache";

const OPS: { id: FilterOp; label: string; needsValue: boolean }[] = [
  { id: "contains", label: "contains", needsValue: true },
  { id: "eq", label: "=", needsValue: true },
  { id: "ne", label: "≠", needsValue: true },
  { id: "gt", label: ">", needsValue: true },
  { id: "ge", label: "≥", needsValue: true },
  { id: "lt", label: "<", needsValue: true },
  { id: "le", label: "≤", needsValue: true },
  { id: "starts_with", label: "starts with", needsValue: true },
  { id: "is_null", label: "is null", needsValue: false },
  { id: "not_null", label: "is not null", needsValue: false },
];

/** A cancellable backend call: the id lets the Cancel button reach it. */
function useCancellable() {
  const [running, setRunning] = React.useState<string | null>(null);
  const run = React.useCallback(async <T,>(prefix: string, call: (id: string) => Promise<T>): Promise<T> => {
    const id = newQueryId(prefix);
    setRunning(id);
    try {
      return await call(id);
    } finally {
      setRunning((r) => (r === id ? null : r));
    }
  }, []);
  const cancel = React.useCallback(() => {
    if (running) void api.queryCancel(running);
  }, [running]);
  return { running: running !== null, run, cancel };
}

function DatasetExplorer() {
  const panel = usePanel("dataset-explorer");
  const project = useApp((s) => s.project);
  if (!project) return <EmptyState icon="database" title="No project open" description="Open a project to explore its datasets." />;
  if (!panel.params.path) {
    const datasets = project.datasets;
    return (
      <EmptyState
        icon="database"
        title="Choose a dataset"
        description={datasets.length ? "CSV, Parquet and JSON files found in this project:" : "No CSV, Parquet or JSON files were found near the project root. Open one from the project explorer."}
        action={
          datasets.length > 0 && (
            <div className="nexis-scrollbar flex max-h-72 w-full max-w-80 flex-col gap-0.5 overflow-y-auto">
              {datasets.slice(0, 50).map((d) => (
                <Button key={d.path} size="sm" variant="ghost" className="justify-start text-xs" onClick={() => panel.setParams({ path: d.path })}>
                  <Icon name={fileIcon(d.kind)} />
                  <span className="truncate font-mono">{d.path}</span>
                  <span className="ml-auto font-mono text-[10px] text-muted-foreground">{formatBytes(d.size)}</span>
                </Button>
              ))}
            </div>
          )
        }
      />
    );
  }
  return <Dataset key={panel.params.path} path={panel.params.path} />;
}

function Dataset({ path }: { path: string }) {
  const settings = useApp((s) => s.settings);
  const pageSize = settings?.data.pageSize ?? 200;
  const [info, setInfo] = React.useState<DatasetInfo | null>(null);
  const [infoError, setInfoError] = React.useState<unknown>(null);
  const [tab, setTab] = React.useState<"data" | "schema" | "stats">("data");
  const describe = useCancellable();

  const load = React.useCallback(() => {
    setInfoError(null);
    setInfo(null);
    describe.run("describe", (id) => api.datasetDescribe(path, id)).then(setInfo, setInfoError);
  }, [path]); // eslint-disable-line react-hooks/exhaustive-deps
  React.useEffect(load, [load]);

  if (infoError) return <ErrorState error={infoError} onRetry={load} />;
  if (!info) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2">
        <LoadingState label={`Reading ${basename(path)}…`} className="h-auto min-h-0" />
        {describe.running && (
          <Button size="xs" variant="ghost" onClick={describe.cancel}>
            Cancel
          </Button>
        )}
      </div>
    );
  }

  return (
    <>
      <PanelToolbar>
        <ToolbarTitle icon={fileIcon(info.kind)} meta={`${formatCount(info.rowCount)} × ${info.columns.length}`}>
          <span title={path}>{basename(path)}</span>
        </ToolbarTitle>
        <span className="hidden font-mono text-[10.5px] text-muted-foreground sm:inline">{formatBytes(info.size)}</span>
        <span className="flex-1" />
        <GlidingTabs
          label="Dataset view"
          value={tab}
          onChange={setTab}
          tabs={[
            { id: "data", label: "Data", icon: "table" },
            { id: "schema", label: "Schema", icon: "list-bullet" },
            { id: "stats", label: "Statistics", icon: "chart-bar" },
          ]}
        />
        <ToolbarButton
          icon="code"
          label="Query with SQL"
          onClick={async () => {
            try {
              const view = await api.sqlRegisterView(path, basename(path).replace(/\.[^.]+$/, ""));
              openPanel("sql-editor", { sql: `SELECT *\nFROM ${view}\nLIMIT 100` }, { reuse: "none" });
            } catch (e) {
              toast.error("Couldn't prepare the SQL view", { description: errorMessage(e) });
            }
          }}
        />
      </PanelToolbar>
      {tab === "data" && <DataTab path={path} info={info} pageSize={pageSize} />}
      {tab === "schema" && <SchemaTab columns={info.columns} />}
      {tab === "stats" && <StatsTab path={path} />}
    </>
  );
}

function DataTab({ path, info, pageSize }: { path: string; info: DatasetInfo; pageSize: number }) {
  const [sort, setSort] = React.useState<Sort[]>([]);
  const [filters, setFilters] = React.useState<Filter[]>([]);
  const [total, setTotal] = React.useState<number>(info.rowCount);
  const key = JSON.stringify([path, sort, filters]);

  const fetchPage = React.useCallback(
    async (offset: number, limit: number) => {
      const page = await api.datasetPage(path, { offset, limit, sort, filters, withTotal: offset === 0 });
      if (page.total !== null) setTotal(page.total);
      return page.rows;
    },
    [path, sort, filters],
  );
  const { cache, ensure, error } = usePageCache(pageSize, fetchPage, key);
  // Count first, so the grid knows its height before scrolling starts.
  React.useEffect(() => {
    if (filters.length === 0) setTotal(info.rowCount);
    ensure(0, 1);
  }, [cache]); // eslint-disable-line react-hooks/exhaustive-deps

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <FilterBar columns={info.columns} filters={filters} onChange={setFilters} total={total} rowCount={info.rowCount} />
      {error ? (
        <ErrorState error={error} onRetry={() => setFilters([...filters])} />
      ) : total === 0 ? (
        <EmptyState icon="filter" title="No rows match" description="Loosen or remove a filter." />
      ) : (
        <DataGrid
          key={key}
          label={`Rows of ${basename(path)}`}
          columns={info.columns}
          rowCount={total}
          getRow={(i) => cache.getRow(i)}
          onVisibleRange={ensure}
          sort={sort}
          onSortChange={setSort}
        />
      )}
    </div>
  );
}

function FilterBar({
  columns,
  filters,
  onChange,
  total,
  rowCount,
}: {
  columns: readonly Column[];
  filters: readonly Filter[];
  onChange(f: Filter[]): void;
  total: number;
  rowCount: number;
}) {
  const [column, setColumn] = React.useState(columns[0]?.name ?? "");
  const [op, setOp] = React.useState<FilterOp>("contains");
  const [value, setValue] = React.useState("");
  const needsValue = OPS.find((o) => o.id === op)?.needsValue ?? true;
  const add = () => {
    if (!column || (needsValue && value === "")) return;
    onChange([...filters, { column, op, value: needsValue ? value : null }]);
    setValue("");
  };
  return (
    <div className="flex shrink-0 flex-wrap items-center gap-1.5 border-b border-border/60 px-2 py-1.5">
      <Select value={column} onValueChange={setColumn}>
        <SelectTrigger size="sm" className="h-7 w-36 text-xs" aria-label="Filter column">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {columns.map((c) => (
            <SelectItem key={c.name} value={c.name} className="font-mono text-xs">
              {c.name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Select value={op} onValueChange={(v) => setOp(v as FilterOp)}>
        <SelectTrigger size="sm" className="h-7 w-28 text-xs" aria-label="Filter operator">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {OPS.map((o) => (
            <SelectItem key={o.id} value={o.id} className="text-xs">
              {o.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      {needsValue && (
        <Input
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && add()}
          placeholder="value"
          aria-label="Filter value"
          className="h-7 w-40 font-mono text-xs"
        />
      )}
      <Button size="xs" variant="outline" onClick={add} disabled={needsValue && value === ""}>
        <Icon name="filter" /> Add filter
      </Button>
      {filters.map((f, i) => (
        <Badge key={i} variant="secondary" className="h-6 gap-1 pr-0.5 font-mono text-[11px]">
          {f.column} {OPS.find((o) => o.id === f.op)?.label} {f.value ?? ""}
          <button type="button" aria-label="Remove filter" onClick={() => onChange(filters.filter((_, j) => j !== i))} className="grid size-4 place-items-center rounded hover:bg-muted">
            <Icon name="close" size={10} />
          </button>
        </Badge>
      ))}
      <span className="ml-auto font-mono text-[10.5px] text-muted-foreground tabular-nums">
        {filters.length ? `${formatCount(total)} of ${formatCount(rowCount)} rows` : `${formatCount(rowCount)} rows`}
      </span>
    </div>
  );
}

function SchemaTab({ columns }: { columns: readonly Column[] }) {
  return (
    <div className="nexis-scrollbar min-h-0 flex-1 overflow-auto">
      <table className="w-full text-xs">
        <thead className="sticky top-0 bg-card text-left text-[10.5px] tracking-wide text-muted-foreground uppercase">
          <tr>
            <th className="w-12 px-3 py-2 font-medium">#</th>
            <th className="px-3 py-2 font-medium">Column</th>
            <th className="px-3 py-2 font-medium">Type</th>
          </tr>
        </thead>
        <tbody>
          {columns.map((c, i) => (
            <tr key={c.name} className="border-t border-border/50">
              <td className="px-3 py-1.5 font-mono text-muted-foreground tabular-nums">{i + 1}</td>
              <td className="px-3 py-1.5 font-mono">{c.name}</td>
              <td className="px-3 py-1.5 font-mono text-muted-foreground">{c.dataType}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function StatsTab({ path }: { path: string }) {
  const [result, setResult] = React.useState<QueryResult | null>(null);
  const [error, setError] = React.useState<unknown>(null);
  const q = useCancellable();
  const load = React.useCallback(() => {
    setError(null);
    setResult(null);
    q.run("stats", (id) => api.datasetStats(path, id)).then(setResult, setError);
  }, [path]); // eslint-disable-line react-hooks/exhaustive-deps
  React.useEffect(load, [load]);
  if (error) return <ErrorState error={error} onRetry={load} />;
  if (!result) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2">
        <LoadingState label="Summarising every column…" className="h-auto min-h-0" />
        {q.running && (
          <Button size="xs" variant="ghost" onClick={q.cancel}>
            Cancel
          </Button>
        )}
      </div>
    );
  }
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <DataGrid label="Column statistics" columns={result.columns} rowCount={result.rows.length} getRow={(i) => result.rows[i]} showRowNumbers={false} />
      <div className="shrink-0 border-t border-border/60 px-3 py-1 text-[10.5px] text-muted-foreground">
        DuckDB SUMMARIZE · {formatDuration(result.elapsedMs)} · distinct counts are approximate
      </div>
    </div>
  );
}

export const datasetExplorerPanel = definePanel({
  type: "dataset-explorer",
  title: "Dataset",
  icon: "table",
  description: "Preview, filter, sort and summarise CSV, Parquet and JSON files with DuckDB.",
  component: DatasetExplorer,
  parseParams: (raw) => stringParams(raw, ["path"] as const),
  tabTitle: (p) => (p.path ? basename(p.path) : null),
  placement: "stage",
  maturity: "partial",
  limitations: "CSV, Parquet, JSON and JSONL. SQLite databases, image and text datasets, remote sources and Polars transforms are planned.",
});
