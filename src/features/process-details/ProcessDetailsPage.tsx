import { ArrowLeft, RefreshCw } from "lucide-react";
import { useMemo } from "react";
import type uPlot from "uplot";
import { EmptyState } from "@/components/EmptyState";
import { PageHeader } from "@/components/PageHeader";
import { LineChart, type LineSeries } from "@/components/charts/LineChart";
import { percentAxis, rateAxis } from "@/components/charts/series";
import { Card, CardHeader } from "@/components/ui/Card";
import { KeyValueList, Unavailable } from "@/components/ui/KeyValueList";
import { childrenOf } from "@/features/processes/tree";
import { STATUS_LABEL } from "@/features/processes/cells";
import { useMetricsStore } from "@/stores/metricsStore";
import { useUiStore } from "@/stores/uiStore";
import type { Connection, ProcessPoint, ProcessRow } from "@/types";
import {
  NOT_AVAILABLE,
  formatBytes,
  formatBytesPerSecond,
  formatDuration,
  formatPercent,
  formatTimestamp,
} from "@/utils/format";
import { useProcessDetails } from "./useProcessDetails";

const HISTORY_SECONDS = 300;
const CPU_SERIES: readonly LineSeries[] = [{ label: "CPU", colorVar: "--chart-1" }];
const MEM_SERIES: readonly LineSeries[] = [{ label: "Memory", colorVar: "--chart-2" }];
const IO_SERIES: readonly LineSeries[] = [
  { label: "Read", colorVar: "--chart-1" },
  { label: "Write", colorVar: "--chart-2", fill: false },
];
const bytesAxis = (v: number) => formatBytes(v);

export function ProcessDetailsPage() {
  const pid = useUiStore((s) => s.selectedPid);
  const navigate = useUiStore((s) => s.navigate);

  if (pid == null) {
    return (
      <>
        <PageHeader title="Process Details" />
        <EmptyState title="No process selected">
          Open a process from the{" "}
          <button
            type="button"
            className="text-accent underline"
            onClick={() => navigate("processes")}
          >
            Processes
          </button>{" "}
          page by double-clicking it or pressing Enter.
        </EmptyState>
      </>
    );
  }
  // Keyed so all local state resets when switching processes.
  return <ProcessDetailsView key={pid} pid={pid} />;
}

