/**
 * Benchmark value formatting. Throughput units are decimal (1 MB = 10^6
 * bytes, 1 Mbps = 10^6 bit/s) and kept distinct: MB/s is storage
 * throughput, Mbps is network bandwidth.
 */
import type {
  BenchmarkKind,
  BenchmarkPhase,
  BenchmarkProfile,
  CategoryStatus,
  Confidence,
  MetricUnit,
} from "@/types";
import { Cpu, Gauge, Globe, HardDrive, type LucideIcon } from "lucide-react";
import { NOT_AVAILABLE, formatBytes, formatFrequency } from "@/utils/format";

export interface FormattedValue {
  value: string;
  unit: string;
}

function group(n: number, digits = 0): string {
  const [int, frac] = n.toFixed(digits).split(".");
  const grouped = (int ?? "0").replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  return frac ? `${grouped}.${frac}` : grouped;
}

/** Three significant figures without scientific notation. */
function sig(n: number): string {
  const abs = Math.abs(n);
  if (abs >= 100) return group(n, 0);
  if (abs >= 10) return n.toFixed(1);
  return n.toFixed(2);
}

export function formatMetric(value: number | null | undefined, unit: MetricUnit): FormattedValue {
  if (value == null || !Number.isFinite(value)) return { value: NOT_AVAILABLE, unit: "" };
  switch (unit) {
    case "mops":
      return value >= 1000
        ? { value: sig(value / 1000), unit: "Gops/s" }
        : { value: sig(value), unit: "Mops/s" };
    case "mflops":
      return value >= 1000
        ? { value: sig(value / 1000), unit: "GFLOP/s" }
        : { value: sig(value), unit: "MFLOP/s" };
    case "gflops":
      return value >= 1000
        ? { value: sig(value / 1000), unit: "TFLOP/s" }
        : { value: sig(value), unit: "GFLOP/s" };
    case "megabytesPerSec":
      return { value: group(value, value < 10 ? 2 : 0), unit: "MB/s" };
    case "gigabytesPerSec":
      return { value: sig(value), unit: "GB/s" };
    case "mbps":
      return value >= 1000
        ? { value: sig(value / 1000), unit: "Gbps" }
        : { value: sig(value), unit: "Mbps" };
    case "iops":
      return value >= 1000
        ? { value: `${sig(value / 1000)}K`, unit: "IOPS" }
        : { value: group(value), unit: "IOPS" };
    case "milliseconds":
      return { value: value < 10 ? value.toFixed(2) : group(value, 1), unit: "ms" };
    case "microseconds":
      return value >= 1000
        ? { value: (value / 1000).toFixed(2), unit: "ms" }
        : { value: group(value, value < 10 ? 2 : 0), unit: "µs" };
    case "percent":
      return { value: value.toFixed(1), unit: "%" };
    case "ratio":
      return { value: value.toFixed(2), unit: "×" };
    case "celsius":
      return { value: value.toFixed(0), unit: "°C" };
    case "megahertz": {
      const [v = NOT_AVAILABLE, u = ""] = formatFrequency(value).split(" ");
      return { value: v, unit: u };
    }
    case "bytes": {
      const [v = NOT_AVAILABLE, u = ""] = formatBytes(value).split(" ");
      return { value: v, unit: u };
    }
  }
}

export function formatMetricText(value: number | null | undefined, unit: MetricUnit): string {
  const f = formatMetric(value, unit);
  return f.unit ? `${f.value} ${f.unit}` : f.value;
}

export function formatScore(score: number | null | undefined): string {
  if (score == null || !Number.isFinite(score)) return "Not tested";
  return group(Math.round(score));
}

/** Signed percent change, e.g. "+4.1%". */
export function formatChange(percent: number): string {
  if (!Number.isFinite(percent)) return NOT_AVAILABLE;
  const sign = percent > 0 ? "+" : percent < 0 ? "−" : "±";
  return `${sign}${Math.abs(percent).toFixed(1)}%`;
}

/** Milliseconds → "00:17" or "3:05:09". */
export function formatElapsed(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`;
}

export const KIND_LABELS: Record<BenchmarkKind, string> = {
  cpu: "CPU",
  gpu: "GPU",
  disk: "Storage",
  network: "Internet",
};

export const PROFILE_LABELS: Record<BenchmarkProfile, string> = {
  quick: "Quick",
  standard: "Standard",
  extended: "Extended",
};

export const PHASE_LABELS: Record<BenchmarkPhase, string> = {
  idle: "Ready",
  preparing: "Preparing",
  warmingUp: "Warming up",
  running: "Running",
  coolingDown: "Cooling down",
  finalizing: "Finalizing",
  completed: "Completed",
  cancelled: "Cancelled",
  failed: "Failed",
};

export const STATUS_LABELS: Record<CategoryStatus, string> = {
  completed: "Completed",
  skipped: "Not tested",
  unsupported: "Unsupported by this system",
  failed: "Failed",
  cancelled: "Cancelled",
};

export const CONFIDENCE_LABELS: Record<Confidence, string> = {
  high: "High",
  medium: "Medium",
  low: "Low",
};

export const KIND_ICONS: Record<BenchmarkKind, LucideIcon> = {
  cpu: Cpu,
  gpu: Gauge,
  disk: HardDrive,
  network: Globe,
};

export const ALL_KINDS: readonly BenchmarkKind[] = ["cpu", "gpu", "disk", "network"];
