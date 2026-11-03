# Security release checklist

- [ ] Freeze and record the exact source commit and working-tree changes.
- [ ] `cargo audit` clean/reviewed, including unsoundness/maintenance warnings.
- [ ] `cargo deny check advisories bans licenses sources` reviewed; dated exceptions still justified.
- [ ] `npm audit --package-lock-only --audit-level=moderate` reviewed.
- [ ] Tauri application command manifest and capabilities reviewed for every window.
- [ ] CSP and navigation allowlist reviewed; no production DevTools/debug server.
- [ ] No shell runner, broad filesystem scope or generic network command.
- [ ] Filesystem destinations, exclusive creation, permissions and cleanup reviewed.
- [ ] Network domains, TLS, redirects, budgets and privacy disclosure reviewed.
- [ ] If an updater is added, independently verify signatures and trusted HTTPS endpoints.
- [ ] Unsafe Rust/FFI inventory refreshed and platform invariants reviewed.
- [ ] Process identity/protection rules and denied-access behavior tested.
- [ ] Native consent verified for destructive operations and sensitive exports.
- [ ] Benchmark lock, limits, cancellation and failure cleanup tested.
- [ ] Any disk test uses an exclusive regular scratch file, free-space reserve and hard write budget.
- [ ] Secrets scan completed for source/untracked files and Git history; values never printed.
- [ ] Strict Rust/TypeScript/lint/format tests pass and generated types match.
- [ ] Windows release built and interactive controls tested on Windows.
- [ ] Linux production build/package and interactive controls tested unprivileged.
- [ ] Real GPU tests run on supported hardware without claiming fake-device tests validate drivers.
- [ ] Installers signed where supported; checksums generated and release provenance retained.
- [ ] Review SECURITY_AUDIT.md remaining risks; do not describe unresolved checks as passed.

The audit validation results are recorded separately. This reusable checklist
is deliberately unchecked: it must be completed for each release.
