# Implementation Plan — Cross-platform Task Manager & System Monitor

## Context
The user wants a production-quality desktop task manager (Windows 11 first-class, Debian/Ubuntu Linux, macOS-ready) built incrementally across 9 phases, with a written `docs/IMPLEMENTATION_PLAN.md` first and continuous implementation afterwards.

**Environment findings (2026-11-02):**
- `C:\Users\digit\Projects\TaskManager` is empty and not a git repo.
- Windows 11 Pro 26300, Node 24.22, npm 11.19, git 2.56, winget available, WebView2 156 installed.
- **Missing:** Rust toolchain and MSVC C++ Build Tools. User chose: I install them via winget.
- Current npm versions: @tauri-apps/cli/api 2.12.4, react 19.3, vite 8.3, tailwindcss 4.3, zustand 5.0, recharts 3.10, @tanstack/react-virtual 3.14, vitest 5.0, lucide-react 1.66, typescript 7.0, plugins notification 2.8 / store 2.5 / autostart 2.7 / window-state 2.5.
- User chose: `git init` + one local commit per verified phase (no push).

## Step 0: Environment setup
1. `winget install Rustlang.Rustup`, then `rustup default stable` and `rustup component add clippy rustfmt`.
2. `winget install Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"`. This triggers a UAC prompt.
3. Verify with `cargo --version` and `rustc --version`. Get crate versions from `cargo search` / crates.io for `sysinfo`, `windows`, `nix`, `thiserror`, `serde`, `ts-rs`, `tracing`, `tracing-appender`.
4. Run `git init` and add a `.gitignore` covering node_modules, dist, and src-tauri/target.

## Key technology decisions (deviations from the brief, with reasons)
- **Charts: uPlot instead of Recharts** for live time series. It draws on canvas and is about 50 KB. The Performance page draws one chart per core plus 4 large charts updating every second, and Recharts' SVG re-render cost is the biggest frontend CPU risk. Keep Recharts out unless a non-timeseries chart needs it.
- **Generate TS types from Rust with `ts-rs`** into `src/types/generated/`. This gives one source of truth, so models can't drift.
- **Process table: `@tanstack/react-virtual` plus our own column/sort/filter model.** Skip react-table: v9 is a new major, and our needs are narrow.
- **UI primitives: shadcn/ui-style components on Radix** (Dialog, ContextMenu, DropdownMenu, Tooltip, Switch, Select), copied into `src/components/ui/`, with Tailwind v4 and Lucide.
- **TypeScript:** try TS 7. If typescript-eslint or Vite tooling doesn't support it yet, pin the newest supported 5.x/6.x and record why in the docs.
- **Tauri plugins:** store (settings), window-state, autostart, notification. Tray uses the core `tray-icon` feature. No shell plugin, and no generic opener scope: "open executable location" is a Rust command that takes a **PID** and resolves the path itself.

## Architecture

```
OS ─► platform/* (sysinfo + native APIs) ─► MonitoringEngine (1 dedicated thread)
        ─► Arc<RwLock<EngineState>> (latest snapshot + ring buffers)
        ─► tauri::ipc::Channel<MetricsTick> ─► Zustand stores ─► memoized React views
```

- **One monitoring thread** owns the `sysinfo::System`/`Networks`/`Disks`. A `std::thread` loop with a control `mpsc` receiver handles SetInterval, Pause, Resume, RefreshNow, TrackPid, and Shutdown. It sleeps with `recv_timeout`, so pausing costs zero CPU.
- **Tiered refresh:** CPU, memory, network, and processes every tick. Disk capacity every 30 s. Static system info once at startup (`get_static_info` command).
- **Rates** are computed from counter deltas divided by the measured `Instant` elapsed time, never the nominal interval. This lives in `monitoring/rates.rs` as pure functions with unit tests.
- **Locks:** collect into local structs with no lock held, then swap the result into state with a short write lock.
- **Ring buffers:** generic `RingBuffer<T>` (`monitoring/history.rs`). The system history holds 3600 points (1 h at 1 s, configurable). Per-process CPU/mem history holds 300 points, and dead PIDs are pruned. The frontend fetches history once via `get_history(range)`, then appends live ticks.
- **Process refresh:** `ProcessRefreshKind` with cpu+memory+disk_usage every tick. exe, cmd, cwd, and user use `UpdateKind::OnlyIfNotSet` so they are read once per PID.

