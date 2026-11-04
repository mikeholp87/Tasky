import type { ReactNode } from "react";
import { useState } from "react";
import { ConnectionNotice } from "@/components/ConnectionNotice";
import { PageHeader } from "@/components/PageHeader";
import { type ChartSeries, TimeSeriesChart } from "@/components/charts/TimeSeriesChart";
import {
  CPU_SERIES,
  coreSeries,
  DISK_SERIES,
  GPU_SERIES,
  GPU_TEMP_SERIES,
  GPU_VRAM_SERIES,
  MEMORY_SERIES,
  NETWORK_SERIES,
  percentAxis,
  rateAxis,
  RATE_MIN_Y,
  tempAxis,
} from "@/components/charts/series";
import { Card } from "@/components/ui/Card";
import { KeyValueList, Unavailable } from "@/components/ui/KeyValueList";
import { Meter } from "@/components/ui/Meter";
import { cn } from "@/lib/cn";
import { useMetricsStore } from "@/stores/metricsStore";
import { useUiStore } from "@/stores/uiStore";
import type { MetricsTick, StaticSystemInfo } from "@/types";
import {
  formatBytes,
  formatBytesPerSecond,
  formatCount,
  formatDuration,
  formatFrequency,
  formatPercent,
} from "@/utils/format";
import { usageLevel } from "@/utils/levels";
import { uptimeSeconds } from "@/utils/system";

type Resource = "cpu" | "memory" | "disk" | "network" | "gpu";

const RANGES = [
  { seconds: 60, label: "60 s" },
  { seconds: 300, label: "5 min" },
  { seconds: 900, label: "15 min" },
  { seconds: 3600, label: "1 h" },
] as const;

interface ResourceDef {
  id: Resource;
  label: string;
  series: readonly ChartSeries[];
  yMax?: number;
  minYMax?: number;
  format: (v: number) => string;
  summary: (t: MetricsTick) => string;
}

const RESOURCES: readonly ResourceDef[] = [
  {
    id: "cpu",
    label: "CPU",
    series: CPU_SERIES,
    yMax: 100,
    format: percentAxis,
    summary: (t) => formatPercent(t.cpu.usagePercent, 0),
  },
  {
    id: "memory",
    label: "Memory",
    series: MEMORY_SERIES,
    yMax: 100,
    format: percentAxis,
    summary: (t) =>
      `${formatBytes(t.memory.usedBytes)} (${formatPercent(t.memory.usedPercent, 0)})`,
  },
  {
    id: "disk",
    label: "Disk",
    series: DISK_SERIES,
    format: rateAxis,
    minYMax: RATE_MIN_Y,
    summary: (t) => formatBytesPerSecond(t.point.diskRead + t.point.diskWrite),
  },
  {
    id: "network",
    label: "Network",
    series: NETWORK_SERIES,
    format: rateAxis,
    minYMax: RATE_MIN_Y,
    summary: (t) =>
      `↓ ${formatBytesPerSecond(t.point.netRx)} ↑ ${formatBytesPerSecond(t.point.netTx)}`,
  },
  {
    id: "gpu",
    label: "GPU",
    series: GPU_SERIES,
    yMax: 100,
    format: percentAxis,
    summary: (t) => {
      const gpu = t.gpus[0];
      if (!gpu) return "Not detected";
      return `${formatBytes(gpu.usedVramBytes)} (${formatPercent(t.point.gpuUsage ?? 0, 0)})`;
    },
  },
];

