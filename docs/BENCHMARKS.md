# Benchmarks

Task Manager includes its own benchmark suite for the CPU, GPU, storage and
internet connection. This document describes how it works, what each number
means, and its limits. The design history is in
[BENCHMARK_IMPLEMENTATION_PLAN.md](BENCHMARK_IMPLEMENTATION_PLAN.md).

Versions: methodology **1.0.0**, score version **1**, record schema **1**.

> These are Task Manager's own measurements. They are not equivalent to, and
> must not be compared with, Cinebench, Geekbench, 3DMark, CrystalDiskMark,
> Speedtest by Ookla or any other product. There are no global rankings:
> results are compared only with your own earlier runs.

## Architecture

```
React (features/benchmarks) ──invoke──► commands/benchmarks.rs ──► tm-core benchmarks/
  BenchmarksPage, PreflightDialog          validates every argument     BenchmarkCoordinator
  RunPanel, ResultView, CompareView    ◄── Channel<BenchmarkEvent> ──      ├ one run at a time, CancelToken
  services/networkProvider.ts ──submit_network_samples──► NetworkInbox  ├ Benchmark Mode guard → Engine
                                                                        ├ cpu/ gpu/ disk/ network/
                                                                        ├ statistics, scoring, confidence
                                                                        └ HistoryStore (JSON per run)
```

| Module (`src-tauri/core/src/benchmarks/`) | Responsibility                                                                |
| ----------------------------------------- | ----------------------------------------------------------------------------- |
| `coordinator.rs`                          | Run slot, sequencing, cooldowns, interference, finalization, persistence      |
| `cancellation.rs`                         | Cooperative cancellation and per-profile run timeouts                         |
| `progress.rs`                             | Event throttling (≤ 8 progress messages/s, ≤ 32 live samples each)            |
| `statistics.rs`                           | Summaries, percentiles, MAD outliers, jitter, geometric mean                  |
| `scoring.rs`                              | Reference constants and score formulas (score version 1)                      |
| `confidence.rs`                           | High / Medium / Low consistency heuristic and derived warnings                |
| `environment.rs`                          | Pre-run snapshot, load assessment, hardware fingerprint, interference monitor |
| `thermal.rs`                              | Temperature and OS-reported frequency tracking                                |
| `history.rs`                              | Atomic JSON records, strict id validation, 500-run cap                        |
| `report.rs`                               | Comparison, CSV / Markdown / summary export                                   |
| `cpu/`, `gpu/`, `disk/`, `network/`       | Category benchmarks                                                           |
| `mock.rs`                                 | Deterministic mocks and the simulated mode                                    |

All workloads run in Rust on a dedicated `benchmark-runner` thread. The only
exception is the internet provider, which must run in the WebView (see
below); even then Rust plans the test, enforces limits, validates the raw
samples and computes every statistic.

### Run lifecycle

`Idle → Preparing → (WarmingUp → Running)* → [CoolingDown] → Finalizing →
Completed | Cancelled | Failed`. A full run executes **CPU → cooldown → GPU →
storage → internet**: the two heat-producing categories come first with a
cooldown between them (Standard 10 s, Extended 30 s, none in Quick,
skippable), storage is insensitive to GPU heat, and the internet test runs
last so no network traffic overlaps a local measurement. The internet test is
only part of a full run when explicitly ticked.

Only completed runs are stored. Cancelled or failed runs never touch history.
A run with some failed or unsupported categories is still stored, with each
category's status. If no category completes, the run fails.

### Benchmark Mode

While a run is active the monitoring engine polls no faster than every 2 s
(`BENCHMARK_MODE_MIN_INTERVAL_MS`) and marks its ticks `benchmarkActive`. The
top bar shows "benchmark mode". The user's configured interval is not changed:
an RAII guard switches the mode off on every exit path.

### Interference

