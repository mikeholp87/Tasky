import { memo } from "react";
import { ConnectionNotice } from "@/components/ConnectionNotice";
import { PageHeader } from "@/components/PageHeader";
import { TimeSeriesChart } from "@/components/charts/TimeSeriesChart";
import { CPU_SERIES, coreSeries, percentAxis } from "@/components/charts/series";
import { Card, CardHeader } from "@/components/ui/Card";
import { KeyValueList, Unavailable } from "@/components/ui/KeyValueList";
import { Meter } from "@/components/ui/Meter";
import { cn } from "@/lib/cn";
import { useMetricsStore } from "@/stores/metricsStore";
import { useUiStore } from "@/stores/uiStore";
import {
  formatBytes,
  formatCount,
  formatDuration,
  formatFrequency,
  formatPercent,
} from "@/utils/format";
import { usageLevel } from "@/utils/levels";
import { uptimeSeconds } from "@/utils/system";

export function CpuPage() {
  const tick = useMetricsStore((s) => s.tick);
  const info = useMetricsStore((s) => s.staticInfo);
  const openProcess = useUiStore((s) => s.openProcess);
  const cpu = tick?.cpu;
  const platform = info?.platform;

  return (
    <>
      <PageHeader title="CPU" description={info?.cpuBrand || "Total and per-core utilization."} />
      {!tick || !cpu ? (
        <ConnectionNotice />
      ) : (
        <div className="space-y-4">
          <div className="grid gap-4 lg:grid-cols-[1fr_320px]">
            <Card>
              <CardHeader title={`Utilization · ${formatPercent(cpu.usagePercent)}`} />
              <div className="px-2 pb-3 pt-2">
                <TimeSeriesChart
                  series={CPU_SERIES}
                  seconds={60}
                  height={220}
                  yMax={100}
                  formatValue={percentAxis}
                  ariaLabel={`Total CPU usage over the last minute, currently ${formatPercent(cpu.usagePercent)}`}
                />
              </div>
            </Card>
            <Card>
              <CardHeader title="Processor" />
              <div className="px-4 pb-4 pt-3">
                <KeyValueList
                  items={[
                    { label: "Utilization", value: formatPercent(cpu.usagePercent) },
                    {
                      label: "Frequency",
                      value: cpu.frequencyMhz ? formatFrequency(cpu.frequencyMhz) : <Unavailable />,
                      hint:
                        platform === "windows"
                          ? "Windows reports the nominal processor frequency."
                          : "Average current frequency across logical processors.",
                    },
                    {
                      label: "Physical cores",
                      value: info?.physicalCoreCount ?? <Unavailable />,
                    },
                    { label: "Logical processors", value: info?.logicalCoreCount ?? "—" },
                    { label: "Vendor", value: info?.cpuVendor || "—" },
                    { label: "Architecture", value: info?.architecture ?? "—" },
                    {
                      label: "Load average",
                      value: cpu.loadAverage ? (
                        `${cpu.loadAverage.one.toFixed(2)} / ${cpu.loadAverage.five.toFixed(2)} / ${cpu.loadAverage.fifteen.toFixed(2)}`
                      ) : (
                        <Unavailable />
                      ),
                      hint: "1, 5 and 15 minute averages. Windows has no load average.",
                    },
                  ]}
                />
              </div>
            </Card>
          </div>

          <Card>
            <CardHeader title={`Logical processors (${cpu.perCore.length})`} />
            <div className="grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-3 p-4">
              {cpu.perCore.map((usage, i) => (
                <CoreTile key={i} index={i} usage={usage} />
              ))}
            </div>
          </Card>

          <div className="grid gap-4 xl:grid-cols-2">
            <SchedulerDistributionCard />
            <TopCpuProcessesCard onSelectPid={openProcess} />
          </div>
        </div>
      )}
    </>
  );
}

