import { memo, useMemo } from "react";
import type uPlot from "uplot";
import type { DeviceHistory } from "@/stores/deviceHistory";
import { useMetricsStore } from "@/stores/metricsStore";
import { LineChart, type LineSeries } from "./LineChart";

export interface DeviceChartSeries extends LineSeries {
  /** Metric name in the device's series, e.g. "read" or "rx". */
  key: string;
}

interface DeviceChartProps {
  source: DeviceHistory;
  device: string;
  series: readonly DeviceChartSeries[];
  height: number;
  /** Visible window in seconds (history keeps up to five minutes). */
  seconds?: number;
  yMax?: number;
  minYMax?: number;
  formatValue: (v: number) => string;
  ariaLabel: string;
}

/** A {@link LineChart} over one device's recent history. */
export const DeviceChart = memo(function DeviceChart({
  source,
  device,
  series,
  seconds = 60,
  ...rest
}: DeviceChartProps) {
  // The buffers are mutated in place; `historyVersion` signals changes.
  const version = useMetricsStore((s) => s.historyVersion);
  const latestT = useMetricsStore((s) => s.tick?.timestampMs ?? 0);

  const { data, xRange } = useMemo(() => {
    const s = source.get(device);
    const xs = (s?.t ?? []).map((t) => t / 1000);
    const cols = series.map((def) => (s?.values[def.key] ?? []).slice());
    const end = latestT / 1000;
    return {
      data: [xs, ...cols] as uPlot.AlignedData,
      xRange: [end - seconds, end] as const,
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- version tracks in-place mutation
  }, [source, device, series, seconds, latestT, version]);

  return <LineChart series={series} data={data} xRange={xRange} {...rest} />;
});
