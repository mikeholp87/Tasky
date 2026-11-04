import {
  Cpu,
  Gpu,
  HardDrive,
  type LucideIcon,
  MemoryStick,
  Network,
  Pause,
  Play,
  RefreshCw,
} from "lucide-react";
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
  const diskRead = useMetricsStore((s) => s.tick?.point.diskRead);
  const diskWrite = useMetricsStore((s) => s.tick?.point.diskWrite);
  const rx = useMetricsStore((s) => s.tick?.point.netRx);
  const tx = useMetricsStore((s) => s.tick?.point.netTx);
  const currentRoute = useUiStore((s) => s.route);
  const navigate = useUiStore((s) => s.navigate);

  const primaryGpu = gpus?.[0];
  const gpuPct = primaryGpu ? (primaryGpu.usagePercent ?? gpuUsagePoint ?? undefined) : undefined;
  const gpuTemp =
    primaryGpu?.temperatureCelsius != null
      ? `${Math.round(primaryGpu.temperatureCelsius)}°C`
      : undefined;

  return (
    <header className="flex h-12 shrink-0 items-center gap-2 overflow-x-auto border-b border-border bg-surface px-4">
      <SummaryItem
        icon={Cpu}
        iconColor="text-[var(--chart-1)]"
        label="CPU"
        fullName="CPU"
        value={cpu}
        route="cpu"
        active={currentRoute === "cpu"}
        onNavigate={navigate}
      />
      <SummaryItem
        icon={MemoryStick}
        iconColor="text-[var(--chart-2)]"
        label="MEM"
        fullName="Memory"
        value={mem}
        route="memory"
        active={currentRoute === "memory"}
        onNavigate={navigate}
      />
      {gpus && gpus.length > 0 && (
        <SummaryItem
          icon={Gpu}
          iconColor="text-[var(--chart-3)]"
          label="GPU"
          fullName="GPU"
          value={gpuPct}
          badge={gpuTemp}
          route="gpu"
          active={currentRoute === "gpu"}
          onNavigate={navigate}
        />
      )}
      <button
        type="button"
        onClick={() => navigate("storage")}
        aria-label="Open Disk details"
        className={cn(
          "flex shrink-0 items-center gap-2 rounded-md border px-2.5 py-1 text-left transition-colors",
          currentRoute === "storage"
            ? "border-accent bg-accent-soft/60"
            : "border-border/70 bg-surface-2/40 hover:border-border-strong hover:bg-surface-2",
        )}
      >
        <HardDrive size={13} className="shrink-0 text-[var(--chart-4)]" aria-hidden />
        <span className="text-[11px] font-semibold uppercase tracking-wide text-fg-muted">
          DISK
        </span>
        <span className="num text-[12px] font-semibold text-fg">
          {diskRead == null || diskWrite == null
            ? NOT_AVAILABLE
            : `R ${formatBytesPerSecond(diskRead)} · W ${formatBytesPerSecond(diskWrite)}`}
        </span>
      </button>
      <button
        type="button"
        onClick={() => navigate("network")}
        aria-label="Open Network details"
        className={cn(
          "flex shrink-0 items-center gap-2 rounded-md border px-2.5 py-1 text-left transition-colors",
          currentRoute === "network"
            ? "border-accent bg-accent-soft/60"
            : "border-border/70 bg-surface-2/40 hover:border-border-strong hover:bg-surface-2",
        )}
      >
        <Network size={13} className="shrink-0 text-[var(--chart-1)]" aria-hidden />
        <span className="text-[11px] font-semibold uppercase tracking-wide text-fg-muted">
          NET
        </span>
        <span className="num text-[12px] font-semibold text-fg">
          {rx == null || tx == null
            ? NOT_AVAILABLE
            : `↓ ${formatBytesPerSecond(rx)}  ↑ ${formatBytesPerSecond(tx)}`}
        </span>
      </button>
      <div className="ml-auto flex shrink-0 items-center gap-1.5 pl-2">
        <MonitoringStatus />
      </div>
    </header>
  );
});

function SummaryItem({
  icon: Icon,
  iconColor,
  label,
  fullName,
  value,
  badge,
  route,
  active,
  onNavigate,
}: {
  icon: LucideIcon;
  iconColor: string;
  label: string;
  fullName: string;
  value: number | undefined;
  badge?: string;
  route: RouteId;
  active: boolean;
  onNavigate: (r: RouteId) => void;
}) {
  const level = usageLevel(value);
  const clamped = value != null && Number.isFinite(value) ? Math.min(100, Math.max(0, value)) : 0;
  return (
    <button
      type="button"
      onClick={() => onNavigate(route)}
      aria-label={`Open ${fullName} details`}
      className={cn(
        "flex shrink-0 items-center gap-2 rounded-md border px-2.5 py-1 text-left transition-colors",
        active
          ? "border-accent bg-accent-soft/60"
          : "border-border/70 bg-surface-2/40 hover:border-border-strong hover:bg-surface-2",
      )}
    >
      <Icon size={13} className={cn("shrink-0", iconColor)} aria-hidden />
      <span className="text-[11px] font-semibold uppercase tracking-wide text-fg-muted">
        {label}
      </span>
      <span
        aria-hidden
        className="h-2 w-12 overflow-hidden rounded-full bg-border/80"
      >
        <span
          className={cn("block h-full rounded-full transition-[width] duration-300", BAR_FILL[level])}
          style={{ width: `${clamped}%` }}
        />
      </span>
      <span
        className={cn(
          "num text-[12px] font-semibold text-fg",
          level === "critical" && "text-crit",
          level === "warning" && "text-warn",
        )}
      >
        {formatPercent(value, 0)}
      </span>
      {badge && (
        <span className="num rounded bg-surface-2 px-1 py-0.5 text-[11px] font-medium text-fg-muted">
          {badge}
        </span>
      )}
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
