# pixpixpix v1 — plan

Status: planned, not started.

Synthesized 2026-10-26 from a customer-service + grilling session ("i'm fed up with
image viewers. i want a rust based fast image viewer for linux mac"). Every entry under
"Decisions" was confirmed one by one in that session, except those marked
*assumption*. The architecture sketch and the milestone order are proposals.

## Problem statement

Existing image viewers are slow to show the image you asked for, block on things you
did not ask for, and make culling, quick fixes and finding an image by sight tedious.
pixpixpix is a keyboard-first image and video viewer whose first duty is to put the
requested image on screen as fast as possible; everything else happens after.

## Decisions

### Product

- Rust, Linux and macOS. License MIT.
- The name is a nod to the band Pigs Pigs Pigs Pigs Pigs Pigs Pigs
  ([Wikipedia](https://en.wikipedia.org/wiki/Pigs_Pigs_Pigs_Pigs_Pigs_Pigs_Pigs),
  [Spotify](https://open.spotify.com/artist/1F7QDWyZTLGzkyGLgFjEhU)). README and docs
  link at least the Wikipedia article. The GitHub Pages docs use the band's albums and
  images as references (license of each image to be checked before embedding).
- Name `pixpixpix` everywhere (repo, package, app id, command); `pix3` is an extra
  short command.
- UI: egui/eframe with the glow (OpenGL) backend.
- Video: libmpv, loaded lazily, never on the first-image path.
- v1 contains everything below.

### Startup

- `pix3 <file>` shows that file; `pix3 <dir>` and plain `pix3` (current directory)
  show the first image of that folder.
- Target: first image on screen within 300 ms for a 12 MP JPEG with a warm disk cache,
  measured and reported every release.
- Nothing runs before the first image is displayed: no folder scan, no config beyond
  the window geometry, no thumbnails, no libmpv, no watcher, no launcher registration.
- Window size and position are remembered; `f` toggles fullscreen.
- Every launch opens a new, independent window; no single-instance check.
- `Ctrl+Q` (Linux) / `Cmd+Q` (macOS) quits. `q` and `Esc` do not quit.

### Navigation and cache

- `←`/`→` previous/next file in the folder, natural file-name sort (`img2` before
  `img10`). No wrap-around; at the ends the key line shows "first image" / "last
  image".
- After the first image is shown, neighbours are preloaded: 5 per side by default,
  changeable in settings.
- All loading (neighbour cache, nav cache, thumbnails, recursive search, overview
  scan) is cancellable mid-read and is cancelled as soon as its result is no longer
  needed, e.g. on a directory change.
- The current folder is watched; files added, removed or renamed outside the app
  update navigation and band on their own.
- Deleting the last file (by order) shows the previous one. A folder that becomes
  empty shows a "no images" placeholder and opens the dir column.
- Zoom: fit to window by default; `+`/`-`/mouse wheel zoom, drag pans, `0` fits,
  `1` is 100 %. Zoom level and relative position carry over to the next image.

### Band (cache bar)

- A thin strip under the image shows thumbnails of exactly the cached images; the
  current one is marked.
- Clicking a thumb jumps to it. `b` focuses the band; `←`/`→` move the highlight,
  `Return` jumps to the highlighted image and gives focus back to the image view.
- Overlays: video (first frame + big play triangle), animated, multi-image, error.

### Focus and key line

- Three focus areas: dir column, image view, band. `Tab` cycles through them and
  skips the dir column while it is closed.
- The bottom line always lists the keys of the focused area.

### Dir column

- `n` toggles a column on the left with the subfolders of the current folder;
  closing it returns focus to the image view.
- `←`, `Esc`, `Backspace` go to the parent folder; `→`, `Enter` enter the highlighted
  subfolder; `↑`/`↓` move the highlight (*assumption*).
- Highlighting a subfolder previews its first image immediately and the band follows.
  A nav cache holds the first image of the parent folder and of the highlighted
  subfolder.
- A subfolder without images directly in it shows a "no images" placeholder with its
  subfolder count.
- Entering a folder behaves like starting the app: first image as fast as possible,
  then caching.
- `.` shows/hides dot files and dot folders. `r` searches the subfolders breadth-first
  (natural order within a level) for the nearest folder containing images, stops
  there and fills the band with that folder's images (normal cache limit); selecting
  one moves there. Both are one-shot actions, not settings: any navigation aborts
  them and resets them.

### Image actions (image view focused)

- `l`/`r` rotate left/right on screen only. `Return` writes the rotation into the
  pixels: JPEG via lossless 90° transform, other formats re-encoded. Navigating away
  without `Return` drops the rotation.
- `x` moves the file to the system trash immediately and shows the next image.
- `c` puts both the pixels (including a pending rotation) and the file reference on
  the clipboard.
- `o` opens the file in the system default app. Settings can override this with one
  app for images and one for videos. pixpixpix stays open.
- `s` opens the settings screen; settings are stored in a TOML file in the platform
  config directory. Also reachable from the `p` toolbar.
- `i` opens the metadata view (see below).
- `p` opens the toolbar (see below).

### Formats

- Images: JPEG, PNG, GIF, WebP, BMP, TIFF, HEIC/HEIF, AVIF. No camera RAW.
- Animated GIF/WebP play automatically and loop.
- Multi-image formats always get a hint: multi-page TIFF, multi-image HEIC (bursts,
  sequences), MPO, Apple Live Photo (HEIC + MOV pair), Google/Samsung Motion Photo
  (video embedded in JPEG). `↑`/`↓` step through inner images/pages; `Space` plays
  the motion part.
- A Live Photo's MOV is hidden from navigation; `x` on a Live Photo trashes both
  files.
- Videos: `mp4`, `mov`, `m4v`, `mkv`, `webm`, `avi`, `mts`, `m2ts`, `3gp`, `wmv`,
  `flv`, `mpg`.
- A file that fails to decode or play shows its first frame if obtainable, otherwise
  a placeholder, always with the decoder's/mpv's error message and the hint that `o`
  opens it externally. Its band thumb gets an error marker. Broken files are never
  skipped silently.

### Video

- A current video shows its first frame with a big play triangle; `Space` or a click
  starts playback. Sound on at system volume; `m` toggles mute.
- `←`/`→` always mean previous/next file (playback stops). `Shift+←/→` ±10 s,
  `Ctrl+←/→` ±30 s, `Alt+←/→` ±1 frame. On-screen step buttons while paused.
  **Open:** macOS takes `Ctrl+←/→` for switching Spaces by default, so the ±30 s
  binding needs a macOS answer.
- Seeks are serialized: at most one seek in flight; further steps only move the
  target, and the newest target is applied when the running seek finishes. Frame
  steps use mpv's frame-step commands, not time seeks. (catlog's frame picker fires
  overlapping seeks from a 150 ms hold timer and sometimes leaves the frame undrawn;
  this avoids that pattern.)

### Tools (`p`)

- Toolbar with: arrow, text, rectangle/ellipse outline, freehand pen, semi-transparent
  highlighter box, blur/pixelate area, free crop (drag handles). A palette of 5–6
  colors and 3 sizes. Undo `Ctrl+Z`/`Cmd+Z`, redo `Ctrl+Shift+Z`/`Cmd+Shift+Z`.
- Export crop: enter target pixels (e.g. 512×512, 4096×1237), draw the rectangle;
  ratio-locked by default, toggle to free (free stretches to the target size). Output
  format JPEG, PNG or WebP; optional maximum file size, met by lowering quality
  (PNG only via compression). The output carries no metadata.
- Tools also work on a paused video frame; the result is an image.

### Save dialog

- Styled like whiteboxed's unsaved-changes dialog (`src/ui/leave.rs`): large, centred
  over a dimmed window, with a panicked cat drawn by the egui painter — pixpixpix's
  own, different cat.
- Options: overwrite, save as new (name field, prefilled), copy to clipboard, undo.
  `Enter` = save as new, `Esc` = back to editing.
- Prefilled names: `<name>_edit.<ext>` (then `_edit2`, …) for tool edits,
  `<name>_<W>x<H>.<ext>` for export crop, `<video name>_<timestamp>.png` for video
  frames. Overwrite is hidden for video frames.
- Shown on `Return`, when leaving an image with unsaved tool edits, and on quit with
  unsaved tool edits. Also used for "remove location" and "remove all EXIF".

### Metadata (`i`)

- Shows the metadata of formats that carry it. Only the capture date/time is
  editable, one image at a time; `Return` in the field writes it immediately
  (metadata only, pixels untouched).
- "Remove location" and "remove all EXIF" go through the save dialog. Before removing
  all EXIF, the orientation is baked into the pixels (lossless for JPEG).

### Overview (`g`)

- `g` toggles a gallery of a root folder and all its subfolders, for finding an image
  by sight. Root = the folder highlighted in the dir column, otherwise the current
  image's folder.
- The dir column stays; image view and band are replaced by one scrollable list of
  sections. Each section has a header with the path relative to the root (e.g.
  `04/23`) followed by thumbnails with file names.
- Placeholders and names appear immediately; thumbnails are decoded in parallel, only
  for visible tiles. No database: the freedesktop thumbnail cache
  (`~/.cache/thumbnails`) is read and written; macOS uses an own cache directory with
  the same layout.
- Order: ascending natural sort, depth-first; a folder's own images come before its
  subfolders. Empty folders are omitted, dot folders are skipped (unless `.`),
  symlinked folders are not followed.
- Videos are included; overlays as in the band.
- Thumb size via `+`/`-`/`Ctrl`+wheel, 96–320 px, remembered.
- Arrows move the selection; `Return`/click opens the image in normal view in its
  folder; `g` returns to the overview at the same spot (scan state kept). `x`, `c`,
  `o` act on the selected thumb.
- Moving the highlight in the dir column scrolls to that folder's section; the root
  changes only on `→`/`Enter` or going up, which aborts the running scan.
- Focus in overview: `Tab` cycles dir column and gallery, `b` does nothing, the key
  line follows focus (*assumption*).

### Distribution

- App id `io.github.paxel.pixpixpix`; Homebrew tap `paxel/tap`
  (`brew install paxel/tap/pixpixpix`), same as catlog and dedup.
- macOS: cask with an unsigned `pixpixpix.app` plus `pixpixpix`/`pix3` CLI links;
  Apple Silicon only.
- Linux Homebrew: formula following the `brew-linux-desktop` recipe (desktop file
  named after the app id, app registers its own launcher, `post_install_steps`).
- Flatpak: a `.flatpak` bundle attached to each GitHub release, not on Flathub. A
  one-line `curl -fsSL … | sh` install script (documented in README and docs)
  downloads the latest bundle, adds the Flathub remote (`--user --if-not-exists`)
  only for the `org.freedesktop.Platform` runtime, installs with
  `flatpak install --user`, and writes `pixpixpix`/`pix3` wrappers to
  `~/.local/bin`. Permissions: `home`, `/media`, `/run/media`, `/mnt`.
- Linux builds for x86_64 and aarch64 (brew and Flatpak).
- pixpixpix appears in "Open with" for all supported image and video types and never
  makes itself the default.
- libmpv is bundled in the `.app` and the Flatpak; those bundles follow libmpv's GPL
  terms, the pixpixpix source stays MIT.

## Architecture sketch

One binary crate, edition 2024. Modules by responsibility; names are a starting point.

| Module      | Responsibility                                                              |
|-------------|-----------------------------------------------------------------------------|
| `startup`   | Argument parsing, geometry read, the first-image fast path, timing probe    |
| `decode`    | Format detection and decoding per format, cancellation token on every read  |
| `folder`    | Listing, natural sort, Live Photo pairing, watcher                          |
| `cache`     | Neighbour cache, nav cache, band thumbnails, cancellation on folder change  |
| `nav`       | Dir column state, `.` and `r` actions, breadth-first search                 |
| `overview`  | Depth-first scan, sections, visible-tile thumbnail jobs                     |
| `thumbs`    | Freedesktop thumbnail cache read/write (macOS: same layout, own directory)  |
| `video`     | Lazy libmpv loading, playback, serialized seeks, first-frame extraction     |
| `edit`      | Lossless JPEG rotation, re-encoding, tool rendering, export crop, size fit  |
| `metadata`  | EXIF read, date write, location/all removal, orientation baking             |
| `actions`   | Trash, clipboard (pixels + file reference), open externally                 |
| `settings`  | TOML config, settings screen                                                |
| `launcher`  | Desktop entry / MIME registration on Linux, in the background after start   |
| `ui`        | egui views: image, band, dir column, overview, key line, toolbar, dialogs   |

Crates to evaluate when each part is built (not yet verified against current
versions): `eframe`/`egui` 0.35, `image`, `zune-jpeg`, `libheif-rs`, a dav1d-backed
AVIF decoder, `libmpv2`, `trash`, `arboard` (check file-reference support per
platform), `notify`, an EXIF reader/writer that can rewrite dates in place, a lossless
JPEG transform (e.g. via `turbojpeg`), `directories`, a natural-sort helper.

## Milestones

Each milestone ships with its own unit tests and passes the compile gate before the
next starts.

1. **Fast path.** Window plus first image for `pix3 <file|dir>`, geometry
   remembered, fullscreen, quit keys, timing probe that reports launch-to-first-frame.
   Proves the 300 ms target before anything else is built.
2. **Folder browsing.** Natural sort, `←`/`→` with end hints, neighbour cache with
   cancellation, band with clicks, key line, zoom/pan carried over, watcher, error
   placeholders.
3. **Dir column.** `n`, parent/child keys, highlight preview and nav cache, empty
   folder placeholder, `.`, `r`, `Tab`/`b` focus, focus-dependent key line.
4. **Image actions.** Rotate with `Return` (lossless JPEG), trash, clipboard, `o` with
   overrides, settings screen and TOML file.
5. **Formats.** HEIC/HEIF, AVIF, animated GIF/WebP, multi-image formats with `↑`/`↓`,
   Live Photo pairing and joint delete, Motion Photo.
6. **Video.** Lazy libmpv, first-frame thumbs, play/pause, sound and mute, steps and
   serialized seeks, error display.
7. **Overview.** `g`, scan, sections, freedesktop thumbnail cache, visible-tile
   decoding, thumb size, selection and actions, dir column as table of contents.
8. **Tools and save dialog.** Toolbar, all drawing tools, crop, undo/redo, export crop,
   save dialog with the panicked cat, tools on video frames.
9. **Metadata.** `i` view, date edit, location/all-EXIF removal with orientation
   baking.
10. **Distribution.** GitHub release CI for macOS arm64 and Linux x86_64/aarch64,
    cask and formula in `paxel/tap`, Flatpak bundle and install script, launcher and
    "Open with" registration.

## Verification

- Per change: tests of the touched files, `cargo fmt --check`, `cargo clippy -- -D
  warnings`, with builds capped (`nice -n 19`, two jobs).
- Before a commit: compile gate only (`cargo check --workspace --all-targets` plus
  format and lint). Full suites run in CI.
- The first-image time is measured on every release and compared with the 300 ms
  target.

## Risks and checks

- Homebrew has been tightening unsigned casks (`--no-quarantine` deprecated). Check
  the current state before milestone 10; third-party taps are expected to keep
  working, Gatekeeper prompts may get worse.
- eframe/glow context creation is the main fixed startup cost; if milestone 1 misses
  300 ms, the fallback (raw winit + softbuffer for the first frame, egui after) was
  discussed and rejected for now; revisit only with measurements.
- libmpv bundling on macOS and in the Flatpak manifest is the heaviest packaging
  task.
- Clipboard with both pixels and a file reference may need platform-specific code.

## Out of scope

- Camera RAW formats.
- Flathub, homebrew-core, code signing and notarization.
- Intel Macs.
- Folder-wide date shifting; editing metadata fields other than the date.
- Becoming the default viewer automatically.
- Burst groups made of separate files (name-pattern grouping).
