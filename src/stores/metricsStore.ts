import { create } from "zustand";
import type {
  AppError,
  HistoryPoint,
  MetricsTick,
  MonitoringState,
  StaticSystemInfo,
} from "@/types";
import { recordDevices } from "./deviceHistory";
import { HistoryBuffer } from "./historyBuffer";
import { MemoryHistory, type MemoryBaseline } from "@/features/memory/diagnostics";

export type ConnectionStatus = "connecting" | "live" | "unavailable" | "error";

/** Shared history buffer; read it when `historyVersion` changes. */
export const history = new HistoryBuffer();
const memoryHistory = new MemoryHistory();

interface MetricsState {
  status: ConnectionStatus;
  error: AppError | null;
  staticInfo: StaticSystemInfo | null;
  tick: MetricsTick | null;
  memoryBaseline: MemoryBaseline | null;
  monitoring: MonitoringState;
  /** Incremented whenever `history` changes. */
  historyVersion: number;

  setStatus: (status: ConnectionStatus, error?: AppError | null) => void;
  setStaticInfo: (info: StaticSystemInfo) => void;
  setMonitoring: (state: MonitoringState) => void;
  ingestTick: (tick: MetricsTick) => void;
  backfillHistory: (points: readonly HistoryPoint[]) => void;
}

export const useMetricsStore = create<MetricsState>((set, get) => ({
  status: "connecting",
  error: null,
  staticInfo: null,
  tick: null,
  memoryBaseline: null,
  monitoring: { paused: false, intervalMs: 1000 },
  historyVersion: 0,

  setStatus: (status, error = null) => set({ status, error }),
  setStaticInfo: (staticInfo) => set({ staticInfo }),
  setMonitoring: (monitoring) => set({ monitoring }),
  ingestTick: (tick) => {
    const current = get().tick;
    // The subscription response can race a newer channel message.
    if (current && tick.seq <= current.seq) return;
    history.push(tick.point);
    recordDevices(tick);
    set((s) => ({
      tick,
      memoryBaseline: memoryHistory.record(tick, s.staticInfo?.platform),
      status: "live",
      error: null,
      historyVersion: s.historyVersion + 1,
    }));
  },
  backfillHistory: (points) => {
    history.backfill(points);
    set((s) => ({ historyVersion: s.historyVersion + 1 }));
  },
}));
