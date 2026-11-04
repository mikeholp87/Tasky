import { describe, expect, it } from "vitest";
import { makeProcessRow } from "@/test/fixtures";
import { flattenApps, groupIds } from "./apps";
import { formatBytes } from "@/utils/format";
import { cellContent } from "./cells";
import { isNewProcess, memoryOf } from "./logic";

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

describe("one memory figure in every view", () => {
  const ctx = { totalMemoryBytes: 10_000, nowSecs: 1000, currentUser: null };
  // Three processes sharing most of their pages: resident memory counts the
  // shared part three times, proportional memory once.
  const shared = [
    makeProcessRow({ pid: 1, name: "app", exe: "/a/app", memoryBytes: 1000, pssBytes: 400 }),
    makeProcessRow({ pid: 2, name: "app", exe: "/a/app", memoryBytes: 900, pssBytes: 300 }),
    makeProcessRow({ pid: 3, name: "app", exe: "/a/app", memoryBytes: 800, pssBytes: 300 }),
  ];

  it("prefers proportional memory and falls back to resident memory", () => {
    expect(memoryOf(shared[0]!)).toBe(400);
    expect(memoryOf(makeProcessRow({ memoryBytes: 700, pssBytes: null }))).toBe(700);
  });

  it("makes an application's total the sum of the figures its processes show", () => {
    const items = flattenApps(shared, { expanded: new Set(groupIds(shared)), compare: byCpu });
    const group = items.find((i) => i.group)!;
    const members = items.filter((i) => !i.group);
    expect(memoryOf(group.row)).toBe(1000);
    expect(members.reduce((a, i) => a + memoryOf(i.row), 0)).toBe(memoryOf(group.row));
    // The cell shows exactly that figure.
    expect(cellContent("memory", group.row, ctx)).toBe(formatBytes(1000));
  });

  it("counts a process the system could not measure at its resident memory", () => {
    const mixed = [
      shared[0]!,
      makeProcessRow({ pid: 9, name: "app", exe: "/a/app", memoryBytes: 500, pssBytes: null }),
    ];
    const group = flattenApps(mixed, { expanded: new Set(), compare: byCpu }).find((i) => i.group)!;
    expect(memoryOf(group.row)).toBe(900); // 400 measured + 500 resident
  });

  it("shows a group as resident memory when nothing was measured", () => {
    const none = shared.map((r) => ({ ...r, pssBytes: null }));
    const group = flattenApps(none, { expanded: new Set(), compare: byCpu }).find((i) => i.group)!;
    expect(memoryOf(group.row)).toBe(2700);
    expect(group.row.pssBytes).toBeNull();
  });
});