export function PerformancePage() {
  const tick = useMetricsStore((s) => s.tick);
  const info = useMetricsStore((s) => s.staticInfo);
  const [selected, setSelected] = useState<Resource>("cpu");
  const [seconds, setSeconds] = useState<number>(60);
  const [cpuGraphMode, setCpuGraphMode] = useState<"overall" | "cores">("overall");
  const def = RESOURCES.find((r) => r.id === selected) as ResourceDef;
  const rangeLabel = RANGES.find((r) => r.seconds === seconds)?.label;
  const subtitle = tick ? resourceHardwareSubtitle(selected, tick, info) : null;

  return (
    <div className="flex min-h-full flex-col">
      <PageHeader
        title="Performance"
        description="Live resource history, kept in memory for up to one hour."
        actions={<RangePicker value={seconds} onChange={setSeconds} />}
      />
      {!tick ? (
        <ConnectionNotice />
      ) : (
        <div className="grid flex-1 items-stretch gap-4 lg:grid-cols-[260px_1fr]">
          <nav aria-label="Resources" className="flex flex-col gap-2">
            {RESOURCES.map((r) => (
              <button
                key={r.id}
                type="button"
                aria-pressed={selected === r.id}
                onClick={() => setSelected(r.id)}
                className={cn(
                  "block w-full rounded-lg border p-3 text-left transition-colors",
                  selected === r.id
                    ? "border-accent bg-accent-soft"
                    : "border-border bg-surface hover:bg-surface-2",
                )}
              >
                <div className="flex items-baseline justify-between gap-2">
                  <span className="font-medium">{r.label}</span>
                  <span className="num truncate text-[12px] text-fg-muted">{r.summary(tick)}</span>
                </div>
                <div className="mt-2">
                  <TimeSeriesChart
                    series={r.series}
                    seconds={60}
                    height={36}
                    yMax={r.yMax}
                    minYMax={r.minYMax}
                    formatValue={r.format}
                    sparkline
                    ariaLabel={`${r.label} over the last minute`}
                  />
                </div>
              </button>
            ))}
          </nav>

          <Card className="flex min-w-0 flex-col p-4">
            <div className="mb-3 flex flex-wrap items-baseline justify-between gap-3">
              <div className="flex min-w-0 flex-wrap items-baseline gap-x-2.5 gap-y-1">
                <h2 className="text-base font-semibold">{def.label}</h2>
                {subtitle && <span className="truncate text-[13px] text-fg-muted">{subtitle}</span>}
              </div>
              <div className="flex items-center gap-3">
                {selected === "cpu" && tick.cpu.perCore.length > 0 && (
                  <div
                    role="radiogroup"
                    aria-label="CPU graph view"
                    className="flex gap-0.5 rounded-md bg-surface-2 p-0.5 text-[11px]"
                  >
                    <button
                      type="button"
                      role="radio"
                      aria-checked={cpuGraphMode === "overall"}
                      onClick={() => setCpuGraphMode("overall")}
                      className={cn(
                        "rounded px-2 py-0.5 text-fg-muted transition-colors",
                        cpuGraphMode === "overall" && "bg-surface font-medium text-fg shadow-sm",
                      )}
                    >
                      Overall
                    </button>
                    <button
                      type="button"
                      role="radio"
                      aria-checked={cpuGraphMode === "cores"}
                      onClick={() => setCpuGraphMode("cores")}
                      className={cn(
                        "rounded px-2 py-0.5 text-fg-muted transition-colors",
                        cpuGraphMode === "cores" && "bg-surface font-medium text-fg shadow-sm",
                      )}
                    >
                      Logical processors
                    </button>
                  </div>
                )}
                <Legend series={def.series} />
              </div>
            </div>

            {selected === "cpu" && cpuGraphMode === "cores" && tick.cpu.perCore.length > 0 ? (
              <CpuLogicalProcessorsChartGrid
                perCore={tick.cpu.perCore}
                seconds={seconds}
                rangeLabel={rangeLabel}
              />
            ) : (
              <TimeSeriesChart
                series={def.series}
                seconds={seconds}
                height={280}
                yMax={def.yMax}
                minYMax={def.minYMax}
                formatValue={def.format}
                ariaLabel={`${def.label} over the last ${rangeLabel}`}
              />
            )}

            <HeroKpiStrip resource={selected} tick={tick} info={info} />

            {selected === "cpu" && <CpuCoreStrip perCore={tick.cpu.perCore} />}

            {selected === "memory" && <MemoryCompositionSection tick={tick} />}

            {selected === "disk" && <MountedDrivesGrid tick={tick} />}

            {selected === "network" && <ActiveInterfacesGrid tick={tick} />}

            {selected === "gpu" && (
              <div className="mt-4 grid gap-4 border-t border-border pt-4 md:grid-cols-2">
                <div>
                  <div className="mb-1.5 flex items-baseline justify-between">
                    <span className="text-[12px] font-medium text-fg-muted">VRAM Usage</span>
                    <span className="num text-[12px] font-semibold">
                      {formatPercent(tick.point.gpuVramPercent)}
                    </span>
                  </div>
                  <TimeSeriesChart
                    series={GPU_VRAM_SERIES}
                    seconds={seconds}
                    height={120}
                    yMax={100}
                    formatValue={percentAxis}
                    ariaLabel={`GPU VRAM usage over the last ${rangeLabel}`}
                  />
                </div>
                <div>
                  <div className="mb-1.5 flex items-baseline justify-between">
                    <span className="text-[12px] font-medium text-fg-muted">Temperature</span>
                    <span className="num text-[12px] font-semibold">
                      {tick.point.gpuTemp != null ? `${Math.round(tick.point.gpuTemp)} °C` : "—"}
                    </span>
                  </div>
                  <TimeSeriesChart
                    series={GPU_TEMP_SERIES}
                    seconds={seconds}
                    height={120}
                    yMax={100}
                    formatValue={tempAxis}
                    ariaLabel={`GPU temperature over the last ${rangeLabel}`}
                  />
                </div>
              </div>
            )}

            <div className="mt-4 grid gap-x-10 gap-y-4 border-t border-border pt-4 md:grid-cols-3">
              <Stats resource={selected} tick={tick} info={info} />
            </div>

            <TopResourceProcesses resource={selected} tick={tick} />
          </Card>
        </div>
      )}
    </div>
  );
}

