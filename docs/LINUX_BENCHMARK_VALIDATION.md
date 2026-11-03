# Benchmark validation report

What was actually run to check the benchmarks, and what was not. Date:
2026-11-03. The CPU and GPU benchmarks exist; disk, internet and the full
benchmark do not, so they have no section rather than an empty one. The GPU
validation follows the CPU one.

## Test machine

Debian forky/sid, kernel 7.2.12, Intel Core 7 150U (10 cores, 12 logical CPUs,
hybrid: 2 performance + 8 efficiency cores), 32 GiB RAM, KDE Plasma on Wayland,
CPU governor `powersave`, release build. It was an ordinary working desktop:
a browser, an editor and this app were open, so numbers include real
background noise. Power source reads "unknown" (no battery or mains entry).

## What was checked

| Check                                                    | Result                                                                                                                                     |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| Workload output deterministic and chunk-independent      | Unit-tested against an unchunked reference implementation                                                                                  |
| SHA-256 correct                                          | Matches the published vector (one round over 32 zero bytes) in a unit test                                                                 |
| SHA-256 speed vs an independent tool                     | `openssl speed` on one thread: 2.07 GiB/s. This benchmark, single-core: 1.95-2.2 GiB/s                                                     |
| Compression round trip                                   | Byte-for-byte equal; ratio checked (about 3-4:1)                                                                                           |
| Floating point stays finite and inside (0,1)             | 32 seeds x 50,000 steps, plus every real run                                                                                               |
| Single-core uses exactly one worker                      | Asserted in tests (`workers == 1`)                                                                                                         |
| Multi-core uses one worker per logical CPU               | Asserted in tests; 12 on the test machine                                                                                                  |
| Every chunk runs exactly once per round whoever takes it | Unit test with deliberately uneven chunk speeds                                                                                            |
| Single vs multi scaling plausible                        | `sysbench cpu`: 3,752 to 24,396 events/s (6.5x). This benchmark: integer 4.2-4.7x, floating point 5.5-6.2x, hashing 6.4-6.8x               |
| Panicking worker does not hang the run                   | Unit test: run fails with a clear message                                                                                                  |
| Output that changes between iterations is rejected       | Unit test                                                                                                                                  |
| Cancellation, mid-run                                    | Real coordinator, Standard profile, cancelled at 8.0 s: stopped at 8.1 s, state Cancelled, history file not created                        |
| Only one benchmark at a time                             | Unit test: second start returns "already running"; lock is free after cancel                                                               |
| State only moves forward                                 | Unit test over the full state sequence. This caught a real bug: the runner re-announced `Preparing` after `Preflight`                      |
| Progress rate limit                                      | Unit test: 5,000 updates in a burst produce one emission per 100 ms; a state change always goes out. Real runs: 1.8-4.6 updates per second |
| Corrupt or unwritable history                            | Unit tests: corrupt file moved aside; failed write leaves memory unchanged; bounded to 200                                                 |
| Development builds                                       | Unit test: flagged, unscored, not saved                                                                                                    |
| Windows target compiles and lints                        | `cargo clippy --target x86_64-pc-windows-msvc` clean. **Not run on Windows.**                                                              |

Test counts at the end of this phase: 183 Rust tests (65 for benchmarks) and
134 frontend tests, all passing; clippy and formatting clean.

## Run times (release build, real coordinator)

| Profile  | Wall time                                 |
| -------- | ----------------------------------------- |
| Quick    | 10.4-10.6 s                               |
| Standard | 25.7-28.0 s                               |
| Extended | 68.6 s (includes the 30 s sustained test) |

## Repeatability

Run-to-run coefficient of variation of the per-run medians. Standard, three
runs each:

| Workload       | Single-core, unpinned | Single-core, pinned | Multi-core, unpinned | Multi-core, pinned |
| -------------- | --------------------- | ------------------- | -------------------- | ------------------ |
| Integer        | 1.0%                  | 1.1%                | 10.6%                | 7.0%               |
| Floating point | 5.3%                  | 11.5%               | 12.6%                | 4.5%               |
| Hashing        | 1.2%                  | 5.8%                | 4.2%                 | 3.7%               |
| Compression    | 4.8%                  | 1.1%                | 5.2%                 | 5.0%               |
| Decompression  | 1.0%                  | 2.5%                | 3.9%                 | 7.0%               |

Quick (after raising its per-iteration work), four runs: single-core
0.9-3.3%, multi-core 1.2-7.7%.

Conclusions, stated as measured and not smoothed over:

- Single-core is steady (about 1-5%). Multi-core is noisier (up to about 12%)
  on this busy hybrid machine.
