# Performance

Targets from the product brief: idle CPU under ~1%, typical monitoring CPU
under 2%, memory preferably under 150 MB. These are goals to measure against,
not hard limits.

## Measured results

Release build (`npx tauri build --no-bundle`, LTO), 2026-11-02.
Machine: Intel Core 7 150U (10 cores / 12 threads), 32 GB RAM, Windows 11 Pro
26300, ~240 processes. Refresh interval 1 s. CPU is a percentage of **all**
logical processors and includes every WebView2 child process.

| Scenario                              | CPU        | Private memory |
| ------------------------------------- | ---------- | -------------- |
| Window visible (60 s)                 | **1.29%**  | 217 MB         |
| Window minimized (60 s)               | **0.11%**  | 204 MB         |
| Rust backend alone (lifetime average) | 0.14–0.17% | 18 MB          |

Memory by process (window visible):

| Process                                 | Private |
| --------------------------------------- | ------- |
| `taskmanager.exe` (Rust engine + Tauri) | 18 MB   |
| WebView2 browser                        | 41 MB   |
| WebView2 GPU process                    | 89 MB   |
| WebView2 renderer (React UI, JS heap)   | 62 MB   |
| WebView2 utility / crashpad             | 21 MB   |

Frontend responsiveness (release, Long Tasks API, 10 s per page): **zero**
main-thread tasks over 50 ms on Processes, Performance (1 h range), CPU
(per-core charts) and Overview.

Backend sampling cost per tick: 13–21 ms of wall time for ~240 processes,
measured with `cargo test --release measure_ -- --ignored --nocapture`.

Tick payload: 70–120 KB of JSON per tick with ~170–240 processes. Command
lines account for roughly a third of that.

Frontend cost per tick in a **development** build (React dev mode,
StrictMode), measured from tick arrival to the next animation frame on the
Processes page:

| Processes                                       | Median | Max    |
| ----------------------------------------------- | ------ | ------ |
| ~240 (real)                                     | 41 ms  | 63 ms  |
| 1,000 (mock)                                    | 48 ms  | 90 ms  |
| 5,000 (mock, includes generating the mock data) | 118 ms | 135 ms |

Filtering and sorting 5,000 processes takes a few milliseconds and is covered
by unit tests with a generous bound. Virtualization keeps about 35 rows in the
DOM regardless of process count.

## Assessment

- **CPU: meets the targets.** Visible monitoring stays under 2%; minimized
  or in the tray it drops to about 0.1%, because the engine stops
  serializing ticks for a hidden window.
- **Memory: above the 150 MB preference, and the reason is known.** About
  200 MB of the total is the WebView2 runtime that any Tauri/WebView2 app
  pays (browser, GPU and utility processes). The app's own backend is 18 MB.
  `--disable-gpu` was measured: it saved only ~10 MB while visible, cost
  +0.26% CPU and increased working set, so GPU compositing stays enabled.

## What made the difference

| Change                                                    | Effect                                                            |
| --------------------------------------------------------- | ----------------------------------------------------------------- |
| One engine thread owning all OS handles                   | No duplicate polling, no lock contention                          |
| Static fields (exe, command line, user) read once per PID | Most per-process syscalls avoided on every tick                   |
| Handle-free `SystemProcessInformation` snapshot (Windows) | One kernel call for threads, priority and memory of all processes |
| Skip JSON for hidden windows                              | ~1.3% → ~0.1% CPU when minimized                                  |
| Tray tooltip throttled to 2 s, posted to the main thread  | No per-tick main-thread work                                      |
| uPlot canvas charts, `setData` in place                   | No DOM churn for live graphs                                      |
| Mutable history buffers + version counter                 | No 7,200-point array copies per tick                              |
| Virtualized process table keyed by PID                    | Rendering cost independent of process count                       |

## How to measure

- **Live:** System Information → _Monitoring diagnostics_ shows backend poll
  time, process count, payload size, frontend ingest time and tick-to-frame
  time. Development builds add a _Mock processes_ selector (100–5,000).
- **Backend sampling:**
  `cd src-tauri && cargo test --release -p tm-core measure_ -- --ignored --nocapture`.
- **Whole app:** run the release binary and sample `TotalProcessorTime` and
  `PrivateMemorySize64` of `taskmanager.exe` and its `msedgewebview2.exe`
  descendants over 60 s.

## Ideas not pursued (yet)

- **Omit command lines from ticks** and fetch them on demand. This would cut
  payloads by about a third, but search needs them; worth revisiting only if
  payload size becomes measurable CPU.
- **Differential process updates.** Not justified by the measurements above.
