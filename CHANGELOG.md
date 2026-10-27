# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.3.0]

### Added

- A [documentation site](https://paxel.github.io/pixpixpix/), opening with what the release brings, and What's new of every release; each release from 0.3.0 on keeps its own docs, chosen in a version menu.
- `Shift`+arrows [move a zoomed-in picture](https://paxel.github.io/pixpixpix/getting-around.html#the-image-view) a tenth of the window per press; the key line shows them while the picture is larger than the window.
- The [key line](https://paxel.github.io/pixpixpix/getting-around.html#the-key-line) ends with `?` hide hints, clickable like the other keys.

### Changed

- The [arrows to the previous and next file](https://paxel.github.io/pixpixpix/getting-around.html#the-image-view) show, dimmed, while the pointer is anywhere over the image, not only near its edges; the one under the pointer lights up.
- The key line lists `C` [copy picture and `Shift+C` copy file](https://paxel.github.io/pixpixpix/turning-and-editing.html#copy-and-open-elsewhere), and `.` and `Shift+.` (keep dot entries), as separate hints.
- A key in the [key line](https://paxel.github.io/pixpixpix/getting-around.html#the-key-line) lights up in amber under the pointer, with a hand pointer, and flashes when clicked; in a pair such as `Left/Right` only the key a click presses lights up.
- Without libturbojpeg, turning a JPEG names the [package to install](https://paxel.github.io/pixpixpix/formats.html#native-libraries).
- While the [trash view](https://paxel.github.io/pixpixpix/trash-and-undo.html#the-trash-view) reads the trash, it counts the seconds so far.

### Fixed

- A hint at the end of a [key line](https://paxel.github.io/pixpixpix/getting-around.html#the-key-line) no longer breaks between its key and what it does.
- A click on the `?` that stands for [hidden hints](https://paxel.github.io/pixpixpix/getting-around.html#the-key-line) brings them back.

---

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
