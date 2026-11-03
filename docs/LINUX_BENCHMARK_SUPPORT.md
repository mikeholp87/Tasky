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
| GPU adapter detection and selection                              | Built                                                                          | `wgpu` (Vulkan), no `vulkaninfo`                | Name, type, backend, driver, limits; chosen by device type              |
| GPU integer and floating-point compute                           | Built                                                                          | WGSL via `wgpu`                                 | Outputs verified against CPU references                                 |
| GPU memory bandwidth                                             | Built, hardware dependent                                                      | WGSL via `wgpu`                                 | Integrated GPUs measure shared system memory                            |
| GPU sustained test                                               | Built                                                                          | `wgpu`                                          | Extended profile, 30 s                                                  |
| GPU timing                                                       | Built                                                                          | GPU timestamps, else wall clock                 | Recorded per result                                                     |
| GPU temperature, clock, utilization                              | Not available                                                                  |                                                 | Not measured, and shown as such                                         |
| Multiple GPUs                                                    | Built (selection); tested with two adapters                                    |                                                 | Hybrid graphics not tested on real hybrid hardware                      |
| Disk sequential and random I/O, latency percentiles, queue depth | Not built                                                                      | planned: file I/O, `O_DIRECT`, maybe `io_uring` |                                                                         |
| Internet download, upload, latency, jitter                       | Not built                                                                      | planned                                         | Needs a provider and consent to use bandwidth                           |
| Full system benchmark, cooldown                                  | Not built                                                                      |                                                 |                                                                         |
| Export                                                           | Raw results are stored as JSON in the app data directory; no export button yet |                                                 |                                                                         |

## Platform notes

- Needs no root. Never writes outside the app data directory (history file
  only). The CPU benchmark creates no benchmark files.
- Display server (Wayland or X11) does not matter to the CPU or GPU benchmark;
  the GPU benchmark uses compute only and opens no window.
- The GPU benchmark needs a working Vulkan driver (`libvulkan1` and a driver
  such as `mesa-vulkan-drivers`). `vulkaninfo` is not needed and never run.
- Windows runs the same shared code; the Windows build was only compiled and
  linted, not run.
