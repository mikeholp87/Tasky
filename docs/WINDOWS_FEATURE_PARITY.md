# Windows feature parity with linux-port

## Latest feature port — 2026-11-08

Source: `linux-port` at `ae97bd6` (Services and drive power-off).
Applied to the local `windows-app` branch, retaining the Windows benchmark
implementation, memory diagnostics, startup policies and process-control audit
fixes.

| New source feature                         | Windows implementation                                                                                                                                                                                                                                                                                               |
| ------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Services page                              | Native Service Control Manager enumeration; search, state filters, startup policy, PID navigation, start/stop/restart and native confirmation. Critical, protected and unreadable service configurations cannot be controlled. Windows uses the System scope.                                                        |
| External-drive mount/unmount and power-off | Volume GUID identities, free drive-letter assignment, exclusive lock before dismount, and Windows PnP safe eject. USB storage and removable SD/MMC media only; every partition on the Windows system disk is excluded. Eject requires every volume on the physical disk to be unmounted and respects Windows vetoes. |
| Immediate storage updates                  | Refresh free space each tick and re-enumerate when the Windows drive-letter mask changes. Refresh benchmark targets when mounted drives change.                                                                                                                                                                      |
| Overview and process-table refinements     | Compact aligned resource cards, clearer details links, quieter row styling, improved executable/group labels, and Tasky branding. Windows resident-memory accounting remains accurately described.                                                                                                                   |
| Contextual process navigation              | Process Details appears in navigation only while open, and is omitted from global jump choices. Explicit process creation times remain attached to navigation.                                                                                                                                                       |
| Pressure accessibility                     | CPU/memory pressure is described in words as well as colour.                                                                                                                                                                                                                                                         |
| Reliability fixes                          | Independent alert episodes per process lifetime/mount, monotonic duration timing, ordered settings reset/save, serialized backend settings transactions, and OS autostart state readback.                                                                                                                            |

Services retain normal Windows access checks; Tasky does not elevate itself.
Drive-letter changes can require administrator rights. Busy drives are refused;
there is no force-unmount path. Windows offers **Eject**, since physical USB power
switching depends on the device. Actual service transitions and removable-drive
mount/unmount/eject require a manual hardware check; automated native tests only
enumerate resources and verify that protected services are refused.

Linux packaging/release workflows, GNOME window rules and the independent Linux
benchmark implementation are platform-specific and are not substituted into the
Windows app.

## Earlier diagnostics port

