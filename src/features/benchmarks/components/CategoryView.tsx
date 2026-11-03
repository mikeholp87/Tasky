import { ArrowLeft, Play } from "lucide-react";
import { useMemo } from "react";
import type uPlot from "uplot";
import { LineChart, type LineSeries } from "@/components/charts/LineChart";
import { Card, CardHeader } from "@/components/ui/Card";
import { KeyValueList, type KeyValueItem } from "@/components/ui/KeyValueList";
import { isTerminal, latestByKind, liveSeries, useBenchmarkStore } from "@/stores/benchmarkStore";
import { useMetricsStore } from "@/stores/metricsStore";
import { useSettingsStore } from "@/stores/settingsStore";
import type { BenchmarkKind, GpuAdapterInfo, MetricUnit, StaticSystemInfo } from "@/types";
import { formatBytes } from "@/utils/format";
import { KIND_LABELS, formatMetricText } from "../format";
import { METHODOLOGY } from "../methodology";
import { HistoryList } from "./HistoryView";
import { useBenchmarkRecord } from "../hooks";
import { MetricRow } from "./ResultView";
import { primaryButton, secondaryButton } from "./styles";

const SERIES_PREFIX: Record<BenchmarkKind, string> = {
  cpu: "cpu.",
  gpu: "gpu.",
  disk: "disk.",
  network: "net.",
};

const LIVE_LINE: readonly LineSeries[] = [{ label: "Value", colorVar: "--chart-1", fill: true }];

function LiveChart({ id, unit }: { id: string; unit: MetricUnit }) {
  // Re-read the mutable buffer whenever the store bumps its version.
  const version = useBenchmarkStore((s) => s.liveVersion);
  const data = useMemo<uPlot.AlignedData>(() => {
    const s = liveSeries.get(id);
    return s ? [s.t.slice(), s.v.slice()] : [[], []];
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id, version]);
  const t = data[0] as number[];
  const first = t[0] ?? 0;
  const last = t[t.length - 1] ?? first + 1;
  return (
    <LineChart
      series={LIVE_LINE}
      data={data}
      xRange={[first, Math.max(last, first + 1)]}
      height={140}
      formatValue={(v) => formatMetricText(v, unit)}
      ariaLabel={`Live ${id} measurements`}
    />
  );
}

function LiveCharts({ kind }: { kind: BenchmarkKind }) {
  const version = useBenchmarkStore((s) => s.liveVersion);
  const ids = useMemo(
    () => [...liveSeries.keys()].filter((k) => k.startsWith(SERIES_PREFIX[kind])),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [kind, version],
  );
  if (ids.length === 0) return <p className="px-4 pb-4 text-fg-muted">No live data yet.</p>;
  return (
    <div className="space-y-3 px-4 pb-4">
      {ids.map((id) => {
        const series = liveSeries.get(id);
        if (!series) return null;
        return (
          <div key={id}>
            <p className="mb-1 text-[12px] text-fg-muted">{id}</p>
            <LiveChart id={id} unit={series.unit} />
          </div>
        );
      })}
    </div>
  );
}

const NO_ADAPTERS: GpuAdapterInfo[] = [];

const GPU_TYPES: Record<GpuAdapterInfo["deviceType"], string> = {
  discrete: "Discrete GPU",
  integrated: "Integrated GPU",
  virtual: "Virtual GPU",
  software: "Software renderer",
  other: "Other",
};

function hardwareItems(
  kind: BenchmarkKind,
  info: StaticSystemInfo | null,
  adapters: readonly GpuAdapterInfo[],
  selectedGpu: string | null,
): KeyValueItem[] {
  if (kind === "gpu") {
    // Mirrors the backend's default selection (see gpu::select).
    const gpu =
      adapters.find((a) => a.id === selectedGpu) ??
      adapters.find((a) => a.preferred && a.deviceType !== "software") ??
      adapters.find((a) => a.deviceType !== "software");
    if (!gpu) return [{ label: "GPU", value: "No compatible adapter detected" }];
    return [
      { label: "GPU", value: gpu.name },
      { label: "Vendor", value: gpu.vendor },
      { label: "Backend", value: gpu.backend },
      { label: "Device type", value: GPU_TYPES[gpu.deviceType] },
      { label: "Driver", value: [gpu.driver, gpu.driverInfo].filter(Boolean).join(" ") || "—" },
      { label: "GPU timing", value: gpu.timestampQueries ? "Timestamp queries" : "Wall clock" },
    ];
  }
  return kind === "cpu" && info
    ? [
        { label: "Processor", value: info.cpuBrand },
        { label: "Physical cores", value: info.physicalCoreCount?.toString() ?? "—" },
        { label: "Logical processors", value: String(info.logicalCoreCount) },
        { label: "Memory", value: formatBytes(info.totalMemoryBytes) },
      ]
    : info
      ? [
          { label: "Operating system", value: info.longOsVersion ?? info.osName ?? "—" },
          { label: "Memory", value: formatBytes(info.totalMemoryBytes) },
        ]
      : [];
}

function HardwareCard({ kind }: { kind: BenchmarkKind }) {
  const info = useMetricsStore((s) => s.staticInfo);
  const adapters = useBenchmarkStore((s) => s.capabilities?.gpuAdapters ?? NO_ADAPTERS);
  const selectedGpu = useSettingsStore((s) => s.benchmark.gpuAdapterId);
  const items = hardwareItems(kind, info, adapters, selectedGpu);
  return (
    <Card>
      <CardHeader title="Hardware" />
      <div className="px-4 pb-3">
        <KeyValueList items={items} />
      </div>
    </Card>
  );
}

export function CategoryView({ kind }: { kind: BenchmarkKind }) {
  const setView = useBenchmarkStore((s) => s.setView);
  const requestLaunch = useBenchmarkStore((s) => s.requestLaunch);
  const run = useBenchmarkStore((s) => s.run);
  const history = useBenchmarkStore((s) => s.history);
  const showMethodology = useSettingsStore((s) => s.benchmark.showMethodology);
  const latest = latestByKind(history)[kind];
  const { record } = useBenchmarkRecord(latest?.id ?? null);
  const previous = record?.categories.find((c) => c.kind === kind);
  const running = run !== null && !isTerminal(run.phase);
  const label = KIND_LABELS[kind];

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <button type="button" className={secondaryButton} onClick={() => setView({ name: "home" })}>
          <ArrowLeft size={14} aria-hidden />
          Benchmarks
        </button>
        <button
          type="button"
          className={primaryButton}
          disabled={running}
          onClick={() => requestLaunch([kind])}
        >
          <Play size={14} aria-hidden />
          Run {label} test
        </button>
      </div>
      <h2 className="text-base font-semibold">Benchmarks / {label}</h2>
      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader title="Live measurements" />
          <LiveCharts kind={kind} />
        </Card>
        <HardwareCard kind={kind} />
        <Card>
          <CardHeader title="Previous result" />
          <div className="divide-y divide-border px-4 pb-3">
            {previous ? (
              previous.metrics.map((m) => <MetricRow key={m.id} metric={m} />)
            ) : (
              <p className="py-2 text-fg-muted">No previous result.</p>
            )}
          </div>
        </Card>
        {showMethodology && (
          <Card>
            <CardHeader title="Methodology" />
            <div className="space-y-2 px-4 pb-3 text-fg-muted">
              {METHODOLOGY[kind].map((p) => (
                <p key={p}>{p}</p>
              ))}
            </div>
          </Card>
        )}
      </div>
      <HistoryList kind={kind} />
    </div>
  );
}
