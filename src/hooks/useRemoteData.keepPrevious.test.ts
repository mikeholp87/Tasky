import { act, renderHook, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { useRemoteData } from "./useRemoteData";

vi.mock("@/services/ipc", async (orig) => ({
  ...(await orig<typeof import("@/services/ipc")>()),
  isBackendAvailable: () => true,
}));

/** A fetcher per call whose results the test releases in any order. */
function deferredCalls() {
  const resolvers: ((value: string) => void)[] = [];
  const fetcher = vi.fn(
    () =>
      new Promise<string>((resolve) => {
        resolvers.push(resolve);
      }),
  );
  return { fetcher, resolve: (call: number, value: string) => resolvers[call]?.(value) };
}

it("drops the previous key's data by default", async () => {
  const { fetcher, resolve } = deferredCalls();
  const { result, rerender } = renderHook(({ k }) => useRemoteData(fetcher, k), {
    initialProps: { k: "a" },
  });
  act(() => resolve(0, "A"));
  await waitFor(() => expect(result.current.data).toBe("A"));
  rerender({ k: "b" });
  expect(result.current.data).toBeNull();
  expect(result.current.loading).toBe(true);
});

it("keeps the previous key's data until the new key's arrives with keepPreviousData", async () => {
  const { fetcher, resolve } = deferredCalls();
  const { result, rerender } = renderHook(
    ({ k }) => useRemoteData(fetcher, k, undefined, { keepPreviousData: true }),
    { initialProps: { k: "a" } },
  );
  act(() => resolve(0, "A"));
  await waitFor(() => expect(result.current.data).toBe("A"));
  rerender({ k: "b" });
  expect(result.current.data).toBe("A");
  expect(result.current.loading).toBe(false);
  act(() => resolve(1, "B"));
  await waitFor(() => expect(result.current.data).toBe("B"));
});

it("ignores a slower earlier call that resolves after a newer one", async () => {
  const { fetcher, resolve } = deferredCalls();
  const { result, rerender } = renderHook(
    ({ k }) => useRemoteData(fetcher, k, undefined, { keepPreviousData: true }),
    { initialProps: { k: "a" } },
  );
  rerender({ k: "b" });
  act(() => resolve(1, "B"));
  await waitFor(() => expect(result.current.data).toBe("B"));
  act(() => resolve(0, "A"));
  await Promise.resolve();
  expect(result.current.data).toBe("B");
});
