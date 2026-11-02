import { create } from "zustand";
import type {
  BenchmarkCapabilities,
  BenchmarkEvent,
  BenchmarkKind,
  BenchmarkPhase,
  BenchmarkProgress,
  BenchmarkRequest,
  BenchmarkSummary,
  FailureInfo,
  MetricUnit,
} from "@/types";

/** Points kept per live series (charts show the whole run at this resolution). */
export const LIVE_POINTS = 600;

/**
 * Mutable, bounded live series. Updated in place on every progress message;
 * consumers re-read it when `liveVersion` changes (like the metrics history).
 */
export class LiveSeries {
  readonly t: number[] = [];
  readonly v: number[] = [];

  constructor(readonly unit: MetricUnit) {}

  /** `unixMs` is wall-clock time, so charts can label the x axis with clock time. */
  push(unixMs: number, value: number): void {
    this.t.push(unixMs / 1000);
    this.v.push(value);
    if (this.t.length > LIVE_POINTS) {
      this.t.shift();
      this.v.shift();
    }
  }

  get length(): number {
    return this.t.length;
  }
}

export const liveSeries = new Map<string, LiveSeries>();
/** Wall-clock start of the current run; live samples carry offsets from it. */
let runStartedAtMs = 0;

export type BenchmarkView =
  | { name: "home" }
  | { name: "category"; kind: BenchmarkKind }
  | { name: "result"; id: string }
  | { name: "history" }
  | { name: "compare"; baselineId?: string; currentId?: string }
  | { name: "about" };

export interface RunState {
  runId: string;
  request: BenchmarkRequest;
  progress: BenchmarkProgress | null;
  phase: BenchmarkPhase;
  /** Set once the run ends. */
  resultId: string | null;
  error: FailureInfo | null;
}

interface BenchmarkState {
  capabilities: BenchmarkCapabilities | null;
  history: BenchmarkSummary[];
  historyLoaded: boolean;
  run: RunState | null;
  liveVersion: number;
  /** Categories awaiting confirmation in the pre-run dialog. */
  launch: BenchmarkKind[] | null;
  requestLaunch: (kinds: BenchmarkKind[]) => void;
  closeLaunch: () => void;
  view: BenchmarkView;
  setView: (view: BenchmarkView) => void;
  setCapabilities: (c: BenchmarkCapabilities) => void;
  setHistory: (h: BenchmarkSummary[]) => void;
  beginRun: (runId: string, request: BenchmarkRequest) => void;
  applyEvent: (event: BenchmarkEvent) => void;
  clearRun: () => void;
}

export const isTerminal = (phase: BenchmarkPhase): boolean =>
  phase === "completed" || phase === "cancelled" || phase === "failed";

export const useBenchmarkStore = create<BenchmarkState>((set, get) => ({
  capabilities: null,
  history: [],
  historyLoaded: false,
  run: null,
  liveVersion: 0,
  view: { name: "home" },
  setView: (view) => set({ view }),
  launch: null,
  requestLaunch: (kinds) => set({ launch: kinds }),
  closeLaunch: () => set({ launch: null }),
  setCapabilities: (capabilities) => set({ capabilities }),
  setHistory: (history) => set({ history, historyLoaded: true }),

  beginRun: (runId, request) => {
    liveSeries.clear();
    runStartedAtMs = Date.now();
    set((s) => ({
      run: { runId, request, progress: null, phase: "preparing", resultId: null, error: null },
      liveVersion: s.liveVersion + 1,
    }));
  },

  applyEvent: (event) => {
    const run = get().run;
    if (!run || event.runId !== run.runId) return;
    switch (event.type) {
      case "progress": {
        for (const sample of event.live) {
          let series = liveSeries.get(sample.series);
          if (!series) {
            series = new LiveSeries(sample.unit);
            liveSeries.set(sample.series, series);
          }
          series.push(runStartedAtMs + sample.tMs, sample.value);
        }
        const { type: _type, ...progress } = event;
        set((s) => ({
          run: { ...run, progress, phase: progress.phase },
          liveVersion: event.live.length > 0 ? s.liveVersion + 1 : s.liveVersion,
        }));
        break;
      }
      case "finished":
        set({
          run: { ...run, phase: event.phase, resultId: event.resultId, error: event.error },
        });
        break;
      case "networkProviderRequest":
        // Handled by the benchmark service.
        break;
    }
  },

  clearRun: () => set({ run: null }),
}));

/** The most recent completed result for each category. */
export function latestByKind(
  history: readonly BenchmarkSummary[],
): Partial<Record<BenchmarkKind, BenchmarkSummary>> {
  const out: Partial<Record<BenchmarkKind, BenchmarkSummary>> = {};
  for (const summary of history) {
    for (const c of summary.categories) {
      if (c.status === "completed" && !out[c.kind]) out[c.kind] = summary;
    }
  }
  return out;
}
