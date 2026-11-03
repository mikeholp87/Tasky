# Network security

## Baseline inventory

At the audit baseline there is no application HTTP client, Internet benchmark,
telemetry, analytics, update checker, remote script, external font fetch,
production HTTP server, Unix command socket or custom named-pipe broker.
The lockfile contains reqwest through Tauri's **build/bundling** tooling; its
presence alone does not establish runtime HTTP traffic.

| Surface                                     | Behavior                                                                                    |
| ------------------------------------------- | ------------------------------------------------------------------------------------------- |
| React assets                                | Bundled JavaScript, CSS, fonts and images                                                   |
| Tauri IPC                                   | Local framework transport; no wildcard remote capability                                    |
| Vite                                        | Development-only localhost:1420; TAURI_DEV_HOST is an explicit developer override           |
| IP Helper / procfs                          | Read socket metadata; do not connect to destinations or reverse-resolve hostnames           |
| Notifications / autostart / single-instance | Native desktop integration; second-instance arguments are discarded                         |
| File finder and executable reveal           | Native OS path/file-manager APIs; these can touch mounted network filesystems               |
| Windows account resolution                  | OS SID/name lookup may consult domain infrastructure; this is not application telemetry     |
| Audit/build tooling                         | Public npm/crates registries, RustSec and GitHub Action metadata; no source upload required |

Windows file finding rejects direct UNC and device namespace paths before OS
resolution. Mapped drives, reparse points, desktop services and installed file
managers remain subject to OS policy. Therefore “local data by default” does
not promise that the operating system never sends network traffic.

## Prospective Internet benchmark boundary

Disk/Internet benchmark model additions appeared from another session during
this audit. They were absent from the initial executable implementation.
Before enabling any provider, require:

- Backend-controlled HTTPS endpoints selected by a provider enum, certificate
  verification enabled, redirects disabled or revalidated against an allowlist.
- Explicit native consent with provider, maximum transferred bytes, and a
  warning that the provider sees public IP, timing and client characteristics.
- Connection/request deadlines, streaming byte limits independent of Content-
  Length, an upload limit, cancellation, a cooldown and the common benchmark lock.
- No arbitrary frontend URL, proxy, headers, local-network endpoint, process
  information upload, or automatic startup test.

This document must be updated with the actual provider and numeric budgets
when the new feature is complete; a declaration in the model is not validation.

## Emerging client reviewed during the audit

The additional, currently unwired provider source uses ureq 3.5.1 with rustls
and the fixed `https://speed.cloudflare.com` endpoint. It sends generated test
bytes only, with `TaskManager-Benchmark` as user agent. No frontend URL or
header is accepted; the alternate HTTP constructor exists only under cfg(test).

Audit fixes disable redirects and reject non-2xx statuses, enforce HTTPS outside
tests, cap each upload/download request at 1 GiB, read at most requested download
bytes plus one oversize sentinel, and cap ping response consumption at 1 KiB plus
one sentinel without accumulating the body. Existing connection/body/global
request deadlines are 5/15/120 seconds; ureq bounds response headers to 64 KiB.
Three isolated loopback regression tests pass for redirects, oversized response
bodies and oversized requests. No Cloudflare request was made during validation.

This is not an end-to-end Internet benchmark validation: total session budget,
transfer concurrency, cooldown, cancellation while blocked, provider disclosure,
result integrity and native start authorization must be checked when the runner
and IPC wiring exist. The coordinator currently refuses Disk/Internet starts.
