import { memo, useEffect, useLayoutEffect, useMemo, useRef } from "react";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import { scaled, useRootPx } from "@/hooks/useRootPx";
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
  /** Smallest auto-scaled upper bound, so idle series keep a sensible axis. */
  minYMax?: number;
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
  minYMax = 1,
  formatValue,
  sparkline = false,
  ariaLabel,
}: LineChartProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const plotRef = useRef<uPlot | null>(null);
  const theme = useUiStore((s) => s.resolvedTheme);
  const rootPx = useRootPx();
  const h = scaled(height, rootPx);

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
    const font = `${Math.round(rootPx * 0.75)}px ${getComputedStyle(document.body).fontFamily}`;
    const topAlpha = theme === "dark" ? 0.5 : 0.36;
    const current = seriesRef.current;

    const opts: uPlot.Options = {
      width: Math.max(el.clientWidth, 50),
      height: h,
      padding: sparkline ? [2, 0, 0, 0] : [scaled(10, rootPx), scaled(8, rootPx), 0, 0],
      legend: { show: false },
      cursor: sparkline ? { show: false } : { points: { size: 6 }, drag: { x: false, y: false } },
      scales: {
        x: { time: true },
        y:
          yMax != null
            ? { range: [0, yMax] }
            : { range: (_u, _min, max) => [0, Math.max(max * 1.1, minYMax)] },
      },
      axes: sparkline
        ? [{ show: false }, { show: false }]
        : [
            {
              stroke: axisColor,
              font,
              grid: { show: false },
              ticks: { show: false },
              size: scaled(30, rootPx),
              // "14:05:09" at this font size needs about this much room per label.
              space: scaled(96, rootPx),
              // Single-line clock labels; uPlot's default date+time wraps and clips.
              values: (_u, ticks) => ticks.map(clockLabel),
            },
            {
              stroke: axisColor,
              font,
              grid: { stroke: grid, width: 1, dash: [3, 5] },
              ticks: { show: false },
              size: scaled(68, rootPx),
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
            width: 2,
            // The area glows from the line's own hue down to nothing at the baseline.
            fill:
              s.fill === false
                ? undefined
                : (u: uPlot) => {
                    const { top, height: plotH } = u.bbox;
                    const g = u.ctx.createLinearGradient(0, top, 0, top + plotH);
                    g.addColorStop(0, withAlpha(color, topAlpha));
                    g.addColorStop(0.65, withAlpha(color, topAlpha * 0.25));
                    g.addColorStop(1, withAlpha(color, 0));
                    return g;
                  },
            points: { show: false },
          };
        }),
      ],
    };

    const plot = new uPlot(opts, [[], ...current.map(() => [])], el);
    plotRef.current = plot;
    const resize = new ResizeObserver(() => {
      plot.setSize({ width: Math.max(el.clientWidth, 50), height: h });
    });
    resize.observe(el);
    return () => {
      resize.disconnect();
      plot.destroy();
      plotRef.current = null;
    };
  }, [seriesKey, h, rootPx, yMax, minYMax, sparkline, theme]);

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
  }, [data, xMin, xMax, seriesKey, h, yMax, sparkline, theme]);

  return (
    <div
      ref={containerRef}
      role="img"
      aria-label={ariaLabel}
      className="w-full"
      style={{ height: h }}
    />
  );
});
