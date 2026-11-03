import { describe, expect, it } from "vitest";
import type { BenchmarkResult } from "@/types";
import { compareScores, percentChange, previousComparable, sameMethod } from "./compare";
import { formatDelta, formatSpread, formatThroughput } from "./format";

function result(over: Partial<BenchmarkResult> & { id: string }): BenchmarkResult {
  return {
    kind: "cpu",
    profile: "standard",
    startedAtMs: 1000,
    durationMs: 1,
    method: { methodId: "cpu-method-v1", scoreVersion: "score-v1", comparability: "crossPlatform" },
    environment: {
      platform: "linux",
      distribution: null,
      kernel: null,
      architecture: "x86_64",
      cpuModel: "Test",
      physicalCores: null,
      logicalCores: 4,
      totalMemoryBytes: 1,
      powerSource: "unknown",
      cpuGovernor: null,
      virtualization: null,
      backgroundCpuPercent: null,
      developmentBuild: false,
    },
    workloads: [],
    sustained: null,
    scores: { singleCore: 1000, multiCore: 4000 },
    gpu: null,
    gpuScore: null,
    disk: null,
    diskScore: null,
    internet: null,
    warnings: [],
    stability: "high",
    outlierMethod: "",
    affinity: "",
    ...over,
  };
}

describe("comparison rules", () => {
  it("compares only results measured the same way", () => {
    const a = result({ id: "a" });
    const b = result({ id: "b", method: { ...a.method, methodId: "cpu-method-v2" } });
    const c = result({ id: "c", method: { ...a.method, scoreVersion: "score-v2" } });
    expect(sameMethod(a, a)).toBe(true);
    expect(sameMethod(a, b)).toBe(false);
    expect(sameMethod(a, c)).toBe(false);
    expect(compareScores(a, b)).toBeNull();
  });

  it("computes score changes in percent", () => {
    const before = result({ id: "a", scores: { singleCore: 1000, multiCore: 4000 } });
    const after = result({ id: "b", scores: { singleCore: 1100, multiCore: 3800 } });
    expect(compareScores(after, before)).toEqual({ singleCore: 10, multiCore: -5, gpu: null });
  });

  it("has nothing to compare without scores, e.g. a development build", () => {
    const dev = result({ id: "a", scores: null });
    expect(compareScores(result({ id: "b" }), dev)).toBeNull();
    expect(compareScores(dev, result({ id: "b" }))).toBeNull();
  });

  it("picks the newest older result with the same method and profile", () => {
    const current = result({ id: "now", startedAtMs: 5000 });
    const history = [
      result({ id: "newer", startedAtMs: 6000 }),
      result({ id: "other-profile", startedAtMs: 4500, profile: "quick" }),
      result({
        id: "other-method",
        startedAtMs: 4400,
        method: { ...current.method, methodId: "x" },
      }),
      result({ id: "best", startedAtMs: 4000 }),
      result({ id: "old", startedAtMs: 1000 }),
      current,
    ];
    expect(previousComparable(history, current)?.id).toBe("best");
    expect(previousComparable([current], current)).toBeNull();
    expect(previousComparable([], current)).toBeNull();
  });

  it("never divides by zero or accepts bad numbers", () => {
    expect(percentChange(0, 5)).toBeNull();
    expect(percentChange(null, 5)).toBeNull();
    expect(percentChange(5, Number.NaN)).toBeNull();
    expect(percentChange(200, 250)).toBe(25);
  });
});

function gpuResult(
  id: string,
  startedAtMs: number,
  adapterId: string,
  score: number,
  timing: "gpuTimestamps" | "wallClock" = "gpuTimestamps",
): BenchmarkResult {
  return result({
    id,
    kind: "gpu",
    startedAtMs,
    method: {
      methodId: "gpu-compute-v1",
      scoreVersion: "gpu-score-v1",
      comparability: "crossPlatform",
    },
    scores: null,
    gpuScore: score,
    gpu: {
      adapter: {
        id: adapterId,
        name: adapterId,
        vendorId: 1,
        deviceId: 2,
        deviceType: "integrated",
        backend: "vulkan",
        driver: "d",
        driverInfo: "i",
        supportsTimestamps: true,
        maxBufferBytes: 1,
        maxStorageBindingBytes: 1,
        maxWorkgroupsPerDimension: 65535,
        recommended: true,
      },
      timingMethod: timing,
      workloads: [],
      sustained: null,
      notes: [],
    },
  });
}

describe("GPU comparison rules", () => {
  it("compares a GPU result only with the same adapter and timing method", () => {
    const now = gpuResult("now", 5000, "vulkan-a", 1100);
    const history = [
      gpuResult("other-card", 4500, "vulkan-b", 3000),
      gpuResult("wall-clock", 4400, "vulkan-a", 900, "wallClock"),
      gpuResult("same", 4000, "vulkan-a", 1000),
      result({ id: "cpu", startedAtMs: 4800 }),
      now,
    ];
    expect(previousComparable(history, now)?.id).toBe("same");
    expect(compareScores(now, gpuResult("same", 4000, "vulkan-a", 1000))?.gpu).toBeCloseTo(10);
    expect(compareScores(now, gpuResult("c", 4000, "vulkan-b", 1000))).toBeNull();
    expect(compareScores(now, gpuResult("w", 4000, "vulkan-a", 1000, "wallClock"))).toBeNull();
  });

  it("never compares a GPU result with a CPU one", () => {
    const g = gpuResult("g", 5000, "vulkan-a", 1000);
    expect(sameMethod(g, result({ id: "c" }))).toBe(false);
    expect(compareScores(g, result({ id: "c" }))).toBeNull();
  });

  it("has nothing to compare when a GPU run was not scored", () => {
    const unscored = gpuResult("u", 4000, "vulkan-a", 0);
    unscored.gpuScore = null;
    expect(compareScores(gpuResult("n", 5000, "vulkan-a", 1000), unscored)).toBeNull();
  });
});

describe("formatting", () => {
  it("uses readable prefixes", () => {
    expect(formatThroughput(61406, "Mops/s")).toBe("61.4 Gops/s");
    expect(formatThroughput(850, "Mflops/s")).toBe("850 Mflops/s");
    expect(formatThroughput(2136, "MiB/s")).toBe("2.09 GiB/s");
    expect(formatThroughput(13.6, "MiB/s")).toBe("13.6 MiB/s");
    expect(formatThroughput(Number.NaN, "MiB/s")).toBe("—");
  });

  it("formats spread and deltas", () => {
    expect(formatSpread({ cvPercent: 2.14 })).toBe("±2.1%");
    expect(formatSpread({ cvPercent: null })).toBe("—");
    expect(formatDelta(3.24)).toBe("+3.2%");
    expect(formatDelta(-1.4)).toBe("−1.4%");
    expect(formatDelta(0.01)).toBe("±0.0%");
    expect(formatDelta(null)).toBeNull();
  });
});
