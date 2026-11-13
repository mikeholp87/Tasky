/**
 * Pure process-list logic: columns, search, filter presets and sorting.
 * No React here, so it is unit-tested and benchmarked directly.
 */
import type { ProcessRow } from "@/types";

export type ColumnId =
  | "name"
  | "pid"
  | "status"
  | "cpu"
  | "memory"
  | "memoryPercent"
  | "diskRead"
  | "diskWrite"
  | "user"
  | "startTime"
  | "runtime"
  | "threads"
  | "parentPid"
  | "priority"
  | "session"
  | "exe"
  | "cmd";

export interface ProcessContext {
  totalMemoryBytes: number;
  /** Current time in Unix seconds. */
  nowSecs: number;
  /** User owning the task manager itself; defines "user processes". */
  currentUser: string | null;
}

export interface ColumnDef {
  id: ColumnId;
  label: string;
  align: "left" | "right";
  width: number;
  minWidth: number;
  defaultVisible: boolean;
  /** Higher-is-more-interesting columns sort descending on first click. */
  descendingFirst: boolean;
  sortValue: (row: ProcessRow, ctx: ProcessContext) => number | string;
}

const num = (v: number | null | undefined) => v ?? -1;
const str = (v: string | null | undefined) => (v ?? "").toLowerCase();

export const COLUMNS: readonly ColumnDef[] = [
  {
    id: "name",
    label: "Name",
    align: "left",
    width: 240,
    minWidth: 120,
    defaultVisible: true,
    descendingFirst: false,
    sortValue: (r) => r.name.toLowerCase(),
  },
  {
    id: "pid",
    label: "PID",
    align: "right",
    width: 80,
    minWidth: 60,
    defaultVisible: true,
    descendingFirst: false,
    sortValue: (r) => r.pid,
  },
  {
    id: "status",
    label: "Status",
    align: "left",
    width: 96,
    minWidth: 70,
    defaultVisible: true,
    descendingFirst: false,
    sortValue: (r) => r.status,
  },
  {
    id: "cpu",
    label: "CPU",
    align: "right",
    width: 80,
    minWidth: 60,
    defaultVisible: true,
    descendingFirst: true,
    sortValue: (r) => (r.restricted ? -1 : r.cpuPercent),
  },
  {
    id: "memory",
    label: "Memory",
    align: "right",
    width: 96,
    minWidth: 70,
    defaultVisible: true,
    descendingFirst: true,
    sortValue: (r) => memoryOf(r),
  },
  {
    id: "memoryPercent",
    label: "Memory %",
    align: "right",
    width: 86,
    minWidth: 60,
    defaultVisible: false,
    descendingFirst: true,
    sortValue: (r) => memoryOf(r),
  },
  {
    id: "diskRead",
    label: "Disk read",
    align: "right",
    width: 100,
    minWidth: 70,
    defaultVisible: true,
    descendingFirst: true,
    sortValue: (r) => (r.restricted ? -1 : r.diskReadPerSec),
  },
  {
    id: "diskWrite",
    label: "Disk write",
    align: "right",
    width: 100,
    minWidth: 70,
    defaultVisible: true,
    descendingFirst: true,
    sortValue: (r) => (r.restricted ? -1 : r.diskWritePerSec),
  },
  {
    id: "user",
    label: "User",
    align: "left",
    width: 130,
    minWidth: 70,
    defaultVisible: true,
    descendingFirst: false,
    sortValue: (r) => str(r.user),
  },
  {
    id: "startTime",
    label: "Started",
    align: "right",
    width: 150,
    minWidth: 90,
    defaultVisible: false,
    descendingFirst: true,
    sortValue: (r) => r.startTime,
  },
  {
    id: "runtime",
    label: "Runtime",
    align: "right",
    width: 96,
    minWidth: 70,
    defaultVisible: true,
    descendingFirst: true,
    sortValue: (r, c) => runtimeSecs(r, c),
  },
  {
    id: "threads",
    label: "Threads",
    align: "right",
    width: 76,
    minWidth: 60,
    defaultVisible: true,
    descendingFirst: true,
    sortValue: (r) => num(r.threads),
  },
  {
    id: "parentPid",
    label: "Parent PID",
    align: "right",
    width: 90,
    minWidth: 60,
    defaultVisible: false,
    descendingFirst: false,
    sortValue: (r) => num(r.parentPid),
  },
  {
    id: "priority",
    label: "Priority",
    align: "right",
    width: 76,
    minWidth: 60,
    defaultVisible: false,
    descendingFirst: true,
    sortValue: (r) => num(r.priority),
  },
  {
    id: "session",
    label: "Session",
    align: "right",
    width: 76,
    minWidth: 60,
    defaultVisible: false,
    descendingFirst: false,
    sortValue: (r) => num(r.sessionId),
  },
  {
    id: "exe",
    label: "Executable",
    align: "left",
    width: 280,
    minWidth: 100,
    defaultVisible: false,
    descendingFirst: false,
    sortValue: (r) => str(r.exe),
  },
  {
    id: "cmd",
    label: "Command line",
    align: "left",
    width: 360,
    minWidth: 100,
    defaultVisible: false,
    descendingFirst: false,
    sortValue: (r) => str(r.cmd),
  },
];

