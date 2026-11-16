# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.4.0]

### Added

- Every release file carries signed build provenance, which `gh attestation verify <file> --repo paxel/pixpixpix` checks, and every release a CycloneDX SBOM of its source ([pix-105](https://github.com/paxel/pixpixpix/issues/105), [pix-127](https://github.com/paxel/pixpixpix/issues/127)).

### Changed

- `x` in the [overview](https://paxel.github.io/pixpixpix/trash-and-undo.html#trashing) dims the tile and shows a bin on it at once and moves the selection on; the tile goes once the file is in the trash.

### Fixed

- `u` pressed while a file is still on its way to the trash, as on a slow drive such as pCloud, keeps its tile in the overview, marked as coming back, and puts the file back once it got there; before, the file came back but its tile stayed gone.

### Security

- rustix is updated to 1.1.6, whose extended-attribute functions no longer hand out uninitialized memory (RUSTSEC-2026-0486, [pix-98](https://github.com/paxel/pixpixpix/issues/98)).
- The Flatpak installer (`packaging/flatpak/install.sh`) follows redirects to HTTPS addresses only ([pix-126](https://github.com/paxel/pixpixpix/issues/126)).

---

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
