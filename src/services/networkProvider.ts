/**
 * Runs the official `@cloudflare/speedtest` package for the internet
 * benchmark. The package must run in the WebView (it measures with the
 * browser's Resource Timing API), so the Rust coordinator sends a
 * measurement plan, this module executes it and reports progress, and the
 * raw samples go back to Rust, which validates them and computes every
 * statistic. Nothing here decides results.
 *
 * Privacy: results logging to Cloudflare (`logAimApiUrl`) is disabled
 * unless the user opted in; per-measurement logging stays off.
 */
import type { MeasurementConfig, Results } from "@cloudflare/speedtest";
import { backend } from "@/services/ipc";
import { isTerminal, useBenchmarkStore } from "@/stores/benchmarkStore";
import type {
  BenchmarkEvent,
  NetworkLiveValue,
  NetworkMeasurementSpec,
  NetworkSubmission,
} from "@/types";

export const PROVIDER_VERSION = "@cloudflare/speedtest 1.15.0";
/** Progress messages to Rust at most this often. */
const PROGRESS_INTERVAL_MS = 250;

type ProviderRequest = Extract<BenchmarkEvent, { type: "networkProviderRequest" }>;

export function toMeasurements(plan: readonly NetworkMeasurementSpec[]): MeasurementConfig[] {
  return plan.map((step) =>
    step.kind === "latency"
      ? { type: "latency", numPackets: step.count }
      : { type: step.kind, bytes: step.bytes, count: step.count },
  );
}

export function toSubmission(results: Results): NetworkSubmission {
  const transfers = (points: { bytes: number; duration: number }[]) =>
    points.map((p) => ({ bytes: p.bytes, durationMs: p.duration }));
  return {
    download: transfers(results.getDownloadBandwidthPoints()),
    upload: transfers(results.getUploadBandwidthPoints()),
    idleLatencyMs: results.getUnloadedLatencyPoints(),
    downloadLoadedLatencyMs: results.getDownLoadedLatencyPoints(),
    uploadLoadedLatencyMs: results.getUpLoadedLatencyPoints(),
    // No TURN server is configured, so packet loss is never measured.
    packetLoss: null,
    error: null,
    providerVersion: PROVIDER_VERSION,
  };
}

function failure(message: string): NetworkSubmission {
  return {
    download: [],
    upload: [],
    idleLatencyMs: [],
    downloadLoadedLatencyMs: [],
    uploadLoadedLatencyMs: [],
    packetLoss: null,
    error: message.slice(0, 500),
    providerVersion: PROVIDER_VERSION,
  };
}

/** Executes one provider request from the backend. */
export async function runNetworkProvider(request: ProviderRequest): Promise<void> {
  const { token } = request;
  const active = () => {
    const run = useBenchmarkStore.getState().run;
    return run?.runId === request.runId && !isTerminal(run.phase);
  };
  if (!active()) return;
  const submit = (samples: NetworkSubmission) =>
    backend.submitNetworkSamples(token, samples).catch(() => {
      // The run was cancelled or timed out; the backend already moved on.
    });

  if (typeof PerformanceResourceTiming === "undefined" || !performance.getEntriesByName) {
    await submit(failure("Resource timing is unavailable in this window."));
    return;
  }

  let SpeedTest: typeof import("@cloudflare/speedtest").default;
  try {
    SpeedTest = (await import("@cloudflare/speedtest")).default;
  } catch {
    await submit(failure("The internet test provider could not be loaded."));
    return;
  }

  // Loading the provider chunk can race cancellation or a new run.
  if (!active()) return;

  let engine: InstanceType<typeof SpeedTest>;
  try {
    engine = new SpeedTest({
      autoStart: false,
      measurements: toMeasurements(request.measurements),
      logMeasurementApiUrl: null,
      // Only the default endpoint may receive results, and only on opt-in.
      ...(request.shareResults ? {} : { logAimApiUrl: null }),
      measureDownloadLoadedLatency: true,
      measureUploadLoadedLatency: true,
      // Includes connection establishment and response transfer. The backend
      // also enforces a whole-test deadline.
      bandwidthAbortRequestDuration: 30_000,
    });
  } catch {
    await submit(failure("The internet test provider could not be initialized."));
    return;
  }

  const total = Math.max(1, request.measurements.length);
  let step = 0;
  let lastSent = 0;
  const sendProgress = (force = false) => {
    const now = performance.now();
    if (!force && now - lastSent < PROGRESS_INTERVAL_MS) return;
    lastSent = now;
    const live: NetworkLiveValue[] = [];
    const down = engine.results.getDownloadBandwidth();
    const up = engine.results.getUploadBandwidth();
    const latency = engine.results.getUnloadedLatency();
    if (down != null) live.push({ series: "net.download", value: down / 1e6 });
    if (up != null) live.push({ series: "net.upload", value: up / 1e6 });
    if (latency != null) live.push({ series: "net.latency", value: latency });
    const current = request.measurements[step];
    const label = current
      ? current.kind === "latency"
        ? "Measuring latency"
        : `Measuring ${current.kind} (${Math.round(current.bytes / 1e6) || "<1"} MB requests)`
      : "Measuring";
    void backend
      .reportNetworkProgress(token, { fraction: step / total, label, live })
      .catch(() => {});
  };

  let unsubscribe = () => {};
  let timeout: ReturnType<typeof setTimeout> | undefined;
  try {
    await new Promise<void>((resolve) => {
      let settled = false;
      const finish = (samples?: NetworkSubmission) => {
        if (settled) return;
        settled = true;
        // Detach before pause: providers may synchronously call back on pause.
        engine.onFinish = () => {};
        engine.onError = () => {};
        engine.onPhaseChange = () => {};
        engine.onResultsChange = () => {};
        engine.pause();
        if (samples) void submit(samples).then(resolve);
        else resolve();
      };
      unsubscribe = useBenchmarkStore.subscribe(() => {
        if (!active()) finish();
      });
      // Covers a lost terminal event or stalled IPC, as well as a hung fetch.
      timeout = setTimeout(() => finish(failure("The internet test timed out.")), 480_000);
      engine.onPhaseChange = ({ measurementId }) => {
        step = measurementId;
        sendProgress(true);
      };
      engine.onResultsChange = () => sendProgress();
      engine.onFinish = (results) => {
        try {
          finish(toSubmission(results));
        } catch {
          finish(failure("The internet test returned unreadable measurements."));
        }
      };
      engine.onError = (message) => {
        finish(failure(message));
      };
      if (!active()) finish();
      else {
        try {
          engine.play();
        } catch {
          finish(failure("The internet test provider could not be started."));
        }
      }
    });
  } finally {
    clearTimeout(timeout);
    unsubscribe();
  }
}
