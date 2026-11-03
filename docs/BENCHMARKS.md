# Benchmarks

What the Benchmarks page measures, how, and what the numbers do and do not
mean. Implementation plan: [`LINUX_BENCHMARK_IMPLEMENTATION.md`](LINUX_BENCHMARK_IMPLEMENTATION.md).
Validation data: [`LINUX_BENCHMARK_VALIDATION.md`](LINUX_BENCHMARK_VALIDATION.md).

**Status:** the CPU and GPU benchmarks exist. Storage, internet and the full
system benchmark are not built yet. The suite is new and shared: Windows and
Linux run the same code, with the same methods.

## CPU (`cpu-method-v1`, scoring `score-v1`)

Five deterministic workloads, each in single-core and multi-core mode:

| Workload       | What is measured                                                     | Unit                 |
| -------------- | -------------------------------------------------------------------- | -------------------- |
| Integer        | Multiply, add, rotate, xor on eight independent 64-bit lanes         | Mops/s               |
| Floating point | The logistic map `x = 3.9·x·(1−x)` on eight lanes (double precision) | Mflops/s             |
| Hashing        | SHA-256, chained over a 1 MiB buffer                                 | MiB/s                |
| Compression    | DEFLATE level 6 over 256 KiB slices of a 4 MiB text                  | MiB/s                |
| Decompression  | DEFLATE inflate of the same slices                                   | MiB/s (uncompressed) |

How a run works:

- **Fixed work, not fixed time.** Every iteration executes an exact amount of
  deterministic work from fixed seeds. Throughput is operations divided by
  elapsed monotonic time.
- **Work is cut into chunks pulled from a shared counter.** A faster core takes
  more chunks. This matters on hybrid CPUs (performance plus efficiency cores):
  giving each thread a fixed share made every multi-core round wait for the
  slowest core and understated throughput.
- **The optimiser cannot delete the work.** Each chunk returns a checksum that
  goes through `black_box` and must be identical on every iteration. The
  floating-point state must stay finite and inside (0, 1). Hashing is checked
  against the published SHA-256 vector, compression round-trips byte for byte.
- **Warm-up, then iterations.** One warm-up iteration is discarded; the reported
  value is the **median** of 3 (Quick), 5 (Standard) or 7 (Extended) iterations.
- **Nothing is removed.** Every raw iteration is stored. Outliers (beyond
  1.5×IQR) are flagged and kept.
- **Extended adds a 30 s sustained test** on all cores, read in 1 s windows.
  It reports peak, median, sustained (last third) and the decline. It says
  performance _decreased_ and that temperature or power limits _may_ have
  contributed; it never claims throttling. CPU frequency and temperature are
  shown per window only when the system exposes them and the sensor can be
  identified as the CPU.

### Scores

`score = 1000 × throughput ÷ reference` per workload. Single-core and
multi-core scores are the geometric mean of their five workloads. There is no
combined CPU score, because weighting single against multi would be arbitrary.

The reference is the development machine (Intel Core 7 150U) measured in
release mode on 2026-11-03. That machine therefore scores about 1000 by
construction. **The score compares runs of this application with each other.
It is not comparable with other benchmark products.**

Development builds are not scored and not saved: unoptimised code is not
representative.

### Stability

High, Medium or Low, from the typical spread between iterations (with a cap for
the worst workload) and any warnings. It is a heuristic about repeatability,
not a statistical confidence interval.

### Warnings (never blockers)

Battery power, other applications using more than 25% CPU, running in a virtual
machine or container, and development builds. The benchmark never changes the
system: no CPU governor, process priority, power profile or cache changes, and
it needs no root.

## GPU (`gpu-compute-v1`, scoring `gpu-score-v1`)

Three workloads, run as WGSL compute shaders through `wgpu` (Vulkan on Linux;
DirectX 12 or Vulkan on Windows). Every platform runs the same shaders.

| Workload               | What is measured                                                                          | Unit           |
| ---------------------- | ----------------------------------------------------------------------------------------- | -------------- |
| Integer compute        | A dependent 32-bit multiply, add, shift, xor chain per invocation (5 operations per step) | Gops/s         |
| Floating-point compute | Four fused multiply-add chains per invocation (8 operations per step)                     | GFLOPS         |
| Memory bandwidth       | Read one buffer, write another, 32 bytes per element, repeated                            | GB/s (decimal) |

