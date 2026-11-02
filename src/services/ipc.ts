/**
 * The only module that talks to the Rust backend. Components and stores call
 * these typed wrappers; they never use `invoke` directly.
 */
import { Channel, invoke, isTauri } from "@tauri-apps/api/core";
import type {
  AppError,
  ErrorKind,
  HistoryPoint,
  MetricsTick,
  MonitoringState,
  StaticSystemInfo,
} from "@/types";

export const isBackendAvailable = (): boolean => isTauri();

const ERROR_KINDS: readonly ErrorKind[] = [
  "permissionDenied",
  "processNotFound",
  "unsupportedOperation",
  "systemQueryFailed",
  "invalidPid",
  "invalidArgument",
  "processProtected",
  "engineUnavailable",
];

function isAppError(value: unknown): value is AppError {
  if (typeof value !== "object" || value === null) return false;
  const v = value as Record<string, unknown>;
  return (
    typeof v.message === "string" &&
    typeof v.kind === "string" &&
    (ERROR_KINDS as readonly string[]).includes(v.kind)
  );
}

/** Normalises anything thrown by `invoke` into an {@link AppError}. */
export function toAppError(error: unknown): AppError {
  if (isAppError(error)) return error;
  const message =
    typeof error === "string"
      ? error
      : error instanceof Error
        ? error.message
        : "An unexpected error occurred.";
  return { kind: "systemQueryFailed", message };
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (error) {
    throw toAppError(error);
  }
}

export const backend = {
  getStaticInfo: () => call<StaticSystemInfo>("get_static_info"),

  getHistory: (seconds: number) => call<HistoryPoint[]>("get_history", { seconds }),

  getMonitoringState: () => call<MonitoringState>("get_monitoring_state"),

  setMonitoringState: (state: MonitoringState) =>
    call<MonitoringState>("set_monitoring_state", {
      paused: state.paused,
      intervalMs: state.intervalMs,
    }),

  refreshNow: () => call<void>("refresh_now"),

  /**
   * Streams metrics ticks to `onTick`. Resolves with the latest tick already
   * collected (if any) so the UI can render immediately.
   */
  subscribeMetrics: (onTick: (tick: MetricsTick) => void) => {
    const channel = new Channel<MetricsTick>();
    channel.onmessage = onTick;
    return call<MetricsTick | null>("subscribe_metrics", { onTick: channel });
  },
};
