import { useCallback, useEffect, useRef, useState } from "react";
import { isBackendAvailable, toAppError } from "@/services/ipc";
import type { AppError } from "@/types";

export interface RemoteData<T> {
  /** Latest successful result; kept while a refresh or a failed poll is pending. */
  data: T | null;
  error: AppError | null;
  /** True until the first result (or error) arrives. */
  loading: boolean;
  refresh: () => void;
}

export interface RemoteDataOptions {
  /**
   * Keep showing the previous key's data until the new key's arrives, for
   * keys that mean "reload" rather than "different data".
   */
  keepPreviousData?: boolean;
}

/**
 * Loads data with `fetcher` when the component mounts (and whenever `key`
 * changes), optionally repeating every `pollMs`. Polling stops on unmount, so
 * expensive inspection runs only while its view is on screen. A poll never
 * starts while the previous call is still running, and polls pause while the
 * window is hidden.
 */
export function useRemoteData<T>(
  fetcher: () => Promise<T>,
  key: string,
  pollMs?: number,
  { keepPreviousData = false }: RemoteDataOptions = {},
): RemoteData<T> {
  const [state, setState] = useState<{ key: string; data: T | null; error: AppError | null }>({
    key,
    data: null,
    error: null,
  });
  const [nonce, setNonce] = useState(0);
  // Always call the newest fetcher without re-running the effect on every render.
  const latest = useRef(fetcher);
  // Effects run in order, so this is current before the fetch effect below reads it.
  useEffect(() => {
    latest.current = fetcher;
  });

  useEffect(() => {
    if (!isBackendAvailable()) return;
    let cancelled = false;
    let inFlight = false;
    const run = () => {
      if (inFlight) return;
      inFlight = true;
      latest.current().then(
        (data) => {
          inFlight = false;
          if (!cancelled) setState({ key, data, error: null });
        },
        (e: unknown) => {
          inFlight = false;
          if (!cancelled)
            setState((prev) => ({
              key,
              data: keepPreviousData || prev.key === key ? prev.data : null,
              error: toAppError(e),
            }));
        },
      );
    };
    run();
    const timer =
      pollMs == null
        ? undefined
        : setInterval(() => {
            if (document.visibilityState === "visible") run();
          }, pollMs);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [key, pollMs, nonce, keepPreviousData]);

  const refresh = useCallback(() => setNonce((n) => n + 1), []);
  // Results of a previous key are stale for this one.
  const current =
    state.key === key ? state : { data: keepPreviousData ? state.data : null, error: null };
  return {
    data: current.data,
    error: current.error,
    loading: isBackendAvailable() && current.data == null && current.error == null,
    refresh,
  };
}
