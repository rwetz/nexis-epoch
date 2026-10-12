import { describe, expect, it } from "vitest";
import type { CellValue } from "@/types/backend";
import { PageCache } from "./pageCache";

const flush = () => new Promise((r) => setTimeout(r, 0));

function rowsFor(offset: number, limit: number): CellValue[][] {
  return Array.from({ length: limit }, (_, i) => [offset + i]);
}

describe("PageCache", () => {
  it("fetches only the pages a range needs, once", async () => {
    const calls: number[] = [];
    const cache = new PageCache(10, 5, async (o, l) => (calls.push(o), rowsFor(o, l)), () => {}, () => {});
    cache.ensure(5, 25);
    cache.ensure(5, 25);
    await flush();
    expect(calls).toEqual([0, 10, 20]);
    expect(cache.getRow(17)).toEqual([17]);
    expect(cache.getRow(35)).toBeUndefined();
  });

  it("keeps at most maxPages, dropping the least recently used", async () => {
    const cache = new PageCache(10, 2, async (o, l) => rowsFor(o, l), () => {}, () => {});
    cache.ensure(0, 10);
    cache.ensure(10, 20);
    await flush();
    cache.getRow(0); // page 0 is now the most recent
    cache.ensure(20, 30);
    await flush();
    expect(cache.size).toBe(2);
    expect(cache.getRow(0)).toEqual([0]);
    expect(cache.getRow(10)).toBeUndefined();
  });

  it("discards results that land after a clear", async () => {
    let release: (rows: CellValue[][]) => void = () => {};
    const cache = new PageCache(10, 5, () => new Promise((r) => (release = r)), () => {}, () => {});
    cache.ensure(0, 10);
    cache.clear();
    release(rowsFor(0, 10));
    await flush();
    expect(cache.getRow(0)).toBeUndefined();
  });

  it("reports errors and lets the page be retried", async () => {
    const errors: unknown[] = [];
    let fail = true;
    const cache = new PageCache(10, 5, async (o, l) => {
      if (fail) throw new Error("boom");
      return rowsFor(o, l);
    }, () => {}, (e) => errors.push(e));
    cache.ensure(0, 10);
    await flush();
    expect(errors).toHaveLength(1);
    fail = false;
    cache.ensure(0, 10);
    await flush();
    expect(cache.getRow(3)).toEqual([3]);
  });
});
