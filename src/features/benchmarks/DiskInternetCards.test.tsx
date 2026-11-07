import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { runBenchmark } from "@/services/benchmarks";
import { useBenchmarkStore } from "@/stores/benchmarkStore";
import type {
  BenchmarkResult,
  BenchmarkStatus,
  DiskPhaseResult,
  DiskPlan,
  DiskTargetInfo,
  InternetPlan,
} from "@/types";
import { BenchmarksPage } from "./BenchmarksPage";

const api = vi.hoisted(() => ({ diskPlan: vi.fn(), internetPlan: vi.fn() }));

vi.mock("@/services/ipc", async (orig) => {
  const real = await orig<typeof import("@/services/ipc")>();
  return {
    ...real,
    isBackendAvailable: () => true,
    backend: { ...real.backend, diskPlan: api.diskPlan, internetPlan: api.internetPlan },
  };
});
vi.mock("@/services/benchmarks", () => ({
  initBenchmarks: vi.fn(),
  runBenchmark: vi.fn(),
  loadGpuAdapters: vi.fn(),
  loadDiskTargets: vi.fn(),
  cancelBenchmark: vi.fn(),
  deleteBenchmarkResult: vi.fn(),
}));

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

const HOME: DiskTargetInfo = {
  id: "home",
  label: "Home folder (/home/me)",
  directory: "/home/me",
  mountPoint: "/",
  fileSystem: "ext4",
  device: "/dev/nvme0n1p2",
  freeBytes: 500e9,
  totalBytes: 900e9,
  recommended: true,
  notes: [],
};
const OVERLAY: DiskTargetInfo = {
  ...HOME,
  id: "tmp",
  label: "Temporary folder",
  directory: "/tmp",
  fileSystem: "overlay",
  recommended: false,
  notes: ["This is a container's overlay filesystem; results may not reflect a physical drive."],
};

const PLAN: DiskPlan = {
  targetId: "home",
  directory: "/home/me",
  fileBytes: 1_073_741_824,
  estimatedBytesWritten: 2_300_000_000,
  estimatedSeconds: 8,
  blocked: null,
};
const NET_PLAN: InternetPlan = {
  provider: "Cloudflare speed test",
  host: "speed.cloudflare.com",
  maxDownloadBytes: 800e6,
  maxUploadBytes: 300e6,
  estimatedSeconds: 30,
};

function phase(over: Partial<DiskPhaseResult>): DiskPhaseResult {
  return {
    kind: "sequentialRead",
    label: "Sequential read · 1 MiB",
    blockBytes: 1 << 20,
    concurrency: 1,
    queueDepthExact: true,
    operations: 1000,
    bytes: 1e9,
    seconds: 1,
    samples: [5000],
    medianMbps: 5100,
    meanMbps: 5100,
    minMbps: 5000,
    maxMbps: 5200,
    cvPercent: 1.2,
    outlierIndices: [],
    medianIops: 4800,
    latency: {
      samples: 1000,
      meanUs: 200,
      p50Us: 196,
      p95Us: 220,
      p99Us: 244,
      maxUs: 400,
      histogram: [],
    },
    validated: true,
    note: null,
    ...over,
  };
}

const BASE: BenchmarkResult = {
  id: "disk-1-0",
  kind: "disk",
  profile: "standard",
  startedAtMs: 1_800_000_000_000,
  durationMs: 5000,
  method: {
    methodId: "disk-direct-v1",
    scoreVersion: "disk-score-v1",
    comparability: "platformSpecific",
  },
  environment: {
    platform: "linux",
    distribution: "Debian GNU/Linux",
    kernel: "6.12",
    architecture: "x86_64",
    cpuModel: "Test CPU",
    physicalCores: 4,
    logicalCores: 8,
    totalMemoryBytes: 1,
    powerSource: "ac",
    cpuGovernor: "powersave",
    virtualization: null,
    backgroundCpuPercent: 2,
    developmentBuild: false,
  },
  workloads: [],
  sustained: null,
  scores: null,
  gpu: null,
  gpuScore: null,
  disk: {
    target: HOME,
    device: { name: "nvme0n1", model: "Test NVMe", rotational: false, transport: "nvme" },
    ioMode: "direct",
    ioModeNote: null,
    fileBytes: 1_073_741_824,
    bytesWritten: 2_300_000_000,
    bytesRead: 5_700_000_000,
    phases: [
      phase({}),
      phase({
        kind: "randomRead",
        label: "Random read · 4 KiB · QD32",
        blockBytes: 4096,
        concurrency: 32,
        medianIops: 490_000,
        medianMbps: 2000,
      }),
    ],
    notes: [],
  },
  diskScore: 1010,
  internet: null,
  warnings: [],
  stability: "high",
  outlierMethod: "1.5×IQR; flagged, none removed",
  affinity: "not pinned",
};

