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
  ActionOutcome,
  ProcessAction,
  ProcessDetails,
  ProtectionAssessment,
  AlertEvent,
  Settings,
  StaticSystemInfo,
} from "@/types";

export const isBackendAvailable = (): boolean => isTauri();

// A Record (not an array) so adding a kind in Rust is a compile error here.
const ERROR_KINDS: Record<ErrorKind, true> = {
  permissionDenied: true,
  processNotFound: true,
  unsupportedOperation: true,
  systemQueryFailed: true,
  invalidPid: true,
  invalidArgument: true,
  processProtected: true,
  actionUnavailable: true,
  engineUnavailable: true,
};

function isAppError(value: unknown): value is AppError {
  if (typeof value !== "object" || value === null) return false;
  const v = value as Record<string, unknown>;
  return (
    typeof v.message === "string" &&
    typeof v.kind === "string" &&
    Object.hasOwn(ERROR_KINDS, v.kind)
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

  getProcessDetails: (pid: number) => call<ProcessDetails>("get_process_details", { pid }),

  assessProcessAction: (pid: number, action: ProcessAction) =>
    call<ProtectionAssessment>("assess_process_action", { pid, action }),

  performProcessAction: (
    pid: number,
    startTime: number,
    action: ProcessAction,
    acknowledgeWarnings: boolean,
  ) =>
    call<ActionOutcome>("perform_process_action", { pid, startTime, action, acknowledgeWarnings }),

  revealExecutable: (pid: number) => call<void>("reveal_executable", { pid }),

  getSettings: () => call<Settings>("get_settings"),
  updateSettings: (settings: Settings) => call<Settings>("update_settings", { settings }),
  resetSettings: () => call<Settings>("reset_settings"),
  getRecentAlerts: () => call<AlertEvent[]>("get_recent_alerts"),

  /** Each export opens a native save dialog; resolves to the path, or null if cancelled. */
  exportProcesses: (format: "csv" | "json", includeCommandLines: boolean) =>
    call<string | null>("export_processes", { format, includeCommandLines }),
  exportSystemSummary: () => call<string | null>("export_system_summary"),
  exportDiagnostics: (includeProcesses: boolean, includeCommandLines: boolean) =>
    call<string | null>("export_diagnostics", { includeProcesses, includeCommandLines }),

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
