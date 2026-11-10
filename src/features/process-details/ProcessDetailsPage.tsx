import { ArrowLeft, RefreshCw } from "lucide-react";
import { Tabs } from "radix-ui";
import { useMemo, useState } from "react";
import { EmptyState } from "@/components/EmptyState";
import { PageHeader } from "@/components/PageHeader";
import type { LineSeries } from "@/components/charts/LineChart";
import { percentAxis, rateAxis, RATE_MIN_Y } from "@/components/charts/series";
import { outlineButton } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/Card";
import { KeyValueList, Unavailable } from "@/components/ui/KeyValueList";
import { childrenOf, isPlausibleParent } from "@/features/processes/tree";
import { STATUS_LABEL } from "@/features/processes/cells";
import { useActivityStore } from "@/stores/activityStore";
import { useMetricsStore } from "@/stores/metricsStore";
import { useUiStore } from "@/stores/uiStore";
import type { ProcessRow } from "@/types";
import {
  NOT_AVAILABLE,
  formatBytes,
  formatBytesPerSecond,
  formatDuration,
  formatPercent,
  formatTimestamp,
} from "@/utils/format";
import { ActionButtons } from "./ActionButtons";
import { ConnectionsCard } from "./ConnectionsCard";
import { DiagnoseTab } from "./DiagnoseTab";
import { MetricCard } from "./MetricCard";
import { MemoryMapTab } from "./inspect/MemoryMapTab";
import { ModulesTab } from "./inspect/ModulesTab";
import { OpenFilesTab } from "./inspect/OpenFilesTab";
import { ThreadsTab } from "./inspect/ThreadsTab";
import { memoryOf } from "@/features/processes/logic";
import { useMemoryDetail } from "@/features/memory/useMemoryDetail";
import { useProcessDetails } from "./useProcessDetails";

type Tab = "overview" | "network" | "files" | "modules" | "maps" | "threads" | "diagnose";
const TABS: readonly { id: Tab; label: string }[] = [
  { id: "overview", label: "Overview" },
  { id: "network", label: "Network" },
  { id: "files", label: "Open Files" },
  { id: "modules", label: "Modules" },
  { id: "maps", label: "Memory Map" },
  { id: "threads", label: "Threads" },
  { id: "diagnose", label: "Diagnose" },
];

const CPU_SERIES: readonly LineSeries[] = [{ label: "CPU", colorVar: "--res-cpu" }];
const MEM_SERIES: readonly LineSeries[] = [{ label: "Memory", colorVar: "--res-mem" }];
const IO_SERIES: readonly LineSeries[] = [
  { label: "Read", colorVar: "--res-disk" },
  { label: "Write", colorVar: "--res-disk-2", fill: false },
];
const bytesAxis = (v: number) => formatBytes(v);

export function ProcessDetailsPage() {
  const pid = useUiStore((s) => s.selectedPid);
  const startTime = useUiStore((s) => s.selectedStartTime);
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
  return <ProcessDetailsView key={`${pid}:${startTime}`} pid={pid} startTime={startTime} />;
}

