# pixpixpix

[![CI](https://github.com/paxel/pixpixpix/actions/workflows/ci.yml/badge.svg?branch=summer)](https://github.com/paxel/pixpixpix/actions/workflows/ci.yml)
[![Security](https://github.com/paxel/pixpixpix/actions/workflows/security.yml/badge.svg?branch=summer)](https://github.com/paxel/pixpixpix/actions/workflows/security.yml)
[![CodeQL](https://github.com/paxel/pixpixpix/actions/workflows/codeql.yml/badge.svg?branch=summer)](https://github.com/paxel/pixpixpix/actions/workflows/codeql.yml)
[![OpenSSF Scorecard](https://api.scorecard.dev/projects/github.com/paxel/pixpixpix/badge)](https://scorecard.dev/viewer/?uri=github.com/paxel/pixpixpix)
[![Quality Gate Status](https://sonarcloud.io/api/project_badges/measure?project=paxel_pixpixpix&metric=alert_status)](https://sonarcloud.io/summary/new_code?id=paxel_pixpixpix)
[![Coverage](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2Fpaxel%2Fpixpixpix%2Fsummer%2F.github%2Fbadges%2Fcoverage-rust.json)](coverage-floors.toml)
[![Fuzz](https://github.com/paxel/pixpixpix/actions/workflows/fuzz.yml/badge.svg?branch=summer)](https://github.com/paxel/pixpixpix/actions/workflows/fuzz.yml)
[![Release](https://img.shields.io/github/v/release/paxel/pixpixpix)](https://github.com/paxel/pixpixpix/releases)
[![License](https://img.shields.io/badge/license-MIT-blue)](LICENSE)
[![panic-free](https://img.shields.io/badge/panic--free-linted-success)](Cargo.toml)
[![unsafe: FFI only](https://img.shields.io/badge/unsafe-FFI%20only-yellowgreen)](Cargo.toml)

[![Documentation](https://img.shields.io/badge/docs-paxel.github.io-informational)](https://paxel.github.io/pixpixpix/)
[![Platforms](https://img.shields.io/badge/platforms-Linux%20%7C%20macOS-informational)](https://paxel.github.io/pixpixpix/install.html)

A fast, keyboard-first image and video viewer for Linux and macOS.

`pixpixpix photo.jpg` (or the short `pix3 photo.jpg`) puts the picture on screen as
fast as possible; nothing else, not even the folder scan, runs before the first image
is shown. From there you step through the folder with the arrow keys, while the
neighbours are preloaded so the next image is already there. Videos, Live Photos, an
overview of every folder below, lossless turns, annotations, a trash you can undo:
all of it a key away, and every key shown in the window.

**[Read the documentation](https://paxel.github.io/pixpixpix/)**: install, every key,
formats, and what each release brought.

## Install

On Linux with Homebrew:

```sh
brew install paxel/tap/pixpixpix
```

Three optional libraries come from your distribution: libmpv for videos, libheif for
HEIC/HEIF and AVIF, libturbojpeg for lossless JPEG turns. Without them everything else
works.

```sh
# Fedora
sudo dnf install mpv-libs libheif turbojpeg
# Debian, Ubuntu
sudo apt install libmpv2 libheif1 libheif-plugin-libde265 libheif-plugin-dav1d libturbojpeg
# Arch
sudo pacman -S mpv libheif libjpeg-turbo
```

On macOS with Homebrew:

```sh
brew install --cask paxel/tap/pixpixpix
```

The cask brings libheif and jpeg-turbo with it and the app bundles libmpv, so there is
nothing else to install.

A Flatpak, release tarballs and building from source are on the
[Install](https://paxel.github.io/pixpixpix/install.html) page.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Tests that need libheif, libmpv or libturbojpeg check the library-present branch when
the library is installed and the clear error otherwise. CI installs all three on
Linux x86_64, Linux aarch64 and macOS arm64.

`PIXPIXPIX_TIMING=1` prints the time from launch to the first frame. `PIXPIXPIX_DEBUG=1`
shows what libmpv and its decoders print, which pixpixpix otherwise drops.

The documentation site lives in `site/` and is built by a small Node script (Node 22,
no dependencies); the screenshots are drawn by a test from generated sample pictures:

```sh
node tool/site/build.js --serve                   # build and preview at localhost:8080
node --test "tool/site/*.test.js"                 # test the site build
cargo test --test screenshots -- --ignored        # redo site/assets/screenshots
```

## Support

pixpixpix is free. If it is useful to you, you can
[support it on Ko-fi](https://ko-fi.com/paxel7).

## The name

The name nods to the band Pigs Pigs Pigs Pigs Pigs Pigs Pigs
([Wikipedia](https://en.wikipedia.org/wiki/Pigs_Pigs_Pigs_Pigs_Pigs_Pigs_Pigs),
[Spotify](https://open.spotify.com/artist/1F7QDWyZTLGzkyGLgFjEhU)).

## License

[MIT](LICENSE). libmpv, which pixpixpix loads at runtime when it is installed, comes
under its own licence (GPL or LGPL, depending on how it was built). The macOS app and
the Flatpak bundle include libmpv and are therefore GPL licensed.
