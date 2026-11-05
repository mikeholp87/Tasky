/**
 * Preview-only stand-in for the Rust backend, so the whole interface (not
 * just the data stores) can be reviewed in a plain browser. It simulates a
 * benchmark run with progress events and saves fake results in memory.
 * Loaded only by `dev/preview.ts`, which is never part of a production build.
 */
import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
import { DEFAULT_SETTINGS } from "@/stores/settingsStore";
import { NOT_HANDLED, inspectPreview } from "./previewInspect";
import type {
  BenchmarkKind,
  BenchmarkProfile,
  BenchmarkResult,
  BenchmarkStatus,
  DiskPhaseResult,
  DiskTargetInfo,
  GpuAdapterInfo,
  InternetPlan,
  GpuWorkloadResult,
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
    gpu: null,
    gpuScore: null,
    disk: null,
    diskScore: null,
    internet: null,
    warnings: [],
    stability: factor > 1.02 ? "medium" : "high",
    outlierMethod: "1.5×IQR; flagged, none removed",
    affinity: "not pinned",
  };
}

const ADAPTERS: GpuAdapterInfo[] = [
  {
    id: "vulkan-8086-a7ac-0",
    name: "Intel(R) Graphics (RPL-U)",
    vendorId: 0x8086,
    deviceId: 0xa7ac,
    deviceType: "integrated",
    backend: "vulkan",
    driver: "Intel open-source Mesa driver",
    driverInfo: "Mesa 26.1.6-1",
    supportsTimestamps: true,
    maxBufferBytes: 2047 * 1024 ** 2,
    maxStorageBindingBytes: 2047 * 1024 ** 2,
    maxWorkgroupsPerDimension: 65535,
    recommended: true,
  },
  {
    id: "vulkan-10005-0000-0",
    name: "llvmpipe (LLVM 21.1.8, 256 bits)",
    vendorId: 0x10005,
    deviceId: 0,
    deviceType: "software",
    backend: "vulkan",
    driver: "llvmpipe",
    driverInfo: "Mesa 26.1.6-1 (LLVM 21.1.8)",
    supportsTimestamps: true,
    maxBufferBytes: 2047 * 1024 ** 2,
    maxStorageBindingBytes: 128 * 1024 ** 2,
    maxWorkgroupsPerDimension: 65535,
    recommended: false,
  },
];

function gpuWorkload(
  id: GpuWorkloadResult["id"],
  label: string,
  unit: string,
  median: number,
  cv: number,
): GpuWorkloadResult {
  return {
    id,
    label,
    unit,
    samples: [median],
    median,
    mean: median,
    min: median * 0.95,
    max: median * 1.05,
    stdDev: median * 0.03,
    cvPercent: cv,
    p95: median * 1.04,
    outlierIndices: [],
    workPerDispatch: 1e10,
    dispatchMs: 60,
    validated: true,
    note: null,
  };
}

function fakeGpuResult(
  id: string,
  startedAtMs: number,
  profile: BenchmarkProfile,
  factor: number,
  info: StaticSystemInfo,
): BenchmarkResult {
  const base = fakeResult(id, startedAtMs, profile, factor, info);
  return {
    ...base,
    kind: "gpu",
    method: {
      methodId: "gpu-compute-v1",
      scoreVersion: "gpu-score-v1",
      comparability: "crossPlatform",
    },
    scores: null,
    workloads: [],
    gpuScore: Math.round(1000 * factor),
    stability: factor > 1.02 ? "medium" : "high",
    gpu: {
      adapter: ADAPTERS[0] as GpuAdapterInfo,
      timingMethod: "gpuTimestamps",
      workloads: [
        gpuWorkload("integer", "Integer compute", "Gops/s", 270 * factor, 4.1),
        gpuWorkload("floatingPoint", "Floating-point compute", "GFLOPS", 490 * factor, 6.8),
        gpuWorkload("memory", "Memory bandwidth", "GB/s", 31 * factor, 5.2),
      ],
      sustained: null,
      notes: [
        "Integrated GPU: the memory test measures shared system memory, not dedicated video memory, so it is not comparable with a discrete GPU's memory bandwidth.",
        "GPU utilization, clock speed and temperature are not measured during this test.",
      ],
    },
  };
}

