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
 * GPU results also need the same adapter and the same timing method: a
 * different card, or timestamps against wall-clock timing, is a different
 * measurement even under the same method id.
 */
function sameDevice(a: BenchmarkResult, b: BenchmarkResult): boolean {
  if (a.kind !== "gpu") return true;
  return (
    a.gpu != null &&
    b.gpu != null &&
    a.gpu.adapter.id === b.gpu.adapter.id &&
    a.gpu.timingMethod === b.gpu.timingMethod
  );
}

/**
 * The newest saved result older than `current` that can be compared with it:
 * same method, profile and (for GPUs) device. A Quick run and an Extended run
 * do not warm up the same way.
 */
export function previousComparable(
  history: readonly BenchmarkResult[],
  current: BenchmarkResult,
): BenchmarkResult | null {
  let best: BenchmarkResult | null = null;
  for (const r of history) {
    if (r.id === current.id || r.startedAtMs >= current.startedAtMs) continue;
    if (!sameMethod(r, current) || r.profile !== current.profile || !sameDevice(r, current)) {
      continue;
    }
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
  /** GPU score change; null for CPU results. */
  gpu: number | null;
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
  if (!sameMethod(current, previous)) return null;
  if (current.kind === "gpu") {
    if (!sameDevice(current, previous)) return null;
    const gpu = percentChange(previous.gpuScore, current.gpuScore);
    return gpu === null ? null : { singleCore: null, multiCore: null, gpu };
  }
  if (!current.scores || !previous.scores) return null;
  return {
    singleCore: percentChange(previous.scores.singleCore, current.scores.singleCore),
    multiCore: percentChange(previous.scores.multiCore, current.scores.multiCore),
    gpu: null,
  };
}