function ProcessDetailsView({ pid, startTime }: { pid: number; startTime: number | null }) {
  const { live, details, history, loading, error, refresh } = useProcessDetails(pid, startTime);
  const navigate = useUiStore((s) => s.navigate);
  const openProcess = useUiStore((s) => s.openProcess);
  const rows = useMetricsStore((s) => s.tick?.processes);
  useMemoryDetail();
  const totalMemory = useMetricsStore((s) => s.tick?.memory.totalBytes ?? 0);
  const nowMs = useMetricsStore((s) => s.tick?.timestampMs ?? 0);
  const platform = useMetricsStore((s) => s.staticInfo?.platform);
  const [tab, setTab] = useState<Tab>("overview");

  const row: ProcessRow | null = live ?? details?.row ?? null;
  const exited = live == null && details != null;
  const children = useMemo(() => (rows && live ? childrenOf(rows, pid) : []), [rows, live, pid]);
  // The parent PID may since have been reused by an unrelated process.
  const parent = useMemo(() => {
    const self = live ?? details?.row;
    const candidate =
      self?.parentPid == null ? undefined : rows?.find((p) => p.pid === self.parentPid);
    return candidate && self && isPlausibleParent(candidate, self) ? candidate : undefined;
  }, [rows, live, details]);
  const allEvents = useActivityStore((s) => s.events);
  // Growth hints recorded for this process (not for a later one with the same PID).
  const hints = useMemo(
    () =>
      allEvents.filter(
        (e) =>
          e.pid === pid &&
          (e.kind === "memoryGrowth" || e.kind === "threadGrowth") &&
          (live == null || e.timestampMs >= live.startTime * 1000),
      ),
    [allEvents, pid, live],
  );

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

  const memPct = totalMemory > 0 ? (memoryOf(row) / totalMemory) * 100 : null;
  const runtime = row.startTime > 0 && nowMs > 0 ? nowMs / 1000 - row.startTime : null;

  return (
    <>
      <PageHeader
        title={row.name}
        description={`PID ${row.pid}${row.user ? ` · ${row.user}` : ""}`}
        actions={
          <>
            <button type="button" onClick={() => navigate("processes")} className={outlineButton}>
              <ArrowLeft size="0.875rem" aria-hidden /> Processes
            </button>
            <button type="button" onClick={refresh} disabled={loading} className={outlineButton}>
              <RefreshCw size="0.875rem" aria-hidden /> Refresh details
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

      <Tabs.Root value={tab} onValueChange={(v) => setTab(v as Tab)} activationMode="manual">
        <Tabs.List
          aria-label="Process information"
          className="mb-4 flex flex-wrap gap-1 border-b border-border"
        >
          {TABS.map((t) => (
            <Tabs.Trigger
              key={t.id}
              value={t.id}
              className="-mb-px border-b-2 border-transparent px-3 py-2 text-fg-muted hover:text-fg data-[state=active]:border-accent data-[state=active]:text-fg"
            >
              {t.label}
            </Tabs.Trigger>
          ))}
        </Tabs.List>
        <Tabs.Content value="network">
          <ConnectionsCard connections={details?.connections} loading={loading} />
        </Tabs.Content>
        {/* Inspection tabs mount only when opened, so nothing is read until then. */}
        <Tabs.Content value="files">
          {!exited && (
            <Card>
              <OpenFilesTab pid={pid} startTime={row.startTime} />
            </Card>
          )}
        </Tabs.Content>
        <Tabs.Content value="modules">
          {!exited && (
            <Card>
              <ModulesTab pid={pid} startTime={row.startTime} windows={platform === "windows"} />
            </Card>
          )}
        </Tabs.Content>
        <Tabs.Content value="maps">
          {!exited && (
            <Card>
              <MemoryMapTab pid={pid} startTime={row.startTime} />
            </Card>
          )}
        </Tabs.Content>
        <Tabs.Content value="diagnose">
          {!exited && (
            <Card>
              <DiagnoseTab
                row={row}
                history={history}
                connections={details?.connections}
                childCount={children.length}
                events={hints}
              />
            </Card>
          )}
        </Tabs.Content>
        <Tabs.Content value="threads">
          {!exited && (
            <Card>
              <ThreadsTab pid={pid} startTime={row.startTime} linux={platform === "linux"} />
            </Card>
          )}
        </Tabs.Content>
        <Tabs.Content value="overview">
          {hints.length > 0 && (
            <div
              role="note"
              className="mb-4 rounded-md border border-warn/40 bg-warn-soft px-4 py-2"
            >
              {hints.map((h) => (
                <p key={h.seq}>{h.detail}</p>
              ))}
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
                        ) : parent ? (
                          <button
                            type="button"
                            className="text-accent hover:underline"
                            onClick={() => openProcess(parent.pid, parent.startTime)}
                          >
                            {details?.parentName ?? "PID"} ({row.parentPid})
                          </button>
                        ) : (
                          `${details?.parentName ?? "PID"} (${row.parentPid})`
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
                      value:
                        row.startTime > 0 ? formatTimestamp(row.startTime * 1000) : NOT_AVAILABLE,
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
              title={`Memory · ${formatBytes(memoryOf(row))}${memPct != null ? ` (${formatPercent(memPct)})` : ""}`}
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

            {children.length > 0 && (
              <Card className="xl:col-span-2">
                <CardHeader title={`Child processes (${children.length})`} />
                <ul className="flex flex-wrap gap-2 px-4 pb-4 pt-3">
                  {children.map((c) => (
                    <li key={c.pid}>
                      <button
                        type="button"
                        onClick={() => openProcess(c.pid, c.startTime)}
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
        </Tabs.Content>
      </Tabs.Root>
    </>
  );
}

function PathField({ label, value }: { label: string; value: string | null }) {
  return (
    <div className="grid grid-cols-[140px_1fr] gap-3">
      <span className="text-fg-muted">{label}</span>
      {value ? (
        <code className="selectable break-all font-mono text-[0.8125rem]">{value}</code>
      ) : (
        <Unavailable />
      )}
    </div>
  );
}
