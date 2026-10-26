#!/usr/bin/env sh
# Renders the macOS Homebrew cask from its template, with the checksum of the
# zipped pixpixpix.app.
#
#   packaging/render-cask.sh <version> <dist dir> <out dir>
set -eu
version=$1
dist=$2
out=$3
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
zip=$dist/pixpixpix-$version-macos-arm64-app.zip
[ -f "$zip" ] || { echo "render-cask: missing $zip" >&2; exit 1; }
# Assigned first, so a failing checksum stops the script.
arm64=$(sha256sum "$zip" | cut -d' ' -f1)
mkdir -p "$out"
sed -e "s/{{VERSION}}/$version/g" \
    -e "s/{{SHA_MACOS_ARM64}}/$arm64/" \
    "$here/homebrew/pixpixpix.rb.tmpl" > "$out/pixpixpix-cask.rb"
