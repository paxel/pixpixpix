//! The overview of `g`: the media files of a folder and all folders below it,
//! in sections per folder, scanned in the background; thumbnails are decoded
//! only for the tiles on screen. A selection moves over the tiles; the file
//! it is on can be opened, trashed, copied or opened elsewhere.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::{Path, PathBuf};

use super::{Command, Core, Focus, Key, MotionKind};
use crate::decode::DecodedImage;
use crate::folder;
use crate::rotate::Rotation;
use crate::settings;

/// How many thumbnails are decoded at once.
const PARALLEL_THUMBNAILS: usize = 4;

/// The overview gallery shown instead of the image view and the band.
#[derive(Debug, Clone, PartialEq)]
pub struct Overview {
    /// The folder the overview is of.
    pub root: PathBuf,
    /// In the order scanned: depth-first, natural order, a folder's own files
    /// before its subfolders.
    pub sections: Vec<Section>,
    /// The tiles of all sections, one after the other.
    pub tiles: Vec<Tile>,
    /// More sections may come.
    pub scanning: bool,
    /// The tiles on screen, as the UI last reported them.
    pub visible: Range<usize>,
    /// The index of the selected tile; the first once there is one.
    pub selected: Option<usize>,
    /// Tiles per row, as the UI last reported them.
    pub columns: usize,
    /// What the UI is to scroll into view once, then report with
    /// [`super::Event::OverviewRevealed`].
    pub reveal: Option<Reveal>,
}

/// A place in the overview to scroll to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reveal {
    /// The tile with this index, as little as needed.
    Tile(usize),
}

/// One folder with media files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// The folder.
    pub dir: PathBuf,
    /// The folder relative to the root, the root's own name for the root.
    pub title: String,
    /// Its files in [`Overview::tiles`].
    pub tiles: Range<usize>,
}

/// One file of the overview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tile {
    pub path: PathBuf,
    /// A video, shown by its first frame with a play mark.
    pub video: bool,
    /// A Live Photo; Motion Photos are not known here.
    pub motion: Option<MotionKind>,
    /// Absent until decoded, and again once off screen.
    pub thumbnail: Option<Result<DecodedImage, String>>,
}

/// What the core keeps of the overview besides its view.
#[derive(Debug, Default)]
pub(super) struct Gallery {
    /// The number of the last scan asked for.
    scan: u64,
    /// Thumbnail decodes running, by file, with the tile index.
    decoding: BTreeMap<PathBuf, usize>,
    /// The tiles holding a thumbnail.
    loaded: BTreeSet<usize>,
    /// The overview left by opening a file, its scan running on; `g` brings
    /// it back.
    kept: Option<Overview>,
    /// The MOVs of the Live Photos among the tiles, by photo.
    live: BTreeMap<PathBuf, PathBuf>,
    /// The file `c` waits for the decode of, to copy it.
    pub(super) copying: Option<PathBuf>,
    /// The focus before the overview opened, given back when `g` closes it.
    focus: Option<Focus>,
}

impl Core {
    /// `g`: brings back the overview left by opening a file while the
    /// current folder is inside its root, or opens the overview of the
    /// folder highlighted in the dir column, or of the column's folder
    /// without one, or else of the current folder; or closes it. The gallery
    /// gets the focus; a playing video is paused.
    pub(super) fn toggle_overview(&mut self) -> Vec<Command> {
        if self.view.overview.is_some() {
            return self.close_overview();
        }
        if !matches!(self.view.focus, Focus::Image | Focus::Dirs) {
            return Vec::new();
        }
        let mut commands = Vec::new();
        let kept = self.gallery.kept.take();
        // A kept overview left behind goes, its scan with it.
        let kept_scanning = kept.as_ref().is_some_and(|o| o.scanning);
        let inside = kept.as_ref().is_some_and(|o| {
            self.dir
                .as_ref()
                .is_some_and(|dir| dir.starts_with(&o.root))
        });
        if let Some(mut kept) = kept.filter(|_| inside) {
            kept.reveal = kept.selected.map(Reveal::Tile);
            self.view.overview = Some(kept);
        } else {
            if kept_scanning {
                commands.push(Command::CancelScan);
            }
            let Some(root) = self.dir.clone() else {
                return Vec::new();
            };
            commands.extend(self.scan_overview(root));
        }
        self.gallery.focus = Some(self.view.focus);
        self.set_focus(Focus::Image);
        if self.video.as_ref().is_some_and(|v| v.playing) {
            commands.extend(self.toggle_play());
        }
        commands
    }