function resourceHardwareSubtitle(
  resource: Resource,
  tick: MetricsTick,
  info: StaticSystemInfo | null,
): string | null {
  switch (resource) {
    case "cpu": {
      const parts: string[] = [];
      if (info?.cpuBrand) parts.push(info.cpuBrand);
      if (info?.physicalCoreCount != null && info?.logicalCoreCount != null) {
        parts.push(`${info.physicalCoreCount}C / ${info.logicalCoreCount}T`);
      } else if (info?.logicalCoreCount != null) {
        parts.push(`${info.logicalCoreCount} Logical Processors`);
      }
      return parts.length > 0 ? parts.join(" · ") : null;
    }
    case "memory":
      return `${formatBytes(tick.memory.totalBytes)} Physical RAM`;
    case "disk": {
      const totalDiskBytes = tick.disks.reduce((s, d) => s + d.totalBytes, 0);
      return `${tick.disks.length} Mounted ${tick.disks.length === 1 ? "Drive" : "Drives"} · ${formatBytes(totalDiskBytes)} Total`;
    }
    case "network": {
      const primary = tick.networks.find((n) => n.isPrimary) ?? tick.networks[0];
      if (!primary) return "No active interfaces";
      const ip = primary.ipv4[0];
      return ip ? `${primary.name} · ${ip}` : primary.name;
    }
    case "gpu":
      return tick.gpus[0]?.name ?? "No dedicated GPU detected";
  }
}

function HeroKpiStrip({
  resource,
  tick,
  info,
}: {
  resource: Resource;
  tick: MetricsTick;
  info: StaticSystemInfo | null;
}) {
  let cards: { label: string; value: string; sub?: string }[];

  switch (resource) {
    case "cpu": {
      const la = tick.cpu.loadAverage;
      cards = [
        {
          label: "Utilization",
          value: formatPercent(tick.cpu.usagePercent),
          sub: `${tick.cpu.perCore.length} logical processors`,
        },
        {
          label: "Clock Speed",
          value: formatFrequency(tick.cpu.frequencyMhz),
          sub: info?.architecture ?? undefined,
        },
        {
          label: "Load Average (1m)",
          value: la ? la.one.toFixed(2) : "—",
          sub: la ? `5m ${la.five.toFixed(2)} · 15m ${la.fifteen.toFixed(2)}` : undefined,
        },
        {
          label: "Processes",
          value: formatCount(tick.engine.processCount),
          sub: `Uptime ${formatDuration(uptimeSeconds(info?.bootTimeSecs ?? null, tick.timestampMs))}`,
        },
      ];
      break;
    }
    case "memory": {
      const m = tick.memory;
      const swapPct = m.swapTotalBytes > 0 ? (m.swapUsedBytes / m.swapTotalBytes) * 100 : 0;
      cards = [
        {
          label: "In Use",
          value: formatBytes(m.usedBytes),
          sub: `${formatPercent(m.usedPercent)} of ${formatBytes(m.totalBytes)}`,
        },
        {
          label: "Available",
          value: formatBytes(m.availableBytes),
          sub: "Ready for allocation",
        },
        {
          label: "Page Cache",
          value: m.cachedBytes != null ? formatBytes(m.cachedBytes) : "—",
          sub: m.buffersBytes != null ? `Buffers ${formatBytes(m.buffersBytes)}` : undefined,
        },
        {
          label: "Swap Used",
          value: m.swapTotalBytes > 0 ? formatBytes(m.swapUsedBytes) : "—",
          sub:
            m.swapTotalBytes > 0
              ? `${formatPercent(swapPct, 0)} of ${formatBytes(m.swapTotalBytes)}`
              : "No swap configured",
        },
      ];
      break;
    }
    case "disk": {
      const readIops = tick.disks.reduce((s, d) => s + (d.readOpsPerSec ?? 0), 0);
      const writeIops = tick.disks.reduce((s, d) => s + (d.writeOpsPerSec ?? 0), 0);
      const active = tick.disks.map((d) => d.activePercent).filter((a): a is number => a != null);
      cards = [
        {
          label: "Read Rate",
          value: formatBytesPerSecond(tick.point.diskRead),
          sub: `${formatCount(readIops)} read IOPS`,
        },
        {
          label: "Write Rate",
          value: formatBytesPerSecond(tick.point.diskWrite),
          sub: `${formatCount(writeIops)} write IOPS`,
        },
        {
          label: "Combined IOPS",
          value: formatCount(readIops + writeIops),
          sub: `Throughput ${formatBytesPerSecond(tick.point.diskRead + tick.point.diskWrite)}`,
        },
        {
          label: "Active Time",
          value: active.length > 0 ? formatPercent(Math.max(...active)) : "—",
          sub: `Across ${tick.disks.length} mounted ${tick.disks.length === 1 ? "drive" : "drives"}`,
        },
      ];
      break;
    }
    case "network": {
      const totalRx = tick.networks.reduce((s, n) => s + n.totalRxBytes, 0);
      const totalTx = tick.networks.reduce((s, n) => s + n.totalTxBytes, 0);
      cards = [
        {
          label: "Download Rate",
          value: formatBytesPerSecond(tick.point.netRx),
          sub: "Inbound traffic",
        },
        {
          label: "Upload Rate",
          value: formatBytesPerSecond(tick.point.netTx),
          sub: "Outbound traffic",
        },
        {
          label: "Total Received",
          value: formatBytes(totalRx),
          sub: `${tick.networks.length} active ${tick.networks.length === 1 ? "interface" : "interfaces"}`,
        },
        {
          label: "Total Sent",
          value: formatBytes(totalTx),
          sub: `Combined ${formatBytesPerSecond(tick.point.netRx + tick.point.netTx)}`,
        },
      ];
      break;
    }
    case "gpu": {
      const gpu = tick.gpus[0];
      const vramPct =
        gpu && gpu.totalVramBytes > 0 ? (gpu.usedVramBytes / gpu.totalVramBytes) * 100 : null;
      cards = [
        {
          label: "Core Load",
          value: gpu ? formatPercent(gpu.usagePercent ?? tick.point.gpuUsage) : "—",
          sub: gpu?.clockMhz != null ? `Clock ${formatFrequency(gpu.clockMhz)}` : undefined,
        },
        {
          label: "VRAM Used",
          value: gpu ? formatBytes(gpu.usedVramBytes) : "—",
          sub:
            gpu && vramPct != null
              ? `${formatPercent(vramPct, 0)} of ${formatBytes(gpu.totalVramBytes)}`
              : undefined,
        },
        {
          label: "Temperature",
          value: gpu?.temperatureCelsius != null ? `${Math.round(gpu.temperatureCelsius)} °C` : "—",
          sub:
            gpu?.fanSpeedPercent != null
              ? `Fan ${formatPercent(gpu.fanSpeedPercent, 0)}`
              : undefined,
        },
        {
          label: "Power Draw",
          value: gpu?.powerWatts != null ? `${gpu.powerWatts.toFixed(1)} W` : "—",
          sub:
            gpu?.powerLimitWatts != null
              ? `Limit ${gpu.powerLimitWatts.toFixed(0)} W`
              : undefined,
        },
      ];
      break;
    }
  }

  return (
    <div className="mt-4 grid grid-cols-2 gap-2.5 sm:grid-cols-4">
      {cards.map((c) => (
        <div
          key={c.label}
          className="rounded-md border border-border bg-surface-2/30 px-3 py-2.5"
        >
          <div className="text-[11px] font-medium uppercase tracking-wide text-fg-muted">
            {c.label}
          </div>
          <div className="num mt-0.5 text-lg font-semibold text-fg">{c.value}</div>
          {c.sub && <div className="num mt-0.5 truncate text-[11px] text-fg-subtle">{c.sub}</div>}
        </div>
      ))}
    </div>
  );
}

