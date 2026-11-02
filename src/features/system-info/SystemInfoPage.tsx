import { Download } from "lucide-react";
import { DiagnosticsCard } from "./DiagnosticsCard";
import { backend, isBackendAvailable, toAppError } from "@/services/ipc";
import { toast } from "@/stores/toastStore";
import { ConnectionNotice } from "@/components/ConnectionNotice";
import { PageHeader } from "@/components/PageHeader";
import { Card, CardHeader } from "@/components/ui/Card";
import { KeyValueList, Unavailable } from "@/components/ui/KeyValueList";
import { useMetricsStore } from "@/stores/metricsStore";
import { formatBytes, formatDuration, formatTimestamp } from "@/utils/format";
import { uptimeSeconds } from "@/utils/system";

export function SystemInfoPage() {
  const info = useMetricsStore((s) => s.staticInfo);
  const now = useMetricsStore((s) => s.tick?.timestampMs);

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
