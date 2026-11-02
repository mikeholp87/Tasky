# Roadmap

## Done (v0.1)

Overview, Processes (list and tree, search, filters, columns, heat mapping,
virtualized), Process Details (history, connections, architecture), safe
process control, Performance (60 s – 1 h), CPU, Memory, Storage (IOPS, active
time), Network, System Information, Alerts with desktop notifications,
persisted settings, tray, autostart, CSV/JSON export, diagnostics, command
palette, light/dark/system themes.

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
- **Startup applications** and **Services** pages.
- **Process history persistence** (opt-in, SQLite, retention 1 h – 30 d) as
  an extra `TickSink`.
- **GPU, temperatures, fans, battery** via sysinfo's `component`/`gpu`
  features where reliable.
- **Docker containers and WSL processes** views.
- **Port viewer** (all listening sockets, not just per process).

## Considered and deferred

- **Differential process updates over IPC.** Full snapshots measured at
  70–120 KB per tick are cheap enough (see `PERFORMANCE.md`). Revisit if
  profiling on larger systems shows IPC cost matters.
- **AI-assisted diagnosis.** The diagnostics bundle and alert log are the
  intended inputs. Any such feature must be strictly opt-in and must show
  exactly what would leave the machine.