- **Pinning the single-core worker did not reduce the spread** (floating point
  got worse, 11.5% vs 5.3%), so it stays off by default. The capability exists
  and its outcome is recorded in each result.
- The Stability rating is a heuristic. A first version used the single noisiest
  workload and rated nearly every run Low; it now uses the typical spread with a
  cap for the worst, and one test covers that.

## Design changes forced by measurement

1. **Chunked work queue instead of fixed work per thread.** The first design
   gave every worker equal work. On this hybrid CPU each multi-core round
   waited for the slowest efficiency core while performance cores sat idle.
2. **Compression sized down.** At about 14 MiB/s the first sizing made a
   Standard run take 52 s, almost all of it compression. Work is now sliced
   (256 KiB per chunk) and Standard takes about 26 s.
3. **Quick profile enlarged (scale 0.3 to 0.6).** At 0.3 an iteration lasted
   70-100 ms, dominated by frequency ramp-up, and produced multi-core scores 36%
   apart between consecutive runs. At 0.6 the spread matches Standard.

## Sustained test (Extended, real hardware)

30 one-second windows: peak 66,504 Mops/s, median 58,482, last third 61,284,
decline 0%, so no note was generated. CPU frequency read 2,258 MHz in the first
window and 2,599 MHz in the last; CPU package temperature 74 and 77 degrees C,
read from the `coretemp Package` sensor. Nothing was claimed about throttling.

## Not validated

- **Windows:** compiled and linted only.
- **GNOME and X11:** the CPU benchmark does not depend on the display server,
  but the page was not opened under them. It was checked in a headless browser
  with simulated progress, and the Rust flow was run from a command-line tool,
  not from the installed app window.
- **Battery, virtual machine and container paths:** unit-tested with
  fixtures, never run on such a system.
- **Debian 12 and Ubuntu:** not run.
- **aarch64:** nothing x86-specific was written, but it was never built.
- **Temperature and frequency during Quick and Standard:** shown live, but only
  the Extended run's windows were inspected.
- **Disk, internet, memory, full benchmark:** not built.
- **Reference anchor:** measured on the same machine that scores about 1000 by
  construction. Scores from other hardware are meaningful relative to it, but
  no second machine was available to test that.

---

# GPU benchmark (`gpu-compute-v1`)

## Test hardware

The same machine, with its **integrated GPU: Intel RPL-U graphics, Mesa 26.1.6,
Vulkan 1.4**, GPU timestamps supported. A second Vulkan adapter, `llvmpipe` (a
software renderer on the CPU), was present and was used to test selection. The
machine was a working desktop: the compositor and a browser were using the GPU,
and another session was often compiling, which shares the processor's power
budget with the integrated GPU (see "Sensitivity to load").

## What was checked

| Check                         | Result                                                                                                                                                                                                           |
| ----------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Adapters listed correctly     | Two Vulkan adapters, same names and types as `vulkaninfo --summary` (used only as a development cross-check): Intel RPL-U (integrated), llvmpipe (software). Enumeration 23-27 ms; opening the device 0.07-0.1 s |
| Default selection             | Intel integrated GPU recommended; llvmpipe never the default. Unit-tested for discrete, integrated, virtual and software ordering, and ties                                                                      |
| Shaders valid WGSL            | Parsed and validated with `naga` in CI, with no GPU                                                                                                                                                              |
| Integer output exact          | Bit for bit against the CPU reference on sampled threads, on real hardware, every run                                                                                                                            |
| Float output correct          | Within 1e-3 of the mathematical solution on every run. The CPU's own fused multiply-add stays under 1e-4 at 1 to 5,000 iterations (unit test), so the tolerance has margin                                       |
| Memory output correct         | Contents at both ends of the buffer verified, including across back-to-back runs with odd and even pass counts (hardware test)                                                                                   |
| **Validation can fail**       | Hardware test corrupts the device's output on purpose; the check returns "wrong output". A validator that cannot fail would prove nothing                                                                        |
| Wrong output discards the run | Fake-device test: no score, plain message                                                                                                                                                                        |
| Timeout is reported, not hung | Hardware test: a far too large job with a 1 ms timeout returns a timeout, and the device is still usable afterwards                                                                                              |
| Device lost, out of memory    | Handled and turned into plain messages (fake-device tests). **A real device-lost event was not provoked**                                                                                                        |
| Cancellation                  | Real coordinator, Standard profile cancelled 2.0 s in: stopped at 2.1 s, state Cancelled, history file not created                                                                                               |
| One lock across CPU and GPU   | Unit test: a running CPU benchmark rejects a GPU start, and the reverse works after cancel                                                                                                                       |
| Progress rate                 | 8-9 updates per second, under the limit of 10                                                                                                                                                                    |
| No adapter at all             | Clear message ("A working Vulkan driver is needed"), the app keeps working, a CPU run can follow                                                                                                                 |
| Software renderer             | Runs and verifies on `llvmpipe` (about 1 Gops/s against about 300 on the integrated GPU); flagged as not a GPU result and never scored                                                                           |
| Old history still loads       | Unit test: results saved before GPU support load with the new fields empty                                                                                                                                       |
| Windows target                | `cargo clippy --target x86_64-pc-windows-msvc` is clean with `wgpu`'s DirectX 12 and Vulkan backends. **Not run on Windows**                                                                                     |

