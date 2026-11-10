# Security and safety audit — 2026-11-03

## Scope and assurance

Initial commit: `5fd2546c8f420fbe7213eaef5666491c66cd00fb`, branch `linux-port`,
including existing GPU/startup changes. The working tree and HEAD also changed
from another session during the audit. Existing work was preserved. The initial
diff is retained locally at `/tmp/tm-preaudit.diff`; this report distinguishes
initial failures, audit fixes, and subsequently added feature code.

The review covers repository architecture, every registered IPC handler,
capabilities/CSP/navigation, native process operations, inspection, persistence,
exports/logs, CPU/GPU orchestration, platform unsafe/FFI, dependencies and CI.
Repository-wide sink searches complement focused call-path review. Generated
and third-party binaries are not represented as fully source-audited. No prior
compatible audit ledger existed. A single review cannot prove absence of bugs.

No confirmed CRITICAL or HIGH exploit was identified. In particular, no
reachable shell runner, elevation broker, arbitrary file-content writer,
arbitrary process-memory command, unsafe updater, telemetry or arbitrary HTTP
IPC was found at baseline. This is an observed result, not a security guarantee.

Severity: CRITICAL = broad takeover; HIGH = meaningful defeat of an explicit
security boundary; MEDIUM = demonstrated limited safety/privacy/integrity
failure; LOW = narrow local failure/hardening; INFO = architecture observation
without an established exploitable boundary violation.

## Confirmed findings and fixes

### SEC-001 — Cached PID identity before live process control

- **Severity / platform / component:** MEDIUM; Linux/Windows; process_control and native controls.
- **Description / attack scenario:** A selected process can exit between monitoring polls.
  Comparing its start time only with the cached row does not validate the numeric PID
  subsequently used by kill/OpenProcess/thread enumeration.
- **Impact / root cause:** The wrong process can be affected after PID reuse; the
  checked object and OS operation were separate, cache-based decisions.
- **Fix:** Check the current creation time at the OS boundary. Linux signals go
  through an owned pidfd with no numeric fallback; Windows kill and priority
  operate through the verified handle. Query critical status on that same
  Windows handle. Check opened thread ownership and retain rollback handles.
  Read-only process details also reject a fresh/cached lifetime mismatch.
- **Validation:** Disposable-child lifetime mismatch test refuses action and
  verifies the child remains alive; child suspend/resume/kill and self-protection
  tests pass on Linux. Windows core and tests cross-compile under strict Clippy.
- **Status:** **Mitigated, residual risks remain.** One-second start-time
  granularity, Linux numeric setpriority, HWND reuse and non-atomic thread
  enumeration are not described as fully solved. Windows runtime validation pending.

### SEC-002 — Predictable save files and symlink-following truncation

- **Severity / platform / component:** MEDIUM; local persistence/startup/export; primarily Unix.
- **Description / attack scenario:** Settings/history/startup used predictable
  `.tmp` paths with truncating writes. An actor able to plant a link in the
  writable save directory could redirect that write. Export truncation also
  followed a replaced destination symlink.
- **Impact / root cause:** Unrelated files writable by the app account could be
  corrupted; there was no exclusive temporary creation or no-follow replacement.
  This does not grant additional OS privileges, and assumes attacker access to
  the save directory.
- **Fix:** Reuse tempfile for exclusive random adjacent temporary files,
  0600 Unix files/0700 new directories, flush then atomic replacement. Read
  application-owned files through a regular-file/no-follow bounded reader.
- **Validation:** Symlink save test preserves the unrelated target; startup
  regression plants the old predictable temporary symlink and verifies the target
  remains unchanged. Read/write/permissions, FIFO rejection, round-trip and
  failed-write tests pass on Linux; Windows implementation cross-compiles.
- **Status:** **Fixed for temporary/leaf-file substitution.** Trusted parent
  directories remain a prerequisite; parent-directory replacement by the same
  owning account is outside this fix. Windows ACL/reparse behavior needs runtime testing.

### SEC-003 — Finishing benchmark cleared another run's lock

- **Severity / platform / component:** MEDIUM; all; benchmark Coordinator.
- **Description / attack scenario:** Completion cleared `active` before event
  delivery, but an unconditional panic-cleanup guard cleared it again on drop.
  A new run could start between those two releases, losing its ownership lock.
- **Impact / root cause:** A later start could overlap heavy workloads despite
  the single-benchmark control. The cleanup guard did not distinguish normal
  handoff from abnormal exit.
- **Fix:** Disarm the guard while atomically releasing the completed run's lock.
- **Validation:** Deterministic regression blocks the first run during result
  delivery, starts a second, lets the first drop, and verifies a third is rejected.
- **Status:** **Fixed; regression passes.** CPU/GPU share this coordinator lock.

### SEC-004 — Diagnostics leaked indirect process identifiers without opt-in

- **Severity / platform / component:** MEDIUM; all; diagnostic export.
- **Description / attack scenario:** A user excluded the process list before
  sharing diagnostics, but recent alerts, log messages and alert rules could
  still contain application names or filters; hostname was also included.
