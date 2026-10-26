#!/bin/sh
# Installs pixpixpix for the current user from the Flatpak bundle of the latest
# GitHub release:
#
#   curl -fsSL https://raw.githubusercontent.com/paxel/pixpixpix/summer/packaging/flatpak/install.sh | sh
#
# Downloads the bundle for this machine (x86_64 or aarch64), adds the Flathub
# remote for the runtime only (--user, if it is not there yet), installs the
# bundle with --user and writes the pixpixpix and pix3 commands to
# ~/.local/bin. Running it again installs the latest release over the old one.
#
# Uninstall:
#   flatpak uninstall --user io.github.paxel.pixpixpix
#   rm ~/.local/bin/pixpixpix ~/.local/bin/pix3
set -eu

app=io.github.paxel.pixpixpix
releases=https://github.com/paxel/pixpixpix/releases
flathub=https://dl.flathub.org/repo/flathub.flatpakrepo
bin_dir=$HOME/.local/bin

fail() {
  echo "pixpixpix install: $*" >&2
  exit 1
}

[ "$(uname -s)" = Linux ] ||
  fail "the Flatpak is for Linux; on macOS use: brew install --cask paxel/tap/pixpixpix"
machine=$(uname -m)
case $machine in
  x86_64 | amd64) arch=x86_64 ;;
  aarch64 | arm64) arch=aarch64 ;;
  *) fail "there is no bundle for $machine, only for x86_64 and aarch64" ;;
esac
command -v flatpak >/dev/null 2>&1 ||
  fail "flatpak is not installed; install it first, see https://flatpak.org/setup/"
command -v curl >/dev/null 2>&1 || fail "curl is not installed"

# The latest release's page redirects to its tag.
latest=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "$releases/latest") ||
  fail "cannot reach $releases"
case $latest in
  */tag/v*) version=${latest##*/tag/v} ;;
  *) fail "no release found at $releases" ;;
esac
bundle=pixpixpix-$version-$arch.flatpak

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "Downloading $bundle"
curl -fSL --progress-bar -o "$tmp/$bundle" "$releases/download/v$version/$bundle" ||
  fail "cannot download $bundle"

flatpak remote-add --user --if-not-exists flathub "$flathub"
flatpak install --user -y --noninteractive --reinstall --bundle "$tmp/$bundle"

mkdir -p "$bin_dir"
for name in pixpixpix pix3; do
  printf '#!/bin/sh\nexec flatpak run %s "$@"\n' "$app" >"$bin_dir/$name"
  chmod 755 "$bin_dir/$name"
done

echo "pixpixpix $version is installed: run pixpixpix or pix3, or find it in the application menu."
case ":$PATH:" in
  *":$bin_dir:"*) ;;
  *) echo "Note: $bin_dir is not on your PATH; add it to use the pixpixpix and pix3 commands." ;;
esac
