/**
 * Monitoring lifecycle and controls. Keeps backend coordination out of React
 * components: components call these functions and read the stores.
 */
import { useMetricsStore } from "@/stores/metricsStore";
import type { MonitoringState } from "@/types";
import { backend, isBackendAvailable, toAppError } from "./ipc";

/** Seconds of history fetched at startup (the longest chart range). */
const BACKFILL_SECONDS = 3_600;

let started = false;

/** Connects to the engine once per page load. Safe to call repeatedly. */
export async function startMonitoring(): Promise<void> {
  if (started) return;
  started = true;
  const store = useMetricsStore.getState();

  if (!isBackendAvailable()) {
    store.setStatus("unavailable");
    return;
  }

  try {
    const [info, state] = await Promise.all([
      backend.getStaticInfo(),
      backend.getMonitoringState(),
    ]);
    store.setStaticInfo(info);
    store.setMonitoring(state);

    const latest = await backend.subscribeMetrics((tick) =>
      useMetricsStore.getState().ingestTick(tick),
    );
    if (latest) store.ingestTick(latest);

    // Backfill after subscribing so no tick falls into a gap.
    store.backfillHistory(await backend.getHistory(BACKFILL_SECONDS));
  } catch (error) {
    started = false;
    store.setStatus("error", toAppError(error));
  }
}

async function applyState(next: MonitoringState): Promise<void> {
  const store = useMetricsStore.getState();
  const previous = store.monitoring;
  store.setMonitoring(next); // optimistic
  try {
    store.setMonitoring(await backend.setMonitoringState(next));
  } catch (error) {
    store.setMonitoring(previous);
    store.setStatus(store.status, toAppError(error));
  }
}

export function togglePaused(): Promise<void> {
  const { monitoring } = useMetricsStore.getState();
  return applyState({ ...monitoring, paused: !monitoring.paused });
}

export function setRefreshInterval(intervalMs: number): Promise<void> {
  const { monitoring } = useMetricsStore.getState();
  return applyState({ ...monitoring, intervalMs });
}

export async function refreshNow(): Promise<void> {
  if (!isBackendAvailable()) return;
  try {
    await backend.refreshNow();
  } catch (error) {
    const store = useMetricsStore.getState();
    store.setStatus(store.status, toAppError(error));
  }
}
