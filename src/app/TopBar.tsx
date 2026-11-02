import { memo } from "react";

/**
 * Persistent system summary. Phase 1 renders placeholders; live values arrive
 * once the monitoring engine is wired up.
 */
export const TopBar = memo(function TopBar() {
  return (
    <header className="flex h-12 shrink-0 items-center gap-6 border-b border-border bg-surface px-5">
      <SummaryItem label="CPU" value="—" />
      <SummaryItem label="Memory" value="—" />
      <SummaryItem label="Network" value="—" />
      <div className="ml-auto text-fg-subtle">Not connected</div>
    </header>
  );
});

function SummaryItem({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-baseline gap-2">
      <span className="text-[11px] font-medium uppercase tracking-wide text-fg-subtle">
        {label}
      </span>
      <span className="num font-semibold">{value}</span>
    </div>
  );
}
