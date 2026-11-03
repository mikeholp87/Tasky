import { create } from "zustand";
import type { BenchmarkResult, BenchmarkStatus } from "@/types";

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

interface BenchmarkState {
  status: BenchmarkStatus;
  /** Saved results, newest first. */
  history: BenchmarkResult[];
  /** Result of the run that just finished; not saved for development builds. */
  latest: BenchmarkResult | null;
  /** A history entry the user opened; takes precedence over `latest`. */
  selectedId: string | null;

  setStatus: (status: BenchmarkStatus) => void;
  setHistory: (history: BenchmarkResult[]) => void;
  setLatest: (result: BenchmarkResult | null) => void;
  select: (id: string | null) => void;
}

export const useBenchmarkStore = create<BenchmarkState>((set) => ({
  status: IDLE,
  history: [],
  latest: null,
  selectedId: null,

  setStatus: (status) => set({ status }),
  setHistory: (history) => set({ history }),
  setLatest: (latest) => set({ latest, selectedId: null }),
  select: (selectedId) => set({ selectedId }),
}));

/** Whether a run is in progress (owns the benchmark lock). */
export function isActive(status: BenchmarkStatus): boolean {
  return !["idle", "completed", "cancelled", "failed"].includes(status.state);
}

/** The result to show: the one the user opened, else the latest run. */
export function shownResult(state: BenchmarkState): BenchmarkResult | null {
  if (state.selectedId) {
    return state.history.find((r) => r.id === state.selectedId) ?? state.latest;
  }
  return state.latest ?? state.history[0] ?? null;
}
