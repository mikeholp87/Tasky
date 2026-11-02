import { formatBytesPerSecond, formatPercent } from "@/utils/format";
import type { ChartSeries } from "./TimeSeriesChart";

// Module-level constants keep chart configuration referentially stable.

export const CPU_SERIES: readonly ChartSeries[] = [
  { label: "CPU", colorVar: "--chart-1", value: (p) => p.cpu },
];

export const MEMORY_SERIES: readonly ChartSeries[] = [
  { label: "Memory", colorVar: "--chart-2", value: (p) => p.memPercent },
];

export const NETWORK_SERIES: readonly ChartSeries[] = [
  { label: "Download", colorVar: "--chart-1", value: (p) => p.netRx },
  { label: "Upload", colorVar: "--chart-2", value: (p) => p.netTx, fill: false },
];

export const DISK_SERIES: readonly ChartSeries[] = [
  { label: "Read", colorVar: "--chart-1", value: (p) => p.diskRead },
  { label: "Write", colorVar: "--chart-2", value: (p) => p.diskWrite, fill: false },
];

export const percentAxis = (v: number) => formatPercent(v, 0);
export const rateAxis = (v: number) => formatBytesPerSecond(v);

/** Cache of single-core series so each core's config object is stable. */
const coreSeriesCache = new Map<number, readonly ChartSeries[]>();

export function coreSeries(index: number): readonly ChartSeries[] {
  let s = coreSeriesCache.get(index);
  if (!s) {
    s = [{ label: `CPU ${index}`, colorVar: "--chart-1", value: (p) => p.perCore[index] ?? 0 }];
    coreSeriesCache.set(index, s);
  }
  return s;
}

/** Byte-rate charts never scale below 1 KB/s, so idle devices read "0 B/s – 1 KB/s". */
export const RATE_MIN_Y = 1024;
