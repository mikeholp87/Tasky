import type { ActivityEvent, ActivityKind } from "@/types";

export type ActivityFilter = "all" | "processes" | "resources" | "actions";

export const FILTER_LABEL: Record<ActivityFilter, string> = {
  all: "All",
  processes: "Starts and exits",
  resources: "CPU, memory, alerts",
  actions: "Actions and benchmarks",
};

const GROUP: Record<ActivityKind, Exclude<ActivityFilter, "all">> = {
  processStarted: "processes",
  processExited: "processes",
  cpuSpike: "resources",
  cpuNormal: "resources",
  memoryJump: "resources",
  memoryGrowth: "resources",
  threadGrowth: "resources",
  alertFired: "resources",
  processAction: "actions",
  benchmarkStarted: "actions",
  benchmarkCompleted: "actions",
};

/** One-line wording for an event; facts only, no diagnosis. */
export function describe(e: ActivityEvent): string {
  const who = e.name ?? "A process";
  switch (e.kind) {
    case "processStarted":
      return `${who} started`;
    case "processExited":
      return `${who} exited`;
    case "cpuSpike":
      return `${who} CPU use is high`;
    case "cpuNormal":
      return `${who} CPU use returned to normal`;
    case "memoryJump":
      return `${who} memory grew quickly`;
    case "memoryGrowth":
      return `${who}: potential memory growth`;
    case "threadGrowth":
      return `${who}: thread count keeps rising`;
    case "alertFired":
      return `Alert: ${who}`;
    case "processAction":
      return `${who}: action from this app`;
    case "benchmarkStarted":
      return "Benchmark started";
    case "benchmarkCompleted":
      return "Benchmark finished";
  }
}

export function filterEvents(
  events: readonly ActivityEvent[],
  filter: ActivityFilter,
  query: string,
): ActivityEvent[] {
  const q = query.trim().toLowerCase();
  return events.filter((e) => {
    if (filter !== "all" && GROUP[e.kind] !== filter) return false;
    if (!q) return true;
    return (
      (e.name?.toLowerCase().includes(q) ?? false) ||
      (e.detail?.toLowerCase().includes(q) ?? false) ||
      String(e.pid) === q ||
      describe(e).toLowerCase().includes(q)
    );
  });
}

/** `HH:MM:SS` in the user's time zone, 24-hour. */
export function clock(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}
