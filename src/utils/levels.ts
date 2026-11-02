/**
 * Severity levels for resource usage. Warning colors are reserved for genuine
 * pressure, so most values render as "normal".
 */
export type UsageLevel = "normal" | "warning" | "critical";

/** Resource-level thresholds (whole-system CPU, memory, disk capacity). */
export const RESOURCE_THRESHOLDS = { warning: 80, critical: 90 } as const;

export function usageLevel(
  percent: number | null | undefined,
  thresholds: { warning: number; critical: number } = RESOURCE_THRESHOLDS,
): UsageLevel {
  if (percent == null || !Number.isFinite(percent)) return "normal";
  if (percent >= thresholds.critical) return "critical";
  if (percent >= thresholds.warning) return "warning";
  return "normal";
}

/** Human wording for a level, so status never relies on color alone. */
export function levelLabel(level: UsageLevel, resource: string): string | null {
  switch (level) {
    case "critical":
      return `Very high ${resource} usage`;
    case "warning":
      return `High ${resource} usage`;
    default:
      return null;
  }
}

/**
 * Optional table heat mapping for per-process values:
 * 0 (none) · 1 (20–50) · 2 (50–80) · 3 (80+).
 */
export type HeatLevel = 0 | 1 | 2 | 3;

export function heatLevel(percent: number | null | undefined): HeatLevel {
  if (percent == null || !Number.isFinite(percent) || percent < 20) return 0;
  if (percent < 50) return 1;
  if (percent < 80) return 2;
  return 3;
}

/**
 * Heat for a process's share of total RAM. A single process using 30% of
 * memory is as notable as one using 80% CPU, so the bands are tighter.
 */
export function memoryHeatLevel(percentOfTotal: number | null | undefined): HeatLevel {
  if (percentOfTotal == null || !Number.isFinite(percentOfTotal) || percentOfTotal < 5) return 0;
  if (percentOfTotal < 15) return 1;
  if (percentOfTotal < 30) return 2;
  return 3;
}
