import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useRemoteData } from "./useRemoteData";

vi.mock("@/services/ipc", async (original) => ({
  ...(await original<typeof import("@/services/ipc")>()),
  isBackendAvailable: () => true,
}));

describe("inspection requests", () => {
  it("never shows the previous process's data after the new process query fails", async () => {
    const fetch = vi.fn<() => Promise<string>>().mockResolvedValueOnce("old process");
    const view = renderHook(({ key }) => useRemoteData(fetch, key), {
      initialProps: { key: "process:1" },
    });
    await waitFor(() => expect(view.result.current.data).toBe("old process"));
    fetch.mockRejectedValueOnce({
      kind: "permissionDenied",
      message: "Cannot inspect the new process.",
    });
    view.rerender({ key: "process:2" });
    await waitFor(() => expect(view.result.current.error?.kind).toBe("permissionDenied"));
    expect(view.result.current.data).toBeNull();
  });

  it("stops polling when the inspection tab is closed", async () => {
    vi.useFakeTimers();
    try {
      const fetch = vi.fn().mockResolvedValue("result");
      const view = renderHook(() => useRemoteData(fetch, "threads:1", 1_500));
      await act(async () => {});
      expect(fetch).toHaveBeenCalledTimes(1);
      view.unmount();
      await act(async () => {
        vi.advanceTimersByTime(10_000);
      });
      expect(fetch).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });
});
