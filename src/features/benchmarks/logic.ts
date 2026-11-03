/** Pure helpers for benchmark history views (unit-tested). */
import type { BenchmarkKind, BenchmarkSummary } from "@/types";

/** The most recent earlier run that measured any of the same categories. */
export function previousComparable(
  history: readonly BenchmarkSummary[],
  id: string,
): BenchmarkSummary | undefined {
  const index = history.findIndex((h) => h.id === id);
  const current = history[index];
  if (!current) return undefined;
  const kinds = new Set(
    current.categories.filter((c) => c.status === "completed").map((c) => c.kind),
  );
  return history
    .slice(index + 1)
    .find(
      (h) =>
        h.simulated === current.simulated &&
        h.categories.some((c) => c.status === "completed" && kinds.has(c.kind)),
    );
}

export interface TrendPoint {
  /** Unix seconds. */
  t: number;
  value: number;
}

/**
 * Values of one metric (or a category score via `score.<kind>`) across
 * history, oldest first. Simulated runs and other methodology/score
 * versions are excluded so incompatible results are never charted together.
 */
export function trend(
  history: readonly BenchmarkSummary[],
  metricId: string,
  methodMajor: string,
  scoreVersion: number,
): TrendPoint[] {
  const points: TrendPoint[] = [];
  for (const h of history) {
    if (h.simulated) continue;
    if (h.methodVersion.split(".")[0] !== methodMajor || h.scoreVersion !== scoreVersion) continue;
    let value: number | null | undefined;
    if (metricId.startsWith("score.")) {
      const kind = metricId.slice(6) as BenchmarkKind;
      value = h.categories.find((c) => c.kind === kind && c.status === "completed")?.score;
    } else {
      for (const c of h.categories) {
        const m = c.metrics.find((x) => x.id === metricId);
        if (m) value = m.value;
      }
    }
    if (value != null && Number.isFinite(value)) {
      points.push({ t: h.startedAtMs / 1000, value });
    }
  }
  return points.reverse();
}
