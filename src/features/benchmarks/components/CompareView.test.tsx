import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BenchmarkComparison, BenchmarkSummary } from "@/types";

const backend = vi.hoisted(() => ({ compareBenchmarks: vi.fn() }));
vi.mock("@/services/ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/services/ipc")>();
  return { ...actual, backend, isBackendAvailable: () => true };
});

const { CompareView } = await import("./CompareView");
const { useBenchmarkStore } = await import("@/stores/benchmarkStore");

function summary(id: string, t: number): BenchmarkSummary {
  return {
    id,
    startedAtMs: t,
    durationMs: 1000,
    profile: "standard",
    methodVersion: "1.0.0",
    scoreVersion: 1,
    simulated: false,
    fingerprint: { cpuBrand: "", logicalCores: 1, memoryGib: 1, os: "", gpu: null, disk: null },
    systemScore: null,
    categories: [],
  };
}

const COMPARISON: BenchmarkComparison = {
  baselineId: "a",
  currentId: "b",
  compatibleMethod: true,
  hardwareChanged: true,
  hardwareChanges: ["GPU: Intel Graphics → NVIDIA RTX"],
  rows: [
    {
      id: "cpu.multi.integer",
      label: "Multi-core integer",
      unit: "mops",
      higherIsBetter: true,
      baseline: 12_882,
      current: 13_407,
      changePercent: 4.08,
      withinNoise: false,
    },
    {
      id: "disk.seq.read",
      label: "Sequential read",
      unit: "megabytesPerSec",
      higherIsBetter: true,
      baseline: 3_201,
      current: 3_220,
      changePercent: 0.6,
      withinNoise: true,
    },
  ],
};

describe("CompareView", () => {
  beforeEach(() => {
    useBenchmarkStore.setState({ history: [summary("b", 2), summary("a", 1)] });
  });

  it("shows changes, noise and hardware changes", async () => {
    backend.compareBenchmarks.mockResolvedValue(COMPARISON);
    render(<CompareView baselineId="a" currentId="b" />);
    expect(await screen.findByText(/Hardware configuration changed/)).toBeInTheDocument();
    expect(screen.getByText(/Intel Graphics → NVIDIA RTX/)).toBeInTheDocument();
    expect(screen.getByText("+4.1%")).toBeInTheDocument();
    expect(screen.getByText("(within variation)")).toBeInTheDocument();
    expect(screen.getByText("better")).toBeInTheDocument();
  });

  it("does not compute changes across methodology versions", async () => {
    backend.compareBenchmarks.mockResolvedValue({
      ...COMPARISON,
      compatibleMethod: false,
      hardwareChanged: false,
      hardwareChanges: [],
    });
    render(<CompareView baselineId="a" currentId="b" />);
    expect(await screen.findByText(/different benchmark methodology/)).toBeInTheDocument();
    expect(screen.getAllByText("Not comparable")).toHaveLength(2);
    expect(screen.queryByText("+4.1%")).not.toBeInTheDocument();
  });
});
