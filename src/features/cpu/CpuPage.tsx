import { memo } from "react";
import { ConnectionNotice } from "@/components/ConnectionNotice";
import { PageHeader } from "@/components/PageHeader";
import { TimeSeriesChart } from "@/components/charts/TimeSeriesChart";
import { CPU_SERIES, coreSeries, percentAxis } from "@/components/charts/series";
import { Card, CardHeader } from "@/components/ui/Card";
import { KeyValueList, Unavailable } from "@/components/ui/KeyValueList";
import { useMetricsStore } from "@/stores/metricsStore";
import { formatFrequency, formatPercent } from "@/utils/format";
import { usageLevel } from "@/utils/levels";
import { cn } from "@/lib/cn";

export function CpuPage() {
  const cpu = useMetricsStore((s) => s.tick?.cpu);
  const info = useMetricsStore((s) => s.staticInfo);
  const platform = info?.platform;

  return (
    <>
      <PageHeader title="CPU" description={info?.cpuBrand || "Total and per-core utilization."} />
      {!cpu ? (
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
        </div>
      )}
    </>
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
