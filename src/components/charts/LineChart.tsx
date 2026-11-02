import { memo, useEffect, useLayoutEffect, useMemo, useRef } from "react";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import { useUiStore } from "@/stores/uiStore";

export interface LineSeries {
  label: string;
  /** CSS custom property holding the color, e.g. "--chart-1". */
  colorVar: string;
  fill?: boolean;
}

export interface LineChartProps {
  series: readonly LineSeries[];
  /** uPlot aligned data: [x seconds, ...one array per series]. */
  data: uPlot.AlignedData;
  /** Visible x range in Unix seconds. */
  xRange: readonly [number, number];
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

/** Unix seconds → "14:05:09" in local time. */
function clockLabel(secs: number): string {
  const d = new Date(secs * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

function withAlpha(color: string, alpha: number): string {
  const m = /^#([0-9a-f]{6})$/i.exec(color);
  if (!m) return color;
  const n = parseInt(m[1] as string, 16);
  return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${alpha})`;
}

/**
 * Canvas line chart (uPlot). The instance is created once per configuration
 * and theme; data updates go through `setData` without re-creating it.
 */
export const LineChart = memo(function LineChart({
  series,
  data,
  xRange,
  height,
  yMax,
  formatValue,
  sparkline = false,
  ariaLabel,
}: LineChartProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const plotRef = useRef<uPlot | null>(null);
  const theme = useUiStore((s) => s.resolvedTheme);

  const seriesKey = useMemo(
    () => series.map((s) => `${s.label}:${s.colorVar}:${s.fill !== false}`).join("|"),
    [series],
  );
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
              // Single-line clock labels; uPlot's default date+time wraps and clips.
              values: (_u, ticks) => ticks.map(clockLabel),
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

  const [xMin, xMax] = xRange;
  useEffect(() => {
    const plot = plotRef.current;
    if (!plot) return;
    plot.batch(() => {
      // Resetting scales re-runs the y range (fixed or auto); x is pinned so
      // the chart scrolls smoothly even when the window is not yet full.
      plot.setData(data);
      plot.setScale("x", { min: xMin, max: xMax });
    });
  }, [data, xMin, xMax, seriesKey, height, yMax, sparkline, theme]);

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
