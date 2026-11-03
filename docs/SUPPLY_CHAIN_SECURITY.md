# Supply-chain security

Audit: 2026-11-03. Application 0.1.0, initial source commit
`5fd2546c8f420fbe7213eaef5666491c66cd00fb` plus the working tree. See the baseline
for independently authored changes present before/during this audit.

## Dependency sources and native trust

Rust dependencies are locked in `src-tauri/Cargo.lock`; npm dependencies in
`package-lock.json`. Cargo sources are crates.io plus the local `tm-core` path.
No Git or alternate registry crate dependencies were found. npm package tarballs
resolve to the public npm registry with lockfile integrity hashes. The policy in
`deny.toml` rejects unknown Git/registry sources and unreviewed licenses.

Linux trusts system GTK3, WebKitGTK 4.1, GLib/GIO, desktop D-Bus services,
AppIndicator and native graphics drivers. Windows trusts WebView2, Win32 APIs,
graphics drivers, and installed desktop services. System libraries and drivers
are not fully represented by Cargo/npm audit results and need distribution/OS
updates. Vulkan/DX12 and WebViews are substantial native attack surfaces.

## Direct runtime Rust dependencies

| Crate                        | Resolved version | Role                                                            |
| ---------------------------- | ---------------- | --------------------------------------------------------------- |
| tauri                        | 2.12.5           | WebView / IPC / tray                                            |
| tm-core                      | 0.1.0            | Local platform/monitoring engine                                |
| serde                        | 1.0.229          | Typed serialization                                             |
| serde_json                   | 1.0.154          | JSON; byte limits enforced by callers                           |
| tracing                      | 0.1.44           | Local diagnostics                                               |
| tracing-subscriber           | 0.3.23           | Filtering and formatting                                        |
| tracing-appender             | 0.2.5            | Bounded asynchronous log writer and rotation                    |
| flate2                       | 1.1.10           | Local benchmark compression                                     |
| sha2                         | 0.11.1           | Benchmark hashing, not credential storage                       |
| sysinfo                      | 0.40.1           | OS metadata; no process environments requested                  |
| thiserror                    | 2.0.21           | Structured IPC-safe errors                                      |
| tempfile                     | 3.27.0           | Exclusive private random temporary files and atomic replacement |
| wgpu                         | 30.0.1           | GPU driver/device abstraction                                   |
| pollster                     | 1.0.1            | Wait for GPU futures                                            |
| windows                      | 0.62.2           | Typed Win32 FFI                                                 |
| libc                         | 0.2.191          | Unix/pidfd/affinity FFI                                         |
| tauri-plugin-autostart       | 2.7.0            | See TAURI_COMMAND_SECURITY.md plugin table                      |
| tauri-plugin-dialog          | 2.8.4            | See TAURI_COMMAND_SECURITY.md plugin table                      |
| tauri-plugin-fs              | 2.7.1            | See TAURI_COMMAND_SECURITY.md plugin table                      |
| tauri-plugin-notification    | 2.8.0            | See TAURI_COMMAND_SECURITY.md plugin table                      |
| tauri-plugin-opener          | 2.7.2            | See TAURI_COMMAND_SECURITY.md plugin table                      |
| tauri-plugin-single-instance | 2.5.5            | See TAURI_COMMAND_SECURITY.md plugin table                      |
| tauri-plugin-window-state    | 2.5.0            | See TAURI_COMMAND_SECURITY.md plugin table                      |

Build/test tooling includes tauri-build, ts-rs and naga. The full transitive graph is reproducible from the lockfile.

## Frontend dependencies

| Package                     | Locked version | Scope            |
| --------------------------- | -------------- | ---------------- |
| @fontsource/ibm-plex-mono   | 5.3.0          | Runtime          |
| @fontsource/ibm-plex-sans   | 5.3.0          | Runtime          |
| @tanstack/react-virtual     | 3.14.25        | Runtime          |
| @tauri-apps/api             | 2.12.4         | Runtime          |
| clsx                        | 2.1.1          | Runtime          |
| lucide-react                | 1.66.0         | Runtime          |
| radix-ui                    | 1.8.1          | Runtime          |
| react                       | 19.3.1         | Runtime          |
| react-dom                   | 19.3.1         | Runtime          |
| tailwind-merge              | 3.7.0          | Runtime          |
| uplot                       | 1.6.32         | Runtime          |
| zustand                     | 5.0.16         | Runtime          |
| @eslint/js                  | 10.0.1         | Development/test |
| @tailwindcss/vite           | 4.3.3          | Development/test |
| @tauri-apps/cli             | 2.12.4         | Development/test |
| @testing-library/jest-dom   | 7.0.1          | Development/test |
| @testing-library/react      | 16.3.3         | Development/test |
| @testing-library/user-event | 14.6.8         | Development/test |
| @types/node                 | 26.6.6         | Development/test |
| @types/react                | 19.3.0         | Development/test |
| @types/react-dom            | 19.3.0         | Development/test |
| @vitejs/plugin-react        | 6.1.2          | Development/test |
| eslint                      | 10.14.0        | Development/test |
| eslint-plugin-react-hooks   | 7.1.1          | Development/test |
| eslint-plugin-react-refresh | 0.5.7          | Development/test |
| globals                     | 17.14.0        | Development/test |
| jsdom                       | 30.1.3         | Development/test |
| prettier                    | 3.9.10         | Development/test |
| tailwindcss                 | 4.3.3          | Development/test |
| typescript                  | 6.0.3          | Development/test |
| typescript-eslint           | 8.72.1         | Development/test |
| vite                        | 8.3.5          | Development/test |
| vitest                      | 5.0.5          | Development/test |

