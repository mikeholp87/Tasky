import { useMemo } from "react";
import { useMetricsStore } from "@/stores/metricsStore";
import { useProcessStore } from "@/stores/processStore";
import type { ProcessRow } from "@/types";
import { type ProcessContext, processView, userOf } from "./logic";

const EMPTY: readonly ProcessRow[] = [];

/** Raw rows from the latest tick. */
export function useProcessRows(): readonly ProcessRow[] {
  return useMetricsStore((s) => s.tick?.processes ?? EMPTY);
}

/** Context values needed by sorting/filtering, derived from the latest tick. */
export function useProcessContext(rows: readonly ProcessRow[]): ProcessContext {
  const totalMemoryBytes = useMetricsStore((s) => s.tick?.memory.totalBytes ?? 0);
  const timestampMs = useMetricsStore((s) => s.tick?.timestampMs ?? 0);
  const selfPid = useMetricsStore((s) => s.staticInfo?.selfPid);
  const currentUser = useMemo(() => userOf(rows, selfPid), [rows, selfPid]);
  return useMemo(
    () => ({ totalMemoryBytes, nowSecs: Math.floor(timestampMs / 1000), currentUser }),
    [totalMemoryBytes, timestampMs, currentUser],
  );
}

/** Filtered and sorted rows for the process table. Recomputed once per tick. */
export function useProcessView() {
  const rows = useProcessRows();
  const ctx = useProcessContext(rows);
  const search = useProcessStore((s) => s.search);
  const preset = useProcessStore((s) => s.preset);
  const sort = useProcessStore((s) => s.sort);
  const view = useMemo(
    () => processView(rows, { search, preset, sort }, ctx),
    [rows, search, preset, sort, ctx],
  );
  return { rows, view, ctx };
}
