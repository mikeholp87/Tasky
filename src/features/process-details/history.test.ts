import { describe, expect, it } from "vitest";
import type { ProcessPoint } from "@/types";
import { appendPoint, mergeHistory } from "./history";

const pt = (t: number): ProcessPoint => ({ t, cpu: t, memory: 0, read: 0, write: 0 });
const ts = (ps: ProcessPoint[]) => ps.map((p) => p.t);

describe("process history", () => {
  it("appends only newer points and caps length", () => {
    let points: ProcessPoint[] = [];
    for (const t of [1, 2, 2, 1, 3, 4]) points = appendPoint(points, pt(t), 3);
    expect(ts(points)).toEqual([2, 3, 4]);
  });

  it("merges backfill with newer live points without duplicates", () => {
    expect(ts(mergeHistory([pt(1), pt(2), pt(3)], [pt(2), pt(3), pt(4), pt(5)], 10))).toEqual([
      1, 2, 3, 4, 5,
    ]);
  });

  it("keeps the newest points when over capacity", () => {
    expect(ts(mergeHistory([pt(1), pt(2)], [pt(3), pt(4)], 3))).toEqual([2, 3, 4]);
  });

  it("works before the backfill arrives", () => {
    expect(ts(mergeHistory([], [pt(7)], 3))).toEqual([7]);
  });
});
