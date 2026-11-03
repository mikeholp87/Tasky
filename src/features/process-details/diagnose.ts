/**
 * "Diagnose process": turns data the app already has into plain observations.
 * It states facts and never concludes that a process is hung or leaking; the
 * one pattern it recognises is worded as "consistent with", and says so.
 */
import type {
  ActivityEvent,
  Connection,
  OpenFileList,
  ProcessPoint,
  ProcessRow,
  ThreadList,
} from "@/types";
import { formatBytes, formatBytesPerSecond, formatDuration } from "@/utils/format";

export interface Finding {
  /** "note" findings deserve a second look; "info" ones are context. */
  level: "info" | "note";
  text: string;
}

export interface DiagnoseInput {
  row: ProcessRow;
  /** Recent samples, oldest first (about one per tick). */
  history: readonly ProcessPoint[];
  threads: ThreadList | null;
  openFiles: OpenFileList | null;
  connections: readonly Connection[] | null | undefined;
  childCount: number;
  /** Timeline events recorded for this process. */
  events: readonly ActivityEvent[];
}

/** Window for the CPU and disk observations, in seconds. */
const WINDOW_SECS = 60;
const IDLE_CPU = 0.1;

const WAITING_STATES = new Set(["sleeping", "diskSleep", "idle"]);

function recent(history: readonly ProcessPoint[]): ProcessPoint[] {
  const last = history.at(-1);
  if (!last) return [];
  return history.filter((p) => p.t >= last.t - WINDOW_SECS * 1000);
}

function spanSecs(points: readonly ProcessPoint[]): number {
  const first = points[0];
  const last = points.at(-1);
  return first && last ? Math.round((last.t - first.t) / 1000) : 0;
}

export function diagnose(input: DiagnoseInput): Finding[] {
  const { row, history, threads, openFiles, connections, childCount, events } = input;
  const out: Finding[] = [];
  const add = (level: Finding["level"], text: string) => out.push({ level, text });

  if (row.status === "stopped") {
    add("note", "This process is suspended. It does not run until it is resumed.");
  } else if (row.status === "zombie") {
    add("note", "This process has exited but its parent has not collected its exit status yet.");
  }

  const window = recent(history);
  const secs = spanSecs(window);
  let cpuIdle = false;
  let diskIdle = false;
  if (row.restricted) {
    add(
      "info",
      "The operating system restricts access to this process, so CPU and disk figures are unavailable.",
    );
  } else if (window.length >= 2 && secs >= 5) {
    const avg = window.reduce((a, p) => a + p.cpu, 0) / window.length;
    const peak = Math.max(...window.map((p) => p.cpu));
    cpuIdle = peak < IDLE_CPU * 5 && avg < IDLE_CPU;
    add(
      "info",
      avg < IDLE_CPU
        ? `Used less than ${IDLE_CPU}% CPU over the last ${formatDuration(secs)}.`
        : `Averaged ${avg.toFixed(1)}% CPU over the last ${formatDuration(secs)} (peak ${peak.toFixed(1)}%).`,
    );
    const io = window.reduce((a, p) => a + p.read + p.write, 0);
    diskIdle = io === 0;
    add(
      "info",
      diskIdle
        ? `No disk activity over the last ${formatDuration(secs)}.`
        : `Disk now: read ${formatBytesPerSecond(row.diskReadPerSec)}, write ${formatBytesPerSecond(row.diskWritePerSec)}.`,
    );
  } else {
    add(
      "info",
      "Not enough history yet for CPU and disk observations. History builds up while this page is open.",
    );
  }

  let mostlyWaiting = false;
  if (threads && threads.threads.length > 0) {
    const all = threads.threads;
    const waiting = all.filter((t) => t.state != null && WAITING_STATES.has(t.state)).length;
    const known = all.filter((t) => t.state != null).length;
    if (known > 0) {
      mostlyWaiting = waiting / known >= 0.9;
      add("info", `${waiting} of ${known} threads are waiting (sleeping or in disk wait).`);
    }
    const diskWait = all.filter((t) => t.state === "diskSleep").length;
    if (diskWait > 0) {
      add(
        "note",
        `${diskWait} ${diskWait === 1 ? "thread is" : "threads are"} in uninterruptible disk wait.`,
      );
    }
    const channels = new Map<string, number>();
    for (const t of all)
      if (t.waitChannel) channels.set(t.waitChannel, (channels.get(t.waitChannel) ?? 0) + 1);
    const top = [...channels].sort((a, b) => b[1] - a[1]).slice(0, 3);
    if (top.length > 0) {
      add("info", `Waiting in: ${top.map(([name, n]) => `${name} (${n})`).join(", ")}.`);
    }
  }

  if (openFiles) {
    const files = openFiles.files.filter((f) => f.kind === "file" || f.kind === "directory").length;
    add("info", `${openFiles.files.length} open descriptors, ${files} of them files or folders.`);
  }
  if (connections) {
    const established = connections.filter((c) => c.state === "established").length;
    add("info", `${connections.length} network sockets, ${established} established.`);
  }
  if (childCount > 0)
    add("info", `${childCount} child ${childCount === 1 ? "process" : "processes"}.`);

  const first = history[0];
  const last = history.at(-1);
  if (first && last && last.t - first.t >= 30_000 && first.memory > 0) {
    const change = (last.memory - first.memory) / first.memory;
    if (Math.abs(change) >= 0.1) {
      add(
        "info",
        `Memory ${change > 0 ? "rose" : "fell"} from ${formatBytes(first.memory)} to ${formatBytes(last.memory)} over the last ${formatDuration(Math.round((last.t - first.t) / 1000))}.`,
      );
    }
  }
  for (const e of events) {
    if ((e.kind === "memoryGrowth" || e.kind === "threadGrowth") && e.detail) add("note", e.detail);
  }

  if (cpuIdle && diskIdle && mostlyWaiting && row.status !== "stopped") {
    add(
      "note",
      "Almost no CPU or disk activity and nearly all threads waiting. That is consistent with a process that is idle or blocked on something. It does not by itself show that the process is hung.",
    );
  }
  return out;
}
