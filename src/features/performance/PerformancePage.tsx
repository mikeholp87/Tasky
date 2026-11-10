import { usePersistedState } from "@/hooks/usePersistedState";
import type { ReactNode } from "react";
import { ConnectionNotice } from "@/components/ConnectionNotice";
import { PageHeader } from "@/components/PageHeader";
import { type ChartSeries, TimeSeriesChart } from "@/components/charts/TimeSeriesChart";
import {
  CPU_SERIES,
  DISK_SERIES,
  MEMORY_SERIES,
  NETWORK_SERIES,
  percentAxis,
  rateAxis,
  RATE_MIN_Y,
} from "@/components/charts/series";
import { Card } from "@/components/ui/Card";
import { KeyValueList, Unavailable } from "@/components/ui/KeyValueList";
import { SegmentedControl } from "@/components/ui/SegmentedControl";
import { cn } from "@/lib/cn";
import { useMetricsStore } from "@/stores/metricsStore";
import type { MetricsTick, StaticSystemInfo } from "@/types";
import {
  formatBytes,
  formatBytesPerSecond,
  formatCount,
  formatDuration,
  formatFrequency,
  formatPercent,
} from "@/utils/format";
import { uptimeSeconds } from "@/utils/system";

type Resource = "cpu" | "memory" | "disk" | "network";

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
];

export function PerformancePage() {
  const tick = useMetricsStore((s) => s.tick);
  const info = useMetricsStore((s) => s.staticInfo);
  const [selected, setSelected] = usePersistedState<Resource>(
    "perf.resource",
    "cpu",
    RESOURCES.map((r) => r.id),
  );
  const [seconds, setSeconds] = usePersistedState<number>(
    "perf.seconds",
    60,
    RANGES.map((r) => r.seconds),
  );
  const def = RESOURCES.find((r) => r.id === selected) as ResourceDef;

  return (
    <>
      <PageHeader
        title="Performance"
        description="Live resource history, kept in memory for up to one hour."
        actions={<RangePicker value={seconds} onChange={setSeconds} />}
      />
      {!tick ? (
        <ConnectionNotice />
      ) : (
        <div className="grid gap-4 lg:grid-cols-[260px_1fr]">
          <nav aria-label="Resources" className="space-y-2">
            {RESOURCES.map((r) => (
              <button
                key={r.id}
                type="button"
                aria-pressed={selected === r.id}
                onClick={() => setSelected(r.id)}
                className={cn(
                  "block w-full rounded-lg border p-3 text-left",
                  selected === r.id
                    ? "border-accent bg-accent-soft"
                    : "border-border bg-surface hover:bg-surface-2",
                )}
              >
                <div className="flex items-baseline justify-between gap-2">
                  <span className="font-medium">{r.label}</span>
                  <span className="num truncate text-[0.8125rem] text-fg-muted">
                    {r.summary(tick)}
                  </span>
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

          <Card className="min-w-0 p-4">
            <div className="mb-3 flex items-baseline justify-between gap-3">
              <h2 className="text-base font-semibold">{def.label}</h2>
              <Legend series={def.series} />
            </div>
            <TimeSeriesChart
              series={def.series}
              seconds={seconds}
              height={320}
              yMax={def.yMax}
              minYMax={def.minYMax}
              formatValue={def.format}
              ariaLabel={`${def.label} over the last ${RANGES.find((r) => r.seconds === seconds)?.label}`}
            />
            <div className="mt-4 grid gap-x-10 border-t border-border pt-4 md:grid-cols-2">
              <Stats resource={selected} tick={tick} info={info} />
            </div>
          </Card>
        </div>
      )}
    </>
  );
}

function RangePicker({ value, onChange }: { value: number; onChange: (s: number) => void }) {
  return (
    <SegmentedControl
      label="Time range"
      value={String(value)}
      onChange={(v) => onChange(Number(v))}
      segments={RANGES.map((r) => ({ value: String(r.seconds), label: r.label }))}
    />
  );
}

function Legend({ series }: { series: readonly ChartSeries[] }) {
  if (series.length < 2) return null;
  return (
    <div className="flex gap-4 text-[0.8125rem] text-fg-muted">
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
    case "cpu":
      return (
        <>
          <KeyValueList
            items={[
              { label: "Utilization", value: formatPercent(tick.cpu.usagePercent) },
              { label: "Speed", value: formatFrequency(tick.cpu.frequencyMhz) },
              { label: "Processes", value: formatCount(tick.engine.processCount) },
            ]}
          />
          <KeyValueList
            items={[
              { label: "Cores", value: info?.physicalCoreCount ?? <Unavailable /> },
              { label: "Logical processors", value: info?.logicalCoreCount ?? "—" },
              {
                label: "Uptime",
                value: formatDuration(uptimeSeconds(info?.bootTimeSecs ?? null, tick.timestampMs)),
              },
            ]}
          />
        </>
      );
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
                label: "Swap used",
                value:
                  m.swapTotalBytes > 0
                    ? `${formatBytes(m.swapUsedBytes)} / ${formatBytes(m.swapTotalBytes)}`
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
      return (
        <>
          <KeyValueList
            items={[
              { label: "Read", value: formatBytesPerSecond(tick.point.diskRead) },
              { label: "Write", value: formatBytesPerSecond(tick.point.diskWrite) },
            ]}
          />
          <KeyValueList
            items={[
              { label: "Drives", value: tick.disks.length },
              {
                label: "Busiest drive",
                value: active.length ? formatPercent(Math.max(...active)) : <Unavailable />,
                hint: "Highest active time across drives.",
              },
            ]}
          />
        </>
      );
    }
    case "network": {
      const primary = tick.networks.find((n) => n.isPrimary);
      return (
        <>
          <KeyValueList
            items={[
              { label: "Download", value: formatBytesPerSecond(tick.point.netRx) },
              { label: "Upload", value: formatBytesPerSecond(tick.point.netTx) },
            ]}
          />
          <KeyValueList
            items={[
              { label: "Interface", value: primary?.name ?? "—" },
              { label: "IPv4", value: primary?.ipv4.join(", ") || "—" },
            ]}
          />
        </>
      );
    }
  }
}
