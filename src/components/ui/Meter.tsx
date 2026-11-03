import { memo } from "react";
import { cn } from "@/lib/cn";
import { usageLevel, type UsageLevel } from "@/utils/levels";

const FILL: Record<UsageLevel, string> = {
  normal: "",
  warning: "bg-warn-bar",
  critical: "bg-crit",
};

interface MeterProps {
  /** 0–100. */
  percent: number;
  label: string;
  className?: string;
  size?: "sm" | "md";
  /** CSS variable for the fill while usage is normal, e.g. "--res-mem". */
  color?: string;
}

/** Horizontal usage bar. Exposed to assistive tech as a meter. */
export const Meter = memo(function Meter({
  percent,
  label,
  className,
  size = "md",
  color,
}: MeterProps) {
  const clamped = Math.min(100, Math.max(0, percent));
  return (
    <div
      role="meter"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(clamped)}
      className={cn(
        "w-full overflow-hidden rounded-full bg-surface-2",
        size === "sm" ? "h-1.5" : "h-2",
        className,
      )}
    >
      <div
        className={cn(
          "h-full rounded-full transition-[width] duration-300",
          FILL[usageLevel(clamped)],
        )}
        style={{
          width: `${clamped}%`,
          ...(usageLevel(clamped) === "normal"
            ? { background: `var(${color ?? "--accent"})` }
            : {}),
        }}
      />
    </div>
  );
});
