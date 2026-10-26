# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.1.0]

### Added

- `pixpixpix <file>` (or the short `pix3`) shows the image as fast as possible; a folder or no argument shows the folder's first image. JPEG, PNG, GIF, WebP, BMP and TIFF, upright according to EXIF orientation.
- Left/Right arrows step to the previous/next image of the folder in natural name order, stopping at the ends with a "first image"/"last image" hint; the bottom key line lists the available keys.
- Up to 5 neighbours per side of the current image are preloaded after it is shown, so stepping to them is instant; a thin band above the key line shows them as thumbnails, marks the current one and jumps to a clicked one. Loading that is no longer needed is cancelled mid-read.
- A file that fails to decode shows the decoder's error and the hint that `o` opens it in another app; its band thumb carries an error marker and navigation still stops on it. A folder that cannot be read is reported in the key line.
- The folder is watched once the image is shown: files other programs add, remove or rename show up in navigation and the band on their own; when the shown file is removed, the next one is shown.
- Images fit the window by default; `+`/`-` and the mouse wheel zoom (the wheel toward the pointer), dragging pans, `0` fits and `1` shows 100 %. Zoom level and position carry over to the next image.
- `n` opens a column listing the subfolders of the current folder: Up/Down pick one, Right/Enter enter it, Left/Esc/Backspace go to the parent. A new folder opens like a start there, its first image first, and all loading for the old folder is cancelled. The key line lists the keys of the focused area.
- While the dir column has the focus, the main view and the band preview the first image of the highlighted subfolder; a subfolder without images shows "No images" with its subfolder count. The first images of the parent and the highlighted subfolder are preloaded, so going up or entering shows an image at once. A folder without images given at start gets a dir column too, and a folder that cannot be read shows why.
- In the dir column, `.` shows dot files and dot folders and `r` searches the subfolders breadth-first for the nearest folder with images, filling the band with them; picking one moves there. Both last until the next move in the column.
- `b` focuses the band: Left/Right move its highlight, Enter shows the highlighted image (a found one too) and returns to the image view, Esc goes back. `Tab` cycles dir column, image view and band, skipping the closed column.
- `s` opens the settings screen (`s` or `Esc` closes it): the number of images preloaded per side (1 to 20, default 5) takes effect at once and is saved to `settings.toml` in the config directory, which may also be edited by hand; a file that cannot be read keeps the defaults and the screen shows why.
- `o` opens the current file, a broken one too, in the system default app (`xdg-open` on Linux, `open` on macOS) and pixpixpix stays open; the settings screen can name one app for images and one for videos instead (`image_app`/`video_app` in `settings.toml`). A failure to launch is shown in the key line with its message.
- `l`/`r` turn the shown image a quarter left/right on screen only; `Enter` writes the turn into the file, JPEG losslessly (needs libturbojpeg) and other formats re-encoded, with the EXIF orientation baked in. Moving to another image without `Enter` drops the turn and leaves the file untouched; a failed write is shown in the key line.
- The window reopens at its last size and position; `f` toggles fullscreen; `Ctrl+Q` (`Cmd+Q` on macOS) quits.
- `PIXPIXPIX_TIMING=1` prints the time from launch to the first frame.
