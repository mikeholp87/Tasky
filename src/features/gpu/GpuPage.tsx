import { Cpu, Flame, MemoryStick, Zap } from "lucide-react";
import { ConnectionNotice } from "@/components/ConnectionNotice";
import { PageHeader } from "@/components/PageHeader";
import { TimeSeriesChart } from "@/components/charts/TimeSeriesChart";
import {
  GPU_SERIES,
  GPU_TEMP_SERIES,
  GPU_VRAM_SERIES,
  percentAxis,
  tempAxis,
} from "@/components/charts/series";
import { Card, CardHeader } from "@/components/ui/Card";
import { KeyValueList, Unavailable } from "@/components/ui/KeyValueList";
import { Meter } from "@/components/ui/Meter";
import { useMetricsStore } from "@/stores/metricsStore";
import {
  NOT_AVAILABLE,
  formatBytes,
  formatFrequency,
  formatPercent,
} from "@/utils/format";

export function GpuPage() {
  const tick = useMetricsStore((s) => s.tick);
  const gpu = tick?.gpus[0];

  const usage = gpu?.usagePercent ?? tick?.point.gpuUsage ?? null;
  const vramPct =
    gpu && gpu.totalVramBytes > 0
      ? (gpu.usedVramBytes / gpu.totalVramBytes) * 100
      : (tick?.point.gpuVramPercent ?? null);
  const temp = gpu?.temperatureCelsius ?? tick?.point.gpuTemp ?? null;

  const vramHeader =
    gpu && gpu.totalVramBytes > 0
      ? `Dedicated VRAM · ${formatBytes(gpu.usedVramBytes)} / ${formatBytes(gpu.totalVramBytes)} (${formatPercent(vramPct, 0)})`
      : `Dedicated VRAM · ${formatPercent(vramPct)}`;

  const tempParts = [
    temp != null ? `${Math.round(temp)} °C` : formatPercent(null),
    gpu?.fanSpeedPercent != null ? `Fan ${formatPercent(gpu.fanSpeedPercent, 0)}` : null,
  ].filter(Boolean);

  const thermalStatus =
    temp == null
      ? { label: "No sensor", cls: "bg-surface-2 text-fg-muted" }
      : temp >= 85
        ? { label: "Hot", cls: "bg-crit/15 text-crit" }
        : temp >= 75
          ? { label: "Warm", cls: "bg-warn-soft text-warn" }
          : { label: "Optimal", cls: "bg-accent-soft text-fg" };

  return (
    <>
      <PageHeader
        title="GPU"
        description={gpu?.name || "Graphics processor utilization, VRAM, and thermal telemetry."}
      />
      {!tick ? (
        <ConnectionNotice />
      ) : (
        <div className="space-y-4">
          <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
            <Card>
              <CardHeader
                title="Core Utilization"
                icon={<Cpu size={14} aria-hidden />}
                action={
                  gpu?.pstate ? (
                    <span className="rounded bg-surface-2 px-1.5 py-0.5 font-mono text-[11px] font-medium text-fg">
                      {gpu.pstate}
                    </span>
                  ) : null
                }
              />
              <div className="space-y-2.5 px-4 pb-4 pt-3">
                <div className="flex items-baseline justify-between">
                  <span className="num text-xl font-semibold">{formatPercent(usage)}</span>
                  <span className="text-[12px] text-fg-muted">
                    {gpu?.pstate ? `State ${gpu.pstate}` : "Shader & compute"}
                  </span>
                </div>
                <Meter percent={usage ?? 0} label="GPU core utilization" size="sm" />
              </div>
            </Card>

            <Card>
              <CardHeader
                title="Dedicated VRAM"
                icon={<MemoryStick size={14} aria-hidden />}
              />
              <div className="space-y-2.5 px-4 pb-4 pt-3">
                <div className="flex items-baseline justify-between">
                  <span className="num text-xl font-semibold">{formatPercent(vramPct)}</span>
                  <span className="num text-[12px] text-fg-muted">
                    {gpu && gpu.totalVramBytes > 0
                      ? `${formatBytes(gpu.usedVramBytes)} / ${formatBytes(gpu.totalVramBytes)}`
                      : NOT_AVAILABLE}
                  </span>
                </div>
                <Meter percent={vramPct ?? 0} label="Dedicated VRAM usage" size="sm" />
              </div>
            </Card>

            <Card>
              <CardHeader
                title="Thermal & Cooling"
                icon={<Flame size={14} aria-hidden />}
                action={
                  <span
                    className={`rounded px-1.5 py-0.5 text-[11px] font-medium ${thermalStatus.cls}`}
                  >
                    {thermalStatus.label}
                  </span>
                }
              />
              <div className="space-y-1.5 px-4 pb-4 pt-3">
                <div className="num text-xl font-semibold">
                  {temp != null ? `${Math.round(temp)} °C` : NOT_AVAILABLE}
                </div>
                <div className="num text-[12px] text-fg-muted">
                  Fan Speed:{" "}
                  {gpu?.fanSpeedPercent != null
                    ? formatPercent(gpu.fanSpeedPercent, 0)
                    : NOT_AVAILABLE}
                </div>
              </div>
            </Card>

            <Card>
              <CardHeader
                title="Power & Clocks"
                icon={<Zap size={14} aria-hidden />}
              />
              <div className="space-y-1.5 px-4 pb-4 pt-3">
                <div className="num text-xl font-semibold">
                  {gpu?.powerWatts != null
                    ? gpu.powerLimitWatts != null
                      ? `${gpu.powerWatts.toFixed(1)} W / ${gpu.powerLimitWatts.toFixed(0)} W`
                      : `${gpu.powerWatts.toFixed(1)} W`
                    : NOT_AVAILABLE}
                </div>
                <div className="num text-[12px] text-fg-muted">
                  Core {gpu?.clockMhz != null ? formatFrequency(gpu.clockMhz) : NOT_AVAILABLE} ·
                  Mem{" "}
                  {gpu?.memoryClockMhz != null
                    ? formatFrequency(gpu.memoryClockMhz)
                    : NOT_AVAILABLE}
                </div>
              </div>
            </Card>
          </div>

          <div className="grid gap-4 lg:grid-cols-2">
            <Card>
              <CardHeader title={`Core Utilization · ${formatPercent(usage)}`} />
              <div className="px-2 pb-3 pt-2">
                <TimeSeriesChart
                  series={GPU_SERIES}
                  seconds={60}
                  height={200}
                  yMax={100}
                  formatValue={percentAxis}
                  ariaLabel={`GPU core utilization over the last minute, currently ${formatPercent(usage)}`}
                />
              </div>
            </Card>

            <Card>
              <CardHeader title={vramHeader} />
              <div className="space-y-2.5 px-4 pb-3 pt-2">
                <Meter percent={vramPct ?? 0} label="Dedicated VRAM usage" size="sm" />
                <div className="-mx-2">
                  <TimeSeriesChart
                    series={GPU_VRAM_SERIES}
                    seconds={60}
                    height={200}
                    yMax={100}
                    formatValue={percentAxis}
                    ariaLabel={`GPU VRAM usage over the last minute, currently ${formatPercent(vramPct)}`}
                  />
                </div>
              </div>
            </Card>
          </div>

          <div className="grid gap-4 lg:grid-cols-2">
            <Card>
              <CardHeader title={`Temperature · ${tempParts.join(" · ")}`} />
              <div className="px-2 pb-3 pt-2">
                <TimeSeriesChart
                  series={GPU_TEMP_SERIES}
                  seconds={60}
                  height={200}
                  yMax={100}
                  formatValue={tempAxis}
                  ariaLabel={`GPU temperature over the last minute, currently ${temp != null ? `${Math.round(temp)} °C` : "unavailable"}`}
                />
              </div>
            </Card>

            <Card>
              <CardHeader title="Hardware & Telemetry Details" />
              <div className="px-4 pb-4 pt-3">
                <KeyValueList
                  items={[
                    { label: "GPU Model", value: gpu?.name || <Unavailable /> },
                    { label: "Driver Version", value: gpu?.driverVersion || <Unavailable /> },
                    {
                      label: "Core Utilization",
                      value: usage != null ? formatPercent(usage) : <Unavailable />,
                    },
                    {
                      label: "Dedicated VRAM",
                      value:
                        gpu && gpu.totalVramBytes > 0 ? (
                          `${formatBytes(gpu.usedVramBytes)} / ${formatBytes(gpu.totalVramBytes)}${
                            vramPct != null ? ` (${formatPercent(vramPct, 0)})` : ""
                          }`
                        ) : (
                          <Unavailable />
                        ),
                    },
                    {
                      label: "Graphics Clock",
                      value:
                        gpu?.clockMhz != null ? formatFrequency(gpu.clockMhz) : <Unavailable />,
                    },
                    {
                      label: "Memory Clock",
                      value:
                        gpu?.memoryClockMhz != null ? (
                          formatFrequency(gpu.memoryClockMhz)
                        ) : (
                          <Unavailable />
                        ),
                    },
                    {
                      label: "Temperature",
                      value: temp != null ? `${Math.round(temp)} °C` : <Unavailable />,
                    },
                    {
                      label: "Fan Speed",
                      value:
                        gpu?.fanSpeedPercent != null ? (
                          formatPercent(gpu.fanSpeedPercent, 0)
                        ) : (
                          <Unavailable />
                        ),
                    },
                    {
                      label: "Power Draw",
                      value:
                        gpu?.powerWatts != null ? (
                          gpu.powerLimitWatts != null ? (
                            `${gpu.powerWatts.toFixed(1)} W / ${gpu.powerLimitWatts.toFixed(1)} W`
                          ) : (
                            `${gpu.powerWatts.toFixed(1)} W`
                          )
                        ) : (
                          <Unavailable />
                        ),
                    },
                    {
                      label: "Performance State",
                      value: gpu?.pstate || <Unavailable />,
                    },
                    {
                      label: "Video Encoder / Decoder",
                      value:
                        gpu?.encoderUsagePercent != null || gpu?.decoderUsagePercent != null ? (
                          `${formatPercent(gpu.encoderUsagePercent ?? 0, 0)} / ${formatPercent(gpu.decoderUsagePercent ?? 0, 0)}`
                        ) : (
                          <Unavailable />
                        ),
                    },
                    {
                      label: "PCIe Link",
                      value:
                        gpu?.pcieGen != null && gpu?.pcieWidth != null ? (
                          `Gen ${gpu.pcieGen} × ${gpu.pcieWidth}`
                        ) : (
                          <Unavailable />
                        ),
                    },
                  ]}
                />
              </div>
            </Card>
          </div>
        </div>
      )}
    </>
  );
}