- **Impact / root cause:** Privacy choice was applied only to the direct list,
  leaving indirect identifiers outside the same consent boundary.
- **Fix:** Central privacy filtering removes hostname, rules, alerts, recent
  logs and process rows by default. Command lines still require a second opt-in.
  UI wording names the sensitive categories. Action logs omit process names.
- **Validation:** Serialization regression seeds private hostname, log and rule
  text and verifies none survives default export, even with command-lines=true.
  Existing CSV/JSON command-line opt-in tests pass.
- **Status:** **Fixed; regression passes.** Opted-in exports remain sensitive;
  redaction is not a substitute for minimization or review before sharing.

### SEC-005 — Unbounded queued work and refresh amplification

- **Severity / platform / component:** MEDIUM; all; IPC workers/monitoring engine.
- **Description / attack scenario:** A compromised WebView could submit many
  expensive scans or refresh requests to unbounded queues.
- **Impact / root cause:** Excess memory use, worker/descriptor pressure and
  continuous native polling; backend resource authority depended on UI pacing.
- **Fix:** Reserve one of eight permits before queuing blocking command work;
  use a 16-entry nonblocking monitoring control queue and coalesce refreshes to
  at most one per 500 ms. Native confirmation is serialized. CPU workers capped
  at 256; process histories capped at 4096 live processes × 300 points.
  Linux socket ownership scans cap descriptors/time, maps have byte/region bounds,
  and file-finder budgets are checked inside descriptor/mapping loops.
- **Validation:** Permit exhaustion/release, 10,000 coalesced refresh requests,
  global history bound and extreme CPU topology tests pass. Existing cancel,
  panic, GPU fake-device, history and inspection tests exercise failure behavior.
- **Status:** **Fixed for the identified queues/buffers.** Synchronous kernel,
  remote-mounted-filesystem and GPU-driver calls cannot always be preempted.

### SEC-006 — Unbounded local-file load and loaded-history retention

- **Severity / platform / component:** LOW; all/Linux; settings/history/XDG parsing.
- **Description / attack scenario:** A malformed or externally edited local
  JSON/desktop file could be read fully before validation. Loaded benchmark
  history bypassed the retention limit enforced on new results.
- **Impact / root cause:** Local memory exhaustion or stalled special-file
  reads; limits were applied after allocation or only on writes.
- **Fix:** Reject non-regular/reparse/symlink files; bound reads while streaming
  (settings/desktop 256 KiB, benchmark history 8 MiB); retain at most 200 loaded
  results. Bound settings column keys/widths and startup enumeration.
- **Validation:** Exact/oversized read tests, FIFO/symlink rejection, oversized
  settings sanitization and loaded-history truncation tests pass.
- **Status:** **Fixed for reviewed local persistence.** Initial IPC JSON
  deserialization and sysinfo's own enumeration allocations are still framework/library trust.

### SEC-007 — Concurrent history saves lost updates

- **Severity / platform / component:** LOW; all; benchmark history/settings.
- **Description / attack scenario:** Concurrent history add/delete operations
  cloned a list, persisted it, then independently reacquired the lock to publish.
- **Impact / root cause:** New results or deletions could be lost or memory
  disagree with disk; the transaction did not hold one lock across both steps.
- **Fix:** Serialize persistence and in-memory publication under the same lock;
  settings persistence similarly holds its write lock through publication.
- **Validation:** Eight concurrent history additions all survive on disk and in
  memory, with restart round-trip and failed-save behavior retained.
- **Status:** **Fixed; regression passes.** Autostart side effects and settings
  persistence remain separate OS transactions; see residual risks.

### SEC-008 — Aligned buffer rounding could overflow

- **Severity / platform / component:** LOW; emerging disk implementation.
- **Description / attack scenario:** An extreme Rust-side requested length could
  overflow rounding before `Layout` rejected the allocation.
- **Impact / root cause:** Panic or invalid rounded size; arithmetic occurred
  before the allocation-layout validation. No frontend path to this input was
  present when found.
- **Fix:** Use checked rounding to the next alignment multiple.
- **Validation:** `usize::MAX`, `usize::MAX - 1` and excessive allocation tests
  return None; the newly added disk core tests pass on Linux.
- **Status:** **Fixed at this source boundary.** This does not establish the
  safety of the whole concurrently developed disk/Internet feature.

### SEC-009 — Unbounded/redirecting responses in emerging provider source

- **Severity / platform / component:** LOW pre-release source hardening; Internet provider.
- **Description / scenario:** The newly added provider followed redirects by
  default and accumulated a latency response using read_to_end. An enabled
  client could be redirected by a compromised provider or exhaust memory on
  an oversized ping response. The download loop also trusted provider length
  until the response ended.
- **Impact / root cause:** Potential off-provider requests, resource exhaustion
  and false successful latency samples. This source was not reachable through
  application IPC when found; no current-app remote exploit is claimed.
- **Fix:** Disable redirects; require non-2xx responses to fail; production
  HTTPS-only requests; 1 GiB per-request ceiling; bounded streaming ping and
  download bodies. Cancelled disk work also returns before creating a scratch file.
