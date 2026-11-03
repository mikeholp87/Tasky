import type { FileUse, FileUsage } from "@/types";

export const USAGE_LABEL: Record<FileUsage, string> = {
  open: "Open file",
  workingDirectory: "Working directory",
  executable: "Running executable",
  mapped: "Memory-mapped",
  inUse: "In use",
};

export interface ProcessUses {
  pid: number;
  name: string;
  uses: FileUse[];
}

/** Groups matches by process, ordered by PID; paths inside a group are sorted. */
export function groupByProcess(uses: readonly FileUse[]): ProcessUses[] {
  const groups = new Map<number, ProcessUses>();
  for (const use of uses) {
    const group = groups.get(use.pid) ?? { pid: use.pid, name: use.processName, uses: [] };
    group.uses.push(use);
    groups.set(use.pid, group);
  }
  return [...groups.values()]
    .map((g) => ({ ...g, uses: [...g.uses].sort((a, b) => a.path.localeCompare(b.path)) }))
    .sort((a, b) => a.pid - b.pid);
}

/** Short sentence for the result header. */
export function summarize(processCount: number, isDirectory: boolean): string {
  const what = isDirectory ? "this folder or something inside it" : "this file";
  if (processCount === 0) return `No process was found using ${what}.`;
  return `${processCount} ${processCount === 1 ? "process is" : "processes are"} using ${what}.`;
}
