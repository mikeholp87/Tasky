import type { ProcessPoint, ProcessRow } from "@/types";

/** Matches the backend's per-process ring buffer. */
export const MAX_PROCESS_POINTS = 300;

export function toPoint(row: ProcessRow, t: number): ProcessPoint {
  return {
    t,
    cpu: row.cpuPercent,
    memory: row.memoryBytes,
    read: row.diskReadPerSec,
    write: row.diskWritePerSec,
  };
}

/** Appends a point if it is newer than the last one, keeping at most `max`. */
export function appendPoint(
  points: readonly ProcessPoint[],
  point: ProcessPoint,
  max = MAX_PROCESS_POINTS,
): ProcessPoint[] {
  const last = points[points.length - 1];
  if (last && point.t <= last.t) return points as ProcessPoint[];
  const next = [...points, point];
  return next.length > max ? next.slice(next.length - max) : next;
}

/**
 * Backend backfill followed by live points that are newer than it, capped to
 * the newest `max` points.
 */
export function mergeHistory(
  base: readonly ProcessPoint[],
  live: readonly ProcessPoint[],
  max: number,
): ProcessPoint[] {
  const lastBase = base[base.length - 1]?.t ?? Number.NEGATIVE_INFINITY;
  const merged = [...base, ...live.filter((p) => p.t > lastBase)];
  return merged.length > max ? merged.slice(merged.length - max) : merged;
}
