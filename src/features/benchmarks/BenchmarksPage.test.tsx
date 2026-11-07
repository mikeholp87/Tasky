import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useBenchmarkStore } from "@/stores/benchmarkStore";
import type { BenchmarkResult, BenchmarkStatus, WorkloadResult } from "@/types";
import { runBenchmark } from "@/services/benchmarks";
import type { GpuAdapterInfo } from "@/types";
import { BenchmarksPage } from "./BenchmarksPage";

vi.mock("@/services/ipc", async (orig) => {
  const real = await orig<typeof import("@/services/ipc")>();
  return {
    ...real,
    isBackendAvailable: () => true,
    backend: {
      ...real.backend,
      diskPlan: () => new Promise(() => {}),
      internetPlan: () => new Promise(() => {}),
    },
  };
});
vi.mock("@/services/benchmarks", () => ({
  initBenchmarks: vi.fn(),
  runBenchmark: vi.fn(),
  loadGpuAdapters: vi.fn(),
  loadDiskTargets: vi.fn(),
  cancelBenchmark: vi.fn(),
  deleteBenchmarkResult: vi.fn(),
}));

function workload(id: WorkloadResult["id"], mode: WorkloadResult["mode"]): WorkloadResult {
  return {
    id,
    mode,
    label: id,
    unit: "Mops/s",
    workers: 1,
    samples: [1000],
    median: 14_000,
    mean: 14_000,
    min: 13_900,
    max: 14_100,
    stdDev: 100,
    cvPercent: 1.2,
    p95: 14_100,
    outlierIndices: [],
    note: null,
  };
}

const RESULT: BenchmarkResult = {
  id: "cpu-1-0",
  kind: "cpu",
  profile: "standard",
  startedAtMs: 1_800_000_000_000,
  durationMs: 30_000,
  method: { methodId: "cpu-method-v1", scoreVersion: "score-v1", comparability: "crossPlatform" },
  environment: {
    platform: "linux",
    distribution: "Debian GNU/Linux",
    kernel: "6.12",
    architecture: "x86_64",
    cpuModel: "Test CPU",
    physicalCores: 4,
    logicalCores: 8,
    totalMemoryBytes: 1,
    powerSource: "ac",
    cpuGovernor: "powersave",
    virtualization: null,
    backgroundCpuPercent: 2,
    developmentBuild: false,
  },
  workloads: (["integer", "floatingPoint", "hash", "compress", "decompress"] as const).flatMap(
    (id) => [workload(id, "singleCore"), workload(id, "multiCore")],
  ),
  sustained: null,
  scores: { singleCore: 1012, multiCore: 3980 },
  gpu: null,
  gpuScore: null,
  disk: null,
  diskScore: null,
  internet: null,
  warnings: ["Battery power may reduce benchmark performance."],
  stability: "medium",
  outlierMethod: "1.5×IQR; flagged, none removed",
  affinity: "not pinned",
};

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

const IGPU: GpuAdapterInfo = {
  id: "vulkan-8086-a7ac-0",
  name: "Intel Graphics",
  vendorId: 0x8086,
  deviceId: 0xa7ac,
  deviceType: "integrated",
  backend: "vulkan",
  driver: "Mesa",
  driverInfo: "26.1",
  supportsTimestamps: true,
  maxBufferBytes: 1 << 30,
  maxStorageBindingBytes: 1 << 27,
  maxWorkgroupsPerDimension: 65535,
  recommended: true,
};
const LLVMPIPE: GpuAdapterInfo = {
  ...IGPU,
  id: "vulkan-10005-0000-0",
  name: "llvmpipe",
  deviceType: "software",
  recommended: false,
};