function CpuLogicalProcessorsChartGrid({
  perCore,
  seconds,
  rangeLabel,
}: {
  perCore: readonly number[];
  seconds: number;
  rangeLabel?: string;
}) {
  return (
    <div className="grid grid-cols-2 gap-2.5 sm:grid-cols-3 xl:grid-cols-4">
      {perCore.map((pct, i) => (
        <div key={i} className="rounded-md border border-border bg-surface-2/25 p-2.5">
          <div className="mb-1 flex items-baseline justify-between gap-1 text-[11px]">
            <span className="whitespace-nowrap font-medium text-fg-muted">CPU {i}</span>
            <span className="num font-semibold text-fg">{formatPercent(pct, 0)}</span>
          </div>
          <TimeSeriesChart
            series={coreSeries(i)}
            seconds={seconds}
            height={56}
            yMax={100}
            formatValue={percentAxis}
            sparkline
            ariaLabel={`CPU ${i} over the last ${rangeLabel}`}
          />
        </div>
      ))}
    </div>
  );
}

function CpuCoreStrip({ perCore }: { perCore: readonly number[] }) {
  if (perCore.length === 0) return null;
  return (
    <div className="mt-4 border-t border-border pt-4">
      <div className="mb-2 flex items-baseline justify-between">
        <span className="text-[12px] font-semibold uppercase tracking-wide text-fg-muted">
          Logical Processor Utilization ({perCore.length})
        </span>
        <span className="num text-[12px] text-fg-subtle">
          Peak {formatPercent(Math.max(...perCore), 0)} · Min{" "}
          {formatPercent(Math.min(...perCore), 0)}
        </span>
      </div>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(115px,1fr))] gap-2">
        {perCore.map((pct, i) => {
          const level = usageLevel(pct);
          return (
            <div
              key={i}
              className="rounded border border-border bg-surface-2/40 px-2.5 py-1.5"
            >
              <div className="mb-1 flex items-baseline justify-between gap-1.5 text-[11px]">
                <span className="whitespace-nowrap text-fg-muted">CPU {i}</span>
                <span
                  className={cn(
                    "num whitespace-nowrap font-semibold",
                    level === "critical" && "text-crit",
                    level === "warning" && "text-warn",
                  )}
                >
                  {formatPercent(pct, 0)}
                </span>
              </div>
              <Meter percent={pct} label={`CPU ${i} utilization`} size="sm" />
            </div>
          );
        })}
      </div>
    </div>
  );
}

