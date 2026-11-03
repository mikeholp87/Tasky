/** Benchmark lifecycle: backend events into the store, and the run controls. */
import { listen } from "@tauri-apps/api/event";
import { useBenchmarkStore } from "@/stores/benchmarkStore";
import { toast } from "@/stores/toastStore";
import type { BenchmarkKind, BenchmarkProfile, BenchmarkResult, BenchmarkStatus } from "@/types";
import { backend, isBackendAvailable, toAppError } from "./ipc";

const STATUS_EVENT = "benchmark://status";
const RESULT_EVENT = "benchmark://result";

let started = false;

async function refreshHistory(): Promise<void> {
  try {
    useBenchmarkStore.getState().setHistory(await backend.getBenchmarkHistory());
  } catch (e) {
    toast.error(toAppError(e).message);
  }
}

/** Subscribes to progress events once and loads saved results. */
export async function initBenchmarks(): Promise<void> {
  if (started || !isBackendAvailable()) return;
  started = true;
  const store = useBenchmarkStore.getState();
  await listen<BenchmarkStatus>(STATUS_EVENT, (e) => store.setStatus(e.payload));
  await listen<BenchmarkResult>(RESULT_EVENT, (e) => {
    store.setLatest(e.payload);
    void refreshHistory();
  });
  try {
    store.setStatus(await backend.getBenchmarkStatus());
  } catch (e) {
    toast.error(toAppError(e).message);
  }
  await refreshHistory();
}

export async function runBenchmark(
  kind: BenchmarkKind,
  profile: BenchmarkProfile,
  adapter?: string,
): Promise<void> {
  try {
    useBenchmarkStore.getState().select(null);
    useBenchmarkStore.getState().setStatus(await backend.startBenchmark(kind, profile, adapter));
  } catch (e) {
    toast.error(toAppError(e).message);
  }
}

/** Lists graphics adapters once; a failure is shown in the GPU card, not as a toast. */
export async function loadGpuAdapters(): Promise<void> {
  const store = useBenchmarkStore.getState();
  if (store.adapters !== null || !isBackendAvailable()) return;
  try {
    store.setAdapters(await backend.listGpuAdapters());
  } catch (e) {
    store.setAdapters([], toAppError(e).message);
  }
}

export async function cancelBenchmark(): Promise<void> {
  try {
    await backend.cancelBenchmark();
  } catch (e) {
    toast.error(toAppError(e).message);
  }
}

export async function deleteBenchmarkResult(id: string): Promise<void> {
  try {
    await backend.deleteBenchmarkResult(id);
    const s = useBenchmarkStore.getState();
    if (s.selectedId === id) s.select(null);
    await refreshHistory();
  } catch (e) {
    toast.error(toAppError(e).message);
  }
}
