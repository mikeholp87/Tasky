/**
 * Preview-only stand-in for the Rust backend, so the whole interface (not
 * just the data stores) can be reviewed in a plain browser. It simulates a
 * benchmark run with progress events and saves fake results in memory.
 * Loaded only by `dev/preview.ts`, which is never part of a production build.
 */
import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
import { DEFAULT_SETTINGS } from "@/stores/settingsStore";
import type {
  BenchmarkProfile,
  BenchmarkResult,
  BenchmarkStatus,
  MetricsTick,
  StaticSystemInfo,
  WorkloadId,
  WorkloadResult,
} from "@/types";

const IDLE: BenchmarkStatus = {
  state: "idle",
  runId: null,
  kind: null,
  profile: null,
  stage: "",
  fraction: 0,
  live: null,
  elapsedMs: 0,
  message: null,
  cpuPercent: null,
  frequencyMhz: null,
  temperatureC: null,
  developmentBuild: false,
};

const REF: Record<WorkloadId, [number, number, string]> = {
  integer: [14_000, 60_000, "Mops/s"],
  floatingPoint: [11_000, 62_000, "Mflops/s"],
  hash: [1_950, 12_800, "MiB/s"],
  compress: [14, 57, "MiB/s"],
  decompress: [415, 2_100, "MiB/s"],
};

function workload(id: WorkloadId, multi: boolean, factor: number): WorkloadResult {
  const [single, many, unit] = REF[id];
  const median = (multi ? many : single) * factor;
  return {
    id,
    mode: multi ? "multiCore" : "singleCore",
    label: id,
    unit,
    workers: multi ? 12 : 1,
    samples: [median],
    median,
    mean: median,
    min: median * 0.97,
    max: median * 1.03,
    stdDev: median * 0.02,
    cvPercent: multi ? 3.4 : 1.6,
    p95: median * 1.02,
    outlierIndices: [],
    note: null,
  };
}

function fakeResult(
  id: string,
  startedAtMs: number,
  profile: BenchmarkProfile,
  factor: number,
  info: StaticSystemInfo,
): BenchmarkResult {
  const ids = Object.keys(REF) as WorkloadId[];
  return {
    id,
    kind: "cpu",
    profile,
    startedAtMs,
    durationMs: 31_000,
    method: { methodId: "cpu-method-v1", scoreVersion: "score-v1", comparability: "crossPlatform" },
    environment: {
      platform: "linux",
      distribution: info.distribution,
      kernel: info.kernelVersion,
      architecture: info.architecture,
      cpuModel: info.cpuBrand,
      physicalCores: info.physicalCoreCount,
      logicalCores: info.logicalCoreCount,
      totalMemoryBytes: info.totalMemoryBytes,
      powerSource: "ac",
      cpuGovernor: "powersave",
      virtualization: null,
      backgroundCpuPercent: 4,
      developmentBuild: false,
    },
    workloads: ids.flatMap((w) => [workload(w, false, factor), workload(w, true, factor)]),
    sustained: null,
    scores: { singleCore: Math.round(1000 * factor), multiCore: Math.round(1000 * factor * 0.98) },
    warnings: [],
    stability: factor > 1.02 ? "medium" : "high",
    outlierMethod: "1.5×IQR; flagged, none removed",
    affinity: "not pinned",
  };
}

const STAGES = [
  "Preparing test data",
  "Single-core · Integer",
  "Single-core · Floating point",
  "Single-core · SHA-256 hashing",
  "Single-core · Compression (DEFLATE)",
  "Multi-core · Integer",
  "Multi-core · Floating point",
  "Multi-core · SHA-256 hashing",
  "Multi-core · Compression (DEFLATE)",
];

export function installBenchmarkPreview(info: StaticSystemInfo, tick: MetricsTick): void {
  // Makes the app believe a backend exists; every command goes to the stub.
  (globalThis as { isTauri?: boolean }).isTauri = true;
  const now = Date.now();
  let history = [
    fakeResult("cpu-3", now - 86_400_000 * 1, "standard", 1.03, info),
    fakeResult("cpu-2", now - 86_400_000 * 3, "standard", 0.99, info),
    fakeResult("cpu-1", now - 86_400_000 * 9, "quick", 1.0, info),
  ];
  let status: BenchmarkStatus = IDLE;
  let timer: ReturnType<typeof setInterval> | undefined;

  const publish = (next: BenchmarkStatus) => {
    status = next;
    void emit("benchmark://status", next);
  };

  const start = (profile: BenchmarkProfile): BenchmarkStatus => {
    const runId = `cpu-${Date.now()}-0`;
    let step = 0;
    const begun = Date.now();
    publish({ ...IDLE, state: "preparing", runId, kind: "cpu", profile, stage: "Starting" });
    timer = setInterval(() => {
      step += 1;
      if (step > STAGES.length * 4) {
        clearInterval(timer);
        const result = fakeResult(runId, begun, profile, 1.06, info);
        history = [result, ...history];
        void emit("benchmark://result", result);
        publish({ ...status, state: "completed", stage: "Done", fraction: 1, live: null });
        return;
      }
      const stage = STAGES[Math.min(Math.floor((step - 1) / 4), STAGES.length - 1)] ?? "";
      publish({
        ...status,
        state: step <= 2 ? "warmingUp" : "running",
        stage,
        fraction: step / (STAGES.length * 4),
        live: { label: stage, value: 40_000 + Math.random() * 25_000, unit: "Mops/s" },
        elapsedMs: Date.now() - begun,
        cpuPercent: 88 + Math.random() * 10,
        frequencyMhz: 3_400 + Math.random() * 400,
        temperatureC: 68 + step * 0.3,
      });
    }, 600);
    return status;
  };

  mockIPC(
    (cmd, args) => {
      switch (cmd) {
        case "get_settings":
        case "update_settings":
        case "reset_settings":
          return DEFAULT_SETTINGS;
        case "get_static_info":
          return info;
        case "get_monitoring_state":
          return { paused: false, intervalMs: 1000 };
        case "subscribe_metrics":
          return tick;
        case "get_history":
          return [];
        case "get_benchmark_status":
          return status;
        case "get_benchmark_history":
          return history;
        case "start_benchmark":
          return start((args as { profile: BenchmarkProfile }).profile);
        case "cancel_benchmark":
          clearInterval(timer);
          publish({
            ...status,
            state: "cancelled",
            message: "Cancelled. Nothing was saved.",
            live: null,
          });
          return undefined;
        case "delete_benchmark_result":
          history = history.filter((r) => r.id !== (args as { id: string }).id);
          return undefined;
        default:
          return undefined;
      }
    },
    { shouldMockEvents: true },
  );
}
