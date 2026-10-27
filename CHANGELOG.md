# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.2.0]

### Changed

- Videos play and get their first frames in helper processes: a crash in libmpv marks only the file that caused it as not playable instead of closing pixpixpix, and libmpv's console output stays silent unless `PIXPIXPIX_DEBUG=1`.
- A video slow to show its first frame is no longer marked broken after 10 seconds.

### Fixed

- A video that cannot be decoded names its codec, also when only its sound plays.
- Without libmpv, the message names every library file tried and the package to install.
- Videos with a rotation flag, as iPhones record them, play and preview upright instead of crashing pixpixpix.

---

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
