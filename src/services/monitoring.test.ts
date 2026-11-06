import { describe, expect, it, vi } from "vitest";
import type { MonitoringState } from "@/types";
import { makeTick, STATIC_INFO } from "@/test/fixtures";
import { useMetricsStore } from "@/stores/metricsStore";

const ipc = vi.hoisted(() => ({
  getStaticInfo: vi.fn(),
  getMonitoringState: vi.fn(),
  watchMonitoringState: vi.fn(),
  getActivity: vi.fn().mockResolvedValue([]),
  subscribeMetrics: vi.fn(),
  getHistory: vi.fn(),
  getRecentAlerts: vi.fn(),
  setMonitoringState: vi.fn(),
}));
vi.mock("./settings", () => ({ loadSettings: vi.fn().mockResolvedValue(undefined) }));
vi.mock("./ipc", async (original) => ({
  ...(await original<typeof import("./ipc")>()),
  backend: ipc,
  isBackendAvailable: () => true,
}));

describe("monitoring controls", () => {
  it("reflects tray pauses so the UI can resume a paused engine", async () => {
    let onState!: (state: MonitoringState) => void;
    ipc.watchMonitoringState.mockImplementation(async (listener) => {
      onState = listener;
      return () => {};
    });
    ipc.getStaticInfo.mockResolvedValue(STATIC_INFO);
    ipc.getMonitoringState.mockResolvedValue({ paused: false, intervalMs: 1000 });
    ipc.subscribeMetrics.mockResolvedValue(makeTick());
    ipc.getHistory.mockResolvedValue([]);
    ipc.getRecentAlerts.mockResolvedValue([]);
    ipc.setMonitoringState.mockImplementation(async (state) => state);
    useMetricsStore.setState({ tick: null });
    const { startMonitoring, togglePaused } = await import("./monitoring");
    await startMonitoring();
    onState({ paused: true, intervalMs: 2000 });
    expect(useMetricsStore.getState().monitoring).toEqual({ paused: true, intervalMs: 2000 });
    await togglePaused();
    expect(ipc.setMonitoringState).toHaveBeenCalledWith({ paused: false, intervalMs: 2000 });
    expect(useMetricsStore.getState().monitoring.paused).toBe(false);
  });
});
