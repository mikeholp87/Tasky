import type { BenchmarkKind } from "@/types";

/** Plain-language methodology summaries shown on each category page. */
export const METHODOLOGY: Record<BenchmarkKind, string[]> = {
  cpu: [
    "Single-core tests run one worker thread; multi-core tests run one identical, independent workload per logical processor, all released at the same moment.",
    "Workloads are deterministic integer, floating-point, SHA-256 hashing and LZ4 compression kernels working on in-memory data, so disk and network activity cannot affect them.",
    "Each test warms up first (not measured), then repeats several timed iterations. The reported value is the median; variation shows how consistent the iterations were.",
  ],
  gpu: [
    "GPU tests run WGSL compute shaders through wgpu (Direct3D 12 or Vulkan) and wait until the GPU has finished, so submission time is never mistaken for completion.",
    "Compute throughput comes from a shader with an exact, known number of floating-point operations. Memory throughput is measured with buffer copies and read/write kernels.",
    "Integrated GPUs share system memory, so their memory results are not directly comparable with discrete graphics cards.",
  ],
  disk: [
    "Storage tests write a temporary file in a dedicated benchmark folder on the selected drive and delete it afterwards, including after cancellation.",
    "Sequential tests use 1 MiB blocks; random tests use 4 KiB blocks and report IOPS and latency percentiles. Direct (unbuffered) I/O is used where the system supports it, so results reflect the drive rather than RAM caching.",
  ],
  network: [
    "The internet test only runs when you start it. It measures download and upload speed, idle latency and latency while the connection is busy, against an external test server that necessarily sees your public IP address.",
    "Speeds are in megabits per second (Mbps). Jitter is the average difference between consecutive latency measurements.",
  ],
};