How a run works:

- **Adapters.** All adapters are listed with the driver's own name, device type,
  backend and driver text. The default is chosen by device type, never by
  vendor name: discrete, then integrated, then virtual, and a software renderer
  only as a last resort. You can pick another adapter, and the result records
  which one was tested.
- **Known work.** Operations and bytes per dispatch are counted from the shader,
  not estimated from elapsed time.
- **Calibration.** Each workload is sized so one dispatch takes about 80 ms: long
  enough to hide submission overhead, short enough to stay far from the driver
  watchdog that resets a GPU after a second or two. Probes grow by at most 8x.
  The first dispatch is discarded because it includes one-off shader compilation.
- **Timing.** GPU timestamps when the device supports them; otherwise the wall
  clock until completion is confirmed, which includes driver overhead. The
  method is recorded in the result and shown on the page. A dispatch is only
  timed after it has finished: the benchmark never measures `submit` and calls
  that execution time.
- **Warm-up, then iterations.** 12 warm-up dispatches (GPU clocks ramp up slowly),
  then 9 (Quick), 15 (Standard) or 25 (Extended) measured ones; the reported
  value is the median, every raw sample is stored, outliers are flagged and kept.
- **Every result is checked.** After each workload the output is compared with a
  CPU reference: integer results bit for bit on up to 2,048 sampled threads;
  floating point against its mathematical solution within 1e-3 (single-precision
  rounding stays under 1e-4); memory contents at both ends of the buffer. A
  mismatch discards the run. A workload that was not verified is never scored.
- **Extended** adds 30 s of repeated floating-point work in 1 s windows, reported
  like the CPU sustained test.
- **Safety.** Per-dispatch timeout (10 s), cancellation between dispatches, and a
  device-lost, timeout or out-of-memory error ends the run with a plain message
  and leaves the application working. Buffer sizes follow the adapter's real
  limits.

**Scores.** The GPU score is the geometric mean of the three workloads' scores,
each `1000 x throughput / reference`. The references are the development
machine's integrated GPU (Intel RPL-U graphics, Mesa 26.1.6, Vulkan): 270 Gops/s,
490 GFLOPS and 31 GB/s. As with the CPU, this anchor is arbitrary and compares
runs of this application, not other benchmark products.

**What is not scored or not claimed:**

- A **software renderer** (for example `llvmpipe`) can be run, but it describes the
  CPU. It is never scored and the page says so.
- On an **integrated GPU the memory test measures shared system memory**, not
  dedicated video memory, and is not comparable with a discrete GPU's bandwidth.
- **GPU utilization, clock speed and temperature are not measured** during the
  test. No stable, vendor-neutral source exists, and none is invented.
- GPU results compare only with the same adapter and the same timing method.

### Comparing results

Two results compare only when `methodId` and `scoreVersion` match and the
profile is the same. Otherwise the page says: _These tests used different
benchmark methodologies and should not be compared directly._ A single run is
not proof of a real difference.

| Benchmark            | Comparable across Windows and Linux?                                                                                                                        |
| -------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| CPU `cpu-method-v1`  | Yes: identical deterministic code and constants                                                                                                             |
| GPU `gpu-compute-v1` | Same shaders and constants everywhere; backend (Vulkan or DirectX 12), driver and timing method are recorded and shown. Comparable only on the same adapter |
| Storage, internet    | Not built                                                                                                                                                   |

### Limits to know about

- Run-to-run spread on a busy desktop is real: 1-5% single-core and 1-12%
  multi-core in validation (see the validation report). Close other applications
  for the steadiest numbers.
- Frequency scaling, shared power budgets and hybrid-core scheduling affect any
  CPU benchmark. Thread pinning was tested and did not reduce the spread on the
  test machine, so it is off by default.
- Thread pinning and sensors are Linux-first. On Windows the benchmark runs but
  does not pin threads, and power source, governor and virtualization read as
  unknown. The Windows build has not been run.
