import { describe, expect, it } from "vitest";
import type { ProcessRow } from "@/types";
import { makeProcessRow as makeRow } from "@/test/fixtures";
import { groupByApplication } from "./groups";

const MB = 1024 ** 2;

describe("groupByApplication", () => {
  it("sums proportional memory per executable and sorts by size", () => {
    const rows: ProcessRow[] = [
      makeRow({
        pid: 1,
        name: "brave",
        exe: "/opt/brave/brave",
        memoryBytes: 700 * MB,
        pssBytes: 400 * MB,
      }),
      makeRow({
        pid: 2,
        name: "brave",
        exe: "/opt/brave/brave",
        memoryBytes: 650 * MB,
        pssBytes: 350 * MB,
      }),
      makeRow({
        pid: 3,
        name: "postgres",
        exe: "/usr/bin/postgres",
        memoryBytes: 500 * MB,
        pssBytes: 480 * MB,
      }),
    ];
    const groups = groupByApplication(rows, 8 * 1024 * MB);
    expect(groups.map((g) => g.name)).toEqual(["brave", "postgres"]);
    // 750 MB real, not the 1350 MB a resident-memory sum would claim.
    expect(groups[0]?.bytes).toBe(750 * MB);
    expect(groups[0]?.processes.map((p) => p.pid)).toEqual([1, 2]);
    expect(groups[0]?.share).toBeCloseTo((750 / 8192) * 100, 5);
    expect(groups[0]?.estimated).toBe(0);
  });

  it("falls back to resident memory and counts the estimates", () => {
    const rows = [
      makeRow({
        pid: 1,
        name: "node",
        exe: "/usr/bin/node",
        memoryBytes: 300 * MB,
        pssBytes: 250 * MB,
      }),
      makeRow({
        pid: 2,
        name: "node",
        exe: "/usr/bin/node",
        memoryBytes: 100 * MB,
        pssBytes: null,
      }),
    ];
    const [g] = groupByApplication(rows, 1024 * MB);
    expect(g?.bytes).toBe(350 * MB);
    expect(g?.estimated).toBe(1);
    expect(g?.processes[1]?.exact).toBe(false);
  });

  it("groups by name when the executable is unreadable and ignores empty processes", () => {
    const rows = [
      makeRow({ pid: 1, name: "sshd", exe: null, memoryBytes: 10 * MB, pssBytes: null }),
      makeRow({ pid: 2, name: "sshd", exe: null, memoryBytes: 5 * MB, pssBytes: null }),
      makeRow({ pid: 3, name: "idle", exe: null, memoryBytes: 0, pssBytes: null }),
    ];
    const groups = groupByApplication(rows, 0);
    expect(groups).toHaveLength(1);
    expect(groups[0]?.name).toBe("sshd");
    expect(groups[0]?.share).toBe(0);
  });

  it("uses the file name of Windows paths", () => {
    const [g] = groupByApplication(
      [
        makeRow({
          pid: 1,
          name: "chrome.exe",
          exe: "C:\\Program Files\\Google\\chrome.exe",
          memoryBytes: MB,
        }),
      ],
      1024 * MB,
    );
    expect(g?.name).toBe("chrome.exe");
  });

  it("flags programs and processes that just started", () => {
    const rows: ProcessRow[] = [
      makeRow({ pid: 1, name: "brave", exe: "/b/brave", memoryBytes: 500 * MB, startTime: 100 }),
      makeRow({ pid: 2, name: "brave", exe: "/b/brave", memoryBytes: 5 * MB, startTime: 995 }),
      makeRow({ pid: 3, name: "tiny", exe: "/t/tiny", memoryBytes: 2 * MB, startTime: 998 }),
    ];
    const groups = groupByApplication(rows, 16 * 1024 * MB, { nowSecs: 1000, newSecs: 10 });
    const brave = groups.find((g) => g.name === "brave")!;
    expect(brave.hasNew).toBe(true);
    expect(brave.processes.map((p) => [p.pid, p.isNew])).toEqual([
      [1, false],
      [2, true],
    ]);
    expect(groups.find((g) => g.name === "tiny")!.hasNew).toBe(true);
    // Switched off (0 seconds) or no clock: nothing is flagged.
    expect(groupByApplication(rows, 1, { nowSecs: 1000, newSecs: 0 }).some((g) => g.hasNew)).toBe(
      false,
    );
    expect(groupByApplication(rows, 1).some((g) => g.hasNew)).toBe(false);
  });

  it("drops a program the moment its last process is gone", () => {
    const rows: ProcessRow[] = [
      makeRow({ pid: 1, name: "a", exe: "/a", memoryBytes: 10 * MB }),
      makeRow({ pid: 2, name: "b", exe: "/b", memoryBytes: 20 * MB }),
    ];
    expect(groupByApplication(rows, 1024 * MB).map((g) => g.name)).toEqual(["b", "a"]);
    expect(groupByApplication(rows.slice(1), 1024 * MB).map((g) => g.name)).toEqual(["b"]);
  });
});