function TopResourceProcesses({ resource, tick }: { resource: Resource; tick: MetricsTick }) {
  const openProcess = useUiStore((s) => s.openProcess);
  const navigate = useUiStore((s) => s.navigate);
  if (tick.processes.length === 0) return null;

  const sortMode: "cpu" | "memory" | "disk" =
    resource === "memory" ? "memory" : resource === "disk" ? "disk" : "cpu";

  const top = [...tick.processes]
    .sort((a, b) => {
      if (sortMode === "memory") return b.memoryBytes - a.memoryBytes;
      if (sortMode === "disk") {
        return b.diskReadPerSec + b.diskWritePerSec - (a.diskReadPerSec + a.diskWritePerSec);
      }
      return b.cpuPercent - a.cpuPercent;
    })
    .slice(0, 5);

  const metricLabel =
    sortMode === "memory" ? "Memory" : sortMode === "disk" ? "Disk I/O" : "CPU";

  return (
    <div className="mt-auto border-t border-border pt-4">
      <div className="mb-2 flex items-baseline justify-between">
        <span className="text-[12px] font-semibold uppercase tracking-wide text-fg-muted">
          Top Processes by {metricLabel}
        </span>
        <button
          type="button"
          onClick={() => navigate("processes")}
          className="text-[12px] font-medium text-accent hover:underline"
        >
          View all processes →
        </button>
      </div>
      <div className="grid gap-2 sm:grid-cols-5">
        {top.map((p) => {
          const primaryMetric =
            sortMode === "memory"
              ? formatBytes(p.memoryBytes)
              : sortMode === "disk"
                ? formatBytesPerSecond(p.diskReadPerSec + p.diskWritePerSec)
                : formatPercent(p.cpuPercent);
          const secondaryMetric =
            sortMode === "memory"
              ? `CPU ${formatPercent(p.cpuPercent)}`
              : `Mem ${formatBytes(p.memoryBytes)}`;
          return (
            <button
              key={p.pid}
              type="button"
              onClick={() => openProcess(p.pid)}
              className="flex flex-col justify-between rounded-md border border-border bg-surface-2/25 px-2.5 py-2 text-left transition-colors hover:border-border-strong hover:bg-surface-2"
            >
              <div className="flex items-baseline justify-between gap-1.5">
                <span className="truncate font-medium text-fg">{p.name}</span>
                <span className="num shrink-0 text-[11px] text-fg-subtle">#{p.pid}</span>
              </div>
              <div className="mt-1.5 flex items-baseline justify-between gap-1 text-[11px]">
                <span className="num font-semibold text-fg">{primaryMetric}</span>
                <span className="num truncate text-fg-muted">{secondaryMetric}</span>
              </div>
            </button>
          );
        })}
      </div>
    </div>
  );
}

function MemoryCompositionSection({ tick }: { tick: MetricsTick }) {
  const m = tick.memory;
  const total = Math.max(1, m.totalBytes);
  const usedBytes = Math.max(0, m.usedBytes);
  const cachedBytes = Math.max(0, m.cachedBytes ?? 0);
  const buffersBytes = Math.max(0, m.buffersBytes ?? 0);
  const remainingBytes = Math.max(0, total - usedBytes - cachedBytes - buffersBytes);

  const usedPct = (usedBytes / total) * 100;
  const cachedPct = (cachedBytes / total) * 100;
  const buffersPct = (buffersBytes / total) * 100;
  const freePct = (remainingBytes / total) * 100;

  const swapPct = m.swapTotalBytes > 0 ? (m.swapUsedBytes / m.swapTotalBytes) * 100 : 0;

  const segments = [
    {
      label: "In Use (Apps)",
      bytes: usedBytes,
      pct: usedPct,
      color: "var(--chart-1)",
    },
    {
      label: "Cached",
      bytes: cachedBytes,
      pct: cachedPct,
      color: "var(--chart-2)",
    },
    {
      label: "Buffers",
      bytes: buffersBytes,
      pct: buffersPct,
      color: "var(--chart-4)",
    },
    {
      label: "Free / Standby",
      bytes: remainingBytes,
      pct: freePct,
      color: "var(--border-strong)",
    },
  ];

  return (
    <div className="mt-4 space-y-3 border-t border-border pt-4">
      <div className="flex items-baseline justify-between">
        <span className="text-[12px] font-semibold uppercase tracking-wide text-fg-muted">
          Memory Composition
        </span>
        <span className="num text-[12px] text-fg-subtle">
          {formatBytes(m.usedBytes)} used of {formatBytes(m.totalBytes)}
        </span>
      </div>

      <div
        className="flex h-3 w-full overflow-hidden rounded-full bg-surface-2"
        role="img"
        aria-label="Memory composition breakdown"
      >
        {segments.map((seg) =>
          seg.pct > 0 ? (
            <div
              key={seg.label}
              style={{ width: `${seg.pct}%`, background: seg.color }}
              title={`${seg.label}: ${formatBytes(seg.bytes)} (${formatPercent(seg.pct)})`}
              className="h-full transition-[width] duration-300"
            />
          ) : null,
        )}
      </div>

      <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
        {segments.map((seg) => (
          <div
            key={seg.label}
            className="flex items-center justify-between rounded border border-border bg-surface-2/30 px-2.5 py-1.5 text-[12px]"
          >
            <span className="flex items-center gap-1.5 text-fg-muted">
              <span
                aria-hidden
                className="inline-block h-2.5 w-2.5 rounded-sm"
                style={{ background: seg.color }}
              />
              {seg.label}
            </span>
            <span className="num font-medium">{formatBytes(seg.bytes)}</span>
          </div>
        ))}
      </div>

      {m.swapTotalBytes > 0 && (
        <div className="pt-1">
          <div className="mb-1 flex items-baseline justify-between text-[12px]">
            <span className="text-fg-muted">Swap Usage</span>
            <span className="num text-fg-muted">
              {formatBytes(m.swapUsedBytes)} / {formatBytes(m.swapTotalBytes)} (
              {formatPercent(swapPct)})
            </span>
          </div>
          <Meter percent={swapPct} label="Swap usage" size="sm" />
        </div>
      )}
    </div>
  );
}

