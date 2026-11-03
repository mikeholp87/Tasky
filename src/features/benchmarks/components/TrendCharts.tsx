import { useMemo } from "react";
import type uPlot from "uplot";
import { LineChart, type LineSeries } from "@/components/charts/LineChart";
import { Card, CardHeader } from "@/components/ui/Card";
import { useBenchmarkStore } from "@/stores/benchmarkStore";
import type { MetricUnit } from "@/types";
import { formatMetricText } from "../format";
import { trend } from "../logic";

const TRENDS: { id: string; title: string; unit: MetricUnit }[] = [
  { id: "score.cpu", title: "CPU score", unit: "score" },
  { id: "score.gpu", title: "GPU score", unit: "score" },
  { id: "disk.seq.read", title: "Storage sequential read", unit: "megabytesPerSec" },
  { id: "disk.seq.write", title: "Storage sequential write", unit: "megabytesPerSec" },
  { id: "net.download", title: "Internet download", unit: "mbps" },
  { id: "net.upload", title: "Internet upload", unit: "mbps" },
  { id: "net.latency.idle", title: "Internet latency", unit: "milliseconds" },
];

const LINE: readonly LineSeries[] = [{ label: "Value", colorVar: "--chart-1" }];

/** Results over time, for spotting regressions (drivers, thermals, SSD wear). */
export function TrendCharts() {
  const history = useBenchmarkStore((s) => s.history);
  const capabilities = useBenchmarkStore((s) => s.capabilities);
  const charts = useMemo(() => {
    const major = (capabilities?.methodVersion ?? "1").split(".")[0] ?? "1";
    const scoreVersion = capabilities?.scoreVersion ?? 1;
    return TRENDS.map((t) => ({ ...t, points: trend(history, t.id, major, scoreVersion) })).filter(
      (t) => t.points.length >= 2,
    );
  }, [history, capabilities]);

  if (charts.length === 0) {
    return (
      <p className="text-fg-muted">Trends appear once the same test has been run at least twice.</p>
    );
  }
  return (
    <div className="grid gap-4 lg:grid-cols-2">
      {charts.map((c) => {
        const data: uPlot.AlignedData = [c.points.map((p) => p.t), c.points.map((p) => p.value)];
        const first = c.points[0]?.t ?? 0;
        const last = c.points[c.points.length - 1]?.t ?? first + 1;
        return (
          <Card key={c.id}>
            <CardHeader title={c.title} />
            <div className="px-4 pb-3">
              <LineChart
                series={LINE}
                data={data}
                xRange={[first, Math.max(last, first + 1)]}
                height={140}
                formatValue={(v) => formatMetricText(v, c.unit)}
                ariaLabel={`${c.title} over ${c.points.length} benchmark runs`}
              />
            </div>
          </Card>
        );
      })}
    </div>
  );
}
