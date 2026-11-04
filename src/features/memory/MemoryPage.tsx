import { ConnectionNotice } from "@/components/ConnectionNotice";
import { PageHeader } from "@/components/PageHeader";
import { TimeSeriesChart } from "@/components/charts/TimeSeriesChart";
import { MEMORY_SERIES, percentAxis } from "@/components/charts/series";
import { Card, CardHeader } from "@/components/ui/Card";
import { KeyValueList, Unavailable } from "@/components/ui/KeyValueList";
import { Meter } from "@/components/ui/Meter";
import { cn } from "@/lib/cn";
import { useMetricsStore } from "@/stores/metricsStore";
import { useUiStore } from "@/stores/uiStore";
import { formatBytes, formatCount, formatPercent } from "@/utils/format";
import { memoryHeatLevel } from "@/utils/levels";

export function MemoryPage() {
  const tick = useMetricsStore((s) => s.tick);
  const platform = useMetricsStore((s) => s.staticInfo?.platform);
  const openProcess = useUiStore((s) => s.openProcess);
  const mem = tick?.memory;

  if (!tick || !mem) {
    return (
      <>
        <PageHeader title="Memory" />
        <ConnectionNotice />
      </>
    );
  }
  const swapPct = mem.swapTotalBytes > 0 ? (mem.swapUsedBytes / mem.swapTotalBytes) * 100 : 0;
  const optional = (v: number | null) => (v == null ? <Unavailable /> : formatBytes(v));

  const total = Math.max(1, mem.totalBytes);
  const usedBytes = Math.max(0, mem.usedBytes);
  const cachedBytes = Math.max(0, mem.cachedBytes ?? 0);
  const buffersBytes = Math.max(0, mem.buffersBytes ?? 0);
  const freeBytes = Math.max(0, total - usedBytes - cachedBytes - buffersBytes);

  const segments = [
    {
      label: "Used / App Memory",
      bytes: usedBytes,
      pct: (usedBytes / total) * 100,
      color: "var(--chart-1)",
    },
    {
      label: "Page Cache",
      bytes: cachedBytes,
      pct: (cachedBytes / total) * 100,
      color: "var(--chart-2)",
    },
    {
      label: "Kernel Buffers",
      bytes: buffersBytes,
      pct: (buffersBytes / total) * 100,
      color: "var(--chart-4)",
    },
    {
      label: "Available / Free",
      bytes: freeBytes,
      pct: (freeBytes / total) * 100,
      color: "var(--border-strong)",
    },
  ];

  const topProcesses = [...tick.processes]
    .sort((a, b) => b.memoryBytes - a.memoryBytes || b.cpuPercent - a.cpuPercent)
    .slice(0, 8);
  const totalProcessRss = tick.processes.reduce((sum, p) => sum + p.memoryBytes, 0);
  const totalProcessVirtual = tick.processes.reduce((sum, p) => sum + p.virtualMemoryBytes, 0);

  return (
    <>
      <PageHeader
        title="Memory"
        description={`${formatBytes(mem.usedBytes)} of ${formatBytes(mem.totalBytes)} in use`}
      />
      <div className="space-y-4">
        <div className="grid gap-4 lg:grid-cols-[1fr_340px]">
          <Card className="flex flex-col justify-between">
            <div>
              <CardHeader title={`Usage · ${formatPercent(mem.usedPercent)}`} />
              <div className="px-2 pb-3 pt-2">
                <TimeSeriesChart
                  series={MEMORY_SERIES}
                  seconds={60}
                  height={220}
                  yMax={100}
                  formatValue={percentAxis}
                  ariaLabel={`Memory usage over the last minute, currently ${formatPercent(mem.usedPercent)}`}
                />
              </div>
            </div>

            <div className="border-t border-border px-4 pb-4 pt-3 space-y-3">
              <div className="flex items-baseline justify-between">
                <span className="text-[12px] font-semibold uppercase tracking-wide text-fg-muted">
                  Memory Allocation Breakdown
                </span>
                <span className="num text-[12px] text-fg-subtle">
                  {formatBytes(mem.availableBytes)} available for applications
                </span>
              </div>
              <div
                className="flex h-3 w-full overflow-hidden rounded-full bg-surface-2"
                role="img"
                aria-label="Memory allocation breakdown"
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
                    className="rounded border border-border bg-surface-2/30 px-2.5 py-1.5 text-[12px]"
                  >
                    <div className="flex items-center gap-1.5 text-fg-muted">
                      <span
                        aria-hidden
                        className="inline-block h-2.5 w-2.5 shrink-0 rounded-sm"
                        style={{ background: seg.color }}
                      />
                      <span className="truncate">{seg.label}</span>
                    </div>
                    <div className="mt-0.5 flex items-baseline justify-between gap-1">
                      <span className="num font-semibold">{formatBytes(seg.bytes)}</span>
                      <span className="num text-[11px] text-fg-subtle">
                        {formatPercent(seg.pct, 0)}
                      </span>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </Card>

          <div className="space-y-4">
            <Card>
              <CardHeader title="Physical memory" />
              <div className="space-y-3 px-4 pb-4 pt-3">
                <Meter percent={mem.usedPercent} label="Physical memory used" />
                <KeyValueList
                  items={[
                    { label: "Total", value: formatBytes(mem.totalBytes) },
                    { label: "Used", value: formatBytes(mem.usedBytes) },
                    {
                      label: "Available",
                      value: formatBytes(mem.availableBytes),
                      hint: "Memory that can be given to programs without swapping, including reclaimable cache.",
                    },
                    {
                      label: "Cached",
                      value: optional(mem.cachedBytes),
                      hint:
                        platform === "windows"
                          ? "Windows system file cache."
                          : "Page cache plus reclaimable kernel memory, as reported by `free`.",
                    },
                    ...(platform === "windows"
                      ? []
                      : [{ label: "Buffers", value: optional(mem.buffersBytes) }]),
                  ]}
                />
              </div>
            </Card>
            <Card>
              <CardHeader title={platform === "windows" ? "Page file" : "Swap"} />
              <div className="space-y-3 px-4 pb-4 pt-3">
                {mem.swapTotalBytes > 0 ? (
                  <>
                    <Meter percent={swapPct} label="Swap used" />
                    <KeyValueList
                      items={[
                        { label: "Total", value: formatBytes(mem.swapTotalBytes) },
                        {
                          label: "Used",
                          value: `${formatBytes(mem.swapUsedBytes)} (${formatPercent(swapPct)})`,
                        },
                      ]}
                    />
                  </>
                ) : (
                  <p className="text-fg-muted">No swap configured.</p>
                )}
                {mem.commitUsedBytes != null && mem.commitLimitBytes != null && (
                  <KeyValueList
                    items={[
                      {
                        label: "Committed",
                        value: `${formatBytes(mem.commitUsedBytes)} / ${formatBytes(mem.commitLimitBytes)}`,
                        hint: "Commit charge: memory promised to programs, backed by RAM plus the page file.",
                      },
                    ]}
                  />
                )}
              </div>
            </Card>
          </div>
        </div>

        <div className="grid gap-4 xl:grid-cols-[1fr_340px]">
          <Card>
            <CardHeader title="Top Memory Consumers" />
            <div className="px-4 pb-4 pt-2">
              {topProcesses.length === 0 ? (
                <p className="py-4 text-fg-muted">No processes available.</p>
              ) : (
                <div className="overflow-x-auto">
                  <table className="w-full text-left text-[12px]">
                    <thead>
                      <tr className="border-b border-border text-[11px] font-medium uppercase tracking-wide text-fg-subtle">
                        <th className="pb-1.5 font-medium">Process</th>
                        <th className="pb-1.5 px-2 text-right font-medium">PID</th>
                        <th className="pb-1.5 px-2 font-medium">User</th>
                        <th className="pb-1.5 px-2 text-right font-medium">Physical RAM</th>
                        <th className="pb-1.5 px-2 text-right font-medium">Share %</th>
                        <th className="pb-1.5 pl-2 text-right font-medium">Virtual Memory</th>
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-border/60">
                      {topProcesses.map((p) => {
                        const sharePct = (p.memoryBytes / total) * 100;
                        const heat = memoryHeatLevel(sharePct);
                        return (
                          <tr
                            key={p.pid}
                            onClick={() => openProcess(p.pid)}
                            className="cursor-pointer transition-colors hover:bg-surface-2"
                            title={`Inspect ${p.name} (PID ${p.pid})`}
                          >
                            <td className="max-w-[180px] truncate py-1.5 pr-2 font-medium text-fg">
                              {p.name}
                            </td>
                            <td className="num py-1.5 px-2 text-right text-fg-subtle">{p.pid}</td>
                            <td className="max-w-[110px] truncate py-1.5 px-2 text-fg-muted">
                              {p.user ?? "—"}
                            </td>
                            <td className="num py-1.5 px-2 text-right font-semibold">
                              {formatBytes(p.memoryBytes)}
                            </td>
                            <td className="py-1.5 px-2 text-right">
                              <div className="flex items-center justify-end gap-2">
                                <div className="hidden w-14 sm:block">
                                  <Meter
                                    percent={sharePct}
                                    label={`${p.name} RAM share`}
                                    size="sm"
                                  />
                                </div>
                                <span
                                  className={cn(
                                    "num w-12 text-right font-medium",
                                    heat === 3 && "text-crit font-semibold",
                                    heat === 2 && "text-warn font-semibold",
                                  )}
                                >
                                  {formatPercent(sharePct)}
                                </span>
                              </div>
                            </td>
                            <td className="num py-1.5 pl-2 text-right text-fg-muted">
                              {formatBytes(p.virtualMemoryBytes)}
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

          <Card>
            <CardHeader title="Memory & Swap Telemetry" />
            <div className="px-4 pb-4 pt-3">
              <KeyValueList
                items={[
                  { label: "Total RAM", value: formatBytes(mem.totalBytes) },
                  {
                    label: "In-Use RAM",
                    value: `${formatBytes(mem.usedBytes)} (${formatPercent(mem.usedPercent)})`,
                  },
                  { label: "Available RAM", value: formatBytes(mem.availableBytes) },
                  { label: "Page Cache", value: optional(mem.cachedBytes) },
                  { label: "Kernel Buffers", value: optional(mem.buffersBytes) },
                  {
                    label: "Swap Total / Used",
                    value:
                      mem.swapTotalBytes > 0
                        ? `${formatBytes(mem.swapTotalBytes)} / ${formatBytes(mem.swapUsedBytes)}`
                        : "Not configured",
                  },
                  {
                    label: "Swap Free",
                    value:
                      mem.swapTotalBytes > 0
                        ? formatBytes(Math.max(0, mem.swapTotalBytes - mem.swapUsedBytes))
                        : "—",
                  },
                  {
                    label: "Commit Charge",
                    value:
                      mem.commitUsedBytes != null && mem.commitLimitBytes != null
                        ? `${formatBytes(mem.commitUsedBytes)} / ${formatBytes(mem.commitLimitBytes)}`
                        : "—",
                  },
                  {
                    label: "Process RSS Sum",
                    value: `${formatBytes(totalProcessRss)} (${formatCount(tick.processes.length)} procs)`,
                    hint: "Sum of resident set sizes across all visible processes (includes shared pages).",
                  },
                  {
                    label: "Total Virtual Mapped",
                    value: formatBytes(totalProcessVirtual),
                  },
                ]}
              />
            </div>
          </Card>
        </div>
      </div>
    </>
  );
}
