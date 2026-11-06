import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ProcessDetails } from "@/types";
import { useMetricsStore } from "@/stores/metricsStore";
import { makeTick, NOW_MS } from "@/test/fixtures";
import { mockProcesses } from "@/features/processes/mock";
import { useProcessDetails } from "./useProcessDetails";

const ipc = vi.hoisted(() => ({ getProcessDetails: vi.fn() }));
vi.mock("@/services/ipc", async (original) => ({
  ...(await original<typeof import("@/services/ipc")>()),
  isBackendAvailable: () => true,
  backend: ipc,
}));

const row = mockProcesses(1, NOW_MS / 1000)[0]!;
const details: ProcessDetails = {
  row,
  parentName: null,
  cwd: null,
  architecture: null,
  totalReadBytes: null,
  totalWrittenBytes: null,
  connections: null,
  history: [],
};

describe("process lifetime details", () => {
  beforeEach(() => {
    ipc.getProcessDetails.mockReset().mockResolvedValue(details);
    useMetricsStore.setState({ tick: makeTick([row]) });
  });

  it("keeps exited details without mixing in a replacement process", async () => {
    const { result } = renderHook(() => useProcessDetails(row.pid, row.startTime));
    await waitFor(() => expect(result.current.details).toEqual(details));
    act(() =>
      useMetricsStore.setState({
        tick: makeTick([{ ...row, startTime: row.startTime + 1, cpuPercent: 99 }], {
          seq: 2,
          timestampMs: NOW_MS + 1000,
        }),
      }),
    );
    expect(result.current.live).toBeNull();
    expect(result.current.details?.row.startTime).toBe(row.startTime);
    expect(result.current.history).toEqual([]);
  });

  it("rejects a mismatched IPC result", async () => {
    ipc.getProcessDetails.mockResolvedValue({
      ...details,
      row: { ...row, startTime: row.startTime + 1 },
    });
    const { result } = renderHook(() => useProcessDetails(row.pid, row.startTime));
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.details).toBeNull();
    expect(result.current.error?.kind).toBe("processNotFound");
  });

  it("ignores a response after unmount", async () => {
    let resolve!: (d: ProcessDetails) => void;
    ipc.getProcessDetails.mockReturnValue(
      new Promise<ProcessDetails>((r) => {
        resolve = r;
      }),
    );
    const { unmount } = renderHook(() => useProcessDetails(row.pid, row.startTime));
    unmount();
    await act(async () => {
      resolve(details);
    });
    expect(ipc.getProcessDetails).toHaveBeenCalledWith(row.pid, row.startTime);
  });
});
