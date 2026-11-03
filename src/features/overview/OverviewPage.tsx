import { ArrowDown, ArrowUp, Cpu, HardDrive, MemoryStick, Network, Server } from "lucide-react";
import { ConnectionNotice } from "@/components/ConnectionNotice";
import { PageHeader } from "@/components/PageHeader";
import { TimeSeriesChart } from "@/components/charts/TimeSeriesChart";
import {
  CPU_SERIES,
  MEMORY_SERIES,
  NETWORK_SERIES,
  percentAxis,
  rateAxis,
  RATE_MIN_Y,
} from "@/components/charts/series";
import { Card, CardHeader } from "@/components/ui/Card";
import { KeyValueList } from "@/components/ui/KeyValueList";
import { Meter } from "@/components/ui/Meter";
import { useMetricsStore } from "@/stores/metricsStore";
import { useUiStore } from "@/stores/uiStore";
import {
  formatBytes,
  formatBytesPerSecond,
  formatDuration,
  formatFrequency,
  formatPercent,
  formatTimestamp,
} from "@/utils/format";
import { levelLabel, usageLevel } from "@/utils/levels";
import { uptimeSeconds } from "@/utils/system";

const CHART_SECONDS = 60;
const CHART_HEIGHT = 96;

export function OverviewPage() {
  const ready = useMetricsStore((s) => s.tick !== null);
  return (
    <>
      <PageHeader title="Overview" description="What your computer is doing right now." />
      {ready ? (
        <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
          <CpuCard />
          <MemoryCard />
          <StorageCard />
          <NetworkCard />
          <SystemCard />
        </div>
      ) : (
        <ConnectionNotice />
      )}
    </>
  );
}

function BigValue({
  value,
  sub,
  warning,
}: {
  value: string;
  sub?: string;
  warning?: string | null;
}) {
  return (
    <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
      <span className="num text-3xl font-semibold tracking-tight">{value}</span>
      {sub && <span className="num text-fg-muted">{sub}</span>}
      {warning && (
        <span className="rounded bg-warn-soft px-1.5 py-0.5 text-[11px] font-medium text-warn">
          {warning}
        </span>
      )}
    </div>
  );
}

function CpuCard() {
  const cpu = useMetricsStore((s) => s.tick?.cpu);
  const info = useMetricsStore((s) => s.staticInfo);
  const navigate = useUiStore((s) => s.navigate);
  if (!cpu) return null;
  const cores = info
    ? `${info.physicalCoreCount ?? "?"} cores · ${info.logicalCoreCount} threads`
    : undefined;
  return (
    <Card>
      <CardHeader
        title="CPU"
        icon={<Cpu size={14} aria-hidden />}
        action={<DetailsLink onClick={() => navigate("cpu")} label="CPU details" />}
      />
      <div className="space-y-3 px-4 pb-4 pt-2">
        <BigValue
          value={formatPercent(cpu.usagePercent)}
          sub={[cores, formatFrequency(cpu.frequencyMhz)].filter(Boolean).join(" · ")}
          warning={levelLabel(usageLevel(cpu.usagePercent), "CPU")}
        />
        <Meter percent={cpu.usagePercent} label="CPU usage" />
        <TimeSeriesChart
          series={CPU_SERIES}
          seconds={CHART_SECONDS}
          height={CHART_HEIGHT}
          yMax={100}
          formatValue={percentAxis}
          sparkline
          ariaLabel={`CPU usage over the last minute, currently ${formatPercent(cpu.usagePercent)}`}
        />
        {cpu.loadAverage && (
          <p className="num text-fg-muted">
            Load average {cpu.loadAverage.one.toFixed(2)} · {cpu.loadAverage.five.toFixed(2)} ·{" "}
            {cpu.loadAverage.fifteen.toFixed(2)}
          </p>
        )}
      </div>
    </Card>
  );
}

function MemoryCard() {
  const mem = useMetricsStore((s) => s.tick?.memory);
  const navigate = useUiStore((s) => s.navigate);
  if (!mem) return null;
  const swapPct = mem.swapTotalBytes > 0 ? (mem.swapUsedBytes / mem.swapTotalBytes) * 100 : 0;
  return (
    <Card>
      <CardHeader
        title="Memory"
        icon={<MemoryStick size={14} aria-hidden />}
        action={<DetailsLink onClick={() => navigate("memory")} label="Memory details" />}
      />
      <div className="space-y-3 px-4 pb-4 pt-2">
        <BigValue
          value={`${formatBytes(mem.usedBytes)} / ${formatBytes(mem.totalBytes)}`}
          sub={formatPercent(mem.usedPercent)}
          warning={levelLabel(usageLevel(mem.usedPercent), "memory")}
        />
        <Meter percent={mem.usedPercent} label="Memory usage" />
        <TimeSeriesChart
          series={MEMORY_SERIES}
          seconds={CHART_SECONDS}
          height={CHART_HEIGHT}
          yMax={100}
          formatValue={percentAxis}
          sparkline
          ariaLabel={`Memory usage over the last minute, currently ${formatPercent(mem.usedPercent)}`}
        />
        <div className="flex items-center justify-between gap-4 text-fg-muted">
          <span>Swap</span>
          <span className="num">
            {mem.swapTotalBytes > 0
              ? `${formatBytes(mem.swapUsedBytes)} / ${formatBytes(mem.swapTotalBytes)} (${formatPercent(swapPct)})`
              : "Not configured"}
          </span>
        </div>
      </div>
    </Card>
  );
}