A small monitor thread reads the engine's existing snapshots (no extra OS
polling) and records CPU used by other processes (total CPU minus this app).
A mean above 15 % adds "Background activity detected during this
benchmark" and lowers confidence. Before a run, ≥ 15 % (Elevated) or ≥ 30 %
(High) background CPU triggers a warning with **Run anyway**, **Wait for the
system to become idle** (re-checks every 3 s, up to 2 minutes) and
**Cancel**. Processes are never terminated.

## Statistics

- Timing uses the monotonic clock (`std::time::Instant`). Warmup is never
  measured.
- Summaries: n, mean, median, min, max, sample standard deviation (n − 1),
  coefficient of variation, p50/p90/p95/p99 (linear interpolation, NumPy's
  default definition).
- Outliers: modified z-score > 3.5 (|x − median| > 3.5 × 1.4826 × MAD). They
  are flagged and kept; summaries include them.
- A coefficient of variation above 5 % adds "Result variation was higher than
  expected".
- **Confidence** (High / Medium / Low) is a consistency indicator, not a
  statistical confidence interval: Low if the worst CV is above 10 %, there
  are fewer than 3 samples, or background CPU averaged above 25 %; Medium if
  the CV is above 3 %, any outlier was flagged, there are fewer than 5
  samples, background CPU was above 10 %, or sustained performance declined.
- Raw samples are always stored. Non-finite values are never stored.

## CPU

| Workload                        | Work unit | Notes                                                                                           |
| ------------------------------- | --------- | ----------------------------------------------------------------------------------------------- |
| Integer                         | Mops/s    | xor/rotate/multiply/shift/xor/add over a 64 KiB table (6 ops per element)                       |
| Floating point                  | MFLOP/s   | 2-D rotation of 256 independent points (4 mul + 2 add per step); values stay on the unit circle |
| SHA-256 throughput              | MB/s      | `sha2` over a 4 MiB buffer in 256 KiB blocks, digests chained                                   |
| LZ4 compression / decompression | MB/s      | `lz4_flex` block format over 8 MiB of deterministic, moderately compressible data               |

- Each workload runs **single-core** (one worker thread) and then
  **multi-core** (one worker per logical processor, overridable 1–256). All
  single-core tests run before any multi-core test.
- Workers start together at a barrier. Each iteration has a fixed amount of
  work, derived from the warmup rate. It is timed between barriers on the
  coordinating thread, so start-up skew is excluded and the slowest worker
  bounds the iteration.
- Warmup 1 / 2 / 3 s; iterations 5 / 7 / 10 of about 200 / 300 / 400 ms
  (Quick / Standard / Extended). The reported value is the median.
- **Scaling** = multi-core ÷ single-core integer throughput, shown with the
  worker count. Per-worker spread is listed.
- **Sustained** (Extended): multi-core integer in 5 s windows (default 60 s,
  30–300 s). Reports peak, sustained (median of the final third) and drop %.
  A drop above 5 % says "Sustained performance decreased during the test.
  Thermal or power limits may be contributing." Throttling is never
  asserted.
- Inputs and results pass through `std::hint::black_box`; kernel state
  carries between chunks so no work can be removed or precomputed.
- Temperature (sysinfo components, hottest CPU-like sensor) and OS-reported
  frequency are recorded before, during and after. When no sensor is
  readable: "Temperature monitoring unavailable on this system." Frequency is
  never inferred from timing. Windows often reports a constant nominal value,
  and the result says so.

## GPU

- Adapters are enumerated through wgpu on Direct3D 12 / Vulkan (Metal later),
  with stable ids `backend:vendor:device:ordinal`. Default selection is the
  adapter the system reports as high-performance, then discrete →
  integrated → virtual → other. Software (CPU) adapters are used only when
  chosen or when nothing else exists. Names are never used for selection.
- Only features and limits the adapter reports are requested. Buffers are
  limited to the adapter's binding and buffer limits and a 256 MiB cap.
  Dispatches are calibrated to about 60 ms, far below the OS GPU watchdog.
