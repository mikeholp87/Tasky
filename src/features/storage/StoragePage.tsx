import { HardDrive, Usb } from "lucide-react";
import { ConnectionNotice } from "@/components/ConnectionNotice";
import { PageHeader } from "@/components/PageHeader";
import { DeviceChart, type DeviceChartSeries } from "@/components/charts/DeviceChart";
import { percentAxis, rateAxis, RATE_MIN_Y } from "@/components/charts/series";
import { Card, CardHeader } from "@/components/ui/Card";
import { KeyValueList, Unavailable } from "@/components/ui/KeyValueList";
import { Meter } from "@/components/ui/Meter";
import { diskHistory } from "@/stores/deviceHistory";
import { useMetricsStore } from "@/stores/metricsStore";
import type { DiskSnapshot } from "@/types";
import { formatBytes, formatBytesPerSecond, formatPercent } from "@/utils/format";
import { levelLabel, usageLevel } from "@/utils/levels";

const THROUGHPUT: readonly DeviceChartSeries[] = [
  { label: "Read", key: "read", colorVar: "--res-disk" },
  { label: "Write", key: "write", colorVar: "--res-disk-2", fill: false },
];
const ACTIVE: readonly DeviceChartSeries[] = [
  { label: "Active time", key: "active", colorVar: "--res-disk" },
];

const KIND_LABEL = { ssd: "SSD", hdd: "HDD", unknown: "Unknown type" } as const;
const ops = (v: number | null) => (v == null ? <Unavailable /> : `${v.toFixed(0)} /s`);

export function StoragePage() {
  const disks = useMetricsStore((s) => s.tick?.disks);
  return (
    <>
      <PageHeader title="Storage" description="Drives, capacity and activity." />
      {!disks ? (
        <ConnectionNotice />
      ) : disks.length === 0 ? (
        <p className="text-fg-muted">No drives reported by the operating system.</p>
      ) : (
        <div className="space-y-4">
          {disks.map((d) => (
            <DiskCard key={d.mountPoint} disk={d} />
          ))}
        </div>
      )}
    </>
  );
}

function DiskCard({ disk: d }: { disk: DiskSnapshot }) {
  const platform = useMetricsStore((s) => s.staticInfo?.platform);
  const warning = levelLabel(usageLevel(d.usedPercent, { warning: 90, critical: 95 }), "disk");
  const Icon = d.removable ? Usb : HardDrive;
  return (
    <Card>
      <CardHeader
        title={
          <span className="normal-case tracking-normal">
            <span className="text-fg">{d.mountPoint}</span>
            <span className="ml-2 font-normal text-fg-subtle">
              {d.fileSystem} · {KIND_LABEL[d.kind]}
              {d.removable ? " · Removable" : ""}
              {d.readOnly ? " · Read-only" : ""}
            </span>
          </span>
        }
        icon={<Icon size="0.875rem" aria-hidden />}
      />
      <div className="grid gap-6 px-4 pb-4 pt-3 lg:grid-cols-[320px_1fr]">
        <div className="space-y-3">
          <div className="flex items-baseline justify-between">
            <span className="num text-xl font-semibold">{formatPercent(d.usedPercent)} used</span>
            {warning && (
              <span className="rounded bg-warn-soft px-1.5 py-0.5 text-[0.75rem] font-medium text-warn">
                {d.usedPercent >= 95 ? "Almost full" : "Low space"}
              </span>
            )}
          </div>
          <Meter
            percent={d.usedPercent}
            label={`${d.mountPoint} capacity used`}
            color="--res-disk"
          />
          <KeyValueList
            items={[
              {
                label: platform === "windows" ? "Volume label" : "Device",
                value: d.name || "—",
              },
              { label: "Capacity", value: formatBytes(d.totalBytes) },
              { label: "Used", value: formatBytes(d.usedBytes) },
              { label: "Free", value: formatBytes(d.availableBytes) },
              {
                label: "Read",
                value:
                  d.readBytesPerSec == null ? (
                    <Unavailable />
                  ) : (
                    formatBytesPerSecond(d.readBytesPerSec)
                  ),
              },
              {
                label: "Write",
                value:
                  d.writeBytesPerSec == null ? (
                    <Unavailable />
                  ) : (
                    formatBytesPerSecond(d.writeBytesPerSec)
                  ),
              },
              { label: "Read IOPS", value: ops(d.readOpsPerSec) },
              { label: "Write IOPS", value: ops(d.writeOpsPerSec) },
              {
                label: "Active time",
                value: d.activePercent == null ? <Unavailable /> : formatPercent(d.activePercent),
                hint: "Share of time the device was busy serving reads or writes.",
              },
            ]}
          />
        </div>
        <div className="grid min-w-0 gap-4 md:grid-cols-2">
          <div>
            <div className="mb-1 text-[0.8125rem] font-medium text-fg-muted">Throughput</div>
            <DeviceChart
              source={diskHistory}
              device={d.mountPoint}
              series={THROUGHPUT}
              height={170}
              formatValue={rateAxis}
              minYMax={RATE_MIN_Y}
              ariaLabel={`Read and write throughput of ${d.mountPoint}`}
            />
          </div>
          <div>
            <div className="mb-1 text-[0.8125rem] font-medium text-fg-muted">Active time</div>
            <DeviceChart
              source={diskHistory}
              device={d.mountPoint}
              series={ACTIVE}
              height={170}
              yMax={100}
              formatValue={percentAxis}
              ariaLabel={`Active time of ${d.mountPoint}`}
            />
          </div>
        </div>
      </div>
    </Card>
  );
}
