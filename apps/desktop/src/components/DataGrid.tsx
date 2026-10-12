/**
 * A virtualised grid for query results and dataset pages.
 *
 * TanStack Table supplies the column model (sizing, resizing, sort state)
 * and TanStack Virtual renders only the rows in view. Rows are never handed
 * to the table as one array: `getRow(i)` reads from whatever paged cache
 * the caller keeps, and `onVisibleRange` tells it which pages to fetch, so
 * a million-row file costs a few pages of memory.
 */
import * as React from "react";
import {
  type ColumnDef,
  type ColumnSizingState,
  getCoreRowModel,
  useReactTable,
} from "@tanstack/react-table";
import { useVirtualizer } from "@tanstack/react-virtual";
import { cn, Icon } from "@nexis/design";
import type { CellValue, Column, Sort } from "@/types/backend";

const ROW_HEIGHT = 26;
const NUMERIC = /^(TINYINT|SMALLINT|INTEGER|BIGINT|HUGEINT|U?(TINY|SMALL|BIG)?INT(EGER)?|UHUGEINT|FLOAT|DOUBLE|REAL|DECIMAL)/i;

export function isNumericType(dataType: string): boolean {
  return NUMERIC.test(dataType);
}

function renderCell(v: CellValue | undefined): string {
  if (v === undefined) return "";
  if (v === null) return "NULL";
  if (typeof v === "object") return JSON.stringify(v);
  return String(v);
}

type GridRow = { index: number };