- **Compute (FMA)**: WGSL kernel with 4 independent vec4 FMA chains =
  32 FLOPs per iteration per invocation (an FMA counts as 2). Multiplier and
  addend come from a uniform buffer, so nothing can be folded. GFLOP/s =
  invocations × iterations × 32 ÷ time. This is a measured synthetic
  throughput, not a manufacturer peak.
- **Memory copy**: full-buffer `copy_buffer_to_buffer`, reported as bytes read
  plus bytes written per second (GB/s, 10⁹).
- **Memory read/write kernel**: reads and writes one vec4 per invocation,
  2 × buffer bytes per pass.
- **Timing method** is recorded per subtest: `gpu_timestamp` (timestamp
  queries around the pass) when supported, otherwise
  `wall_clock_synchronized` (CPU clock from submission until `Device::poll`
  confirms completion). Submission-only timing is never used.
- Integrated GPUs get a warning that memory results are not comparable with
  discrete cards.
- **Sustained** (Extended, default 60 s) runs compute in 5 s windows, with a
  warning shown before the run. No power-virus kernels are used.
- wgpu errors are captured by our own handlers (never wgpu's default
  panicking handler). Device loss and errors fail only the GPU category. With
  no adapter the category is "Unsupported by this GPU/backend". GPU
  temperature is not available.

## Storage

- **Location**: one of the engine's mounted, writable volumes, selected by
  mount point and checked against the live list (never a free-form path).
  Files live in `<app cache>/benchmark` on the app's own volume and in
  `<mount>/.taskmanager-benchmark/` elsewhere. The folder is canonicalized
  and must be a real directory inside the selected volume (symlinks and
  escapes are refused).
- **File**: `.taskmanager-benchmark-<uuid v4>.tmp`, created with create-new
  semantics (existing files are never touched). It is deleted on drop: after
  completion, cancellation and errors. At start-up, abandoned files are swept,
  but only exact-pattern regular files inside known benchmark folders, without
  recursion or following symlinks.
- **Size**: 512 MiB / 1 GiB / 4 GiB by profile, reduced so that
  max(10 % of capacity, 5 GiB) stays free and within the write limit
  (Conservative 1 GiB, Standard 4 GiB, Extended 16 GiB). Below 256 MiB the
  test is refused with an explanation. The amount written is shown before the
  run and recorded afterwards.
- **I/O mode**: direct I/O (Linux `O_DIRECT`; Windows
  `FILE_FLAG_NO_BUFFERING | FILE_FLAG_WRITE_THROUGH`) with 4 KiB-aligned
  buffers, offsets and sizes. If the filesystem refuses, the test falls back
  to buffered I/O, says "I/O Mode: Buffered" and warns about cache effects
  (and that read results may reflect RAM when the file is smaller than
  memory). OS caches are never flushed destructively and no elevated rights
  are needed.
- **Sequential write**: one 1 MiB-block segment per iteration, each ending
  with `sync_all` inside the timed region. This is also how the file is
  created; with writes disabled the creation is untimed and disclosed.
- **Sequential read**: the file read back in 1 MiB blocks.
- **Random 4 KiB read / write**: QD1 (one thread, one outstanding request)
  and "8 threads × QD1" (eight threads with one synchronous request each).
  This is labelled honestly and is not an asynchronous QD8. Durations are
  2 / 4 / 8 s. IOPS come from 250 ms windows. Per-operation latency gives the
  mean and p50/p95, plus p99 when there are ≥ 100 samples. Random writes are
  capped (256 MiB / 512 MiB / 1 GiB, at most a quarter of the write limit).
- **Flush semantics**: direct I/O is write-through, but drives may still
  acknowledge writes from their own cache, so short write tests can exceed
  sustained flash speed. This is stated with the result.

## Internet

