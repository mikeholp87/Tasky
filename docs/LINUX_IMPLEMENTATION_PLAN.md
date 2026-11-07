# Linux implementation plan

Written 2026-11-03 after inspecting the repository on the target machine.
It records what already existed, what was missing, and what this pass
changes. Results are in [`LINUX_PORT_REPORT.md`](LINUX_PORT_REPORT.md).

## 1. Existing architecture

- **Workspace.** `src-tauri/` (Tauri 2 shell, crate `taskmanager`) and
  `src-tauri/core/` (crate `tm-core`: engine, models, platform layer; no Tauri
  dependency, so it builds and tests without WebKitGTK).
- **Engine.** One dedicated thread owns all polling (`monitoring/engine.rs`).
  A persistent `sysinfo::System`, `Disks` and `Networks` live in
  `Collector`; it never re-creates them. Ticks are skipped rather than
  queued when a cycle overruns. The tick is serialized once and pushed to the
  webview through a Tauri channel.
- **Platform layer.** `core/src/platform/{linux,windows,macos}` behind
  `platform/mod.rs`; higher layers do not use `cfg(target_os)`. Pure `/proc`
  parsers live in `platform/procfs.rs` and are unit-tested on every OS.
- **Process control.** `process_control/` validates PID + start time
  (PID-reuse guard), evaluates protection rules server-side, requires
  acknowledgement for warnings, then calls the platform. Linux uses
  `kill(2)`/`setpriority(2)` through `libc`.
- **Frontend.** React 19 + TypeScript + Vite + Zustand + uPlot. Types are
  generated from Rust with `ts-rs` (`npm run gen:types`). One frontend for all
  platforms; it already branches on `staticInfo.platform` for labels
  (Swap vs Page file, Device vs Volume label).
- **Toolchain.** Rust edition 2024, MSRV 1.90; Tauri 2.12; Node 24; npm.

## 2. Linux compatibility gaps found

| Gap                                                           | Evidence on this machine                                                         |
| ------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| App crate never built locally on Linux                        | WebKitGTK/AppIndicator dev packages missing                                      |
| Distribution name incomplete                                  | `os_version` = None, `long_os_version` = "Linux (Debian GNU/Linux)" on forky/sid |
| Desktop, display server, container, service manager not shown | not collected                                                                    |
| Kernel threads clutter the list                               | 114 of 349 rows were kernel threads                                              |
| States `D` and `t` folded into Sleeping/Stopped               | `ProcessState` had no Disk sleep / Tracing                                       |
| Zombies could be "ended"                                      | no protection rule; signals do nothing to a zombie                               |
| Network interfaces not classified; totals include VPN/virtual | `tailscale0` summed with `wlo1`; Docker `veth*` would double count               |
| Disk history double-counts devices mounted twice              | btrfs subvolumes / bind mounts share one device                                  |
| A few pseudo file systems not filtered                        | cgroup, debugfs, tracefs, … rely on size 0 only                                  |
| No temperatures                                               | ROADMAP item                                                                     |
| No GPU information                                            | ROADMAP item                                                                     |
| Messages suggest root/admin                                   | Linux EPERM text and the "restricted" hint                                       |
| `.desktop` category is only `Utility`                         | Tauri maps `category` to a single generic category                               |
| Release bundle built on `ubuntu-24.04`                        | glibc 2.39 baseline excludes Debian 12 / Ubuntu 22.04                            |

## 3. Windows-specific components

`platform/windows/*` (Toolhelp/NtQuerySystemInformation snapshots,
`IsProcessCritical`, `WM_CLOSE` terminate, thread suspension, PDH-free disk
I/O via `IOCTL_DISK_PERFORMANCE`, account lookup), NSIS/MSI bundling. None of
it changes in this pass.

## 4. Shared components

Engine, collector, rate maths (`monitoring/rates.rs`: real elapsed time,
counter resets yield 0), history ring buffers, alerts, settings, export,
protection rules, every React page.

## 5. Linux backend architecture

Keep the existing layout. New Linux code goes in
`core/src/platform/linux/`:

- `system.rs` — os-release `PRETTY_NAME`, desktop, display server,
  container, systemd detection (read once at startup).
- `net.rs` — interface classification from `/sys/class/net`.
- `gpu.rs` — DRM adapters from `/sys/class/drm`, optional live metrics.

Pure parsers/classifiers are plain functions with fixture tests; readers are
thin wrappers that return `Option`/empty on any I/O error.

No trait objects are added: each concern has exactly one implementation per
OS selected at compile time, which the existing free-function facade
already expresses without a vtable. A `ProcessController` trait would wrap
the same five functions with no second implementation to swap in.

## 6. Required OS packages (build)

Tauri 2 prerequisites, verified available on Debian forky:
`libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev
libayatana-appindicator3-dev librsvg2-dev`. Nothing else: no udev, DRM or
sensors library is linked.

