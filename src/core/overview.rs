//! The overview of `g`: the media files of a folder and all folders below it,
//! in sections per folder, scanned in the background; thumbnails are decoded
//! only for the tiles on screen.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::{Path, PathBuf};

use super::{Command, Core, Focus, focus_keys};
use crate::decode::DecodedImage;
use crate::folder;

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
}

/// One folder with media files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// The folder relative to the root, the root's own name for the root.
    pub title: String,
    /// Its files in [`Overview::tiles`].
    pub tiles: Range<usize>,
}

/// One file of the overview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tile {
    pub path: PathBuf,
    /// A video, shown as a placeholder.
    pub video: bool,
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
}

impl Core {
    /// `g`: opens the overview of the folder highlighted in the dir column,
    /// or of the column's folder without one, or else of the current folder;
    /// or closes it. A playing video is paused.
    pub(super) fn toggle_overview(&mut self) -> Vec<Command> {
        if self.view.overview.is_some() {
            return self.close_overview();
        }
        if !matches!(self.view.focus, Focus::Image | Focus::Dirs) {
            return Vec::new();
        }
        let root = match &self.view.dir_column {
            Some(column) => column
                .highlight
                .and_then(|i| column.dirs.get(i))
                .unwrap_or(&column.dir)
                .clone(),
            None => match &self.dir {
                Some(dir) => dir.clone(),
                None => return Vec::new(),
            },
        };
        self.gallery.scan = self.gallery.scan.wrapping_add(1);
        self.view.overview = Some(Overview {
            root: root.clone(),
            sections: Vec::new(),
            tiles: Vec::new(),
            scanning: true,
            visible: 0..0,
        });
        let mut commands = vec![Command::ScanOverview {
            scan: self.gallery.scan,
            root,
            dots: self.dots,
        }];
        if self.video.as_ref().is_some_and(|v| v.playing) {
            commands.extend(self.toggle_play());
        }
        commands
    }

    /// Drops the overview, cancelling its scan and thumbnail decodes; the
    /// view behind it shows again as it was.
    fn close_overview(&mut self) -> Vec<Command> {
        let Some(overview) = self.view.overview.take() else {
            return Vec::new();
        };
        let mut commands = Vec::new();
        if overview.scanning {
            commands.push(Command::CancelScan);
        }
        commands.extend(
            std::mem::take(&mut self.gallery.decoding)
                .into_keys()
                .map(Command::CancelThumbnail),
        );
        self.gallery.loaded.clear();
        self.view.key_line.keys = focus_keys(self.view.focus, self.view.dir_column.is_some());
        commands
    }

    /// A folder of the running scan with its media files: a section of
    /// placeholders, the MOVs of Live Photos left out.
    pub(super) fn overview_section(
        &mut self,
        scan: u64,
        dir: &Path,
        files: Vec<PathBuf>,
    ) -> Vec<Command> {
        let Some(overview) = self
            .view
            .overview
            .as_mut()
            .filter(|_| scan == self.gallery.scan)
        else {
            return Vec::new();
        };
        let files = folder::without_live_videos(files);
        if files.is_empty() {
            return Vec::new();
        }
        let start = overview.tiles.len();
        overview.tiles.extend(files.into_iter().map(|path| Tile {
            video: folder::is_video(&path),
            path,
            thumbnail: None,
        }));
        overview.sections.push(Section {
            title: title(&overview.root, dir),
            tiles: start..overview.tiles.len(),
        });
        self.overview_refresh()
    }

    /// The running scan found every folder.
    pub(super) fn overview_scanned(&mut self, scan: u64) {
        if let Some(overview) = self.view.overview.as_mut()
            && scan == self.gallery.scan
        {
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

    /// A thumbnail asked for arrived.
    pub(super) fn thumbnail_decoded(
        &mut self,
        path: &Path,
        result: Result<DecodedImage, String>,
    ) -> Vec<Command> {
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
    /// a few at a time. Videos keep their placeholder.
    fn overview_refresh(&mut self) -> Vec<Command> {
        let Self { view, gallery, .. } = self;
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
            if tile.video || tile.thumbnail.is_some() || gallery.decoding.contains_key(&tile.path) {
                continue;
            }
            gallery.decoding.insert(tile.path.clone(), index);
            commands.push(Command::DecodeThumbnail(tile.path.clone()));
        }
        commands
    }
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
