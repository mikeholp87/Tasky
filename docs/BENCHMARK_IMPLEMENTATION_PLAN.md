# Benchmark Suite — Implementation Plan

Status: in progress. This document is the design of record for the benchmarking
subsystem; `docs/BENCHMARKS.md` is the user/maintainer-facing methodology
reference produced at the end of the work.

## 1. Current architecture (as found)

| Area                        | Finding                                                                                                                                                                                     |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Tauri                       | 2.12.5 (`tauri-cli` 2.12.4), plugins: opener, notification, autostart, window-state, dialog, single-instance                                                                                |
| Rust                        | edition 2024, `rust-version = "1.90"`, local toolchain 1.99.1 (MSVC)                                                                                                                        |
| Frontend                    | React 19.3, TypeScript 6.0, Vite 8, Tailwind 4, zustand 5, uPlot 1.6, Radix UI, Vitest 5                                                                                                    |
| Workspace                   | `src-tauri/core` (`tm-core`: engine, models, platform layer, no Tauri) + `src-tauri/src` (thin shell: commands, state, settings, tray)                                                      |
| Monitoring                  | One `monitoring-engine` thread (`core/src/monitoring/engine.rs`) owns sysinfo; publishes `MetricsTick` to `TickSink`s; frontend receives ticks over a `tauri::ipc::Channel`                 |
| Models                      | `StaticSystemInfo` (OS, kernel, CPU brand, cores, RAM, app version), `MetricsTick` (CPU %/per-core/frequency, memory, disks with fs/kind/free space, NICs with IPs/primary flag, processes) |
| IPC                         | 17 commands; untrusted-input validation in `commands/validate.rs`; blocking work via `spawn_blocking`                                                                                       |
| Errors                      | `AppError` → `{kind, message}`; `ErrorKind` mirrored exhaustively in `src/services/ipc.ts`                                                                                                  |
| Settings                    | `Settings` (serde `default`, `validate`/`sanitized`) persisted as JSON with atomic writes in `app_config_dir()`                                                                             |
| Persistence                 | No database. History is memory-only by design                                                                                                                                               |
| Charts                      | `LineChart` (uPlot wrapper, in-place `setData`) + `TimeSeriesChart`/`DeviceChart`                                                                                                           |
| GPU / temperature / battery | Not implemented (sysinfo `component`/`gpu` features off)                                                                                                                                    |
| Network access              | None; CSP `connect-src ipc: http://ipc.localhost`                                                                                                                                           |
| Tests                       | 110 Rust tests (inline), 72 Vitest tests; gates in `npm run check:all`                                                                                                                      |

## 2. Proposed benchmark architecture

```
React (features/benchmarks)            Tauri shell (commands/benchmarks.rs)        tm-core (benchmarks/)
───────────────────────────            ────────────────────────────────────        ─────────────────────
BenchmarksPage / detail pages  ─invoke─►  validate args ─────────────────────────►  BenchmarkCoordinator
benchmarkStore (throttled)     ◄Channel─  ChannelProgressSink  ◄──── events ──────   ├ single run slot + CancelToken
services/benchmarks.ts                                                               ├ BenchmarkModeGuard → Engine
  └ Cloudflare JS provider ──submit raw samples──► validate ──────────────────────►  ├ CategoryBenchmark impls (cpu/gpu/disk/network)
                                                                                     ├ statistics / scoring / confidence
                                                                                     └ HistoryStore (JSON per run)
```

- All workloads run in Rust on a dedicated `benchmark-runner` OS thread
  (CPU workers are scoped threads beneath it). The only exception is the
  optional Cloudflare network provider, whose official package must run in
  the WebView; it is driven by a run token issued by the coordinator and
  Rust recomputes every statistic from the raw samples it submits.
- `CategoryBenchmark` trait: one implementation per category plus mocks.
  The coordinator never knows about workload details.
- Progress is pushed through a `tauri::ipc::Channel` (the same mechanism as
  metrics ticks), throttled to ≤ 8 messages/s; state transitions are always
  delivered.
- Benchmark Mode: while a run is active the monitoring engine polls no
  faster than every 2 s and flags ticks with `benchmarkActive`; an RAII
  guard restores it on every exit path.

## 3. Dependencies (verified 2026-11-02 with `cargo info` / `npm view`)

