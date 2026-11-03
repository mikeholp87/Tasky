import { describe, expect, it } from "vitest";
import type { BenchmarkSummary, CategorySummary } from "@/types";
import { previousComparable, trend } from "./logic";

function run(
  id: string,
  t: number,
  categories: Partial<CategorySummary>[],
  extra: Partial<BenchmarkSummary> = {},
): BenchmarkSummary {
  return {
    id,
    startedAtMs: t * 1000,
    durationMs: 0,
    profile: "standard",
    methodVersion: "1.0.0",
    scoreVersion: 1,
    simulated: false,
    fingerprint: { cpuBrand: "", logicalCores: 1, memoryGib: 1, os: "", gpu: null, disk: null },
    systemScore: null,
    categories: categories.map((c) => ({
      kind: "cpu",
      status: "completed",
      score: null,
      confidence: null,
      metrics: [],
      ...c,
    })),
    ...extra,
  };
}

const HISTORY = [
  run("c", 300, [{ kind: "cpu", score: 1100, metrics: [] }]),
  run("sim", 250, [{ kind: "cpu", score: 9999 }], { simulated: true }),
  run("b", 200, [{ kind: "gpu", score: 500 }]),
  run("a", 100, [
    {
      kind: "cpu",
      score: 1000,
      metrics: [{ id: "cpu.multi.integer", label: "", unit: "mops", value: 16000, cvPercent: 1 }],
    },
  ]),
  run("old", 50, [{ kind: "cpu", score: 1 }], { methodVersion: "0.9.0" }),
];

describe("benchmark history logic", () => {
  it("finds the previous run with an overlapping category", () => {
    expect(previousComparable(HISTORY, "c")?.id).toBe("a");
    expect(previousComparable(HISTORY, "b")).toBeUndefined();
    expect(previousComparable(HISTORY, "missing")).toBeUndefined();
  });

  it("builds oldest-first trends excluding simulated and incompatible runs", () => {
    expect(trend(HISTORY, "score.cpu", "1", 1)).toEqual([
      { t: 100, value: 1000 },
      { t: 300, value: 1100 },
    ]);
    expect(trend(HISTORY, "cpu.multi.integer", "1", 1)).toEqual([{ t: 100, value: 16000 }]);
    expect(trend(HISTORY, "score.cpu", "1", 2)).toEqual([]);
  });
});
