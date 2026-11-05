import { describe, expect, it } from "vitest";
import { STATIC_INFO } from "@/test/fixtures";
import type { HardwareInfo } from "@/types";
import {
  graphicsSummary,
  machineSummary,
  networkSummary,
  powerSummary,
  sensorsSummary,
  storageSummary,
} from "./summaries";

const hw = (over: Partial<HardwareInfo> = {}): HardwareInfo => ({
  machine: {
    vendor: null,
    model: null,
    board: null,
    biosVendor: null,
    biosVersion: null,
    biosDate: null,
    chassis: null,
  },
  cpu: {
    maxMhz: null,
    minMhz: null,
    governor: null,
    caches: [],
    virtualization: null,
    virtualized: false,
  },
  batteries: [],
  powerSource: "unknown",
  ...over,
});

describe("system info summaries", () => {
  it("describes the machine and power state", () => {
    expect(machineSummary(null)).toBe("Loading…");
    expect(machineSummary(hw())).toBe("Not reported");
    expect(
      machineSummary(hw({ machine: { ...hw().machine, vendor: "LENOVO", model: "X1" } })),
    ).toBe("LENOVO X1");
    expect(powerSummary(hw())).toBe("No battery");
    expect(powerSummary(hw({ powerSource: "ac" }))).toBe("Mains power");
    const battery = {
      name: "BAT0",
      percent: 82,
      status: null,
      technology: null,
      cycleCount: null,
      healthPercent: null,
    };
    expect(powerSummary(hw({ batteries: [battery] }))).toBe("Battery 82%");
  });

  it("summarises devices", () => {
    expect(graphicsSummary([])).toBe("None detected");
    const gpu = {
      id: "a",
      name: "Iris",
      vendor: null,
      usagePercent: null,
      memoryUsedBytes: null,
      memoryTotalBytes: null,
    };
    expect(graphicsSummary([gpu])).toBe("Iris");
    expect(graphicsSummary([gpu, gpu])).toBe("Iris and 1 more");
    expect(sensorsSummary([])).toBe("No sensors");
    expect(
      sensorsSummary([
        { label: "a", celsius: 41.2, maxCelsius: null, criticalCelsius: null },
        { label: "b", celsius: 60.6, maxCelsius: null, criticalCelsius: null },
      ]),
    ).toBe("Hottest 61 °C");
    expect(storageSummary([])).toBe("0 volumes · 0 B");
    const net = (name: string, kind: "wifi" | "virtual", isPrimary: boolean) => ({
      name,
      kind,
      isPrimary,
      rxBytesPerSec: 0,
      txBytesPerSec: 0,
      totalRxBytes: 0,
      totalTxBytes: 0,
      ipv4: ["10.0.0.2"],
      ipv6: [],
      macAddress: null,
    });
    expect(
      networkSummary([net("docker0", "virtual", false), net("wlo1", "wifi", true)], false),
    ).toBe("wlo1 · 10.0.0.2");
    expect(networkSummary([], false)).toBe("No adapters");
    expect(STATIC_INFO.cpuBrand).toBeDefined();
  });
});
