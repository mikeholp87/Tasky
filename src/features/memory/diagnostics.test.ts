import { describe, expect, it } from "vitest";
import { makeTick, NOW_MS } from "@/test/fixtures";
import { mockProcesses } from "@/features/processes/mock";
import { commitPercent, groupMemory, MemoryHistory } from "./diagnostics";

const process = (
  pid: number,
  memoryBytes: number,
  exe: string | null = "C:\\Browser\\browser.exe",
) => ({
  ...mockProcesses(1, NOW_MS / 1000)[0]!,
  pid,
  name: "browser.exe",
  exe,
  memoryBytes,
});

describe("memory diagnostics", () => {
  it("combines multi-process apps, distinguishes paths, and normalizes only Windows casing", () => {
    const rows = [
      process(1, 100),
      process(2, 200, "c:\\browser\\BROWSER.exe"),
      process(3, 400, "C:\\Other\\browser.exe"),
      process(4, 50, null),
    ];
    const groups = groupMemory(rows, "windows");
    expect(groups.map((g) => g.memoryBytes)).toEqual([400, 300, 50]);
    expect(groups[1]?.processes.map((p) => p.pid)).toEqual([2, 1]);
    expect(groupMemory(rows, "linux")).toHaveLength(4);
    expect(rows.map((p) => p.pid)).toEqual([1, 2, 3, 4]);
  });

  it("reports net group changes including starts/exits without reusing old process readings", () => {
    const history = new MemoryHistory();
    expect(history.record(makeTick([process(1, 100), process(2, 200)]), "windows")).toBeNull();
    const rows = [process(1, 200), process(3, 400), process(4, 80, "C:\\New\\new.exe")];
    const baseline = history.record(
      makeTick(rows, { seq: 2, timestampMs: NOW_MS + 10_000 }),
      "windows",
    );
    const groups = groupMemory(rows, "windows", baseline);
    expect(groups.map((g) => g.changeBytes)).toEqual([300, 80]);
    const shrinking = groupMemory([process(1, 50)], "windows", baseline);
    expect(shrinking[0]?.changeBytes).toBe(-250);
  });

  it("bounds the growth window and resets after a monitoring gap or engine restart", () => {
    const history = new MemoryHistory();
    for (let second = 0; second <= 300; second++) {
      const baseline = history.record(
        makeTick([process(1, second)], {
          seq: second + 1,
          timestampMs: NOW_MS + second * 1000,
        }),
        "windows",
      );
      if (second >= 60) {
        expect(baseline).not.toBeNull();
        expect(NOW_MS + second * 1000 - baseline!.timestampMs).toBeLessThanOrEqual(60_000);
        expect(NOW_MS + second * 1000 - baseline!.timestampMs).toBeGreaterThanOrEqual(55_000);
      }
    }
    expect(
      history.record(makeTick([], { seq: 302, timestampMs: NOW_MS + 400_000 }), "windows"),
    ).toBeNull();
    expect(
      history.record(makeTick([], { seq: 1, timestampMs: NOW_MS + 410_000 }), "windows"),
    ).toBeNull();
  });

  it("does not invent growth before a baseline, or commit pressure from unavailable counters", () => {
    expect(groupMemory([process(1, 100)])[0]?.changeBytes).toBeNull();
    const memory = makeTick().memory;
    expect(commitPercent(memory)).toBeNull();
    expect(commitPercent({ ...memory, commitUsedBytes: 95, commitLimitBytes: 100 })).toBe(95);
    expect(commitPercent({ ...memory, commitUsedBytes: 95, commitLimitBytes: 0 })).toBeNull();
  });
});
