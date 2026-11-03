import { describe, expect, it } from "vitest";
import type { MemoryRegion, ModuleInfo, OpenFile } from "@/types";
import { filterModules, filterOpenFiles, filterRegions, segments } from "./logic";

const region = (
  kind: MemoryRegion["kind"],
  path: string | null,
  start = "1000",
  permissions = "r-xp",
): MemoryRegion => ({
  start,
  end: "2000",
  size: 4096,
  permissions,
  offset: 0,
  path,
  kind,
  shared: false,
});

describe("process inspection helpers", () => {
  it("turns totals into shares and drops empty kinds", () => {
    const out = segments([
      { kind: "heap", bytes: 750, regions: 2 },
      { kind: "stack", bytes: 250, regions: 1 },
      { kind: "anonymous", bytes: 0, regions: 0 },
    ]);
    expect(out.map((s) => [s.kind, s.percent])).toEqual([
      ["heap", 75],
      ["stack", 25],
    ]);
    expect(segments([])).toEqual([]);
  });

  it("filters regions by kind, path, address and permissions", () => {
    const regions = [
      region("sharedLibrary", "/usr/lib/libc.so.6"),
      region("heap", "[heap]", "55d0"),
      region("anonymous", null, "7f00", "rw-p"),
    ];
    expect(filterRegions(regions, "heap", "")).toHaveLength(1);
    expect(filterRegions(regions, "all", "LIBC")).toHaveLength(1);
    expect(filterRegions(regions, "all", "7f00")).toHaveLength(1);
    expect(filterRegions(regions, "all", "rw-p")).toHaveLength(1);
    expect(filterRegions(regions, "all", "")).toHaveLength(3);
  });

  it("filters open files and modules", () => {
    const files: OpenFile[] = [
      { descriptor: "3", kind: "file", target: "/home/a/video.mp4", access: "read" },
      { descriptor: "12", kind: "socket", target: "socket:[99]", access: null },
    ];
    expect(filterOpenFiles(files, "VIDEO")).toHaveLength(1);
    expect(filterOpenFiles(files, "12")).toHaveLength(1);
    expect(filterOpenFiles(files, "socket")).toHaveLength(1);

    const mods: ModuleInfo[] = [
      { name: "libz.so.1", path: "/usr/lib/libz.so.1", base: "7f", size: 1, executable: true },
      { name: "app", path: "/usr/bin/app", base: "55", size: 1, executable: true },
    ];
    expect(filterModules(mods, "libz").map((m) => m.name)).toEqual(["libz.so.1"]);
    expect(filterModules(mods, "")).toHaveLength(2);
  });
});
