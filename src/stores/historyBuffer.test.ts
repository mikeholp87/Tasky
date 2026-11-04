import { describe, expect, it } from "vitest";
import type { HistoryPoint } from "@/types";
import { HistoryBuffer } from "./historyBuffer";

const pt = (t: number, cpu = 0): HistoryPoint => ({
  t,
  cpu,
  perCore: [],
  memPercent: 0,
  memUsed: 0,
  swapUsed: 0,
  netRx: 0,
  netTx: 0,
  diskRead: 0,
  diskWrite: 0,
  gpuUsage: null,
  gpuVramPercent: null,
  gpuTemp: null,
});

describe("HistoryBuffer", () => {
  it("drops the oldest points beyond capacity", () => {
    const buf = new HistoryBuffer(3);
    [1, 2, 3, 4, 5].forEach((t) => buf.push(pt(t)));
    expect(buf.window(1e9, 10).map((p) => p.t)).toEqual([3, 4, 5]);
  });

  it("ignores duplicate or out-of-order points", () => {
    const buf = new HistoryBuffer(10);
    buf.push(pt(10));
    buf.push(pt(10));
    buf.push(pt(5));
    expect(buf.length).toBe(1);
  });

  it("returns only the requested time window", () => {
    const buf = new HistoryBuffer(100);
    for (let t = 0; t <= 10_000; t += 1000) buf.push(pt(t));
    expect(buf.window(3, 10_000).map((p) => p.t)).toEqual([7000, 8000, 9000, 10_000]);
    expect(buf.window(0.5, 20_000)).toEqual([]);
  });

  it("backfills older points without duplicating live ones", () => {
    const buf = new HistoryBuffer(100);
    buf.push(pt(5000));
    buf.push(pt(6000));
    buf.backfill([pt(3000), pt(4000), pt(5000)]);
    expect(buf.window(1e9, 6000).map((p) => p.t)).toEqual([3000, 4000, 5000, 6000]);
  });
});

describe("HistoryBuffer gap filling", () => {
  it("merges points into a gap in the middle", () => {
    const buf = new HistoryBuffer(100);
    [1000, 2000, 6000, 7000].forEach((t) => buf.push(pt(t)));
    buf.backfill([pt(2000), pt(3000), pt(4000), pt(5000), pt(6000)]);
    expect(buf.window(1e9, 7000).map((p) => p.t)).toEqual([
      1000, 2000, 3000, 4000, 5000, 6000, 7000,
    ]);
  });
});
