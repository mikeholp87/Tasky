import type { BenchmarkResult } from "@/types";

export const DIFFERENT_METHOD_NOTE =
  "These tests used different benchmark methodologies and should not be compared directly.";

/** Results measured the same way: same method and scoring version. */
export function sameMethod(a: BenchmarkResult, b: BenchmarkResult): boolean {
  return (
    a.kind === b.kind &&
    a.method.methodId === b.method.methodId &&
    a.method.scoreVersion === b.method.scoreVersion
  );
}

/**
 * The newest saved result older than `current` that can be compared with it:
 * same method and the same profile (a Quick run and an Extended run do not
 * warm up the same way).
 */
export function previousComparable(
  history: readonly BenchmarkResult[],
  current: BenchmarkResult,
): BenchmarkResult | null {
  let best: BenchmarkResult | null = null;
  for (const r of history) {
    if (r.id === current.id || r.startedAtMs >= current.startedAtMs) continue;
    if (!sameMethod(r, current) || r.profile !== current.profile) continue;
    if (best === null || r.startedAtMs > best.startedAtMs) best = r;
  }
  return best;
}

/** Percent change from `from` to `to`; null when it cannot be computed. */
export function percentChange(from: number | null, to: number | null): number | null {
  if (from == null || to == null || !Number.isFinite(from) || !Number.isFinite(to) || from === 0) {
    return null;
  }
  return ((to - from) / Math.abs(from)) * 100;
}

export interface ScoreDelta {
  singleCore: number | null;
  multiCore: number | null;
}

/**
 * Score changes between two results, or null when they must not be compared
 * (different methods) or have no scores. A single run is no proof of a real
 * difference; the UI words this as a change, never as significance.
 */
export function compareScores(
  current: BenchmarkResult,
  previous: BenchmarkResult,
): ScoreDelta | null {
  if (!sameMethod(current, previous) || !current.scores || !previous.scores) return null;
  return {
    singleCore: percentChange(previous.scores.singleCore, current.scores.singleCore),
    multiCore: percentChange(previous.scores.multiCore, current.scores.multiCore),
  };
}
