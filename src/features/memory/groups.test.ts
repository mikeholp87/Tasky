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
});
