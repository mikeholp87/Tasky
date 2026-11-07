import { create } from "zustand";
import type {
  BenchmarkKind,
  BenchmarkResult,
  BenchmarkStatus,
  DiskTargetInfo,
  GpuAdapterInfo,
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

interface BenchmarkState {
  status: BenchmarkStatus;
  cancelling: boolean;
  /** Saved results, newest first. */
  history: BenchmarkResult[];
  /** Result of the run that just finished; not saved for development builds. */
  latest: BenchmarkResult | null;
  /** A history entry the user opened; takes precedence over `latest`. */
  selectedId: string | null;
  /** Graphics adapters; null until listed. */
  adapters: GpuAdapterInfo[] | null;
  adaptersError: string | null;
  /** Disk locations; null until listed. */
  diskTargets: DiskTargetInfo[] | null;
  diskTargetsError: string | null;

  setStatus: (status: BenchmarkStatus) => void;
  setCancelling: (cancelling: boolean) => void;
  setHistory: (history: BenchmarkResult[]) => void;
  setLatest: (result: BenchmarkResult | null) => void;
  select: (id: string | null) => void;
  setAdapters: (adapters: GpuAdapterInfo[] | null, error?: string | null) => void;
  setDiskTargets: (targets: DiskTargetInfo[] | null, error?: string | null) => void;
}

export const useBenchmarkStore = create<BenchmarkState>((set) => ({
  status: IDLE,
  cancelling: false,
  history: [],
  latest: null,
  selectedId: null,
  adapters: null,
  adaptersError: null,
  diskTargets: null,
  diskTargetsError: null,

  setStatus: (status) =>
    set((s) => ({
      status,
      cancelling: isActive(status) && s.status.runId === status.runId && s.cancelling,
    })),
  setCancelling: (cancelling) => set({ cancelling }),
  setHistory: (history) => set({ history }),
  setLatest: (latest) => set({ latest, selectedId: null }),
  select: (selectedId) => set({ selectedId }),
  setAdapters: (adapters, error = null) => set({ adapters, adaptersError: error }),
  setDiskTargets: (diskTargets, error = null) => set({ diskTargets, diskTargetsError: error }),
}));

/** Whether a run is in progress (owns the benchmark lock). */
export function isActive(status: BenchmarkStatus): boolean {
  return !["idle", "completed", "cancelled", "failed"].includes(status.state);
}

/**
 * The result to show in the card for `kind`: the one the user opened if it is
 * of that kind, else the latest run of that kind, else the newest saved one.
 */
export function shownResultFor(state: BenchmarkState, kind: BenchmarkKind): BenchmarkResult | null {
  const selected = state.selectedId
    ? state.history.find((r) => r.id === state.selectedId)
    : undefined;
  if (selected?.kind === kind) return selected;
  if (state.latest?.kind === kind) return state.latest;
  return state.history.find((r) => r.kind === kind) ?? null;
}
