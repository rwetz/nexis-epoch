import { describe, expect, it } from "vitest";
import { formatBytes, formatMetric, formatRelativeTime } from "./format";

describe("format", () => {
  it("formats bytes", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(50 * 1024 * 1024)).toBe("50 MB");
  });
  it("formats metric values without hiding precision", () => {
    expect(formatMetric(0.000101)).toBe("1.01e-4");
    expect(formatMetric(0.98958333)).toBe("0.9896");
    expect(formatMetric(null)).toBe("—");
    expect(formatMetric(Number.NaN)).toBe("—");
  });
  it("formats relative times", () => {
    expect(formatRelativeTime(1_000_000, 1_000_000 + 5 * 60_000)).toBe("5 min ago");
  });
});
