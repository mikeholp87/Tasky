import { describe, expect, it } from "vitest";
import type { ProcessRow } from "@/types";
import { buildForest, childrenOf, flattenTree, parentPids } from "./tree";

function p(pid: number, parentPid: number | null, name: string, startTime = 100): ProcessRow {
  return {
    pid,
    parentPid,
    name,
    status: "running",
    cpuPercent: 0,
    memoryBytes: 0,
    virtualMemoryBytes: 0,
    diskReadPerSec: 0,
    diskWritePerSec: 0,
    user: null,
    startTime,
    threads: null,
    priority: null,
    exe: null,
    cmd: null,
    sessionId: null,
    kernelThread: false,
    pssBytes: null,
    restricted: false,
  };
}

// systemd ─┬─ NetworkManager
//          ├─ dockerd ── containerd
//          └─ session ─┬─ brave ── renderer
//                      └─ code
const rows = [
  p(1, 0, "systemd", 10),
  p(10, 1, "NetworkManager", 20),
  p(20, 1, "dockerd", 20),
  p(21, 20, "containerd", 30),
  p(30, 1, "session", 20),
  p(31, 30, "brave", 40),
  p(32, 31, "renderer", 50),
  p(33, 30, "code", 40),
];
const byName = (a: ProcessRow, b: ProcessRow) => a.name.localeCompare(b.name);
const names = (items: ReturnType<typeof flattenTree>) =>
  items.map((i) => `${"  ".repeat(i.depth ?? 0)}${i.row.name}`);

describe("buildForest", () => {
  it("links children to parents and finds roots", () => {
    const f = buildForest(rows);
    expect(f.roots.map((r) => r.name)).toEqual(["systemd"]);
    expect(f.children.get(30)?.map((r) => r.name)).toEqual(["brave", "code"]);
  });

  it("treats a parent that started after the child as PID reuse", () => {
    const f = buildForest([p(5, null, "new-owner-of-pid-5", 500), p(6, 5, "orphan", 100)]);
    expect(f.roots.map((r) => r.name).sort()).toEqual(["new-owner-of-pid-5", "orphan"]);
  });

  it("survives self-parenting and cycles", () => {
    const cyclic = [p(1, 2, "a", 100), p(2, 1, "b", 100), p(3, 3, "self", 100)];
    const items = flattenTree(cyclic, { collapsed: new Set(), compare: byName });
    // Every process appears exactly once, even inside a cycle.
    expect(items.map((i) => i.row.pid).sort()).toEqual([1, 2, 3]);
  });
});

describe("flattenTree", () => {
  it("renders the full hierarchy sorted by sibling order", () => {
    expect(names(flattenTree(rows, { collapsed: new Set(), compare: byName }))).toEqual([
      "systemd",
      "  dockerd",
      "    containerd",
      "  NetworkManager",
      "  session",
      "    brave",
      "      renderer",
      "    code",
    ]);
  });

  it("hides children of collapsed nodes", () => {
    const items = flattenTree(rows, { collapsed: new Set([30, 20]), compare: byName });
    expect(names(items)).toEqual(["systemd", "  dockerd", "  NetworkManager", "  session"]);
    expect(items.find((i) => i.row.pid === 30)).toMatchObject({
      hasChildren: true,
      expanded: false,
    });
  });

  it("shows matches with their ancestors, expanded, when filtering", () => {
    const items = flattenTree(rows, {
      collapsed: new Set(parentPids(rows)),
      compare: byName,
      matches: (r) => r.name === "renderer",
    });
    expect(names(items)).toEqual(["systemd", "  session", "    brave", "      renderer"]);
  });
});

describe("helpers", () => {
  it("lists parents and direct children", () => {
    expect([...parentPids(rows)].sort((a, b) => a - b)).toEqual([1, 20, 30, 31]);
    expect(
      childrenOf(rows, 1)
        .map((r) => r.name)
        .sort(),
    ).toEqual(["NetworkManager", "dockerd", "session"]);
  });
});
