import { Fragment, useMemo, useState } from "react";
import { ChevronDown, ChevronRight, Search } from "lucide-react";
import { Card, CardHeader } from "@/components/ui/Card";
import { useMetricsStore } from "@/stores/metricsStore";
import { useUiStore } from "@/stores/uiStore";
import { formatBytes, formatPercent } from "@/utils/format";
import { levelLabel, usageLevel } from "@/utils/levels";
import { commitPercent, groupMemory, type MemoryGroup } from "./diagnostics";

export function MemoryDiagnostics() {
  const tick = useMetricsStore((s) => s.tick);
  const platform = useMetricsStore((s) => s.staticInfo?.platform);
  const baseline = useMetricsStore((s) => s.memoryBaseline);
  const paused = useMetricsStore((s) => s.monitoring.paused);
  const status = useMetricsStore((s) => s.status);
  const [search, setSearch] = useState("");
  const [sort, setSort] = useState("memory");
  const [limit, setLimit] = useState(10);
  const groups = useMemo(
    () => groupMemory(tick?.processes ?? [], platform, baseline),
    [tick?.processes, platform, baseline],
  );
  const visible = useMemo(() => {
    const query = search.trim().toLowerCase();
    const filtered = groups.filter(
      (g) =>
        g.name.toLowerCase().includes(query) ||
        g.exe?.toLowerCase().includes(query) ||
        g.processes.some((p) => String(p.pid).includes(query)),
    );
    return sort === "growth"
      ? filtered.sort(
          (a, b) => (b.changeBytes ?? 0) - (a.changeBytes ?? 0) || b.memoryBytes - a.memoryBytes,
        )
      : filtered;
  }, [groups, search, sort]);
  if (!tick) return null;

  const memory = tick.memory;
  const level = usageLevel(memory.usedPercent);
  const leader = groups[0];
  const reportedBytes = groups.reduce((sum, g) => sum + g.memoryBytes, 0);
  const commit = commitPercent(memory);
  const seconds = baseline ? Math.round((tick.timestampMs - baseline.timestampMs) / 1000) : null;
  const largePool =
    memory.totalBytes > 0 &&
    memory.kernelNonpagedBytes != null &&
    memory.kernelNonpagedBytes >= memory.totalBytes * 0.1;

  return (
    <div className="mb-4 space-y-4">
      <Card>
        <CardHeader
          title="What's using your memory?"
          action={
            <span
              className={
                level === "critical"
                  ? "text-crit"
                  : level === "warning"
                    ? "text-warn"
                    : "text-fg-muted"
              }
            >
              {memory.totalBytes > 0
                ? (levelLabel(level, "memory") ?? "Memory usage is normal")
                : "Memory capacity unavailable"}
            </span>
          }
        />
        <div className="space-y-3 px-4 pb-4 pt-3">
          {(paused || status !== "live") && (
            <p role="status" className="rounded bg-warn-soft px-3 py-2 text-warn">
              {paused ? "Monitoring is paused." : "Live monitoring is interrupted."} Showing the
              last collected sample.
            </p>
          )}
          <p>
            <strong className="num">{formatPercent(memory.usedPercent)}</strong> of RAM is in use.{" "}
            <strong className="num">{formatBytes(memory.availableBytes)}</strong> is available for
            programs.
            {level !== "normal" &&
              " With little headroom, opening more apps may slow the computer down."}
          </p>
          <ul className="list-disc space-y-1.5 pl-5 text-fg-muted">
            {leader && leader.memoryBytes > 0 ? (
              <li>
                <strong className="text-fg">{leader.name}</strong> is the largest reported group:{" "}
                {formatBytes(leader.memoryBytes)} across {leader.processes.length}{" "}
                {leader.processes.length === 1 ? "process" : "processes"}. Expand it below to
                inspect individual processes.
              </li>
            ) : (
              <li>
                No process memory was reported in this sample. Overall RAM usage alone cannot
                identify the cause.
              </li>
            )}
            {largePool && (
              <li>
                The nonpaged kernel pool uses {formatBytes(memory.kernelNonpagedBytes)} of RAM. If
                this keeps growing while apps are closed, investigate drivers or kernel services;
                this sample alone cannot identify a driver leak.
              </li>
            )}
            {commit != null && commit >= 90 && (
              <li>
                Committed memory is at {formatPercent(commit)} of its limit. Programs may fail to
                allocate memory. Save work and close apps you no longer need; on Windows, check that
                the page file is system managed.
              </li>
            )}
            {groups.some((g) => /^(vmmem(?:wsl)?|vmwp)(\.exe)?$/i.test(g.name)) && (
              <li>
                Virtual machine memory is present. Check running WSL distributions, Docker
                containers, and virtual machines for workloads you no longer need.
              </li>
            )}
            <li>
              Sort by largest increase to find growing groups. Growth can be normal activity or new
              processes; it is not proof of a memory leak.
            </li>
          </ul>
        </div>
      </Card>

      <Card>
        <CardHeader title="Memory by app / executable" />
        <div className="space-y-3 px-4 pb-4 pt-3">
          <div className="flex flex-wrap items-center gap-3">
            <label className="flex min-w-0 flex-1 items-center gap-2 rounded border border-border bg-bg px-2 py-1.5">
              <Search size={14} aria-hidden className="shrink-0 text-fg-muted" />
              <input
                type="search"
                aria-label="Search memory consumers"
                placeholder="Search name, executable path, or PID"
                value={search}
                onChange={(event) => {
                  setSearch(event.target.value);
                  setLimit(10);
                }}
                className="w-full min-w-0 bg-transparent outline-none"
              />
            </label>
            <label className="flex items-center gap-2 text-fg-muted">
              Sort by
              <select
                aria-label="Sort memory consumers"
                value={sort}
                onChange={(event) => {
                  setSort(event.target.value);
                  setLimit(10);
                }}
                className="rounded border border-border bg-surface px-2 py-1.5 text-fg"
              >
                <option value="memory">Most RAM</option>
                <option value="growth">Largest increase</option>
              </select>
            </label>
          </div>
          <p className="text-[12px] text-fg-muted">
            {seconds != null
              ? `Change over the last ${seconds}s.`
              : "Collecting growth baseline (at least 5 seconds of monitoring needed)."}{" "}
            Includes process starts and exits. Groups use executable paths when available, otherwise
            names.
          </p>
          <div className="overflow-x-auto">
            <table className="w-full text-left">
              <caption className="sr-only">
                Memory consumers with expandable process details
              </caption>
              <thead className="border-b border-border text-[12px] text-fg-muted">
                <tr>
                  <th scope="col" className="py-2 pr-3 font-medium">
                    App / executable
                  </th>
                  <th scope="col" className="px-3 py-2 text-right font-medium">
                    Processes
                  </th>
                  <th scope="col" className="px-3 py-2 text-right font-medium">
                    RAM (working set)
                  </th>
                  <th scope="col" className="py-2 pl-3 text-right font-medium">
                    Change
                  </th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border">
                {visible.slice(0, limit).map((group) => (
                  <GroupRow key={group.key} group={group} totalBytes={memory.totalBytes} />
                ))}
                {visible.length === 0 && (
                  <tr>
                    <td colSpan={4} className="py-5 text-center text-fg-muted">
                      {groups.length === 0
                        ? "No process data available."
                        : "No matching memory consumers."}
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
          <div className="flex flex-wrap items-center justify-between gap-2 text-[12px] text-fg-muted">
            <span>
              {Math.min(limit, visible.length)} of {visible.length} groups ·{" "}
              {formatBytes(reportedBytes)} reported working sets
            </span>
            {visible.length > limit && (
              <button
                type="button"
                onClick={() => setLimit((value) => value + 10)}
                className="rounded px-2 py-1 text-accent hover:bg-accent-soft"
              >
                Show 10 more groups
              </button>
            )}
          </div>
          <details className="rounded bg-surface-2 px-3 py-2 text-[12px] text-fg-muted">
            <summary className="cursor-pointer font-medium">
              Why don't these numbers add up to total RAM used?
            </summary>
            <p className="mt-2">
              Working sets include shared pages counted in multiple processes. Overall RAM also
              includes operating system memory, drivers, and allocations outside process working
              sets. Some process readings may be unavailable. Subtracting these totals cannot
              reliably measure a hidden app or driver.
            </p>
            <p className="mt-2">
              Available RAM already includes reclaimable memory. Cached memory overlaps other
              figures and should not be added to used RAM. Commit is memory promised to programs,
              backed by RAM and the page file; it is not extra physical RAM usage.
            </p>
            <p className="mt-2">
              Review apps you recognize and save work before closing them. Keep system processes and
              services running.
            </p>
          </details>
        </div>
      </Card>
    </div>
  );
}

function GroupRow({ group, totalBytes }: { group: MemoryGroup; totalBytes: number }) {
  const [expanded, setExpanded] = useState(false);
  const [limit, setLimit] = useState(10);
  const openProcess = useUiStore((s) => s.openProcess);
  return (
    <Fragment>
      <tr>
        <th scope="row" className="w-full min-w-48 max-w-0 py-2 pr-3 font-normal">
          <button
            type="button"
            aria-label={`Inspect ${group.name} processes`}
            aria-expanded={expanded}
            onClick={() => setExpanded((value) => !value)}
            className="flex w-full items-center gap-2 rounded text-left hover:text-accent"
          >
            {expanded ? (
              <ChevronDown size={14} className="shrink-0" aria-hidden />
            ) : (
              <ChevronRight size={14} className="shrink-0" aria-hidden />
            )}
            <span className="min-w-0">
              <span className="block truncate font-medium">{group.name}</span>
              <span
                className="block truncate text-[11px] text-fg-muted"
                title={group.exe ?? undefined}
              >
                {group.exe ?? "Executable path unavailable; grouped by name"}
              </span>
            </span>
          </button>
        </th>
        <td className="num px-3 py-2 text-right">{group.processes.length}</td>
        <td className="num whitespace-nowrap px-3 py-2 text-right">
          {formatBytes(group.memoryBytes)}
          <span className="block text-[11px] text-fg-muted">
            {totalBytes > 0
              ? `${formatPercent((group.memoryBytes / totalBytes) * 100)} of RAM`
              : "Capacity unavailable"}
          </span>
        </td>
        <td
          className={`num whitespace-nowrap py-2 pl-3 text-right ${group.changeBytes != null && group.changeBytes > 0 ? "text-warn" : "text-fg-muted"}`}
        >
          {group.changeBytes == null
            ? "—"
            : `${group.changeBytes > 0 ? "+" : group.changeBytes < 0 ? "−" : ""}${formatBytes(Math.abs(group.changeBytes))}`}
        </td>
      </tr>
      {expanded && (
        <tr>
          <td colSpan={4} className="bg-surface-2 px-4 py-2">
            <ul aria-label={`${group.name} processes`} className="divide-y divide-border">
              {group.processes.slice(0, limit).map((process) => (
                <li
                  key={`${process.pid}:${process.startTime}`}
                  className="flex flex-wrap items-center justify-between gap-2 py-2"
                >
                  <span className="num">
                    PID {process.pid}{" "}
                    <span className="text-fg-muted">
                      · {process.user ?? "User unavailable"} · {formatBytes(process.memoryBytes)}
                      {process.restricted ? " · Limited access" : ""}
                    </span>
                  </span>
                  <button
                    type="button"
                    onClick={() => openProcess(process.pid)}
                    className="rounded px-2 py-1 text-accent hover:bg-accent-soft"
                  >
                    Inspect PID {process.pid}
                  </button>
                </li>
              ))}
            </ul>
            {group.processes.length > limit && (
              <button
                type="button"
                onClick={() => setLimit((value) => value + 10)}
                className="rounded px-2 py-1 text-accent hover:bg-accent-soft"
              >
                Show 10 more processes
              </button>
            )}
          </td>
        </tr>
      )}
    </Fragment>
  );
}
