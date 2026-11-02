import type { ReactNode } from "react";
import { cn } from "@/lib/cn";

export interface KeyValueItem {
  label: string;
  value: ReactNode;
  /** Shown as a tooltip explaining platform differences. */
  hint?: string;
}

/** Two-column definition list for labelled values. */
export function KeyValueList({ items, className }: { items: KeyValueItem[]; className?: string }) {
  return (
    <dl className={cn("grid grid-cols-[max-content_1fr] gap-x-6 gap-y-1.5", className)}>
      {items.map((item) => (
        <div key={item.label} className="contents">
          <dt className="text-fg-muted" title={item.hint}>
            {item.label}
            {item.hint && <span className="sr-only"> ({item.hint})</span>}
          </dt>
          <dd className="num selectable min-w-0 truncate text-right font-medium">{item.value}</dd>
        </div>
      ))}
    </dl>
  );
}

/** Rendered when the OS cannot report a metric, instead of a made-up number. */
export function Unavailable() {
  return <span className="font-normal text-fg-subtle">Unavailable on this platform</span>;
}
