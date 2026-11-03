# Benchmark validation report

What was actually run to check the CPU benchmark, and what was not. Date:
2026-11-03. Only the CPU benchmark exists, so GPU, disk and internet sections
are absent rather than empty.

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
- **GPU, disk, internet, memory, full benchmark:** not built.
- **Reference anchor:** measured on the same machine that scores about 1000 by
  construction. Scores from other hardware are meaningful relative to it, but
  no second machine was available to test that.
