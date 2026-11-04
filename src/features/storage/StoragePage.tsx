import { Activity, ArrowDown, ArrowUp, Database, HardDrive, Usb } from "lucide-react";
import { useMemo } from "react";
import { ConnectionNotice } from "@/components/ConnectionNotice";
import { PageHeader } from "@/components/PageHeader";
import { DeviceChart, type DeviceChartSeries } from "@/components/charts/DeviceChart";
import { percentAxis, rateAxis, RATE_MIN_Y } from "@/components/charts/series";
import { Card, CardHeader } from "@/components/ui/Card";
import { KeyValueList, Unavailable } from "@/components/ui/KeyValueList";
import { Meter } from "@/components/ui/Meter";
import { STATUS_LABEL } from "@/features/processes/cells";
import { diskHistory } from "@/stores/deviceHistory";
import { useMetricsStore } from "@/stores/metricsStore";
import { useUiStore } from "@/stores/uiStore";
import type { DiskSnapshot } from "@/types";
import {
  NOT_AVAILABLE,
  formatBytes,
  formatBytesPerSecond,
  formatPercent,
} from "@/utils/format";
import { levelLabel, usageLevel } from "@/utils/levels";

const THROUGHPUT: readonly DeviceChartSeries[] = [
  { label: "Read", key: "read", colorVar: "--chart-1" },
  { label: "Write", key: "write", colorVar: "--chart-2", fill: false },
];
const ACTIVE: readonly DeviceChartSeries[] = [
  { label: "Active time", key: "active", colorVar: "--chart-1" },
];

const KIND_LABEL = { ssd: "SSD", hdd: "HDD", unknown: "Unknown type" } as const;
const ops = (v: number | null) => (v == null ? <Unavailable /> : `${v.toFixed(0)} /s`);

