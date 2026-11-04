import type { ReactNode } from "react";
import type { ProcessRow, ProcessState } from "@/types";
import {
  NOT_AVAILABLE,
  formatBytes,
  formatBytesPerSecond,
  formatDuration,
  formatPercent,
  formatTimestamp,
} from "@/utils/format";
import { type ColumnId, type ProcessContext, memoryOf, runtimeSecs } from "./logic";

export const STATUS_LABEL: Record<ProcessState, string> = {
  running: "Running",
  sleeping: "Sleeping",
  idle: "Idle",
  diskSleep: "Disk sleep",
  stopped: "Suspended",
  tracing: "Traced",
  zombie: "Zombie",
  dead: "Dead",
  unknown: "Unknown",
};

/** Values the OS withholds for restricted processes. */
const RESTRICTED_COLUMNS: ReadonlySet<ColumnId> = new Set(["cpu", "diskRead", "diskWrite"]);

const RESTRICTED_HINT =
  "Restricted: the operating system does not share this process's CPU and disk activity with a standard account.";

const rate = (v: number) => (v > 0 ? formatBytesPerSecond(v) : "0 B/s");
const opt = (v: number | null) => (v == null ? NOT_AVAILABLE : String(v));

/** Display content for a cell. Kept separate from layout for reuse in exports/tree. */
export function cellContent(column: ColumnId, row: ProcessRow, ctx: ProcessContext): ReactNode {
  if (row.restricted && RESTRICTED_COLUMNS.has(column)) return NOT_AVAILABLE;
  switch (column) {
    case "name":
      return row.name;
    case "pid":
      return row.pid;
    case "status":
      return STATUS_LABEL[row.status];
    case "cpu":
      return formatPercent(row.cpuPercent);
    case "memory":
      return formatBytes(memoryOf(row));
    case "memoryPercent":
      return ctx.totalMemoryBytes > 0
        ? formatPercent((memoryOf(row) / ctx.totalMemoryBytes) * 100)
        : NOT_AVAILABLE;
    case "diskRead":
      return rate(row.diskReadPerSec);
    case "diskWrite":
      return rate(row.diskWritePerSec);
    case "user":
      return row.user ?? NOT_AVAILABLE;
    case "startTime":
      return row.startTime > 0 ? formatTimestamp(row.startTime * 1000) : NOT_AVAILABLE;
    case "runtime": {
      const secs = runtimeSecs(row, ctx);
      return secs >= 0 ? formatDuration(secs) : NOT_AVAILABLE;
    }
    case "threads":
      return opt(row.threads);
    case "parentPid":
      return opt(row.parentPid);
    case "priority":
      return opt(row.priority);
    case "session":
      return opt(row.sessionId);
    case "exe":
      return row.exe ?? NOT_AVAILABLE;
    case "cmd":
      return row.cmd ?? NOT_AVAILABLE;
  }
}

/** Tooltip for cells whose value may need explanation. */
export function cellTitle(column: ColumnId, row: ProcessRow): string | undefined {
  if (row.restricted && RESTRICTED_COLUMNS.has(column)) return RESTRICTED_HINT;
  switch (column) {
    case "user":
      return row.user == null
        ? "Owner unavailable: the operating system restricts this information for processes of other accounts unless Task Manager runs with administrator/root rights."
        : undefined;
    case "exe":
      return row.exe ?? undefined;
    case "cmd":
      return row.cmd ?? undefined;
    case "name":
      return row.exe ?? row.name;
    default:
      return undefined;
  }
}
