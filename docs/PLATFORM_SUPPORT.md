# Platform support

| Platform                             | Status                                                                                                                                                           |
| ------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Windows 11 (x64)                     | Primary target. Built, tested and run during development.                                                                                                        |
| Debian / Ubuntu                      | Supported. Linux-specific code is unit-tested, type-checked and linted from Windows (`npm run check:linux`); full builds and tests run in CI on `ubuntu-latest`. |
| Other mainstream Linux distributions | Expected to work (only `/proc` and standard syscalls are used).                                                                                                  |
| macOS                                | Not yet supported. The platform layer has compiling stubs that report features as unavailable.                                                                   |

## Feature matrix

| Feature                               | Windows                                                                                | Linux                                                          |
| ------------------------------------- | -------------------------------------------------------------------------------------- | -------------------------------------------------------------- |
| CPU total / per core                  | ✓                                                                                      | ✓                                                              |
| CPU frequency                         | Nominal (base) frequency                                                               | Current frequency                                              |
| Load average                          | — (Windows has none)                                                                   | ✓                                                              |
| Memory, swap                          | ✓ (swap = page file)                                                                   | ✓                                                              |
| Cached memory                         | System file cache                                                                      | Page cache + reclaimable slab (as `free`)                      |
| Buffers                               | —                                                                                      | ✓                                                              |
| Commit charge                         | ✓                                                                                      | —                                                              |
| Per-process CPU / memory / I/O        | ✓; restricted processes show CPU and I/O as unavailable                                | ✓                                                              |
| Per-process I/O meaning               | All I/O including network (`GetProcessIoCounters`)                                     | Storage I/O                                                    |
| Process owner                         | Own processes; others need admin                                                       | ✓                                                              |
| Threads / priority                    | ✓ / base priority class                                                                | ✓ / nice                                                       |
| Process architecture                  | ✓ (`IsWow64Process2`)                                                                  | ✓ (ELF header)                                                 |
| Working directory                     | Own processes                                                                          | Own processes (others need root)                               |
| Network connections per process       | ✓ (TCP/UDP, IPv4/IPv6)                                                                 | ✓ (own processes; others need root)                            |
| Ports view (all sockets + owner)      | ✓ (IP Helper owner-PID tables)                                                         | ✓ (`/proc/net` + socket inode scan; others' sockets need root) |
| Open files of a process               | — (needs undocumented handle APIs; not used)                                           | ✓ (`/proc/<pid>/fd`; own processes, others need root)          |
| Find process using a file             | Files only, via Restart Manager (not folders)                                          | Files and folders (`/proc/*/fd`, cwd, exe, maps)               |
| Modules / libraries                   | ✓ (ToolHelp)                                                                           | ✓ (executable + `.so` mappings from `/proc/<pid>/maps`)        |
| Memory map                            | ✓ committed regions (`VirtualQueryEx`); heap/stack not distinguished                   | ✓ (`/proc/<pid>/maps`; heap, main stack, libraries, files)     |
| Threads                               | ID, CPU time, base priority, start time (no state or wait reason)                      | ID, name, state, CPU %, nice, core, start, wait channel        |
| Machine / firmware identity           | — (not read yet)                                                                       | ✓ model, board, BIOS (`/sys/class/dmi/id`; no serial numbers)  |
| CPU caches, max frequency, VT-x/AMD-V | —                                                                                      | ✓ (`/sys/devices/system/cpu`, `/proc/cpuinfo`)                 |
| Battery (charge, health, cycles)      | —                                                                                      | ✓ (`/sys/class/power_supply`)                                  |
| Activity timeline                     | ✓                                                                                      | ✓                                                              |
| Disk throughput, IOPS, active time    | ✓ (drive-letter volumes)                                                               | ✓                                                              |
| End process (graceful)                | Sends `WM_CLOSE` to the process's windows; windowless processes need Force end         | `SIGTERM`                                                      |
| Force end                             | `TerminateProcess`                                                                     | `SIGKILL`                                                      |
| Suspend / resume                      | Suspends every thread (`SuspendThread`); status shown for processes this app suspended | `SIGSTOP` / `SIGCONT`; status read from `/proc`                |
| Priority                              | Idle … High priority classes (no Real-time)                                            | nice 19 … −10; raising needs `CAP_SYS_NICE`                    |
| Critical-process detection            | `IsProcessCritical` + verified System32 paths                                          | PID 1, kernel threads                                          |
| Tray, notifications, autostart        | ✓                                                                                      | ✓ (needs a tray-capable desktop / AppIndicator)                |

## Known differences and limitations

- **Unelevated Windows.** Protected processes (services, `System`, AV) cannot
  be opened. Their memory still comes from `SystemProcessInformation`, but
  CPU, I/O, owner, path and command line are unavailable, and are labelled
  that way rather than shown as 0.
- **Windows status.** Windows has no documented per-process "suspended"
  state. The app marks processes it suspended itself; processes suspended by
  other tools show as Running.
- **Windows CPU frequency.** sysinfo reports the nominal frequency, not the
  current boost clock.
- **Linux disk mapping.** Device-mapper volumes are resolved to `dm-N` for
  `/proc/diskstats`. Network file systems (NFS/CIFS) are excluded by default,
  because querying a hung mount can block.
- **Linux tray.** Needs a desktop environment with StatusNotifier/AppIndicator
  support (GNOME needs the AppIndicator extension). Without one, closing the
  window quits the app and a window that starts hidden is shown after 15 s.
- **Window position on GNOME (Wayland).** Wayland apps cannot read or set
  their own position and GNOME does not remember it, so only size is
  restored natively. At 100% scaling on every monitor, Tasky runs through
  XWayland on GNOME so position is remembered too; with fractional or HiDPI
  scaling it stays on native Wayland to avoid a blurry or tiny window. It
  also stays native if GNOME's display service cannot report every monitor's
  scale. Set `GDK_BACKEND=wayland` (or `x11`) to choose yourself.

- **Windows file lookup.** Restart Manager reports applications and services
  that hold a file, and cannot search folders; the UI says so. Listing every
  open handle of a process would require undocumented `NtQuerySystemInformation`
  structures, so the Open Files tab is unavailable on Windows.
- **Windows memory map paths.** Mapped file names are NT device paths
  (`\Device\HarddiskVolumeN\…`) as the system reports them.
- **Windows inspection code is untested on a real machine.** It compiles and
  passes clippy for `x86_64-pc-windows-msvc` and has tests that run in the
  Windows CI job, but was written on Linux.
- **Other users' processes (Linux).** Open files, memory maps, threads and
  socket owners of processes owned by other accounts need root. The UI reports
  how many processes could not be inspected instead of showing an empty result.

## Linux build prerequisites

```sh
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

See the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for
other distributions.
