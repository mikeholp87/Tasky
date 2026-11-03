import type { MetricsTick, ProcessRow, StaticSystemInfo } from "@/types";

export const NOW_MS = 1_800_000_000_000;

export function makeTick(
  processes: ProcessRow[] = [],
  overrides: Partial<MetricsTick> = {},
): MetricsTick {
  return {
    seq: 1,
    timestampMs: NOW_MS,
    elapsedMs: 1000,
    cpu: { usagePercent: 12, perCore: [10, 14], frequencyMhz: 3000, loadAverage: null },
    memory: {
      totalBytes: 16 * 1024 ** 3,
      usedBytes: 8 * 1024 ** 3,
      availableBytes: 8 * 1024 ** 3,
      usedPercent: 50,
      cachedBytes: null,
      buffersBytes: null,
      swapTotalBytes: 0,
      swapUsedBytes: 0,
      commitUsedBytes: null,
      commitLimitBytes: null,
    },
    disks: [],
    networks: [],
    processes,
    gpus: [],
    temperatures: [],
    alerts: [],
    point: {
      t: NOW_MS,
      cpu: 12,
      perCore: [10, 14],
      memPercent: 50,
      memUsed: 8 * 1024 ** 3,
      swapUsed: 0,
      netRx: 0,
      netTx: 0,
      diskRead: 0,
      diskWrite: 0,
    },
    engine: {
      pollDurationMs: 5,
      intervalMs: 1000,
      processCount: processes.length,
      lastPayloadBytes: 0,
    },
    ...overrides,
  };
}

export const STATIC_INFO: StaticSystemInfo = {
  platform: "linux",
  osName: "Debian GNU/Linux",
  osVersion: "13",
  longOsVersion: "Linux 13 Debian GNU/Linux",
  kernelVersion: "6.12.0",
  hostname: "test-host",
  architecture: "x86_64",
  cpuBrand: "Test CPU",
  cpuVendor: "TestVendor",
  physicalCoreCount: 2,
  logicalCoreCount: 2,
  totalMemoryBytes: 16 * 1024 ** 3,
  bootTimeSecs: NOW_MS / 1000 - 3600,
  appVersion: "0.1.0",
  selfPid: 100,
  distribution: "Debian GNU/Linux 13 (trixie)",
  desktopEnvironment: "KDE Plasma",
  displayServer: "Wayland",
  container: null,
  initSystem: "systemd",
};
