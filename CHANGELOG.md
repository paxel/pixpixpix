# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.2.0]

### Added

- `Shift`+`x` in the overview trashes every file of the selected file's folder, its subfolders left, after asking; one `u` brings them all back.
- `u` puts trashed files back, newest first, as long as pixpixpix runs, and shows each in its folder; it also finds files trashed on drives such as pCloud, which go into the drive's own trash folder. In the overview the file is back as a tile in its section, selected.
- `b` in the band hides it, and `b` brings it back; a hidden band stays hidden across starts, and `Tab` skips it.
- The settings can make pixpixpix the default app per file type, or for all images or videos, and give each type back to the app it replaced; `o` then opens files in that app. The Flatpak only shows the defaults.
- An About page, linked from the settings, shows the version, the license and links, and which optional libraries were found where, with install hints for missing ones and a Copy button.
- Every error has a Copy button that copies the message with the file, the version, the system and the install method, for a search or a bug report.
- The key line names the area that has the focus, and every area shows a diode in its bottom right corner that lights while it has it.
- A click on a key in the key line does what the key does; arrows near the image's edges step to the previous or next file; the toolbar has Save and Close buttons.
- `?` hides the key hints and brings them back, remembered across starts; a `?` marks where they were, and tags and errors still show.
- The dir column works with the mouse: a click goes to a folder, a double-click goes into its first subfolder, and a `..` row goes to the parent.

### Changed

- `Esc` always leads back to the image view: it closes the dir column and the overview (showing the selected file), and leaves the band keeping the file shown. `Left` and `Backspace` still go up a folder.
- A new icon: a pig face in front of three stacked pictures, also on the window.
- `c` copies only the picture, for chats and image apps; `Shift`+`c` copies the file, for file managers.
- The settings screen is laid out in sections with dividers, its labels and controls lined up.
- `g` opens the overview with the file shown selected, and `g` again shows the file selected there; the key line tags the overview while it is open.
- The dir column lists the current folder among its siblings, highlighted: `Up`/`Down` go to the folder above or below, `Right`/`Enter` into the first subfolder, `Left` to the parent. Opening it changes nothing on screen, and there are no folder previews; the overview's column works the same.
- The band keeps the current file in its middle, shows a placeholder for each file still loading, and is called "preloaded files" in the key line.
- Videos play and get their first frames in helper processes: a crash in libmpv marks only the file that caused it as not playable instead of closing pixpixpix, and libmpv's console output stays silent unless `PIXPIXPIX_DEBUG=1`.
- A video slow to show its first frame is no longer marked broken after 10 seconds.

### Removed

- `r` and `Shift`+`r` in the dir column no longer search the subfolders for images; the overview (`g`) shows them all.

### Fixed

- The key line wraps in narrow windows instead of cutting hints off.
- Frame steps forward and back are exact, so alternating them shows the same two frames.
- A jump past the end of a video stops paused on its last frame instead of ending it; a jump back past the start stops on the first frame.
- A video that cannot be decoded names its codec, also when only its sound plays.
- Without libmpv, the message names every library file tried and the package to install.
- Videos with a rotation flag, as iPhones record them, play and preview upright instead of crashing pixpixpix.

---

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
