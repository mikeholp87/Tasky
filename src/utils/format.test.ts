import { describe, expect, it } from "vitest";
import {
  NOT_AVAILABLE,
  formatBytes,
  formatBytesPerSecond,
  formatCount,
  formatDuration,
  formatFrequency,
  formatPercent,
  formatTimestamp,
} from "./format";

describe("formatBytes", () => {
  it("matches the documented examples", () => {
    expect(formatBytes(932)).toBe("932 B");
    expect(formatBytes(14.2 * 1024)).toBe("14.2 KB");
    expect(formatBytes(82.6 * 1024 ** 2)).toBe("82.6 MB");
    expect(formatBytes(1_580_000_000)).toBe("1.47 GB");
  });

  it("handles boundaries without '1024 KB'", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1023)).toBe("1023 B");
    expect(formatBytes(1024)).toBe("1.00 KB");
    expect(formatBytes(1024 * 1024 - 1)).toBe("1.00 MB");
    expect(formatBytes(512 * 1024 ** 3)).toBe("512 GB");
  });

  it("returns a placeholder for missing or invalid values", () => {
    expect(formatBytes(null)).toBe(NOT_AVAILABLE);
    expect(formatBytes(undefined)).toBe(NOT_AVAILABLE);
    expect(formatBytes(-5)).toBe(NOT_AVAILABLE);
    expect(formatBytes(Number.NaN)).toBe(NOT_AVAILABLE);
  });
});

describe("formatBytesPerSecond", () => {
  it("appends /s", () => {
    expect(formatBytesPerSecond(3 * 1024 ** 2)).toBe("3.00 MB/s");
    expect(formatBytesPerSecond(null)).toBe(NOT_AVAILABLE);
  });
});

describe("formatDuration", () => {
  it("matches the documented example", () => {
    expect(formatDuration(93_784)).toBe("1d 2h 3m");
  });
  it("picks the two most significant units", () => {
    expect(formatDuration(45)).toBe("45s");
    expect(formatDuration(192)).toBe("3m 12s");
    expect(formatDuration(3_600 * 4 + 300)).toBe("4h 5m");
    expect(formatDuration(-1)).toBe(NOT_AVAILABLE);
  });
});

describe("formatPercent / formatFrequency / formatCount", () => {
  it("formats percentages", () => {
    expect(formatPercent(57.4567)).toBe("57.5%");
    expect(formatPercent(3, 0)).toBe("3%");
    expect(formatPercent(null)).toBe(NOT_AVAILABLE);
  });
  it("formats frequencies", () => {
    expect(formatFrequency(4200)).toBe("4.20 GHz");
    expect(formatFrequency(800)).toBe("800 MHz");
    expect(formatFrequency(0)).toBe(NOT_AVAILABLE);
  });
  it("groups thousands", () => {
    expect(formatCount(1234567)).toBe("1,234,567");
    expect(formatCount(12)).toBe("12");
  });
});

describe("formatTimestamp", () => {
  it("formats in local time with zero padding", () => {
    const ms = new Date(2026, 0, 5, 7, 8, 9).getTime();
    expect(formatTimestamp(ms)).toBe("2026-01-05 07:08:09");
  });
});
