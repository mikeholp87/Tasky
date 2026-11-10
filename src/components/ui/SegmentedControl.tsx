import { RadioGroup } from "radix-ui";
import type { ReactNode } from "react";
import { cn } from "@/lib/cn";

export interface Segment<T extends string> {
  value: T;
  label: ReactNode;
  /** Accessible name and tooltip, for icon-only segments. */
  title?: string;
}

/**
 * Single-choice button strip. One tab stop; arrow keys move between and
 * select segments.
 */
export function SegmentedControl<T extends string>({
  value,
  onChange,
  segments,
  label,
  labelledBy,
  disabled,
  className,
  itemClassName,
}: {
  value: T;
  onChange: (value: T) => void;
  segments: readonly Segment<T>[];
  label?: string;
  labelledBy?: string;
  disabled?: boolean;
  className?: string;
  itemClassName?: string;
}) {
  return (
    <RadioGroup.Root
      value={value}
      onValueChange={(v) => onChange(v as T)}
      disabled={disabled}
      aria-label={label}
      aria-labelledby={labelledBy}
      className={cn("flex gap-0.5 rounded-md bg-surface-2 p-0.5", className)}
    >
      {segments.map((s) => (
        <RadioGroup.Item
          key={s.value}
          value={s.value}
          aria-label={s.title}
          title={s.title}
          className={cn(
            "rounded px-2.5 py-1 text-fg-muted data-[state=checked]:bg-surface data-[state=checked]:text-fg data-[state=checked]:shadow-sm",
            itemClassName,
          )}
        >
          {s.label}
        </RadioGroup.Item>
      ))}
    </RadioGroup.Root>
  );
}
