# Benchmark suite: implementation plan

Written 2026-11-03. Status: **phases 1 (framework + CPU) and 4 (GPU) complete**;
phases 2 (disk), 3 (internet) and 5 (full benchmark) are **not started**.

## 1. Starting point

The brief assumed a Windows benchmark suite to port. **There is none.** Both
branches, the full git history, GitHub's branch list and every other folder
under `~/Projects` were searched; the repository has no benchmark code on any
platform. So there is no existing methodology, scoring, history or UI to match.

Decision (confirmed with the project owner): build the suite from scratch as a
**shared, cross-platform framework** whose algorithms, scoring, history and UI
are common to Windows and Linux. Only thin platform hooks differ. The methods
defined here become "method v1"; later Windows work should reuse them rather
than invent a second set.

Consequence for the brief's "keep the same methodology as Windows": the rule is
inverted. These definitions are the reference, and each carries a version so a
future change cannot be mistaken for the same measurement.

## 2. Architecture

```text
tm-core::benchmarks
    model.rs        kinds, profiles, state machine, results, progress (serde + ts-rs)
    stats.rs        median, stddev, CV, interpolated percentiles, outlier flags
    cancel.rs       one CancelToken shared by every benchmark
    cpu/workloads.rs   deterministic algorithms (shared, no OS code)
    cpu/runner.rs      workers, barrier, warmup, iterations, sustained windows
    scoring.rs      raw measurements -> scores (derived, never stored alone)
    environment.rs  preflight: power, governor, VM/container, background load
    history.rs      JSON file, atomic writes, bounded
    coordinator.rs  exclusivity, state machine, progress coalescing, RAII guard
tm-core::platform   (existing) thread affinity, CPU frequency: the only OS code
src-tauri           commands + progress events
src                 Benchmarks page, store, history
```

Platform code lives only in `platform/`: `pin_current_thread` (Linux
`sched_setaffinity`, other platforms report "unavailable") and
`cpu_frequency_mhz` (Linux sysfs). Everything else is plain Rust and runs
unchanged on Windows.

## 3. Method v1 (CPU), `cpu-method-v1`

Fixed work, not fixed time: every iteration executes an exact, deterministic
number of operations, so throughput = operations / elapsed. Inputs come from a
fixed-seed generator (SplitMix64). Results are folded into a checksum that is
verified, and passed through `std::hint::black_box`, so the compiler cannot
remove the work.

| Workload       | Definition                                                                                                      | Unit                                 |
| -------------- | --------------------------------------------------------------------------------------------------------------- | ------------------------------------ |
| Integer        | 8 independent u64 lanes; per step per lane: multiply, add, rotate, xor (4 ops)                                  | Mops/s                               |
| Floating point | 8 independent f64 lanes of the logistic map `x = 3.9·x·(1−x)` (4 flops); stays inside (0,1), checked finite     | Mflops/s                             |
| Hash           | SHA-256, chained: `d = SHA256(d ‖ buf)` over a fixed 1 MiB buffer                                               | MiB/s                                |
| Compression    | DEFLATE level 6 of a fixed 4 MiB semi-compressible buffer, then inflate and verify byte-equality; all in memory | MiB/s (compress, decompress) + ratio |

Single-core: exactly one worker, pinned to one logical CPU where the OS allows
it (failure is recorded, never fatal). Multi-core: one worker per logical CPU,
started together by a barrier; workers persist across iterations so thread
creation is never timed. An iteration's time runs from barrier release until
the **slowest** worker finishes.

Profiles change repetition counts and per-iteration size only; the operation
definitions never change. Throughput normalises size away.

| Profile                      | Warm-up | Iterations | Sustained phase              |
| ---------------------------- | ------- | ---------- | ---------------------------- |
| Quick (about 10 s)           | 1       | 3          | none                         |
| Standard (about 26 s)        | 1       | 5          | none                         |
| Extended (about 70 s)        | 1       | 7          | 30 s multi-core, 1 s windows |
| Test (internal, not exposed) | none    | 2          | none; tiny workloads for CI  |

Statistics: every raw iteration is stored. The reported value is the median.
Outliers (1.5×IQR) are **flagged, never removed**; the result records
`outlierMethod` and the flagged indices. Stability is High/Medium/Low from the
_typical_ (median) coefficient of variation across the ten workloads, capped by
the worst one: Low if typical > 10% or worst > 25%, Medium if typical > 4% or
worst > 10%, else High; any warning or more than three outliers costs one
level. It is a heuristic, not a confidence interval. (A first version used the
worst workload alone and rated nearly every run on a busy desktop "Low", which
told the user nothing.)