| Crate / package             | Version                                   | MSRV | License        | Use                                                                                                         |
| --------------------------- | ----------------------------------------- | ---- | -------------- | ----------------------------------------------------------------------------------------------------------- |
| `wgpu`                      | 30.0.1                                    | 1.87 | MIT/Apache-2.0 | GPU adapters, compute & memory tests; `default-features = false` + `wgsl`, `dx12`, `vulkan` (`metal` later) |
| `pollster`                  | 1.0.1                                     | 1.69 | MIT/Apache-2.0 | Block on wgpu's async adapter/device requests on the runner thread                                          |
| `sha2`                      | 0.10.9 (already in the tree; 0.11 exists) | —    | MIT/Apache-2.0 | SHA-256 throughput workload (reuse 0.10 to avoid a duplicate)                                               |
| `lz4_flex`                  | 0.14                                      | 1.81 | MIT            | Pure-Rust compression workload (no C toolchain)                                                             |
| `uuid`                      | 1.29 (in the tree)                        | 1.89 | MIT/Apache-2.0 | Run ids, temp file names                                                                                    |
| sysinfo `component` feature | 0.40                                      | —    | MIT            | Temperatures where the OS exposes them                                                                      |
| `@cloudflare/speedtest`     | 1.15.0 (pinned)                           | —    | MIT            | Optional network provider in the WebView                                                                    |

All versions are within the existing `rust-version = 1.90`; no upgrades of
existing dependencies are required.

**Rejected:** SQLite (history volume is tiny; the existing atomic-JSON
pattern suffices and avoids a native dependency); `reqwest` for a Rust HTTP
provider (its default rustls backend pulls in `aws-lc-rs`, which needs a C
toolchain/NASM on Windows, and we have no first-party endpoint to point it
at yet — the provider trait keeps that door open).

### Cloudflare package investigation

- License: MIT (package `LICENSE`).
- Endpoints: the package's documented defaults
  (`https://speed.cloudflare.com/__down`, `__up`). We use the package as
  documented and never call those endpoints from Rust or scrape the site.
- Timing: uses the `PerformanceResourceTiming` API; must be verified at
  runtime in WebView2 (Windows) and WebKitGTK (Linux). If resource timing
  entries are unavailable the provider reports itself unavailable.
- Data collection: the README states results are collected by Cloudflare on
  completion. The source shows this is the `logAimApiUrl` option
  (default `https://speed.cloudflare.com/__results`); `logMeasurementApiUrl`
  defaults to `null`. We set `logAimApiUrl: null` unless the user opts in,
  disclose this in the privacy notice, and pin the package version with a
  test asserting the option still exists in the bundled defaults.
- Packet loss requires a TURN server and credentials that we do not operate,
  so packet loss is reported as unavailable.
- CSP change: `connect-src` gains exactly `https://speed.cloudflare.com`.

## 4. CPU methodology

Workloads (deterministic, seeded, `std::hint::black_box` on inputs/outputs):

| Id                                 | Workload                                                                   | Unit                     |
| ---------------------------------- | -------------------------------------------------------------------------- | ------------------------ |
| `cpu.integer`                      | xorshift/rotate/multiply/bitwise mixing over a 64 KiB L1/L2-resident table | Mops/s                   |
| `cpu.float`                        | Bounded fused multiply-add recurrence (values provably in [-2, 2])         | Mflop/s (exact op count) |
| `cpu.sha256`                       | SHA-256 over a 4 MiB buffer                                                | MB/s                     |
| `cpu.lz4.compress` / `.decompress` | LZ4 block format over 8 MiB semi-compressible data                         | MB/s (+ ratio)           |

Each is run single-core (1 thread) and multi-core (`available_parallelism`
workers, overridable, capped at 256) with a `Barrier` start. Warmup 1/2/3 s
(Quick/Standard/Extended), then 5/7/10 timed iterations of fixed work sized
to ≥ 200 ms each. Scaling = multi ÷ single. Extended adds a sustained
integer+float run in 5 s windows (default 60 s, max 300 s) reporting peak,
sustained (median of final third) and drop %. Temperature and frequency are
recorded only when the OS reports them; frequency is never inferred from
timing.

## 5. GPU methodology

- Enumerate adapters (`Instance::enumerate_adapters`), id =
  `backend:vendor:device:index`; default = `PowerPreference::HighPerformance`
  adapter. Software/CPU adapters are labelled and not used by default.
