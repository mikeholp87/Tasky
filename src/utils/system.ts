/** Seconds since boot, derived from the static boot time and the latest tick. */
export function uptimeSeconds(bootTimeSecs: number | null, nowMs: number): number | null {
  if (bootTimeSecs == null || bootTimeSecs <= 0) return null;
  return Math.max(0, Math.floor(nowMs / 1000 - bootTimeSecs));
}
