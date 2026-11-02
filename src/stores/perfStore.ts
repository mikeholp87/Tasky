import { create } from "zustand";

/**
 * Developer diagnostics: how long the frontend spends on each tick.
 * Measured from the tick's arrival to the next animation frame, which
 * covers store updates, React rendering and chart redraws.
 */
interface PerfState {
  ingestMs: number;
  frameMs: number;
  /** Development builds only: replace real processes with N mock rows. */
  mockProcessCount: number;
  record: (ingestMs: number, frameMs: number) => void;
  setMockProcessCount: (n: number) => void;
}

/** Development builds: recent frame costs, readable by measurement scripts. */
const devSamples: number[] = [];
if (import.meta.env.DEV && typeof window !== "undefined") {
  Object.assign(window, {
    __tmPerf: {
      drain: () => devSamples.splice(0),
      setMock: (n: number) => usePerfStore.getState().setMockProcessCount(n),
    },
  });
}

export const usePerfStore = create<PerfState>((set) => ({
  ingestMs: 0,
  frameMs: 0,
  mockProcessCount: 0,
  record: (ingestMs, frameMs) => {
    set({ ingestMs, frameMs });
    if (import.meta.env.DEV && devSamples.push(frameMs) > 600) devSamples.shift();
  },
  setMockProcessCount: (mockProcessCount) => set({ mockProcessCount }),
}));