Sustained phase reports peak, median, sustained (median of the last third),
and decline. It never says "thermal throttling"; it says performance decreased
and that temperature or power limits _may_ have contributed, and shows
frequency and CPU temperature per window only when the OS exposes them.

## 4. Scoring `score-v1`

`score = round(1000 × throughput / reference)` per workload, where the
reference is the single-core median of this project's development machine
(Intel Core 7 150U, Debian forky, release build). That anchor is arbitrary and
documented as such: it makes scores comparable between runs of this app, not
against other benchmark products. Single-core and multi-core scores are the
geometric mean of their four workloads. No combined "CPU score" is invented.
Scores are computed from stored raw data and are never the only thing saved.

Debug builds are marked `developmentBuild` and are **not scored**.

## 5. Comparability

| Benchmark        | Class                                                        | Why                                                                            |
| ---------------- | ------------------------------------------------------------ | ------------------------------------------------------------------------------ |
| CPU              | Cross-platform comparable                                    | Same deterministic code, same method version                                   |
| GPU (later)      | Comparable only with an identical WGSL workload through wgpu | Driver/backend differences are reported                                        |
| Disk (later)     | Platform-specific                                            | Page cache, I/O APIs and filesystems differ; I/O mode is part of the method id |
| Internet (later) | Comparable only for the same provider and method             | Network path dominates                                                         |

The UI refuses to compare results whose `methodId` differs.

## 6. Preflight and warnings

Collected once per run and stored: OS, distribution, kernel, architecture, CPU,
core counts, RAM, power source (AC/battery), CPU frequency governor, virtual
machine or container, build type, and background CPU load. Warnings, never
blockers: battery power, background load above a threshold, VM/container,
development build. Nothing is modified (no governor, priority, affinity left
changed, caches or power profile).

## 7. Safety

- Exclusive run: starting while another benchmark runs returns
  `BenchmarkAlreadyRunning`.
- One `CancelToken` is checked by workers at chunk boundaries; cancellation
  never produces a result.
- Panics in a worker are contained and reported as a failed run.
- An RAII guard releases the exclusive lock and restores affinity on every exit
  path.
- No root, no shell commands, no external tools. Frontend can send only a
  `kind` and a `profile`; both are enums.
- Progress is coalesced to at most 10 events per second.

## 8. Dependencies

Added in phase 1: `sha2` (RustCrypto, MIT/Apache-2.0, pure Rust, the reference
SHA-256) and `flate2` with its default pure-Rust `miniz_oxide` backend
(MIT/Apache-2.0). Nothing else; affinity uses the existing `libc`.

Added in the GPU phase: `wgpu` 30 (MIT/Apache-2.0, Rust 1.87+; default features
off, only `std`, `parking_lot`, `wgsl`, plus `vulkan` on Linux and `dx12` and
`vulkan` on Windows) and `pollster` (a tiny `block_on` for wgpu's async
functions). `naga` is a test-only dependency (the same version wgpu uses) so
the shaders are validated in CI without a GPU.

Deferred decisions, to be taken with the owner when each phase starts: an
`io_uring` crate (only if disk queue-depth accuracy needs it), and the internet
provider (which endpoint, and consent to use bandwidth).

## 9. Phases

1. **Framework + CPU** (this phase): models, statistics, cancellation,
   coordinator, history, preflight, CPU method v1, UI page. Tested with fixture
   and live tests; repeatability measured on this machine.
2. **Disk**: benchmark file lifecycle and space safety, sequential and random
   I/O, latency histogram and percentiles, buffered vs direct (`O_DIRECT`) with
   truthful labelling, queue depth, validation against `fio`.
3. **Internet**: provider abstraction, idle/loaded latency, jitter, data usage.
4. **GPU**: wgpu adapter enumeration and selection, one WGSL workload, timing
   method recorded, output validation.
5. **Full benchmark**: sequencing, cooldown, partial-result rules, comparison.

The final reports (`LINUX_BENCHMARK_SUPPORT.md`, `LINUX_BENCHMARK_VALIDATION.md`,
`LINUX_BENCHMARK_PORT_REPORT.md`) are written as each phase is validated, and
state only what was actually run.