function ProcessDetailsView({ pid }: { pid: number }) {
  const { live, details, history, loading, error, refresh } = useProcessDetails(pid);
  const navigate = useUiStore((s) => s.navigate);
  const openProcess = useUiStore((s) => s.openProcess);
  const rows = useMetricsStore((s) => s.tick?.processes);
  const totalMemory = useMetricsStore((s) => s.tick?.memory.totalBytes ?? 0);
  const nowMs = useMetricsStore((s) => s.tick?.timestampMs ?? 0);

  const row: ProcessRow | null = live ?? details?.row ?? null;
  const exited = live == null && details != null;
  const children = useMemo(() => (rows && live ? childrenOf(rows, pid) : []), [rows, live, pid]);

  if (!row) {
    return (
      <>
        <PageHeader title={`Process ${pid}`} />
        {error ? (
          <EmptyState title="Unable to load process">{error.message}</EmptyState>
        ) : loading ? (
          <EmptyState title="Loading…" />
        ) : (
          <EmptyState title="Process not found">PID {pid} is not running.</EmptyState>
        )}
      </>
    );
  }

  const memPct = totalMemory > 0 ? (row.memoryBytes / totalMemory) * 100 : null;
  const runtime = row.startTime > 0 && nowMs > 0 ? nowMs / 1000 - row.startTime : null;

  return (
    <>
      <PageHeader
        title={row.name}
        description={`PID ${row.pid}${row.user ? ` · ${row.user}` : ""}`}
        actions={
          <>
            <button
              type="button"
              onClick={() => navigate("processes")}
              className="flex h-8 items-center gap-1.5 rounded-md border border-border px-2.5 text-fg-muted hover:text-fg"
            >
              <ArrowLeft size={14} aria-hidden /> Processes
            </button>
            <button
              type="button"
              onClick={refresh}
              disabled={loading}
              className="flex h-8 items-center gap-1.5 rounded-md border border-border px-2.5 text-fg-muted hover:text-fg disabled:opacity-50"
            >
              <RefreshCw size={14} aria-hidden /> Refresh details
            </button>
          </>
        }
      />
      {exited && (
        <div
          role="status"
          className="mb-4 rounded-md border border-border bg-surface-2 px-4 py-2 text-fg-muted"
        >
          This process has exited. Showing the last known information.
        </div>
      )}
      {row.restricted && (
        <div
          role="note"
          className="mb-4 rounded-md border border-border bg-surface-2 px-4 py-2 text-fg-muted"
        >
          The operating system restricts access to this process. CPU and disk activity are
          unavailable unless Task Manager runs with administrator/root rights; memory comes from a
          system-wide snapshot.
        </div>
      )}
      {error && !exited && (
        <div role="alert" className="mb-4 rounded-md bg-warn-soft px-4 py-2 text-warn">
          {error.message}
        </div>
      )}

      <div className="grid gap-4 xl:grid-cols-2">
        <Card className="xl:col-span-2">
          <CardHeader title="General" />
          <div className="grid gap-x-10 px-4 pb-4 pt-3 md:grid-cols-2">
            <KeyValueList
              items={[
                { label: "Name", value: row.name },
                { label: "PID", value: row.pid },
                {
                  label: "Parent",
                  value:
                    row.parentPid == null ? (
                      NOT_AVAILABLE
                    ) : (
                      <button
                        type="button"
                        className="text-accent hover:underline"
                        onClick={() => row.parentPid != null && openProcess(row.parentPid)}
                      >
                        {details?.parentName ?? "PID"} ({row.parentPid})
                      </button>
                    ),
                },
                {
                  label: "User",
                  value: row.user ?? <Unavailable />,
                  hint: "Owner information for other accounts' processes may require administrator/root rights.",
                },
                { label: "Status", value: STATUS_LABEL[row.status] },
                { label: "Architecture", value: details?.architecture ?? NOT_AVAILABLE },
              ]}
            />
            <KeyValueList
              items={[
                {
                  label: "Started",
                  value: row.startTime > 0 ? formatTimestamp(row.startTime * 1000) : NOT_AVAILABLE,
                },
                { label: "Runtime", value: formatDuration(runtime) },
                { label: "Threads", value: row.threads ?? <Unavailable /> },
                { label: "Priority", value: row.priority ?? <Unavailable /> },
                { label: "Session", value: row.sessionId ?? NOT_AVAILABLE },
                { label: "Children", value: live ? children.length : NOT_AVAILABLE },
              ]}
            />
          </div>
          <div className="space-y-2 border-t border-border px-4 py-3">
            <PathField label="Executable" value={row.exe} />
            <PathField label="Command line" value={row.cmd} />
            <PathField label="Working directory" value={details?.cwd ?? null} />
          </div>
        </Card>

        <MetricCard
          nowMs={nowMs}
          title={`CPU · ${row.restricted ? NOT_AVAILABLE : formatPercent(row.cpuPercent)}`}
          series={CPU_SERIES}
          history={history}
          pick={(p) => [p.cpu]}
          yMax={100}
          format={percentAxis}
          label="CPU usage of this process"
        />
        <MetricCard
          nowMs={nowMs}
          title={`Memory · ${formatBytes(row.memoryBytes)}${memPct != null ? ` (${formatPercent(memPct)})` : ""}`}
          series={MEM_SERIES}
          history={history}
          pick={(p) => [p.memory]}
          format={bytesAxis}
          label="Memory usage of this process"
          footer={`Virtual memory ${formatBytes(row.virtualMemoryBytes)}`}
        />
        <MetricCard
          nowMs={nowMs}
          title={
            row.restricted
              ? `Disk · ${NOT_AVAILABLE}`
              : `Disk · ↓ ${formatBytesPerSecond(row.diskReadPerSec)} ↑ ${formatBytesPerSecond(row.diskWritePerSec)}`
          }
          series={IO_SERIES}
          history={history}
          pick={(p) => [p.read, p.write]}
          format={rateAxis}
          label="Disk I/O of this process"
          footer={
            details?.totalReadBytes != null || details?.totalWrittenBytes != null
              ? `Total read ${formatBytes(details?.totalReadBytes)} · written ${formatBytes(details?.totalWrittenBytes)}`
              : "Lifetime totals unavailable for this process"
          }
        />
        <ConnectionsCard connections={details?.connections} loading={loading} />

        {children.length > 0 && (
          <Card className="xl:col-span-2">
            <CardHeader title={`Child processes (${children.length})`} />
            <ul className="flex flex-wrap gap-2 px-4 pb-4 pt-3">
              {children.map((c) => (
                <li key={c.pid}>
                  <button
                    type="button"
                    onClick={() => openProcess(c.pid)}
                    className="rounded-md border border-border px-2 py-1 hover:bg-surface-2"
                  >
                    {c.name} <span className="num text-fg-subtle">{c.pid}</span>
                  </button>
                </li>
              ))}
            </ul>
          </Card>
        )}
      </div>
    </>
  );
}

