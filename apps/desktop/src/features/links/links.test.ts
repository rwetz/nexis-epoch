import { describe, expect, it } from "vitest";
import { relativeTo, samePath } from "./links";

describe("Nexis link paths", () => {
  it("compares paths across separators and drive-letter case", () => {
    expect(samePath("C:\\work\\plant-doctor", "c:/work/plant-doctor/")).toBe(true);
    expect(samePath("/a/b", "/a/c")).toBe(false);
  });

  it("makes a dataset path project-relative, or refuses", () => {
    expect(relativeTo("C:\\work\\p", "C:\\work\\p\\data\\plants.csv")).toBe("data/plants.csv");
    expect(relativeTo("/work/p", "/work/p/data/x.parquet")).toBe("data/x.parquet");
    expect(relativeTo("/work/p", "/work/pp/x.csv")).toBeNull();
    expect(relativeTo("/work/p", "/elsewhere/x.csv")).toBeNull();
  });
});
