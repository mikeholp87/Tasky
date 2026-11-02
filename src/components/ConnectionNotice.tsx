import { useMetricsStore } from "@/stores/metricsStore";
import { EmptyState } from "./EmptyState";

/**
 * Shown in place of live content when there is no data yet. Returns null once
 * the first tick has arrived.
 */
export function ConnectionNotice() {
  const status = useMetricsStore((s) => s.status);
  const error = useMetricsStore((s) => s.error);
  const hasTick = useMetricsStore((s) => s.tick !== null);
  if (hasTick) return null;

  if (status === "unavailable") {
    return (
      <EmptyState title="Monitoring backend not available">
        The interface is running outside the desktop app, so no system data can be read.
      </EmptyState>
    );
  }
  if (status === "error") {
    return <EmptyState title="Unable to start monitoring">{error?.message}</EmptyState>;
  }
  return <EmptyState title="Collecting first sample…" />;
}
