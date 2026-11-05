import type { Stability } from "@/types";

export const STABILITY: Record<Stability, { label: string; tone: string }> = {
  high: { label: "High stability", tone: "text-ok" },
  medium: { label: "Medium stability", tone: "text-warn" },
  low: { label: "Low stability", tone: "text-crit" },
};
