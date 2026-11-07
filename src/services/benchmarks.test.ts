import { beforeEach, expect, it, vi } from "vitest";
import { isActive, useBenchmarkStore } from "@/stores/benchmarkStore";
import { cancelBenchmark } from "./benchmarks";

const cancel = vi.hoisted(() => vi.fn<() => Promise<void>>());
vi.mock("@/services/ipc", async (orig) => ({
  ...(await orig<typeof import("@/services/ipc")>()),
  backend: { cancelBenchmark: cancel },
}));

beforeEach(() => {
  cancel.mockReset();
  const store = useBenchmarkStore.getState();
  store.setCancelling(false);
  store.setStatus({ ...store.status, state: "running", runId: "fixture", kind: "internet" });
});

it("shows cancelling until the worker finishes and retains the active state", async () => {
  let finish: () => void = () => {};
  cancel.mockImplementationOnce(
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
  cancel.mockRejectedValueOnce(new Error("fixture failure"));
  await cancelBenchmark();
  expect(useBenchmarkStore.getState().cancelling).toBe(false);
  expect(isActive(useBenchmarkStore.getState().status)).toBe(true);
});
