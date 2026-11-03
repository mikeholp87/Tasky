/**
 * Development-only preview: fills the stores with synthetic, animated data so
 * the interface can be reviewed in a plain browser (`npm run dev`, then open
 * `/?preview&theme=dark&route=overview`). Loaded only in dev builds.
 */
import { mockProcesses } from "@/features/processes/mock";
import { STATIC_INFO, makeTick } from "@/test/fixtures";
import { useMetricsStore } from "@/stores/metricsStore";
import { useSettingsStore, type ThemePreference } from "@/stores/settingsStore";
import { useUiStore } from "@/stores/uiStore";
import type { HistoryPoint, MetricsTick } from "@/types";
import type { RouteId } from "@/app/routes";

const CORES = 12;
const GIB = 1024 ** 3;

function wave(t: number, period: number, phase: number): number {
  return 0.5 + 0.5 * Math.sin((t / period) * 2 * Math.PI + phase);
}

function sample(tMs: number): { point: HistoryPoint; tick: MetricsTick } {
  const t = tMs / 1000;
  const perCore = Array.from({ length: CORES }, (_, i) => {
    const base = i === 3 ? 0.92 : 0.06 + 0.5 * wave(t, 40 + i * 7, i);
    return Math.min(100, Math.max(1, base * 100 + (Math.random() - 0.5) * 8));
  });
  const cpu = perCore.reduce((a, b) => a + b, 0) / CORES;
  const memPercent = 58 + 10 * wave(t, 300, 0);
  const point: HistoryPoint = {
    t: tMs,
    cpu,
    perCore,
    memPercent,
    memUsed: (memPercent / 100) * 32 * GIB,
    swapUsed: 0,
    netRx: 2.4e6 * wave(t, 25, 1) + 2e5,
    netTx: 4e5 * wave(t, 31, 2) + 3e4,
    diskRead: 8e6 * wave(t, 50, 3),
    diskWrite: 3e6 * wave(t, 37, 4),
  };
  const tick = makeTick(mockProcesses(220, Math.floor(tMs / 1000)), {
    timestampMs: tMs,
    cpu: {
      usagePercent: cpu,
      perCore,
      frequencyMhz: 2800 + Math.round(900 * wave(t, 20, 0)),
      loadAverage: { one: 2.31, five: 1.87, fifteen: 1.42 },
    },
    memory: {
      totalBytes: 32 * GIB,
      usedBytes: point.memUsed,
      availableBytes: 32 * GIB - point.memUsed,
      usedPercent: memPercent,
      cachedBytes: 11 * GIB,
      buffersBytes: 0.4 * GIB,
      swapTotalBytes: 8 * GIB,
      swapUsedBytes: 0.3 * GIB,
      commitUsedBytes: null,
      commitLimitBytes: null,
    },
    disks: [
      ["/dev/nvme0n1p1", "/", 228, 0.46],
      ["/dev/nvme0n1p2", "/home", 915, 0.81],
    ].map(([name, mount, gib, used]) => ({
      name: String(name),
      mountPoint: String(mount),
      fileSystem: "ext4",
      kind: "ssd" as const,
      removable: false,
      readOnly: false,
      totalBytes: Number(gib) * GIB,
      availableBytes: Number(gib) * GIB * (1 - Number(used)),
      usedBytes: Number(gib) * GIB * Number(used),
      usedPercent: Number(used) * 100,
      readBytesPerSec: point.diskRead,
      writeBytesPerSec: point.diskWrite,
      readOpsPerSec: 420,
      writeOpsPerSec: 130,
      activePercent: 14,
    })),
    networks: [
      ["wlo1", "wifi", true, ["192.168.0.23"]],
      ["tailscale0", "vpn", false, ["100.125.209.87"]],
    ].map(([name, kind, primary, ipv4]) => ({
      name: String(name),
      kind: kind as "wifi" | "vpn",
      isPrimary: Boolean(primary),
      rxBytesPerSec: point.netRx,
      txBytesPerSec: point.netTx,
      totalRxBytes: 48 * GIB,
      totalTxBytes: 6 * GIB,
      ipv4: ipv4 as string[],
      ipv6: [],
      macAddress: "aa:bb:cc:dd:ee:ff",
    })),
    gpus: [
      {
        id: "0000:00:02.0",
        name: "Raptor Lake-U [Intel Graphics]",
        vendor: "Intel Corporation",
        usagePercent: null,
        memoryUsedBytes: null,
        memoryTotalBytes: null,
      },
    ],
    temperatures: [
      { label: "coretemp Package id 0", celsius: 61, maxCelsius: 62, criticalCelsius: 100 },
      { label: "nvme Composite", celsius: 41, maxCelsius: 41, criticalCelsius: 94 },
    ],
    point,
  });
  return { point, tick };
}

const params = new URLSearchParams(location.search);
const metrics = useMetricsStore.getState();
const now = Date.now();
metrics.setStaticInfo({ ...STATIC_INFO, logicalCoreCount: CORES, physicalCoreCount: 10 });
metrics.backfillHistory(
  Array.from({ length: 600 }, (_, i) => sample(now - (600 - i) * 1000).point),
);

const theme = params.get("theme");
if (theme === "dark" || theme === "light") {
  useSettingsStore.setState({ theme: theme as ThemePreference });
}
const route = params.get("route");
if (route) useUiStore.getState().navigate(route as RouteId);

// Start after the app's own (no-backend) startup has set its status.
setTimeout(() => {
  const push = () => useMetricsStore.getState().ingestTick(sample(Date.now()).tick);
  push();
  setInterval(push, 1000);
}, 400);
