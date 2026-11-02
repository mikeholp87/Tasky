import type { ReactNode } from "react";

export function EmptyState({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="flex h-full min-h-48 flex-col items-center justify-center gap-1 rounded-lg border border-dashed border-border p-8 text-center">
      <p className="font-medium">{title}</p>
      {children && <div className="max-w-md text-fg-muted">{children}</div>}
    </div>
  );
}
