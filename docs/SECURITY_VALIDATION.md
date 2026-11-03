# Security validation record — 2026-11-03

## Final stable-tree check

The final check hashed source/configuration/lockfiles before and after running
the commands below. **No source changes occurred during that validation run.**
Source hashes are retained in SECURITY_COVERAGE.json. This matters because
another session added disk/Internet code and changed HEAD during the audit.

| Command                                                                                         | Result                                                                                       |
| ----------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| `cargo fmt --all --check`                                                                       | Pass                                                                                         |
| `cargo check --workspace --all-features --locked`                                               | Pass                                                                                         |
| `cargo test --workspace --all-features --locked`                                                | Pass                                                                                         |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`                 | Pass                                                                                         |
| `cargo clippy -p tm-core --all-targets --target x86_64-pc-windows-msvc --locked -- -D warnings` | Blocked: new ring TLS dependency needs MSVC lib.exe, unavailable on this Linux host          |
| `npm run typecheck`                                                                             | Pass                                                                                         |
| `npm run lint`                                                                                  | Pass                                                                                         |
| `npm test`                                                                                      | Pass                                                                                         |
| `npm run format:check`                                                                          | Only regenerated unsafe-audit Markdown needed formatting; formatted and rechecked separately |
| `npm run build`                                                                                 | Pass                                                                                         |

Rust tests: **367 core tests and 11 application tests passed**, eight optional
hardware/measurement tests ignored. Frontend: **179 tests in 28 files passed**.
An additional isolated harness compiled the emerging provider source directly
and passed **three loopback security tests**. The Internet module was not yet
wired into the main crate's benchmark module when those tests ran; this is
explicitly separate evidence rather than a claim that the normal suite ran it.

Strict Clippy passed without adding warning suppressions. Existing generated
types retain ts-rs formatting; `git diff --check` reports generator whitespace
in BenchmarkResult.ts. Generated files are excluded by the existing Prettier
policy and were not hand-reformatted, which would conflict with regeneration.

## Production build and runtime

`npm run tauri -- build --bundles deb --config src-tauri/tauri.linux.conf.json`
passed and produced the Linux release executable and Debian package:

- `src-tauri/target/release/tasky`
- `src-tauri/target/release/bundle/deb/Tasky_0.1.0_amd64.deb`
- Adjacent `.deb.sha256` checksum; signing is not configured.

Debian artifact SHA-256: `ae8a425250fce2f9b367b36b211349f2688edace6c15ff7e297bac8c3b9828f2`.

The first production attempt failed while the other session was midway
through adding enum variants; the later build passed after those changes and
optional-field fixtures were integrated. The release build used the actual
production configuration, including restrictive CSP, disabled DevTools and
explicit capabilities. The newer Internet provider source remains unwired.

The release executable was launched as UID 1000 with an empty environment,
explicit temporary XDG data/config/cache/state paths and a 10-second timeout.
Core dumps were disabled. There is no graphical display in this environment:
GTK initialization aborted before a window could open (exit 134). Therefore
**interactive release behavior is unverified**, including native consent,
process-list/detail rendering, startup toggles, file/port views and exports.
No root GUI, existing-user settings or production credentials were used.

Windows: earlier strict all-target core cross-checks passed before the new
ureq/rustls/ring graph was added. Final MSVC cross-check is blocked by missing
`lib.exe`/native Windows build tools. No full Windows Tauri release build or
Windows runtime test was possible. CI retains a real Windows job; this report
does not substitute Linux cross-compilation for Windows testing.

## Dependency/security tools

- cargo-audit 0.22.2 and cargo-deny 0.20.2 installed under `/tmp` from public
  registry sources; no repository content uploaded.
- Initial npm audit: 12 moderate entries from minimatch; final npm audit: zero.
- RustSec: zero vulnerability-list entries; glib unsoundness and four maintenance
  warnings remain reviewed/unresolved. async-executor yanked version corrected.
- Final `cargo deny check advisories bans licenses sources`: all four pass with
  documented maintenance exceptions; duplicate-major warnings remain visible.
- Initial Git secrets scan: 18 commits, no leaks. Source/untracked scan: clean.
  A broad scan also examined build outputs and flagged five generated dependency
  metadata/schema artifacts; these are not confirmed application credentials.
  Do not include their values in reports. Final history scan: 19 commits, no leaks. The final source scan initially flagged two generated source-digest records in SECURITY_COVERAGE.json; these were verified as SHA-256 output, represented as explicit path/digest records, and re-scanned. No credential values were printed.

## Baseline failures and validation limits

Before fixes, frontend typecheck/lint/test/build and Cargo format/check passed.
Restricted execution failed two machine-dependent core tests (disk visibility
and opening an own loopback listener), and strict Clippy failed an existing GPU
modulo warning. The full suite passes with normal user OS visibility; no tests
were weakened to bypass those initial failures. The GPU lint was corrected.

No real bandwidth test, full-size disk write workload, real-GPU ignored test,
installer signing, Windows protected-process/UAC interaction, GUI resource
measurement or before/after performance comparison was performed. No speed or
monitoring-overhead improvement is claimed. Native drivers and blocking kernel
calls remain outside cooperative timeout guarantees.

## Checks after the stable-tree run

Sensitive export opt-ins now also require a native backend confirmation; a
new denied-file/directory regression verifies EACCES preserves prior data.
The complete Rust suite then passed with 367 core and 11 application tests,
and strict all-target/all-feature Clippy passed. The added Unix permission
literal was normalized to octal and its regression rerun successfully.

The root-certificate data license resource and its narrow policy exception
were then checked with all four cargo-deny checks, two configuration boundary
tests, and a successful final Linux release/deb rebuild. The generated Tauri
ACL manifest was inspected: exactly 35 main application commands, local-only
main capability, event listen/unlisten and no remote origins. The new CI shell
block passed shellcheck and shfmt checks.

These follow-up changes and final source hashes are recorded separately in
SECURITY_COVERAGE.json so the earlier stable-tree validation is not presented
as if it automatically tested later edits.

Final source/untracked scan: **440 files, zero findings**. Final Git-history
scan: **19 commits, zero findings**. The final Debian package was inspected
and contains the root-certificate license resource. Its adjacent checksum
matches the packaged artifact. Full repository Prettier validation passes
following the documented formatting-only corrections.