function SchedulerDistributionCard() {
  const tick = useMetricsStore((s) => s.tick);
  const info = useMetricsStore((s) => s.staticInfo);
  if (!tick) return null;

  const { cpu, processes, engine, timestampMs } = tick;
  const logicalCores = Math.max(1, info?.logicalCoreCount ?? cpu.perCore.length ?? 1);
  const peakCore = cpu.perCore.length > 0 ? Math.max(...cpu.perCore) : 0;
  const meanCore =
    cpu.perCore.length > 0
      ? cpu.perCore.reduce((sum, v) => sum + v, 0) / cpu.perCore.length
      : cpu.usagePercent;
  const activeCores = cpu.perCore.filter((v) => v > 25).length;

  const runningCount = processes.filter((p) => p.status === "running").length;
  const sleepingCount = processes.filter(
    (p) => p.status === "sleeping" || p.status === "idle",
  ).length;
  const totalThreads = processes.reduce((sum, p) => sum + (p.threads ?? 0), 0);

  const la = cpu.loadAverage;
  const loadBars = la
    ? [
        { label: "1 min average", val: la.one, pct: Math.min(100, (la.one / logicalCores) * 100) },
        {
          label: "5 min average",
          val: la.five,
          pct: Math.min(100, (la.five / logicalCores) * 100),
        },
        {
          label: "15 min average",
          val: la.fifteen,
          pct: Math.min(100, (la.fifteen / logicalCores) * 100),
        },
      ]
    : null;

  return (
    <Card>
      <CardHeader title="Scheduler & Core Distribution" />
      <div className="space-y-4 px-4 pb-4 pt-3">
        {loadBars ? (
          <div className="space-y-2.5">
            {loadBars.map((b) => (
              <div key={b.label}>
                <div className="mb-1 flex items-baseline justify-between text-[12px]">
                  <span className="text-fg-muted">{b.label}</span>
                  <span className="num font-medium">
                    {b.val.toFixed(2)}{" "}
                    <span className="text-fg-subtle">({formatPercent(b.pct, 0)} of capacity)</span>
                  </span>
                </div>
                <Meter percent={b.pct} label={b.label} size="sm" />
              </div>
            ))}
          </div>
        ) : (
          <p className="text-[12px] text-fg-subtle">
            Load average metrics are not reported on this platform.
          </p>
        )}

        <div className="grid gap-x-8 gap-y-2 border-t border-border pt-3 sm:grid-cols-2">
          <KeyValueList
            items={[
              { label: "Peak core load", value: formatPercent(peakCore) },
              { label: "Mean core load", value: formatPercent(meanCore) },
              {
                label: "Active cores (>25%)",
                value: `${activeCores} / ${cpu.perCore.length}`,
              },
              {
                label: "Total threads",
                value: totalThreads > 0 ? formatCount(totalThreads) : "—",
              },
            ]}
          />
          <KeyValueList
            items={[
              { label: "Total processes", value: formatCount(engine.processCount) },
              {
                label: "Running / Sleeping",
                value: `${formatCount(runningCount)} / ${formatCount(sleepingCount)}`,
              },
              {
                label: "System uptime",
                value: formatDuration(uptimeSeconds(info?.bootTimeSecs ?? null, timestampMs)),
              },
              {
                label: "Sample interval",
                value: `${engine.intervalMs} ms`,
              },
            ]}
          />
        </div>
      </div>
    </Card>
  );
}

function TopCpuProcessesCard({ onSelectPid }: { onSelectPid: (pid: number) => void }) {
  const processes = useMetricsStore((s) => s.tick?.processes);
  if (!processes) return null;

  const top = [...processes]
    .sort((a, b) => b.cpuPercent - a.cpuPercent || b.memoryBytes - a.memoryBytes)
    .slice(0, 8);

  return (
    <Card>
      <CardHeader title="Top CPU Processes" />
      <div className="px-4 pb-4 pt-2">
        {top.length === 0 ? (
          <p className="py-4 text-fg-muted">No processes available.</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-left text-[12px]">
              <thead>
                <tr className="border-b border-border text-[11px] font-medium uppercase tracking-wide text-fg-subtle">
                  <th className="pb-1.5 font-medium">Process</th>
                  <th className="pb-1.5 px-2 text-right font-medium">PID</th>
                  <th className="pb-1.5 px-2 font-medium">User</th>
                  <th className="pb-1.5 px-2 text-right font-medium">Threads</th>
                  <th className="pb-1.5 px-2 text-right font-medium">CPU %</th>
                  <th className="pb-1.5 pl-2 text-right font-medium">Memory</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border/60">
                {top.map((p) => {
                  const level = usageLevel(p.cpuPercent);
                  return (
                    <tr
                      key={p.pid}
                      onClick={() => onSelectPid(p.pid)}
                      className="cursor-pointer transition-colors hover:bg-surface-2"
                      title={`Inspect ${p.name} (PID ${p.pid})`}
                    >
                      <td className="max-w-[160px] truncate py-1.5 pr-2 font-medium text-fg">
                        {p.name}
                      </td>
                      <td className="num py-1.5 px-2 text-right text-fg-subtle">{p.pid}</td>
                      <td className="max-w-[100px] truncate py-1.5 px-2 text-fg-muted">
                        {p.user ?? "—"}
                      </td>
                      <td className="num py-1.5 px-2 text-right text-fg-muted">
                        {p.threads != null ? formatCount(p.threads) : "—"}
                      </td>
                      <td className="py-1.5 px-2 text-right">
                        <div className="flex items-center justify-end gap-2">
                          <div className="hidden w-14 sm:block">
                            <Meter
                              percent={p.cpuPercent}
                              label={`${p.name} CPU usage`}
                              size="sm"
                            />
                          </div>
                          <span
                            className={cn(
                              "num w-12 text-right font-semibold",
                              level === "critical" && "text-crit",
                              level === "warning" && "text-warn",
                            )}
                          >
                            {formatPercent(p.cpuPercent)}
                          </span>
                        </div>
                      </td>
                      <td className="num py-1.5 pl-2 text-right text-fg-muted">
                        {formatBytes(p.memoryBytes)}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </Card>
  );
}

const CoreTile = memo(function CoreTile({ index, usage }: { index: number; usage: number }) {
  const level = usageLevel(usage);
  return (
    <div className="rounded-md border border-border p-2">
      <div className="mb-1 flex items-baseline justify-between">
        <span className="text-fg-muted">CPU {index}</span>
        <span
          className={cn(
            "num font-semibold",
            level === "critical" && "text-crit",
            level === "warning" && "text-warn",
          )}
        >
          {formatPercent(usage, 0)}
        </span>
      </div>
      <TimeSeriesChart
        series={coreSeries(index)}
        seconds={60}
        height={40}
        yMax={100}
        formatValue={percentAxis}
        sparkline
        ariaLabel={`CPU ${index} usage, currently ${formatPercent(usage, 0)}`}
      />
    </div>
  );
});
