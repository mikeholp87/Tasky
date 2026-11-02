import { memo, useMemo } from "react";
import type uPlot from "uplot";
import { history, useMetricsStore } from "@/stores/metricsStore";
import type { HistoryPoint } from "@/types";
import { LineChart, type LineSeries } from "./LineChart";

export interface ChartSeries extends LineSeries {
  value: (p: HistoryPoint) => number;
}

interface TimeSeriesChartProps {
  series: readonly ChartSeries[];
  /** Visible time window in seconds. */
  seconds: number;
  height: number;
  yMax?: number;
  minYMax?: number;
  formatValue: (value: number) => string;
  sparkline?: boolean;
  ariaLabel: string;
}

/** A {@link LineChart} over the shared system history buffer. */
export const TimeSeriesChart = memo(function TimeSeriesChart({
  series,
  seconds,
  ...rest
}: TimeSeriesChartProps) {
  // `history` is mutated in place; `historyVersion` signals that it changed.
  const version = useMetricsStore((s) => s.historyVersion);
  const latestT = useMetricsStore((s) => s.tick?.timestampMs);

  const { data, xRange } = useMemo(() => {
    const end = latestT ?? Date.now();
    const points = history.window(seconds, end);
    const aligned: uPlot.AlignedData = [
      points.map((p) => p.t / 1000),
      ...series.map((s) => points.map(s.value)),
    ];
    return { data: aligned, xRange: [end / 1000 - seconds, end / 1000] as const };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- version tracks in-place mutation
  }, [version, latestT, seconds, series]);

  return <LineChart series={series} data={data} xRange={xRange} {...rest} />;
});