function StorageCard() {
  const disks = useMetricsStore((s) => s.tick?.disks);
  const navigate = useUiStore((s) => s.navigate);
  if (!disks) return null;
  return (
    <Card>
      <CardHeader
        title="Storage"
        icon={<HardDrive size={14} aria-hidden />}
        action={<DetailsLink onClick={() => navigate("storage")} label="Storage details" />}
      />
      <ul className="space-y-3 px-4 pb-4 pt-3">
        {disks.length === 0 && <li className="text-fg-muted">No drives reported.</li>}
        {disks.map((d) => (
          <li key={d.mountPoint}>
            <div className="mb-1 flex items-baseline justify-between gap-3">
              <span className="truncate font-medium">
                {d.mountPoint}
                <span className="ml-2 font-normal text-fg-subtle">{d.fileSystem}</span>
              </span>
              <span className="num shrink-0 text-fg-muted">
                {formatBytes(d.availableBytes)} free of {formatBytes(d.totalBytes)}
              </span>
            </div>
            <Meter percent={d.usedPercent} label={`${d.mountPoint} used`} size="sm" />
            {d.readBytesPerSec != null && (
              <div className="num mt-1 text-[12px] text-fg-subtle">
                Read {formatBytesPerSecond(d.readBytesPerSec)} · Write{" "}
                {formatBytesPerSecond(d.writeBytesPerSec)}
              </div>
            )}
          </li>
        ))}
      </ul>
    </Card>
  );
}

function NetworkCard() {
  const networks = useMetricsStore((s) => s.tick?.networks);
  const point = useMetricsStore((s) => s.tick?.point);
  const navigate = useUiStore((s) => s.navigate);
  if (!networks || !point) return null;
  const primary = networks.find((n) => n.isPrimary) ?? networks[0];
  // Engine totals skip VPN and virtual interfaces, which re-count bytes.
  const rx = point.netRx;
  const tx = point.netTx;
  return (
    <Card>
      <CardHeader
        title="Network"
        icon={<Network size={14} aria-hidden />}
        action={<DetailsLink onClick={() => navigate("network")} label="Network details" />}
      />
      <div className="space-y-3 px-4 pb-4 pt-2">
        <div className="flex flex-wrap items-baseline gap-x-6 gap-y-1">
          <span className="num flex items-center gap-1 text-2xl font-semibold">
            <ArrowDown size={18} className="text-[var(--chart-1)]" aria-label="Download" />
            {formatBytesPerSecond(rx)}
          </span>
          <span className="num flex items-center gap-1 text-2xl font-semibold">
            <ArrowUp size={18} className="text-[var(--chart-2)]" aria-label="Upload" />
            {formatBytesPerSecond(tx)}
          </span>
        </div>
        <TimeSeriesChart
          series={NETWORK_SERIES}
          seconds={CHART_SECONDS}
          height={CHART_HEIGHT}
          formatValue={rateAxis}
          minYMax={RATE_MIN_Y}
          sparkline
          ariaLabel="Network throughput over the last minute"
        />
        {primary ? (
          <KeyValueList
            items={[
              { label: "Interface", value: primary.name },
              { label: "IPv4", value: primary.ipv4.join(", ") || "—" },
              {
                label: "Total",
                value: `↓ ${formatBytes(primary.totalRxBytes)} · ↑ ${formatBytes(primary.totalTxBytes)}`,
              },
            ]}
          />
        ) : (
          <p className="text-fg-muted">No active interfaces.</p>
        )}
      </div>
    </Card>
  );
}

function SystemCard() {
  const info = useMetricsStore((s) => s.staticInfo);
  const tickTime = useMetricsStore((s) => s.tick?.timestampMs);
  const navigate = useUiStore((s) => s.navigate);
  if (!info || tickTime == null) return null;
  const now = tickTime;
  const bootMs = info.bootTimeSecs != null ? info.bootTimeSecs * 1000 : null;
  return (
    <Card className="xl:col-span-2">
      <CardHeader
        title="System"
        icon={<Server size={14} aria-hidden />}
        action={<DetailsLink onClick={() => navigate("system")} label="System information" />}
      />
      <div className="grid gap-x-10 gap-y-1.5 px-4 pb-4 pt-3 md:grid-cols-2">
        <KeyValueList
          items={[
            { label: "Operating system", value: info.longOsVersion ?? info.osName ?? "—" },
            { label: "Kernel", value: info.kernelVersion ?? "—" },
            { label: "Hostname", value: info.hostname ?? "—" },
            { label: "Architecture", value: info.architecture },
          ]}
        />
        <KeyValueList
          items={[
            { label: "Uptime", value: formatDuration(uptimeSeconds(info.bootTimeSecs, now)) },
            { label: "Boot time", value: formatTimestamp(bootMs) },
            { label: "Processor", value: info.cpuBrand || "—" },
            { label: "Installed RAM", value: formatBytes(info.totalMemoryBytes) },
          ]}
        />
      </div>
    </Card>
  );
}

function DetailsLink({ onClick, label }: { onClick: () => void; label: string }) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={label}
      className="rounded px-1.5 py-0.5 text-[12px] text-fg-muted hover:bg-surface-2 hover:text-fg"
    >
      Details →
    </button>
  );
}