const DISK_TARGETS: DiskTargetInfo[] = [
  {
    id: "home",
    label: "Home folder (/home/preview)",
    directory: "/home/preview",
    mountPoint: "/",
    fileSystem: "ext4",
    device: "/dev/nvme0n1p2",
    freeBytes: 588e9,
    totalBytes: 954e9,
    recommended: true,
    notes: [],
  },
];

function diskPhase(
  kind: DiskPhaseResult["kind"],
  label: string,
  block: number,
  qd: number,
  mbps: number,
  iops: number,
  p50: number,
  p99: number,
): DiskPhaseResult {
  return {
    kind,
    label,
    blockBytes: block,
    concurrency: qd,
    queueDepthExact: true,
    operations: 50_000,
    bytes: 2e9,
    seconds: 1,
    samples: [mbps],
    medianMbps: mbps,
    meanMbps: mbps,
    minMbps: mbps * 0.97,
    maxMbps: mbps * 1.03,
    cvPercent: 2.1,
    outlierIndices: [],
    medianIops: iops,
    latency: {
      samples: 50_000,
      meanUs: p50 * 1.1,
      p50Us: p50,
      p95Us: p50 * 1.6,
      p99Us: p99,
      maxUs: p99 * 6,
      histogram: [],
    },
    validated: true,
    note: null,
  };
}

function fakeDiskResult(
  id: string,
  startedAtMs: number,
  profile: BenchmarkProfile,
  factor: number,
  info: StaticSystemInfo,
): BenchmarkResult {
  const base = fakeResult(id, startedAtMs, profile, factor, info);
  const f = factor;
  return {
    ...base,
    kind: "disk",
    method: {
      methodId: "disk-direct-v1",
      scoreVersion: "disk-score-v1",
      comparability: "platformSpecific",
    },
    scores: null,
    workloads: [],
    sustained: null,
    diskScore: Math.round(1000 * f),
    stability: "high",
    disk: {
      target: DISK_TARGETS[0] as DiskTargetInfo,
      device: { name: "nvme0n1", model: "Preview NVMe SSD", rotational: false, transport: "nvme" },
      ioMode: "direct",
      ioModeNote: null,
      fileBytes: 1_073_741_824,
      bytesWritten: 2_204_000_000,
      bytesRead: 5_705_000_000,
      phases: [
        diskPhase(
          "sequentialWrite",
          "Sequential write · 1 MiB",
          1 << 20,
          1,
          5400 * f,
          5150 * f,
          176,
          298,
        ),
        diskPhase(
          "sequentialRead",
          "Sequential read · 1 MiB",
          1 << 20,
          1,
          5100 * f,
          4860 * f,
          195,
          244,
        ),
        diskPhase("randomRead", "Random read · 4 KiB · QD1", 4096, 1, 94 * f, 23_000 * f, 42, 62),
        diskPhase(
          "randomRead",
          "Random read · 4 KiB · QD8",
          4096,
          8,
          590 * f,
          144_000 * f,
          52,
          108,
        ),
        diskPhase(
          "randomRead",
          "Random read · 4 KiB · QD32",
          4096,
          32,
          2000 * f,
          490_000 * f,
          57,
          114,
        ),
        diskPhase("randomWrite", "Random write · 4 KiB · QD1", 4096, 1, 400 * f, 98_000 * f, 6, 36),
        diskPhase(
          "randomWrite",
          "Random write · 4 KiB · QD32",
          4096,
          32,
          4400 * f,
          1_080_000 * f,
          22,
          40,
        ),
      ],
      notes: [],
    },
  };
}

const INTERNET_PLAN: InternetPlan = {
  provider: "Cloudflare speed test",
  host: "speed.cloudflare.com",
  maxDownloadBytes: 800e6,
  maxUploadBytes: 300e6,
  estimatedSeconds: 30,
};