- **Provider**: the official `@cloudflare/speedtest` package (MIT, pinned to
  1.15.0) runs in the WebView, because it relies on the browser's Resource
  Timing API. Rust sends a measurement plan sized for the data profile (Data
  Saver ≈ 25 MB, Standard ≈ 200 MB, Maximum ≈ 1 GB, all upper bounds; the
  package stops escalating payload sizes once a size takes over a second).
  The WebView reports progress and submits raw samples over a single-use,
  token-guarded inbox. Rust validates every value and the total bytes.
- The provider abstraction (`NetworkProvider`) allows other providers, such
  as a self-hosted endpoint, to be added without UI changes.
- **Download / upload** (Mbps, 10⁶ bit/s): bytes × 8 ÷ transfer time per
  request. The headline is the 90th percentile of transfers lasting
  ≥ 250 ms; shorter transfers are latency-limited and stay only in the raw
  samples.
- **Latency**: median of ≥ 20 idle round trips (minimum and p95 listed).
  **Jitter**: mean absolute difference of consecutive round trips.
  **Loaded latency**: measured during download and during upload. A rise to
  more than twice idle and by more than 30 ms adds "Latency increased
  substantially while the connection was busy". No router diagnosis is made.
- **Packet loss**: needs a TURN server we do not operate, so it is reported
  as "Packet loss measurement unavailable with this provider". Failed
  requests are never counted as packet loss.
- **Context**: interface name, Wi-Fi / Ethernet when the name makes it
  unambiguous, and IPv4/IPv6 availability. Addresses and the public IP are not
  stored. Active adapters named like VPNs or tunnels (WireGuard, TUN/TAP,
  Tailscale, …) produce "A VPN or network tunnel may affect these results."
- **Privacy**: see below. CSP `connect-src` allows exactly
  `https://speed.cloudflare.com`. TLS is the WebView's normal certificate
  validation.

## Scoring (score version 1)

- Metric score = 1000 × measured ÷ reference (inverted for lower-is-better
  metrics). Category score = geometric mean of its metric scores. Only
  metrics listed in `scoring.rs` are scored. Latencies, ratios and bytes are
  shown but unscored.
- Reference constants are round numbers close to the development machine
  (2024 laptop, Intel Core 7 150U with integrated Intel Graphics, NVMe SSD,
  on AC power), so **about 1000 means "comparable to that laptop"**.
- **System score** = geometric mean of the CPU, GPU and storage category
  scores present. It is only computed with at least two of the three, and is
  labelled **Partial Benchmark** when one is missing. Skipped categories are
  never counted as zero. A geometric mean keeps one very strong component from
  hiding a weak one.
- **Internet is excluded** from the system score: it describes the path to a
  remote server and changes with time of day and ISP, not the computer.
- Changing any constant or formula requires a new `SCORE_VERSION`. Changing
  any workload, sizing, timing or statistic requires a new `METHOD_VERSION`.

## Comparison and history

- Records live in `<app data>/benchmarks/results/<uuid>.json`, written
  atomically. Unreadable or foreign files are skipped, never deleted. At most
  500 runs are kept, and history can be disabled in settings.
- Comparison refuses to compute changes across different methodology major
  versions or score versions. A hardware fingerprint change (CPU, logical
  processors, memory, tested GPU or drive) shows "Hardware configuration
  changed. Direct comparison may not be meaningful." OS updates are listed
  but not treated as hardware changes.
- A change smaller than the combined variation √(cv₁² + cv₂²) (minimum 2 %)
  is shown as "within variation" rather than better or worse.
- Trend charts use only non-simulated runs of the current methodology and
  score versions.

## Export

JSON (complete record, including raw samples), CSV (one row per metric with
statistics and raw samples; formula-injection guarded) and a Markdown report
are saved through a native save dialog. "Copy summary" puts a short text
summary on the clipboard. Exports contain the hardware summary,
measurements, scores, versions and warnings only: no process lists, host
names, IP addresses or user files.

## Privacy

