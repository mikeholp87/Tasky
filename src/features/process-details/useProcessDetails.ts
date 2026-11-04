import { useCallback, useEffect, useMemo, useState } from "react";
import { backend, isBackendAvailable, toAppError } from "@/services/ipc";
import { useMetricsStore } from "@/stores/metricsStore";
import type { AppError, ProcessDetails, ProcessPoint, ProcessRow } from "@/types";
import { MAX_PROCESS_POINTS, appendPoint, mergeHistory, toPoint } from "./history";

export interface ProcessDetailsState {
  /** Latest live row; null once the process has exited. */
  live: ProcessRow | null;
  /** Static details from the last fetch (kept after exit for context). */
  details: ProcessDetails | null;
  history: ProcessPoint[];
  loading: boolean;
  error: AppError | null;
  refresh: () => void;
}

interface FetchResult {
  key: string;
  details: ProcessDetails | null;
  error: AppError | null;
}

/**
 * Loads details for `pid` once, then extends CPU/memory history from the
 * regular metrics ticks (no extra polling). Callers key the consuming
 * component by PID so state never leaks between processes.
 */
export function useProcessDetails(pid: number): ProcessDetailsState {
  const [nonce, setNonce] = useState(0);
  const [result, setResult] = useState<FetchResult | null>(null);
  const [livePoints, setLivePoints] = useState<ProcessPoint[]>([]);
  const requestKey = `${pid}:${nonce}`;

  // Once details load they pin the process instance, so a reused PID is
  // reported as an exit rather than shown as this process.
  const startTime = result?.details?.row.startTime;
  const live = useMetricsStore(
    (s) =>
      s.tick?.processes.find(
        (p) => p.pid === pid && (startTime == null || p.startTime === startTime),
      ) ?? null,
  );

  useEffect(() => {
    if (!isBackendAvailable()) return;
    let cancelled = false;
    backend.getProcessDetails(pid).then(
      (details) => !cancelled && setResult({ key: requestKey, details, error: null }),
      (e: unknown) =>
        !cancelled &&
        setResult((prev) => ({
          key: requestKey,
          details: prev?.details ?? null,
          error: toAppError(e),
        })),
    );
    return () => {
      cancelled = true;
    };
  }, [pid, requestKey]);

  // Accumulate points from live ticks via a store subscription.
  useEffect(
    () =>
      useMetricsStore.subscribe((state, prev) => {
        const tick = state.tick;
        if (!tick || tick === prev.tick) return;
        const row = tick.processes.find(
          (p) => p.pid === pid && (startTime == null || p.startTime === startTime),
        );
        if (row) {
          setLivePoints((points) => appendPoint(points, toPoint(row, tick.timestampMs)));
        }
      }),
    [pid, startTime],
  );

  const details = result?.details ?? null;
  const history = useMemo(
    () => mergeHistory(details?.history ?? [], livePoints, MAX_PROCESS_POINTS),
    [details, livePoints],
  );
  const refresh = useCallback(() => setNonce((n) => n + 1), []);

  return {
    live,
    details,
    history,
    loading: isBackendAvailable() && result?.key !== requestKey,
    error: result?.error ?? null,
    refresh,
  };
}
