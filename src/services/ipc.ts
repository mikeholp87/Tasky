import type { Volume, ServiceAction, ServiceInfo, ServiceScope } from "@/types";
/**
 * The only module that talks to the Rust backend. Components and stores call
 * these typed wrappers; they never use `invoke` directly.
 */
import { Channel, invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  ActivityEvent,
  AppError,
  BenchmarkCapabilities,
  BenchmarkComparison,
  BenchmarkEvent,
  BenchmarkRecord,
  BenchmarkRequest,
  BenchmarkSummary,
  NetworkProgress,
  NetworkSubmission,
  PreflightReport,
  ErrorKind,
  FileUserList,
  HardwareInfo,
  HistoryPoint,
  MemoryMap,
  MetricsTick,
  ModuleInfo,
  MonitoringState,
  ActionOutcome,
  OpenFileList,
  ProcessAction,
  ProcessDetails,
  ProtectionAssessment,
  AlertEvent,
  Settings,
  SocketList,
  StartupItem,
  StaticSystemInfo,
  ThreadList,
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
  operationCancelled: true,
  operationBusy: true,
  engineUnavailable: true,
  benchmarkAlreadyRunning: true,
  benchmarkCancelled: true,
  gpuUnavailable: true,
  gpuUnsupportedFeature: true,
  gpuInitializationFailed: true,
  insufficientDiskSpace: true,
  diskPermissionDenied: true,
  networkUnavailable: true,
  networkTimeout: true,
  providerUnavailable: true,
  unsupportedPlatform: true,
  invalidBenchmarkConfiguration: true,
  thermalSensorUnavailable: true,
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
  listServices: (scope: ServiceScope) => call<ServiceInfo[]>("list_services", { scope }),
  controlService: (scope: ServiceScope, unit: string, action: ServiceAction) =>
    call<void>("control_service", { scope, unit, action }),
  listVolumes: () => call<Volume[]>("list_volumes"),
  mountVolume: (device: string) => call<string | null>("mount_volume", { device }),
  unmountVolume: (device: string) => call<void>("unmount_volume", { device }),
  powerOffVolume: (device: string) => call<void>("power_off_volume", { device }),
  getStaticInfo: () => call<StaticSystemInfo>("get_static_info"),

  getHistory: (seconds: number) => call<HistoryPoint[]>("get_history", { seconds }),

  getMonitoringState: () => call<MonitoringState>("get_monitoring_state"),
  watchMonitoringState: (onState: (state: MonitoringState) => void) =>
    listen<MonitoringState>("monitoring-state-changed", (event) => onState(event.payload)),

  setMonitoringState: (state: MonitoringState) =>
    call<MonitoringState>("set_monitoring_state", {
      paused: state.paused,
      intervalMs: state.intervalMs,
    }),

  refreshNow: () => call<void>("refresh_now"),

  setMemoryDetail: (enabled: boolean) => call<void>("set_memory_detail", { enabled }),

  getProcessDetails: (pid: number, startTime: number) =>
    call<ProcessDetails>("get_process_details", { pid, startTime }),

  assessProcessAction: (pid: number, action: ProcessAction) =>
    call<ProtectionAssessment>("assess_process_action", { pid, action }),

  performProcessAction: (
    pid: number,
    startTime: number,
    action: ProcessAction,
    acknowledgeWarnings: boolean,
  ) =>
    call<ActionOutcome>("perform_process_action", { pid, startTime, action, acknowledgeWarnings }),

  /** On-demand inspection; each call walks OS tables, so call only while its view is open. */
  listSockets: () => call<SocketList>("list_sockets"),
  listStartupItems: () => call<StartupItem[]>("list_startup_items"),
  setStartupEnabled: (id: string, enabled: boolean) =>
    call<void>("set_startup_enabled", { id, enabled }),
  getOpenFiles: (pid: number, startTime: number) =>
    call<OpenFileList>("get_open_files", { pid, startTime }),
  findFileUsers: (path: string) => call<FileUserList>("find_file_users", { path }),
  getMemoryMap: (pid: number, startTime: number) =>
    call<MemoryMap>("get_memory_map", { pid, startTime }),
  getModules: (pid: number, startTime: number) =>
    call<ModuleInfo[]>("get_modules", { pid, startTime }),
  getThreads: (pid: number, startTime: number) =>
    call<ThreadList>("get_threads", { pid, startTime }),
  /** Native file/folder picker; resolves to the chosen path or null if cancelled. */
  pickPath: (directory: boolean) => call<string | null>("pick_path", { directory }),
  getHardwareInfo: () => call<HardwareInfo>("get_hardware_info"),
  getActivity: (after: number) => call<ActivityEvent[]>("get_activity", { after }),

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

  getBenchmarkCapabilities: () => call<BenchmarkCapabilities>("get_benchmark_capabilities"),
  runBenchmarkPreflight: (request: BenchmarkRequest) =>
    call<PreflightReport>("run_benchmark_preflight", { request }),
  /** Starts a run; events stream to `onEvent`. Resolves with the run id. */
  startBenchmark: (request: BenchmarkRequest, onEvent: (event: BenchmarkEvent) => void) => {
    const channel = new Channel<BenchmarkEvent>();
    channel.onmessage = onEvent;
    return call<string>("start_benchmark", { request, onEvent: channel });
  },
  cancelBenchmark: (runId: string) => call<boolean>("cancel_benchmark", { runId }),
  skipBenchmarkCooldown: (runId: string) => call<boolean>("skip_benchmark_cooldown", { runId }),
  listBenchmarkHistory: () => call<BenchmarkSummary[]>("list_benchmark_history"),
  getBenchmarkResult: (id: string) => call<BenchmarkRecord>("get_benchmark_result", { id }),
  deleteBenchmarkResult: (id: string) => call<void>("delete_benchmark_result", { id }),
  reportNetworkProgress: (token: string, progress: NetworkProgress) =>
    call<void>("report_network_progress", { token, progress }),
  submitNetworkSamples: (token: string, samples: NetworkSubmission) =>
    call<void>("submit_network_samples", { token, samples }),
  compareBenchmarks: (baselineId: string, currentId: string) =>
    call<BenchmarkComparison>("compare_benchmarks", { baselineId, currentId }),
  /** Opens a native save dialog; resolves to the path, or null if cancelled. */
  exportBenchmark: (id: string, format: "json" | "csv" | "markdown") =>
    call<string | null>("export_benchmark", { id, format }),
  benchmarkSummaryText: (id: string) => call<string>("benchmark_summary_text", { id }),
};
