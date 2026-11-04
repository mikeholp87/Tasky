import { isNewProcess } from "@/features/processes/logic";
import type { ProcessRow } from "@/types";

export interface MemoryProcess {
  pid: number;
  name: string;
  bytes: number;
  /** Shared-memory-aware figure (PSS); otherwise resident memory. */
  exact: boolean;
  /** Started within the "new process" window, so it is always listed. */
  isNew: boolean;
}

export interface MemoryGroup {
  key: string;
  name: string;
  bytes: number;
  /** Percent of installed memory. */
  share: number;
  processes: MemoryProcess[];
  /** Processes in this group that only have a resident-memory figure. */
  estimated: number;
  /** One of its processes is new, so the application is always listed. */
  hasNew: boolean;
}

function baseName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

/**
 * Sums memory per application, like Task Manager's grouped view. Processes
 * are grouped by executable path (or by name when the path is unreadable).
 * Memory is the proportional figure when known, because summing resident
 * memory counts pages shared between a program's processes once per process.
 */
export function groupByApplication(
  rows: readonly ProcessRow[],
  totalBytes: number,
  /** Current time and the "new" window, to flag processes that just started. */
  now: { nowSecs: number; newSecs: number } = { nowSecs: 0, newSecs: 0 },
): MemoryGroup[] {
  const groups = new Map<string, MemoryGroup>();
  for (const row of rows) {
    const exact = row.pssBytes != null;
    const bytes = row.pssBytes ?? row.memoryBytes;
    if (bytes <= 0) continue;
    const key = row.exe ?? row.name;
    let group = groups.get(key);
    if (!group) {
      group = {
        key,
        name: row.exe ? baseName(row.exe) : row.name,
        bytes: 0,
        share: 0,
        processes: [],
        estimated: 0,
        hasNew: false,
      };
      groups.set(key, group);
    }
    group.bytes += bytes;
    if (!exact) group.estimated += 1;
    const isNew = isNewProcess(row.startTime, now.nowSecs, now.newSecs);
    if (isNew) group.hasNew = true;
    group.processes.push({ pid: row.pid, name: row.name, bytes, exact, isNew });
  }
  const out = [...groups.values()];
  for (const g of out) {
    g.processes.sort((a, b) => b.bytes - a.bytes || a.pid - b.pid);
    g.share = totalBytes > 0 ? (g.bytes / totalBytes) * 100 : 0;
  }
  return out.sort((a, b) => b.bytes - a.bytes || a.name.localeCompare(b.name));
}
