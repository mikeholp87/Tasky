# Tauri command security inventory

Baseline and audit date: 2026-11-03. All commands below are registered in
`src-tauri/src/lib.rs`, declared in `build.rs` using `AppManifest::commands`,
and allowed only by `permissions/main.toml` / capability `default` for the
local `main` window. No remote origins are authorized. A new window receives
none of these application commands automatically.

All commands run with the current user's OS privileges; none elevates. Typed
Rust deserialization rejects unknown enum variants and malformed numeric types.
Read-only commands expose private machine/process information to the main UI.
On-demand blocking work is limited to eight queued/active operations, and native
confirmation is serialized. Monitoring control has a separate bounded queue.

| Command                   | Purpose; inputs → outputs                                  | Filesystem / process / network access                      | Destructive? Validation / consent                                                                                              |
| ------------------------- | ---------------------------------------------------------- | ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `get_static_info`         | Static identity; none → system facts                       | Cached hardware/OS identity; no network                    | No; backend snapshot                                                                                                           |
| `subscribe_metrics`       | Channel → latest tick and stream                           | Live process/OS metadata; per-WebView channel              | No; replaces prior subscription for the same label                                                                             |
| `get_history`             | seconds → chart points                                     | Bounded in-memory system history                           | No; 1–3600 seconds                                                                                                             |
| `get_monitoring_state`    | none → monitoring state                                    | Memory only                                                | No                                                                                                                             |
| `set_monitoring_state`    | paused, interval → state                                   | Persists refresh setting                                   | Changes preferences; allowed intervals only, bounded control queue                                                             |
| `refresh_now`             | none → result                                              | OS sampling                                                | No; refresh requests coalesced to at most one per 500 ms                                                                       |
| `set_memory_detail`       | enabled → void                                             | Optional read-only PSS sampling                            | No; boolean, cached/budgeted reads                                                                                             |
| `start_benchmark`         | kind, profile, adapter → status                            | CPU/GPU load and saved history at baseline                 | Heavy load; enums, adapter ID ≤64, native consent, one coordinator lock                                                        |
| `list_gpu_adapters`       | none → adapters                                            | Loads native graphics drivers                              | No; bounded worker pool                                                                                                        |
| `cancel_benchmark`        | none → void                                                | Cancels active workload                                    | Stops load; backend token, does not write a completed result                                                                   |
| `get_benchmark_status`    | none → status                                              | Memory only                                                | No                                                                                                                             |
| `get_benchmark_history`   | none → results                                             | Cached private JSON; ≤200 results                          | No; file read bounded to 8 MiB                                                                                                 |
| `delete_benchmark_result` | id → result                                                | Rewrites application history                               | Yes; bounded ID and native confirmation                                                                                        |
| `get_process_details`     | pid → row/details/history/connections                      | OS metadata and current working directory; no environments | No; positive u32 PID; fresh start-time match before combining data                                                             |
| `list_sockets`            | none → socket list                                         | Read OS socket tables; no active connection                | No; limits/time budget, inaccessible owners reported                                                                           |
| `list_startup_items`      | none → XDG entries                                         | Reads known user/system autostart directories              | No; Linux only, bounded regular .desktop files                                                                                 |
| `set_startup_enabled`     | id, enabled → result                                       | User override for a known XDG entry                        | Yes; filename-only ID ≤255, no traversal, native consent, atomic save                                                          |
| `get_open_files`          | pid → file metadata                                        | Reads /proc fd metadata; unsupported on Windows            | No; positive PID, ≤5000 results                                                                                                |
| `find_file_users`         | path → owners                                              | Canonical path and process ownership metadata; read-only   | No; ≤4096 bytes/no NUL, bounded scan; Windows rejects direct UNC/device namespaces                                             |
| `get_memory_map`          | pid → regions                                              | Read-only address-space metadata; no memory bytes          | No; positive PID, region/byte limits                                                                                           |
| `get_modules`             | pid → modules                                              | Read-only loaded-module metadata                           | No; positive PID and platform limits                                                                                           |
| `get_threads`             | pid → thread metadata                                      | Read-only OS thread snapshot and prior sample              | No thread control IPC; positive PID, start-time sampling identity                                                              |
| `get_activity`            | after sequence → events                                    | ≤1000 in-memory local events                               | No; sequence filters the existing bounded ring                                                                                 |
| `pick_path`               | directory flag → selected path or null                     | Native file/folder picker only                             | No; no arbitrary file content returned                                                                                         |
| `get_hardware_info`       | none → hardware facts                                      | Read sysfs/procfs; no serial numbers or UUIDs              | No; best-effort and optional fields                                                                                            |
| `assess_process_action`   | pid, action → protection assessment                        | Cached process facts and native critical-process query     | No; positive PID, action/priority enums                                                                                        |
| `perform_process_action`  | pid, start time, action, acknowledgement → outcome         | Native signal/handle operations                            | Yes; cached and live identity, self/system protection, native disruptive/warning consent; remaining races documented           |
| `reveal_executable`       | pid → result                                               | Native file manager; path from engine snapshot             | Launches OS file manager, no frontend URL/path accepted                                                                        |
| `get_settings`            | none → preferences                                         | Cached private settings                                    | No; local identifiers in alert filters are sensitive                                                                           |
| `update_settings`         | settings → saved preferences                               | Private JSON and own-executable autostart registration     | Changes settings; Rust ranges/count/key validation, OS authorization                                                           |
| `reset_settings`          | none → default settings                                    | Private JSON; disables own autostart if enabled            | Resets preferences; user UI confirmation, no arbitrary autostart executable                                                    |
| `get_recent_alerts`       | none → alert events                                        | Bounded local alert ring                                   | No; may contain process names                                                                                                  |
| `export_processes`        | format, include command lines → saved path/null            | Native save selection; backend-generated CSV/JSON          | User-approved write; format enum, private atomic save, 32 MiB output cap, formula guard                                        |
| `export_system_summary`   | none → saved path/null                                     | Native save selection; machine/volume/interface summary    | User-approved write; no per-process list, identifiers remain sensitive                                                         |
| `export_diagnostics`      | include processes, include command lines → saved path/null | Native save selection; minimized diagnostic JSON           | User-approved write; default excludes process list, hostname, alerts, logs and alert rules; command lines require both opt-ins |

