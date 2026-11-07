#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 || ! -f $1 ]]; then
	echo "Usage: $0 PACKAGE.deb" >&2
	exit 2
fi

for command in dpkg-deb desktop-file-validate; do
	command -v "$command" >/dev/null || {
		echo "Missing required command: $command" >&2
		exit 1
	}
done

deb=$(realpath "$1")
[[ $(dpkg-deb -f "$deb" Package) == tasky ]]
[[ $(dpkg-deb -f "$deb" Architecture) == amd64 ]]

depends=$(dpkg-deb -f "$deb" Depends | tr ',' '\n' | sed -E 's/^[[:space:]]*//; s/[[:space:](].*$//')
for package in libayatana-appindicator3-1 libwebkit2gtk-4.1-0 libgtk-3-0; do
	grep -Fxq "$package" <<<"$depends" || {
		echo "Missing runtime dependency: $package" >&2
		exit 1
	}
done

root=$(mktemp -d)
trap 'rm -rf "$root"' EXIT
dpkg-deb --extract "$deb" "$root"
[[ -x $root/usr/bin/tasky ]]
[[ -s $root/usr/share/icons/hicolor/128x128/apps/tasky.png ]]
[[ -s $root/usr/share/man/man1/tasky.1 ]]
desktop=$root/usr/share/applications/Tasky.desktop
[[ -s $desktop ]]
desktop-file-validate "$desktop"
grep -Fxq 'Exec=tasky' "$desktop"
grep -Fxq 'Icon=tasky' "$desktop"
