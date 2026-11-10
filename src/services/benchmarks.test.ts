import { beforeEach, describe, expect, it, vi } from "vitest";
import { isActive, useBenchmarkStore } from "@/stores/benchmarkStore";
import { useToastStore } from "@/stores/toastStore";
import type { BenchmarkStatus, DiskTargetInfo } from "@/types";
import { cancelBenchmark, initBenchmarks, loadDiskTargets, runBenchmark } from "./benchmarks";

const api = vi.hoisted(() => ({
  cancelBenchmark: vi.fn<() => Promise<void>>(),
  startBenchmark: vi.fn<() => Promise<BenchmarkStatus>>(),
  getBenchmarkStatus: vi.fn<() => Promise<BenchmarkStatus>>(),
  getBenchmarkHistory: vi.fn(() => Promise.resolve([])),
  listDiskTargets: vi.fn<() => Promise<DiskTargetInfo[]>>(),
}));
vi.mock("@/services/ipc", async (orig) => ({
  ...(await orig<typeof import("@/services/ipc")>()),
  isBackendAvailable: () => true,
  backend: api,
}));
const listen = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/event", () => ({ listen }));

const IDLE = useBenchmarkStore.getState().status;
const running: BenchmarkStatus = {
  ...IDLE,
  state: "running",
  runId: "fixture",
  kind: "internet",
  fraction: 0.5,
  elapsedMs: 5000,
};

beforeEach(() => {
  Object.values(api).forEach((m) => m.mockReset());
  api.getBenchmarkHistory.mockResolvedValue([]);
  listen.mockReset();
  useToastStore.setState({ toasts: [] });
  useBenchmarkStore.setState({ status: IDLE, cancelling: false, diskTargets: null });
  useBenchmarkStore.getState().setStatus(running);
});

describe("cancelling", () => {
  it("shows cancelling until the worker finishes and retains the active state", async () => {
    let finish: () => void = () => {};
    api.cancelBenchmark.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const pending = cancelBenchmark();
    expect(useBenchmarkStore.getState().cancelling).toBe(true);
    expect(isActive(useBenchmarkStore.getState().status)).toBe(true);
    finish();
    await pending;
    const store = useBenchmarkStore.getState();
    store.setStatus({ ...store.status, stage: "Download" });
    expect(useBenchmarkStore.getState().cancelling).toBe(true);
    store.setStatus({ ...store.status, state: "cancelled" });
    expect(useBenchmarkStore.getState().cancelling).toBe(false);
  });

  it("allows retry if the cancellation request fails", async () => {
    api.cancelBenchmark.mockRejectedValueOnce(new Error("fixture failure"));
    await cancelBenchmark();
    expect(useBenchmarkStore.getState().cancelling).toBe(false);
    expect(isActive(useBenchmarkStore.getState().status)).toBe(true);
  });
});

describe("status ordering", () => {
  const status = () => useBenchmarkStore.getState().status;

  it("ignores an older snapshot of the same run", () => {
    useBenchmarkStore.getState().setStatus({ ...running, elapsedMs: 4000, fraction: 0.4 });
    expect(status().elapsedMs).toBe(5000);
    useBenchmarkStore.getState().setStatus({ ...running, elapsedMs: 6000, fraction: 0.6 });
    expect(status().elapsedMs).toBe(6000);
  });

  it("never returns a finished run to running", () => {
    useBenchmarkStore.getState().setStatus({ ...running, state: "completed", fraction: 1 });
    useBenchmarkStore.getState().setStatus({ ...running, elapsedMs: 9000 });
    expect(status().state).toBe("completed");
  });

  it("accepts any status of a different run", () => {
    useBenchmarkStore
      .getState()
      .setStatus({ ...running, runId: "next", elapsedMs: 0, fraction: 0 });
    expect(status().runId).toBe("next");
  });

  it("keeps newer progress events when the start reply arrives late", async () => {
    let reply: (s: BenchmarkStatus) => void = () => {};
    api.startBenchmark.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          reply = resolve;
        }),
    );
    useBenchmarkStore.setState({ status: IDLE });
    const started = runBenchmark("internet", "standard");
    useBenchmarkStore.getState().setStatus(running);
    reply({ ...running, state: "preparing", elapsedMs: 0, fraction: 0 });
    await started;
    expect(status()).toEqual(running);
  });
});

describe("initBenchmarks", () => {
  it("can be retried after subscribing to events fails, without leaking listeners", async () => {
    const unlisten = vi.fn();
    listen.mockResolvedValueOnce(unlisten).mockRejectedValueOnce(new Error("no event bus"));
    await initBenchmarks();
    expect(unlisten).toHaveBeenCalledTimes(1);
    expect(useToastStore.getState().toasts[0]?.message).toBe("no event bus");
    expect(api.getBenchmarkStatus).not.toHaveBeenCalled();

    listen.mockResolvedValue(vi.fn());
    api.getBenchmarkStatus.mockResolvedValue(running);
    await initBenchmarks();
    expect(listen).toHaveBeenCalledTimes(4);
    expect(api.getBenchmarkStatus).toHaveBeenCalledTimes(1);
  });
});

describe("loadDiskTargets", () => {
  const target = (id: string) => ({ id }) as DiskTargetInfo;

  it("lists once, and again only on reload, keeping the old list meanwhile", async () => {
    api.listDiskTargets.mockResolvedValueOnce([target("a")]);
    await loadDiskTargets();
    await loadDiskTargets();
    expect(api.listDiskTargets).toHaveBeenCalledTimes(1);

    let reply: (t: DiskTargetInfo[]) => void = () => {};
    api.listDiskTargets.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          reply = resolve;
        }),
    );
    const reloading = loadDiskTargets({ reload: true });
    expect(useBenchmarkStore.getState().diskTargets).toEqual([target("a")]);
    reply([target("b")]);
    await reloading;
    expect(useBenchmarkStore.getState().diskTargets).toEqual([target("b")]);
  });

  it("keeps the old list when a reload fails, and applies only the newest of overlapping reloads", async () => {
    useBenchmarkStore.setState({ diskTargets: [target("a")] });
    api.listDiskTargets.mockRejectedValueOnce({ kind: "systemQueryFailed", message: "boom" });
    await loadDiskTargets({ reload: true });
    expect(useBenchmarkStore.getState().diskTargets).toEqual([target("a")]);
    expect(useBenchmarkStore.getState().diskTargetsError).toBe("boom");

    let slow: (t: DiskTargetInfo[]) => void = () => {};
    api.listDiskTargets
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            slow = resolve;
          }),
      )
      .mockResolvedValueOnce([target("new")]);
    const first = loadDiskTargets({ reload: true });
    await loadDiskTargets({ reload: true });
    slow([target("old")]);
    await first;
    expect(useBenchmarkStore.getState().diskTargets).toEqual([target("new")]);
  });
});