- Nothing runs automatically and results are never uploaded.
- Before the first internet test a notice explains that the test server sees
  the public IP address, that the test uses internet data, and that the test
  is optional. The user must tick a consent box.
- Cloudflare's optional collection of anonymized results (`logAimApiUrl`) is
  **off** unless enabled in Settings → Benchmarks. A test fails if the pinned
  package stops exposing that option.
- Upload payloads are generated bytes, never user data. The record schema has
  no personal fields, so opt-in anonymous comparison could be added later.

## Security

- No command, URL or path parameters: requests are enums, bounded numbers,
  adapter ids and a mount point checked against the engine's list. History
  ids must be UUIDs, so ids cannot escape the history folder.
- Resource caps: CPU workers ≤ 256, CPU buffers ≈ 12 MiB of shared input plus
  small per-worker state, GPU buffers ≤ adapter limits and 256 MiB, disk
  free-space reserve and write limit, network byte cap. Each profile has a
  maximum run time (15 / 30 / 90 minutes).
- One run at a time (`BenchmarkAlreadyRunning`). Benchmark code avoids
  `unwrap`/`expect`. In debug builds a panic in the runner thread becomes a
  failed run (release builds use `panic = "abort"`, so production paths must
  not panic).
- Closing the app cancels a running benchmark and waits up to 5 s for cleanup.
  If the WebView reloads during a run, the run continues headless and the
  Benchmarks page offers to cancel it.

## Platform differences

| Topic             | Windows 11                               | Debian / Ubuntu                                               | macOS (not yet supported)  |
| ----------------- | ---------------------------------------- | ------------------------------------------------------------- | -------------------------- |
| GPU backend       | Direct3D 12, Vulkan                      | Vulkan (needs a Vulkan driver/loader)                         | Metal (compiled, untested) |
| Direct I/O        | `FILE_FLAG_NO_BUFFERING` + write-through | `O_DIRECT` (tmpfs and some FUSE mounts fall back to buffered) | Buffered only              |
| Temperatures      | Usually unavailable without elevation    | hwmon via sysinfo                                             | Not implemented            |
| CPU frequency     | Often a constant nominal value           | Current frequency                                             | Not implemented            |
| Power source      | `GetSystemPowerStatus`                   | `/sys/class/power_supply`                                     | Unknown                    |
| Internet provider | WebView2                                 | WebKitGTK                                                     | WKWebView                  |

## Known limitations

- The internet provider depends on Resource Timing working in the WebView.
  If it does not, the test fails with a clear message instead of reporting
  wrong numbers. Verified behaviour in WebView2 and WebKitGTK still needs a
  manual run of the packaged app.
- Packet loss is not measured.
- GPU utilisation and temperature are not measured.
- "8 threads × QD1" is not an asynchronous queue depth; true QD8/QD32 would
  need IOCP / io_uring.
- Short random-write tests may be absorbed by the drive's internal cache.
- The score references come from a single development machine and are
  arbitrary anchors.

## Testing

- `cargo test --workspace` covers statistics, scoring golden values,
  confidence, unit conversions, IOPS/latency maths, disk sizing and safety,
  file naming and the sweep, path validation, the coordinator state machine,
  cancellation, the single-run lock, history, comparison, export, GPU
  selection and limits, network validation and analysis, and the WebView
  handshake. None of these load the hardware.
- Hardware tests are opt-in:
  `cargo test --release -p tm-core bench_ -- --ignored --nocapture --test-threads=1`
  (`bench_cpu_quick`, `bench_gpu_quick`, `bench_disk_quick`, which writes
  about 0.8 GB, `bench_full_quick` and `bench_list_gpu_adapters`).
- Simulated mode (debug builds, or `TM_BENCH_SIMULATED=1`) runs deterministic
  mocks through the real orchestration, progress, cancellation and
  persistence for UI testing without load.
- Vitest covers the store, formatting, the Benchmarks page (preflight
  choices, consent, accessible progress, cancel), comparison and the network
  provider with a mocked package.