export function DataGrid({
  columns,
  rowCount,
  getRow,
  onVisibleRange,
  sort,
  onSortChange,
  showRowNumbers = true,
  className,
  label,
}: {
  columns: readonly Column[];
  rowCount: number;
  getRow(index: number): CellValue[] | undefined;
  onVisibleRange?(start: number, end: number): void;
  /** Server-side sort: the grid only reports clicks. */
  sort?: readonly Sort[];
  onSortChange?(sort: Sort[]): void;
  showRowNumbers?: boolean;
  className?: string;
  label: string;
}) {
  const scroller = React.useRef<HTMLDivElement>(null);
  const [sizing, setSizing] = React.useState<ColumnSizingState>({});

  const columnDefs = React.useMemo<ColumnDef<GridRow>[]>(
    () =>
      columns.map((c, i) => ({
        id: `${i}:${c.name}`,
        header: c.name,
        size: Math.min(320, Math.max(90, c.name.length * 8 + 48)),
        minSize: 56,
        maxSize: 900,
        meta: { column: c, index: i },
      })),
    [columns],
  );

  // Data is empty on purpose: rows come from getRow (see header comment).
  const table = useReactTable<GridRow>({
    data: [],
    columns: columnDefs,
    state: { columnSizing: sizing },
    onColumnSizingChange: setSizing,
    columnResizeMode: "onChange",
    enableColumnResizing: true,
    getCoreRowModel: getCoreRowModel(),
    manualSorting: true,
  });

  const leaf = table.getVisibleLeafColumns();
  const numberWidth = showRowNumbers ? Math.max(48, String(rowCount).length * 8 + 20) : 0;
  const totalWidth = numberWidth + leaf.reduce((w, c) => w + c.getSize(), 0);

  const virtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => scroller.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 16,
  });
  const items = virtualizer.getVirtualItems();
  const first = items[0]?.index ?? 0;
  const last = items[items.length - 1]?.index ?? 0;
  React.useEffect(() => {
    if (rowCount > 0) onVisibleRange?.(first, last + 1);
  }, [first, last, rowCount, onVisibleRange]);

  const sortOf = (name: string) => sort?.find((s) => s.column === name);
  const clickHeader = (name: string) => {
    if (!onSortChange) return;
    const current = sortOf(name);
    // none → ascending → descending → none
    onSortChange(!current ? [{ column: name, desc: false }] : !current.desc ? [{ column: name, desc: true }] : []);
  };

  return (
    <div
      ref={scroller}
      role="grid"
      aria-label={label}
      aria-rowcount={rowCount + 1}
      aria-colcount={columns.length}
      className={cn("nexis-scrollbar relative min-h-0 flex-1 overflow-auto font-mono text-[12px]", className)}
    >
      <div style={{ width: totalWidth, minWidth: "100%" }}>
        <div role="row" className="sticky top-0 z-10 flex h-8 border-b border-border bg-card/95 backdrop-blur-sm">
          {showRowNumbers && <div className="shrink-0 border-r border-border/60" style={{ width: numberWidth }} />}
          {table.getHeaderGroups()[0]?.headers.map((header) => {
            const column = (header.column.columnDef.meta as { column: Column }).column;
            const s = sortOf(column.name);
            const numeric = isNumericType(column.dataType);
            return (
              <div
                key={header.id}
                role="columnheader"
                aria-sort={s ? (s.desc ? "descending" : "ascending") : "none"}
                className="group relative flex shrink-0 items-center border-r border-border/60"
                style={{ width: header.getSize() }}
              >
                <button
                  type="button"
                  disabled={!onSortChange}
                  onClick={() => clickHeader(column.name)}
                  className={cn(
                    "flex h-full min-w-0 flex-1 flex-col justify-center px-2 text-left font-sans",
                    numeric && "items-end text-right",
                    onSortChange && "hover:bg-muted/60",
                  )}
                  title={`${column.name} · ${column.dataType}`}
                >
                  <span className="flex w-full min-w-0 items-center gap-1 text-[11.5px] font-medium text-foreground">
                    <span className={cn("truncate", numeric && "ml-auto")}>{column.name}</span>
                    {s && <Icon name={s.desc ? "chevron-down" : "chevron-up"} size="xs" className="shrink-0 text-brand" />}
                  </span>
                  <span className="w-full truncate font-mono text-[9.5px] text-muted-foreground uppercase">{column.dataType}</span>
                </button>
                <div
                  onMouseDown={header.getResizeHandler()}
                  onTouchStart={header.getResizeHandler()}
                  onDoubleClick={() => header.column.resetSize()}
                  className={cn(
                    "absolute top-0 right-0 h-full w-1.5 cursor-col-resize select-none",
                    header.column.getIsResizing() ? "bg-brand" : "hover:bg-border",
                  )}
                  aria-hidden
                />
              </div>
            );
          })}
        </div>
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {items.map((item) => {
            const row = getRow(item.index);
            return (
              <div
                key={item.key}
                role="row"
                aria-rowindex={item.index + 2}
                className={cn("absolute left-0 flex w-full border-b border-border/40", item.index % 2 === 1 && "bg-muted/25")}
                style={{ height: ROW_HEIGHT, transform: `translateY(${item.start}px)` }}
              >
                {showRowNumbers && (
                  <div className="shrink-0 border-r border-border/60 px-2 text-right leading-[26px] text-muted-foreground/70 tabular-nums" style={{ width: numberWidth }}>
                    {item.index + 1}
                  </div>
                )}
                {row
                  ? leaf.map((col) => {
                      const { column, index } = col.columnDef.meta as { column: Column; index: number };
                      const v = row[index];
                      return (
                        <div
                          key={col.id}
                          role="gridcell"
                          title={renderCell(v)}
                          className={cn(
                            "shrink-0 truncate border-r border-border/40 px-2 leading-[26px]",
                            isNumericType(column.dataType) && "text-right tabular-nums",
                            v === null && "text-muted-foreground/60 italic",
                          )}
                          style={{ width: col.getSize() }}
                        >
                          {renderCell(v)}
                        </div>
                      );
                    })
                  : (
                    <div className="flex items-center px-2">
                      <span className="h-2 w-40 rounded-full bg-muted" />
                    </div>
                  )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
