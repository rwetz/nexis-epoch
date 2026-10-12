import { describe, expect, it } from "vitest";
import { basename } from "./fileIcons";

describe("basename", () => {
  it("handles Windows and POSIX separators", () => {
    expect(basename("C:\\Users\\me\\proj\\.nexis-ml\\runs\\2026-09-18-1640-tabular-5")).toBe("2026-09-18-1640-tabular-5");
    expect(basename("data/players.parquet")).toBe("players.parquet");
    expect(basename("train.toml")).toBe("train.toml");
  });
});
