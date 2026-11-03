import { describe, expect, it } from "vitest";
import { makeProcessRow } from "@/test/fixtures";
import { flattenApps, groupIds } from "./apps";
import { isNewProcess } from "./logic";

const byCpu = (a: { cpuPercent: number }, b: { cpuPercent: number }) => b.cpuPercent - a.cpuPercent;

const rows = [
  makeProcessRow({
    pid: 10,
    name: "brave",
    exe: "/opt/brave/brave",
    cpuPercent: 10,
    memoryBytes: 300,
    startTime: 500,
  }),
  makeProcessRow({
    pid: 11,
    name: "brave",
    exe: "/opt/brave/brave",
    cpuPercent: 12,
    memoryBytes: 700,
    startTime: 400,
  }),
  makeProcessRow({
    pid: 12,
    name: "brave",
    exe: "/opt/brave/brave",
    cpuPercent: 0,
    memoryBytes: 100,
    startTime: 450,
  }),
  makeProcessRow({ pid: 20, name: "sshd", exe: "/usr/sbin/sshd", cpuPercent: 1 }),
  makeProcessRow({ pid: 30, name: "mystery", exe: null, cpuPercent: 5 }),
];

describe("applications view", () => {
  it("sums a program's processes and shows single processes as themselves", () => {
    const items = flattenApps(rows, { expanded: new Set(), compare: byCpu });
    expect(items.map((i) => i.row.name)).toEqual(["brave (3)", "mystery", "sshd"]);
    const group = items[0];
    expect(group?.group).toBe(true);
    expect(group?.row.cpuPercent).toBe(22);
    expect(group?.row.memoryBytes).toBe(1100);
    expect(group?.row.startTime).toBe(400);
    expect(group?.row.pid).toBeLessThan(0);
    expect(items[1]?.group).toBeUndefined();
    expect(items.every((i) => i.depth === 0)).toBe(true);
  });

  it("expands a group on request, sorting its processes", () => {
    const [id] = groupIds(rows);
    expect(id).toBeLessThan(0);
    const items = flattenApps(rows, { expanded: new Set([id as number]), compare: byCpu });
    expect(items.map((i) => [i.row.pid, i.depth])).toEqual([
      [id, 0],
      [11, 1],
      [10, 1],
      [12, 1],
      [30, 0],
      [20, 0],
    ]);
    expect(items[0]?.expanded).toBe(true);
  });

  it("keeps the same group id between calls and never uses a real PID", () => {
    expect(groupIds(rows)).toEqual(groupIds([...rows].reverse()));
  });

  it("filters before grouping and expands what matches", () => {
    const items = flattenApps(rows, {
      expanded: new Set(),
      compare: byCpu,
      matches: (r) => r.cpuPercent >= 10,
    });
    expect(items.map((i) => i.row.pid)).toEqual([expect.any(Number), 11, 10]);
    expect(items[0]?.row.name).toBe("brave (2)");
    expect(items[0]?.row.cpuPercent).toBe(22);
  });
});

describe("new process highlight", () => {
  it("covers the configured window only", () => {
    expect(isNewProcess(100, 105, 10)).toBe(true);
    expect(isNewProcess(100, 110, 10)).toBe(false);
    expect(isNewProcess(100, 105, 0)).toBe(false); // off
    expect(isNewProcess(0, 105, 10)).toBe(false); // unknown start time
    expect(isNewProcess(200, 105, 10)).toBe(false); // clock skew
  });
});
