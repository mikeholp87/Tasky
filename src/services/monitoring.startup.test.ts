import { beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  getStaticInfo: vi.fn(),
  getMonitoringState: vi.fn(),
  subscribeMetrics: vi.fn(),
  getHistory: vi.fn(),
  getRecentAlerts: vi.fn(),
  getActivity: vi.fn(),
  setMonitoringState: vi.fn(),
  refreshNow: vi.fn(),
  watchMonitoringState: vi.fn(),
}));
vi.mock("@/services/ipc", async (orig) => ({
  ...(await orig<typeof import("@/services/ipc")>()),
  isBackendAvailable: () => true,
  backend: api,
}));
vi.mock("./settings", () => ({ loadSettings: vi.fn(() => Promise.resolve()) }));

// `startMonitoring` connects once per page load, so each test gets fresh modules.
async function load() {
  vi.resetModules();
  const monitoring = await import("./monitoring");
  const { useToastStore } = await import("@/stores/toastStore");
  const { useMetricsStore } = await import("@/stores/metricsStore");
  return { ...monitoring, useToastStore, useMetricsStore };
}

const failure = { kind: "systemQueryFailed", message: "backend hiccup" };

beforeEach(() => {
  Object.values(api).forEach((m) => m.mockReset());
  api.getStaticInfo.mockResolvedValue({});
  api.getMonitoringState.mockResolvedValue({ paused: false, intervalMs: 1000 });
  api.subscribeMetrics.mockResolvedValue(null);
  api.getHistory.mockResolvedValue([]);
  api.getRecentAlerts.mockResolvedValue([]);
  api.getActivity.mockResolvedValue([]);
  api.watchMonitoringState.mockResolvedValue(() => {});
  Object.defineProperty(document, "visibilityState", { configurable: true, value: "visible" });
});

describe("startMonitoring", () => {
  it("reports a failed backfill but never subscribes twice and still catches up on show", async () => {
    const m = await load();
    api.getHistory.mockRejectedValueOnce(failure);
    await m.startMonitoring();

    expect(api.getRecentAlerts).toHaveBeenCalledTimes(1);
    expect(m.useToastStore.getState().toasts[0]?.message).toMatch(/backend hiccup/);
    await m.startMonitoring();
    expect(api.subscribeMetrics).toHaveBeenCalledTimes(1);

    document.dispatchEvent(new Event("visibilitychange"));
    expect(api.getHistory).toHaveBeenCalledTimes(2);
  });

  it("allows a retry when it fails before subscribing", async () => {
    const m = await load();
    api.getStaticInfo.mockRejectedValueOnce(failure);
    await m.startMonitoring();
    expect(m.useMetricsStore.getState().status).toBe("error");
    expect(api.subscribeMetrics).not.toHaveBeenCalled();

    await m.startMonitoring();
    expect(api.subscribeMetrics).toHaveBeenCalledTimes(1);
  });
});

describe("controls", () => {
  it("rolls back and shows a toast when pausing fails", async () => {
    const m = await load();
    api.setMonitoringState.mockRejectedValueOnce(failure);
    await m.togglePaused();
    expect(m.useMetricsStore.getState().monitoring.paused).toBe(false);
    expect(m.useToastStore.getState().toasts[0]).toMatchObject({
      tone: "error",
      message: "Could not change monitoring: backend hiccup",
    });
  });

  it("shows a toast when a manual refresh fails", async () => {
    const m = await load();
    api.refreshNow.mockRejectedValueOnce(failure);
    await m.refreshNow();
    expect(m.useToastStore.getState().toasts[0]?.message).toBe("Could not refresh: backend hiccup");
  });
});