## Capability and plugin table

| Capability | Window           | Permission                | Required?                             | Risk                                                   |
| ---------- | ---------------- | ------------------------- | ------------------------------------- | ------------------------------------------------------ |
| default    | main, local only | main-commands             | Yes; 35 reviewed application commands | Process metadata/control and user-approved persistence |
| default    | main, local only | core:event:allow-listen   | Yes; benchmark progress/results       | Read event payloads; backend emits to main only        |
| default    | main, local only | core:event:allow-unlisten | Yes; listener lifecycle               | Removes event listeners                                |
| removed    | main             | core:default              | No; replaced by two event permissions | Previously included unrelated core defaults            |

Explicit `app.security.capabilities = ["default"]` prevents accidental inclusion
of a newly added capability file. No shell, fs, HTTP, clipboard, global shortcut,
window creation or updater plugin permissions are granted to JavaScript.

| Plugin/library    | Backend purpose                                                       | Frontend plugin permissions      |
| ----------------- | --------------------------------------------------------------------- | -------------------------------- |
| dialog            | Native save/pick/confirmation                                         | None; app handlers call Rust API |
| notification      | Explicitly enabled alert notifications                                | None                             |
| autostart         | This app's trusted executable only                                    | None                             |
| window-state      | Restore/save main geometry                                            | None                             |
| single-instance   | Focus existing instance; ignore args/cwd                              | None                             |
| opener            | Rust-only reveal executable location; plugin not registered           | None                             |
| navigation-policy | Deny navigation outside bundled origins (debug allows localhost:1420) | No commands                      |

Application commands are not automatically covered merely by limiting plugin
permissions; the explicit build manifest is essential. This follows
[Tauri's documented command ACL behavior](https://tauri.app/security/capabilities/).
Tests compare registration, manifest and permission names to prevent drift.

The startup/system-information/benchmark files were also being changed by
another session. Re-run the inventory test whenever command registration changes.

Sensitive process/command-line exports additionally require native backend
consent; frontend inclusion flags alone cannot silently authorize that content.
The generated build ACL and capability JSON were inspected after the production
build and contain the same 35 application command names and local main scope.
