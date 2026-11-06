import type { MemorySnapshot, MetricsTick, Platform, ProcessRow } from "@/types";

export function memoryGroupKey(process: ProcessRow, platform?: Platform): string {
  const value = process.exe || process.name;
  return `${process.exe ? "exe" : "name"}:${platform === "windows" ? value.toLowerCase() : value}`;
}

export interface MemoryBaseline {
  timestampMs: number;
  totals: ReadonlyMap<string, number>;
}

/** Only grouped byte totals: at most 13 samples, with no command lines or full process rows. */
export class MemoryHistory {
  private samples: MemoryBaseline[] = [];
  private lastTimestamp = 0;
  private lastSeq = 0;
  private platform: Platform | undefined;

  record(tick: MetricsTick, platform?: Platform): MemoryBaseline | null {
    if (
      platform !== this.platform ||
      tick.timestampMs < this.lastTimestamp ||
      tick.seq < this.lastSeq
    ) {
      this.samples = [];
    }
    this.platform = platform;
    this.lastTimestamp = tick.timestampMs;
    this.lastSeq = tick.seq;
    this.samples = this.samples.filter((s) => s.timestampMs >= tick.timestampMs - 60_000);
    const last = this.samples.at(-1);
    if (!last || tick.timestampMs - last.timestampMs >= 5_000) {
      const totals = new Map<string, number>();
      for (const process of tick.processes) {
        const key = memoryGroupKey(process, platform);
        totals.set(key, (totals.get(key) ?? 0) + process.memoryBytes);
      }
      this.samples.push({ timestampMs: tick.timestampMs, totals });
      this.samples = this.samples.slice(-13);
    }
    const first = this.samples[0];
    return first && tick.timestampMs - first.timestampMs >= 5_000 ? first : null;
  }
}

export interface MemoryGroup {
  key: string;
  name: string;
  exe: string | null;
  memoryBytes: number;
  changeBytes: number | null;
  processes: ProcessRow[];
}

export function groupMemory(
  processes: readonly ProcessRow[],
  platform?: Platform,
  baseline: MemoryBaseline | null = null,
): MemoryGroup[] {
  const groups = new Map<string, MemoryGroup>();
  for (const process of processes) {
    const key = memoryGroupKey(process, platform);
    let group = groups.get(key);
    if (!group) {
      group = {
        key,
        name: process.name,
        exe: process.exe,
        memoryBytes: 0,
        changeBytes: null,
        processes: [],
      };
      groups.set(key, group);
    }
    group.memoryBytes += process.memoryBytes;
    group.processes.push(process);
  }
  for (const group of groups.values()) {
    // A newly observed group has a zero baseline. Changes include process starts/exits.
    group.changeBytes = baseline ? group.memoryBytes - (baseline.totals.get(group.key) ?? 0) : null;
    group.processes.sort((a, b) => b.memoryBytes - a.memoryBytes || a.pid - b.pid);
  }
  return [...groups.values()].sort(
    (a, b) => b.memoryBytes - a.memoryBytes || a.key.localeCompare(b.key),
  );
}

export function commitPercent(memory: MemorySnapshot): number | null {
  return memory.commitUsedBytes != null &&
    memory.commitLimitBytes != null &&
    memory.commitLimitBytes > 0
    ? (memory.commitUsedBytes / memory.commitLimitBytes) * 100
    : null;
}
