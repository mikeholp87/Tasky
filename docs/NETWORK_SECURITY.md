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

## Internet benchmark as built (2026-11-04)

Update to the sections above; the audit text is left as written.

- **Provider and endpoints.** One fixed provider, Cloudflare's public speed
  endpoints at `https://speed.cloudflare.com` (`/__down?bytes=N`, `/__up`). No
  frontend value reaches a URL, header, proxy or port; the plain-HTTP
  constructor exists only under `cfg(test)`.
- **Consent.** The command shows a native dialog naming the provider and host,
  the largest download and upload for the chosen profile, that no files or
  system information are sent, and that the server sees the public IP address,
  request timing and the client name. The backend sets the consent flag only
  after the dialog is accepted, and the coordinator refuses an internet start
  without it. The page also states this before the Run button.
- **Numeric budgets.** Per run: Quick 100 MB down / 30 MB up, Standard 250 / 80,
  Extended 600 / 200 (decimal). Per app session: 1 GB total, refused up front
  when the next run's maximum would exceed what is left. Cooldown: 120 s
  between starts. Request limits: 25 MB (download) and 10 MB (upload) per
  request, at most 1 GiB by the client's own hard cap; the response body read
  is bounded by the requested size plus a one-byte sentinel; a ping response
  is read to at most 1 KiB plus a sentinel.
- **Deadlines.** Connect 5 s, response headers 10 s, each request body 60 s
  (a per-chunk total, not an idle timeout), whole request 120 s. Cancellation
  is checked on every received chunk; on a stalled connection a worker blocked
  in a read is released only by its deadline, so a cancel can take up to that
  long to finish in that case.
- **Concurrency.** At most 8 download and 4 upload connections plus one latency
  connection, all inside the common benchmark lock (one benchmark at a time).
- **Rate limiting.** The provider limits repeated large transfers per address
  (HTTP 429, `Retry-After` of about an hour was observed). 403 and 429 are shown
  with the retry time; nothing retries automatically.
- **Not covered.** Whether the real provider's certificate chain is accepted on
  every platform was exercised on Linux only; certificate-verification failure
  paths are not unit-tested (only the library's defaults are relied on).