    /// Opens an empty overview of `root` and scans it.
    fn scan_overview(&mut self, root: PathBuf) -> Vec<Command> {
        self.gallery.scan = self.gallery.scan.wrapping_add(1);
        self.gallery.live.clear();
        self.view.overview = Some(Overview {
            root: root.clone(),
            sections: Vec::new(),
            tiles: Vec::new(),
            scanning: true,
            visible: 0..0,
            selected: None,
            columns: 1,
            reveal: None,
        });
        vec![Command::ScanOverview {
            scan: self.gallery.scan,
            root,
            dots: self.dots,
        }]
    }

    /// Drops the overview, cancelling its scan and thumbnail decodes; the
    /// view behind it shows again as it was, with the focus it had.
    pub(super) fn close_overview(&mut self) -> Vec<Command> {
        let Some(overview) = self.view.overview.take() else {
            return Vec::new();
        };
        let mut commands = Vec::new();
        if overview.scanning {
            commands.push(Command::CancelScan);
        }
        commands.extend(self.drop_thumbnails(None));
        self.gallery.live.clear();
        let focus = match self.gallery.focus.take() {
            Some(Focus::Dirs) if self.view.dir_column.is_some() => Focus::Dirs,
            _ => Focus::Image,
        };
        self.set_focus(focus);
        commands
    }

    /// Cancels the thumbnail decodes and drops the thumbnails of `overview`,
    /// or of the open one.
    fn drop_thumbnails(&mut self, overview: Option<&mut Overview>) -> Vec<Command> {
        let commands = std::mem::take(&mut self.gallery.decoding)
            .into_keys()
            .map(Command::CancelThumbnail)
            .collect();
        let loaded = std::mem::take(&mut self.gallery.loaded);
        if let Some(overview) = overview.or(self.view.overview.as_mut()) {
            for index in loaded {
                if let Some(tile) = overview.tiles.get_mut(index) {
                    tile.thumbnail = None;
                }
            }
        }
        commands
    }

    /// The overview open, or else the one kept while a file is open.
    fn overview_mut(&mut self) -> Option<&mut Overview> {
        self.view.overview.as_mut().or(self.gallery.kept.as_mut())
    }

    /// A folder of the running scan with its media files: a section of
    /// placeholders, the MOVs of Live Photos left out. The first tile is
    /// selected.
    pub(super) fn overview_section(
        &mut self,
        scan: u64,
        dir: &Path,
        files: Vec<PathBuf>,
    ) -> Vec<Command> {
        if scan != self.gallery.scan {
            return Vec::new();
        }
        let live = folder::live_photos(&files);
        let files = folder::without_live_videos(files);
        let Some(overview) = self.overview_mut() else {
            return Vec::new();
        };
        if files.is_empty() {
            return Vec::new();
        }
        let start = overview.tiles.len();
        overview.tiles.extend(files.into_iter().map(|path| Tile {
            video: folder::is_video(&path),
            motion: live.contains_key(&path).then_some(MotionKind::LivePhoto),
            path,
            thumbnail: None,
        }));
        overview.sections.push(Section {
            dir: dir.to_path_buf(),
            title: title(&overview.root, dir),
            tiles: start..overview.tiles.len(),
        });
        if overview.selected.is_none() {
            overview.selected = Some(start);
        }
        self.gallery.live.extend(live);
        self.overview_refresh()
    }

