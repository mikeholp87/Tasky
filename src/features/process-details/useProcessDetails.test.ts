import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useMetricsStore } from "@/stores/metricsStore";
import { makeProcessRow, makeTick } from "@/test/fixtures";
import type { ProcessDetails } from "@/types";
import { useProcessDetails } from "./useProcessDetails";

const opened = makeProcessRow({ pid: 42, startTime: 1_000 });
const details = { row: opened, history: [] } as unknown as ProcessDetails;

vi.mock("@/services/ipc", async (orig) => ({
  ...(await orig<typeof import("@/services/ipc")>()),
  isBackendAvailable: () => true,
  backend: { getProcessDetails: () => Promise.resolve(details) },
}));

describe("useProcessDetails", () => {
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
});
