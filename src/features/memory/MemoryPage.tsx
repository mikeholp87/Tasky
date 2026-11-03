import { ConnectionNotice } from "@/components/ConnectionNotice";
import { PageHeader } from "@/components/PageHeader";
import { TimeSeriesChart } from "@/components/charts/TimeSeriesChart";
import { MEMORY_SERIES, percentAxis } from "@/components/charts/series";
import { Card, CardHeader } from "@/components/ui/Card";
import { KeyValueList, Unavailable } from "@/components/ui/KeyValueList";
import { Meter } from "@/components/ui/Meter";
import { useMetricsStore } from "@/stores/metricsStore";
import { TopMemoryCard } from "./TopMemoryCard";
import { formatBytes, formatPercent } from "@/utils/format";

export function MemoryPage() {
  const mem = useMetricsStore((s) => s.tick?.memory);
  const platform = useMetricsStore((s) => s.staticInfo?.platform);

  if (!mem) {
    return (
      <>
        <PageHeader title="Memory" />
        <ConnectionNotice />
      </>
    );
  }
  const swapPct = mem.swapTotalBytes > 0 ? (mem.swapUsedBytes / mem.swapTotalBytes) * 100 : 0;
  const optional = (v: number | null) => (v == null ? <Unavailable /> : formatBytes(v));

  return (
    <>
      <PageHeader
        title="Memory"
        description={`${formatBytes(mem.usedBytes)} of ${formatBytes(mem.totalBytes)} in use`}
      />
      <div className="grid gap-4 lg:grid-cols-[1fr_340px]">
        <div className="min-w-0 space-y-4">
          <Card>
            <CardHeader title={`Usage · ${formatPercent(mem.usedPercent)}`} />
            <div className="px-2 pb-3 pt-2">
              <TimeSeriesChart
                series={MEMORY_SERIES}
                seconds={60}
                height={150}
                yMax={100}
                formatValue={percentAxis}
                ariaLabel={`Memory usage over the last minute, currently ${formatPercent(mem.usedPercent)}`}
              />
            </div>
          </Card>
          <TopMemoryCard />
        </div>
        <div className="space-y-4">
          <Card>
            <CardHeader title="Physical memory" />
            <div className="space-y-3 px-4 pb-4 pt-3">
              <Meter percent={mem.usedPercent} label="Physical memory used" color="--res-mem" />
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
                  <Meter percent={swapPct} label="Swap used" color="--res-mem" />
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
    </>
  );
}
