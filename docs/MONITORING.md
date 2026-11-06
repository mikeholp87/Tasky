# Monitoring

How each metric is collected, refreshed and calculated.

## Refresh schedule

| Data                                             | Frequency       | Source                                               |
| ------------------------------------------------ | --------------- | ---------------------------------------------------- |
| CPU (total, per core, frequency)                 | every tick      | sysinfo                                              |
| Load average (Unix)                              | every tick      | sysinfo                                              |
| Memory, swap                                     | every tick      | sysinfo                                              |
| Cached / buffers (Linux)                         | every tick      | `/proc/meminfo`                                      |
| System cache, commit charge (Windows)            | every tick      | `GetPerformanceInfo`                                 |
| Processes: CPU, memory, I/O                      | every tick      | sysinfo                                              |
| Processes: exe, command line, user               | once per PID    | sysinfo (`UpdateKind::OnlyIfNotSet`)                 |
| Threads, priority (Windows)                      | every tick      | `NtQuerySystemInformation(SystemProcessInformation)` |
| Threads, nice (Linux)                            | every tick      | `/proc/[pid]/stat`                                   |
| Disk throughput                                  | every tick      | sysinfo (`IOCTL_DISK_PERFORMANCE` on Windows)        |
| Disk IOPS, active time                           | every tick      | `IOCTL_DISK_PERFORMANCE` / `/proc/diskstats`         |
| Disk list, capacity                              | every 30 s      | sysinfo                                              |
| Network rates and totals                         | every tick      | sysinfo                                              |
| OS, CPU model, core counts, boot time            | once at startup | sysinfo                                              |
| Process details (cwd, architecture, connections) | when opened     | sysinfo, IP Helper / `/proc/net`                     |

A tick happens every 500 ms / 1 s / 2 s / 5 s (default 1 s), or on demand.

## Rates

Every rate divides a counter delta by the **measured** time between samples
(`Instant` elapsed), never by the nominal interval. Timers drift, ticks can be
late, and "refresh now" creates irregular spacing.

- `counter_rate(prev, cur, elapsed)`: a counter that goes backwards (reset,
  wrap, re-attached device) yields 0 rather than a huge spike.
- A disk or interface seen for the first time reports 0 until it has a
  baseline.
- Disk active time on Windows uses the OS's own clock (`QueryTime`) and
  idle time; on Linux it is I/O milliseconds over elapsed time.

## CPU percentages

- System CPU is 0–100 across all logical processors.
- Process CPU is **normalized to the whole machine** (sysinfo reports
  per-core percentages, up to `cores × 100`; we divide by the core count), as
  Windows Task Manager does. `top`'s "Irix mode" would show 100% for one
  saturated core; here that is `100 / cores`.

## Unavailable versus zero

The app never reports an invented number:

- Values a platform cannot provide are `null` and shown as
  "Unavailable on this platform".
- On Windows, processes an unelevated app cannot open ("restricted") get real
  memory from a handle-free system snapshot. Their CPU and disk activity are
  shown as unavailable instead of sysinfo's default 0.
- Owners of other accounts' processes are shown as unavailable on Windows
  without elevation. Windows withholds the owner SID even from
  `WTSEnumerateProcessesEx`, which was tried and removed.

## Memory diagnostics

Open Memory or select **Explain usage** on Overview. The view ranks groups
by summed resident memory / working set, using executable paths when known
and process names otherwise (case insensitive on Windows). Expand a group
to inspect individual PIDs using the existing process details view.

Recent growth compares each current group's total with its total in the
oldest available sample within the last minute. Group totals are sampled
every five seconds while the window receives monitoring ticks, regardless
of the selected page. At most 13 grouped samples are retained; no command
lines or full process rows are retained by this history. New groups use a
zero baseline, exited processes reduce a group's total, and growth is not
evidence of a leak. After a gap longer than a minute or an engine restart,
a new baseline is collected. No process history is backfilled for this view.

Working sets can count shared pages multiple times and exclude some system
allocations. Their sum is not an exact partition of RAM; subtraction from
system usage cannot identify hidden memory. Cache overlaps other figures,
and available memory already includes reclaimable pages. Windows cache is
the standby list plus the system working set. Commit measures promised
memory, not physical RAM usage.

Windows kernel paged and nonpaged pools come from the existing
`GetPerformanceInfo` call. Nonpaged pool stays in RAM; paged pool can include
pages on disk. Pool counters are unavailable on other platforms. The view
highlights nonpaged pool at 10% or more of usable RAM as an investigation
hint, and commit charge at 90% or more of its limit. Neither identifies a
specific leaking driver. See Microsoft's
[performance counter definitions](https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-performance_information)
and [working set explanation](https://learn.microsoft.com/en-us/windows/win32/memory/working-set).

## History

- System history: a ring buffer in the engine sized from the "History length"
  setting (15/30/60 min, at most 7,200 points). The frontend keeps a matching
  buffer, backfilled once at startup and again whenever the window becomes
  visible.
- Per-process history: 300 points per process, keyed by (PID, start time) so
  a reused PID never inherits another process's graph. Pruned when the
  process exits.
- Per-disk and per-interface history: 300 points, frontend only, from the
  time the app started.

Nothing is written to disk; history is memory-only by design.

## Alerts

Rules are evaluated on the engine thread, so they work while the window is in
the tray. A rule fires once per _episode_: when its condition has held for
`duration` seconds. It re-arms only after the condition clears, so a
sustained spike produces one notification. Process rules match names
case-insensitively and ignore a trailing `.exe`, so `node` works on both
platforms. Restricted processes never trigger CPU rules, because their CPU is
unknown.
