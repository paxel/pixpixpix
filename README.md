# pixpixpix

<p>
  <a href="https://github.com/paxel/pixpixpix/actions/workflows/ci.yml"><img src="https://github.com/paxel/pixpixpix/actions/workflows/ci.yml/badge.svg?branch=summer" alt="CI"/></a>
  <a href="https://github.com/paxel/pixpixpix/actions/workflows/quality.yml"><img src="https://github.com/paxel/pixpixpix/actions/workflows/quality.yml/badge.svg?branch=summer" alt="Quality"/></a>
  <a href="https://github.com/paxel/pixpixpix/actions/workflows/security.yml"><img src="https://github.com/paxel/pixpixpix/actions/workflows/security.yml/badge.svg?branch=summer" alt="Security"/></a>
  <a href="https://sonarcloud.io/summary/new_code?id=paxel_pixpixpix"><img src="https://sonarcloud.io/api/project_badges/measure?project=paxel_pixpixpix&amp;metric=alert_status" alt="Quality Gate Status"/></a>
  <a href="https://scorecard.dev/viewer/?uri=github.com/paxel/pixpixpix"><img src="https://api.scorecard.dev/projects/github.com/paxel/pixpixpix/badge" alt="OpenSSF Scorecard"/></a>
  <a href="https://github.com/paxel/pixpixpix/blob/summer/scripts/coverage.sh"><img src="https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2Fpaxel%2Fpixpixpix%2Fsummer%2F.github%2Fbadges%2Fcoverage.json" alt="Coverage"/></a>
  <a href="https://github.com/paxel/pixpixpix/actions/workflows/fuzz.yml"><img src="https://github.com/paxel/pixpixpix/actions/workflows/fuzz.yml/badge.svg?branch=summer" alt="Fuzz"/></a>
  <a href="https://github.com/paxel/pixpixpix/blob/summer/Cargo.toml"><img src="https://img.shields.io/badge/unsafe-FFI%20only-yellowgreen" alt="unsafe: FFI only"/></a>
  <a href="https://github.com/paxel/pixpixpix/blob/summer/src/lib.rs"><img src="https://img.shields.io/badge/panic--free-linted-success" alt="panic-free"/></a>
  <a href="https://github.com/paxel/pixpixpix/releases/latest"><img src="https://img.shields.io/github/v/release/paxel/pixpixpix" alt="Release"/></a>
  <a href="https://paxel.github.io/pixpixpix/"><img src="https://img.shields.io/badge/docs-paxel.github.io-informational" alt="Documentation"/></a>
  <a href="https://paxel.github.io/pixpixpix/install.html"><img src="https://img.shields.io/badge/platforms-Linux%20%7C%20macOS-informational" alt="Platforms"/></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="License"/></a>
</p>

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

On macOS with Homebrew:

```sh
brew install --cask paxel/tap/pixpixpix
```

A Flatpak, release tarballs, building from source, and the system libraries a few
formats need are on the [Install](https://paxel.github.io/pixpixpix/install.html) page.

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

## The name

The name nods to the band Pigs Pigs Pigs Pigs Pigs Pigs Pigs
([Wikipedia](https://en.wikipedia.org/wiki/Pigs_Pigs_Pigs_Pigs_Pigs_Pigs_Pigs),
[Spotify](https://open.spotify.com/artist/1F7QDWyZTLGzkyGLgFjEhU)).

## License

[MIT](LICENSE). libmpv, which pixpixpix loads at runtime when it is installed, comes
under its own licence (GPL or LGPL, depending on how it was built). The macOS app and
the Flatpak bundle include libmpv and are therefore GPL licensed.
