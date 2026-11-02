import { describe, expect, it } from "vitest";
import type { ProcessRow } from "@/types";
import {
  type ProcessContext,
  matchesSearch,
  nextSort,
  processView,
  runtimeSecs,
  userOf,
} from "./logic";
import { mockProcesses } from "./mock";

const NOW = 1_800_000_000;
const ctx: ProcessContext = {
  totalMemoryBytes: 16 * 1024 ** 3,
  nowSecs: NOW,
  currentUser: "alice",
};

function row(overrides: Partial<ProcessRow>): ProcessRow {
  return {
    pid: 1,
    parentPid: null,
    name: "proc",
    status: "running",
    cpuPercent: 0,
    memoryBytes: 0,
    virtualMemoryBytes: 0,
    diskReadPerSec: 0,
    diskWritePerSec: 0,
    user: "alice",
    startTime: NOW - 60,
    threads: 1,
    priority: 0,
    exe: null,
    cmd: null,
    sessionId: null,
    restricted: false,
    ...overrides,
  };
}

const rows = [
  row({
    pid: 12345,
    name: "chrome",
    cpuPercent: 30,
    memoryBytes: 2 * 1024 ** 3,
    cmd: "chrome --type=renderer",
  }),
  row({
    pid: 200,
    name: "node",
    cpuPercent: 5,
    memoryBytes: 300 * 1024 ** 2,
    cmd: "node /opt/codex/server.js",
    user: "bob",
  }),
  row({ pid: 300, name: "python3", cpuPercent: 0.5, exe: "/usr/bin/python3", status: "stopped" }),
  row({ pid: 1, name: "systemd", user: "root", startTime: NOW - 3 * 86_400 }),
];

describe("search", () => {
  it("matches name, command line, executable and user case-insensitively", () => {
    expect(matchesSearch(rows[0] as ProcessRow, "chrome")).toBe(true);
    expect(matchesSearch(rows[1] as ProcessRow, "codex")).toBe(true);
    expect(matchesSearch(rows[2] as ProcessRow, "/usr/bin")).toBe(true);
    expect(matchesSearch(rows[1] as ProcessRow, "bob")).toBe(true);
    expect(matchesSearch(rows[1] as ProcessRow, "claude")).toBe(false);
  });

  it("matches PIDs by prefix for numeric queries", () => {
    const view = processView(
      rows,
      { search: "123", preset: "all", sort: { column: "pid", direction: "asc" } },
      ctx,
    );
    expect(view.map((r) => r.pid)).toEqual([12345]);
  });

  it("trims and lowercases the query", () => {
    const view = processView(
      rows,
      { search: "  NODE ", preset: "all", sort: { column: "pid", direction: "asc" } },
      ctx,
    );
    expect(view.map((r) => r.name)).toEqual(["node"]);
  });
});

describe("filter presets", () => {
  const sort = { column: "pid" as const, direction: "asc" as const };
  const pids = (preset: Parameters<typeof processView>[1]["preset"]) =>
    processView(rows, { search: "", preset, sort }, ctx).map((r) => r.pid);

  it("applies thresholds from the spec", () => {
    expect(pids("highCpu")).toEqual([12345]);
    expect(pids("highMemory")).toEqual([12345]);
    expect(pids("suspended")).toEqual([300]);
    expect(pids("longRunning")).toEqual([1]);
    expect(pids("recent")).toEqual([200, 300, 12345]);
  });

  it("splits user and system processes by the current user", () => {
    expect(pids("user")).toEqual([300, 12345]);
    expect(pids("system")).toEqual([1, 200]);
  });

  it("combines presets with search", () => {
    const view = processView(rows, { search: "chrome", preset: "highCpu", sort }, ctx);
    expect(view).toHaveLength(1);
  });
});

describe("sorting", () => {
  it("sorts numerically and breaks ties by PID", () => {
    const tied = [
      row({ pid: 9, cpuPercent: 1 }),
      row({ pid: 3, cpuPercent: 1 }),
      row({ pid: 5, cpuPercent: 7 }),
    ];
    const view = processView(
      tied,
      { search: "", preset: "all", sort: { column: "cpu", direction: "desc" } },
      ctx,
    );
    expect(view.map((r) => r.pid)).toEqual([5, 9, 3]);
  });

  it("sorts names case-insensitively", () => {
    const named = [
      row({ pid: 1, name: "b" }),
      row({ pid: 2, name: "A" }),
      row({ pid: 3, name: "c" }),
    ];
    const view = processView(
      named,
      { search: "", preset: "all", sort: { column: "name", direction: "asc" } },
      ctx,
    );
    expect(view.map((r) => r.name)).toEqual(["A", "b", "c"]);
  });

  it("does not mutate the input", () => {
    const input = [...rows];
    processView(
      input,
      { search: "", preset: "all", sort: { column: "cpu", direction: "desc" } },
      ctx,
    );
    expect(input).toEqual(rows);
  });

  it("toggles direction and uses sensible first directions", () => {
    expect(nextSort({ column: "name", direction: "asc" }, "name")).toEqual({
      column: "name",
      direction: "desc",
    });
    expect(nextSort({ column: "name", direction: "asc" }, "cpu")).toEqual({
      column: "cpu",
      direction: "desc",
    });
    expect(nextSort({ column: "cpu", direction: "desc" }, "pid")).toEqual({
      column: "pid",
      direction: "asc",
    });
  });
});

describe("helpers", () => {
  it("computes runtime and handles unknown start time", () => {
    expect(runtimeSecs(row({ startTime: NOW - 90 }), ctx)).toBe(90);
    expect(runtimeSecs(row({ startTime: 0 }), ctx)).toBe(-1);
  });

  it("finds the owning user of a PID", () => {
    expect(userOf(rows, 200)).toBe("bob");
    expect(userOf(rows, 999)).toBeNull();
  });
});

describe("performance with large process lists", () => {
  it.each([100, 500, 1_000, 5_000])("filters and sorts %i processes quickly", (count) => {
    const data = mockProcesses(count, NOW);
    const start = performance.now();
    const view = processView(
      data,
      { search: "node", preset: "all", sort: { column: "cpu", direction: "desc" } },
      ctx,
    );
    processView(
      data,
      { search: "", preset: "all", sort: { column: "name", direction: "asc" } },
      ctx,
    );
    const elapsed = performance.now() - start;
    expect(view.length).toBeGreaterThan(0);
    // Generous bound for slow CI machines; typical is a few ms for 5,000 rows.
    expect(elapsed).toBeLessThan(250);
  });
});

describe("restricted processes", () => {
  it("sort below every known CPU value instead of pretending to be 0%", () => {
    const list = [
      row({ pid: 1, cpuPercent: 0 }),
      row({ pid: 2, cpuPercent: 0, restricted: true }),
      row({ pid: 3, cpuPercent: 0.1 }),
    ];
    const view = processView(
      list,
      { search: "", preset: "all", sort: { column: "cpu", direction: "desc" } },
      ctx,
    );
    expect(view.map((r) => r.pid)).toEqual([3, 1, 2]);
  });
});