- **Validation:** Three isolated tests against disposable loopback servers
  verify redirects do not contact the destination, excess bodies are rejected,
  and oversized requests fail without a connection. No live provider contacted.
- **Status:** **Source fix validated in an isolated harness.** Future runner/IPC
  wiring, cumulative budgets and consent still require review before enablement.

## Dependency findings

| ID      | Severity                           | Finding / fix / validation / status                                                                                                                                                          |
| ------- | ---------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| DEP-001 | MEDIUM upstream, development scope | minimatch glob amplification caused 12 npm advisory entries; targeted 11.0.1 override; npm audit now reports zero; typecheck/lint/test/build must be revalidated after concurrent type edits |
| DEP-002 | INFO                               | yanked async-executor 1.14.0 → compatible 1.14.1; lockfile updated; Rust suite passes                                                                                                        |
| DEP-003 | Unresolved upstream unsoundness    | glib 0.18.5 RUSTSEC-2024-0429 remains through GTK3; no direct application use found; dependency reachability not proved; Tauri-compatible migration required                                 |
| DEP-004 | INFO maintenance                   | proc-macro-error and zbus-family maintenance advisories reviewed; narrow cargo-deny exceptions document upstream migration dependencies                                                      |

See SUPPLY_CHAIN_SECURITY.md for sources, roles, versions, licenses, duplicate
versions, evidence and advisory links. A zero RustSec vulnerability-list count
does not mean its unsoundness/maintenance warnings are absent.

## Additional hardening (not inflated into proven exploits)

- Explicit main-only application ACL and capability selection; removed broad
  core defaults in favor of event listen/unlisten. Command/permission drift tests.
- Restrictive CSP, no production DevTools, navigation allowlist, main-only
  benchmark event emission. React malicious-name rendering regression.
- Native consent for destructive/warned process controls, startup toggles,
  all benchmark starts and history deletion. Structured cancellation/busy
  errors prevent treating cancellation as an unsupported graceful-close failure.
- CSV formula guarding additionally covers newline and whitespace prefixes.
- User-private Unix log directory, seven-file retention and 56 MiB total output
  budget; disk-budget test. Existing logs are not automatically scrubbed.
- Windows query buffers/count arithmetic bounded; unaligned snapshot references
  removed; thread ownership/rollback handles improved; direct UNC/device file
  finder inputs rejected before lookup. These compile; native runtime remains pending.
- CI actions pinned to resolved SHAs, read-only tokens, no persisted checkout
  credentials, dedicated security workflow, reviewed cargo-deny policy.

## Remaining risks and release gates

1. **Windows runtime:** Native process controls, critical/protected processes,
   thread races, file ACLs/reparse points, Restart Manager and release WebView
   behavior require a real Windows runner. Core cross-compilation is not a
   full Windows Tauri build or native test pass.
2. **Lifetime precision:** UI identity uses Unix seconds. Same-second PID reuse
   cannot be distinguished perfectly. Linux priority still has a numeric-PID
   interval; Windows HWND/thread enumeration has non-atomic intervals. A future
   native high-resolution lifetime token and platform operation redesign are needed.
3. **Filesystem parents:** Atomic leaf replacement assumes trustworthy parent
   directories. A malicious process with the same account's directory authority
   can replace ancestors. Windows inherited ACLs need deployment verification.
4. **Native blocking/OOM:** Kernel I/O, FUSE/mapped drives, drivers and thread
   creation can stall or fail beyond cooperative cancellation. Release panic=abort
   means catch_unwind does not recover a production panic; CPU barriers/native
   driver workloads merit process isolation before stronger guarantees.
5. **GTK/glib:** Retained upstream unsoundness and maintenance advisories; no
   claim of dependency-complete safety. Patch the OS WebView/graphics stack too.
6. **Privacy/spoofing:** Process strings and notifications can contain control/
   bidi characters. React prevents HTML execution, not visual impersonation.
   Sensitive opted-in exports and explicit clipboard contents are unredacted.
7. **Settings transaction:** An autostart registration may succeed before a
   settings-file write fails. No privilege escalation, but settings/OS state can
   disagree. Full OS/persistence rollback is not implemented.
8. **Release integrity:** No updater exists; installer signing/checksum/provenance
   and actual platform release testing remain release-owner responsibilities.
9. **Concurrent feature work:** New disk/Internet model and disk core files
   appeared after baseline. They require a fresh stable-tree review of target
   identity, directory races, maximum total writes, network/provider budgets and
   consent before being enabled. A disabled coordinator arm is not an audit of
   a future enabled feature. Tests/builds can become stale while files change.

No secrets, source, diagnostics or local process data were uploaded. No real
process was controlled except disposable test children; no full-size disk,
CPU/GPU stress or external bandwidth benchmark was run by the audit.

## Validation record

See SECURITY_VALIDATION.md for exact commands, counts, advisory outcomes,
pre-existing sandbox failures, production-build status and unchecked runtime
requirements. That record is authoritative when later feature edits supersede
an earlier passing run. Do not mark the overall cross-platform release checklist
complete based on compile-time tests alone.