    /// The running scan found every folder.
    pub(super) fn overview_scanned(&mut self, scan: u64) {
        if scan != self.gallery.scan {
            return;
        }
        if let Some(overview) = self.overview_mut() {
            overview.scanning = false;
        }
    }

    /// The UI shows the tiles in `visible`.
    pub(super) fn overview_visible(&mut self, visible: Range<usize>) -> Vec<Command> {
        let Some(overview) = self.view.overview.as_mut() else {
            return Vec::new();
        };
        overview.visible = visible;
        self.overview_refresh()
    }

    /// The UI lays out `columns` tiles per row.
    pub(super) fn overview_columns(&mut self, columns: usize) {
        if let Some(overview) = self.view.overview.as_mut() {
            overview.columns = columns.max(1);
        }
    }

    /// The UI scrolled to what was to be revealed.
    pub(super) fn overview_revealed(&mut self) {
        if let Some(overview) = self.view.overview.as_mut() {
            overview.reveal = None;
        }
    }

    /// A thumbnail asked for arrived; one of another size than the tiles'
    /// is late, the one in their size still to come.
    pub(super) fn thumbnail_decoded(
        &mut self,
        path: &Path,
        size: u32,
        result: Result<DecodedImage, String>,
    ) -> Vec<Command> {
        if size != self.view.settings.thumbnail_size {
            return Vec::new();
        }
        let Some(index) = self.gallery.decoding.remove(path) else {
            return Vec::new();
        };
        if let Some(tile) = self
            .view
            .overview
            .as_mut()
            .and_then(|o| o.tiles.get_mut(index))
        {
            tile.thumbnail = Some(result);
            self.gallery.loaded.insert(index);
        }
        self.overview_refresh()
    }

    /// Fits the thumbnails to the tiles on screen: those off screen are
    /// dropped or their decode cancelled, missing ones are asked for in order,
    /// a few at a time, in the size of the tiles.
    fn overview_refresh(&mut self) -> Vec<Command> {
        let Self { view, gallery, .. } = self;
        let size = view.settings.thumbnail_size;
        let Some(overview) = view.overview.as_mut() else {
            return Vec::new();
        };
        let end = overview.visible.end.min(overview.tiles.len());
        let visible = overview.visible.start.min(end)..end;
        let gone: Vec<usize> = gallery
            .loaded
            .iter()
            .filter(|i| !visible.contains(i))
            .copied()
            .collect();
        for index in gone {
            gallery.loaded.remove(&index);
            if let Some(tile) = overview.tiles.get_mut(index) {
                tile.thumbnail = None;
            }
        }
        let (keep, cancel): (BTreeMap<_, _>, BTreeMap<_, _>) =
            std::mem::take(&mut gallery.decoding)
                .into_iter()
                .partition(|(_, i)| visible.contains(i));
        gallery.decoding = keep;
        let mut commands: Vec<Command> = cancel.into_keys().map(Command::CancelThumbnail).collect();
        for index in visible {
            if gallery.decoding.len() >= PARALLEL_THUMBNAILS {
                break;
            }
            let Some(tile) = overview.tiles.get(index) else {
                break;
            };
            if tile.thumbnail.is_some() || gallery.decoding.contains_key(&tile.path) {
                continue;
            }
            gallery.decoding.insert(tile.path.clone(), index);
            commands.push(Command::DecodeThumbnail {
                path: tile.path.clone(),
                size,
            });
        }
        commands
    }

    /// The thumbnail size changed: every thumbnail is made again in the new
    /// size.
    pub(super) fn overview_resized(&mut self) -> Vec<Command> {
        let mut commands = self.drop_thumbnails(None);
        commands.extend(self.overview_refresh());
        commands
    }

