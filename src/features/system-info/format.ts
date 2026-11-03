import type { BatteryInfo, CacheInfo, DiskKind } from "@/types";

/** "3.40 GHz" from MHz; MHz below 1 GHz. */
export function formatMhz(mhz: number | null | undefined): string | null {
  if (mhz == null || mhz <= 0) return null;
  return mhz >= 1000 ? `${(mhz / 1000).toFixed(2)} GHz` : `${Math.round(mhz)} MHz`;
}

/** "L1 data", "L2", "L3": unified caches need no kind. */
export function cacheLabel(c: CacheInfo): string {
  return c.kind === "Unified" ? `L${c.level}` : `L${c.level} ${c.kind.toLowerCase()}`;
}

export const DISK_KIND_LABEL: Record<DiskKind, string> = {
  ssd: "Solid-state drive",
  hdd: "Hard disk drive",
  unknown: "Unknown",
};

/** "82% · Discharging" style summary of a battery's present state. */
export function batteryState(b: BatteryInfo): string | null {
  const parts = [b.percent != null ? `${b.percent}%` : null, b.status].filter(Boolean);
  return parts.length > 0 ? parts.join(" · ") : null;
}

export interface Screen {
  width: number;
  height: number;
  pixelRatio: number;
}

/** "2560 × 1600 (scale 2×)" */
export function describeScreen(s: Screen): string {
  const w = Math.round(s.width * s.pixelRatio);
  const h = Math.round(s.height * s.pixelRatio);
  return s.pixelRatio === 1 ? `${w} × ${h}` : `${w} × ${h} (scale ${s.pixelRatio}×)`;
}
