# Diagnostic baseline

Captured on 2026-11-03, before application changes. Working tree was clean.

| Item             | Baseline                                                      |
| ---------------- | ------------------------------------------------------------- |
| Version          | 0.1.0 (frontend, desktop shell, core)                         |
| Git commit       | `d38ca294790f90ca4542c37b551d95971287b75e`                    |
| OS               | Windows 11 Pro, 10.0.26300, build 26300                       |
| CPU              | Intel Core 7 150U; 10 physical cores, 12 logical processors   |
| RAM              | 34,119,442,432 bytes (31.78 GiB usable)                       |
| GPU              | Intel Graphics, driver 32.0.101.7088                          |
| Rust / Cargo     | 1.99.1 / 1.99.1, x86_64-pc-windows-msvc; project minimum 1.90 |
| Node / npm       | v24.22.0 / 11.19.1; npm, committed package-lock.json          |
| Stack            | Tauri 2; React 19.3.1; TypeScript 6.0.3; Vite 8.3.5           |
| Release settings | opt-level 3, LTO, one codegen unit, stripped, panic abort     |

## Existing checks

All commands run from the repository root, except Cargo commands from `src-tauri`.

| Command                                                          | Original result                                                              |
| ---------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| `npm run typecheck`                                              | Pass                                                                         |
| `npm run lint`                                                   | Pass                                                                         |
| `npm test`                                                       | 94 passed across 19 files                                                    |
| `npm run build`                                                  | Pass; main JS chunk warning (over 500 kB)                                    |
| `npm run format:check`                                           | Pass                                                                         |
| `cargo fmt --all --check`                                        | Pass                                                                         |
| `cargo check --workspace --locked`                               | Pass                                                                         |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Pass                                                                         |
| `cargo test --workspace --locked`                                | 267 passed; 6 ignored (5 hardware benchmarks, 1 measurement); doc tests pass |

Baseline bundle: main JS 601.89 kB (192.12 kB gzip), speedtest lazy chunk
30.61 kB (9.49 kB gzip), CSS 31.09 kB (6.85 kB gzip). Vite reports 9.39 s
for this build; timings include concurrent diagnostic work and are not a
controlled startup benchmark.

## Application launch

`npm run tauri dev -- --no-watch` successfully started Vite on port 1420 and
`target/debug/taskmanager.exe`. The native process exposed a window titled
"Task Manager". Initial spot observation: 58,707,968 bytes working set,
22,396,928 bytes private memory, 626 handles, 22 threads. This is one debug
process observation, excluding WebView2 child processes, not an idle CPU or
steady-state memory measurement.

The shell initially ignored the supplied working directory; subsequent
commands explicitly use `Set-Location`. CIM hardware queries required
unsandboxed read access. Cargo emitted a sandbox path canonicalization warning
for the user directory; the compiler/checks themselves succeeded.

## Repository discovery

- Rust workspace: `src-tauri` desktop package and `core` member (`tm-core`).
- Entrypoints: `src/main.tsx`, `src/app/App.tsx`, `src-tauri/src/main.rs`,
  `src-tauri/src/lib.rs`.
- Monitoring: `core/src/monitoring/{engine,collector,processes,disks,network}`;
  sysinfo 0.40 plus Win32 and Linux procfs integrations. Single owning thread.
- Process actions: `core/src/process_control`, native platform implementations.
- Benchmarks: `core/src/benchmarks`, coordinator, dedicated runner, CPU kernels
  (sha2, lz4_flex), wgpu 30 GPU workloads, direct disk I/O, WebView Cloudflare
  speedtest 1.15.0, mock factory for deterministic orchestration tests.
- IPC: typed commands in `src-tauri/src/commands`, Tauri Channels for monitoring
  and benchmark events, frontend wrappers in `src/services/ipc.ts`.
- State: Zustand stores in `src/stores`; generated Rust/TS models via ts-rs.
- Process table: TanStack Virtual; charts: uPlot canvas; bounded memory history.
- Persistence: JSON settings and individual benchmark result JSON files; no
  SQL database. History store handles retention and export.
- Platform layer: `core/src/platform/{windows,linux,macos}`; macOS is a stub.
- Logs: tracing, tracing-subscriber, tracing-appender daily files (7 retained),
  bounded recent warning/error buffer.
- Tests: Rust unit/integration/ignored hardware tests; Vitest 5, jsdom and React
  Testing Library. CI checks and packaging on Windows and Ubuntu 24.04.
- Packaging: Tauri bundle all; per-user Windows NSIS plus MSI; Linux deb, rpm,
  AppImage. Explicit CSP and one main-window capability (`core:default`).

Linux and macOS runtime validation cannot be inferred from this Windows host.
Final release, hardware, stress and regression results are recorded separately
in `FULL_DIAGNOSTIC_REPORT.md`.