    /// Keys while the overview is open: `Tab` moves between dir column and
    /// gallery, the rest goes to the focused one.
    pub(super) fn overview_key(&mut self, key: Key) -> Vec<Command> {
        match (key, self.view.focus) {
            (Key::Tab, Focus::Dirs) => {
                self.set_focus(Focus::Image);
                Vec::new()
            }
            (Key::Tab, _) if self.view.dir_column.is_some() => {
                self.set_focus(Focus::Dirs);
                Vec::new()
            }
            (_, Focus::Dirs) => self.contents_key(key),
            (_, Focus::Image) => self.gallery_key(key),
            _ => Vec::new(),
        }
    }

    /// Keys of the gallery.
    fn gallery_key(&mut self, key: Key) -> Vec<Command> {
        match key {
            Key::Left | Key::Right | Key::Up | Key::Down => {
                self.move_selection(key);
                Vec::new()
            }
            Key::Enter => self.open_selected(),
            Key::Plus => self.resize_thumbnails(true),
            Key::Minus => self.resize_thumbnails(false),
            Key::X => self.trash_selected(),
            Key::C => self.copy_selected(),
            Key::O => {
                let path = self.selected_path();
                self.open_externally(path)
            }
            _ => Vec::new(),
        }
    }

    /// Keys of the dir column while the overview is open, as without it.
    /// `.` does nothing.
    fn contents_key(&mut self, key: Key) -> Vec<Command> {
        match key {
            Key::Up
            | Key::Down
            | Key::Left
            | Key::Right
            | Key::Enter
            | Key::Escape
            | Key::Backspace => self.column_action(|core| core.dir_key(key)),
            _ => Vec::new(),
        }
    }

    /// Runs `action` of the dir column; an open overview follows the folder
    /// it went to, scanned anew unless it is the root already.
    pub(super) fn column_action(
        &mut self,
        action: impl FnOnce(&mut Self) -> Vec<Command>,
    ) -> Vec<Command> {
        let before = self.dir.clone();
        let mut commands = action(self);
        let Some(root) = self.dir.clone().filter(|dir| Some(dir) != before.as_ref()) else {
            return commands;
        };
        if self.view.overview.as_ref().is_some_and(|o| o.root != root) {
            if self.view.overview.as_ref().is_some_and(|o| o.scanning) {
                commands.push(Command::CancelScan);
            }
            commands.extend(self.drop_thumbnails(None));
            commands.extend(self.scan_overview(root));
        }
        commands
    }

    /// The arrows move the selection: Left/Right through all tiles in order,
    /// Up/Down by rows, from section to section.
    fn move_selection(&mut self, key: Key) {
        let Some(overview) = self.view.overview.as_mut() else {
            return;
        };
        let Some(selected) = overview.selected else {
            return;
        };
        let target = match key {
            Key::Left => selected.checked_sub(1),
            Key::Right => selected
                .checked_add(1)
                .filter(|i| *i < overview.tiles.len()),
            Key::Up => row_above(&overview.sections, overview.columns, selected),
            Key::Down => row_below(&overview.sections, overview.columns, selected),
            _ => None,
        };
        if let Some(index) = target {
            overview.selected = Some(index);
            overview.reveal = Some(Reveal::Tile(index));
        }
    }

    /// The file of the selected tile.
    fn selected_path(&self) -> Option<PathBuf> {
        let overview = self.view.overview.as_ref()?;
        let tile = overview.tiles.get(overview.selected?)?;
        Some(tile.path.clone())
    }

    /// A tile was clicked: it is selected and opened.
    pub(super) fn open_tile(&mut self, index: usize) -> Vec<Command> {
        let Some(overview) = self.view.overview.as_mut() else {
            return Vec::new();
        };
        if index >= overview.tiles.len() {
            return Vec::new();
        }
        overview.selected = Some(index);
        self.open_selected()
    }

