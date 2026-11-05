# Debian releases and signed APT repository

## Goal

Build and validate Tasky's Debian package on every version tag, attach the
package and checksum to a GitHub Release, and publish the same package through
a signed APT repository hosted on GitHub Pages. Users should be able to install
Tasky from APT and receive upgrades with normal `apt update` / `apt upgrade`.

## Existing behavior

Tauri already builds `.deb` packages. Linux bundle CI is currently manual,
targets Ubuntu 22.04, and uploads bundle artifacts. The Linux Tauri config
already sets the product name, app identifier, and binary name. The main Tauri
config defines Debian runtime dependencies and a desktop entry template.

## Design

Keep Tauri's Debian bundler as the package authority. Add a Debian packaging
check that inspects the built archive with `dpkg-deb`, verifies its control
dependencies, desktop file, icon, and application executable, then installs it
in a clean Ubuntu environment and checks the installed desktop entry. Preserve
the Ubuntu 22.04 build floor and existing runtime dependency metadata.

Add a tag-triggered release workflow for tags matching `v*`. It builds the
Linux `.deb`, validates the package, generates a SHA-256 checksum, and creates
or updates a GitHub Release containing both files. Workflow permissions stay
read-only by default and are elevated only on the release and publication jobs.
The workflow must reject tags whose version disagrees with the Tauri package
version.

Publish the package to a Debian repository generated with `aptly` and hosted
on GitHub Pages. The workflow imports the release `.deb`, signs the repository
metadata with a dedicated GPG signing key held as a protected GitHub Actions
environment secret, and deploys the generated static repository. Publish the
public key separately so installation instructions can use `signed-by=` with
a dedicated keyring. Do not use `apt-key` or check in private key material.

Document the repository URL, key fingerprint verification, install command,
supported Ubuntu/Debian architectures, release/tag process, and the one-time
GitHub Pages and signing-secret setup. Continue publishing standalone `.deb`
files on GitHub Releases.

## Failure and safety behavior

Fail the release if package inspection, runtime dependency checks, signing,
repository generation, checksum generation, or Pages deployment fails. Never
publish an unsigned `Release` file as trusted metadata. Protect the signing
environment so only authorized release tags can access the key. Keep package
build validation runnable on pull requests without release credentials.

## Verification

Use a locally generated `.deb` to validate control metadata, desktop entry,
installation, removal, and dependency resolution in a clean Ubuntu container.
Exercise repository generation and signature verification with a temporary
test key. Validate workflow syntax and permissions locally. The final GitHub
Pages deployment requires repository Pages configuration and a protected
signing secret; document any owner-side setup that cannot be done from source.
