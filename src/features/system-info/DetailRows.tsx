import type { ReactNode } from "react";

export interface DetailItem {
  label: string;
  value: ReactNode;
  /** Explanation shown under the value. */
  note?: string;
}

/**
 * Label and value side by side with the value left-aligned: in a wide pane,
 * right-aligned values sit too far from the label they belong to.
 */
export function DetailRows({ items }: { items: readonly DetailItem[] }) {
  return (
    <dl className="divide-y divide-border/60">
      {items.map((item) => (
        <div
          key={item.label}
          className="grid grid-cols-1 gap-x-6 gap-y-0.5 py-2 sm:grid-cols-[12rem_minmax(0,1fr)]"
        >
          <dt className="text-fg-muted">{item.label}</dt>
          <dd className="num selectable min-w-0 break-words font-medium">
            {item.value}
            {item.note && (
              <span className="mt-0.5 block font-normal text-fg-muted">{item.note}</span>
            )}
          </dd>
        </div>
      ))}
    </dl>
  );
}

/** A named block inside a pane, e.g. one disk or one network adapter. */
export function DetailGroup({ title, children }: { title?: ReactNode; children: ReactNode }) {
  return (
    <section className="mt-6 first:mt-0">
      {title && <h3 className="mb-1 truncate text-[0.875rem] font-semibold">{title}</h3>}
      {children}
    </section>
  );
}