### Rust layout (`src-tauri/src/`)
`lib.rs` (builder/plugins/setup only), `commands/{system,processes,control,settings,diagnostics,export}.rs`, `monitoring/{engine,cpu,memory,processes,disks,network,history,rates}.rs`, `process_control/{mod,protection}.rs`, `platform/{mod.rs (traits), linux/, windows/, macos/ (stubs)}`, `models/*.rs` (Serde + ts-rs), `errors.rs` (`AppError` via thiserror: PermissionDenied, ProcessNotFound, UnsupportedOperation, SystemQueryFailed, InvalidPid, ProcessProtected, serialized as `{kind, message}`), `state.rs`, `alerts/` (rule evaluation in Rust so alerts work while the window is hidden in the tray), `tray.rs`, `logging.rs`.

### Platform abstraction
The `ProcessManager` trait has `terminate`, `force_kill`, `suspend`, `resume`, `set_priority`, `details_extra` (cwd, arch, priority/nice, session), and `protection_info(pid)`. `platform::current()` returns the impl, and `#[cfg]` appears only in `platform/mod.rs`.
- **Windows** (`windows` crate):
  - Terminate: `WM_CLOSE` to top-level windows of the PID (graceful); `TerminateProcess` for force kill.
  - Suspend/resume: Toolhelp32 thread snapshot plus `SuspendThread`/`ResumeThread` (documented APIs, not `NtSuspendProcess`).
  - Priority: `SetPriorityClass`. Architecture: `IsWow64Process2`.
  - Protection: `IsProcessCritical`, PID 0/4, and session-0 System32 binaries (csrss, wininit, smss, services, lsass, winlogon) **verified by full path**, not by name.
- **Linux** (`nix`):
  - Terminate sends SIGTERM, force kill sends SIGKILL, suspend/resume send SIGSTOP/SIGCONT.
  - Priority: `setpriority` (nice). Raising priority needs CAP_SYS_NICE, which shows up as a clean PermissionDenied.
  - Protection: PID 1, kernel threads (ppid 2 / no exe), and own PID plus ancestors.
  - `/proc/diskstats` for IOPS, `/proc/loadavg` via sysinfo.
- **ESRCH / ERROR_INVALID_PARAMETER** map to `ProcessNotFound`, and a process that vanished mid-read is skipped silently (debug-level log at most).
- Metrics a platform can't report are `Option<T>` → the UI shows "Unavailable on this platform".

### IPC
- Commands: `subscribe_metrics(channel)`, `get_static_info`, `get_history(range)`, `get_process_details(pid)`, `get_process_history(pid)`, `set_monitoring(interval|paused)`, `refresh_now`, `terminate/force_kill/suspend/resume/set_priority(pid,…)`, `reveal_executable(pid)`, `export_processes(format, include_cmdline)`, `export_diagnostics(opts)`, alert rule CRUD.
- Every PID is validated as `u32 > 0`, must exist in the current snapshot, and goes through a protection check.
- The capabilities file grants only the needed plugin permissions and the app's own commands.
- `MetricsTick` = dynamic system metrics plus a lean process row list (no environment). Measure the payload in Phase 8 before considering diffs.

### Frontend layout (`src/`)
- `app/` (shell, router, sidebar, top summary bar, theme provider)
- `features/{overview,processes,performance,cpu,memory,storage,network,process-details,system-info,alerts,settings}/`, each with `components/`, `hooks/`, and pure `logic.ts` (filter/sort/search/tree building, tested with Vitest)
- `stores/` (metrics, history, ui, settings; selectors keep re-renders scoped)
- `services/ipc.ts` (the only place that calls `invoke`/Channel)
- `utils/format.ts`, `types/generated/`, `components/ui/`
- Routing uses a tiny internal state router with no react-router dependency.

