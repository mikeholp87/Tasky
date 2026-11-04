import { Pause, Play, RefreshCw } from "lucide-react";
import { memo } from "react";
import { cn } from "@/lib/cn";
import { refreshNow, togglePaused } from "@/services/monitoring";
import { useMetricsStore } from "@/stores/metricsStore";
import { useUiStore } from "@/stores/uiStore";
import { formatBytesPerSecond, formatPercent, NOT_AVAILABLE } from "@/utils/format";
import { usageLevel, type UsageLevel } from "@/utils/levels";
import type { RouteId } from "./routes";

const BAR_FILL: Record<UsageLevel, string> = {
  normal: "bg-accent",
  warning: "bg-warn",
  critical: "bg-crit",
};

/** Persistent system summary and monitoring controls. */
export const TopBar = memo(function TopBar() {
  const cpu = useMetricsStore((s) => s.tick?.cpu.usagePercent);
  const mem = useMetricsStore((s) => s.tick?.memory.usedPercent);
  const gpus = useMetricsStore((s) => s.tick?.gpus);
  const gpuUsagePoint = useMetricsStore((s) => s.tick?.point.gpuUsage);
  const rx = useMetricsStore((s) => s.tick?.point.netRx);
  const tx = useMetricsStore((s) => s.tick?.point.netTx);
  const navigate = useUiStore((s) => s.navigate);

  const primaryGpu = gpus?.[0];
  const gpuPct = primaryGpu ? (primaryGpu.usagePercent ?? gpuUsagePoint ?? undefined) : undefined;
  const gpuTemp =
    primaryGpu?.temperatureCelsius != null
      ? `${Math.round(primaryGpu.temperatureCelsius)}°C`
      : undefined;

  return (
    <header className="flex h-12 shrink-0 items-center gap-4 border-b border-border bg-surface px-5">
      <SummaryItem label="CPU" value={cpu} route="cpu" onNavigate={navigate} />
      <SummaryItem label="Memory" value={mem} route="memory" onNavigate={navigate} />
      {gpus && gpus.length > 0 && (
        <SummaryItem
          label="GPU"
          value={gpuPct}
          badge={gpuTemp}
          route="gpu"
          onNavigate={navigate}
        />
      )}
      <button
        type="button"
        onClick={() => navigate("network")}
        aria-label="Open Network details"
        className="flex items-baseline gap-2 rounded-md px-2 py-1 text-left transition-colors hover:bg-surface-2"
      >
        <span className="text-[11px] font-medium uppercase tracking-wide text-fg-subtle">
          Network
        </span>
        <span className="num font-semibold">
          {rx == null || tx == null
            ? NOT_AVAILABLE
            : `↓ ${formatBytesPerSecond(rx)}  ↑ ${formatBytesPerSecond(tx)}`}
        </span>
      </button>
      <div className="ml-auto flex items-center gap-2">
        <MonitoringStatus />
      </div>
    </header>
  );
});

function SummaryItem({
  label,
  value,
  badge,
  route,
  onNavigate,
}: {
  label: string;
  value: number | undefined;
  badge?: string;
  route: RouteId;
  onNavigate: (r: RouteId) => void;
}) {
  const level = usageLevel(value);
  const clamped = value != null && Number.isFinite(value) ? Math.min(100, Math.max(0, value)) : 0;
  return (
    <button
      type="button"
      onClick={() => onNavigate(route)}
      aria-label={`Open ${label} details`}
      className="flex items-center gap-2 rounded-md px-2 py-1 text-left transition-colors hover:bg-surface-2"
    >
      <span className="text-[11px] font-medium uppercase tracking-wide text-fg-subtle">
        {label}
      </span>
      <span
        aria-hidden
        className="h-1.5 w-10 overflow-hidden rounded-full bg-surface-2"
      >
        <span
          className={cn("block h-full rounded-full transition-[width] duration-300", BAR_FILL[level])}
          style={{ width: `${clamped}%` }}
        />
      </span>
      <span
        className={cn(
          "num w-12 font-semibold",
          level === "critical" && "text-crit",
          level === "warning" && "text-warn",
        )}
      >
        {formatPercent(value)}
      </span>
      {badge && <span className="num text-[11px] text-fg-muted">{badge}</span>}
    </button>
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
        {paused ? <Play size={15} /> : <Pause size={15} />}
      </IconButton>
      <IconButton label="Refresh now" disabled={!live} onClick={() => void refreshNow()}>
        <RefreshCw size={15} />
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
