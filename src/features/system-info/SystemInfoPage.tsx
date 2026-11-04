import { Activity, Download } from "lucide-react";
import { DiagnosticsCard } from "./DiagnosticsCard";
import { backend, isBackendAvailable, toAppError } from "@/services/ipc";
import { toast } from "@/stores/toastStore";
import { ConnectionNotice } from "@/components/ConnectionNotice";
import { PageHeader } from "@/components/PageHeader";
import { Card, CardHeader } from "@/components/ui/Card";
import { KeyValueList, Unavailable } from "@/components/ui/KeyValueList";
import { useMetricsStore } from "@/stores/metricsStore";
import {
  formatBytes,
  formatBytesPerSecond,
  formatCount,
  formatDuration,
  formatFrequency,
  formatPercent,
  formatTimestamp,
} from "@/utils/format";
import { uptimeSeconds } from "@/utils/system";

export function SystemInfoPage() {
  const info = useMetricsStore((s) => s.staticInfo);
  const tick = useMetricsStore((s) => s.tick);
  const now = tick?.timestampMs;
  const gpu = tick?.gpus[0];

  const diskRead =
    tick?.disks.reduce((acc, d) => acc + (d.readBytesPerSec ?? 0), 0) ?? 0;
  const diskWrite =
    tick?.disks.reduce((acc, d) => acc + (d.writeBytesPerSec ?? 0), 0) ?? 0;
  const netRx =
    tick?.networks.reduce((acc, n) => acc + n.rxBytesPerSec, 0) ?? 0;
  const netTx =
    tick?.networks.reduce((acc, n) => acc + n.txBytesPerSec, 0) ?? 0;

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
      {!info || now == null || !tick ? (
        <ConnectionNotice />
      ) : (
        <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
          <Card>
            <CardHeader title="Operating system" />
            <div className="px-4 pb-4 pt-3">
              <KeyValueList
                items={[
                  { label: "Name", value: info.osName ?? <Unavailable /> },
                  {
                    label: "Version",
                    value: info.longOsVersion ?? info.osVersion ?? <Unavailable />,
                  },
                  { label: "Kernel", value: info.kernelVersion ?? <Unavailable /> },
                  { label: "Hostname", value: info.hostname ?? <Unavailable /> },
                  { label: "Architecture", value: info.architecture },
                ]}
              />
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
                  {
                    label: "Graphics (GPU)",
                    value: gpu ? (
                      `${gpu.name}${gpu.totalVramBytes > 0 ? ` (${formatBytes(gpu.totalVramBytes)})` : ""}`
                    ) : (
                      <Unavailable />
                    ),
                  },
                  { label: "GPU driver", value: gpu?.driverVersion ?? <Unavailable /> },
                ]}
              />
            </div>
          </Card>
          <Card>
            <CardHeader
              title="Live Resource Overview"
              icon={<Activity size={14} aria-hidden />}
            />
            <div className="px-4 pb-4 pt-3">
              <KeyValueList
                items={[
                  {
                    label: "CPU Utilization",
                    value: `${formatPercent(tick.cpu.usagePercent)}${
                      tick.cpu.frequencyMhz != null
                        ? ` · ${formatFrequency(tick.cpu.frequencyMhz)}`
                        : ""
                    }`,
                  },
                  {
                    label: "Memory in Use",
                    value: `${formatBytes(tick.memory.usedBytes)} / ${formatBytes(tick.memory.totalBytes)} (${formatPercent(tick.memory.usedPercent, 0)})`,
                  },
                  {
                    label: "GPU Utilization",
                    value:
                      gpu?.usagePercent != null ? (
                        `${formatPercent(gpu.usagePercent)}${
                          gpu.temperatureCelsius != null
                            ? ` · ${Math.round(gpu.temperatureCelsius)} °C`
                            : ""
                        }`
                      ) : (
                        <Unavailable />
                      ),
                  },
                  {
                    label: "Disk Throughput",
                    value: `↓ ${formatBytesPerSecond(diskRead)} · ↑ ${formatBytesPerSecond(diskWrite)}`,
                  },
                  {
                    label: "Network Throughput",
                    value: `↓ ${formatBytesPerSecond(netRx)} · ↑ ${formatBytesPerSecond(netTx)}`,
                  },
                  {
                    label: "Active Processes",
                    value: formatCount(tick.processes.length),
                  },
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
                ]}
              />
            </div>
          </Card>
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

async function exportSummary() {
  try {
    const path = await backend.exportSystemSummary();
    if (path) toast.success(`System summary saved to ${path}`);
  } catch (e) {
    toast.error(toAppError(e).message);
  }
}