Test counts when this was written: 297 Rust tests in `tm-core` (including work
from a parallel session), 178 frontend tests, plus 4 opt-in hardware tests.

## Run times (release build, real coordinator, real GPU)

| Profile  | Wall time                               |
| -------- | --------------------------------------- |
| Quick    | 3.9 s                                   |
| Standard | 6.1 s                                   |
| Extended | 38 s (includes the 30 s sustained test) |

## Repeatability

Run-to-run coefficient of variation of the per-run medians. This integrated GPU
is noticeably less repeatable than the CPU: **about 5-12%**, depending on the
workload and the day.

| Configuration                                   | Runs | Floating point | Integer | Memory |
| ----------------------------------------------- | ---- | -------------- | ------- | ------ |
| Standard, 3 warm-up dispatches                  | 4    | 12.2%          | 7.1%    | 11.4%  |
| Quick, 3 warm-up dispatches                     | 4    | 14.9%          | 13.8%   | 8.8%   |
| Standard, 12 warm-up dispatches                 | 4    | 2.6%           | 11.3%   | 3.2%   |
| Quick, 12 warm-up, 9 iterations                 | 5    | 7.5%           | 12.5%   | 6.6%   |
| Standard, 12 warm-up, 15 iterations             | 5    | 9.1%           | 7.7%    | 10.5%  |
| Quick, final code, quiet machine (load about 2) | 4    | 6.7%           | 10.1%   | 10.1%  |

Within one run the spread between iterations is 4-17%, which is why the
Stability rating is often Medium or Low on this hardware. That is a true
statement about this GPU, not a flaw to hide.

Experiments, including the ones that did not help:

- **Longer warm-up** (3 to 12 dispatches) helped in one batch of runs (floating
  point 12% to 2.6%) and not consistently in the next. It was kept: it costs
  about a second.
- **Longer dispatches (200 ms instead of 80 ms) were reverted.** Throughput fell
  15-25% (the GPU slowed under longer continuous load) and integer and
  floating-point spread did not improve.
- **More Quick iterations** (5 to 9) did not clearly reduce the spread.

## Sensitivity to load

Floating-point throughput was 340-376 GFLOPS while the machine's load average
was about 30 (a security scan and compiles running), against 494-577 GFLOPS at a
load average of about 2. The integrated GPU shares the processor's power
budget, so a busy CPU lowers the GPU score. The benchmark's preflight warns when
other applications use more than 25% CPU, but it measures CPU load only.

## Sustained test (Extended, real GPU)

Peak 516 GFLOPS, median 459, last third 407: a **15.7% decline** over 30 s, with
the note "Performance decreased by 16% during sustained load. Temperature or
power limits may have contributed." No cause was claimed, and GPU clock and
temperature were not read (not available through a vendor-neutral source).

## Bugs found by measurement

1. **Memory bandwidth read half its true value.** The byte count was divided
   by 2 with no basis. Noticed because the work per dispatch did not match
   passes x elements x 32 bytes; fixed.
2. **Calibration was fooled by shader compilation.** On `llvmpipe` the first
   dispatch included compile time, so the device looked slow and the smallest
   possible size was chosen. The first dispatch is now discarded; a unit test
   covers it.
3. **The GPU warm-up was too short** (3 dispatches, about a quarter of a second)
   for the clocks to ramp up; raised to 12.

## Not validated

- **Discrete GPUs, NVIDIA and AMD:** none available. Only an Intel integrated
  GPU and a software renderer were tested.
- **Hybrid graphics** selection on real hybrid hardware (two real GPUs).
- **DirectX 12 and Windows:** compiled and linted only.
- **Wall-clock timing path on real hardware:** the test GPU supports
  timestamps. The fallback is tested with a fake device only.
- **A real device-lost event or driver reset:** the handling is unit-tested with
  injected faults; a driver reset was not provoked.
- **GNOME, X11, Debian 12, Ubuntu, aarch64:** not run.
- **Reference anchors** come from the same machine that scores about 1000 by
  construction.
- **Cross-check against another GPU benchmark:** none was run. Adapter details
  matched `vulkaninfo`; throughput was not compared with any other tool.
