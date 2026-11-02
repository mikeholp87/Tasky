import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BenchmarkEvent, BenchmarkRequest, PreflightReport } from "@/types";

const backend = vi.hoisted(() => ({
  getBenchmarkCapabilities: vi.fn(),
  listBenchmarkHistory: vi.fn(),
  runBenchmarkPreflight: vi.fn(),
  startBenchmark: vi.fn(),
  cancelBenchmark: vi.fn(),
  skipBenchmarkCooldown: vi.fn(),
  getBenchmarkResult: vi.fn(),
  deleteBenchmarkResult: vi.fn(),
}));

vi.mock("@/services/ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/services/ipc")>();
  return { ...actual, backend, isBackendAvailable: () => true };
});

const { BenchmarksPage } = await import("./BenchmarksPage");
const { useBenchmarkStore } = await import("@/stores/benchmarkStore");

const PREFLIGHT: PreflightReport = {
  environment: {
    timestampMs: 0,
    osName: "TestOS",
    osVersion: "1",
    kernelVersion: null,
    architecture: "x86_64",
    cpuBrand: "Test CPU",
    cpuVendor: "Test",
    physicalCores: 4,
    logicalCores: 8,
    totalMemoryBytes: 1,
    appVersion: "0.1.0",
    cpuUsagePercent: 37,
    externalCpuPercent: 37,
    memoryUsedPercent: 50,
    diskActivePercent: null,
    networkRxBytesPerSec: null,
    networkTxBytesPerSec: null,
    cpuFrequencyMhz: null,
    temperatures: [],
    power: { source: "ac", batteryPercent: null },
  },
  loadLevel: "high",
  warnings: [
    {
      code: "highBackgroundLoad",
      message:
        "High background CPU activity detected. Current CPU usage by other applications: 37%.",
    },
  ],
  estimatedSecs: 30,
  estimatedDiskWriteBytes: 0,
  estimatedNetworkBytes: 0,
  simulated: false,
};

let emit: (event: BenchmarkEvent) => void = () => {};

describe("BenchmarksPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useBenchmarkStore.setState({ run: null, launch: null, view: { name: "home" }, history: [] });
    backend.getBenchmarkCapabilities.mockResolvedValue({
      methodVersion: "1.0.0",
      scoreVersion: 1,
      logicalCores: 8,
      physicalCores: 4,
      simulationAvailable: false,
      temperatureAvailable: false,
      powerAvailable: false,
      runningRunId: null,
    });
    backend.listBenchmarkHistory.mockResolvedValue([]);
    backend.runBenchmarkPreflight.mockResolvedValue(PREFLIGHT);
    backend.startBenchmark.mockImplementation(
      (_req: BenchmarkRequest, onEvent: (e: BenchmarkEvent) => void) => {
        emit = onEvent;
        return Promise.resolve("run-1");
      },
    );
    backend.cancelBenchmark.mockResolvedValue(true);
  });

  it("shows a card per category and nothing runs automatically", async () => {
    render(<BenchmarksPage />);
    expect(screen.getByRole("heading", { level: 1, name: "Benchmarks" })).toBeInTheDocument();
    for (const name of ["CPU", "GPU", "Storage", "Internet"]) {
      expect(screen.getByRole("button", { name: `Run ${name} test` })).toBeEnabled();
    }
    await waitFor(() => expect(backend.listBenchmarkHistory).toHaveBeenCalled());
    expect(backend.startBenchmark).not.toHaveBeenCalled();
  });

  it("warns about background load and starts only after confirmation", async () => {
    const user = userEvent.setup();
    render(<BenchmarksPage />);
    await user.click(screen.getByRole("button", { name: "Run CPU test" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("37%");
    expect(screen.getByRole("button", { name: "Wait for system to become idle" })).toBeVisible();
    expect(backend.startBenchmark).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: "Run anyway" }));
    await waitFor(() => expect(backend.startBenchmark).toHaveBeenCalledTimes(1));
    const request = backend.startBenchmark.mock.calls[0]?.[0] as BenchmarkRequest;
    expect(request.kinds).toEqual(["cpu"]);
    expect(request.simulate).toBe(false);
  });

  it("shows accessible progress and cancels the active run", async () => {
    const user = userEvent.setup();
    render(<BenchmarksPage />);
    await user.click(screen.getByRole("button", { name: "Run CPU test" }));
    await user.click(await screen.findByRole("button", { name: "Run anyway" }));
    await waitFor(() => expect(useBenchmarkStore.getState().run?.runId).toBe("run-1"));

    emit({
      type: "progress",
      runId: "run-1",
      phase: "running",
      kind: "cpu",
      subtest: "Multi-core floating point",
      percent: 72,
      elapsedMs: 17_000,
      iteration: 3,
      totalIterations: 5,
      measurement: null,
      message: "",
      live: [],
    });

    const bar = await screen.findByRole("progressbar");
    expect(bar).toHaveAttribute("aria-valuenow", "72");
    expect(screen.getByText(/Iteration 3 of 5/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Run CPU test" })).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "Cancel benchmark" }));
    expect(backend.cancelBenchmark).toHaveBeenCalledWith("run-1");

    emit({ type: "finished", runId: "run-1", phase: "cancelled", resultId: null, error: null });
    await waitFor(() => expect(screen.queryByRole("progressbar")).not.toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Run CPU test" })).toBeEnabled();
  });
});
