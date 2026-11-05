import { Pause, Play, RefreshCw } from "lucide-react";
import { memo } from "react";
import { cn } from "@/lib/cn";
import { CoreStrip } from "@/components/ui/CoreStrip";
import { refreshNow, togglePaused } from "@/services/monitoring";
import { useMetricsStore } from "@/stores/metricsStore";
import { formatBytesPerSecond, formatPercent, NOT_AVAILABLE } from "@/utils/format";
import { usageLevel } from "@/utils/levels";

/** Persistent system summary and monitoring controls. */
export const TopBar = memo(function TopBar() {
  const cpu = useMetricsStore((s) => s.tick?.cpu.usagePercent);
  const mem = useMetricsStore((s) => s.tick?.memory.usedPercent);
  const perCore = useMetricsStore((s) => s.tick?.cpu.perCore);
  const rx = useMetricsStore((s) => s.tick?.point.netRx);
  const tx = useMetricsStore((s) => s.tick?.point.netTx);

  return (
    <header className="flex h-12 shrink-0 items-center gap-6 border-b border-border bg-surface px-5">
      <SummaryItem label="CPU" value={cpu}>
        {perCore && <CoreStrip perCore={perCore} height={18} className="self-center" />}
      </SummaryItem>
      <SummaryItem label="Memory" value={mem} />
      <div className="flex items-baseline gap-2">
        <span className="text-fg-subtle">Network</span>
        <span className="num font-semibold">
          {rx == null || tx == null
            ? NOT_AVAILABLE
            : `↓ ${formatBytesPerSecond(rx)}  ↑ ${formatBytesPerSecond(tx)}`}
        </span>
      </div>
      <div className="ml-auto flex items-center gap-2">
        <MonitoringStatus />
      </div>
    </header>
  );
});

function SummaryItem({
  label,
  value,
  children,
}: {
  label: string;
  value: number | undefined;
  children?: React.ReactNode;
}) {
  const level = usageLevel(value);
  return (
    <div className="flex items-baseline gap-2">
      <span className="text-fg-subtle">{label}</span>
      <span
        className={cn(
          "num w-14 font-semibold",
          level === "critical" && "text-crit",
          level === "warning" && "text-warn",
        )}
      >
        {formatPercent(value)}
      </span>
      {children}
    </div>
  );
}

function MonitoringStatus() {
  const status = useMetricsStore((s) => s.status);
  const paused = useMetricsStore((s) => s.monitoring.paused);
  const live = status === "live";

  let text: string;
  if (status === "unavailable") text = "Not connected";
  else if (status === "error") text = "Monitoring error";
  else if (status === "connecting") text = "Connecting…";
  else text = paused ? "Monitoring paused" : "Live";

  return (
    <>
      <span role="status" className="flex items-center gap-1.5 text-fg-muted">
        <span
          aria-hidden
          className={cn(
            "size-2 rounded-full",
            live && !paused && "bg-ok",
            live && paused && "bg-warn",
            !live && "bg-fg-subtle",
          )}
        />
        {text}
      </span>
      <IconButton
        label={paused ? "Resume monitoring" : "Pause monitoring"}
        disabled={!live}
        onClick={() => void togglePaused()}
      >
        {paused ? <Play size="0.9375rem" /> : <Pause size="0.9375rem" />}
      </IconButton>
      <IconButton label="Refresh now" disabled={!live} onClick={() => void refreshNow()}>
        <RefreshCw size="0.9375rem" />
      </IconButton>
    </>
  );
}

function IconButton({
  label,
  onClick,
  disabled,
  children,
}: {
  label: string;
  onClick: () => void;
  disabled?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={onClick}
      className="grid size-8 place-items-center rounded-md text-fg-muted hover:bg-surface-2 hover:text-fg disabled:opacity-40 disabled:hover:bg-transparent"
    >
      {children}
    </button>
  );
}
