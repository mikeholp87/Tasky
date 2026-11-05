# Debian packages and APT updates

## Supported packages

The release workflow builds an `amd64` `.deb` on Ubuntu 22.04. The binary is
built on the glibc 2.35 baseline and is supported on Ubuntu 22.04 or newer and
Debian 12 or newer. Other architectures and older distributions are not
published. The Tauri bundler derives Debian runtime dependencies from the
binary's linked libraries; the workflow checks that GTK, WebKitGTK, and
AppIndicator dependencies are present in the package metadata.

Every `v<version>` tag matching `src-tauri/tauri.conf.json` builds a `.deb`,
validates and installs it in a clean Ubuntu 22.04 container, and attaches the
package and SHA-256 checksum to a GitHub Release. The same release is published
in the signed APT repository. The repository contains the package from the
most recently published release.

## Install from APT

Download the key and inspect its full fingerprint. Compare it with the
fingerprint announced by the project maintainers before trusting it:

```sh
curl -fsSLo /tmp/tasky-archive-keyring.asc \
  https://mikeholp87.github.io/Tasky/tasky-archive-keyring.asc
gpg --show-keys --with-fingerprint /tmp/tasky-archive-keyring.asc
curl -fsSLo /tmp/tasky-archive-keyring.fingerprint \
  https://mikeholp87.github.io/Tasky/tasky-archive-keyring.fingerprint
cat /tmp/tasky-archive-keyring.fingerprint
```

Then install the public key and add the repository:

```sh
sudo install -d -m 0755 /usr/share/keyrings
gpg --dearmor < /tmp/tasky-archive-keyring.asc \
  | sudo tee /usr/share/keyrings/tasky-archive-keyring.gpg >/dev/null
echo 'deb [arch=amd64 signed-by=/usr/share/keyrings/tasky-archive-keyring.gpg] https://mikeholp87.github.io/Tasky stable main' \
  | sudo tee /etc/apt/sources.list.d/tasky.list >/dev/null
sudo apt update
sudo apt install tasky
```

To update later, run `sudo apt update && sudo apt upgrade`. To remove Tasky,
run `sudo apt remove tasky` and optionally remove the source and keyring files.
Standalone `.deb` files and their checksums remain available from GitHub
Releases.

## Repository owner setup

1. In repository **Settings → Pages**, choose **GitHub Actions** as the build
   and deployment source.
2. Create an Actions environment named `apt-signing`. Restrict deployments to
   version tags matching `v*`; add required reviewers if available.
3. Generate a dedicated signing key on a trusted machine. Protect it with a
   passphrase and keep its private material out of the repository:

   ```sh
   gpg --quick-generate-key "Tasky APT Repository <apt@tasky>" rsa3072 sign 3y
   gpg --with-colons --fingerprint "apt@tasky" \
     | awk -F: '$1 == "fpr" { print $10; exit }'
   ```

   The `fpr` value is the full fingerprint. Store it as the environment
   variable `APTLY_KEY_ID` in `apt-signing`. The workflow also accepts it as an
   environment secret for compatibility with an existing setup.
4. Add this environment secret in `apt-signing`:
   - `APTLY_GPG_KEY`: output of
     `gpg --armor --export-secret-keys <FULL_FINGERPRINT>`.

   If the key has a passphrase, also add `APTLY_GPG_PASSPHRASE`. The workflow
   supports an unprotected, dedicated signing key when that secret is absent.
   - `APTLY_GPG_PASSPHRASE`: the key's passphrase.

   You can send the private key directly to GitHub without saving it to a file
   using GitHub CLI:

   ```sh
   gpg --armor --export-secret-keys <FULL_FINGERPRINT> \
     | gh secret set APTLY_GPG_KEY --env apt-signing
   ```

   Enter the passphrase as the `APTLY_GPG_PASSPHRASE` environment secret in
   the repository settings. Keep an offline backup of the key and its
   revocation certificate.
5. Commit the desired application version, push it, create the matching
   annotated tag (for example `git tag -a v0.1.0 -m 'Release v0.1.0'`), and
   push that tag. The release fails if the tag differs from the Tauri version.

The signing job imports the key only after the protected environment grants
access, verifies the complete fingerprint, builds the repository, and checks
both `InRelease` and `Release.gpg` signatures before uploading the Pages
artifact. A separate job deploys that verified artifact with only Pages
permissions.

## Local package checks

After building the Linux package with
`npx tauri build --bundles deb --config src-tauri/tauri.linux.conf.json`, run:

```sh
scripts/validate-deb.sh src-tauri/target/release/bundle/deb/Tasky_0.1.0_amd64.deb
```

The CI and release workflows also install and remove the package in a clean
Ubuntu 22.04 container to check dependency resolution and desktop integration.
