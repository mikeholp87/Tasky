# Benchmark support on Linux

Honest status of each benchmark. "Built" means implemented, tested and run on
real hardware; "Not built" means nothing exists yet.

| Benchmark                                                        | Status                                                                         | Linux backend                                   | Notes                                                                   |
| ---------------------------------------------------------------- | ------------------------------------------------------------------------------ | ----------------------------------------------- | ----------------------------------------------------------------------- |
| CPU single-core                                                  | Built                                                                          | Rust threads                                    | Optional pinning via `sched_setaffinity`, off by default                |
| CPU multi-core                                                   | Built                                                                          | Rust threads, shared work queue                 | One worker per logical CPU                                              |
| Integer, floating point                                          | Built                                                                          | Rust                                            | Deterministic, validated                                                |
| Hashing (SHA-256)                                                | Built                                                                          | `sha2` crate                                    | Checked against the published vector                                    |
| Compression / decompression                                      | Built                                                                          | `flate2` (pure-Rust `miniz_oxide`)              | In memory only                                                          |
| Sustained CPU test                                               | Built                                                                          | Rust                                            | Extended profile, 30 s                                                  |
| CPU frequency                                                    | Built, hardware dependent                                                      | Monitoring engine (sysinfo)                     | `Unavailable`, never 0                                                  |
| CPU temperature                                                  | Built, hardware dependent                                                      | hwmon via sysinfo                               | Only labelled package sensors (`coretemp Package`, `k10temp Tctl`, ...) |
| Power source, governor, VM/container                             | Built                                                                          | sysfs, `/proc/cpuinfo`                          | Recorded and warned about, never changed                                |
| Progress, cancel, history, export of raw data                    | Built                                                                          | Coordinator, JSON file                          | One benchmark at a time                                                 |
| Memory benchmark                                                 | Not built                                                                      |                                                 |                                                                         |
| GPU detection, selection, compute, memory, sustained             | Not built                                                                      | planned: wgpu/Vulkan                            | `wgpu` is a large dependency; decision deferred                         |
| Disk sequential and random I/O, latency percentiles, queue depth | Not built                                                                      | planned: file I/O, `O_DIRECT`, maybe `io_uring` |                                                                         |
| Internet download, upload, latency, jitter                       | Not built                                                                      | planned                                         | Needs a provider and consent to use bandwidth                           |
| Full system benchmark, cooldown                                  | Not built                                                                      |                                                 |                                                                         |
| Export                                                           | Raw results are stored as JSON in the app data directory; no export button yet |                                                 |                                                                         |

## Platform notes

- Needs no root. Never writes outside the app data directory (history file
  only). The CPU benchmark creates no benchmark files.
- Display server (Wayland or X11) does not matter to the CPU benchmark.
- Windows runs the same shared code; the Windows build was only compiled and
  linted, not run.
