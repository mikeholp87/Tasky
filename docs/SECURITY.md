# Application security model

Task Manager treats the WebView and all OS-provided text as untrusted. It runs
as the normal user and respects OS permission failures. Never run the GUI as
root/Administrator to make a feature work. No elevation broker, shell runner,
service-control command or arbitrary memory/file/HTTP command is provided.

## IPC and frontend

Only the local main window receives the explicitly selected `default`
capability. The application command ACL is declared in `build.rs` and
`permissions/main.toml`; plugin permissions alone do not scope application
commands. JavaScript gets only event subscribe/unsubscribe plus the reviewed
application commands. See TAURI_COMMAND_SECURITY.md for every command/plugin.

Production DevTools are disabled. Scripts/fonts are bundled; the CSP disallows
remote scripts, eval, frames, objects, forms and base-URL changes. Inline styles
remain necessary for dynamic charts and layout. Navigation is restricted to
bundled Tauri origins (plus localhost:1420 in debug builds). OS strings render
as React text, never executable HTML. Clipboard writes require explicit UI
actions. No sensitive data is persisted in browser storage.

## Process controls

Every action is a Rust enum; priority maps to fixed portable values, with no
real-time class or arbitrary nice input. The backend validates positive PIDs,
selection start time, current OS creation time, self/system protection and
warning acknowledgement. A native dialog independently confirms disruptive
and warned actions. A WebView boolean cannot replace that dialog.

Linux signals use a lifetime-bound pidfd and fail closed when unavailable.
Windows force-kill/priority act through the creation-time-validated handle;
critical-process queries must succeed for disruptive actions. Thread suspend/
resume validates the opened thread's owner and retains handles for rollback.
The UI also explains unsaved-work and suspension risks.

Residual limitations: selected start times currently have one-second
resolution; Linux setpriority uses a numeric PID; Windows WM_CLOSE has an HWND
reuse interval, and thread enumeration is not atomic. These are documented
in SECURITY_AUDIT.md rather than described as fully race-free.

## Files and local history

Settings/history/startup saves and exports use exclusive random private temp
files beside the destination, flush before atomic replacement, and do not
truncate an existing symlink target. Newly created Unix directories are 0700
and files 0600. Parent directories must be trustworthy: same-user directory
replacement and Windows inherited ACLs require deployment review. Exports use
a backend-owned native save dialog; the frontend cannot supply a destination
or arbitrary contents. Intentional replacement of a selected file remains
possible through that native dialog.

Local JSON and desktop-entry reads reject special files/symlinks and enforce
byte limits while reading. Settings are capped at 256 KiB; benchmark history
at 8 MiB/200 results. Corrupt JSON is kept aside when possible. History updates
serialize disk and memory changes under one mutex. Monitoring/control queues,
on-demand scans, per-process histories and total log output have hard bounds.
Logs rotate daily, retain at most seven files and stop at 56 MiB total until
rotation frees space; the application log directory is user-private on Unix.

## Privacy

Process command lines, working directories, usernames, paths, destinations,
hostname and alert filters may be sensitive. They stay local by default.
Environments, credentials, browser profiles and process memory are not
collected. Process action logs omit process names and command lines.

Default diagnostics exclude processes, hostname, alert rules, alerts and recent
logs. Enabling process information explicitly includes these identifiers;
command lines need a second opt-in. Review exports before sharing: opted-in
command lines can contain secrets, and redaction is not a guarantee. Machine
summary export includes machine/volume/interface identifiers but no process
list. Nothing is uploaded automatically.

CSV fields are quoted correctly and prefixed with an apostrophe for formula
markers, control-leading fields, or whitespace followed by a formula marker.
JSON uses serde serialization. There is no executable HTML report export or
process-dump feature.

## Benchmarks and networking

CPU/GPU profiles are backend constants, not arbitrary frontend resource
settings. They share one backend lock; a finishing run cannot clear the next
run's lock. CPU workers are capped at 256. GPU profiles bound buffer sizes and
iterations; dispatches have deadlines and cancellation between dispatches.
Device/driver calls may still block beyond application control. Do not infer
real-device safety from fake-device tests.

No telemetry existed at the original audit baseline and none was added. The
Internet benchmark (one fixed provider, Cloudflare's speed endpoints) and the
disk benchmark were completed after the audit; their budgets, consent and
limits are recorded in NETWORK_SECURITY.md ("Internet benchmark as built") and
the checklist items still have to be completed for each release. OS domain/file-manager/mounted-filesystem activity is subject to OS
policy even when the application does not initiate HTTP traffic.

## Updates, release and reporting

No auto-updater is configured. Distributed artifacts still require trustworthy
signing/checksums and protected release credentials; a successful local bundle
is not signed provenance. Dependency policies, advisories and CI controls are
in SUPPLY_CHAIN_SECURITY.md. Remaining risks and exact validation status are
in SECURITY_AUDIT.md. For private reporting, see the repository SECURITY.md.
