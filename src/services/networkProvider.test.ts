import { beforeEach, describe, expect, it, vi } from "vitest";
import type { NetworkSubmission } from "@/types";

const backend = vi.hoisted(() => ({
  submitNetworkSamples: vi.fn(),
  reportNetworkProgress: vi.fn(),
}));
const engines = vi.hoisted(() => [] as { config: Record<string, unknown>; instance: FakeEngine }[]);

interface FakeEngine {
  onFinish: (results: unknown) => void;
  onError: (message: string) => void;
  play: () => void;
}

vi.mock("@/services/ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/services/ipc")>();
  return { ...actual, backend };
});

vi.mock("@cloudflare/speedtest", () => {
  class SpeedTest {
    onPhaseChange = () => {};
    onResultsChange = () => {};
    onFinish: (results: unknown) => void = () => {};
    onError: (message: string) => void = () => {};
    isRunning = false;
    results = {
      getDownloadBandwidth: () => undefined,
      getUploadBandwidth: () => undefined,
      getUnloadedLatency: () => undefined,
    };
    constructor(config: Record<string, unknown>) {
      engines.push({ config, instance: this });
    }
    play() {
      this.onFinish(fakeResults);
    }
    pause() {}
  }
  return { default: SpeedTest };
});

const fakeResults = {
  getDownloadBandwidthPoints: () => [{ bytes: 1e7, duration: 400, bps: 2e8 }],
  getUploadBandwidthPoints: () => [{ bytes: 1e6, duration: 200, bps: 4e7 }],
  getUnloadedLatencyPoints: () => [18, 19, 20],
  getDownLoadedLatencyPoints: () => [30],
  getUpLoadedLatencyPoints: () => [70],
};

// PerformanceResourceTiming is absent in jsdom; the provider requires it.
vi.stubGlobal("PerformanceResourceTiming", class {});

const { runNetworkProvider, toMeasurements, toSubmission } = await import("./networkProvider");

const REQUEST = {
  type: "networkProviderRequest" as const,
  runId: "run-1",
  token: "token-1",
  maxBytes: 25_000_000,
  shareResults: false,
  measurements: [
    { kind: "latency" as const, bytes: 0, count: 20 },
    { kind: "download" as const, bytes: 100_000, count: 5 },
  ],
};

describe("network provider", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    engines.length = 0;
    backend.submitNetworkSamples.mockResolvedValue(undefined);
    backend.reportNetworkProgress.mockResolvedValue(undefined);
  });

  it("maps the backend plan to package measurements", () => {
    expect(toMeasurements(REQUEST.measurements)).toEqual([
      { type: "latency", numPackets: 20 },
      { type: "download", bytes: 100_000, count: 5 },
    ]);
  });

  it("submits raw samples, not computed results", () => {
    const s = toSubmission(fakeResults as never);
    expect(s.download).toEqual([{ bytes: 1e7, durationMs: 400 }]);
    expect(s.idleLatencyMs).toEqual([18, 19, 20]);
    expect(s.packetLoss).toBeNull();
  });

  it("disables result logging unless the user opted in", async () => {
    await runNetworkProvider(REQUEST);
    expect(engines[0]?.config.logAimApiUrl).toBeNull();
    expect(engines[0]?.config.logMeasurementApiUrl).toBeNull();
    expect(engines[0]?.config.autoStart).toBe(false);
    const [token, samples] = backend.submitNetworkSamples.mock.calls[0] as [
      string,
      NetworkSubmission,
    ];
    expect(token).toBe("token-1");
    expect(samples.error).toBeNull();

    await runNetworkProvider({ ...REQUEST, shareResults: true });
    expect(engines[1]?.config).not.toHaveProperty("logAimApiUrl");
  });

  it("the pinned package still exposes the results-logging option we disable", () => {
    // If a package update renames the option, sharing could silently turn on.
    const files = import.meta.glob<string>(
      [
        "/node_modules/@cloudflare/speedtest/dist/speedtest.d.ts",
        "/node_modules/@cloudflare/speedtest/package.json",
      ],
      { query: "?raw", import: "default", eager: true },
    );
    const types = files["/node_modules/@cloudflare/speedtest/dist/speedtest.d.ts"] ?? "";
    const version = JSON.parse(
      files["/node_modules/@cloudflare/speedtest/package.json"] ?? "{}",
    ) as { version?: string };
    expect(types).toContain("logAimApiUrl: string | null");
    expect(version.version).toBe("1.15.0");
  });
});
