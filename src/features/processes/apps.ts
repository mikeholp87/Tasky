/**
 * Applications view: processes of one program (same executable) are shown
 * under a single row with summed CPU, memory and disk. A program with one
 * process is shown as that process. Pure functions, unit-tested.
 */
import type { ProcessRow } from "@/types";
import { memoryOf } from "./logic";
import type { TableItem } from "./tree";

export const GROUP_MEMORY_NOTE =
  "Sum of each process's memory. Linux uses proportional memory where available; Windows uses resident memory, so shared pages may be counted more than once.";

function baseName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

const keyOf = (row: ProcessRow): string => row.exe ?? row.name;

/**
 * Stable negative id for a group, so it can never equal a real PID and keeps
 * its expanded/selected state across ticks.
 */
function groupId(key: string, taken: ReadonlySet<number>): number {
  let h = 2166136261;
  for (let i = 0; i < key.length; i++) h = Math.imul(h ^ key.charCodeAt(i), 16777619);
  let id = -(1 + ((h >>> 0) % 0x3fffffff));
  while (taken.has(id)) id--;
  return id;
}

function summarize(members: readonly ProcessRow[], id: number, key: string): ProcessRow {
  const first = members[0] as ProcessRow;
  const sum = (pick: (r: ProcessRow) => number) => members.reduce((a, r) => a + pick(r), 0);
  const started = members.map((r) => r.startTime).filter((t) => t > 0);
  const users = new Set(members.map((r) => r.user));
  const anyPss = members.some((r) => r.pssBytes != null);
  return {
    ...first,
    pid: id,
    parentPid: null,
    name: `${first.exe ? baseName(first.exe) : first.name} (${members.length})`,
    status: members.some((r) => r.status === "running") ? "running" : first.status,
    cpuPercent: sum((r) => r.cpuPercent),
    memoryBytes: sum((r) => r.memoryBytes),
    virtualMemoryBytes: sum((r) => r.virtualMemoryBytes),
    diskReadPerSec: sum((r) => r.diskReadPerSec),
    diskWritePerSec: sum((r) => r.diskWritePerSec),
    user: users.size === 1 ? (first.user ?? null) : null,
    startTime: started.length > 0 ? Math.min(...started) : 0,
    threads: members.every((r) => r.threads != null) ? sum((r) => r.threads ?? 0) : null,
    priority: null,
    exe: key === first.name ? null : first.exe,
    cmd: null,
    sessionId: null,
    kernelThread: false,
    // The same figure the member rows show, so the group equals their sum.
    pssBytes: anyPss ? sum(memoryOf) : null,
    restricted: members.every((r) => r.restricted),
  };
}

export interface AppsOptions {
  /** Group ids whose processes are shown; groups start collapsed. */
  expanded: ReadonlySet<number>;
  compare: (a: ProcessRow, b: ProcessRow) => number;
  /** When set, only matching processes are grouped; expansion is unchanged. */
  matches?: (row: ProcessRow) => boolean;
}

function groupsOf(rows: readonly ProcessRow[]): Map<string, ProcessRow[]> {
  const groups = new Map<string, ProcessRow[]>();
  for (const row of rows) {
    const list = groups.get(keyOf(row));
    if (list) list.push(row);
    else groups.set(keyOf(row), [row]);
  }
  return groups;
}

/** Ids of the groups with more than one process, for "Expand all". */
export function groupIds(rows: readonly ProcessRow[]): number[] {
  const taken = new Set<number>();
  const ids: number[] = [];
  for (const [key, members] of groupsOf(rows)) {
    if (members.length < 2) continue;
    const id = groupId(key, taken);
    taken.add(id);
    ids.push(id);
  }
  return ids;
}

export function flattenApps(rows: readonly ProcessRow[], options: AppsOptions): TableItem[] {
  const { expanded, compare, matches } = options;
  const visible = matches ? rows.filter(matches) : rows;
  const taken = new Set<number>();
  const tops: { row: ProcessRow; members: ProcessRow[]; group: boolean }[] = [];
  for (const [key, members] of groupsOf(visible)) {
    if (members.length === 1) {
      tops.push({ row: members[0] as ProcessRow, members, group: false });
      continue;
    }
    const id = groupId(key, taken);
    taken.add(id);
    tops.push({ row: summarize(members, id, key), members, group: true });
  }
  tops.sort((a, b) => compare(a.row, b.row));

  const out: TableItem[] = [];
  for (const top of tops) {
    if (!top.group) {
      out.push({ row: top.row, depth: 0, hasChildren: false });
      continue;
    }
    const open = expanded.has(top.row.pid);
    out.push({ row: top.row, depth: 0, hasChildren: true, expanded: open, group: true });
    if (open) {
      for (const member of [...top.members].sort(compare)) {
        out.push({ row: member, depth: 1, hasChildren: false });
      }
    }
  }
  return out;
}
