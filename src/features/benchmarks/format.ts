import type { WorkloadResult } from "@/types";

/** Throughput with a sensible prefix: 61 406 Mops/s reads as 61.4 Gops/s. */
export function formatThroughput(value: number, unit: string): string {
  if (!Number.isFinite(value)) return "—";
  if (unit === "Mops/s" || unit === "Mflops/s") {
    const base = unit === "Mops/s" ? "ops/s" : "flops/s";
    return value >= 1000 ? `${(value / 1000).toFixed(1)} G${base}` : `${value.toFixed(0)} M${base}`;
  }
  if (unit === "MB/s" && value >= 1000) return `${(value / 1000).toFixed(2)} GB/s`;
  if (unit === "IOPS") {
    if (value >= 1e6) return `${(value / 1e6).toFixed(2)} M IOPS`;
    if (value >= 1000) return `${(value / 1000).toFixed(value >= 100_000 ? 0 : 1)} k IOPS`;
    return `${value.toFixed(0)} IOPS`;
  }
  if (unit === "Mbit/s" && value >= 1000) return `${(value / 1000).toFixed(2)} Gbit/s`;
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

/** Microseconds as µs below a millisecond, else ms. */
export function formatMicros(us: number): string {
  if (!Number.isFinite(us)) return "—";
  if (us < 1000) return `${us < 10 ? us.toFixed(1) : us.toFixed(0)} µs`;
  const ms = us / 1000;
  return `${ms < 10 ? ms.toFixed(2) : ms.toFixed(1)} ms`;
}

/** Decimal bytes: 1.2 GB, 640 MB. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes)) return "—";
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(bytes >= 1e10 ? 0 : 1)} GB`;
  if (bytes >= 1e6) return `${(bytes / 1e6).toFixed(0)} MB`;
  return `${(bytes / 1e3).toFixed(0)} kB`;
}
