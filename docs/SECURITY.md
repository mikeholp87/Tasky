# Security model

Task Manager can end, suspend and reprioritize processes, so it treats process
control as privileged and treats the webview as untrusted input.

## Principles

1. **The backend enforces; the UI only explains.** Every safety rule runs in
   Rust on every request. A compromised or buggy frontend cannot skip them.
2. **No shell, no generic execution.** There is no "run command" endpoint.
   Process control uses native APIs (`TerminateProcess`, `SuspendThread`,
   `kill(2)`, `setpriority(2)`); no shell commands are constructed.
3. **Minimal Tauri permissions.** The capability file grants only
   `core:default`. Notifications, dialogs, autostart, window state and "open
   file location" are used from Rust only, so the frontend has no permission
   to call those plugins directly.
4. **The app runs unprivileged.** It never requests elevation. Operations the
   OS denies return a clear `PermissionDenied` explaining that administrator
   or root rights may be needed.
5. **Local only.** No network requests and no telemetry. All data stays on
   the machine.

## Command input validation

| Input                | Validation                                                                                                                           |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| PID                  | Accepted as `i64`; must be `1..=u32::MAX`, else `InvalidPid`.                                                                        |
| PID identity         | Actions carry the process start time the user saw. If the PID now belongs to a different process (PID reuse), the action is refused. |
| Refresh interval     | One of 500 / 1000 / 2000 / 5000 ms.                                                                                                  |
| History range        | 1–3600 s.                                                                                                                            |
| Settings             | Fully validated (`Settings::validate`); alert rules capped at 50, with bounded names, thresholds and durations.                      |
| File-finder path     | Trimmed, non-empty, ≤ 4096 bytes, no NUL. Resolved (symlinks) by the platform code; a missing path is `InvalidArgument`. Read-only.  |
| File / folder picker | Native dialog opened by Rust (`pick_path`); the webview gets the chosen path only, not a file-system permission.                     |
| Export destinations  | Chosen by the user in a native save dialog opened by Rust. The frontend never supplies a path or file contents.                      |
| "Open file location" | The path comes from the engine's own snapshot for the PID, never from the frontend.                                                  |

## Process protection

Protection rules (`core/src/process_control/protection.rs`) are pure and
unit-tested:

- **Blocked:** Task Manager itself; PID 0/4 (Windows kernel); PID 1 and
  kernel threads (Linux); processes Windows flags with `IsProcessCritical`;
  `csrss`, `wininit`, `smss`, `services`, `lsass`, `winlogon`, but **only
  when their path is verified** under `%SystemRoot%\System32`.
- **Warning (requires explicit acknowledgement):** critical names at
  unverified paths; `svchost`, `explorer`, `dwm`; root-owned core Linux
  daemons; root services started by init; raising priority to High.

Names alone never block. A process named `lsass.exe` in a Downloads folder
gets a warning, not special protection.

Dangerous actions always need confirmation in the UI. Force end uses stronger
wording, and the Delete key only opens the dialog.

## Inspection commands

Ports, open files, modules, memory maps, threads and the file finder are
read-only and run on a blocking worker only while their view is open. They
read what the operating system already exposes to the current account:
processes of other users that cannot be read are counted and reported, never
guessed. Results are capped (sockets 20,000, open files 5,000, regions 20,000,
file matches 2,000) and the path search stops after 4 seconds, with a
"results may be incomplete" notice. Paths, ports and connection endpoints are
shown locally and are not logged or exported; the activity timeline keeps
process names and PIDs only, in memory, for the last 1,000 events.

## Sensitive data

- Command lines can contain passwords and tokens. They are:
  - never logged;
  - excluded from CSV/JSON exports unless the user ticks "Include command
    lines", which comes with a warning;
  - excluded from diagnostics unless the user opts in twice (process list,
    then command lines).
- Process environments are never read.
- Logs contain process names and PIDs for actions taken, never arguments.
- CSV exports neutralize spreadsheet formula injection: process names are
  attacker-controlled, so values starting with `=`, `+`, `-` or `@` are
  prefixed with `'`.

## Content Security Policy

`default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src ipc: http://ipc.localhost`.
No remote scripts, fonts or images are loaded.

## Reporting

Please report vulnerabilities privately to the maintainers rather than in a
public issue.