Compared on 2026-11-06 against
[`linux-port` at `7def4ae`](https://github.com/mikeholp87/Tasky/tree/7def4ae).
The GitHub repository was renamed from `task-manager` to `Tasky`.
This comparison includes the existing uncommitted Windows monitoring fixes and
memory diagnostics. Those changes and the Windows benchmark implementation are
preserved.

## Gaps identified and implemented

| Capability in linux-port       | Windows before this port                                       | Result                                                                                                                                    |
| ------------------------------ | -------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| Ports and port-owner lookup    | Per-process connections only                                   | Searchable TCP/UDP IPv4/IPv6 tables, listening/all views, owner details and confirmed process actions                                     |
| Find processes using a file    | Missing                                                        | Native file/folder picker and file users via Windows Restart Manager, with explicit limits                                                |
| Modules and memory map         | Missing                                                        | ToolHelp modules, committed regions via VirtualQueryEx, per-kind totals and searchable views                                              |
| Threads                        | Missing                                                        | ToolHelp thread list, CPU time and sampled CPU percentages, priority and creation time                                                    |
| Diagnose Process               | Missing                                                        | Observations from recent CPU, memory, I/O, thread and activity data, without asserting a deadlock or memory leak                          |
| Activity timeline              | Alerts only                                                    | Starts/exits, sustained CPU spikes, memory jumps, growth hints, alerts and successful process actions; bounded to 1,000 events            |
| Applications view              | Memory diagnostics already grouped apps; process table did not | Expandable executable groups with CPU/memory/I/O totals, keyboard navigation and new-process highlights                                   |
| Retained view preferences      | Partial                                                        | Restore page, process view/sort/filter, chart range and other view choices; process identities and sensitive search text are not restored |
| Rich System Information        | Basic static details                                           | Section navigation with summaries; machine/firmware, CPU caches/virtualization, power, GPU, sensor, network, storage and display sections |
| GPU and temperature monitoring | Benchmark probes only                                          | Optional live GPU identity/usage and exposed temperature sensors, refreshed on a bounded schedule                                         |
| Interface kinds                | Missing                                                        | Native IP Helper classification of Ethernet, Wi-Fi, VPN, virtual and loopback interfaces; avoid counting virtual traffic twice            |
| Startup management             | Own app's autostart only                                       | Current-user and all-user Run keys, 32-bit system Run keys, and Startup-folder entries, with reversible enable/disable and readback       |
| Resource visual improvements   | Basic charts                                                   | Resource colors, gradients, per-core strip and locally bundled IBM Plex fonts                                                             |

The existing CPU/GPU/storage/internet benchmark suite, history, exports,
preflight checks and benchmark mode remain in use. The Linux branch's independent
benchmark implementation is not substituted for it. Existing memory-growth,
commit-pressure and kernel-pool diagnostics remain on the Memory page.

## Windows-specific behavior and limits

- **File users:** Restart Manager identifies applications and services holding an
  individual file. It does not enumerate directory handles or all handle types.
  A folder search reports that limitation instead of claiming the folder is unused.
- **Open Files tab:** Full per-process handle enumeration remains unavailable on
  Windows, as it is on the source branch. The tab reports this explicitly. The
  implementation uses documented Win32 APIs for process inspection.
- **Threads:** Windows does not expose thread state, last processor or wait-channel
  data through the APIs used here. These fields are unavailable. CPU percentage
  requires two samples and is normalized to the whole machine.
- **Memory:** Windows groups use resident memory, which can count shared pages
  more than once. Linux can use PSS on demand. A group total is not presented as
  exclusive physical memory.
- **GPU memory:** The current library's Windows DXGI reading is per-process
  allocation, so it is not shown as device-wide VRAM usage. Unsupported sensors
  remain absent rather than showing invented zeroes.
- **Hardware:** Machine identity comes from the firmware registry data. Serial
  numbers and UUIDs are not collected. Battery chemistry, cycles, health and
  unavailable CPU frequency limits stay unknown. “Hypervisor detected” includes
  a Windows host running Hyper-V; it is not a claim that the host is a guest VM.
- **Startup:** Explorer's Windows-specific StartupApproved policy is used to
  preserve commands and shortcuts. Only recognized 12-byte states are changed;
  unknown formats are read-only. Entry IDs resolve against a fresh native list;
  the webview cannot supply registry paths or commands. Changing a system entry
  may require administrator rights. Scheduled tasks, services and packaged-app
  startup tasks are outside this list.
- **Privilege:** Protected processes can deny inspection. Denial and process-exit
  errors are shown. This app does not silently elevate or bypass OS permissions.
- **Persistence:** View preferences survive restarts. Metrics, process history
  and activity remain bounded in memory for the running session, matching the
  source branch; benchmark results retain their existing local persistence.

At the earlier source revision, services management, logged-in-user management, persistent application history,
thread control and full Windows handle enumeration were backlog items on
linux-port, not implemented features to port.

## Integration safeguards

Process inspections include PID and start time and validate the process identity
before and after native reads. Process-control protection and native lifetime
pinning are preserved. Inspection runs on bounded blocking workers, only while
its page/tab is open; polling stops when closed and pauses while hidden.
Activity backfills merge by event identity so a newer live event cannot erase
older events that arrive later. Context menus retain the selected process
instance across ticks. Startup writes require the backend's native confirmation
dialog and read back the saved state.

## Validation

Run from the repository root:

```powershell
npm.cmd run build
npm.cmd run lint
npm.cmd test
cargo fmt --all --check --manifest-path src-tauri/Cargo.toml
cargo clippy --workspace --all-targets --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo test --workspace --manifest-path src-tauri/Cargo.toml
npm.cmd run tauri -- build --no-bundle
```

The Windows-native tests exercise a loopback listener, this test process's
modules/memory/threads, Restart Manager with a temporary file held by the test,
hardware queries, read-only startup enumeration and policy encoding. UI tests
exercise port-to-process navigation, file picker/search/error handling, startup
save/readback, process-identity arguments, polling cleanup and late backfills.
No test changes an existing startup program or sends process-control actions to
a process it did not create.

Native API references:
[Restart Manager](https://learn.microsoft.com/windows/win32/rstmgr/restart-manager-portal),
[registry value reads](https://learn.microsoft.com/windows/win32/api/winreg/nf-winreg-reggetvaluew),
[system information functions](https://learn.microsoft.com/windows/win32/sysinfo/system-information-functions).