## Advisory review and targeted changes

- Initial npm audit: 12 moderate entries, all one transitive root cause,
  `minimatch` 10.2.6 through ESLint/TypeScript-ESLint. No application runtime
  path used minimatch. [GHSA-g3hv-75wg-m249](https://github.com/advisories/GHSA-g3hv-75wg-m249)
  describes untrusted glob amplification and patches it in 11.0.1. A targeted
  `minimatch` override updates that dependency. Its Node engine supports Node
  24; typecheck, lint, tests and build validate the affected toolchain. Do not
  downgrade ESLint via npm's suggested `audit fix --force`.
- Initial RustSec audit: zero vulnerability-list entries, **not zero concerns**.
  It reports glib 0.18.5 unsoundness, four maintenance advisories, and yanked
  async-executor 1.14.0. The latter was updated compatibly to 1.14.1.
- [RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429.html), glib
  `VariantStrIter`: **retained, unresolved** through Tauri's GTK3 stack. No
  direct application call to the affected iterator was found; that does not
  prove dependency-level unreachability. glib >=0.20 is patched but is not a
  compatible drop-in replacement for the GTK3 binding graph. Do not claim this
  warning is fixed or add a global ignore for it.
- RUSTSEC-2024-0370: proc-macro-error 1.0.4 is unmaintained through GTK3
  procedural macros. Review on Tauri/GTK migration; no application exploit
  demonstrated.
- RUSTSEC-2026-0370/0371/0372: zvariant, zbus_names and zvariant_derive are
  merged into zbus 6, while installed Tauri plugins use zbus 5. They have
  narrowly documented maintenance exceptions in cargo-deny. No RustSec
  vulnerability was inferred merely from maintenance status.

`cargo-deny` initially ran with default policy (no repository policy existed):
license checks rejected everything and maintenance advisories failed the check.
The added license policy covers actual permissive/Unicode licenses plus
MPL-2.0 dependencies; MPL notices/source obligations still apply to distribution.
Duplicate versions are warnings, not automatically removed: GTK/Tauri, wgpu,
macro and hashing stacks require different compatible major versions.

## CI and release integrity

CI uses immutable GitHub Action commits, read-only `contents` permission and
checkout without persisted credentials. PR validation uses `pull_request`, not
`pull_request_target`; it does not receive signing secrets. The separate
security workflow checks npm, RustSec and cargo-deny on PRs/main, weekly, and on
manual request. Advisory outages can fail that dedicated workflow; ordinary
local builds remain offline-capable and do not depend on advisory availability.
Audit tool versions are pinned to cargo-audit 0.22.2 and cargo-deny 0.20.2.

The existing bundle workflow produces artifacts; it is not a signed-release
pipeline. No updater is installed, so there is no update signing key or endpoint
to validate. Before public distribution: Windows signing, Linux package/release
verification, SHA-256 checksums, dependency notices, artifact provenance and a
protected maintainer release workflow are still required. Never give PR code
release signing credentials.

## Graph summary

The reviewed Cargo lockfile contains 555 package records. 38 crate names occur at multiple versions. Full graph: `cargo tree --locked`.

The concurrently added ureq/rustls/ring graph introduces a native C/assembly build
requirement on Windows. Final Linux-host MSVC cross-checks cannot complete without
`lib.exe`/the Windows toolchain; earlier core checks before this addition passed.
This is recorded as a validation gap, not worked around by disabling TLS.

The new TLS root-certificate dataset, webpki-roots 1.0.11, uses
CDLA-Permissive-2.0. Its sharing condition requires the agreement text to
accompany the data. A version-specific cargo-deny license exception was reviewed;
the license is included in `src-tauri/licenses/` and bundled as a resource.
This is not a global allow for unrelated data-licensed dependencies.
