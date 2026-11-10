# Security baseline — 2026-11-03

Recorded before audit fixes. Scope is the complete working tree, including
uncommitted GPU benchmark and Linux startup-manager work. Existing changes
were preserved; the starting diff was retained locally in `/tmp/tm-preaudit.diff`.

| Item                   | Baseline                                                                                      |
| ---------------------- | --------------------------------------------------------------------------------------------- |
| Application            | Task Manager / Tasky 0.1.0                                                                    |
| Commit                 | `5fd2546c8f420fbe7213eaef5666491c66cd00fb`, branch `linux-port`                               |
| Rust / Cargo           | 1.96.0 / 1.96.0; declared minimum 1.90, edition 2024                                          |
| Node / package manager | Node 24.16.0 / npm 12.0.1; committed package-lock.json                                        |
| Rust reproducibility   | Committed src-tauri/Cargo.lock, with existing GPU additions                                   |
| Supported platforms    | Windows and Linux; macOS backend is an unsupported stub                                       |
| Windows bindings       | windows 0.62; ToolHelp, IP Helper, Restart Manager, process/memory APIs                       |
| Linux bindings         | libc 0.2, sysinfo 0.40, procfs/sysfs, Vulkan via wgpu 30                                      |
| Frontend               | React 19, TypeScript 6, Vite 8, Zustand, Radix, uPlot, react-virtual; bundled fonts           |
| Tauri                  | 2.x; exact resolved versions recorded in SUPPLY_CHAIN_SECURITY.md                             |
| Plugins                | single-instance, notification, dialog, autostart, window-state; opener used as a Rust library |
| Capability             | default, main window only, core:default; automatic capability discovery                       |
| Application IPC        | 35 registered commands, initially outside an explicit application-command ACL                 |
| CSP                    | default-src self; inline styles; local/data images; ipc/http://ipc.localhost connections      |
| Network providers      | None; CPU/GPU benchmarks only, no Internet test or updater                                    |
| Privileges             | Normal user, current-user Windows installer, no elevation helper                              |

## Checks before fixes

Commands ran from the repository root or `src-tauri` as appropriate, with
`--locked` for Cargo dependency resolution.

| Check                                | Result                                                                     |
| ------------------------------------ | -------------------------------------------------------------------------- |
| npm run typecheck                    | Pass                                                                       |
| npm run lint                         | Pass                                                                       |
| npm test                             | Pass; exact counts retained in validation report                           |
| npm run build                        | Pass; existing large-chunk warning                                         |
| cargo fmt --all --check              | Pass                                                                       |
| cargo check --workspace --locked     | Pass                                                                       |
| cargo test --workspace --locked      | 285 core tests passed, 2 failed, 7 ignored                                 |
| Strict all-target/all-feature Clippy | Existing manual-is-multiple-of warning in GPU backend fails -D warnings    |
| Windows tm-core cross-check          | Pass, x86_64-pc-windows-msvc; no Windows runtime available                 |
| cargo audit / cargo deny             | Initially unavailable; installation and advisory checks tracked separately |
| npm audit                            | Initial sandbox DNS failure; retry tracked separately                      |
| gitleaks git --redact --no-banner    | 18 commits scanned; no leaks                                               |

The two initial core failures were real-machine inspection assertions in
`monitoring::disks::tests::samples_real_disks` and
`platform::linux::inspect::tests::finds_own_listening_socket` in the restricted
execution environment. They are not attributed to audit changes. Tests only
control their own disposable child processes. No root/administrator app run,
live external benchmark, credential access, or repository upload is authorized
or needed for this audit.

## Initial posture and review priorities

Positive controls already include typed process actions, native APIs, backend
protection rules, bounded monitoring history, a backend benchmark lock, native
save dialogs, optional command-line exports, and React text rendering.

Priorities found during initial source tracing: cached PID checks before raw
PID actions; predictable temporary save paths; unbounded local JSON reads;
application commands not explicitly scoped through Tauri ACL; diagnostic
alerts/logs carrying process identifiers even without process-list consent.
These are hypotheses here; confirmed findings and test outcomes belong in
SECURITY_AUDIT.md.