    /// `Enter`: the selected file shows in the image view, in its folder as
    /// if entered there. The overview is kept as it is, its scan running on,
    /// for `g` to bring back; its thumbnails are dropped meanwhile.
    fn open_selected(&mut self) -> Vec<Command> {
        let Some(path) = self.selected_path() else {
            return Vec::new();
        };
        let Some(dir) = path.parent().map(Path::to_path_buf) else {
            return Vec::new();
        };
        let mut kept = self.view.overview.take();
        let mut commands = self.drop_thumbnails(kept.as_mut());
        if let Some(kept) = kept.as_mut() {
            kept.visible = 0..0;
            kept.reveal = None;
        }
        self.gallery.kept = kept;
        self.gallery.focus = None;
        if self.dir.as_deref() != Some(dir.as_path()) {
            commands.extend(if self.view.dir_column.is_some() {
                self.enter(dir)
            } else {
                self.switch_folder(dir)
            });
        }
        self.view.key_line.notice = None;
        commands.extend(self.go_to(path));
        self.set_focus(Focus::Image);
        commands
    }

    /// `+`/`-`: the next larger or smaller thumbnail size, saved.
    fn resize_thumbnails(&mut self, larger: bool) -> Vec<Command> {
        let mut settings = self.view.settings.clone();
        settings.thumbnail_size = settings::step_thumbnail_size(settings.thumbnail_size, larger);
        self.change_settings(settings)
    }

    /// `Ctrl`+wheel over the overview.
    pub(super) fn wheel_thumbnails(&mut self, larger: bool) -> Vec<Command> {
        if self.view.overview.is_none() || self.view.settings_open {
            return Vec::new();
        }
        self.resize_thumbnails(larger)
    }

    /// `x`: sends the selected file to the trash, with the MOV of a Live
    /// Photo; its tile goes once it is there.
    fn trash_selected(&mut self) -> Vec<Command> {
        let Some(path) = self.selected_path() else {
            return Vec::new();
        };
        if self.trashing.contains(&path) {
            return Vec::new();
        }
        self.view.trash_error = None;
        let mov = self.gallery.live.get(&path).cloned();
        let files = [vec![path], mov.into_iter().collect()].concat();
        self.trashing.extend(files.iter().cloned());
        files.into_iter().map(Command::Trash).collect()
    }

    /// A file is in the trash: its tile goes, the selection moves on to the
    /// tile after it, or the one before after the last.
    pub(super) fn overview_trashed(&mut self, path: &Path) -> Vec<Command> {
        self.gallery.live.remove(path);
        let open = self.view.overview.is_some();
        let Self { view, gallery, .. } = self;
        let Some(overview) = view.overview.as_mut().or(gallery.kept.as_mut()) else {
            return Vec::new();
        };
        let Some(index) = overview.tiles.iter().position(|t| t.path == path) else {
            return Vec::new();
        };
        overview.tiles.remove(index);
        let earlier = |i: usize| if i > index { i.saturating_sub(1) } else { i };
        for section in &mut overview.sections {
            section.tiles = earlier(section.tiles.start)..earlier(section.tiles.end);
        }
        overview.sections.retain(|s| !s.tiles.is_empty());
        let last = overview.tiles.len().checked_sub(1);
        overview.selected = overview
            .selected
            .map(earlier)
            .and_then(|s| last.map(|l| s.min(l)));
        if !open {
            return Vec::new();
        }
        let mut commands = Vec::new();
        if gallery.decoding.remove(path).is_some() {
            commands.push(Command::CancelThumbnail(path.to_path_buf()));
        }
        gallery.loaded = gallery
            .loaded
            .iter()
            .filter(|i| **i != index)
            .map(|i| earlier(*i))
            .collect();
        for i in gallery.decoding.values_mut() {
            *i = earlier(*i);
        }
        commands.extend(self.overview_refresh());
        commands
    }

