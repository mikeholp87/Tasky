import { describe, expect, it } from "vitest";
import { batteryState, cacheLabel, describeScreen, formatMhz } from "./format";

describe("system info formatting", () => {
  it("formats frequencies", () => {
    expect(formatMhz(3400)).toBe("3.40 GHz");
    expect(formatMhz(800)).toBe("800 MHz");
    expect(formatMhz(0)).toBeNull();
    expect(formatMhz(null)).toBeNull();
  });

  it("labels caches", () => {
    expect(cacheLabel({ level: 1, kind: "Data", sizeBytes: 1 })).toBe("L1 data");
    expect(cacheLabel({ level: 3, kind: "Unified", sizeBytes: 1 })).toBe("L3");
  });

  it("summarises battery state and screens", () => {
    const b = {
      name: "BAT0",
      percent: 82,
      status: "Charging",
      technology: null,
      cycleCount: null,
      healthPercent: null,
    };
    expect(batteryState(b)).toBe("82% · Charging");
    expect(batteryState({ ...b, percent: null, status: null })).toBeNull();
    expect(describeScreen({ width: 1280, height: 800, pixelRatio: 2 })).toBe(
      "2560 × 1600 (scale 2×)",
    );
    expect(describeScreen({ width: 1920, height: 1080, pixelRatio: 1 })).toBe("1920 × 1080");
  });
});