function MountedDrivesGrid({ tick }: { tick: MetricsTick }) {
  if (tick.disks.length === 0) return null;
  return (
    <div className="mt-4 border-t border-border pt-4">
      <div className="mb-2.5 flex items-baseline justify-between">
        <span className="text-[12px] font-semibold uppercase tracking-wide text-fg-muted">
          Mounted Drives ({tick.disks.length})
        </span>
      </div>
      <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
        {tick.disks.map((d) => (
          <div
            key={d.mountPoint}
            className="rounded-md border border-border bg-surface-2/30 p-3 space-y-2"
          >
            <div className="flex items-center justify-between gap-2">
              <div className="min-w-0">
                <span className="truncate font-semibold">{d.mountPoint}</span>
                <span className="ml-2 text-[12px] text-fg-subtle">{d.fileSystem}</span>
              </div>
              <span className="rounded bg-surface-2 px-1.5 py-0.5 text-[11px] font-medium uppercase text-fg-muted">
                {d.kind === "unknown" ? "Drive" : d.kind.toUpperCase()}
              </span>
            </div>
            <Meter percent={d.usedPercent} label={`${d.mountPoint} capacity used`} size="sm" />
            <div className="flex items-baseline justify-between text-[12px] text-fg-muted">
              <span className="num">
                {formatBytes(d.usedBytes)} / {formatBytes(d.totalBytes)}
              </span>
              <span className="num font-medium">{formatPercent(d.usedPercent)}</span>
            </div>
            <div className="flex items-baseline justify-between border-t border-border/60 pt-1.5 text-[11px] text-fg-subtle">
              <span className="num">
                R {formatBytesPerSecond(d.readBytesPerSec)} · W{" "}
                {formatBytesPerSecond(d.writeBytesPerSec)}
              </span>
              <span className="num">
                Active {d.activePercent != null ? formatPercent(d.activePercent, 0) : "—"}
              </span>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function ActiveInterfacesGrid({ tick }: { tick: MetricsTick }) {
  if (tick.networks.length === 0) return null;
  return (
    <div className="mt-4 border-t border-border pt-4">
      <div className="mb-2.5 flex items-baseline justify-between">
        <span className="text-[12px] font-semibold uppercase tracking-wide text-fg-muted">
          Active Interfaces ({tick.networks.length})
        </span>
      </div>
      <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
        {tick.networks.map((n) => (
          <div
            key={n.name}
            className="rounded-md border border-border bg-surface-2/30 p-3 space-y-2"
          >
            <div className="flex items-center justify-between gap-2">
              <span className="truncate font-semibold">{n.name}</span>
              {n.isPrimary && (
                <span className="rounded bg-accent-soft px-1.5 py-0.5 text-[11px] font-medium text-accent">
                  Primary
                </span>
              )}
            </div>
            <div className="flex items-baseline justify-between text-[12px]">
              <span className="num font-medium text-[var(--chart-1)]">
                ↓ {formatBytesPerSecond(n.rxBytesPerSec)}
              </span>
              <span className="num font-medium text-[var(--chart-2)]">
                ↑ {formatBytesPerSecond(n.txBytesPerSec)}
              </span>
            </div>
            <div className="space-y-1 border-t border-border/60 pt-1.5 text-[11px] text-fg-muted">
              <div className="flex justify-between gap-2">
                <span className="text-fg-subtle">Total RX / TX</span>
                <span className="num">
                  {formatBytes(n.totalRxBytes)} / {formatBytes(n.totalTxBytes)}
                </span>
              </div>
              <div className="flex justify-between gap-2">
                <span className="text-fg-subtle">IPv4</span>
                <span className="num selectable truncate">{n.ipv4.join(", ") || "—"}</span>
              </div>
              <div className="flex justify-between gap-2">
                <span className="text-fg-subtle">MAC</span>
                <span className="num selectable truncate">{n.macAddress ?? "—"}</span>
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function RangePicker({ value, onChange }: { value: number; onChange: (s: number) => void }) {
  return (
    <div
      role="radiogroup"
      aria-label="Time range"
      className="flex gap-0.5 rounded-md bg-surface-2 p-0.5"
    >
      {RANGES.map((r) => (
        <button
          key={r.seconds}
          type="button"
          role="radio"
          aria-checked={value === r.seconds}
          onClick={() => onChange(r.seconds)}
          className={cn(
            "rounded px-2.5 py-1 text-fg-muted",
            value === r.seconds && "bg-surface text-fg shadow-sm",
          )}
        >
          {r.label}
        </button>
      ))}
    </div>
  );
}

function Legend({ series }: { series: readonly ChartSeries[] }) {
  if (series.length < 2) return null;
  return (
    <div className="flex gap-4 text-[12px] text-fg-muted">
      {series.map((s) => (
        <span key={s.label} className="flex items-center gap-1.5">
          <span
            aria-hidden
            className="h-0.5 w-3 rounded"
            style={{ background: `var(${s.colorVar})` }}
          />
          {s.label}
        </span>
      ))}
    </div>
  );
}

function Stats({
  resource,
  tick,
  info,
}: {
  resource: Resource;
  tick: MetricsTick;
  info: StaticSystemInfo | null;
}): ReactNode {
  switch (resource) {
    case "cpu": {
      const la = tick.cpu.loadAverage;
      return (
        <>
          <KeyValueList
            items={[
              { label: "Utilization", value: formatPercent(tick.cpu.usagePercent) },
              { label: "Speed", value: formatFrequency(tick.cpu.frequencyMhz) },
              {
                label: "Load average",
                value: la ? (
                  `${la.one.toFixed(2)} / ${la.five.toFixed(2)} / ${la.fifteen.toFixed(2)}`
                ) : (
                  <Unavailable />
                ),
              },
            ]}
          />
          <KeyValueList
            items={[
              { label: "Physical cores", value: info?.physicalCoreCount ?? <Unavailable /> },
              { label: "Logical processors", value: info?.logicalCoreCount ?? "—" },
              { label: "Processes", value: formatCount(tick.engine.processCount) },
            ]}
          />
          <KeyValueList
            items={[
              { label: "Processor model", value: info?.cpuBrand || "—" },
              { label: "Architecture", value: info?.architecture ?? "—" },
              {
                label: "Uptime",
                value: formatDuration(uptimeSeconds(info?.bootTimeSecs ?? null, tick.timestampMs)),
              },
            ]}
          />
        </>
      );
    }
    case "memory": {
      const m = tick.memory;
      return (
        <>
          <KeyValueList
            items={[
              { label: "In use", value: formatBytes(m.usedBytes) },
              { label: "Available", value: formatBytes(m.availableBytes) },
              { label: "Total", value: formatBytes(m.totalBytes) },
            ]}
          />
          <KeyValueList
            items={[
              {
                label: "Cached",
                value: m.cachedBytes == null ? <Unavailable /> : formatBytes(m.cachedBytes),
              },
              {
                label: "Buffers",
                value: m.buffersBytes == null ? <Unavailable /> : formatBytes(m.buffersBytes),
              },
              { label: "Utilization", value: formatPercent(m.usedPercent) },
            ]}
          />
          <KeyValueList
            items={[
              {
                label: "Swap used",
                value:
                  m.swapTotalBytes > 0
                    ? `${formatBytes(m.swapUsedBytes)} / ${formatBytes(m.swapTotalBytes)}`
                    : "—",
              },
              {
                label: "Swap free",
                value:
                  m.swapTotalBytes > 0
                    ? formatBytes(Math.max(0, m.swapTotalBytes - m.swapUsedBytes))
                    : "—",
              },
              {
                label: "Committed",
                value:
                  m.commitUsedBytes != null && m.commitLimitBytes != null
                    ? `${formatBytes(m.commitUsedBytes)} / ${formatBytes(m.commitLimitBytes)}`
                    : "—",
              },
            ]}
          />
        </>
      );
    }
    case "disk": {
      const active = tick.disks.map((d) => d.activePercent).filter((a): a is number => a != null);
      const readIops = tick.disks.reduce((s, d) => s + (d.readOpsPerSec ?? 0), 0);
      const writeIops = tick.disks.reduce((s, d) => s + (d.writeOpsPerSec ?? 0), 0);
      const totalCap = tick.disks.reduce((s, d) => s + d.totalBytes, 0);
      const totalUsed = tick.disks.reduce((s, d) => s + d.usedBytes, 0);
      const totalAvail = tick.disks.reduce((s, d) => s + d.availableBytes, 0);
      return (
        <>
          <KeyValueList
            items={[
              { label: "Read rate", value: formatBytesPerSecond(tick.point.diskRead) },
              { label: "Write rate", value: formatBytesPerSecond(tick.point.diskWrite) },
              {
                label: "Read / Write IOPS",
                value: `${formatCount(readIops)} / ${formatCount(writeIops)}`,
              },
            ]}
          />
          <KeyValueList
            items={[
              { label: "Mounted drives", value: tick.disks.length },
              { label: "Total capacity", value: formatBytes(totalCap) },
              {
                label: "Total used / free",
                value: `${formatBytes(totalUsed)} / ${formatBytes(totalAvail)}`,
              },
            ]}
          />
          <KeyValueList
            items={[
              {
                label: "Busiest drive",
                value: active.length ? formatPercent(Math.max(...active)) : <Unavailable />,
                hint: "Highest active time across drives.",
              },
              {
                label: "Combined throughput",
                value: formatBytesPerSecond(tick.point.diskRead + tick.point.diskWrite),
              },
            ]}
          />
        </>
      );
    }
    case "network": {
      const primary = tick.networks.find((n) => n.isPrimary) ?? tick.networks[0];
      const totalRx = tick.networks.reduce((s, n) => s + n.totalRxBytes, 0);
      const totalTx = tick.networks.reduce((s, n) => s + n.totalTxBytes, 0);
      return (
        <>
          <KeyValueList
            items={[
              { label: "Download rate", value: formatBytesPerSecond(tick.point.netRx) },
              { label: "Upload rate", value: formatBytesPerSecond(tick.point.netTx) },
              {
                label: "Combined rate",
                value: formatBytesPerSecond(tick.point.netRx + tick.point.netTx),
              },
            ]}
          />
          <KeyValueList
            items={[
              { label: "Primary interface", value: primary?.name ?? "—" },
              { label: "IPv4", value: primary?.ipv4.join(", ") || "—" },
              { label: "MAC address", value: primary?.macAddress ?? "—" },
            ]}
          />
          <KeyValueList
            items={[
              { label: "Active interfaces", value: tick.networks.length },
              { label: "Total received", value: formatBytes(totalRx) },
              { label: "Total sent", value: formatBytes(totalTx) },
            ]}
          />
        </>
      );
    }
    case "gpu": {
      const gpu = tick.gpus[0];
      const vramPct =
        gpu && gpu.totalVramBytes > 0 ? (gpu.usedVramBytes / gpu.totalVramBytes) * 100 : null;
      return (
        <>
          <KeyValueList
            items={[
              {
                label: "Utilization",
                value: gpu ? (
                  formatPercent(gpu.usagePercent ?? tick.point.gpuUsage)
                ) : (
                  <Unavailable />
                ),
              },
              {
                label: "Dedicated VRAM",
                value: gpu ? (
                  `${formatBytes(gpu.usedVramBytes)} / ${formatBytes(gpu.totalVramBytes)}${
                    vramPct != null ? ` (${formatPercent(vramPct, 0)})` : ""
                  }`
                ) : (
                  <Unavailable />
                ),
              },
              {
                label: "Temperature",
                value:
                  gpu?.temperatureCelsius != null ? (
                    `${Math.round(gpu.temperatureCelsius)} °C${
                      gpu.fanSpeedPercent != null
                        ? ` · Fan ${formatPercent(gpu.fanSpeedPercent, 0)}`
                        : ""
                    }`
                  ) : (
                    <Unavailable />
                  ),
              },
              {
                label: "Power draw",
                value:
                  gpu?.powerWatts != null ? (
                    gpu.powerLimitWatts != null ? (
                      `${gpu.powerWatts.toFixed(1)} W / ${gpu.powerLimitWatts.toFixed(1)} W`
                    ) : (
                      `${gpu.powerWatts.toFixed(1)} W`
                    )
                  ) : (
                    <Unavailable />
                  ),
              },
            ]}
          />
          <KeyValueList
            items={[
              { label: "GPU", value: gpu?.name ?? <Unavailable /> },
              {
                label: "Graphics clock",
                value: gpu?.clockMhz != null ? formatFrequency(gpu.clockMhz) : <Unavailable />,
              },
              {
                label: "Memory clock",
                value:
                  gpu?.memoryClockMhz != null ? (
                    formatFrequency(gpu.memoryClockMhz)
                  ) : (
                    <Unavailable />
                  ),
              },
              {
                label: "Video engines",
                value:
                  gpu?.encoderUsagePercent != null || gpu?.decoderUsagePercent != null ? (
                    `Enc ${formatPercent(gpu.encoderUsagePercent ?? 0, 0)} · Dec ${formatPercent(gpu.decoderUsagePercent ?? 0, 0)}`
                  ) : (
                    <Unavailable />
                  ),
              },
            ]}
          />
          <KeyValueList
            items={[
              {
                label: "Performance state",
                value: gpu?.pstate ?? <Unavailable />,
              },
              {
                label: "PCIe link",
                value:
                  gpu?.pcieGen != null && gpu?.pcieWidth != null ? (
                    `PCIe Gen ${gpu.pcieGen} × ${gpu.pcieWidth}`
                  ) : (
                    <Unavailable />
                  ),
              },
              {
                label: "Driver version",
                value: gpu?.driverVersion ?? <Unavailable />,
              },
            ]}
          />
        </>
      );
    }
  }
}
