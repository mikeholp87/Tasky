import { ChevronDown, ChevronRight } from "lucide-react";
import { useMemo, useState } from "react";
import { Card, CardHeader } from "@/components/ui/Card";
import { Meter } from "@/components/ui/Meter";
import { useMetricsStore } from "@/stores/metricsStore";
import { useProcessStore } from "@/stores/processStore";
import { useUiStore } from "@/stores/uiStore";
import { formatBytes, formatPercent } from "@/utils/format";
import { groupByApplication, type MemoryGroup } from "./groups";
import { useMemoryDetail } from "./useMemoryDetail";

const TOP_GROUPS = 10;
const TOP_PROCESSES = 8;

function openProcessesBySize() {
  useProcessStore.setState({
    sort: { column: "memory", direction: "desc" },
    search: "",
    preset: "all",
  });
  useUiStore.getState().navigate("processes");
}

/** Which applications use the memory: grouped, sorted, expandable to processes. */
export function TopMemoryCard() {
  useMemoryDetail();
  const processes = useMetricsStore((s) => s.tick?.processes);
  const totalBytes = useMetricsStore((s) => s.tick?.memory.totalBytes ?? 0);
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());
  const [showAll, setShowAll] = useState(false);

  const groups = useMemo(
    () => groupByApplication(processes ?? [], totalBytes),
    [processes, totalBytes],
  );
  const measured = useMemo(() => (processes ?? []).some((p) => p.pssBytes != null), [processes]);
  const estimated = useMemo(() => groups.reduce((n, g) => n + g.estimated, 0), [groups]);

  const top = groups[0];
  const shown = showAll ? groups : groups.slice(0, TOP_GROUPS);
  const toggle = (key: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });

  return (
    <Card>
      <CardHeader
        title="What is using memory"
        action={
          <button
            type="button"
            onClick={openProcessesBySize}
            className="rounded-md border border-border px-2.5 py-1 text-[0.8125rem] text-fg-muted hover:bg-surface-2 hover:text-fg"
          >
            Sort all processes by memory
          </button>
        }
      />
      <div className="px-4 pb-4 pt-2">
        {top ? (
          <p className="pb-3 text-fg-muted">
            <span className="font-medium text-fg">{top.name}</span> uses the most:{" "}
            <span className="num">{formatBytes(top.bytes)}</span> (
            <span className="num">{formatPercent(top.share)}</span> of memory)
            {top.processes.length > 1 && <> across {top.processes.length} processes</>}.
          </p>
        ) : (
          <p className="pb-3 text-fg-muted">Collecting process memory…</p>
        )}
        <ul className="divide-y divide-border">
          {shown.map((g) => (
            <GroupRow
              key={g.key}
              group={g}
              open={expanded.has(g.key)}
              onToggle={() => toggle(g.key)}
            />
          ))}
        </ul>
        {groups.length > TOP_GROUPS && (
          <button
            type="button"
            onClick={() => setShowAll((v) => !v)}
            className="mt-2 rounded-md px-2 py-1 text-[0.8125rem] text-fg-muted hover:bg-surface-2 hover:text-fg"
          >
            {showAll ? "Show fewer" : `Show all ${groups.length} applications`}
          </button>
        )}
        <p className="mt-3 text-[0.8125rem] text-fg-subtle">
          {measured
            ? "Figures split memory shared between processes (such as a browser's libraries) among the processes that share it, so an application's total is what it actually occupies. "
            : "Figures are resident memory, which counts memory shared between an application's processes more than once. "}
          {estimated > 0 &&
            `${estimated} ${estimated === 1 ? "process shows" : "processes show"} resident memory (marked ~) because the system does not share its detailed memory, usually another user's process, or it is not measured yet.`}
        </p>
      </div>
    </Card>
  );
}

function GroupRow({
  group: g,
  open,
  onToggle,
}: {
  group: MemoryGroup;
  open: boolean;
  onToggle: () => void;
}) {
  const openProcess = useUiStore((s) => s.openProcess);
  const Chevron = open ? ChevronDown : ChevronRight;
  const approx = g.estimated > 0 ? "~" : "";
  return (
    <li className="py-1.5">
      <button
        type="button"
        onClick={onToggle}
        aria-expanded={open}
        className="grid w-full grid-cols-[16px_1fr_auto_56px] items-center gap-x-2 rounded-md px-1 py-1 text-left hover:bg-surface-2"
      >
        <Chevron size="0.875rem" aria-hidden className="text-fg-subtle" />
        <span className="min-w-0 truncate">
          <span className="font-medium">{g.name}</span>
          {g.processes.length > 1 && (
            <span className="ml-2 text-fg-muted">{g.processes.length} processes</span>
          )}
        </span>
        <span className="num font-medium">
          {approx}
          {formatBytes(g.bytes)}
        </span>
        <span className="num text-right text-fg-muted">{formatPercent(g.share)}</span>
        <span />
        <span className="col-span-3">
          <Meter
            percent={g.share}
            label={`${g.name} share of memory`}
            size="sm"
            color="--res-mem"
          />
        </span>
      </button>
      {open && (
        <ul className="ml-6 mt-1 space-y-0.5 border-l border-border pl-3">
          {g.processes.slice(0, TOP_PROCESSES).map((p) => (
            <li key={p.pid} className="flex items-center gap-3 py-0.5">
              <span className="min-w-0 flex-1 truncate text-fg-muted">
                {p.name} <span className="num text-fg-subtle">PID {p.pid}</span>
              </span>
              <span className="num">
                {p.exact ? "" : "~"}
                {formatBytes(p.bytes)}
              </span>
              <button
                type="button"
                onClick={() => openProcess(p.pid)}
                className="rounded px-1.5 py-0.5 text-[0.8125rem] text-fg-muted hover:bg-surface-2 hover:text-fg"
                aria-label={`Details for ${p.name} (PID ${p.pid})`}
              >
                Details
              </button>
            </li>
          ))}
          {g.processes.length > TOP_PROCESSES && (
            <li className="py-0.5 text-[0.8125rem] text-fg-subtle">
              and {g.processes.length - TOP_PROCESSES} smaller processes
            </li>
          )}
        </ul>
      )}
    </li>
  );
}
