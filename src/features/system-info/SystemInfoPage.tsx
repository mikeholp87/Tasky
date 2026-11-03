import { Download } from "lucide-react";
import { DiagnosticsCard } from "./DiagnosticsCard";
import { backend, isBackendAvailable, toAppError } from "@/services/ipc";
import { toast } from "@/stores/toastStore";
import { ConnectionNotice } from "@/components/ConnectionNotice";
import { PageHeader } from "@/components/PageHeader";
import { Card, CardHeader } from "@/components/ui/Card";
import { KeyValueList, Unavailable } from "@/components/ui/KeyValueList";
import { useMetricsStore } from "@/stores/metricsStore";
import type { KeyValueItem } from "@/components/ui/KeyValueList";
import type { StaticSystemInfo } from "@/types";
import { formatBytes, formatDuration, formatPercent, formatTimestamp } from "@/utils/format";
import { uptimeSeconds } from "@/utils/system";

/** OS rows. Linux shows the os-release name instead of sysinfo's name/version. */
function osItems(info: StaticSystemInfo): KeyValueItem[] {
  const named: KeyValueItem[] = info.distribution
    ? [{ label: "Distribution", value: info.distribution }]
    : [
        { label: "Name", value: info.osName ?? <Unavailable /> },
        { label: "Version", value: info.longOsVersion ?? info.osVersion ?? <Unavailable /> },
      ];
  const optional: [string, string | null][] = [
    ["Desktop", info.desktopEnvironment],
    ["Display server", info.displayServer],
    ["Init system", info.initSystem],
    ["Container", info.container],
  ];
  return [
    ...named,
    { label: "Kernel", value: info.kernelVersion ?? <Unavailable /> },
    { label: "Hostname", value: info.hostname ?? <Unavailable /> },
    { label: "Architecture", value: info.architecture },
    // Shown only where the concept exists and was detected.
    ...optional.flatMap(([label, value]) => (value ? [{ label, value }] : [])),
  ];
}

export function SystemInfoPage() {
  const info = useMetricsStore((s) => s.staticInfo);
  const now = useMetricsStore((s) => s.tick?.timestampMs);
  const load = useMetricsStore((s) => s.tick?.cpu.loadAverage);

  return (
    <>
      <PageHeader
        title="System Information"
        description="Operating system and hardware."
        actions={
          <button
            type="button"
            disabled={!isBackendAvailable()}
            onClick={() => void exportSummary()}
            className="flex h-8 items-center gap-1.5 rounded-md border border-border px-2.5 text-fg-muted hover:text-fg disabled:opacity-50"
          >
            <Download size={14} aria-hidden /> Export summary…
          </button>
        }
      />
      {!info || now == null ? (
        <ConnectionNotice />
      ) : (
        <div className="grid max-w-5xl gap-4 lg:grid-cols-2">
          <Card>
            <CardHeader title="Operating system" />
            <div className="px-4 pb-4 pt-3">
              <KeyValueList items={osItems(info)} />
            </div>
          </Card>
          <Card>
            <CardHeader title="Hardware" />
            <div className="px-4 pb-4 pt-3">
              <KeyValueList
                items={[
                  { label: "Processor", value: info.cpuBrand || <Unavailable /> },
                  { label: "Vendor", value: info.cpuVendor || <Unavailable /> },
                  { label: "Physical cores", value: info.physicalCoreCount ?? <Unavailable /> },
                  { label: "Logical processors", value: info.logicalCoreCount },
                  { label: "Installed RAM", value: formatBytes(info.totalMemoryBytes) },
                ]}
              />
            </div>
          </Card>
          <Card>
            <CardHeader title="Session" />
            <div className="px-4 pb-4 pt-3">
              <KeyValueList
                items={[
                  { label: "Uptime", value: formatDuration(uptimeSeconds(info.bootTimeSecs, now)) },
                  {
                    label: "Boot time",
                    value:
                      info.bootTimeSecs != null ? (
                        formatTimestamp(info.bootTimeSecs * 1000)
                      ) : (
                        <Unavailable />
                      ),
                  },
                  ...(load
                    ? [
                        {
                          label: "Load average (1/5/15 min)",
                          hint: "Average number of tasks running or waiting to run. Not a percentage: compare it with the number of logical processors.",
                          value: `${load.one.toFixed(2)} / ${load.five.toFixed(2)} / ${load.fifteen.toFixed(2)}`,
                        },
                      ]
                    : []),
                ]}
              />
            </div>
          </Card>
          <GraphicsCard />
          <TemperaturesCard />
          <Card>
            <CardHeader title="Task Manager" />
            <div className="px-4 pb-4 pt-3">
              <KeyValueList
                items={[
                  { label: "Version", value: info.appVersion },
                  { label: "Process ID", value: info.selfPid },
                ]}
              />
            </div>
          </Card>
          <DiagnosticsCard />
        </div>
      )}
    </>
  );
}

function GraphicsCard() {
  const gpus = useMetricsStore((s) => s.tick?.gpus);
  if (!gpus) return null;
  const notReported = (
    <span className="font-normal text-fg-subtle">Not reported by the driver</span>
  );
  return (
    <Card>
      <CardHeader title="Graphics" />
      <div className="space-y-4 px-4 pb-4 pt-3">
        {gpus.length === 0 && <p className="text-fg-muted">No graphics adapter detected.</p>}
        {gpus.map((g) => (
          <KeyValueList
            key={g.id}
            items={[
              { label: "Adapter", value: <span title={g.name}>{g.name}</span> },
              { label: "Vendor", value: g.vendor ?? <Unavailable /> },
              {
                label: "Utilization",
                value: g.usagePercent != null ? formatPercent(g.usagePercent, 0) : notReported,
              },
              {
                label: "Dedicated memory",
                hint: "Integrated GPUs share system memory and have no dedicated memory to report.",
                value:
                  g.memoryTotalBytes != null
                    ? `${g.memoryUsedBytes != null ? `${formatBytes(g.memoryUsedBytes)} of ` : ""}${formatBytes(g.memoryTotalBytes)}`
                    : notReported,
              },
            ]}
          />
        ))}
      </div>
    </Card>
  );
}

function TemperaturesCard() {
  const temps = useMetricsStore((s) => s.tick?.temperatures);
  if (!temps) return null;
  const deg = (v: number) => `${v.toFixed(0)} °C`;
  return (
    <Card>
      <CardHeader title="Temperatures" />
      <div className="px-4 pb-4 pt-3">
        {temps.length === 0 ? (
          <p className="text-fg-muted">
            No temperature sensors are exposed by the system (common in virtual machines).
          </p>
        ) : (
          <KeyValueList
            items={temps.map((t) => ({
              label: t.label,
              hint: [
                t.maxCelsius != null ? `Highest seen ${deg(t.maxCelsius)}` : null,
                t.criticalCelsius != null ? `Critical at ${deg(t.criticalCelsius)}` : null,
              ]
                .filter(Boolean)
                .join(" · "),
              value: (
                <span
                  className={
                    t.criticalCelsius != null && t.celsius >= t.criticalCelsius - 5
                      ? "text-crit"
                      : undefined
                  }
                >
                  {deg(t.celsius)}
                </span>
              ),
            }))}
          />
        )}
      </div>
    </Card>
  );
}

async function exportSummary() {
  try {
    const path = await backend.exportSystemSummary();
    if (path) toast.success(`System summary saved to ${path}`);
  } catch (e) {
    toast.error(toAppError(e).message);
  }
}