const NET: BenchmarkResult = {
  ...BASE,
  id: "net-1-0",
  kind: "internet",
  method: {
    methodId: "internet-cloudflare-v1",
    scoreVersion: "none",
    comparability: "crossPlatform",
  },
  disk: null,
  diskScore: null,
  internet: {
    provider: "Cloudflare speed test",
    host: "speed.cloudflare.com",
    serverLocation: "AMS",
    path: { interface: "wlo1", kind: "wifi", vpnSuspected: false },
    idleLatency: { samples: 20, minMs: 8, medianMs: 11, p95Ms: 19, maxMs: 25, jitterMs: 2.4 },
    download: {
      medianMbps: 412.6,
      meanMbps: 410,
      peakMbps: 480,
      samples: [400, 420],
      bytes: 500e6,
      seconds: 10,
      connections: 6,
      loadedLatency: { samples: 30, minMs: 12, medianMs: 48, p95Ms: 90, maxMs: 120, jitterMs: 9 },
    },
    upload: {
      medianMbps: 38.2,
      meanMbps: 38,
      peakMbps: 41,
      samples: [37, 39],
      bytes: 100e6,
      seconds: 10,
      connections: 4,
      loadedLatency: null,
    },
    bytesDownloaded: 500e6,
    bytesUploaded: 100e6,
    packetLossPercent: null,
    notes: ["Packet loss is not measured: HTTP cannot tell a lost packet from a retransmission."],
  },
};

beforeEach(() => {
  vi.mocked(runBenchmark).mockClear();
  api.diskPlan.mockReset().mockResolvedValue(PLAN);
  api.internetPlan.mockReset().mockResolvedValue(NET_PLAN);
  useBenchmarkStore.setState({
    status: IDLE,
    history: [],
    latest: null,
    selectedId: null,
    adapters: null,
    adaptersError: null,
    diskTargets: [HOME, OVERLAY],
    diskTargetsError: null,
  });
});

