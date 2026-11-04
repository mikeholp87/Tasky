import {
  Activity,
  ArrowLeft,
  Cpu,
  FolderOpen,
  HardDrive,
  MemoryStick,
  OctagonX,
  Pause,
  Play,
  RefreshCw,
  Search,
  Square,
} from "lucide-react";
import { requestAction, revealExecutable } from "@/features/processes/actions";
import { useMemo, useState, type ReactNode } from "react";
import type uPlot from "uplot";
import { ConnectionNotice } from "@/components/ConnectionNotice";
import { EmptyState } from "@/components/EmptyState";
import { PageHeader } from "@/components/PageHeader";
import { LineChart, type LineSeries } from "@/components/charts/LineChart";
import { percentAxis, rateAxis, RATE_MIN_Y } from "@/components/charts/series";
import { Card, CardHeader } from "@/components/ui/Card";
import { KeyValueList, Unavailable } from "@/components/ui/KeyValueList";
import { Meter } from "@/components/ui/Meter";
import { childrenOf } from "@/features/processes/tree";
import { STATUS_LABEL } from "@/features/processes/cells";
import { cn } from "@/lib/cn";
import { useMetricsStore } from "@/stores/metricsStore";
import { useUiStore } from "@/stores/uiStore";
import type { Connection, ProcessPoint, ProcessRow } from "@/types";
import {
  NOT_AVAILABLE,
  formatBytes,
  formatBytesPerSecond,
  formatCount,
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

type DirectorySort = "cpu" | "memory" | "io" | "pid";

const SORT_PILLS: readonly { id: DirectorySort; label: string }[] = [
  { id: "cpu", label: "Top CPU" },
  { id: "memory", label: "Top Memory" },
  { id: "io", label: "Top Disk I/O" },
  { id: "pid", label: "PID" },
];

export function ProcessDetailsPage() {
  const pid = useUiStore((s) => s.selectedPid);

  if (pid == null) {
    return <ProcessInspectorDirectory />;
  }
  // Keyed so all local state resets when switching processes.
  return <ProcessDetailsView key={pid} pid={pid} />;
}

function ProcessInspectorDirectory() {
  const processes = useMetricsStore((s) => s.tick?.processes);
  const totalMemory = useMetricsStore((s) => s.tick?.memory.totalBytes ?? 0);
  const navigate = useUiStore((s) => s.navigate);
  const openProcess = useUiStore((s) => s.openProcess);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<DirectorySort>("cpu");

  const highlights = useMemo(() => {
    if (!processes || processes.length === 0) {
      return { topCpu: null, topMem: null, topIo: null };
    }
    let topCpu = processes[0] ?? null;
    let topMem = processes[0] ?? null;
    let topIo = processes[0] ?? null;
    for (const p of processes) {
      if (topCpu && !p.restricted && p.cpuPercent > (topCpu.restricted ? -1 : topCpu.cpuPercent)) {
        topCpu = p;
      }
      if (topMem && p.memoryBytes > topMem.memoryBytes) {
        topMem = p;
      }
      const pIo = p.restricted ? -1 : p.diskReadPerSec + p.diskWritePerSec;
      const bestIo = topIo ? (topIo.restricted ? -1 : topIo.diskReadPerSec + topIo.diskWritePerSec) : -1;
      if (topIo && pIo > bestIo) {
        topIo = p;
      }
    }
    return { topCpu, topMem, topIo };
  }, [processes]);

  const filtered = useMemo(() => {
    if (!processes) return [];
    const q = query.trim().toLowerCase();
    const matched = q
      ? processes.filter(
          (p) =>
            p.name.toLowerCase().includes(q) ||
            String(p.pid).includes(q) ||
            (p.user && p.user.toLowerCase().includes(q)) ||
            (p.cmd && p.cmd.toLowerCase().includes(q)),
        )
      : [...processes];

    matched.sort((a, b) => {
      switch (sort) {
        case "cpu": {
          const aCpu = a.restricted ? -1 : a.cpuPercent;
          const bCpu = b.restricted ? -1 : b.cpuPercent;
          return bCpu - aCpu || b.memoryBytes - a.memoryBytes;
        }
        case "memory":
          return b.memoryBytes - a.memoryBytes || b.cpuPercent - a.cpuPercent;
        case "io": {
          const aIo = a.restricted ? -1 : a.diskReadPerSec + a.diskWritePerSec;
          const bIo = b.restricted ? -1 : b.diskReadPerSec + b.diskWritePerSec;
          return bIo - aIo || b.cpuPercent - a.cpuPercent;
        }
        case "pid":
          return a.pid - b.pid;
      }
    });

    return matched.slice(0, 25);
  }, [processes, query, sort]);

  return (
    <>
      <PageHeader
        title="Process Details"
        description="Inspect live per-process telemetry, resource history, threads, sockets, and hierarchy."
        actions={
          <button
            type="button"
            onClick={() => navigate("processes")}
            className="flex h-8 items-center gap-1.5 rounded-md border border-border px-2.5 text-fg-muted hover:text-fg"
          >
            <ArrowLeft size={14} aria-hidden /> Full Process Table
          </button>
        }
      />

      {!processes ? (
        <ConnectionNotice />
      ) : (
        <div className="space-y-4">
          <div className="grid gap-4 md:grid-cols-3">
            <HighlightProcessCard
              title="Highest CPU Process"
              icon={<Cpu size={14} aria-hidden />}
              process={highlights.topCpu}
              primaryMetric={
                highlights.topCpu
                  ? highlights.topCpu.restricted
                    ? NOT_AVAILABLE
                    : formatPercent(highlights.topCpu.cpuPercent)
                  : NOT_AVAILABLE
              }
              secondaryMetric={
                highlights.topCpu
                  ? `Memory ${formatBytes(highlights.topCpu.memoryBytes)}`
                  : "No active process"
              }
              percent={
                highlights.topCpu && !highlights.topCpu.restricted
                  ? highlights.topCpu.cpuPercent
                  : 0
              }
              onInspect={openProcess}
            />
            <HighlightProcessCard
              title="Highest Memory Process"
              icon={<MemoryStick size={14} aria-hidden />}
              process={highlights.topMem}
              primaryMetric={
                highlights.topMem ? formatBytes(highlights.topMem.memoryBytes) : NOT_AVAILABLE
              }
              secondaryMetric={
                highlights.topMem
                  ? `${
                      totalMemory > 0
                        ? `${formatPercent((highlights.topMem.memoryBytes / totalMemory) * 100)} of RAM`
                        : "RAM"
                    } · CPU ${
                      highlights.topMem.restricted
                        ? NOT_AVAILABLE
                        : formatPercent(highlights.topMem.cpuPercent)
                    }`
                  : "No active process"
              }
              percent={
                highlights.topMem && totalMemory > 0
                  ? (highlights.topMem.memoryBytes / totalMemory) * 100
                  : 0
              }
              onInspect={openProcess}
            />
            <HighlightProcessCard
              title="Highest Disk I/O Process"
              icon={<HardDrive size={14} aria-hidden />}
              process={highlights.topIo}
              primaryMetric={
                highlights.topIo
                  ? highlights.topIo.restricted
                    ? NOT_AVAILABLE
                    : formatBytesPerSecond(
                        highlights.topIo.diskReadPerSec + highlights.topIo.diskWritePerSec,
                      )
                  : NOT_AVAILABLE
              }
              secondaryMetric={
                highlights.topIo && !highlights.topIo.restricted
                  ? `↓ ${formatBytesPerSecond(highlights.topIo.diskReadPerSec)} · ↑ ${formatBytesPerSecond(highlights.topIo.diskWritePerSec)}`
                  : "No active disk I/O"
              }
              percent={
                highlights.topIo && !highlights.topIo.restricted
                  ? Math.min(
                      100,
                      ((highlights.topIo.diskReadPerSec + highlights.topIo.diskWritePerSec) /
                        (50 * 1024 * 1024)) *
                        100,
                    )
                  : 0
              }
              onInspect={openProcess}
            />
          </div>

          <Card>
            <CardHeader
              title={`Select a Process to Inspect (${formatCount(processes.length)} active)`}
              icon={<Activity size={14} aria-hidden />}
              action={
                <div className="flex flex-wrap items-center gap-1">
                  {SORT_PILLS.map((pill) => (
                    <button
                      key={pill.id}
                      type="button"
                      onClick={() => setSort(pill.id)}
                      className={cn(
                        "rounded-md px-2.5 py-1 text-[11px] font-medium transition-colors",
                        sort === pill.id
                          ? "bg-accent text-accent-fg"
                          : "bg-surface-2 text-fg-muted hover:text-fg",
                      )}
                    >
                      {pill.label}
                    </button>
                  ))}
                </div>
              }
            />
            <div className="space-y-3 px-4 pb-4 pt-3">
              <div className="relative">
                <Search
                  size={14}
                  aria-hidden
                  className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-fg-subtle"
                />
                <input
                  type="search"
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  placeholder="Filter by process name, PID, user, or command…"
                  aria-label="Filter processes to inspect"
                  className="h-9 w-full rounded-md border border-border bg-surface pl-9 pr-3 outline-none focus:border-accent"
                />
              </div>

              <div className="overflow-x-auto">
                <table className="w-full text-left text-[12px]">
                  <thead className="border-b border-border text-fg-muted">
                    <tr>
                      <th className="py-2 pr-3 font-medium">Name</th>
                      <th className="py-2 pr-3 text-right font-medium">PID</th>
                      <th className="py-2 pr-3 font-medium">User</th>
                      <th className="py-2 pr-3 font-medium">Status</th>
                      <th className="py-2 pr-3 text-right font-medium">Threads</th>
                      <th className="w-36 py-2 pr-3 font-medium">CPU %</th>
                      <th className="w-40 py-2 pr-3 font-medium">Memory</th>
                      <th className="py-2 pr-3 text-right font-medium">Disk I/O</th>
                      <th className="py-2 pr-3 font-medium">Command Line</th>
                      <th className="py-2 text-right font-medium">Action</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-border/60">
                    {filtered.length === 0 ? (
                      <tr>
                        <td colSpan={10} className="py-6 text-center text-fg-muted">
                          No processes match your filter.
                        </td>
                      </tr>
                    ) : (
                      filtered.map((p) => {
                        const memPct = totalMemory > 0 ? (p.memoryBytes / totalMemory) * 100 : 0;
                        const totalIo = p.diskReadPerSec + p.diskWritePerSec;
                        return (
                          <tr
                            key={p.pid}
                            onClick={() => openProcess(p.pid)}
                            className="cursor-pointer transition-colors hover:bg-surface-2"
                          >
                            <td className="max-w-[180px] truncate py-2 pr-3 font-medium text-fg">
                              {p.name}
                            </td>
                            <td className="num py-2 pr-3 text-right text-fg-muted">{p.pid}</td>
                            <td className="max-w-[110px] truncate py-2 pr-3 text-fg-muted">
                              {p.user ?? NOT_AVAILABLE}
                            </td>
                            <td className="py-2 pr-3 text-fg-muted">{STATUS_LABEL[p.status]}</td>
                            <td className="num py-2 pr-3 text-right text-fg-muted">
                              {p.threads ?? NOT_AVAILABLE}
                            </td>
                            <td className="py-2 pr-3">
                              <div className="space-y-1">
                                <div className="num flex justify-between text-[11px]">
                                  <span>
                                    {p.restricted ? NOT_AVAILABLE : formatPercent(p.cpuPercent)}
                                  </span>
                                </div>
                                <Meter
                                  percent={p.restricted ? 0 : p.cpuPercent}
                                  label={`${p.name} CPU usage`}
                                  size="sm"
                                />
                              </div>
                            </td>
                            <td className="py-2 pr-3">
                              <div className="space-y-1">
                                <div className="num flex justify-between text-[11px]">
                                  <span>{formatBytes(p.memoryBytes)}</span>
                                  <span className="text-fg-subtle">{formatPercent(memPct, 0)}</span>
                                </div>
                                <Meter
                                  percent={memPct}
                                  label={`${p.name} memory usage`}
                                  size="sm"
                                />
                              </div>
                            </td>
                            <td className="num py-2 pr-3 text-right text-fg-muted">
                              {p.restricted
                                ? NOT_AVAILABLE
                                : totalIo > 0
                                  ? formatBytesPerSecond(totalIo)
                                  : "0 B/s"}
                            </td>
                            <td className="max-w-[240px] truncate py-2 pr-3 font-mono text-[11px] text-fg-subtle">
                              {p.cmd ?? p.exe ?? NOT_AVAILABLE}
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
                      })
                    )}
                  </tbody>
                </table>
              </div>
            </div>
          </Card>
        </div>
      )}
    </>
  );
}

function HighlightProcessCard({
  title,
  icon,
  process: p,
  primaryMetric,
  secondaryMetric,
  percent,
  onInspect,
}: {
  title: string;
  icon: ReactNode;
  process: ProcessRow | null;
  primaryMetric: string;
  secondaryMetric: string;
  percent: number;
  onInspect: (pid: number) => void;
}) {
  return (
    <Card>
      <CardHeader
        title={title}
        icon={icon}
        action={
          p && (
            <button
              type="button"
              onClick={() => onInspect(p.pid)}
              className="rounded-md bg-accent-soft px-2 py-0.5 text-[11px] font-medium text-fg hover:bg-accent hover:text-accent-fg"
            >
              Inspect →
            </button>
          )
        }
      />
      <div className="space-y-2.5 px-4 pb-4 pt-3">
        <div className="flex items-baseline justify-between gap-2">
          <div className="min-w-0">
            <div className="truncate text-base font-semibold text-fg">
              {p ? p.name : NOT_AVAILABLE}
            </div>
            <div className="num text-[12px] text-fg-muted">
              {p ? `PID ${p.pid}${p.user ? ` · ${p.user}` : ""}` : "—"}
            </div>
          </div>
          <span className="num shrink-0 text-xl font-semibold text-fg">{primaryMetric}</span>
        </div>
        <Meter percent={percent} label={title} size="sm" />
        <div className="num truncate text-[12px] text-fg-muted">{secondaryMetric}</div>
      </div>
    </Card>
  );
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

  const topSwitchableProcesses = useMemo(() => {
    if (!rows) return [];
    const sorted = [...rows]
      .sort((a, b) => (b.restricted ? -1 : b.cpuPercent) - (a.restricted ? -1 : a.cpuPercent) || b.memoryBytes - a.memoryBytes)
      .slice(0, 20);
    if (row && !sorted.some((p) => p.pid === row.pid)) {
      sorted.unshift(row);
    }
    return sorted;
  }, [rows, row]);

  if (!row) {
    return (
      <>
        <PageHeader
          title={`Process ${pid}`}
          actions={
            <button
              type="button"
              onClick={() => useUiStore.setState({ selectedPid: null })}
              className="flex h-8 items-center gap-1.5 rounded-md border border-border px-2.5 text-fg-muted hover:text-fg"
            >
              Change process…
            </button>
          }
        />
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
            {topSwitchableProcesses.length > 0 && (
              <select
                aria-label="Switch active process"
                value={row.pid}
                onChange={(e) => openProcess(Number(e.target.value))}
                className="h-8 max-w-[210px] rounded-md border border-border bg-surface px-2 text-[12px] text-fg outline-none focus:border-accent"
              >
                {topSwitchableProcesses.map((p) => (
                  <option key={p.pid} value={p.pid}>
                    {p.name} (PID {p.pid} · {p.restricted ? NOT_AVAILABLE : formatPercent(p.cpuPercent, 0)})
                  </option>
                ))}
              </select>
            )}
            <button
              type="button"
              onClick={() => useUiStore.setState({ selectedPid: null })}
              className="flex h-8 items-center gap-1.5 rounded-md border border-border px-2.5 text-fg-muted hover:text-fg"
            >
              Change process…
            </button>
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
            {live && <ActionButtons row={live} />}
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
          minYMax={RATE_MIN_Y}
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
  minYMax,
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
  minYMax?: number;
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
          minYMax={minYMax}
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

const actionBtn =
  "flex h-8 items-center gap-1.5 rounded-md border border-border px-2.5 text-fg-muted hover:text-fg disabled:opacity-50";

function ActionButtons({ row }: { row: ProcessRow }) {
  const suspended = row.status === "stopped";
  return (
    <>
      <button
        type="button"
        className={actionBtn}
        disabled={!row.exe}
        onClick={() => void revealExecutable(row)}
      >
        <FolderOpen size={14} aria-hidden /> Open location
      </button>
      <button
        type="button"
        className={actionBtn}
        onClick={() => void requestAction(row, { type: suspended ? "resume" : "suspend" })}
      >
        {suspended ? <Play size={14} aria-hidden /> : <Pause size={14} aria-hidden />}
        {suspended ? "Resume" : "Suspend"}
      </button>
      <button
        type="button"
        className={actionBtn}
        onClick={() => void requestAction(row, { type: "terminate" })}
      >
        <Square size={13} aria-hidden /> End process
      </button>
      <button
        type="button"
        className={`${actionBtn} text-crit hover:text-crit`}
        onClick={() => void requestAction(row, { type: "forceKill" })}
      >
        <OctagonX size={14} aria-hidden /> Force end
      </button>
    </>
  );
}
