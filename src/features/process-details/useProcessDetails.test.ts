import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useMetricsStore } from "@/stores/metricsStore";
import { makeProcessRow, makeTick } from "@/test/fixtures";
import type { ProcessDetails } from "@/types";
import { useProcessDetails } from "./useProcessDetails";

const opened = makeProcessRow({ pid: 42, startTime: 1_000 });
const details = { row: opened, history: [] } as unknown as ProcessDetails;
const getProcessDetails = vi.hoisted(() => vi.fn());

vi.mock("@/services/ipc", async (orig) => ({
  ...(await orig<typeof import("@/services/ipc")>()),
  isBackendAvailable: () => true,
  backend: { getProcessDetails },
}));

describe("useProcessDetails", () => {
  beforeEach(() => {
    getProcessDetails.mockReset().mockResolvedValue(details);
    useMetricsStore.setState({ tick: { ...makeTick(), processes: [opened] } });
  });
  it("treats a reused PID as an exit, not as the same process", async () => {
    const send = (startTime: number) =>
      act(() =>
        useMetricsStore.getState().ingestTick({
          ...makeTick(),
          processes: [{ ...opened, startTime }],
        }),
      );
    send(1_000);
    const { result } = renderHook(() => useProcessDetails(42));
    await waitFor(() => expect(result.current.details).not.toBeNull());
    expect(result.current.live).not.toBeNull();

    send(5_000); // same PID, different start time: a new process
    expect(result.current.live).toBeNull();
  });

  it("keeps the original instance and history when Refresh returns a reused PID", async () => {
    const { result } = renderHook(() => useProcessDetails(42));
    await waitFor(() => expect(result.current.details).toBe(details));
    act(() =>
      useMetricsStore.getState().ingestTick({
        ...makeTick(),
        timestampMs: 10_000,
        processes: [opened],
      }),
    );
    const replacement = { ...opened, startTime: 5_000, name: "replacement" };
    act(() =>
      useMetricsStore.getState().ingestTick({
        ...makeTick(),
        timestampMs: 11_000,
        processes: [replacement],
      }),
    );
    getProcessDetails.mockResolvedValue({ row: replacement, history: [] });
    act(() => result.current.refresh());
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.live).toBeNull();
    expect(result.current.details).toBe(details);
    expect(result.current.error?.kind).toBe("processNotFound");
    expect(getProcessDetails).toHaveBeenLastCalledWith(42, 1_000);
    expect(result.current.history.map((p) => p.t)).toContain(10_000);
    expect(result.current.history.map((p) => p.t)).not.toContain(11_000);
  });

  it("pins the selected tick before the first request resolves", async () => {
    const replacement = { ...opened, startTime: 5_000 };
    getProcessDetails.mockResolvedValue({ row: replacement, history: [] });
    const { result } = renderHook(() => useProcessDetails(42));
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.details).toBeNull();
    expect(result.current.error?.kind).toBe("processNotFound");
  });

  it("reports an unknown start time as not found without fetching or following ticks", () => {
    useMetricsStore.setState({ tick: makeTick([]) });
    const { result } = renderHook(() => useProcessDetails(42));
    act(() =>
      useMetricsStore.getState().ingestTick({
        ...makeTick(),
        timestampMs: 12_000,
        processes: [{ ...opened, startTime: 5_000 }],
      }),
    );
    expect(getProcessDetails).not.toHaveBeenCalled();
    expect(result.current.loading).toBe(false);
    expect(result.current.error?.kind).toBe("processNotFound");
    expect(result.current.details).toBeNull();
    expect(result.current.live).toBeNull();
    expect(result.current.history).toEqual([]);
  });

  it("asks for the exact instance it was opened with", async () => {
    const { result } = renderHook(() => useProcessDetails(42, 1_000));
    await waitFor(() => expect(result.current.details).toBe(details));
    expect(getProcessDetails).toHaveBeenCalledWith(42, 1_000);
  });

  it("never follows replacement ticks with unknown zero creation times", async () => {
    const unknown = { ...opened, startTime: 0 };
    useMetricsStore.setState({ tick: { ...makeTick(), processes: [unknown] } });
    getProcessDetails.mockResolvedValue({ row: unknown, history: [] });
    const { result } = renderHook(() => useProcessDetails(42));
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.error?.kind).toBe("actionUnavailable");
    expect(result.current.live).toBeNull();
    act(() =>
      useMetricsStore.getState().ingestTick({
        ...makeTick(),
        timestampMs: 13_000,
        processes: [{ ...unknown, name: "replacement" }],
      }),
    );
    expect(result.current.live).toBeNull();
    expect(result.current.details).toBeNull();
    expect(result.current.history).toEqual([]);
  });
});
