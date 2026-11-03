/**
 * Benchmark orchestration on the frontend side: builds requests from
 * settings, starts runs and feeds events into the store. Workloads always
 * run in the backend.
 */
import { backend, isBackendAvailable, toAppError } from "@/services/ipc";
import { runNetworkProvider } from "@/services/networkProvider";
import { isTerminal, useBenchmarkStore } from "@/stores/benchmarkStore";
import { useSettingsStore } from "@/stores/settingsStore";
import { toast } from "@/stores/toastStore";
import type {
  BenchmarkEvent,
  BenchmarkKind,
  BenchmarkProfile,
  BenchmarkRequest,
  PreflightReport,
} from "@/types";

export interface RunOptions {
  profile?: BenchmarkProfile;
  simulate?: boolean;
}

export function buildRequest(kinds: BenchmarkKind[], options: RunOptions = {}): BenchmarkRequest {
  const b = useSettingsStore.getState().benchmark;
  return {
    kinds,
    profile: options.profile ?? b.profile,
    cpuWorkers: b.cpuWorkers,
    gpuAdapterId: b.gpuAdapterId,
    diskMountPoint: b.diskMountPoint,
    diskAllowWrite: b.diskAllowWrite,
    diskWriteLimit: b.diskWriteLimit,
    networkDataProfile: b.networkDataProfile,
    networkShareResults: b.networkShareResults,
    sustainedSecs: b.sustainedSecs,
    cooldown: b.cooldown,
    simulate: options.simulate ?? false,
  };
}

export async function loadBenchmarkData(): Promise<void> {
  if (!isBackendAvailable()) return;
  const store = useBenchmarkStore.getState();
  try {
    const [capabilities, history] = await Promise.all([
      backend.getBenchmarkCapabilities(),
      backend.listBenchmarkHistory(),
    ]);
    store.setCapabilities(capabilities);
    store.setHistory(history);
  } catch (e) {
    toast.error(`Benchmarks unavailable: ${toAppError(e).message}`);
  }
}

export async function refreshHistory(): Promise<void> {
  if (!isBackendAvailable()) return;
  try {
    useBenchmarkStore.getState().setHistory(await backend.listBenchmarkHistory());
  } catch (e) {
    toast.error(toAppError(e).message);
  }
}

export function preflight(request: BenchmarkRequest): Promise<PreflightReport> {
  return backend.runBenchmarkPreflight(request);
}

function onEvent(event: BenchmarkEvent): void {
  const store = useBenchmarkStore.getState();
  store.applyEvent(event);
  if (event.type === "networkProviderRequest" && event.runId === store.run?.runId) {
    void runNetworkProvider(event);
    return;
  }
  if (event.type === "finished" && isTerminal(event.phase)) {
    const capabilities = store.capabilities;
    if (capabilities?.runningRunId === event.runId) {
      store.setCapabilities({ ...capabilities, runningRunId: null });
    }
    void refreshHistory();
    if (event.phase === "completed") {
      toast.success("Benchmark complete.");
      if (event.resultId) store.setView({ name: "result", id: event.resultId });
    } else if (event.phase === "failed" && event.error) {
      toast.error(event.error.message);
    }
  }
}

export async function startRun(request: BenchmarkRequest): Promise<boolean> {
  const store = useBenchmarkStore.getState();
  try {
    // Events can arrive before the run id resolves; buffer them until the
    // store knows which run they belong to.
    const early: BenchmarkEvent[] = [];
    let ready = false;
    const runId = await backend.startBenchmark(request, (event) => {
      if (ready) onEvent(event);
      else early.push(event);
    });
    store.beginRun(runId, request);
    ready = true;
    early.forEach(onEvent);
    return true;
  } catch (e) {
    toast.error(toAppError(e).message);
    return false;
  }
}

export async function cancelRun(): Promise<void> {
  const run = useBenchmarkStore.getState().run;
  if (!run || isTerminal(run.phase)) return;
  try {
    await backend.cancelBenchmark(run.runId);
  } catch (e) {
    toast.error(toAppError(e).message);
  }
}

export async function skipCooldown(): Promise<void> {
  const run = useBenchmarkStore.getState().run;
  if (!run) return;
  try {
    await backend.skipBenchmarkCooldown(run.runId);
  } catch (e) {
    toast.error(toAppError(e).message);
  }
}

export async function deleteResult(id: string): Promise<void> {
  try {
    await backend.deleteBenchmarkResult(id);
    await refreshHistory();
    useBenchmarkStore.getState().setView({ name: "home" });
  } catch (e) {
    toast.error(toAppError(e).message);
  }
}
