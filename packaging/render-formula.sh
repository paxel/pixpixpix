#!/usr/bin/env sh
# Renders the Linux Homebrew formula from its template, with the checksums of the
# built Linux tarballs.
#
#   packaging/render-formula.sh <version> <dist dir> <out dir>
set -eu
version=$1
dist=$2
out=$3
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
sha() {
  [ -f "$dist/$1" ] || { echo "render-formula: missing $dist/$1" >&2; exit 1; }
  sha256sum "$dist/$1" | cut -d' ' -f1
}
# Assigned first, so a missing tarball stops the script.
aarch64=$(sha "pixpixpix-$version-linux-aarch64.tar.gz")
x86_64=$(sha "pixpixpix-$version-linux-x86_64.tar.gz")
mkdir -p "$out"
sed -e "s/{{VERSION}}/$version/g" \
    -e "s/{{SHA_LINUX_AARCH64}}/$aarch64/" \
    -e "s/{{SHA_LINUX_X86_64}}/$x86_64/" \
    "$here/homebrew/pixpixpix-linux.rb.tmpl" > "$out/pixpixpix.rb"
