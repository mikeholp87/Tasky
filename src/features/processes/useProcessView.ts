import { useMemo } from "react";
import { useMetricsStore } from "@/stores/metricsStore";
import { useProcessStore } from "@/stores/processStore";
import type { ProcessRow } from "@/types";
import { type ProcessContext, compareRows, isUnfiltered, makeMatcher, userOf } from "./logic";
import { flattenApps } from "./apps";
import { type TableItem, flattenTree } from "./tree";

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

/**
 * Items for the process table in the current mode. List and tree modes keep
 * process hierarchy, sort siblings and show matches with
 * their ancestors; applications mode groups processes by program. Recomputed once per tick.
 */
export function useProcessView() {
  const rows = useProcessRows();
  const ctx = useProcessContext(rows);
  const search = useProcessStore((s) => s.search);
  const preset = useProcessStore((s) => s.preset);
  const sort = useProcessStore((s) => s.sort);
  const mode = useProcessStore((s) => s.mode);
  const collapsed = useProcessStore((s) => s.collapsed);
  const expandedApps = useProcessStore((s) => s.expandedApps);

  const items = useMemo((): TableItem[] => {
    const options = { search, preset };
    if (mode === "apps") {
      return flattenApps(rows, {
        expanded: expandedApps,
        compare: (a, b) => compareRows(a, b, sort, ctx),
        matches: isUnfiltered(options) ? undefined : makeMatcher(options, ctx),
      });
    }
    return flattenTree(rows, {
      collapsed,
      compare: (a, b) => compareRows(a, b, sort, ctx),
      matches: isUnfiltered(options) ? undefined : makeMatcher(options, ctx),
    });
  }, [rows, search, preset, sort, ctx, mode, collapsed, expandedApps]);

  return { rows, items, ctx };
}
