import { memo } from "react";
import { cn } from "@/lib/cn";
import { usageLevel } from "@/utils/levels";

const TONE = {
  normal: "bg-res-cpu",
  warning: "bg-warn-bar",
  critical: "bg-crit",
} as const;

interface CoreStripProps {
  /** Load per logical core, 0–100. */
  perCore: readonly number[];
  /** Strip height in px. */
  height?: number;
  /** Stretch bars to fill the available width instead of a compact fixed width. */
  fill?: boolean;
  className?: string;
}

/**
 * One bar per logical core. Shows at a glance whether a single thread is
 * pinned or the whole machine is busy, which the overall percentage hides.
 */
export const CoreStrip = memo(function CoreStrip({
  perCore,
  height = 20,
  fill = false,
  className,
}: CoreStripProps) {
  if (perCore.length === 0) return null;
  const label = `Load per core: ${perCore.map((v, i) => `core ${i + 1} ${Math.round(v)}%`).join(", ")}`;
  return (
    <div
      role="img"
      aria-label={label}
      className={cn("flex items-end", fill ? "gap-1" : "gap-[2px]", className)}
      style={{ height }}
    >
      {perCore.map((v, i) => {
        const pct = Math.min(100, Math.max(0, v));
        return (
          <span
            // Cores are a fixed, ordered list, so the index is the identity.
            key={i}
            className={cn(
              fill ? "max-w-6 flex-1 rounded-[3px]" : "w-[5px] rounded-[1.5px]",
              "transition-[height] duration-300",
              TONE[usageLevel(pct)],
            )}
            style={{ height: `${Math.max(pct, 6)}%`, opacity: pct < 3 ? 0.35 : 1 }}
          />
        );
      })}
    </div>
  );
});
