# Roadmap

## Done (v0.1)

Overview, Processes (list and tree, search, filters, columns, heat mapping,
virtualized), Process Details (history, connections, architecture), safe
process control, Performance (60 s – 1 h), CPU, Memory, Storage (IOPS, active
time), Network, System Information, Alerts with desktop notifications,
persisted settings, tray, autostart, CSV/JSON export, diagnostics, command
palette, light/dark/system themes.

## Done (diagnostics, 2026-11)

Ports view with port-conflict diagnosis, Find Process Using File, per-process
Open Files, Modules, Memory Map and Threads tabs, and the Activity timeline
(starts/exits, CPU spikes, memory jumps, alerts, actions, benchmarks). See
`FEATURE_GAP_ANALYSIS.md` for the research and scoring behind them.

Also done: Applications view in the process table (processes grouped by
program), new-process highlighting (configurable, default 10 s), memory- and
thread-growth hints in the activity log, and a Diagnose tab in Process Details.

## Next

- **Windows: elevation helper.** A narrow, signed helper process for actions
  that need admin rights (ending a protected process, reading restricted
  CPU/owner data) instead of elevating the whole app.
- **Windows: CPU for restricted processes.** The CPU times in
  `SystemProcessInformation` are undocumented; investigate a documented
  source (e.g. PDH process counters) before using them.
- **macOS support.** Fill in `platform/macos` (`proc_pidinfo`, `kill`,
  `setpriority`, `IOKit` disk stats) and sign/notarize the bundle.
- **Automatic updates** with the Tauri updater, once releases are signed.
- **Startup applications** and **Services** pages (P1 in the gap analysis;
  Services needs a D-Bus client for systemd).
- **File-descriptor / handle growth hints.** Needs counting descriptors for
  selected processes over time; memory and thread growth are already covered.
- **Wait/blocking analysis** beyond the thread wait channels shown today
  (Windows wait-chain traversal; Linux futex owners).
- **Logged-in users** view and **global resource search** (files, ports,
  processes, modules in one box).
- **Thread suspend/resume** (advanced, with warnings) and complete Windows
  handle enumeration, only if an isolated, clearly documented approach is
  acceptable.
- **Process history persistence** (opt-in, SQLite, retention 1 h – 30 d) as
  an extra `TickSink`.
- **GPU, temperatures, fans, battery** via sysinfo's `component`/`gpu`
  features where reliable.
- **Docker containers and WSL processes** views.

## Considered and deferred

- **Differential process updates over IPC.** Full snapshots measured at
  70–120 KB per tick are cheap enough (see `PERFORMANCE.md`). Revisit if
  profiling on larger systems shows IPC cost matters.
- **AI-assisted diagnosis.** The diagnostics bundle and alert log are the
  intended inputs. Any such feature must be strictly opt-in and must show
  exactly what would leave the machine.
