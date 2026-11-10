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
export function useProcessDetails(
  pid: number,
  selectedStartTime?: number | null,
): ProcessDetailsState {
  const [nonce, setNonce] = useState(0);
  const [result, setResult] = useState<FetchResult | null>(null);
  const [livePoints, setLivePoints] = useState<ProcessPoint[]>([]);
  const requestKey = `${pid}:${nonce}`;
  // The start time pins the process instance, so a reused PID is reported as
  // an exit rather than shown as this process. Without one (the process was
  // gone before the page opened) there is nothing that can be verified.
  const [startTime] = useState(
    () =>
      selectedStartTime ??
      useMetricsStore.getState().tick?.processes.find((p) => p.pid === pid)?.startTime,
  );
  const missing = useMemo(() => (startTime == null ? notFound(pid) : null), [pid, startTime]);
  const live = useMetricsStore(
    (s) =>
      s.tick?.processes.find(
        (p) => p.pid === pid && startTime != null && startTime > 0 && p.startTime === startTime,
      ) ?? null,
  );

  useEffect(() => {
    if (!isBackendAvailable() || startTime == null) return;
    let cancelled = false;
    backend
      .getProcessDetails(pid, startTime)
      .then((details) => {
        if (details.row.startTime <= 0) {
          throw {
            kind: "actionUnavailable",
            message: "The process start time is unavailable, so its identity cannot be verified.",
          } satisfies AppError;
        }
        if (details.row.startTime !== startTime) throw notFound(pid);
        if (!cancelled) setResult({ key: requestKey, details, error: null });
      })
      .catch(
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
  }, [pid, requestKey, startTime]);

  // Accumulate points from live ticks via a store subscription.
  useEffect(
    () =>
      useMetricsStore.subscribe((state, prev) => {
        const tick = state.tick;
        if (!tick || tick === prev.tick) return;
        const row = tick.processes.find(
          (p) => p.pid === pid && startTime != null && p.startTime > 0 && p.startTime === startTime,
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
    loading: !missing && isBackendAvailable() && result?.key !== requestKey,
    error: missing ?? result?.error ?? null,
    refresh,
  };
}

function notFound(pid: number): AppError {
  return { kind: "processNotFound", message: `Process ${pid} is no longer running.` };
}
