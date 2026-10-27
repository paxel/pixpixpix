# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.3.0]

### Added

- The key line ends with `?` hide hints, clickable like the other keys.

### Changed

- The key line lists `C` copy picture and `Shift+C` copy file, and `.` and `Shift+.` (keep dot entries), as separate hints.
- A key in the key line lights up in amber under the pointer, with a hand pointer, and flashes when clicked; in a pair such as `Left/Right` only the key a click presses lights up.
- Without libturbojpeg, turning a JPEG names the package to install.
- While the bin view reads the trash, it counts the seconds so far.

### Fixed

- A click on the `?` that stands for hidden hints brings them back.

---

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
