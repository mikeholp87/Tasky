import { describe, expect, it } from "vitest";
import { heatLevel, levelLabel, memoryHeatLevel, usageLevel } from "./levels";

describe("usageLevel", () => {
  it("reserves warnings for genuinely high usage", () => {
    expect(usageLevel(42)).toBe("normal");
    expect(usageLevel(79.9)).toBe("normal");
    expect(usageLevel(80)).toBe("warning");
    expect(usageLevel(93)).toBe("critical");
    expect(usageLevel(null)).toBe("normal");
  });

  it("uses neutral wording rather than alarming language", () => {
    expect(levelLabel("warning", "CPU")).toBe("High CPU usage");
    expect(levelLabel("normal", "CPU")).toBeNull();
  });
});

describe("heatLevel", () => {
  it("maps the documented bands", () => {
    expect(heatLevel(5)).toBe(0);
    expect(heatLevel(20)).toBe(1);
    expect(heatLevel(49.9)).toBe(1);
    expect(heatLevel(50)).toBe(2);
    expect(heatLevel(80)).toBe(3);
    expect(heatLevel(100)).toBe(3);
  });
});

describe("memoryHeatLevel", () => {
  it("uses tighter bands for share of RAM", () => {
    expect(memoryHeatLevel(2)).toBe(0);
    expect(memoryHeatLevel(5)).toBe(1);
    expect(memoryHeatLevel(20)).toBe(2);
    expect(memoryHeatLevel(45)).toBe(3);
  });
});
