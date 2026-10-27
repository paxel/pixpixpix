# pixpixpix

[![CI](https://github.com/paxel/pixpixpix/actions/workflows/ci.yml/badge.svg?branch=summer)](https://github.com/paxel/pixpixpix/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/paxel/pixpixpix)](https://github.com/paxel/pixpixpix/releases/latest)
[![License](https://img.shields.io/badge/license-MIT-blue)](LICENSE)

A fast, keyboard-first image and video viewer for Linux and macOS.

`pixpixpix photo.jpg` (or the short `pix3 photo.jpg`) puts the picture on screen as
fast as possible; nothing else, not even the folder scan, runs before the first image
is shown. Started on a folder or without an argument, it shows the folder's first
image. From there you step through the folder with the arrow keys, while the
neighbours are preloaded so the next image is already there.

> Status: early development; 0.1.0 is the first release.

## What it does

- **Browse.** Left and Right step through the folder in natural name order (`img2`
  before `img10`), with a band of the preloaded neighbours, the current file in its middle. Files other programs add,
  remove or rename show up on their own.
- **Move through folders.** A dir column lists the current folder among its
  siblings; moving through it moves the image view along.
- **Overview.** A scrollable gallery of the current folder and every folder below it,
  with thumbnails shared with other apps through the freedesktop thumbnail cache.
- **Act on the file.** Rotate (JPEG losslessly), move to the trash, copy to the
  clipboard, or open in another app.
- **Videos and moving photos.** Videos play through libmpv with seeking and frame
  stepping; Apple Live Photos and Google/Samsung Motion Photos play their moment.
  Animated GIF and WebP play on their own.
- **Annotate and export.** Arrows, text, outlines, pen, highlighter, pixelation and a
  free crop, plus an export of an area at an exact pixel size, all behind one save
  dialog that never overwrites by accident. Paused video frames can be annotated and
  saved as images too.
- **Metadata.** A panel with camera, lens, exposure, capture date and location; the
  capture date can be corrected in place, and the location or all EXIF can be
  removed.

The [CHANGELOG](CHANGELOG.md) describes every feature in detail.

## Keys

`Esc` always leads back to the image view, the main screen.

The bottom line of the window names the area that has the focus, which also shows a lit
diode in its bottom right corner, and lists its keys, on as many
lines as it needs; `?` hides them and brings them back. A click on a key in that line does what
the key does. Near the left or right edge of the image an arrow appears that steps to
the previous or next file.
On macOS, `Cmd` takes the place of `Ctrl` in every key below.

### Everywhere

| Key      | Action             |
|----------|--------------------|
| `s`      | settings           |
| `f`      | fullscreen         |
| `Ctrl+Q` | quit               |
| `Tab`    | next area          |
| `?`      | hide or show keys  |

`Tab` cycles through the dir column, the image view and the band, skipping the column
while it is closed.

### Image view

| Key                    | Action                                           |
|------------------------|--------------------------------------------------|
| `Left` / `Right`       | previous / next file                             |
| `+` / `-`, mouse wheel | zoom (the wheel toward the pointer)              |
| drag                   | pan                                              |
| `0`                    | fit to the window                                |
| `1`                    | 100 %                                            |
| `l` / `r`              | turn a quarter left / right, on screen only      |
| `Enter`                | write the turn into the file                     |
| `c`                    | copy the picture to the clipboard                |
| `Shift`+`c`            | copy the file to the clipboard                   |
| `o`                    | open in another app                              |
| `i`                    | metadata panel                                   |
| `x`                    | move to the trash                                |
| `p`                    | tools (annotation toolbar)                       |
| `g`                    | overview, the file shown selected                |
| `n`                    | dir column                                       |
| `b`                    | focus the preloaded files                        |
| `Up` / `Down`          | previous / next image inside a multi-image file  |
| `Space`                | play / pause a video, Live Photo or Motion Photo |
| click                  | play / pause a video                             |
| `m`                    | mute / unmute                                    |
| `Shift+Left` / `Right` | seek a video 10 s                                |
| `Ctrl+Left` / `Right`  | seek a video 30 s                                |
| `Alt+Left` / `Right`   | step a video one frame and pause                 |

### Annotation toolbar (`p`)

| Key                     | Action                     |
|-------------------------|----------------------------|
| drag on the image       | draw with the chosen tool  |
| `Ctrl+Z`                | undo                       |
| `Ctrl+Shift+Z`          | redo                       |
| `Enter`                 | save (opens the dialog)    |

In the save dialog, `Enter` saves as a new file and `Esc` goes back to editing.

### Dir column (`n`)

| Key                          | Action                                                         |
|------------------------------|----------------------------------------------------------------|
| `Up` / `Down`                | go to the folder above or below                                |
| `Right` / `Enter`            | go into the first subfolder                                    |
| `Left` / `Backspace`         | go to the parent                                               |
| `Esc`                        | close, back to the image view                                  |
| `.`                          | show dot files and dot folders until you leave the level       |
| `Shift`+`.`                  | show dot files and dot folders in every folder, until `.`      |
| `g`                          | overview of the current folder                                 |
| `n`                          | close                                                          |
| `b`                          | focus the preloaded files                                      |

A click on a folder goes there, a double-click goes into its first subfolder, and the
`..` row on top goes to the parent.

The key line tags what is on.

### Band (`b`)

| Key              | Action                                     |
|------------------|--------------------------------------------|
| `Left` / `Right` | move the highlight                         |
| `Enter`          | show the highlighted image                 |
| `Esc`            | back to the image view, the file kept      |
| click            | show the clicked image (also without `b`)  |

### Overview (`g`)

| Key                     | Action                                            |
|-------------------------|---------------------------------------------------|
| arrows                  | move the selection                                |
| `Enter`, click          | show the file in the image view                   |
| `+` / `-`, `Ctrl`+wheel | thumbnail size                                    |
| `c` / `Shift`+`c`       | copy the picture / the file                       |
| `o`                     | open in another app                               |
| `x`                     | move to the trash                                 |
| `g` / `Esc`             | close, showing the selected file                  |
| `Tab`                   | switch between gallery and dir column             |

The dir column works the same in the overview: the overview follows the folder it
goes to.

### Settings (`s`)

| Key           | Action |
|---------------|--------|
| `s` / `Esc`   | close  |

The settings are kept in `settings.toml` in the platform config directory and may be
edited by hand.

Under "Default apps" the settings make pixpixpix the default app for a file type, or for
all images or all videos at once, and give the type back to the app it replaced, which
they remember. pixpixpix becomes a default only there, never by being installed. While
it is the default, `o` opens files in the app it replaced. On Linux this goes through
`xdg-mime`, on macOS through Launch Services; the Flatpak can only show the defaults.

## Formats

| Kind                | Formats                                                               |
|---------------------|-----------------------------------------------------------------------|
| Images              | JPEG, PNG, GIF, WebP, BMP, TIFF                                       |
| Through libheif     | HEIC/HEIF, AVIF                                                       |
| Animations          | animated GIF and WebP                                                 |
| Multi-image files   | multi-page TIFF, MPO, multi-image HEIC/HEIF                           |
| Through libmpv      | mp4, mov, m4v, mkv, webm, avi, mts, m2ts, 3gp, wmv, flv, mpg          |
| Moving photos       | Apple Live Photos (photo plus MOV), Google and Samsung Motion Photos  |

Images are shown upright according to their EXIF orientation.

## Native libraries

pixpixpix itself is a plain Rust program. A few formats and features need system
libraries, which are loaded only the first time they are needed, never at start, so
they never slow down the first image. Without them everything else works; the
feature that needs them says what is missing. The About page, linked from the settings
(`s`), lists which of them were found and where.

| Library      | Needed for                                          | Without it                                         |
|--------------|-----------------------------------------------------|----------------------------------------------------|
| libheif      | HEIC/HEIF and AVIF                                  | such files show as broken, with the reason         |
| libmpv       | videos, Live Photos and Motion Photos               | videos say libmpv is missing; overview placeholder |
| libturbojpeg | lossless JPEG turns, upright before removing EXIF   | the turn or the removal is refused, naming it      |

libheif decodes through plugins: HEIC needs its libde265 plugin and AVIF its dav1d or
aom plugin. Some distributions package these separately.

| System         | Packages                                                                                  |
|----------------|-------------------------------------------------------------------------------------------|
| Ubuntu 24.04   | `libheif1 libheif-plugin-libde265 libheif-plugin-dav1d libmpv2 libturbojpeg`              |
| Fedora         | `libheif mpv-libs turbojpeg`                                                              |
| macOS (brew)   | `libheif mpv jpeg-turbo`                                                                  |

Fedora ships `ffmpeg-free`, which cannot decode H.264 and HEVC, the codecs of most
phone videos; such a video then says its codec cannot be decoded. The full ffmpeg
from [RPM Fusion](https://rpmfusion.org/Configuration) decodes them:

```sh
sudo dnf swap ffmpeg-free ffmpeg --allowerasing
```

## Install

### Homebrew (macOS and Linux)

From the `paxel/tap` tap. Install by the full name: Homebrew then trusts just this
formula or cask from the tap, not the whole tap (see Homebrew's
[Tap Trust](https://docs.brew.sh/Tap-Trust)).

On Linux (x86_64 and aarch64), the formula:

```sh
brew install paxel/tap/pixpixpix
```

It puts pixpixpix into the application menu and into "Open with" for images and
videos, never as the default app. The native libraries come from your distribution,
see above.

On macOS (Apple Silicon), the cask installs `pixpixpix.app` with libmpv bundled, plus
the `pixpixpix` and `pix3` commands:

```sh
brew install --cask paxel/tap/pixpixpix
```

The app is not signed, so macOS blocks its first start: allow it once in System
Settings > Privacy & Security > "Open Anyway", or run
`xattr -dr com.apple.quarantine /Applications/pixpixpix.app`.

### Flatpak (Linux)

Each release carries a Flatpak bundle for x86_64 and aarch64, with libmpv, libheif
and libturbojpeg included. This installs the latest one for the current user:

```sh
curl -fsSL https://raw.githubusercontent.com/paxel/pixpixpix/summer/packaging/flatpak/install.sh | sh
```

It needs `flatpak` and `curl`. The script adds the Flathub remote (`--user`, if it
is not there yet) only for the runtime, installs the bundle with `--user` and writes
`pixpixpix` and `pix3` commands that start the Flatpak to `~/.local/bin`. Running it
again updates to the latest release.

The Flatpak sees your home, `/media`, `/run/media` and `/mnt`. `o` opens files in
your default apps through the desktop portal; other apps named in the settings
cannot be started from the sandbox. Its settings live in
`~/.var/app/io.github.paxel.pixpixpix`.

To uninstall:

```sh
flatpak uninstall --user io.github.paxel.pixpixpix
rm ~/.local/bin/pixpixpix ~/.local/bin/pix3
```

### Release tarballs

Each [GitHub release](https://github.com/paxel/pixpixpix/releases) carries a tarball
for Linux x86_64, Linux aarch64 and macOS arm64 with the `pixpixpix` and `pix3`
binaries. Put them anywhere on your `PATH`. The macOS build is unsigned.

### From source

With a current stable Rust toolchain:

```sh
git clone https://github.com/paxel/pixpixpix
cd pixpixpix
cargo build --release
# target/release/pixpixpix and target/release/pix3
```

No development packages are needed to build; the native libraries above are only
loaded at runtime.

## Reporting a problem

Every error has a Copy button. It copies the message together with the file, the
pixpixpix version, the system and how pixpixpix was installed, ready to paste into a
search or an [issue](https://github.com/paxel/pixpixpix/issues).

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

## The name

The name nods to the band Pigs Pigs Pigs Pigs Pigs Pigs Pigs
([Wikipedia](https://en.wikipedia.org/wiki/Pigs_Pigs_Pigs_Pigs_Pigs_Pigs_Pigs),
[Spotify](https://open.spotify.com/artist/1F7QDWyZTLGzkyGLgFjEhU)).

## License

[MIT](LICENSE). libmpv, which pixpixpix loads at runtime when it is installed, comes
under its own licence (GPL or LGPL, depending on how it was built). The macOS app and
the Flatpak bundle include libmpv and are therefore GPL licensed.