## Phases (each ends with: cargo fmt/clippy/test, tsc, eslint, vitest, launch `tauri dev`, update the plan doc, git commit)
1. **Shell.** Write `docs/IMPLEMENTATION_PLAN.md` first. Scaffold with `create-tauri-app` (react-ts), then add Tailwind v4, the sidebar with 11 sections, light/dark/system theme, placeholder dashboard, ESLint/Prettier, and the Vitest setup.
2. **System metrics.** Engine thread, CPU (total, per-core, freq, model, load avg), memory/swap, static info, ring buffer, rates, format utils plus tests. Overview cards with uPlot sparklines.
3. **Processes.** Enumeration, virtualized table, column visibility/resizing, sorting, instant search (name/PID/cmd/exe/user), filter presets, heat mapping, keyboard navigation, mock generator for 100/500/1k/5k processes (dev perf mode).
4. **Details + tree.** Detail page (general/CPU/mem/disk with history), tree view with expand/collapse all, per-process network connections where the native API is reliable (Windows `GetExtendedTcpTable`, Linux `/proc/net` plus inode mapping). Otherwise show "Unavailable".
5. **Process control.** Context menu, copy actions, reveal, terminate/force kill with escalating confirmation dialogs, suspend/resume/priority, protection safeguards, Delete key opens the dialog only.
6. **Performance.** CPU/memory/disk/network large charts, 60s/5m/15m/1h ranges, per-core grid, per-disk throughput/IOPS, per-interface network.
7. **Alerts, settings, tray, export, diagnostics.** Rule engine with sustained-duration windows and desktop notifications, settings persistence (store plugin), autostart/minimize-to-tray/window-state, tray menu and tooltip (updated at most every 2 s), CSV/JSON export with a command-line warning, diagnostics bundle, rotating logs (tracing-appender, capped files), command palette (Ctrl+K).
8. **Optimize.** Dev diagnostics panel (poll ms, process count, payload bytes, render ms). Profile the app's own CPU/RAM against targets (<1% idle, <150 MB) and fix measured hotspots only.
9. **Harden & package.** Clippy `-D warnings`, `tauri build` → MSI/NSIS on Windows. Document Linux `.deb`/AppImage/`.rpm` builds; verify those via WSL if available, otherwise mark as documented and not verified locally. README plus `docs/{ARCHITECTURE,MONITORING,PLATFORM_SUPPORT,SECURITY,PERFORMANCE,ROADMAP}.md`.

## Known limitations to state honestly
- Linux builds/runtime can't be tested natively on this Windows machine. Use WSL2 if present for `cargo check`/tests on Linux code, and keep CI config (GitHub Actions matrix) in the repo for real verification.
- Windows per-process disk I/O comes from sysinfo (`GetProcessIoCounters`), which includes non-disk I/O. Label it "I/O" with a tooltip.
- Elevated operations are never auto-elevated. PermissionDenied explains how to rerun as admin/root for that action. A narrow helper is a roadmap item.

## Verification
- After each phase: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`, `npx tsc --noEmit`, `npx eslint .`, `npx vitest run`, `npm run tauri dev`, then confirm the window renders real data (spawn a throwaway `ping -t`/`timeout` process to test search, terminate, and suspend; never kill pre-existing processes).
- Rust tests: rates, ring buffer, error mapping, protection rules, process conversion, engine produces a valid snapshot. Frontend tests: format, filter/sort/search, tree building, alert evaluation, settings store, IPC tick handling with mock data.
- Phase 8: measure the app's own CPU/RAM in Task Manager / sysinfo self-stats and record the results in `docs/PERFORMANCE.md`.
- Final: `npm run tauri build` produces an installer, and the installed app launches.

## Version pins (checked 2026-11-02)
- Rust 1.99.1 stable (MSVC toolchain). Tauri **2.x stable**: crates.io's newest is `3.0.0-alpha`, which is not production-ready, so we stay on Tauri 2 and its 2.x plugins.
- sysinfo 0.40, windows 0.62, nix 0.31, thiserror 2, ts-rs 12, tracing 0.1 / tracing-appender 0.2.

## Performance risks
| Risk | Mitigation |
|---|---|
| Full process refresh every tick (1000+ PIDs) | Narrow `ProcessRefreshKind`; read static fields once per PID; measure poll ms in the diagnostics panel |
| IPC payload size (~200 B × N processes per tick) | Lean rows, no environment; measure bytes per tick in Phase 8 before adding diffs |
| React re-rendering the whole table each tick | Virtualized rows, memoized row components keyed by PID, store selectors |
| Chart rendering cost (per-core charts) | uPlot canvas, `setData` in place, charts on hidden pages don't render |
| Lock contention between engine and commands | Collect without locks; swap under a short `RwLock` write |
| Background CPU while minimized to tray | Tray tooltip throttled to ≥2 s; when hidden, the engine keeps sampling only for alerts and skips history fan-out |

## Security risks
| Risk | Mitigation |
|---|---|
| Frontend abuse of process control | Typed commands only; PID validated (>0, exists); protection check in Rust, not just the UI |
| Arbitrary path/shell execution | No shell plugin, no generic exec; reveal-executable resolves the path from the PID server-side |
| Over-broad Tauri capabilities | Single capability file listing only needed plugin permissions |
| Leaking sensitive data | No network calls; exports/diagnostics exclude command lines unless explicitly opted in; never log environments |
| Privilege escalation | App runs unelevated; PermissionDenied surfaced with guidance instead of auto-elevation |
