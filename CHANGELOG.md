# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.1.0]

### Added

- `pixpixpix <file>` (or the short `pix3`) shows the image as fast as possible; a folder or no argument shows the folder's first image. JPEG, PNG, GIF, WebP, BMP and TIFF, upright according to EXIF orientation; HEIC/HEIF and AVIF too through libheif, loaded on the first such file (a missing libheif or decoder shows as a broken file with the reason). `Enter` refuses to write a turn into HEIC/HEIF and AVIF.
- Animated GIF and WebP show their first frame at once, then play and loop once all frames are decoded (delays under 20 ms play as 100 ms, as in browsers; an animation over 512 MB of pixels stays still); their band thumbs carry an animated badge.
- Multi-page TIFF, MPO and multi-image HEIC/HEIF show which of their images is shown ("2 / 5") and their band thumbs carry the image count; Up/Down in the image view step through the images, starting at the first again on each visit. Reduced-resolution TIFF pages, MPO thumbnails and Ultra HDR gain maps do not count. `Enter` refuses to write a turn into such files, as the other images would be lost.
- Videos (mp4, mov, m4v, mkv, webm, avi, mts, m2ts, 3gp, wmv, flv, mpg) are part of navigation through libmpv, loaded on the first video: the main view and the band show the first frame with a play triangle; `Space` or a click plays and pauses, with sound at system volume, and `m` mutes. Left/Right still change the file and stop playback; at the end the first frame shows again. A video that cannot play shows mpv's error (or that libmpv is missing) with the `o` hint, over its first frame when there is one. Frames are drawn in software, at most 1920 pixels a side.
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
- `x` moves the current file, a broken one too, to the system trash and shows the next image at once, or the previous one after the last; a folder left without images says so and opens the dir column. A failure brings the file back and is shown in the key line.
- `c` puts the shown image, with a pending turn, and the file itself on the clipboard at once, so pasting into an image app gives the picture and pasting into a file manager copies the file; the key line says `copied` or why not. Linux only for now, through X11 (on Wayland via XWayland).
- The window reopens at its last size and position; `f` toggles fullscreen; `Ctrl+Q` (`Cmd+Q` on macOS) quits.
- `PIXPIXPIX_TIMING=1` prints the time from launch to the first frame.