- Read `adapter.limits()` / `features()`; clamp buffers to
  `min(max_storage_buffer_binding_size, max_buffer_size, 256 MiB)`.
- Compute: WGSL kernel with a fixed FMA chain (exact FLOPs per invocation)
  → GFLOP/s. Memory: `copy_buffer_to_buffer` and a read+write kernel
  → GB/s (integrated GPUs labelled "shared system memory").
- Timing: `TIMESTAMP_QUERY` (+ `TIMESTAMP_QUERY_INSIDE_ENCODERS` not
  required) when available → `gpu_timestamp`; otherwise submit and block on
  `Device::poll(wait)` → `wall_clock_synchronized`. Submission-only timing is
  never used.
- Missing features → subtest `unsupported`; device loss/init failure →
  category `failed`; the rest of the suite continues. No power-virus loops;
  Extended sustained default 60 s with a warning.

## 6. Disk methodology

- Location = one of the engine's current mounted, writable, non-removable-by-
  default volumes, selected by mount point and re-validated in Rust.
  Directory: `app_cache_dir()/benchmark` when on the system volume, else
  `<mount>/.taskmanager-benchmark/`. Canonicalized; symlinks rejected.
- File: `.taskmanager-benchmark-<uuid v4>.tmp`, `create_new`; RAII
  `TempBenchFile` deletes on drop; startup sweep deletes only names matching
  the exact pattern inside known benchmark directories.
- Size: 512 MiB / 1 GiB / 4 GiB, reduced to keep
  `max(10 % capacity, 5 GiB)` free, refused below 256 MiB.
- I/O mode: direct (Linux `O_DIRECT`; Windows
  `FILE_FLAG_NO_BUFFERING | FILE_FLAG_WRITE_THROUGH`) with 4 KiB-aligned
  buffers; falls back to buffered with `ioMode: "buffered"` and a cache
  warning when the file is smaller than RAM.
- Tests: sequential read/write 1 MiB blocks; random 4 KiB read/write at QD1
  and "8 threads × QD1" (honestly labelled — not an async queue); per-op
  latency → p50/p95/p99; writes `sync_all` inside the timed region.
- Writes require `allowWrite`; total bytes written are estimated in
  preflight, capped by the write-limit setting and recorded.

## 7. Internet methodology

Provider trait (`prepare`, `measure_latency`, `measure_download`,
`measure_upload`, `measure_loaded_latency`, `measure_packet_loss`,
`finalize`). Implementations: Cloudflare (WebView), Mock. Requires explicit
start and one-time privacy consent; data profiles Data Saver (~25 MB),
Standard (~200 MB), Maximum (~1 GB) as upper bounds. Results: download/upload
Mbps (bits, decimal), idle latency (≥ 20 samples, median/min/p95), loaded
latency during download and upload, jitter = mean absolute difference of
consecutive RTTs, packet loss only when the provider measures it. Interface
name/type/IP families from the engine; public IP not stored; VPN/tunnel
heuristic warning.

## 8. Statistical methodology

- Monotonic `Instant`; warmup excluded; ≥ 5 iterations.
- Summary: n, mean, median, min, max, standard deviation (sample, n−1),
  coefficient of variation, p50/p90/p95/p99 (linear interpolation between
  closest ranks).
- Outliers: `|x − median| > 3.5 × 1.4826 × MAD` are flagged, kept in raw data
  and listed; summaries use all samples (nothing hidden).
- CV > 5 % → "variation higher than expected" warning.
- Confidence (High/Medium/Low) is a consistency heuristic, not a
  statistical confidence interval: inputs are CV, outlier count, sample
  count, background interference and thermal change.

## 9. Scoring methodology (score version 1)

- Raw measurements are the source of truth; scores are presentation aids.
- Metric score = 1000 × measured ÷ reference (lower-is-better metrics use
  reference ÷ measured). Reference constants live in `scoring.rs`, keyed by
  metric id, and are frozen per `SCORE_VERSION`.
- Category score = geometric mean of its metric scores (a category with no
  scored metric has no score).
- System score = geometric mean of the CPU, GPU and Disk category scores
  present; fewer than three → labelled "Partial". Skipped categories are
  never treated as zero.
- Internet is reported as a separate "Connection" result and excluded from
  the System score: it measures the path to a remote server, not the
  computer, and varies with time of day and ISP.

## 10. Storage schema

