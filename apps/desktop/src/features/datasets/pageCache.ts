import * as React from "react";
import type { CellValue } from "@/types/backend";

export type FetchPage = (offset: number, limit: number) => Promise<CellValue[][]>;

/**
 * Rows by index, fetched a page at a time and kept for at most `maxPages`
 * pages (least recently used are dropped). A grid scrolling a huge file
 * holds a few hundred rows, never the file.
 */
export class PageCache {
  private pages = new Map<number, CellValue[][]>();
  private inflight = new Set<number>();
  private generation = 0;

  constructor(
    private readonly pageSize: number,
    private readonly maxPages: number,
    private readonly fetchPage: FetchPage,
    private readonly onChange: () => void,
    private readonly onError: (e: unknown) => void,
  ) {}

  getRow(index: number): CellValue[] | undefined {
    const page = Math.floor(index / this.pageSize);
    const rows = this.pages.get(page);
    if (!rows) return undefined;
    // Touch: most recently used goes last.
    this.pages.delete(page);
    this.pages.set(page, rows);
    return rows[index - page * this.pageSize];
  }

  /** Make sure rows [start, end) are loaded or loading. */
  ensure(start: number, end: number): void {
    const firstPage = Math.floor(start / this.pageSize);
    const lastPage = Math.floor(Math.max(start, end - 1) / this.pageSize);
    for (let p = firstPage; p <= lastPage; p++) {
      if (this.pages.has(p) || this.inflight.has(p)) continue;
      this.inflight.add(p);
      const gen = this.generation;
      this.fetchPage(p * this.pageSize, this.pageSize).then(
        (rows) => {
          if (gen !== this.generation) return;
          this.inflight.delete(p);
          this.pages.set(p, rows);
          while (this.pages.size > this.maxPages) {
            const oldest = this.pages.keys().next().value;
            if (oldest === undefined) break;
            this.pages.delete(oldest);
          }
          this.onChange();
        },
        (e: unknown) => {
          if (gen !== this.generation) return;
          this.inflight.delete(p);
          this.onError(e);
        },
      );
    }
  }

  /** Seed a page that arrived some other way (the first page, with the count). */
  put(page: number, rows: CellValue[][]): void {
    this.pages.set(page, rows);
  }

  /** Forget everything; in-flight results are discarded when they land. */
  clear(): void {
    this.generation++;
    this.pages.clear();
    this.inflight.clear();
  }

  get size(): number {
    return this.pages.size;
  }
}

/** A PageCache tied to React: re-renders when pages land. */
export function usePageCache(pageSize: number, fetchPage: FetchPage, resetKey: string) {
  const [version, setVersion] = React.useState(0);
  const [error, setError] = React.useState<unknown>(null);
  const fetchRef = React.useRef(fetchPage);
  fetchRef.current = fetchPage;
  const cache = React.useMemo(
    () =>
      new PageCache(
        pageSize,
        24,
        (o, l) => fetchRef.current(o, l),
        () => setVersion((v) => v + 1),
        setError,
      ),
    // A new key (file, sort, filters) means a new cache.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [pageSize, resetKey],
  );
  React.useEffect(() => () => cache.clear(), [cache]);
  React.useEffect(() => setError(null), [cache]);
  const ensure = React.useCallback((s: number, e: number) => cache.ensure(s, e), [cache]);
  return { cache, ensure, version, error };
}
