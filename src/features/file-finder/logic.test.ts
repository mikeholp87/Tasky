import { describe, expect, it } from "vitest";
import type { FileUse } from "@/types";
import { groupByProcess, summarize } from "./logic";

const use = (pid: number, processName: string, path: string, usage: FileUse["usage"]): FileUse => ({
  pid,
  processName,
  path,
  usage,
});

describe("file finder", () => {
  it("groups matches by process in PID order with sorted paths", () => {
    const groups = groupByProcess([
      use(19122, "ffmpeg", "/v/b.mp4", "open"),
      use(18341, "resolve", "/v/video.mp4", "open"),
      use(19122, "ffmpeg", "/v/a.mp4", "mapped"),
    ]);
    expect(groups.map((g) => g.pid)).toEqual([18341, 19122]);
    expect(groups[1]?.uses.map((u) => u.path)).toEqual(["/v/a.mp4", "/v/b.mp4"]);
    expect(groupByProcess([])).toEqual([]);
  });

  it("words the summary for files, folders and none", () => {
    expect(summarize(0, false)).toBe("No process was found using this file.");
    expect(summarize(1, false)).toBe("1 process is using this file.");
    expect(summarize(3, true)).toBe("3 processes are using this folder or something inside it.");
  });
});