## 7. Required Rust dependencies

None new. `sysinfo` gains its `component` feature (temperatures; on Linux it
reads `/sys/class/hwmon`). `libc` already covers signals and `setpriority`.
`procfs`, `nix`, `zbus`, `udev`, `wgpu` and NVML are **not** added — see the
report for why each was unnecessary.

## 8. Process monitoring

sysinfo for enumeration, CPU, memory, user, exe, command line, start time;
`/proc/[pid]/stat` for threads and nice (already). Add a `kernelThread` flag
(sysinfo `ThreadKind::Kernel`) and hide kernel threads by default with a
setting. Add `DiskSleep` and `Tracing` states. Disappearing PIDs are already
skipped silently.

## 9. CPU

sysinfo usage/per-core/frequency; `/proc/loadavg` via sysinfo. Already shown
separately from utilisation. No change.

## 10. Memory

sysinfo `MemAvailable`-based available/used; `/proc/meminfo` cached and
buffers. No change.

## 11. Disk

sysinfo `Disks` for mounts (30 s) and capacity/bytes (every tick);
`/proc/diskstats` for IOPS and busy time. Extend the pseudo-FS list; dedupe
devices in history totals.

## 12. Network

sysinfo `Networks` deltas over real elapsed time (counter resets yield 0).
Add `kind` (Ethernet, Wi-Fi, VPN, Virtual, Loopback, Unknown) from sysfs
`type`, `wireless/`, `tun_flags`, `bridge/`, `device` and `uevent DEVTYPE`,
not from names. Totals and history use physical + unknown only. Virtual
adapters are hidden by default with a setting.

## 13. GPU

Read-only DRM sysfs provider: vendor/device IDs, driver, PCI slot; name
from the system `pci.ids` database when present. Live metrics only where the
kernel exposes them (`gpu_busy_percent`, `mem_info_vram_*` on amdgpu;
`gt_act_freq_mhz` on i915; `hwmon` temperature). Everything else is
"Unavailable". NVML is out of scope.

## 14. Sensors

sysinfo `Components` (hwmon labels, e.g. "coretemp Package id 0"),
refreshed every 2 s. No fan or voltage control, ever.

## 15. Process control

Already correct: `SIGTERM`, `SIGKILL`, `SIGSTOP`, `SIGCONT`, `setpriority`;
ESRCH → not found, EPERM/EACCES → permission denied. Add zombie protection.

## 16. systemd

Detect it (`/run/systemd/system`, the `sd_booted()` check) and show it. A
Services page does not exist on Windows either; it is future work.

## 17. Wayland / X11

No display-server-specific code exists or is added. Report the session type.
Test the app under the native KDE Wayland session and under XWayland
(`GDK_BACKEND=x11`).

## 18. Benchmarks

The repository contained **no** benchmark code (CPU, GPU, disk or internet), so
there was nothing to port and building a benchmark suite was kept out of this
pass. A shared CPU benchmark was added afterwards; see
[`BENCHMARKS.md`](BENCHMARKS.md) and
[`LINUX_BENCHMARK_IMPLEMENTATION.md`](LINUX_BENCHMARK_IMPLEMENTATION.md).

## 19. Permissions

Run as a normal user. Never suggest sudo. EPERM/EACCES surface as
"Permission denied". Other users' `/proc/[pid]/{exe,cwd,fd}` failures are
expected and shown as unavailable.

## 20. Packaging

`.deb` and AppImage via `tauri build`; RPM stays since CI already installs
`rpm`. Add a desktop-entry template with `Categories=System;Monitor;`.
Build release bundles on Ubuntu 22.04 (oldest supported glibc with
WebKitGTK 4.1).

## 21. Testing

Fixture tests for os-release, container detection, interface
classification, DRM parsing, pci.ids lookup, new state mapping and zombie
protection. Existing live tests already spawn a disposable `sleep` child for
SIGSTOP/SIGCONT/SIGTERM/SIGKILL.

## 22. Performance

Measure with `cargo run -p tm-core --release --example probe`. Baseline on
this machine: 6.3 ms average cycle, 7.0 ms p95, 143 KiB per tick at 349
processes. New reads are static (once) or every 2 s.

## 23. Security

No shell plugin, no new capabilities, no new commands that take paths or
arbitrary input. Environments are never read. Command lines are never logged.

## 24. Phases

1. Linux static info (distro, desktop, display server, container, systemd).
2. Process states, kernel-thread flag, zombie protection, message wording.
3. Network classification and totals; disk pseudo-FS and dedupe.
4. Temperatures (sysinfo components) and GPU (DRM sysfs).
5. Frontend: settings toggles, network kind, Linux system card, GPU and
   sensor cards.
6. Packaging (desktop template, CI on 22.04), build `.deb`/AppImage,
   install and run.
7. Regression suite, docs, report.
