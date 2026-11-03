import { describe as suite, expect, it } from "vitest";
import type { ActivityEvent } from "@/types";
import { clock, describe, filterEvents } from "./logic";

const ev = (
  seq: number,
  kind: ActivityEvent["kind"],
  name: string | null,
  extra: Partial<ActivityEvent> = {},
): ActivityEvent => ({
  seq,
  timestampMs: 0,
  kind,
  pid: null,
  name,
  detail: null,
  ...extra,
});

const events = [
  ev(1, "processStarted", "node", { pid: 28119 }),
  ev(2, "cpuSpike", "node", { pid: 28119, detail: "93% of CPU" }),
  ev(3, "processAction", "node", { pid: 28119, detail: "Suspended from this app" }),
  ev(4, "benchmarkStarted", null, { detail: "Cpu benchmark, Quick profile" }),
  ev(5, "processExited", "ffmpeg", { pid: 5 }),
];

suite("activity", () => {
  it("words events plainly", () => {
    expect(describe(events[0] as ActivityEvent)).toBe("node started");
    expect(describe(ev(9, "processExited", null))).toBe("A process exited");
    expect(describe(events[3] as ActivityEvent)).toBe("Benchmark started");
    expect(describe(ev(9, "memoryGrowth", "node"))).toBe("node: potential memory growth");
  });

  it("filters by group and by text, PID or detail", () => {
    expect(filterEvents(events, "processes", "").map((e) => e.seq)).toEqual([1, 5]);
    expect(filterEvents(events, "resources", "").map((e) => e.seq)).toEqual([2]);
    expect(filterEvents(events, "actions", "").map((e) => e.seq)).toEqual([3, 4]);
    expect(filterEvents(events, "all", "FFMPEG").map((e) => e.seq)).toEqual([5]);
    expect(filterEvents(events, "all", "28119").map((e) => e.seq)).toEqual([1, 2, 3]);
    expect(filterEvents(events, "all", "93%").map((e) => e.seq)).toEqual([2]);
  });

  it("formats a clock time with two digits per part", () => {
    expect(clock(new Date(2026, 10, 3, 9, 5, 7).getTime())).toBe("09:05:07");
  });
});
