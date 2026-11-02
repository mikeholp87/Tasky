/**
 * Display formatting. All functions are pure and locale-independent so values
 * line up in tables and tests are deterministic.
 */

const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB", "PB"] as const;

/** Placeholder shown when a metric is unavailable. */
export const NOT_AVAILABLE = "—";

/**
 * Binary (1024-based) byte formatting with three significant digits:
 * 932 B · 14.2 KB · 82.6 MB · 1.47 GB.
 */
export function formatBytes(bytes: number | null | undefined): string {
  if (bytes == null || !Number.isFinite(bytes) || bytes < 0) return NOT_AVAILABLE;
  if (bytes < 1024) return `${Math.round(bytes)} B`;
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < BYTE_UNITS.length - 1) {
    value /= 1024;
    unit++;
  }
  // Avoid "1024 KB" after rounding: promote to the next unit.
  if (Number(trimPrecision(value)) >= 1024 && unit < BYTE_UNITS.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${trimPrecision(value)} ${BYTE_UNITS[unit]}`;
}

export function formatBytesPerSecond(bytesPerSec: number | null | undefined): string {
  if (bytesPerSec == null) return NOT_AVAILABLE;
  return `${formatBytes(bytesPerSec)}/s`;
}

/** 3 significant digits, but never scientific notation and no trailing ".0". */
function trimPrecision(value: number): string {
  if (value >= 100) return value.toFixed(0);
  if (value >= 10) return value.toFixed(1);
  return value.toFixed(2);
}

/** Duration in whole seconds → "1d 2h 3m", "4h 5m", "3m 12s", "45s". */
export function formatDuration(totalSeconds: number | null | undefined): string {
  if (totalSeconds == null || !Number.isFinite(totalSeconds) || totalSeconds < 0) {
    return NOT_AVAILABLE;
  }
  const s = Math.floor(totalSeconds);
  const days = Math.floor(s / 86_400);
  const hours = Math.floor((s % 86_400) / 3_600);
  const minutes = Math.floor((s % 3_600) / 60);
  const seconds = s % 60;
  if (days > 0) return `${days}d ${hours}h ${minutes}m`;
  if (hours > 0) return `${hours}h ${minutes}m`;
  if (minutes > 0) return `${minutes}m ${seconds}s`;
  return `${seconds}s`;
}

export function formatPercent(value: number | null | undefined, digits = 1): string {
  if (value == null || !Number.isFinite(value)) return NOT_AVAILABLE;
  return `${value.toFixed(digits)}%`;
}

/** MHz → "4.20 GHz" or "800 MHz". */
export function formatFrequency(mhz: number | null | undefined): string {
  if (mhz == null || !Number.isFinite(mhz) || mhz <= 0) return NOT_AVAILABLE;
  if (mhz >= 1000) return `${(mhz / 1000).toFixed(2)} GHz`;
  return `${Math.round(mhz)} MHz`;
}

/** Unix milliseconds → "2026-11-02 13:05:09" in local time. */
export function formatTimestamp(ms: number | null | undefined): string {
  if (ms == null || !Number.isFinite(ms)) return NOT_AVAILABLE;
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return (
    `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ` +
    `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
  );
}

/** Integer with thin grouping: 12345 → "12,345". */
export function formatCount(n: number | null | undefined): string {
  if (n == null || !Number.isFinite(n)) return NOT_AVAILABLE;
  return Math.round(n)
    .toString()
    .replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}