const GPU_RESULT: BenchmarkResult = {
  ...RESULT,
  id: "gpu-1-0",
  kind: "gpu",
  method: {
    methodId: "gpu-compute-v1",
    scoreVersion: "gpu-score-v1",
    comparability: "crossPlatform",
  },
  scores: null,
  gpuScore: 1034,
  gpu: {
    adapter: IGPU,
    timingMethod: "gpuTimestamps",
    workloads: [
      {
        id: "integer",
        label: "Integer compute",
        unit: "Gops/s",
        samples: [270],
        median: 270,
        mean: 270,
        min: 260,
        max: 280,
        stdDev: 5,
        cvPercent: 2,
        p95: 278,
        outlierIndices: [],
        workPerDispatch: 1e10,
        dispatchMs: 45,
        validated: true,
        note: null,
      },
    ],
    sustained: null,
    notes: ["Integrated GPU: the memory test measures shared system memory."],
  },
  warnings: [],
};

beforeEach(() => {
  vi.mocked(runBenchmark).mockClear();
  useBenchmarkStore.setState({
    status: IDLE,
    history: [],
    latest: null,
    selectedId: null,
    adapters: null,
    adaptersError: null,
    diskTargets: null,
    diskTargetsError: null,
  });
});

describe("BenchmarksPage", () => {
  it("invites a first run when there is no result", () => {
    render(<BenchmarksPage />);
    expect(screen.getByRole("button", { name: /run cpu benchmark/i })).toBeEnabled();
    expect(screen.getByText(/run the benchmark to measure/i)).toBeInTheDocument();
  });

  it("shows scores, stability, warnings and the method of the latest result", () => {
    useBenchmarkStore.setState({ latest: RESULT, history: [RESULT] });
    render(<BenchmarksPage />);
    const panel = within(screen.getByRole("region", { name: /latest cpu result/i }));
    expect(panel.getByText("1012")).toBeInTheDocument();
    expect(panel.getByText("3980")).toBeInTheDocument();
    expect(panel.getByText("Medium stability")).toBeInTheDocument();
    expect(panel.getByText(/battery power may reduce/i)).toBeInTheDocument();
    expect(panel.getByText(/cpu-method-v1/)).toBeInTheDocument();
    expect(panel.getByText(/comparable across windows and linux/i)).toBeInTheDocument();
    // Single measurement, nothing earlier to compare with.
    expect(screen.queryByText(/compared with your previous/i)).not.toBeInTheDocument();
  });

  it("shows progress and a cancel button while running, and locks the profile", () => {
    useBenchmarkStore.setState({
      status: {
        ...IDLE,
        state: "running",
        kind: "cpu",
        stage: "Multi-core · Integer",
        fraction: 0.42,
        live: { label: "x", value: 61_406, unit: "Mops/s" },
        cpuPercent: 99,
        frequencyMhz: null,
        temperatureC: null,
        elapsedMs: 12_000,
      },
    });
    render(<BenchmarksPage />);
    expect(screen.getByRole("status")).toHaveTextContent("Multi-core · Integer");
    expect(screen.getByRole("meter", { name: /benchmark progress/i })).toHaveAttribute(
      "aria-valuenow",
      "42",
    );
    expect(screen.getByText("61.4 Gops/s")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /cancel/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /run cpu benchmark/i })).not.toBeInTheDocument();
    for (const quick of screen.getAllByRole("radio", { name: "Quick" }))
      expect(quick).toBeDisabled();
    // Missing sensors read as unavailable, never as zero.
    expect(screen.getByText("Temperature —")).toBeInTheDocument();
  });

  it("warns that development builds are not representative", () => {
    useBenchmarkStore.setState({ status: { ...IDLE, developmentBuild: true } });
    render(<BenchmarksPage />);
    expect(screen.getByRole("note")).toHaveTextContent(/not representative/i);
  });

  it("says so when a run was cancelled or failed", () => {
    useBenchmarkStore.setState({
      status: {
        ...IDLE,
        state: "cancelled",
        kind: "cpu",
        message: "Cancelled. Nothing was saved.",
      },
    });
    const { unmount } = render(<BenchmarksPage />);
    expect(screen.getByText("Cancelled. Nothing was saved.")).toBeInTheDocument();
    unmount();
    useBenchmarkStore.setState({
      status: { ...IDLE, state: "failed", kind: "cpu", message: "A worker stopped unexpectedly." },
    });
    render(<BenchmarksPage />);
    expect(screen.getByRole("alert")).toHaveTextContent("A worker stopped unexpectedly.");
  });

  it("shows a persistence failure even when the measurement completed", () => {
    useBenchmarkStore.setState({
      status: {
        ...IDLE,
        state: "completed",
        kind: "cpu",
        message: "The result could not be saved: existing history could not be preserved.",
      },
    });
    render(<BenchmarksPage />);
    expect(screen.getByRole("alert")).toHaveTextContent(/could not be saved/);
  });

  it("lists graphics adapters with the recommended one selected and runs on it", async () => {
    useBenchmarkStore.setState({ adapters: [LLVMPIPE, IGPU] });
    render(<BenchmarksPage />);
    const select = screen.getByRole("combobox", { name: /adapter/i });
    expect(select).toHaveValue(IGPU.id);
    expect(
      within(select).getByRole("option", { name: /intel graphics.*recommended/i }),
    ).toBeInTheDocument();
    expect(
      within(select).getByRole("option", { name: /llvmpipe.*software renderer/i }),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /run gpu benchmark/i }));
    expect(runBenchmark).toHaveBeenCalledWith("gpu", "standard", { adapter: IGPU.id });
  });

  it("warns that a software renderer is not a GPU result", async () => {
    useBenchmarkStore.setState({ adapters: [LLVMPIPE] });
    render(<BenchmarksPage />);
    expect(screen.getByText(/software renderer that runs on the cpu/i)).toBeInTheDocument();
  });

  it("explains when no graphics adapter exists, and offers no run button", () => {
    useBenchmarkStore.setState({ adapters: [] });
    render(<BenchmarksPage />);
    expect(screen.getByRole("alert")).toHaveTextContent(/no graphics adapter/i);
    expect(screen.queryByRole("button", { name: /run gpu benchmark/i })).not.toBeInTheDocument();
  });

  it("shows a GPU result with its adapter, timing, verification and notes", () => {
    useBenchmarkStore.setState({ adapters: [IGPU], latest: GPU_RESULT, history: [GPU_RESULT] });
    render(<BenchmarksPage />);
    const panel = within(screen.getByRole("region", { name: /latest gpu result/i }));
    expect(panel.getByText("1034")).toBeInTheDocument();
    expect(panel.getByText("Verified")).toBeInTheDocument();
    expect(panel.getByText(/shared system memory/i)).toBeInTheDocument();
    expect(panel.getByText(/GPU timestamps/)).toBeInTheDocument();
    expect(panel.getByText(/gpu-compute-v1/)).toBeInTheDocument();
    // The CPU card is unaffected by a GPU result.
    expect(screen.getByText(/run the benchmark to measure single-core/i)).toBeInTheDocument();
  });

  it("keeps the two benchmarks separate: one lock, each card owns its own progress", () => {
    useBenchmarkStore.setState({
      adapters: [IGPU],
      status: {
        ...IDLE,
        state: "running",
        kind: "cpu",
        stage: "Single-core · Integer",
        fraction: 0.2,
      },
    });
    render(<BenchmarksPage />);
    // The GPU card cannot start while the CPU benchmark runs...
    expect(screen.getByRole("button", { name: /run gpu benchmark/i })).toBeDisabled();
    // ...and shows no progress of its own; only one progress bar exists.
    expect(screen.getAllByRole("meter", { name: /benchmark progress/i })).toHaveLength(1);
    expect(screen.queryByText(/not measured/i)).not.toBeInTheDocument();
  });

  it("says GPU sensors are not measured while a GPU benchmark runs", () => {
    useBenchmarkStore.setState({
      adapters: [IGPU],
      status: {
        ...IDLE,
        state: "running",
        kind: "gpu",
        stage: "GPU · Integer compute",
        fraction: 0.4,
        live: { label: "x", value: 270, unit: "Gops/s" },
      },
    });
    render(<BenchmarksPage />);
    expect(
      screen.getByText(/gpu utilization, clock speed and temperature are not measured/i),
    ).toBeInTheDocument();
    // No CPU temperature placeholder is shown for a GPU run.
    expect(screen.queryByText("Temperature —")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /cancel/i })).toBeInTheDocument();
  });
});