export const COLUMN_BY_ID: Readonly<Record<ColumnId, ColumnDef>> = Object.fromEntries(
  COLUMNS.map((c) => [c.id, c]),
) as Record<ColumnId, ColumnDef>;

export const DEFAULT_VISIBLE_COLUMNS: readonly ColumnId[] = COLUMNS.filter(
  (c) => c.defaultVisible,
).map((c) => c.id);

export function runtimeSecs(row: ProcessRow, ctx: Pick<ProcessContext, "nowSecs">): number {
  return row.startTime > 0 ? Math.max(0, ctx.nowSecs - row.startTime) : -1;
}

// ---------------------------------------------------------------- presets

export type FilterPreset =
  | "all"
  | "highCpu"
  | "highMemory"
  | "user"
  | "system"
  | "suspended"
  | "recent"
  | "longRunning"
  | "heavyDisk";

const GIB = 1024 ** 3;
const MIB = 1024 ** 2;

export const FILTER_PRESETS: readonly {
  id: FilterPreset;
  label: string;
  description: string;
  test: (r: ProcessRow, ctx: ProcessContext) => boolean;
}[] = [
  { id: "all", label: "All", description: "Every process", test: () => true },
  {
    id: "highCpu",
    label: "High CPU",
    description: "CPU above 25%",
    test: (r) => r.cpuPercent > 25,
  },
  {
    id: "highMemory",
    label: "High memory",
    description: "Memory above 1 GB",
    test: (r) => memoryOf(r) > GIB,
  },
  {
    id: "user",
    label: "My processes",
    description: "Owned by the current user",
    test: (r, c) => c.currentUser != null && r.user === c.currentUser,
  },
  {
    id: "system",
    label: "System",
    description: "Owned by other or system accounts",
    test: (r, c) => c.currentUser == null || r.user !== c.currentUser,
  },
  {
    id: "suspended",
    label: "Suspended",
    description: "Stopped or suspended",
    test: (r) => r.status === "stopped",
  },
  {
    id: "recent",
    label: "Recently started",
    description: "Started in the last 5 minutes",
    test: (r, c) => {
      const rt = runtimeSecs(r, c);
      return rt >= 0 && rt < 300;
    },
  },
  {
    id: "longRunning",
    label: "Long running",
    description: "Running for more than 24 hours",
    test: (r, c) => runtimeSecs(r, c) > 86_400,
  },
  {
    id: "heavyDisk",
    label: "Heavy disk I/O",
    description: "More than 1 MB/s read + write",
    test: (r) => r.diskReadPerSec + r.diskWritePerSec > MIB,
  },
];

const PRESET_BY_ID = Object.fromEntries(FILTER_PRESETS.map((p) => [p.id, p])) as Record<
  FilterPreset,
  (typeof FILTER_PRESETS)[number]
>;

// ---------------------------------------------------------------- search

/**
 * Case-insensitive match on name, PID, command line, executable and user.
 * A purely numeric query also matches PIDs and parent PIDs by prefix.
 */
export function matchesSearch(row: ProcessRow, query: string): boolean {
  if (!query) return true;
  if (/^\d+$/.test(query)) {
    if (String(row.pid).startsWith(query) || String(row.parentPid ?? "").startsWith(query)) {
      return true;
    }
  }
  return (
    row.name.toLowerCase().includes(query) ||
    (row.cmd?.toLowerCase().includes(query) ?? false) ||
    (row.exe?.toLowerCase().includes(query) ?? false) ||
    (row.user?.toLowerCase().includes(query) ?? false)
  );
}

// ---------------------------------------------------------------- sort

