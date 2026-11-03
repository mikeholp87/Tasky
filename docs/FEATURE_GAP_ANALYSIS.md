# Feature gap analysis

Research date: **2026-11-03**. Software changes; re-check before relying on a cell.

The goal is to find diagnostic capabilities that users of mature Windows and Linux
monitors rely on and this app lacks, then build the highest-value ones with
independent implementations. Nothing here copies competitor code, assets, names or
internals; only publicly documented user-facing capabilities were compared.

## Method and sources

"Our App" cells come from reading this repository (Rust commands in
`src-tauri/src/commands`, routes in `src/app/routes.ts`, process-details UI), not from
assumption. Competitor cells come from primary pages fetched on the research date:

- Process Explorer - [Microsoft Learn](https://learn.microsoft.com/en-us/sysinternals/downloads/process-explorer)
  (page dated 2026-10-15): handle mode, DLL mode, search for handles/DLLs, leak finding.
- Restart Manager - [Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/rstmgr/restart-manager-portal):
  identifies applications holding files; intended for installers.
- Windows Task Manager - [Microsoft support](https://support.microsoft.com/en-us/windows/experience/startup-boot/configure-startup-applications-in-windows),
  [Learn troubleshooting guide](https://learn.microsoft.com/en-us/troubleshoot/windows-server/support-tools/support-tools-task-manager):
  Startup apps with impact ratings (None / Not measured / Low / Medium / High, based on CPU
  and disk use at startup), Services tab, Details tab, **Analyze wait chain**.
- System Informer - [GitHub README](https://github.com/winsiderss/systeminformer): resource
  graphs, "which processes are using a file you can't edit or delete", network connections,
  disk access, stack traces, service control; MIT licensed.
- GNOME System Monitor - [help](https://help.gnome.org/users/gnome-system-monitor/stable/):
  memory maps, list open files of a process, find which program uses a file, resources,
  file systems.
- Plasma System Monitor - [apps.kde.org](https://apps.kde.org/plasma-systemmonitor/) and release
  notes: Applications page (6.4 added a Background services group), History page with GPU
  graph, re-nice dialog (6.6), CPU affinity (6.8).
- Mission Center - [README](https://gitlab.com/mission-center-devs/mission-center): per-core CPU,
  memory, disk, network, GPU (usage, encoder/decoder, power), fans, battery, Apps page
  (per-app and per-process breakdown), Services (systemd or OpenRC). Per-process network
  needs manual setup.
- btop - [GitHub](https://github.com/aristocratos/btop): process tree, filter, detailed stats for
  the selected process, send any signal, network/disk graphs, GPU, sensors.
- htop - [GitHub](https://github.com/htop-dev/htop): tree view, filter, search, kill and renice.

Not reached with primary text (404 or thin pages): Process Monitor, Autoruns, RAMMap,
VMMap, KDE handbook, Wait Chain Traversal API page. Cells marked **†** below rest on
general knowledge of the tool, not on a page fetched in this research; verify before quoting.

## Matrix

Values: Yes, Partial, No, Platform-specific, Not applicable. "Our App" is the state at
commit `808360d`, before this work. Windows Task Manager = WTM, Process Explorer = PE,
System Informer = SI, Mission Center = MC.

| Feature                            | Our App                    | WTM                                    | PE                      | SI       | GNOME    | KDE                    | MC         | btop              |
| ---------------------------------- | -------------------------- | -------------------------------------- | ----------------------- | -------- | -------- | ---------------------- | ---------- | ----------------- |
| Process list + tree                | Yes                        | Partial†                               | Yes†                    | Yes†     | Yes      | Yes†                   | Partial    | Yes               |
| Kill / suspend / priority          | Yes                        | Partial†                               | Yes†                    | Yes†     | Yes      | Yes                    | Partial†   | Partial (signals) |
| Per-process network connections    | Yes                        | No†                                    | Yes†                    | Yes      | No†      | Partial†               | Partial    | Partial†          |
| Global listening-ports view        | No                         | No†                                    | No†                     | Yes†     | No       | No†                    | No         | No                |
| Open files of a process            | No                         | No                                     | Yes                     | Yes†     | Yes      | No†                    | No         | No                |
| Find which process uses a file/dir | No                         | No                                     | Yes                     | Yes      | Yes      | No                     | No         | No                |
| Loaded modules / libraries         | No                         | No                                     | Yes                     | Yes†     | Partial† | No†                    | No         | No                |
| Memory map                         | No                         | No                                     | Partial†                | Yes†     | Yes      | No†                    | No         | No                |
| Thread list                        | No                         | No†                                    | Yes†                    | Yes†     | No       | No†                    | No         | No                |
| Thread suspend/resume              | No                         | No                                     | Yes†                    | Yes†     | No       | No                     | No         | No                |
| Wait / blocking analysis           | No                         | Platform-specific (Analyze wait chain) | Partial†                | Yes†     | No       | No                     | No         | No                |
| Services manager                   | No                         | Platform-specific                      | No                      | Yes      | No       | No†                    | Yes        | No                |
| Startup app manager                | No (own autostart only)    | Platform-specific (with impact)        | No                      | No†      | No       | No†                    | No         | No                |
| Logged-in users                    | No                         | Platform-specific†                     | No                      | Yes†     | No       | No                     | No         | No                |
| Application history (cumulative)   | No                         | Platform-specific†                     | No                      | No       | No       | Partial (History page) | No         | No                |
| Application grouping               | Partial (Memory page only) | Yes†                                   | No                      | No       | No       | Yes (Applications)     | Yes (Apps) | No                |
| Activity / event timeline          | No (alerts log only)       | No                                     | No†                     | No       | No       | No                     | No         | No                |
| Spike detection                    | No                         | No                                     | No                      | No       | No       | No                     | No         | No                |
| Handle / fd leak hints             | No                         | No                                     | Partial (finding leaks) | Partial† | No       | No                     | No         | No                |
| GPU / sensors                      | Yes (basic)                | Yes†                                   | No                      | Partial† | No       | Yes                    | Yes        | Yes               |
| Alerts + notifications             | Yes                        | No                                     | No                      | No†      | No       | No                     | No         | No                |
| CPU benchmark                      | Yes                        | No                                     | No                      | No       | No       | No                     | No         | No                |
| History persistence                | No (memory only)           | Partial†                               | No                      | No†      | No       | Partial                | No         | No                |
| Export / diagnostics bundle        | Yes                        | No                                     | Partial†                | Partial† | No       | No                     | No         | No                |

## Scoring

Value axes (higher is better): User, Diagnostic, Cross-platform. Cost axes (higher is
worse): Implementation, Security risk, Performance risk, Privilege needed, Maintenance.
Priority is a judgement from these, not a formula: high value with low risk and no extra
privilege floats up; admin-only or undocumented-API features sink.

| Candidate                                                 | User | Diag | X-plat | Impl | Sec | Perf | Priv | Maint | Priority                                        |
| --------------------------------------------------------- | ---- | ---- | ------ | ---- | --- | ---- | ---- | ----- | ----------------------------------------------- |
| Ports view + port-conflict diagnosis                      | 5    | 4    | 5      | 2    | 1   | 2    | 2    | 2     | **P0**                                          |
| Find process using file/dir                               | 5    | 5    | 4      | 3    | 2   | 3    | 3    | 3     | **P0**                                          |
| Per-process open files                                    | 4    | 5    | 3      | 2    | 2   | 2    | 3    | 2     | **P0** (Linux full; Windows limited)            |
| Modules / loaded libraries                                | 4    | 4    | 5      | 2    | 1   | 1    | 2    | 2     | **P0**                                          |
| Memory map                                                | 3    | 5    | 4      | 3    | 1   | 2    | 2    | 2     | **P0**                                          |
| Threads (selected process only)                           | 4    | 5    | 5      | 3    | 1   | 2    | 2    | 3     | **P0**                                          |
| Activity timeline + start/exit + spike events             | 4    | 4    | 5      | 3    | 1   | 2    | 1    | 3     | **P0**                                          |
| Application grouping in process table                     | 5    | 3    | 5      | 3    | 1   | 1    | 1    | 2     | P1                                              |
| New-process highlight                                     | 3    | 3    | 5      | 1    | 1   | 1    | 1    | 1     | P1                                              |
| Services (systemd D-Bus / SCM)                            | 4    | 3    | 4      | 4    | 3   | 1    | 4    | 4     | P1 (needs zbus)                                 |
| Startup manager (XDG autostart, Run keys, startup folder) | 4    | 3    | 4      | 3    | 3   | 1    | 3    | 3     | P1                                              |
| Memory growth / fd-leak hints                             | 4    | 4    | 5      | 3    | 1   | 2    | 1    | 3     | P1 (needs retained history)                     |
| Wait / blocking analysis (wchan; Windows wait-chain API)  | 3    | 5    | 3      | 4    | 2   | 2    | 3    | 4     | P1                                              |
| Diagnose Process (facts from the above)                   | 4    | 5    | 5      | 3    | 1   | 1    | 1    | 2     | P1 (after threads/files)                        |
| Users / sessions                                          | 3    | 2    | 3      | 4    | 2   | 1    | 3    | 3     | P2 (logind D-Bus)                               |
| Persisted app history (SQLite)                            | 4    | 3    | 5      | 4    | 2   | 2    | 1    | 4     | P2 (opt-in, bounded)                            |
| Global resource search                                    | 4    | 4    | 5      | 3    | 1   | 2    | 2    | 2     | P2 (builds on ports/files)                      |
| Thread suspend/resume                                     | 2    | 3    | 3      | 3    | 4   | 1    | 3    | 3     | P3 (deadlock risk; advanced + warning)          |
| Full Windows handle enumeration                           | 3    | 4    | 1      | 5    | 4   | 3    | 5    | 5     | P3 (needs undocumented NT interface; not built) |

## Phase 1 selection

Built in this change: Ports + conflict diagnosis, File Lock Finder + per-process Open
Files, Modules + Memory Map, Threads, Activity timeline. Reasons:

- All are read-only, need no elevated privilege for the user's own processes, and add no
  work to the steady-state polling loop (they run on demand when a page or tab is open).
  The one exception is the activity diff, which compares two in-memory snapshots per tick.
- They answer the questions users bring to a task manager that this app could not:
  "what owns port 3000", "why can't I delete this", "what is this process doing".
- They need no new dependencies; Linux uses `/proc`, Windows uses the `windows` crate that
  is already linked.

Deferred (backlog above): services, startup, users, persisted history, leak hints, wait
analysis, Diagnose, application grouping, global search, thread suspend/resume.

## Phase 2 (quick wins)

Built next, with no new dependencies: Applications view (groups by executable;
summed memory is labelled as resident memory counted per process), new-process
highlight, memory- and thread-growth hints (one sample per minute for up to an
hour, reported once per process, worded as "potential"/"possible"), and a
Diagnose tab that lists measurable facts and recognises one pattern ("consistent
with idle or blocked"), never "deadlocked". Not built: file-descriptor growth
tracking (needs per-process polling) and recently-exited highlighting (needs row
retention).

## Windows approach and limits

- **Find process using a file:** Restart Manager (`RmGetList`) is the documented route. It
  works for files, is aimed at installers, and does not enumerate directory handles or
  every handle type. The UI states that limit on Windows.
- **Per-process open files:** complete handle enumeration needs undocumented
  `NtQuerySystemInformation` structures. Not implemented; the Open Files tab says
  "Not supported on Windows" there. If added later it must live in an isolated module with a
  clear warning.
- **Modules:** ToolHelp module snapshot. **Memory map:** `VirtualQueryEx` walk.
  **Threads:** ToolHelp thread snapshot plus `GetThreadTimes`/`GetThreadPriority`.
  **Ports:** the IP Helper TCP/UDP owner-PID tables already used for per-process connections.
- Windows code is compiled and linted (`npm run check:windows`) but has **not been run on
  a Windows machine** in this work.

## Linux approach and limits

- Sources: `/proc/<pid>/{fd,fdinfo,maps,task/<tid>/*}` and `/proc/net/{tcp,tcp6,udp,udp6}`.
  No `lsof`, `ss` or `netstat` is executed.
- Other users' processes need root. Unreadable processes are counted and reported
  ("N processes could not be inspected"), never silently treated as "not in use".
- Socket-to-process mapping scans every process's fd directory, so it runs only when the
  Ports page is open, at a slow interval, and with a time budget.
