import type { WorkloadResult } from "@/types";

/** Throughput with a sensible prefix: 61 406 Mops/s reads as 61.4 Gops/s. */
export function formatThroughput(value: number, unit: string): string {
  if (!Number.isFinite(value)) return "—";
  if (unit === "Mops/s" || unit === "Mflops/s") {
    const base = unit === "Mops/s" ? "ops/s" : "flops/s";
    return value >= 1000 ? `${(value / 1000).toFixed(1)} G${base}` : `${value.toFixed(0)} M${base}`;
  }
  if (unit === "MiB/s" && value >= 1024) return `${(value / 1024).toFixed(2)} GiB/s`;
  return `${value >= 100 ? value.toFixed(0) : value.toFixed(1)} ${unit}`;
}

/** Spread between iterations, e.g. "±2.1%". */
export function formatSpread(w: Pick<WorkloadResult, "cvPercent">): string {
  return w.cvPercent == null ? "—" : `±${w.cvPercent.toFixed(1)}%`;
}

/** "+3.2%" or "−1.4%"; null when there is nothing to compare. */
export function formatDelta(percent: number | null): string | null {
  if (percent == null) return null;
  const rounded = Math.abs(percent) < 0.05 ? 0 : percent;
  const sign = rounded > 0 ? "+" : rounded < 0 ? "−" : "±";
  return `${sign}${Math.abs(rounded).toFixed(1)}%`;
}