export type SortDirection = "asc" | "desc";
export interface SortState {
  column: ColumnId;
  direction: SortDirection;
}

export function compareRows(
  a: ProcessRow,
  b: ProcessRow,
  sort: SortState,
  ctx: ProcessContext,
): number {
  const col = COLUMN_BY_ID[sort.column];
  const av = col.sortValue(a, ctx);
  const bv = col.sortValue(b, ctx);
  let cmp: number;
  if (typeof av === "number" && typeof bv === "number") cmp = av - bv;
  else cmp = String(av).localeCompare(String(bv));
  if (cmp === 0) cmp = a.pid - b.pid; // stable, deterministic ordering
  return sort.direction === "asc" ? cmp : -cmp;
}

/** Next sort state when a column header is clicked. */
export function nextSort(current: SortState, column: ColumnId): SortState {
  if (current.column === column) {
    return { column, direction: current.direction === "asc" ? "desc" : "asc" };
  }
  return { column, direction: COLUMN_BY_ID[column].descendingFirst ? "desc" : "asc" };
}

// ---------------------------------------------------------------- pipeline

export interface ViewOptions {
  search: string;
  preset: FilterPreset;
  sort: SortState;
}

/** True when no search or preset narrows the list. */
export function isUnfiltered(options: Pick<ViewOptions, "search" | "preset">): boolean {
  return options.search.trim() === "" && options.preset === "all";
}

/** Predicate combining the search query and filter preset. */
export function makeMatcher(
  options: Pick<ViewOptions, "search" | "preset">,
  ctx: ProcessContext,
): (row: ProcessRow) => boolean {
  const query = options.search.trim().toLowerCase();
  const preset = PRESET_BY_ID[options.preset];
  return (r) => preset.test(r, ctx) && matchesSearch(r, query);
}

/** Filter + sort in one pass over the rows. Returns a new array. */
export function processView(
  rows: readonly ProcessRow[],
  options: ViewOptions,
  ctx: ProcessContext,
): ProcessRow[] {
  const out = rows.filter(makeMatcher(options, ctx));
  out.sort((a, b) => compareRows(a, b, options.sort, ctx));
  return out;
}

/** The user that owns the given PID (normally the task manager itself). */
export function userOf(rows: readonly ProcessRow[], pid: number | undefined): string | null {
  if (pid == null) return null;
  return rows.find((r) => r.pid === pid)?.user ?? null;
}

/** DOM id of a process table row, for `aria-activedescendant`. */
export function rowDomId(pid: number): string {
  return `proc-row-${pid}`;
}

/** Whether a process started within the last `windowSecs` (0 turns the highlight off). */
export function isNewProcess(startTime: number, nowSecs: number, windowSecs: number): boolean {
  return (
    windowSecs > 0 && startTime > 0 && nowSecs >= startTime && nowSecs - startTime < windowSecs
  );
}

/** Execution state of a process, for red/green colouring. */
/**
 * The memory figure shown for a process in every view: proportional memory
 * (PSS, shared pages split between the processes using them) when the system
 * has measured it, else resident memory. Using one figure everywhere makes an
 * application's total equal the sum of its processes and match the Memory page.
 */
export function memoryOf(row: Pick<ProcessRow, "pssBytes" | "memoryBytes">): number {
  return row.pssBytes ?? row.memoryBytes;
}

export type RunState = "active" | "idle" | "problem";

export function baseName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

/**
 * Name to show for a process. Linux truncates the kernel name to 15
 * characters ("WebKitWebProces"); when the executable's file name extends it,
 * show the full file name.
 */
export function displayName(row: Pick<ProcessRow, "name" | "exe">): string {
  if (!row.exe) return row.name;
  const base = baseName(row.exe);
  return base.length > row.name.length && base.startsWith(row.name) ? base : row.name;
}

/**
 * The colour follows the status label exactly, so the two can never disagree.
 * (The backend already reports "Running" for a process that used CPU during
 * the last interval, since the instantaneous kernel state is "sleeping" for
 * almost every busy process.)
 *
 * Green: Running. Red: Suspended, Zombie, Dead or Disk sleep (stuck waiting on
 * the disk). Everything else, including Sleeping and Idle, is uncoloured.
 */
export function runState(row: Pick<ProcessRow, "status">): RunState {
  switch (row.status) {
    case "running":
      return "active";
    case "stopped":
    case "zombie":
    case "dead":
    case "diskSleep":
      return "problem";
    default:
      return "idle";
  }
}