describe("Disk card", () => {
  it("states what a run would write before it starts, and runs on the chosen location id", async () => {
    render(<BenchmarksPage />);
    const plan = await screen.findByTestId("disk-plan");
    expect(plan).toHaveTextContent(/temporary 1\.1 GB file in \/home\/me/i);
    expect(plan).toHaveTextContent(/writes about 2\.3 GB/i);
    expect(plan).toHaveTextContent(/wear/i);
    await userEvent.click(screen.getByRole("button", { name: /run disk benchmark/i }));
    expect(runBenchmark).toHaveBeenCalledWith("disk", "standard", { diskTarget: "home" });
  });

  it("warns about a container filesystem and re-plans when another location is picked", async () => {
    render(<BenchmarksPage />);
    await screen.findByTestId("disk-plan");
    await userEvent.selectOptions(screen.getByLabelText("Location"), "tmp");
    expect(screen.getByText(/container's overlay filesystem/i)).toBeInTheDocument();
    await waitFor(() => expect(api.diskPlan).toHaveBeenCalledWith("tmp", "standard"));
  });

  it("does not offer a run when the location is blocked, and says why", async () => {
    api.diskPlan.mockResolvedValue({ ...PLAN, fileBytes: 0, blocked: "Not enough free space." });
    render(<BenchmarksPage />);
    expect(await screen.findByText("Not enough free space.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /run disk benchmark/i })).toBeDisabled();
  });

  it("shows score, speeds, response times, verification and the wear this run caused", () => {
    useBenchmarkStore.setState({ latest: BASE, history: [BASE] });
    render(<BenchmarksPage />);
    const panel = within(screen.getByRole("region", { name: /latest disk result/i }));
    expect(panel.getByText("1010")).toBeInTheDocument();
    expect(
      panel.getByText(/direct i\/o: the operating system's cache was bypassed/i),
    ).toBeInTheDocument();
    expect(panel.getByText("5.10 GB/s")).toBeInTheDocument();
    expect(panel.getByText("490 k IOPS")).toBeInTheDocument();
    expect(panel.getAllByText("Verified")).toHaveLength(2);
    expect(panel.getByText(/wrote 2\.3 GB to the drive and read 5\.7 GB/i)).toBeInTheDocument();
    expect(panel.getByText(/Test NVMe/)).toBeInTheDocument();
    expect(panel.getByText(/disk-direct-v1/)).toBeInTheDocument();
  });

  it("says a buffered run is not scored and why", () => {
    const buffered: BenchmarkResult = {
      ...BASE,
      diskScore: null,
      method: { ...BASE.method, methodId: "disk-buffered-v1" },
      disk: { ...BASE.disk!, ioMode: "buffered", ioModeNote: "O_DIRECT is not supported here." },
    };
    useBenchmarkStore.setState({ latest: buffered, history: [buffered] });
    render(<BenchmarksPage />);
    const panel = within(screen.getByRole("region", { name: /latest disk result/i }));
    expect(
      panel.getByText(/only direct i\/o, which measures the drive itself, is scored/i),
    ).toBeInTheDocument();
    expect(panel.getByText(/cache is part of what was measured/i)).toBeInTheDocument();
    expect(panel.getByText(/O_DIRECT is not supported here/)).toBeInTheDocument();
  });

  it("explains when no location is available", () => {
    useBenchmarkStore.setState({ diskTargets: [] });
    render(<BenchmarksPage />);
    expect(screen.getByText(/no writable drive location/i)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /run disk benchmark/i })).not.toBeInTheDocument();
  });
});

describe("Internet card", () => {
  it.each(["profile", "method"] as const)(
    "does not compare runs with a different %s",
    (difference) => {
      const previous: BenchmarkResult = {
        ...NET,
        id: "earlier",
        startedAtMs: NET.startedAtMs - 1_000,
        ...(difference === "profile"
          ? { profile: "quick" as const }
          : { method: { ...NET.method, methodId: "old-method" } }),
        internet: {
          ...NET.internet!,
          download: {
            ...NET.internet!.download!,
            medianMbps: NET.internet!.download!.medianMbps / 2,
          },
        },
      };
      useBenchmarkStore.setState({ latest: NET, history: [NET, previous] });
      render(<BenchmarksPage />);
      const panel = within(screen.getByRole("region", { name: /latest internet result/i }));
      expect(panel.queryByText("+100.0%")).not.toBeInTheDocument();
    },
  );

  it("compares internet runs with the same profile and method", () => {
    const previous: BenchmarkResult = {
      ...NET,
      id: "earlier",
      startedAtMs: NET.startedAtMs - 1_000,
      internet: {
        ...NET.internet!,
        download: {
          ...NET.internet!.download!,
          medianMbps: NET.internet!.download!.medianMbps / 2,
        },
      },
    };
    useBenchmarkStore.setState({ latest: NET, history: [NET, previous] });
    render(<BenchmarksPage />);
    expect(
      within(screen.getByRole("region", { name: /latest internet result/i })).getByText("+100.0%"),
    ).toBeInTheDocument();
  });
  it("tells the user the host, the data volume and what is not sent before running", async () => {
    render(<BenchmarksPage />);
    const plan = await screen.findByTestId("internet-plan");
    expect(plan).toHaveTextContent("speed.cloudflare.com");
    expect(plan).toHaveTextContent(/up to 800 MB down and 300 MB up/);
    expect(plan).toHaveTextContent(/data cap/i);
    expect(plan).toHaveTextContent(/no files or information about this computer are sent/i);
    expect(plan).toHaveTextContent(/asked to confirm/i);
    await userEvent.click(screen.getByRole("button", { name: /run internet test/i }));
    expect(runBenchmark).toHaveBeenCalledWith("internet", "standard");
  });

  it("shows speeds, latency, jitter, data used and no score or packet-loss figure", () => {
    useBenchmarkStore.setState({ latest: NET, history: [NET] });
    render(<BenchmarksPage />);
    const panel = within(screen.getByRole("region", { name: /latest internet result/i }));
    expect(panel.getAllByText("413 Mbit/s").length).toBeGreaterThan(0);
    expect(panel.getAllByText("38.2 Mbit/s").length).toBeGreaterThan(0);
    expect(panel.getByText(/median 11 ms/)).toBeInTheDocument();
    expect(panel.getByText(/jitter 2\.4 ms/i)).toBeInTheDocument();
    expect(panel.getByText(/moved 500 MB down and 100 MB up/i)).toBeInTheDocument();
    expect(panel.getByText(/packet loss is not measured/i)).toBeInTheDocument();
    expect(panel.getByText(/server AMS/)).toBeInTheDocument();
    expect(panel.getByText(/via wlo1 \(Wi-Fi\)/)).toBeInTheDocument();
    expect(panel.getByText(/no internet score/i)).toBeInTheDocument();
    expect(panel.queryByText(/not scored/i)).not.toBeInTheDocument();
  });

  it("shows a failed run's reason", () => {
    useBenchmarkStore.setState({
      status: {
        ...IDLE,
        state: "failed",
        kind: "internet",
        message:
          "speed.cloudflare.com did not respond in time. Check the connection and try again.",
      },
    });
    render(<BenchmarksPage />);
    expect(screen.getByRole("alert", { name: "" })).toBeDefined();
    expect(screen.getByText(/did not respond in time/)).toBeInTheDocument();
  });
});
