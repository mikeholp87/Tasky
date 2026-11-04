import {
  ArrowDown,
  ArrowUp,
  Cpu,
  Gpu,
  HardDrive,
  ListTree,
  MemoryStick,
  Network,
  Server,
} from "lucide-react";
import { ConnectionNotice } from "@/components/ConnectionNotice";
import { PageHeader } from "@/components/PageHeader";
import { TimeSeriesChart } from "@/components/charts/TimeSeriesChart";
import {
  CPU_SERIES,
  GPU_SERIES,
  MEMORY_SERIES,
  NETWORK_SERIES,
  percentAxis,
  rateAxis,
  RATE_MIN_Y,
} from "@/components/charts/series";
import { Card, CardHeader } from "@/components/ui/Card";
import { KeyValueList } from "@/components/ui/KeyValueList";
import { Meter } from "@/components/ui/Meter";
import { cn } from "@/lib/cn";
import { useMetricsStore } from "@/stores/metricsStore";
import { useUiStore } from "@/stores/uiStore";
import {
  formatBytes,
  formatBytesPerSecond,
  formatCount,
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
          <GpuCard />
          <StorageCard />
          <NetworkCard />
          <TopProcessesCard />
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

function GpuCard() {
  const gpu = useMetricsStore((s) => s.tick?.gpus[0]);
  const gpuUsagePoint = useMetricsStore((s) => s.tick?.point.gpuUsage);
  const navigate = useUiStore((s) => s.navigate);

  if (!gpu) {
    return (
      <Card>
        <CardHeader
          title="GPU"
          icon={<Gpu size={14} aria-hidden />}
          action={<DetailsLink onClick={() => navigate("gpu")} label="GPU details" />}
        />
        <div className="flex h-[212px] flex-col justify-between px-4 pb-4 pt-2">
          <div>
            <BigValue value="—" sub="No dedicated GPU telemetry" />
            <p className="mt-2 text-fg-muted">
              No supported discrete or integrated GPU telemetry provider was detected on this
              system.
            </p>
          </div>
          <div className="flex items-center justify-between border-t border-border pt-2.5 text-[12px] text-fg-subtle">
            <span>Dedicated VRAM</span>
            <span className="num">Not available</span>
          </div>
        </div>
      </Card>
    );
  }

  const usage = gpu.usagePercent ?? gpuUsagePoint ?? 0;
  const vramPct = gpu.totalVramBytes > 0 ? (gpu.usedVramBytes / gpu.totalVramBytes) * 100 : 0;
  const subParts = [
    gpu.totalVramBytes > 0
      ? `${formatBytes(gpu.usedVramBytes)} / ${formatBytes(gpu.totalVramBytes)}`
      : null,
    gpu.temperatureCelsius != null ? `${Math.round(gpu.temperatureCelsius)} °C` : null,
    gpu.powerWatts != null ? `${gpu.powerWatts.toFixed(1)} W` : null,
  ].filter(Boolean);
  const tempPowerParts = [
    gpu.temperatureCelsius != null ? `${Math.round(gpu.temperatureCelsius)} °C` : null,
    gpu.powerWatts != null ? `${gpu.powerWatts.toFixed(1)} W` : null,
  ].filter(Boolean);

  return (
    <Card>
      <CardHeader
        title={`GPU · ${gpu.name}`}
        icon={<Gpu size={14} aria-hidden />}
        action={<DetailsLink onClick={() => navigate("gpu")} label="GPU details" />}
      />
      <div className="space-y-3 px-4 pb-4 pt-2">
        <BigValue
          value={formatPercent(usage)}
          sub={subParts.join(" · ")}
          warning={levelLabel(usageLevel(usage), "GPU")}
        />
        <Meter percent={usage} label="GPU usage" />
        <TimeSeriesChart
          series={GPU_SERIES}
          seconds={CHART_SECONDS}
          height={CHART_HEIGHT}
          yMax={100}
          formatValue={percentAxis}
          sparkline
          ariaLabel={`GPU usage over the last minute, currently ${formatPercent(usage)}`}
        />
        <div className="flex flex-wrap items-center justify-between gap-4 text-fg-muted">
          <span>
            Dedicated VRAM{" "}
            <span className="num text-fg">
              {gpu.totalVramBytes > 0
                ? `${formatBytes(gpu.usedVramBytes)} / ${formatBytes(gpu.totalVramBytes)} (${formatPercent(vramPct, 0)})`
                : "—"}
            </span>
          </span>
          {tempPowerParts.length > 0 && <span className="num">{tempPowerParts.join(" · ")}</span>}
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
  const navigate = useUiStore((s) => s.navigate);
  if (!networks) return null;
  const primary = networks.find((n) => n.isPrimary) ?? networks[0];
  const rx = networks.reduce((sum, n) => sum + n.rxBytesPerSec, 0);
  const tx = networks.reduce((sum, n) => sum + n.txBytesPerSec, 0);
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

function TopProcessesCard() {
  const processes = useMetricsStore((s) => s.tick?.processes);
  const navigate = useUiStore((s) => s.navigate);
  if (!processes) return null;

  const top = [...processes]
    .sort((a, b) => b.cpuPercent - a.cpuPercent || b.memoryBytes - a.memoryBytes)
    .slice(0, 5);
  const runningCount = processes.filter((p) => p.status === "running").length;

  return (
    <Card className="flex flex-col justify-between">
      <div>
        <CardHeader
          title="Top Processes"
          icon={<ListTree size={14} aria-hidden />}
          action={<DetailsLink onClick={() => navigate("processes")} label="All processes" />}
        />
        <div className="px-4 pb-2 pt-2">
          {top.length === 0 ? (
            <p className="py-4 text-fg-muted">No active processes reported.</p>
          ) : (
            <table className="w-full text-left text-[12px]">
              <thead>
                <tr className="border-b border-border text-[11px] font-medium uppercase tracking-wide text-fg-subtle">
                  <th className="pb-1.5 font-medium">Process</th>
                  <th className="pb-1.5 text-right font-medium">PID</th>
                  <th className="pb-1.5 text-right font-medium">CPU</th>
                  <th className="pb-1.5 text-right font-medium">Memory</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border/60">
                {top.map((p) => {
                  const level = usageLevel(p.cpuPercent);
                  return (
                    <tr
                      key={p.pid}
                      onClick={() => useUiStore.getState().openProcess(p.pid)}
                      className="cursor-pointer transition-colors hover:bg-surface-2"
                      title={`Inspect ${p.name} (PID ${p.pid})`}
                    >
                      <td className="max-w-[160px] truncate py-1.5 pr-2 font-medium text-fg">
                        {p.name}
                      </td>
                      <td className="num py-1.5 px-2 text-right text-fg-subtle">{p.pid}</td>
                      <td
                        className={cn(
                          "num py-1.5 px-2 text-right font-semibold",
                          level === "critical" && "text-crit",
                          level === "warning" && "text-warn",
                        )}
                      >
                        {formatPercent(p.cpuPercent)}
                      </td>
                      <td className="num py-1.5 pl-2 text-right text-fg-muted">
                        {formatBytes(p.memoryBytes)}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          )}
        </div>
      </div>
      <div className="mx-4 mb-3 flex items-center justify-between border-t border-border pt-2 text-[12px] text-fg-muted">
        <span>
          Total processes: <span className="num font-medium text-fg">{formatCount(processes.length)}</span>
        </span>
        <span>
          Running: <span className="num font-medium text-fg">{formatCount(runningCount)}</span>
        </span>
      </div>
    </Card>
  );
}

function SystemCard() {
  const info = useMetricsStore((s) => s.staticInfo);
  const tickTime = useMetricsStore((s) => s.tick?.timestampMs);
  const engine = useMetricsStore((s) => s.tick?.engine);
  const gpu = useMetricsStore((s) => s.tick?.gpus[0]);
  const navigate = useUiStore((s) => s.navigate);
  if (!info || tickTime == null) return null;
  const now = tickTime;
  const bootMs = info.bootTimeSecs != null ? info.bootTimeSecs * 1000 : null;
  const gpuSpec = gpu
    ? `${gpu.name}${gpu.totalVramBytes > 0 ? ` (${formatBytes(gpu.totalVramBytes)})` : ""}`
    : "—";
  const coreSpec = `${info.physicalCoreCount ?? "?"} physical · ${info.logicalCoreCount} logical`;

  return (
    <Card className="xl:col-span-2">
      <CardHeader
        title="System"
        icon={<Server size={14} aria-hidden />}
        action={<DetailsLink onClick={() => navigate("system")} label="System information" />}
      />
      <div className="grid gap-x-10 gap-y-4 px-4 pb-4 pt-3 md:grid-cols-2 xl:grid-cols-3">
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
            { label: "Processor", value: info.cpuBrand || "—" },
            { label: "Cores / Threads", value: coreSpec },
            { label: "Installed RAM", value: formatBytes(info.totalMemoryBytes) },
            { label: "Graphics (GPU)", value: gpuSpec },
          ]}
        />
        <KeyValueList
          items={[
            { label: "Uptime", value: formatDuration(uptimeSeconds(info.bootTimeSecs, now)) },
            { label: "Boot time", value: formatTimestamp(bootMs) },
            { label: "GPU driver", value: gpu?.driverVersion ?? "—" },
            {
              label: "Active processes",
              value: engine ? formatCount(engine.processCount) : "—",
            },
            {
              label: "Telemetry poll",
              value: engine ? `${engine.intervalMs} ms (${engine.pollDurationMs} ms)` : "—",
            },
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