`app_data_dir()/benchmarks/results/<uuid>.json` (atomic tmp + rename), max 500
records (oldest pruned). `BenchmarkRecord`:

```
schemaVersion, methodVersion, scoreVersion, id, startedAtMs, durationMs,
appVersion, profile, kinds, environment{before, after}, hardwareFingerprint,
categories[{kind, status, metrics[{id,label,unit,higherIsBetter,summary,samples,outliers}],
            score, confidence, warnings, details, timeline, durationMs, error}],
systemScore{value, partial}, warnings
```

Only completed runs are written (cancelled/failed runs never touch history).
No hostname, user name, public IP or file paths beyond the selected mount
point. The shape is suitable for future opt-in anonymous submission.

## 11. Security considerations

- No generic command, URL or path parameters; enums, ids and a mount point
  validated against the engine's live disk list.
- All numeric arguments bounded (workers, durations, sizes).
- Resource caps: CPU buffers ≤ 64 MiB, GPU buffers ≤ adapter limits and
  256 MiB, disk free-space reserve, network byte cap, per-profile run timeout.
- One run at a time (`BenchmarkAlreadyRunning`).
- No unwrap/expect in benchmark paths; panics in the runner thread become
  `Failed`.
- HTTPS only; certificate validation never disabled; CSP extended minimally.

## 12. Privacy considerations

Nothing runs automatically. Results are stored locally only and never sent
anywhere. The internet test shows a notice before first use (public IP is
visible to the provider, data usage, provider may collect anonymized
measurements — disabled by default here). Upload payloads are generated
bytes. Exports exclude process lists and personal files.

## 13. Platform differences

| Topic            | Windows 11                             | Debian / Ubuntu                          | macOS (later) |
| ---------------- | -------------------------------------- | ---------------------------------------- | ------------- |
| GPU backend      | DX12 (Vulkan if present)               | Vulkan                                   | Metal         |
| Direct I/O       | `FILE_FLAG_NO_BUFFERING`               | `O_DIRECT` (falls back on tmpfs/some fs) | `F_NOCACHE`   |
| Temperatures     | Usually unavailable to unelevated apps | hwmon via sysinfo                        | SMC           |
| Power state      | `GetSystemPowerStatus`                 | `/sys/class/power_supply`                | IOKit         |
| Network provider | WebView2                               | WebKitGTK                                | WKWebView     |

## 14. Testing strategy

- Rust unit tests: statistics, scoring golden values, confidence, unit
  conversions, IOPS/latency, jitter, disk sizing, filename pattern & sweep,
  path validation, state machine, cancellation, single-run lock,
  serialization, version compatibility, GPU capability selection, network
  sample validation.
- Mock category benchmarks + a "simulated" mode (debug builds or
  `TM_BENCH_SIMULATED=1`) exercising orchestration, progress, cancellation
  and persistence without load.
- Hardware tests are `#[ignore]`d: `cargo test -p tm-core -- --ignored bench_`.
- Vitest: store reducers/throttling, formatting, page rendering with a mocked
  `@/services/ipc`, preflight choices, progress accessibility, comparison
  warnings. No real workloads in frontend tests or CI.

## 15. Implementation phases

| Phase | Scope                                                                                                                                   | Status |
| ----- | --------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| 0     | Baseline gates, commit WIP, this plan                                                                                                   | done   |
| 1     | Models, errors, cancellation, progress throttle, statistics, coordinator, Benchmark Mode, history store, mocks, IPC, UI shell, settings | done   |
| 2     | CPU workloads, single/multi/sustained, temps/frequency                                                                                  | done   |
| 3     | GPU (wgpu) detection, compute, memory, timing, fallback                                                                                 | done   |
| 4     | Disk: validation, RAII files, direct I/O, seq/random/latency, limits                                                                    |        |
| 5     | Internet: provider trait, Cloudflare provider, privacy, loaded latency, jitter                                                          |        |
| 6     | Full run orchestration, scoring, dashboard, history, comparison, export                                                                 |        |
| 7     | Profiling, security review, failure-recovery testing, docs, production build                                                            |        |

Each phase ends with: `cargo fmt --all --check`, `cargo clippy --workspace
--all-targets -- -D warnings`, `cargo test --workspace`, `npm run gen:types`,
`npm run typecheck`, `npm run lint`, `npm test`, `npm run build`, and a local
commit.
