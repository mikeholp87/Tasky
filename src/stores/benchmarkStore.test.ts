import { beforeEach, describe, expect, it } from "vitest";
import type { BenchmarkProgress, BenchmarkRequest, BenchmarkSummary } from "@/types";
import {
  LIVE_POINTS,
  LiveSeries,
  latestByKind,
  liveSeries,
  useBenchmarkStore,
} from "./benchmarkStore";

const REQUEST: BenchmarkRequest = {
  kinds: ["cpu"],
  profile: "quick",
  cpuWorkers: null,
  gpuAdapterId: null,
  diskMountPoint: null,
  diskAllowWrite: false,
  diskWriteLimit: "standard",
  networkShareResults: false,
  networkDataProfile: "standard",
  sustainedSecs: 60,
  cooldown: true,
  simulate: true,
};

function progress(overrides: Partial<BenchmarkProgress> = {}): BenchmarkProgress {
  return {
    runId: "run-1",
    phase: "running",
    kind: "cpu",
    subtest: "Single-core integer",
    percent: 40,
    elapsedMs: 1200,
    iteration: 2,
    totalIterations: 5,
    measurement: null,
    message: "",
    live: [],
    ...overrides,
  };
}

describe("benchmarkStore", () => {
  beforeEach(() => {
    useBenchmarkStore.setState({ run: null, history: [], liveVersion: 0 });
    liveSeries.clear();
  });

  it("tracks progress and live samples for the active run only", () => {
    const store = useBenchmarkStore.getState();
    store.beginRun("run-1", REQUEST);
    store.applyEvent({
      type: "progress",
      ...progress({
        live: [{ tMs: 100, series: "cpu.single.integer", value: 2000, unit: "mops" }],
      }),
    });
    // Events from another run are ignored.
    store.applyEvent({ type: "progress", ...progress({ runId: "other", percent: 99 }) });

    const run = useBenchmarkStore.getState().run;
    expect(run?.phase).toBe("running");
    expect(run?.progress?.percent).toBe(40);
    expect(liveSeries.get("cpu.single.integer")?.v).toEqual([2000]);
    expect(liveSeries.get("cpu.single.integer")?.unit).toBe("mops");
  });

  it("records the outcome when the run finishes", () => {
    const store = useBenchmarkStore.getState();
    store.beginRun("run-1", REQUEST);
    store.applyEvent({
      type: "finished",
      runId: "run-1",
      phase: "completed",
      resultId: "abc",
      error: null,
    });
    const run = useBenchmarkStore.getState().run;
    expect(run?.phase).toBe("completed");
    expect(run?.resultId).toBe("abc");
  });

  it("bounds live series", () => {
    const s = new LiveSeries("mops");
    for (let i = 0; i < LIVE_POINTS + 50; i++) s.push(i * 1000, i);
    expect(s.length).toBe(LIVE_POINTS);
    expect(s.v[0]).toBe(50);
  });

  it("finds the latest completed result per category", () => {
    const summary = (id: string, kinds: ("cpu" | "gpu")[]): BenchmarkSummary => ({
      id,
      startedAtMs: 0,
      durationMs: 0,
      profile: "quick",
      methodVersion: "1.0.0",
      scoreVersion: 1,
      simulated: false,
      fingerprint: {
        cpuBrand: "",
        logicalCores: 1,
        memoryGib: 1,
        os: "",
        gpu: null,
        disk: null,
      },
      systemScore: null,
      categories: kinds.map((kind) => ({
        kind,
        status: kind === "gpu" ? "failed" : "completed",
        score: null,
        confidence: null,
        metrics: [],
      })),
    });
    const latest = latestByKind([summary("new", ["cpu", "gpu"]), summary("old", ["cpu"])]);
    expect(latest.cpu?.id).toBe("new");
    expect(latest.gpu).toBeUndefined();
  });
});