function PathField({ label, value }: { label: string; value: string | null }) {
  return (
    <div className="grid grid-cols-[140px_1fr] gap-3">
      <span className="text-fg-muted">{label}</span>
      {value ? (
        <code className="selectable break-all font-mono text-[12px]">{value}</code>
      ) : (
        <Unavailable />
      )}
    </div>
  );
}

function MetricCard({
  title,
  series,
  history,
  pick,
  yMax,
  format,
  label,
  footer,
  nowMs,
}: {
  title: string;
  series: readonly LineSeries[];
  history: ProcessPoint[];
  pick: (p: ProcessPoint) => number[];
  yMax?: number;
  format: (v: number) => string;
  label: string;
  footer?: string;
  /** Latest tick time; anchors the x axis before history exists. */
  nowMs: number;
}) {
  const { data, xRange } = useMemo(() => {
    const xs = history.map((p) => p.t / 1000);
    const columns = series.map((_, i) => history.map((p) => pick(p)[i] ?? 0));
    const end = history.length > 0 ? (xs[xs.length - 1] as number) : nowMs / 1000;
    return {
      data: [xs, ...columns] as uPlot.AlignedData,
      xRange: [end - HISTORY_SECONDS, end] as const,
    };
  }, [history, series, pick, nowMs]);
  return (
    <Card>
      <CardHeader title={title} />
      <div className="px-2 pb-3 pt-2">
        <LineChart
          series={series}
          data={data}
          xRange={xRange}
          height={150}
          yMax={yMax}
          formatValue={format}
          ariaLabel={label}
        />
        {footer && <p className="num px-2 pt-2 text-fg-muted">{footer}</p>}
      </div>
    </Card>
  );
}

const STATE_LABEL: Record<string, string> = {
  listen: "Listening",
  established: "Established",
  timeWait: "Time wait",
  closeWait: "Close wait",
  synSent: "SYN sent",
  synReceived: "SYN received",
  finWait1: "FIN wait 1",
  finWait2: "FIN wait 2",
  closing: "Closing",
  lastAck: "Last ACK",
  closed: "Closed",
  unknown: "Unknown",
};

function ConnectionsCard({
  connections,
  loading,
}: {
  connections: Connection[] | null | undefined;
  loading: boolean;
}) {
  return (
    <Card>
      <CardHeader
        title={connections ? `Network connections (${connections.length})` : "Network connections"}
      />
      <div className="max-h-72 overflow-auto px-4 pb-4 pt-2">
        {connections === undefined ? (
          <p className="text-fg-muted">{loading ? "Loading…" : NOT_AVAILABLE}</p>
        ) : connections === null ? (
          <p className="text-fg-muted">
            Unavailable on this platform or for this process. Listing another account's sockets may
            require administrator/root rights.
          </p>
        ) : connections.length === 0 ? (
          <p className="text-fg-muted">No open TCP or UDP sockets.</p>
        ) : (
          <table className="w-full text-left text-[12px]">
            <thead className="text-fg-muted">
              <tr>
                <th className="py-1 pr-3 font-medium">Protocol</th>
                <th className="py-1 pr-3 font-medium">Local</th>
                <th className="py-1 pr-3 font-medium">Remote</th>
                <th className="py-1 font-medium">State</th>
              </tr>
            </thead>
            <tbody className="num selectable font-mono">
              {connections.map((c, i) => (
                <tr key={i} className="border-t border-border/60">
                  <td className="py-1 pr-3 uppercase">{c.protocol}</td>
                  <td className="py-1 pr-3">{endpoint(c.localAddress, c.localPort)}</td>
                  <td className="py-1 pr-3">
                    {c.remoteAddress != null ? endpoint(c.remoteAddress, c.remotePort) : "—"}
                  </td>
                  <td className="py-1 font-sans">{c.state ? STATE_LABEL[c.state] : "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </Card>
  );
}

function endpoint(address: string, port: number | null): string {
  const host = address.includes(":") ? `[${address}]` : address;
  return port == null ? host : `${host}:${port}`;
}
