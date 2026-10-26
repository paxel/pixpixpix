#!/usr/bin/env sh
# Packs pixpixpix.app (Apple Silicon, unsigned: ad-hoc signed only) with
# libmpv and every library it needs from Homebrew bundled in
# Contents/Frameworks, and zips it for the cask.
#
#   packaging/macos/pack.sh <version> <dir with pixpixpix and pix3> <dist dir>
#
# Needs on the runner: brew install mpv dylibbundler librsvg
set -eu
version=$1
bins=$2
dist=$3
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$here/../..
brew_prefix=$(brew --prefix)
stage=$(mktemp -d)
app=$stage/pixpixpix.app
frameworks=$app/Contents/Frameworks
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$frameworks" "$dist"
cp "$bins/pixpixpix" "$bins/pix3" "$app/Contents/MacOS/"
chmod 0755 "$app/Contents/MacOS/pixpixpix" "$app/Contents/MacOS/pix3"
sed -e "s/{{VERSION}}/$version/g" "$here/Info.plist" > "$app/Contents/Info.plist"

# The icon: the ten sizes iconutil wants, drawn from the SVG.
iconset=$stage/pixpixpix.iconset
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
  double=$((size * 2))
  rsvg-convert -w $size -h $size "$root/assets/icon/icon.svg" -o "$iconset/icon_${size}x${size}.png"
  rsvg-convert -w $double -h $double "$root/assets/icon/icon.svg" -o "$iconset/icon_${size}x${size}@2x.png"
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/pixpixpix.icns"

# libmpv, found by the app at Contents/Frameworks/libmpv.2.dylib (src/mpv.rs),
# and everything it links, each pointing at the others next to itself.
cp -L "$brew_prefix/lib/libmpv.2.dylib" "$frameworks/libmpv.2.dylib"
chmod u+w "$frameworks/libmpv.2.dylib"
install_name_tool -id @loader_path/libmpv.2.dylib "$frameworks/libmpv.2.dylib"
# A library it cannot find makes it ask for a directory, forever on a closed
# stdin: "quit" fails the job instead.
printf 'quit\n' | dylibbundler -of -b -x "$frameworks/libmpv.2.dylib" -d "$frameworks" \
  -p @loader_path/ -s "$brew_prefix/lib"
# Proof of the bundling: nothing may still point into Homebrew.
leaks=0
for lib in "$frameworks"/*.dylib; do
  if otool -L "$lib" | tail -n +2 | grep -E "$brew_prefix|/usr/local/"; then
    echo "pack: $lib still links outside the app" >&2
    leaks=1
  fi
done
[ "$leaks" = 0 ]

# The bundled libmpv and ffmpeg make the app a GPL distribution.
cp "$root/LICENSE" "$app/Contents/Resources/LICENSE"
{
  echo "pixpixpix is MIT licensed, see LICENSE."
  echo
  echo "This app bundles libmpv and the libraries it links, from Homebrew, in"
  echo "Contents/Frameworks. libmpv and ffmpeg are distributed under the GPL;"
  echo "this app as a whole follows their terms. Sources:"
  echo "https://github.com/mpv-player/mpv and https://ffmpeg.org"
  echo
  echo "Bundled versions:"
  brew list --versions mpv ffmpeg
} > "$app/Contents/Resources/THIRD-PARTY.txt"

# Apple Silicon runs no unsigned code, and changed install names void a
# signature: ad-hoc sign the libraries, then the commands, the app last.
codesign --force --sign - "$frameworks"/*.dylib
codesign --force --sign - "$app/Contents/MacOS/pix3"
codesign --force --sign - "$app"
codesign --verify --strict "$app"

ditto -c -k --keepParent "$app" "$dist/pixpixpix-$version-macos-arm64-app.zip"
rm -rf "$stage"
ls -1 "$dist"
