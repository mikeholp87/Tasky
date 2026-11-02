import { memo, useEffect, useLayoutEffect, useMemo, useRef } from "react";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import { history, useMetricsStore } from "@/stores/metricsStore";
import { useUiStore } from "@/stores/uiStore";
import type { HistoryPoint } from "@/types";

export interface ChartSeries {
  label: string;
  /** CSS custom property holding the color, e.g. "--chart-1". */
  colorVar: string;
  value: (p: HistoryPoint) => number;
  fill?: boolean;
}

interface TimeSeriesChartProps {
  series: readonly ChartSeries[];
  /** Visible time window in seconds. */
  seconds: number;
  height: number;
  /** Fixed upper bound (e.g. 100 for percentages). Omit for auto-scaling. */
  yMax?: number;
  formatValue: (value: number) => string;
  /** Hide axes/grid for compact sparklines. */
  sparkline?: boolean;
  ariaLabel: string;
}

function cssVar(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || "#888";
}

function withAlpha(color: string, alpha: number): string {
  // Tokens are hex; convert to rgba for area fills.
  const m = /^#([0-9a-f]{6})$/i.exec(color);
  if (!m) return color;
  const n = parseInt(m[1] as string, 16);
  return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${alpha})`;
}

function toData(points: HistoryPoint[], series: readonly ChartSeries[]): uPlot.AlignedData {
  const xs = points.map((p) => p.t / 1000);
  return [xs, ...series.map((s) => points.map(s.value))];
}

/**
 * Canvas time-series chart (uPlot). Data is pushed with `setData` on each
 * history update; the chart instance is only rebuilt when its configuration
 * or the theme changes.
 */
export const TimeSeriesChart = memo(function TimeSeriesChart({
  series,
  seconds,
  height,
  yMax,
  formatValue,
  sparkline = false,
  ariaLabel,
}: TimeSeriesChartProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const plotRef = useRef<uPlot | null>(null);
  const theme = useUiStore((s) => s.resolvedTheme);
  const version = useMetricsStore((s) => s.historyVersion);
  const latestT = useMetricsStore((s) => s.tick?.timestampMs ?? 0);

  // Stable key so a parent re-render with equal series doesn't rebuild.
  const seriesKey = useMemo(() => series.map((s) => s.label + s.colorVar).join("|"), [series]);
  const seriesRef = useRef(series);
  const formatRef = useRef(formatValue);
  useLayoutEffect(() => {
    seriesRef.current = series;
    formatRef.current = formatValue;
  });

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const grid = cssVar("--chart-grid");
    const axisColor = cssVar("--fg-subtle");
    const font = `11px ${getComputedStyle(document.body).fontFamily}`;
    const current = seriesRef.current;

    const opts: uPlot.Options = {
      width: Math.max(el.clientWidth, 50),
      height,
      padding: sparkline ? [2, 0, 0, 0] : [8, 8, 0, 0],
      legend: { show: false },
      cursor: sparkline ? { show: false } : { points: { size: 6 }, drag: { x: false, y: false } },
      scales: {
        x: { time: true },
        y:
          yMax != null
            ? { range: [0, yMax] }
            : { range: (_u, _min, max) => [0, Math.max(max * 1.1, 1)] },
      },
      axes: sparkline
        ? [{ show: false }, { show: false }]
        : [
            {
              stroke: axisColor,
              font,
              grid: { stroke: grid, width: 1 },
              ticks: { show: false },
              size: 28,
            },
            {
              stroke: axisColor,
              font,
              grid: { stroke: grid, width: 1 },
              ticks: { show: false },
              size: 64,
              values: (_u, ticks) => ticks.map((v) => formatRef.current(v)),
            },
          ],
      series: [
        {},
        ...current.map((s) => {
          const color = cssVar(s.colorVar);
          return {
            label: s.label,
            stroke: color,
            width: 1.5,
            fill: s.fill === false ? undefined : withAlpha(color, 0.12),
            points: { show: false },
          };
        }),
      ],
    };

    const plot = new uPlot(opts, [[], ...current.map(() => [])], el);
    plotRef.current = plot;

    const resize = new ResizeObserver(() => {
      plot.setSize({ width: Math.max(el.clientWidth, 50), height });
    });
    resize.observe(el);

    return () => {
      resize.disconnect();
      plot.destroy();
      plotRef.current = null;
    };
  }, [seriesKey, height, yMax, sparkline, theme]);

  useEffect(() => {
    const plot = plotRef.current;
    if (!plot) return;
    const now = latestT || Date.now();
    plot.batch(() => {
      // Resetting scales re-runs the y range (fixed or auto); x is pinned so
      // the chart scrolls smoothly even when the window is not yet full.
      plot.setData(toData(history.window(seconds, now), seriesRef.current));
      plot.setScale("x", { min: now / 1000 - seconds, max: now / 1000 });
    });
  }, [version, seconds, latestT, yMax, seriesKey, theme]);

  return (
    <div
      ref={containerRef}
      role="img"
      aria-label={ariaLabel}
      className="w-full"
      style={{ height }}
    />
  );
});
