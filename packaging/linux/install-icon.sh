#!/usr/bin/env sh
# Install the pixpixpix launcher entry and its icons into the current user's XDG
# directories, so the app appears in the application menu and in "Open with" for
# images and videos. It never becomes the default app: mimeapps.list is not touched.
# Idempotent; --uninstall takes them out again.
#
# Homebrew's own share directory is not on a desktop session's XDG_DATA_DIRS, so an
# entry installed there alone stays invisible to menus; this copies into
# ~/.local/share, which every session reads. On Wayland the compositor finds the
# window's icon through the entry named after its app id,
# io.github.paxel.pixpixpix.desktop. pixpixpix writes the same entry itself after
# its first frame.
#
# Usage:
#   install-icon.sh                launcher entry and icons
#   install-icon.sh --uninstall    remove them again
set -eu

mode=install
for argument in "$@"; do
  case $argument in
    -h | --help)
      sed -n '2,16p' "$0"
      exit 0
      ;;
    --uninstall)
      mode=uninstall
      ;;
    *)
      echo "install-icon: unknown argument $argument" >&2
      exit 2
      ;;
  esac
done

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
# Works both from the release tarball, where the icons sit next to this script, and
# from a source checkout (packaging/linux/../../assets/icon).
if [ -f "$here/icon_256.png" ]; then
  icons=$here
else
  icons=$here/../../assets/icon
fi
desktop_source="$here/io.github.paxel.pixpixpix.desktop"

data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
desktop_target="$data_home/applications/io.github.paxel.pixpixpix.desktop"
svg_target="$data_home/icons/hicolor/scalable/apps/pixpixpix.svg"
png_target="$data_home/icons/hicolor/256x256/apps/pixpixpix.png"

# Whether the entry is absent or carries pixpixpix's marker; anything else was
# written by someone else and is left alone.
ours() {
  [ ! -e "$desktop_target" ] || grep -q '^X-Pixpixpix-Managed=true$' "$desktop_target"
}

refresh_caches() {
  # Where the tools exist (harmless if they don't).
  command -v gtk-update-icon-cache >/dev/null 2>&1 &&
    gtk-update-icon-cache -f -t "$data_home/icons/hicolor" >/dev/null 2>&1 || true
  command -v update-desktop-database >/dev/null 2>&1 &&
    update-desktop-database "$data_home/applications" >/dev/null 2>&1 || true
  command -v kbuildsycoca6 >/dev/null 2>&1 && kbuildsycoca6 >/dev/null 2>&1 || true
}

if [ "$mode" = uninstall ]; then
  if ! ours; then
    echo "  kept $desktop_target (not written by pixpixpix)"
  elif [ -e "$desktop_target" ]; then
    rm -f "$desktop_target"
    echo "  removed $desktop_target"
  fi
  for target in "$svg_target" "$png_target"; do
    if [ -e "$target" ]; then
      rm -f "$target"
      echo "  removed $target"
    fi
  done
  refresh_caches
  echo "pixpixpix is out of your application menu."
  exit 0
fi

if ours; then
  mkdir -p "$(dirname "$desktop_target")"
  cp "$desktop_source" "$desktop_target"
  echo "  $desktop_target"
else
  echo "  kept $desktop_target (not written by pixpixpix)"
fi

# The 256 pixel PNG is what every desktop draws; the scalable icon is there for
# the ones that prefer it.
if [ -f "$icons/icon.svg" ]; then
  mkdir -p "$(dirname "$svg_target")"
  cp "$icons/icon.svg" "$svg_target"
  echo "  $svg_target"
fi
if [ -f "$icons/icon_256.png" ]; then
  mkdir -p "$(dirname "$png_target")"
  cp "$icons/icon_256.png" "$png_target"
  echo "  $png_target"
fi

refresh_caches

echo "pixpixpix is in your application menu and in \"Open with\". A window already"
echo "open keeps its old icon; start pixpixpix again to see the new one."
