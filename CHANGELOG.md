# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.1.0]

### Added

- `pixpixpix <file>` (or the short `pix3`) shows the image as fast as possible; a folder or no argument shows the folder's first image. JPEG, PNG, GIF, WebP, BMP and TIFF, upright according to EXIF orientation.
- Left/Right arrows step to the previous/next image of the folder in natural name order, stopping at the ends with a "first image"/"last image" hint; the bottom key line lists the available keys.
- The window reopens at its last size and position; `f` toggles fullscreen; `Ctrl+Q` (`Cmd+Q` on macOS) quits.
- `PIXPIXPIX_TIMING=1` prints the time from launch to the first frame.