    /// `c`: puts the selected file and its pixels on the clipboard, once
    /// decoded.
    fn copy_selected(&mut self) -> Vec<Command> {
        let Some(path) = self.selected_path() else {
            return Vec::new();
        };
        self.view.key_line.notice = None;
        self.view.copy_error = None;
        let mut commands = Vec::new();
        if let Some(old) = self.gallery.copying.take()
            && !self.decoding_elsewhere(&old)
        {
            commands.push(Command::CancelDecode(old));
        }
        if let Some(Ok(image)) = self.cache.get(&path) {
            commands.push(Command::Copy {
                path: Some(path.clone()),
                image: image.clone(),
                rotation: Rotation::None,
                shapes: Vec::new(),
            });
            return commands;
        }
        if !self.decoding_elsewhere(&path) {
            commands.push(Command::Decode(path.clone()));
        }
        self.gallery.copying = Some(path);
        commands
    }

    /// Cancels the decode of `path` the image view no longer needs, unless
    /// `c` waits for it.
    pub(super) fn cancel_decode(&self, path: PathBuf) -> Option<Command> {
        (self.gallery.copying.as_ref() != Some(&path)).then_some(Command::CancelDecode(path))
    }

    /// The decode `c` waits for arrived: the image goes to the clipboard, or
    /// why it cannot is shown.
    pub(super) fn copy_decoded(
        &mut self,
        path: &Path,
        result: &Result<DecodedImage, String>,
    ) -> Vec<Command> {
        if self.gallery.copying.as_deref() != Some(path) {
            return Vec::new();
        }
        self.gallery.copying = None;
        match result {
            Ok(image) => vec![Command::Copy {
                path: Some(path.to_path_buf()),
                image: image.clone(),
                rotation: Rotation::None,
                shapes: Vec::new(),
            }],
            Err(message) => {
                self.view.copy_error = Some(message.clone());
                Vec::new()
            }
        }
    }
}

/// The section holding tile `index` and the tile's offset in it.
fn place(sections: &[Section], index: usize) -> Option<(usize, usize)> {
    let section = sections.iter().position(|s| s.tiles.contains(&index))?;
    let start = sections.get(section)?.tiles.start;
    Some((section, index.checked_sub(start)?))
}

/// The tile below `index` in rows of `columns`: in the same column of the
/// next row, the last tile of a shorter next row, or the next section's
/// first row.
fn row_below(sections: &[Section], columns: usize, index: usize) -> Option<usize> {
    let (section, offset) = place(sections, index)?;
    let tiles = &sections.get(section)?.tiles;
    let len = tiles.len();
    let column = offset.checked_rem(columns)?;
    let row_start = offset.checked_sub(column)?;
    if row_start.checked_add(columns)? < len {
        let below = offset.checked_add(columns)?.min(len.checked_sub(1)?);
        return tiles.start.checked_add(below);
    }
    let next = &sections.get(section.checked_add(1)?)?.tiles;
    next.start
        .checked_add(column.min(next.len().checked_sub(1)?))
}

/// The tile above `index` in rows of `columns`: in the same column of the
/// row before, or of the previous section's last row, its last tile when
/// that row is shorter.
fn row_above(sections: &[Section], columns: usize, index: usize) -> Option<usize> {
    let (section, offset) = place(sections, index)?;
    let column = offset.checked_rem(columns)?;
    if offset >= columns {
        return index.checked_sub(columns);
    }
    let previous = &sections.get(section.checked_sub(1)?)?.tiles;
    let last = previous.len().checked_sub(1)?;
    let last_row = last.checked_sub(last.checked_rem(columns)?)?;
    previous
        .start
        .checked_add(last_row.checked_add(column)?.min(last))
}

/// The heading of the section of `dir`: its path relative to `root`, with
/// `/` between folders; the root's own name for the root.
fn title(root: &Path, dir: &Path) -> String {
    match dir.strip_prefix(root) {
        Ok(relative) if !relative.as_os_str().is_empty() => relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
        _ => match folder::file_name(root) {
            name if name.is_empty() => root.display().to_string(),
            name => name,
        },
    }
}
