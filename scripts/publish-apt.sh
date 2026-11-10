#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 || ! -f $1 ]]; then
	echo "Usage: $0 PACKAGE.deb vVERSION OUTPUT_DIR" >&2
	exit 2
fi
: "${APTLY_KEY_ID:?Set APTLY_KEY_ID to the full signing-key fingerprint}"
: "${APTLY_PASSPHRASE_FILE:?Set APTLY_PASSPHRASE_FILE to the key passphrase file}"
temp=''
stage=''
cleanup() {
	[[ -z $temp ]] || rm -rf "$temp"
	[[ -z $stage ]] || rm -rf "$stage"
	rm -f "$APTLY_PASSPHRASE_FILE"
}
trap cleanup EXIT

for command in aptly gpg gpgv; do
	command -v "$command" >/dev/null || {
		echo "Missing required command: $command" >&2
		exit 1
	}
done

deb=$(realpath "$1")
tag=$2
output=$(realpath -m "$3")
version=${tag#v}
temp=$(mktemp -d)
parent=$(dirname "$output")
mkdir -p "$parent"
[[ ! -e $output ]] || {
	echo "Output directory already exists: $output" >&2
	exit 1
}
stage=$(mktemp -d "$parent/.tasky-apt.XXXXXX")

actual_fingerprint=$(gpg --batch --with-colons --list-secret-keys "$APTLY_KEY_ID" | awk -F: '$1 == "fpr" { print $10; exit }')
[[ $actual_fingerprint == "$APTLY_KEY_ID" ]]

aptly repo create -distribution=stable -component=main tasky
aptly repo add tasky "$deb"
aptly snapshot create "tasky-$version" from repo tasky
aptly publish snapshot -batch -gpg-key="$APTLY_KEY_ID" -passphrase-file="$APTLY_PASSPHRASE_FILE" -architectures=amd64 "tasky-$version"

cp -a "$HOME/.aptly/public/." "$stage/"
cp docs/apt-landing.html "$stage/index.html"
cp src-tauri/icons/128x128.png "$stage/tasky-icon.png"
mkdir "$stage/assets"
cp docs/landing/*.webp docs/landing/og.png "$stage/assets/"
gpg --armor --export "$APTLY_KEY_ID" >"$stage/tasky-archive-keyring.asc"
printf '%s\n' "$APTLY_KEY_ID" >"$stage/tasky-archive-keyring.fingerprint"
printf '%s\n' "$tag" >"$stage/latest"
gpg --batch --export "$APTLY_KEY_ID" >"$temp/archive-keyring.gpg"
test -s "$stage/dists/stable/InRelease"
test -s "$stage/dists/stable/Release.gpg"
test -s "$stage/index.html"
test -s "$stage/tasky-icon.png"
test -s "$stage/assets/overview-dark-1440.webp"
test -s "$stage/assets/processes-light-1440.webp"
test -s "$stage/assets/og.png"
gpgv --keyring "$temp/archive-keyring.gpg" "$stage/dists/stable/InRelease"
gpgv --keyring "$temp/archive-keyring.gpg" "$stage/dists/stable/Release.gpg" "$stage/dists/stable/Release"
mv "$stage" "$output"