export function StoragePage() {
  const disks = useMetricsStore((s) => s.tick?.disks);
  const processes = useMetricsStore((s) => s.tick?.processes);
  const openProcess = useUiStore((s) => s.openProcess);

  const summary = useMemo(() => {
    if (!disks || disks.length === 0) return null;
    let totalBytes = 0;
    let usedBytes = 0;
    let availableBytes = 0;
    let ssdCount = 0;
    let hddCount = 0;
    let readBytesPerSec = 0;
    let writeBytesPerSec = 0;
    let readOpsPerSec = 0;
    let writeOpsPerSec = 0;
    let hasReadRate = false;
    let hasWriteRate = false;
    let hasReadOps = false;
    let hasWriteOps = false;

    for (const d of disks) {
      totalBytes += d.totalBytes;
      usedBytes += d.usedBytes;
      availableBytes += d.availableBytes;
      if (d.kind === "ssd") ssdCount++;
      else if (d.kind === "hdd") hddCount++;
      if (d.readBytesPerSec != null) {
        readBytesPerSec += d.readBytesPerSec;
        hasReadRate = true;
      }
      if (d.writeBytesPerSec != null) {
        writeBytesPerSec += d.writeBytesPerSec;
        hasWriteRate = true;
      }
      if (d.readOpsPerSec != null) {
        readOpsPerSec += d.readOpsPerSec;
        hasReadOps = true;
      }
      if (d.writeOpsPerSec != null) {
        writeOpsPerSec += d.writeOpsPerSec;
        hasWriteOps = true;
      }
    }

    const usedPercent = totalBytes > 0 ? (usedBytes / totalBytes) * 100 : 0;
    return {
      totalBytes,
      usedBytes,
      availableBytes,
      usedPercent,
      ssdCount,
      hddCount,
      readBytesPerSec: hasReadRate ? readBytesPerSec : null,
      writeBytesPerSec: hasWriteRate ? writeBytesPerSec : null,
      readOpsPerSec: hasReadOps ? readOpsPerSec : null,
      writeOpsPerSec: hasWriteOps ? writeOpsPerSec : null,
    };
  }, [disks]);

  const topIoProcesses = useMemo(() => {
    if (!processes) return [];
    return [...processes]
      .filter((p) => !p.restricted)
      .sort(
        (a, b) =>
          b.diskReadPerSec + b.diskWritePerSec - (a.diskReadPerSec + a.diskWritePerSec) ||
          b.cpuPercent - a.cpuPercent,
      )
      .slice(0, 6);
  }, [processes]);

  return (
    <>
      <PageHeader title="Storage" description="Drives, capacity and activity." />
      {!disks ? (
        <ConnectionNotice />
      ) : disks.length === 0 ? (
        <p className="text-fg-muted">No drives reported by the operating system.</p>
      ) : (
        <div className="space-y-4">
          {summary && (
            <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
              <Card>
                <CardHeader
                  title="Total Capacity & Used"
                  icon={<Database size={14} aria-hidden />}
                />
                <div className="space-y-2.5 px-4 pb-4 pt-3">
                  <div className="flex items-baseline justify-between">
                    <span className="num text-xl font-semibold">
                      {formatPercent(summary.usedPercent)}
                    </span>
                    <span className="num text-[12px] text-fg-muted">
                      {formatBytes(summary.usedBytes)} / {formatBytes(summary.totalBytes)}
                    </span>
                  </div>
                  <Meter
                    percent={summary.usedPercent}
                    label="Overall storage capacity used"
                    size="sm"
                  />
                </div>
              </Card>

              <Card>
                <CardHeader
                  title="Total Available Space"
                  icon={<HardDrive size={14} aria-hidden />}
                />
                <div className="space-y-1.5 px-4 pb-4 pt-3">
                  <div className="num text-xl font-semibold">
                    {formatBytes(summary.availableBytes)}
                  </div>
                  <div className="num text-[12px] text-fg-muted">
                    {disks.length} {disks.length === 1 ? "drive" : "drives"} · {summary.ssdCount}{" "}
                    SSD · {summary.hddCount} HDD
                  </div>
                </div>
              </Card>

              <Card>
                <CardHeader
                  title="Aggregate Read Throughput"
                  icon={<ArrowDown size={14} aria-hidden />}
                />
                <div className="space-y-1.5 px-4 pb-4 pt-3">
                  <div className="num text-xl font-semibold">
                    {formatBytesPerSecond(summary.readBytesPerSec)}
                  </div>
                  <div className="num text-[12px] text-fg-muted">
                    Read IOPS:{" "}
                    {summary.readOpsPerSec != null
                      ? `${summary.readOpsPerSec.toFixed(0)} /s`
                      : NOT_AVAILABLE}
                  </div>
                </div>
              </Card>

              <Card>
                <CardHeader
                  title="Aggregate Write Throughput"
                  icon={<ArrowUp size={14} aria-hidden />}
                />
                <div className="space-y-1.5 px-4 pb-4 pt-3">
                  <div className="num text-xl font-semibold">
                    {formatBytesPerSecond(summary.writeBytesPerSec)}
                  </div>
                  <div className="num text-[12px] text-fg-muted">
                    Write IOPS:{" "}
                    {summary.writeOpsPerSec != null
                      ? `${summary.writeOpsPerSec.toFixed(0)} /s`
                      : NOT_AVAILABLE}
                  </div>
                </div>
              </Card>
            </div>
          )}

          {disks.map((d) => (
            <DiskCard key={d.mountPoint} disk={d} />
          ))}

          {topIoProcesses.length > 0 && (
            <Card>
              <CardHeader
                title="Top Disk I/O Processes"
                icon={<Activity size={14} aria-hidden />}
              />
              <div className="overflow-x-auto px-4 pb-4 pt-2">
                <table className="w-full text-left text-[12px]">
                  <thead className="border-b border-border text-fg-muted">
                    <tr>
                      <th className="py-2 pr-3 font-medium">Process</th>
                      <th className="py-2 pr-3 text-right font-medium">PID</th>
                      <th className="py-2 pr-3 font-medium">Status</th>
                      <th className="py-2 pr-3 text-right font-medium">Read Rate</th>
                      <th className="py-2 pr-3 text-right font-medium">Write Rate</th>
                      <th className="py-2 pr-3 text-right font-medium">Total Disk I/O</th>
                      <th className="py-2 text-right font-medium">Action</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-border/60">
                    {topIoProcesses.map((p) => {
                      const totalIo = p.diskReadPerSec + p.diskWritePerSec;
                      return (
                        <tr
                          key={p.pid}
                          onClick={() => openProcess(p.pid)}
                          className="cursor-pointer transition-colors hover:bg-surface-2"
                        >
                          <td className="max-w-[200px] truncate py-2 pr-3 font-medium text-fg">
                            {p.name}
                          </td>
                          <td className="num py-2 pr-3 text-right text-fg-muted">{p.pid}</td>
                          <td className="py-2 pr-3 text-fg-muted">{STATUS_LABEL[p.status]}</td>
                          <td className="num py-2 pr-3 text-right text-fg-muted">
                            {p.diskReadPerSec > 0
                              ? formatBytesPerSecond(p.diskReadPerSec)
                              : "0 B/s"}
                          </td>
                          <td className="num py-2 pr-3 text-right text-fg-muted">
                            {p.diskWritePerSec > 0
                              ? formatBytesPerSecond(p.diskWritePerSec)
                              : "0 B/s"}
                          </td>
                          <td className="num py-2 pr-3 text-right font-medium text-fg">
                            {totalIo > 0 ? formatBytesPerSecond(totalIo) : "0 B/s"}
                          </td>
                          <td className="py-2 text-right">
                            <button
                              type="button"
                              onClick={(e) => {
                                e.stopPropagation();
                                openProcess(p.pid);
                              }}
                              className="rounded-md border border-border bg-surface px-2.5 py-1 text-[11px] font-medium text-fg hover:border-accent hover:text-accent"
                            >
                              Inspect →
                            </button>
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            </Card>
          )}
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
        icon={<Icon size={14} aria-hidden />}
      />
      <div className="grid gap-6 px-4 pb-4 pt-3 lg:grid-cols-[320px_1fr]">
        <div className="space-y-3">
          <div className="flex items-baseline justify-between">
            <span className="num text-xl font-semibold">{formatPercent(d.usedPercent)} used</span>
            {warning && (
              <span className="rounded bg-warn-soft px-1.5 py-0.5 text-[11px] font-medium text-warn">
                {d.usedPercent >= 95 ? "Almost full" : "Low space"}
              </span>
            )}
          </div>
          <Meter percent={d.usedPercent} label={`${d.mountPoint} capacity used`} />
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
            <div className="mb-1 text-[12px] font-medium text-fg-muted">Throughput</div>
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
            <div className="mb-1 text-[12px] font-medium text-fg-muted">Active time</div>
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
