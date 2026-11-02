import type { HistoryPoint } from "@/types";

/** One hour at the fastest (500 ms) interval; mirrors the backend buffer. */
export const MAX_HISTORY_POINTS = 7_200;

/**
 * Bounded, time-ordered history of ticks. Mutated in place (no per-tick array
 * copies); consumers re-read when the store's `historyVersion` changes.
 */
export class HistoryBuffer {
  private points: HistoryPoint[] = [];

  constructor(private readonly capacity = MAX_HISTORY_POINTS) {}

  get length(): number {
    return this.points.length;
  }

  push(point: HistoryPoint): void {
    const last = this.points[this.points.length - 1];
    // Ignore duplicates/out-of-order points (e.g. backfill racing a live tick).
    if (last && point.t <= last.t) return;
    this.points.push(point);
    const overflow = this.points.length - this.capacity;
    if (overflow > 0) this.points.splice(0, overflow);
  }

  /**
   * Merges points from the backend (e.g. a startup backfill, or the gap left
   * while the window was hidden) into the buffer, keeping time order and
   * dropping duplicates.
   */
  backfill(points: readonly HistoryPoint[]): void {
    const merged: HistoryPoint[] = [];
    let i = 0;
    let j = 0;
    const a = this.points;
    while (i < a.length || j < points.length) {
      const next =
        j >= points.length ||
        (i < a.length && (a[i] as HistoryPoint).t <= (points[j] as HistoryPoint).t)
          ? (a[i++] as HistoryPoint)
          : (points[j++] as HistoryPoint);
      const last = merged[merged.length - 1];
      if (!last || next.t > last.t) merged.push(next);
    }
    this.points = merged.slice(-this.capacity);
  }

  /** Points with `t >= nowMs - seconds * 1000`, oldest first. */
  window(seconds: number, nowMs: number): HistoryPoint[] {
    const cutoff = nowMs - seconds * 1000;
    let lo = 0;
    let hi = this.points.length;
    while (lo < hi) {
      const mid = (lo + hi) >>> 1;
      if ((this.points[mid] as HistoryPoint).t < cutoff) lo = mid + 1;
      else hi = mid;
    }
    return this.points.slice(lo);
  }

  clear(): void {
    this.points = [];
  }
}