function fakeInternetResult(
  id: string,
  startedAtMs: number,
  profile: BenchmarkProfile,
  factor: number,
  info: StaticSystemInfo,
): BenchmarkResult {
  const base = fakeResult(id, startedAtMs, profile, factor, info);
  const transfer = (mbps: number, conns: number, loaded: number | null) => ({
    medianMbps: mbps,
    meanMbps: mbps,
    peakMbps: mbps * 1.12,
    samples: [mbps * 0.97, mbps, mbps * 1.02],
    bytes: Math.round((mbps * 1e6 * 10) / 8),
    seconds: 10,
    connections: conns,
    loadedLatency:
      loaded === null
        ? null
        : {
            samples: 30,
            minMs: 12,
            medianMs: loaded,
            p95Ms: loaded * 1.8,
            maxMs: loaded * 2.4,
            jitterMs: 8,
          },
  });
  const down = transfer(412 * factor, 6, 46);
  const up = transfer(38 * factor, 4, 31);
  return {
    ...base,
    kind: "internet",
    method: {
      methodId: "internet-cloudflare-v1",
      scoreVersion: "none",
      comparability: "crossPlatform",
    },
    scores: null,
    workloads: [],
    sustained: null,
    stability: "medium",
    internet: {
      provider: INTERNET_PLAN.provider,
      host: INTERNET_PLAN.host,
      serverLocation: "AMS",
      path: { interface: "wlo1", kind: "wifi", vpnSuspected: false },
      idleLatency: { samples: 20, minMs: 8, medianMs: 11, p95Ms: 19, maxMs: 25, jitterMs: 2.4 },
      download: down,
      upload: up,
      bytesDownloaded: down.bytes,
      bytesUploaded: up.bytes,
      packetLossPercent: null,
      notes: [
        "Throughput is bytes actually moved, sampled in short windows after the first seconds of ramp-up.",
        "Packet loss is not measured: HTTP cannot tell a lost packet from a retransmission.",
      ],
    },
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
    fakeDiskResult("disk-1", now - 86_400_000 * 2.5, "standard", 0.99, info),
    fakeInternetResult("net-1", now - 86_400_000 * 1.5, "standard", 0.9, info),
    fakeGpuResult("gpu-1", now - 86_400_000 * 2, "standard", 0.98, info),
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

  const start = (kind: BenchmarkKind, profile: BenchmarkProfile): BenchmarkStatus => {
    const runId = `${kind}-${Date.now()}-0`;
    let step = 0;
    const begun = Date.now();
    publish({ ...IDLE, state: "preparing", runId, kind, profile, stage: "Starting" });
    timer = setInterval(() => {
      step += 1;
      if (step > STAGES.length * 4) {
        clearInterval(timer);
        const make = {
          cpu: fakeResult,
          gpu: fakeGpuResult,
          disk: fakeDiskResult,
          internet: fakeInternetResult,
        }[kind];
        const result = make(runId, begun, profile, 1.06, info);
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
        live: {
          label: stage,
          ...(kind === "disk"
            ? { value: 1_000 + Math.random() * 4_000, unit: "MB/s" }
            : kind === "internet"
              ? { value: 300 + Math.random() * 120, unit: "Mbit/s" }
              : { value: 40_000 + Math.random() * 25_000, unit: "Mops/s" }),
        },
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
        case "list_gpu_adapters":
          return ADAPTERS;
        case "list_disk_targets":
          return DISK_TARGETS;
        case "disk_plan":
          return {
            targetId: "home",
            directory: "/home/preview",
            fileBytes: 1_073_741_824,
            estimatedBytesWritten: 2_204_000_000,
            estimatedSeconds: 8,
            blocked: null,
          };
        case "internet_plan":
          return INTERNET_PLAN;
        case "start_benchmark": {
          const a = args as { kind: BenchmarkKind; profile: BenchmarkProfile };
          return start(a.kind, a.profile);
        }
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
        default: {
          const reply = inspectPreview(cmd, args);
          return reply === NOT_HANDLED ? undefined : reply;
        }
      }
    },
    { shouldMockEvents: true },
  );
}
