import { render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useBenchmarkStore } from "@/stores/benchmarkStore";
import type { BenchmarkResult, BenchmarkStatus, WorkloadResult } from "@/types";
import { BenchmarksPage } from "./BenchmarksPage";

vi.mock("@/services/ipc", async (orig) => ({
  ...(await orig<typeof import("@/services/ipc")>()),
  isBackendAvailable: () => true,
}));
vi.mock("@/services/benchmarks", () => ({
  initBenchmarks: vi.fn(),
  runCpuBenchmark: vi.fn(),
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

beforeEach(() => {
  useBenchmarkStore.setState({ status: IDLE, history: [], latest: null, selectedId: null });
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
    expect(screen.getByRole("radio", { name: "Quick" })).toBeDisabled();
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
      status: { ...IDLE, state: "cancelled", message: "Cancelled. Nothing was saved." },
    });
    const { unmount } = render(<BenchmarksPage />);
    expect(screen.getByText("Cancelled. Nothing was saved.")).toBeInTheDocument();
    unmount();
    useBenchmarkStore.setState({
      status: { ...IDLE, state: "failed", message: "A worker stopped unexpectedly." },
    });
    render(<BenchmarksPage />);
    expect(screen.getByRole("alert")).toHaveTextContent("A worker stopped unexpectedly.");
  });
});
