import { useMemo } from "react";
import type uPlot from "uplot";
import { LineChart, type LineSeries } from "@/components/charts/LineChart";
import { Card, CardHeader } from "@/components/ui/Card";
import type { ProcessPoint } from "@/types";

const HISTORY_SECONDS = 300;

export function MetricCard({
  title,
  series,
  history,
  pick,
  yMax,
  minYMax,
  format,
  label,
  footer,
  nowMs,
}: {
  title: string;
  series: readonly LineSeries[];
  history: ProcessPoint[];
  pick: (p: ProcessPoint) => number[];
  yMax?: number;
  minYMax?: number;
  format: (v: number) => string;
  label: string;
  footer?: string;
  /** Latest tick time; anchors the x axis before history exists. */
  nowMs: number;
}) {
  const { data, xRange } = useMemo(() => {
    const xs = history.map((p) => p.t / 1000);
    const columns = series.map((_, i) => history.map((p) => pick(p)[i] ?? 0));
    const end = history.length > 0 ? (xs[xs.length - 1] as number) : nowMs / 1000;
    return {
      data: [xs, ...columns] as uPlot.AlignedData,
      xRange: [end - HISTORY_SECONDS, end] as const,
    };
  }, [history, series, pick, nowMs]);
  return (
    <Card>
      <CardHeader title={title} />
      <div className="px-2 pb-3 pt-2">
        <LineChart
          series={series}
          data={data}
          xRange={xRange}
          height={150}
          yMax={yMax}
          minYMax={minYMax}
          formatValue={format}
          ariaLabel={label}
        />
        {footer && <p className="num px-2 pt-2 text-fg-muted">{footer}</p>}
      </div>
    </Card>
  );
}
