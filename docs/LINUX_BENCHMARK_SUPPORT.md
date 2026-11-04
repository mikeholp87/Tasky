# Benchmark support on Linux

Honest status of each benchmark. "Built" means implemented, tested and run on
real hardware; "Not built" means nothing exists yet.

| Benchmark                                                  | Status                                                                         | Linux backend                                    | Notes                                                                   |
| ---------------------------------------------------------- | ------------------------------------------------------------------------------ | ------------------------------------------------ | ----------------------------------------------------------------------- |
| CPU single-core                                            | Built                                                                          | Rust threads                                     | Optional pinning via `sched_setaffinity`, off by default                |
| CPU multi-core                                             | Built                                                                          | Rust threads, shared work queue                  | One worker per logical CPU                                              |
| Integer, floating point                                    | Built                                                                          | Rust                                             | Deterministic, validated                                                |
| Hashing (SHA-256)                                          | Built                                                                          | `sha2` crate                                     | Checked against the published vector                                    |
| Compression / decompression                                | Built                                                                          | `flate2` (pure-Rust `miniz_oxide`)               | In memory only                                                          |
| Sustained CPU test                                         | Built                                                                          | Rust                                             | Extended profile, 30 s                                                  |
| CPU frequency                                              | Built, hardware dependent                                                      | Monitoring engine (sysinfo)                      | `Unavailable`, never 0                                                  |
| CPU temperature                                            | Built, hardware dependent                                                      | hwmon via sysinfo                                | Only labelled package sensors (`coretemp Package`, `k10temp Tctl`, ...) |
| Power source, governor, VM/container                       | Built                                                                          | sysfs, `/proc/cpuinfo`                           | Recorded and warned about, never changed                                |
| Progress, cancel, history, export of raw data              | Built                                                                          | Coordinator, JSON file                           | One benchmark at a time                                                 |
| Memory benchmark                                           | Not built                                                                      |                                                  |                                                                         |
| GPU adapter detection and selection                        | Built                                                                          | `wgpu` (Vulkan), no `vulkaninfo`                 | Name, type, backend, driver, limits; chosen by device type              |
| GPU integer and floating-point compute                     | Built                                                                          | WGSL via `wgpu`                                  | Outputs verified against CPU references                                 |
| GPU memory bandwidth                                       | Built, hardware dependent                                                      | WGSL via `wgpu`                                  | Integrated GPUs measure shared system memory                            |
| GPU sustained test                                         | Built                                                                          | `wgpu`                                           | Extended profile, 30 s                                                  |
| GPU timing                                                 | Built                                                                          | GPU timestamps, else wall clock                  | Recorded per result                                                     |
| GPU temperature, clock, utilization                        | Not available                                                                  |                                                  | Not measured, and shown as such                                         |
| Multiple GPUs                                              | Built (selection); tested with two adapters                                    |                                                  | Hybrid graphics not tested on real hybrid hardware                      |
| Disk sequential and random I/O                             | Built (Linux only)                                                             | `pread`/`pwrite` on a temporary file, `O_DIRECT` | Falls back to buffered I/O, labelled and not scored                     |
| Disk latency percentiles, queue depth                      | Built (Linux only)                                                             | Threads with synchronous I/O                     | Exact percentiles; "QD" shown only with direct I/O                      |
| Disk target list, space reserve, cleanup                   | Built (Linux only)                                                             | Mounted file systems, `statvfs`                  | Never tmpfs, network or read-only; file deleted on every exit path      |
| Disk device facts (model, transport, rotational)           | Built, hardware dependent                                                      | sysfs                                            | Omitted when unreadable                                                 |
| `io_uring`                                                 | Not used                                                                       |                                                  | Threads reach the queue depths measured; see the implementation notes   |
| Internet latency, jitter, download, upload, loaded latency | Built                                                                          | `ureq` + rustls, Cloudflare speed endpoints      | Native confirmation, data caps, cooldown, session allowance             |
| Internet packet loss                                       | Not available                                                                  |                                                  | HTTP cannot measure it; never reported                                  |
| Internet interface and VPN detection                       | Built                                                                          | `/proc/net/route`, interface facts               | A tunnel being up is noted on the result                                |
| Full system benchmark, cooldown                            | Not built                                                                      |                                                  |                                                                         |
| Export                                                     | Raw results are stored as JSON in the app data directory; no export button yet |                                                  |                                                                         |

## Platform notes

- Needs no root. The history file is the only file written outside a disk run.
  A disk run writes one temporary file inside the location you choose and
  deletes it; the CPU, GPU and internet benchmarks create no benchmark files.
- The internet benchmark is the only part of the application that uses the
  network. It runs only after a native confirmation and talks only to
  `speed.cloudflare.com`.
- Display server (Wayland or X11) does not matter to the CPU or GPU benchmark;
  the GPU benchmark uses compute only and opens no window.
- The GPU benchmark needs a working Vulkan driver (`libvulkan1` and a driver
  such as `mesa-vulkan-drivers`). `vulkaninfo` is not needed and never run.
- Windows runs the same shared code except the disk benchmark, which is not
  implemented there yet. The Windows build was only compiled and linted (the
  TLS library cannot be cross-compiled from Linux here, so the Windows check
  was done with the HTTPS feature off), not run.
