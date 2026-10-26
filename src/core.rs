//! The app core: every behaviour of the viewer as a state machine. It consumes
//! [`Event`]s, keeps the [`View`] the UI draws, and asks the outside world for
//! work through [`Command`]s. It does no I/O itself.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::decode::DecodedImage;
use crate::folder::{self, Found};
use crate::rotate::Rotation;
use crate::settings::Settings;
use crate::startup::NoFile;

mod search;

use search::Search;

#[derive(Debug, Clone, Copy)]
enum Step {
    Previous,
    Next,
}

/// A key as the core understands it. The platform's primary modifier (`Ctrl` on
/// Linux, `Cmd` on macOS) arrives already mapped to [`KeyPress::command`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    F,
    Q,
    Escape,
    Left,
    Right,
    Plus,
    Minus,
    Num0,
    Num1,
    N,
    Tab,
    Up,
    Down,
    Enter,
    Backspace,
    Period,
    R,
    B,
    S,
    O,
    L,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyPress {
    pub key: Key,
    pub command: bool,
    pub shift: bool,
    pub alt: bool,
}

impl KeyPress {
    pub fn plain(key: Key) -> Self {
        Self {
            key,
            command: false,
            shift: false,
            alt: false,
        }
    }

    pub fn command(key: Key) -> Self {
        Self {
            command: true,
            ..Self::plain(key)
        }
    }
}

/// A size or an offset in screen points.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Points {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Key(KeyPress),
    /// A decode asked for with [`Command::Decode`] finished.
    Decoded {
        path: PathBuf,
        result: Result<DecodedImage, String>,
    },
    /// The first frame showing an image (or its error) reached the screen.
    FirstFrameShown,
    /// A listing asked for with [`Command::ListFolder`] finished, or the folder
    /// watched with [`Command::WatchFolder`] changed; the files are in natural
    /// order.
    FolderListed {
        dir: PathBuf,
        result: Result<Vec<PathBuf>, String>,
    },
    /// A thumb of the band was clicked.
    Show(PathBuf),
    /// The size of the area the image is shown in.
    Viewport(Points),
    /// The mouse wheel turned by `delta` points, positive away from the user,
    /// with the pointer at `pointer` from the viewport's centre.
    Wheel {
        delta: f32,
        pointer: Points,
    },
    /// The image was dragged by this many points.
    Pan(Points),
    /// A listing asked for with [`Command::ListDirs`] finished; the folders are
    /// in natural order.
    DirsListed {
        dir: PathBuf,
        result: Result<Vec<PathBuf>, String>,
    },
    /// A search asked for with [`Command::Search`] finished; `found` is absent
    /// when no folder below `dir` has media files.
    Searched {
        dir: PathBuf,
        dots: bool,
        found: Option<Found>,
    },
    /// The settings asked for with [`Command::LoadSettings`], or why the file
    /// cannot be used.
    SettingsLoaded(Result<Settings, String>),
    /// The settings screen changed a value.
    SettingsChanged(Settings),
    /// A [`Command::SaveSettings`] finished.
    SettingsSaved(Result<(), String>),
    /// A [`Command::OpenExternally`] failed to start the program, or the
    /// program reported failure; the message says which and why.
    OpenFailed(String),
    /// A [`Command::WriteRotation`] finished.
    RotationWritten {
        path: PathBuf,
        result: Result<(), String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Decode(PathBuf),
    /// The result of an earlier [`Command::Decode`] is no longer needed; it
    /// reports nothing once cancelled.
    CancelDecode(PathBuf),
    /// List the media files of a folder, answered by [`Event::FolderListed`].
    ListFolder(PathBuf),
    /// Watch a folder, replacing the folder watched so far, and answer every
    /// change in it with a fresh [`Event::FolderListed`].
    WatchFolder(PathBuf),
    /// List the subfolders of a folder, answered by [`Event::DirsListed`].
    ListDirs(PathBuf),
    /// Search the folders below `dir` breadth-first for the shallowest one with
    /// media files, dot folders only with `dots`, replacing any search so far;
    /// answered by [`Event::Searched`].
    Search {
        dir: PathBuf,
        dots: bool,
    },
    /// The running search is no longer needed; it reports nothing once
    /// cancelled.
    CancelSearch,
    /// Read the settings file, answered by [`Event::SettingsLoaded`].
    LoadSettings,
    /// Write the settings file, answered by [`Event::SettingsSaved`].
    SaveSettings(Settings),
    /// Open the file in another program, `app` or else the system default,
    /// without waiting for it; only a failure is answered, by
    /// [`Event::OpenFailed`].
    OpenExternally {
        path: PathBuf,
        app: Option<String>,
    },
    /// Turn the image in the file by `rotation` from how it is shown,
    /// answered by [`Event::RotationWritten`].
    WriteRotation {
        path: PathBuf,
        rotation: Rotation,
    },
    SetFullscreen(bool),
    Quit,
}

/// What the main area shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    /// Waiting for the decode of `path`.
    Loading {
        path: PathBuf,
    },
    /// Waiting for the listing of the folder just entered.
    Listing {
        dir: PathBuf,
    },
    Image {
        path: PathBuf,
        image: DecodedImage,
    },
    /// `path` is absent when there was nothing to open at all.
    Error {
        path: Option<PathBuf>,
        message: String,
    },
    /// The previewed subfolder has no media files directly in it.
    NoImages {
        dir: PathBuf,
        /// How many subfolders it has; absent until they are counted.
        folders: Option<usize>,
    },
    /// Searching the folders below `dir` for images.
    Searching {
        dir: PathBuf,
    },
    /// No folder below `dir` has images.
    NothingBelow {
        dir: PathBuf,
    },
}

/// One entry of the key line: the keys and what they do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyHint {
    pub keys: &'static str,
    pub action: &'static str,
}

/// Why the last key did nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notice {
    FirstImage,
    LastImage,
}

/// The bottom line: the keys of the focused area and an optional notice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyLine {
    pub keys: Vec<KeyHint>,
    pub notice: Option<Notice>,
}

const QUIT_KEYS: &str = if cfg!(target_os = "macos") {
    "Cmd+Q"
} else {
    "Ctrl+Q"
};

const IMAGE_VIEW_KEYS: &[KeyHint] = &[
    KeyHint {
        keys: "Left/Right",
        action: "previous/next",
    },
    KeyHint {
        keys: "+/-",
        action: "zoom",
    },
    KeyHint {
        keys: "0",
        action: "fit",
    },
    KeyHint {
        keys: "1",
        action: "100 %",
    },
    KeyHint {
        keys: "L/R",
        action: "rotate",
    },
    KeyHint {
        keys: "Enter",
        action: "write rotation",
    },
    KeyHint {
        keys: "O",
        action: "open in another app",
    },
    KeyHint {
        keys: "N",
        action: "folders",
    },
    KeyHint {
        keys: "B",
        action: "band",
    },
    KeyHint {
        keys: "Tab",
        action: "band",
    },
];

const DIR_COLUMN_KEYS: &[KeyHint] = &[
    KeyHint {
        keys: "Up/Down",
        action: "select",
    },
    KeyHint {
        keys: "Left/Esc/Backspace",
        action: "parent",
    },
    KeyHint {
        keys: "Right/Enter",
        action: "open",
    },
    KeyHint {
        keys: ".",
        action: "dot entries",
    },
    KeyHint {
        keys: "R",
        action: "find images",
    },
    KeyHint {
        keys: "N",
        action: "close",
    },
    KeyHint {
        keys: "B",
        action: "band",
    },
    KeyHint {
        keys: "Tab",
        action: "image view",
    },
];

const BAND_KEYS: &[KeyHint] = &[
    KeyHint {
        keys: "Left/Right",
        action: "select",
    },
    KeyHint {
        keys: "Enter",
        action: "show",
    },
    KeyHint {
        keys: "Esc",
        action: "back",
    },
];

/// Band keys while the dir column is open.
const BAND_TAB_TO_COLUMN: KeyHint = KeyHint {
    keys: "Tab",
    action: "folders",
};

/// Band keys while the dir column is closed.
const BAND_TAB_TO_IMAGE: KeyHint = KeyHint {
    keys: "Tab",
    action: "image view",
};

/// Keys of every focus area, listed last.
const GLOBAL_KEYS: &[KeyHint] = &[
    KeyHint {
        keys: "S",
        action: "settings",
    },
    KeyHint {
        keys: "F",
        action: "fullscreen",
    },
    KeyHint {
        keys: QUIT_KEYS,
        action: "quit",
    },
];

/// The keys while the settings screen is open.
const SETTINGS_KEYS: &[KeyHint] = &[
    KeyHint {
        keys: "S/Esc",
        action: "close",
    },
    KeyHint {
        keys: QUIT_KEYS,
        action: "quit",
    },
];

/// The area keys go to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Dirs,
    Image,
    Band,
}

/// The keys the key line lists for `focus`.
fn focus_keys(focus: Focus, column_open: bool) -> Vec<KeyHint> {
    let mut keys = match focus {
        Focus::Dirs => DIR_COLUMN_KEYS.to_vec(),
        Focus::Image => IMAGE_VIEW_KEYS.to_vec(),
        Focus::Band => BAND_KEYS.to_vec(),
    };
    if focus == Focus::Band {
        keys.push(if column_open {
            BAND_TAB_TO_COLUMN
        } else {
            BAND_TAB_TO_IMAGE
        });
    }
    keys.extend_from_slice(GLOBAL_KEYS);
    keys
}

/// The column on the left listing the subfolders of the current folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirColumn {
    /// The current folder.
    pub dir: PathBuf,
    /// Its subfolders in natural order; empty until listed.
    pub dirs: Vec<PathBuf>,
    /// The index of the highlighted subfolder, if there is one.
    pub highlight: Option<usize>,
    /// Why the subfolders could not be listed.
    pub error: Option<String>,
}

/// One file of the band.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumb {
    pub path: PathBuf,
    /// Absent when the file failed to decode.
    pub image: Option<DecodedImage>,
    pub current: bool,
}

/// Zoom factor of one `+`/`-` press.
const KEY_ZOOM_STEP: f32 = 1.25;
/// Zoom per point of mouse wheel, as egui zooms with `Ctrl`+wheel.
const WHEEL_ZOOM_SPEED: f32 = 1.0 / 200.0;
const MIN_SCALE: f32 = 1.0 / 32.0;
const MAX_SCALE: f32 = 32.0;

/// How large the image is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Scale {
    /// As large as the viewport allows, but never above 100 %.
    Fit,
    /// Screen points per image pixel.
    Exact(f32),
}

/// Zoom level and position, kept when the image changes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Zoom {
    pub scale: Scale,
    /// The image point shown in the middle of the viewport, as fractions of
    /// the image's width and height. An axis the viewport fully shows is
    /// centred instead.
    pub center_x: f32,
    pub center_y: f32,
}

/// Where an image is drawn, in points from the viewport's top left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl Zoom {
    const FIT: Self = Self {
        scale: Scale::Fit,
        center_x: 0.5,
        center_y: 0.5,
    };

    /// Screen points per image pixel for an image of `size` in `viewport`.
    fn factor(&self, size: Points, viewport: Points) -> f32 {
        match self.scale {
            Scale::Fit => fit_scale(size, viewport),
            Scale::Exact(scale) => scale,
        }
    }

    /// Where `image` is drawn turned by `rotation`.
    pub fn placement(
        &self,
        image: &DecodedImage,
        rotation: Rotation,
        viewport: Points,
    ) -> Placement {
        let size = shown_size(image, rotation);
        let scale = self.factor(size, viewport);
        let width = size.x * scale;
        let height = size.y * scale;
        Placement {
            left: offset(width, viewport.x, self.center_x),
            top: offset(height, viewport.y, self.center_y),
            width,
            height,
        }
    }

    /// The centre actually shown: axes the viewport fully shows are centred.
    fn shown_center(&self, size: Points, viewport: Points) -> (f32, f32) {
        let scale = self.factor(size, viewport);
        let shown = |size: f32, view: f32, center: f32| {
            if size * scale <= view { 0.5 } else { center }
        };
        (
            shown(size.x, viewport.x, self.center_x),
            shown(size.y, viewport.y, self.center_y),
        )
    }
}

/// The size of `image` in pixels as shown turned by `rotation`.
fn shown_size(image: &DecodedImage, rotation: Rotation) -> Points {
    let (width, height) = (image.width as f32, image.height as f32);
    if rotation.sideways() {
        Points {
            x: height,
            y: width,
        }
    } else {
        Points {
            x: width,
            y: height,
        }
    }
}

/// The largest scale that fits an image of `size` into `viewport`, at most
/// 100 %.
fn fit_scale(size: Points, viewport: Points) -> f32 {
    if size.x <= 0.0 || size.y <= 0.0 {
        return 0.0;
    }
    (viewport.x / size.x).min(viewport.y / size.y).min(1.0)
}

/// Where an image side of `size` starts in a viewport side of `view`, with the
/// fraction `center` of it in the middle; centred when it fits.
fn offset(size: f32, view: f32, center: f32) -> f32 {
    if size <= view {
        (view - size) / 2.0
    } else {
        view / 2.0 - center * size
    }
}

/// Everything the UI needs to draw a frame.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub screen: Screen,
    /// The cached files in folder order, broken ones included.
    pub band: Vec<Thumb>,
    pub fullscreen: bool,
    pub key_line: KeyLine,
    pub zoom: Zoom,
    /// The size of the area the image is shown in; zero until the UI reports
    /// it.
    pub viewport: Points,
    /// Why the folder could not be read the last time it was listed.
    pub folder_error: Option<String>,
    pub focus: Focus,
    /// The index of the highlighted thumb while the band has the focus.
    pub band_highlight: Option<usize>,
    /// Absent while the column is closed.
    pub dir_column: Option<DirColumn>,
    /// The settings in effect.
    pub settings: Settings,
    /// Whether the settings screen is open; it gets every key then.
    pub settings_open: bool,
    /// Why the settings file could not be read or written; the defaults are
    /// in effect after a failed read.
    pub settings_error: Option<String>,
    /// Why the last `o` could not open the current file elsewhere.
    pub open_error: Option<String>,
    /// The pending turn of the image shown, not yet written to its file.
    pub rotation: Rotation,
    /// Why the last rotation could not be written.
    pub rotation_error: Option<String>,
}

pub struct Core {
    view: View,
    /// What the main area shows for the current folder; the view shows a
    /// preview instead while there is one.
    screen: Screen,
    /// The band for the current folder; see [`Core::screen`].
    band: Vec<Thumb>,
    first_frame_shown: bool,
    /// The file the main area is about, if any.
    current: Option<PathBuf>,
    /// The folder listed and watched, once the first frame is shown.
    dir: Option<PathBuf>,
    /// The media files of the current file's folder, in natural order; empty
    /// until listed. Dot files only while [`Core::dots`] is set.
    files: Vec<PathBuf>,
    /// The last listing of the folder, dot files included; absent until listed.
    all_files: Option<Vec<PathBuf>>,
    /// The last listing of the dir column's folder, dot folders included;
    /// absent until listed.
    column_dirs: Option<Vec<PathBuf>>,
    /// Whether dot files and dot folders are shown: the one-shot `.` action.
    dots: bool,
    /// The one-shot `r` search of the dir column.
    search: Option<Search>,
    /// Decode results of the files in the window around the current one.
    /// Failures are kept too, so a broken file is not decoded again and again.
    cache: BTreeMap<PathBuf, Result<DecodedImage, String>>,
    /// Decodes asked for and neither answered nor cancelled yet.
    decoding: BTreeSet<PathBuf>,
    /// The subfolder to highlight once the dir column is listed: the folder
    /// just left by going up.
    came_from: Option<PathBuf>,
    /// The folder given on the command line when it had no file to show.
    start_dir: Option<PathBuf>,
    /// The nav cache: the first image of the parent folder and of the
    /// highlighted subfolder, by folder, while the dir column is open.
    nav: BTreeMap<PathBuf, Nav>,
    /// The area that had the focus before the band; the band keeps showing
    /// the dir column's preview or search when it was the column.
    band_from: Focus,
    /// The highlighted file of the band while it has the focus; the current
    /// one when unset or no longer in the band.
    band_highlight: Option<PathBuf>,
    /// The pending turn of the current file; dropped when it changes.
    rotation: Rotation,
    /// The file whose rotation is being written, until it is decoded again.
    writing: Option<PathBuf>,
}

/// What the nav cache knows of a folder.
#[derive(Debug)]
struct Nav {
    state: NavState,
    /// How many subfolders a folder without media has, once counted.
    folders: Option<usize>,
}

#[derive(Debug)]
enum NavState {
    /// Waiting for the listing of the folder.
    Listing,
    /// The folder could not be read.
    Failed(String),
    /// The folder has no media files.
    Empty,
    /// Waiting for the decode of the folder's first file.
    Decoding(PathBuf),
    Done(PathBuf, Result<DecodedImage, String>),
}

impl Nav {
    const LISTING: Self = Self {
        state: NavState::Listing,
        folders: None,
    };

    /// The main area and band previewing `dir` from this entry.
    fn preview(&self, dir: &Path) -> (Screen, Vec<Thumb>) {
        match &self.state {
            NavState::Listing => (
                Screen::Listing {
                    dir: dir.to_path_buf(),
                },
                Vec::new(),
            ),
            NavState::Failed(message) => (
                Screen::Error {
                    path: None,
                    message: cannot_read(dir, message),
                },
                Vec::new(),
            ),
            NavState::Empty => (
                Screen::NoImages {
                    dir: dir.to_path_buf(),
                    folders: self.folders,
                },
                Vec::new(),
            ),
            NavState::Decoding(path) => (Screen::Loading { path: path.clone() }, Vec::new()),
            NavState::Done(path, result) => (
                screen(path, result.clone()),
                vec![Thumb {
                    path: path.clone(),
                    image: result.as_ref().ok().cloned(),
                    current: true,
                }],
            ),
        }
    }
}

impl Core {
    /// Starts on `start`, the file chosen from the command line, or the folder
    /// without one and the reason. Returns the commands to run right away.
    pub fn start(start: Result<PathBuf, NoFile>) -> (Self, Vec<Command>) {
        let current = start.as_ref().ok().cloned();
        let decoding = current.iter().cloned().collect();
        let (screen, commands, start_dir) = match start {
            Ok(path) => (
                Screen::Loading { path: path.clone() },
                vec![Command::Decode(path)],
                None,
            ),
            Err(NoFile { dir, message }) => (
                Screen::Error {
                    path: None,
                    message,
                },
                Vec::new(),
                Some(dir),
            ),
        };
        let core = Self {
            view: View {
                screen: screen.clone(),
                band: Vec::new(),
                fullscreen: false,
                key_line: KeyLine {
                    keys: focus_keys(Focus::Image, false),
                    notice: None,
                },
                zoom: Zoom::FIT,
                viewport: Points::default(),
                folder_error: None,
                focus: Focus::Image,
                band_highlight: None,
                dir_column: None,
                settings: Settings::default(),
                settings_open: false,
                settings_error: None,
                open_error: None,
                rotation: Rotation::None,
                rotation_error: None,
            },
            screen,
            band: Vec::new(),
            first_frame_shown: false,
            current,
            dir: None,
            files: Vec::new(),
            all_files: None,
            column_dirs: None,
            dots: false,
            search: None,
            cache: BTreeMap::new(),
            decoding,
            came_from: None,
            start_dir,
            nav: BTreeMap::new(),
            band_from: Focus::Image,
            band_highlight: None,
            rotation: Rotation::None,
            writing: None,
        };
        (core, commands)
    }

    pub fn view(&self) -> &View {
        &self.view
    }

    pub fn first_frame_shown(&self) -> bool {
        self.first_frame_shown
    }

    pub fn handle(&mut self, event: Event) -> Vec<Command> {
        let before = self.current.clone();
        let mut commands = self.dispatch(event);
        // Leaving a file drops its pending rotation; nothing is written.
        if self.current != before {
            self.rotation = Rotation::None;
            self.view.rotation_error = None;
        }
        commands.extend(self.sync_nav());
        self.publish();
        commands
    }

    fn dispatch(&mut self, event: Event) -> Vec<Command> {
        match event {
            Event::Key(key) => self.key(key),
            Event::Decoded { path, result } => self.decoded(path, result),
            Event::FirstFrameShown => self.first_frame(),
            Event::FolderListed { dir, result } => self.folder_listed(&dir, result),
            Event::Show(path) => self.show(path),
            Event::Viewport(size) => {
                self.view.viewport = size;
                Vec::new()
            }
            Event::Wheel { delta, pointer } => {
                self.zoom_by((delta * WHEEL_ZOOM_SPEED).exp(), pointer);
                Vec::new()
            }
            Event::Pan(delta) => {
                self.pan(delta);
                Vec::new()
            }
            Event::DirsListed { dir, result } => {
                self.dirs_listed(&dir, result);
                Vec::new()
            }
            Event::Searched { dir, dots, found } => self.searched(dir, dots, found),
            Event::SettingsLoaded(Ok(settings)) => self.apply_settings(settings),
            Event::SettingsLoaded(Err(message)) => {
                self.view.settings_error = Some(message);
                Vec::new()
            }
            Event::SettingsChanged(settings) => {
                let settings = settings.clamped();
                if settings == self.view.settings {
                    return Vec::new();
                }
                let mut commands = self.apply_settings(settings.clone());
                commands.push(Command::SaveSettings(settings));
                commands
            }
            Event::SettingsSaved(result) => {
                self.view.settings_error = result.err();
                Vec::new()
            }
            Event::OpenFailed(message) => {
                self.view.open_error = Some(message);
                Vec::new()
            }
            Event::RotationWritten { path, result } => self.rotation_written(path, result),
        }
    }

    /// Puts `settings` in effect: a changed cache size re-plans the window
    /// around the current file.
    fn apply_settings(&mut self, settings: Settings) -> Vec<Command> {
        self.view.settings = settings.clamped();
        self.refresh()
    }

    /// Whether the dir column decides what the main area and the band show:
    /// while it has the focus, or the band has it coming from the column.
    pub(super) fn column_shown(&self) -> bool {
        match self.view.focus {
            Focus::Dirs => true,
            Focus::Band => self.band_from == Focus::Dirs,
            Focus::Image => false,
        }
    }

    /// The highlighted subfolder while the dir column decides what is shown:
    /// the main area and the band preview it.
    fn previewed(&self) -> Option<&Path> {
        if !self.column_shown() {
            return None;
        }
        let column = self.view.dir_column.as_ref()?;
        column
            .highlight
            .and_then(|i| column.dirs.get(i))
            .map(PathBuf::as_path)
    }

    /// Shows the search or the preview if there is one, otherwise the current
    /// folder.
    fn publish(&mut self) {
        let preview = self.search_view().or_else(|| {
            self.previewed()
                .and_then(|dir| Some(self.nav.get(dir)?.preview(dir)))
        });
        // A preview is never the current file, so it is shown unturned.
        self.view.rotation = if preview.is_some() {
            Rotation::None
        } else {
            self.rotation
        };
        (self.view.screen, self.view.band) =
            preview.unwrap_or_else(|| (self.screen.clone(), self.band.clone()));
        self.publish_band_highlight();
    }

    /// Keeps the band highlight on a thumb while the band has the focus: on
    /// the one highlighted so far, otherwise the current one, otherwise the
    /// first.
    fn publish_band_highlight(&mut self) {
        let band = &self.view.band;
        let index = (self.view.focus == Focus::Band)
            .then(|| {
                self.band_highlight
                    .as_ref()
                    .and_then(|h| band.iter().position(|t| t.path == *h))
                    .or_else(|| band.iter().position(|t| t.current))
                    .or((!band.is_empty()).then_some(0))
            })
            .flatten();
        self.band_highlight = index.and_then(|i| band.get(i)).map(|t| t.path.clone());
        self.view.band_highlight = index;
    }

    /// The folders the nav cache holds while the dir column is open: the
    /// highlighted subfolder, the parent, and the folder just left until it is
    /// highlighted.
    fn nav_wanted(&self) -> Vec<PathBuf> {
        let Some(column) = &self.view.dir_column else {
            return Vec::new();
        };
        let highlighted = column.highlight.and_then(|i| column.dirs.get(i)).cloned();
        let parent = column.dir.parent().map(Path::to_path_buf);
        highlighted
            .into_iter()
            .chain(parent)
            .chain(self.came_from.clone())
            .collect()
    }

    /// Fits the nav cache to the folders it should hold: work for the others
    /// is cancelled, missing ones are listed.
    fn sync_nav(&mut self) -> Vec<Command> {
        let wanted = self.nav_wanted();
        let (keep, gone): (BTreeMap<_, _>, BTreeMap<_, _>) = std::mem::take(&mut self.nav)
            .into_iter()
            .partition(|(dir, _)| wanted.contains(dir));
        self.nav = keep;
        let mut commands: Vec<Command> = gone
            .into_values()
            .filter_map(|nav| match nav.state {
                NavState::Decoding(path) => Some(Command::CancelDecode(path)),
                _ => None,
            })
            .collect();
        for dir in wanted {
            if !self.nav.contains_key(&dir) {
                self.nav.insert(dir.clone(), Nav::LISTING);
                commands.push(Command::ListFolder(dir));
            }
        }
        commands
    }

    /// A nav folder listed: its first file is decoded, a folder without media
    /// gets its subfolders counted. Dot files never count here.
    fn nav_listed(&mut self, dir: &Path, result: Result<Vec<PathBuf>, String>) -> Vec<Command> {
        let Some(nav) = self
            .nav
            .get_mut(dir)
            .filter(|n| matches!(n.state, NavState::Listing))
        else {
            return Vec::new();
        };
        match result.map(|files| files.into_iter().find(|f| !folder::is_dot(f))) {
            Ok(Some(first)) => {
                nav.state = NavState::Decoding(first.clone());
                vec![Command::Decode(first)]
            }
            Ok(None) => {
                nav.state = NavState::Empty;
                vec![Command::ListDirs(dir.to_path_buf())]
            }
            Err(message) => {
                nav.state = NavState::Failed(message);
                Vec::new()
            }
        }
    }

    fn nav_decoded(&mut self, path: PathBuf, result: Result<DecodedImage, String>) {
        let waiting = self
            .nav
            .values_mut()
            .find(|n| matches!(&n.state, NavState::Decoding(p) if *p == path));
        if let Some(nav) = waiting {
            nav.state = NavState::Done(path, result);
        }
    }

    /// The first image is on screen: now the settings and the folder may be
    /// read and the folder watched. The watch starts before the listing, so no
    /// change between the two is missed.
    fn first_frame(&mut self) -> Vec<Command> {
        if self.first_frame_shown {
            return Vec::new();
        }
        self.first_frame_shown = true;
        self.dir = self
            .current
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .or_else(|| self.start_dir.take());
        let mut commands = vec![Command::LoadSettings];
        if let Some(dir) = &self.dir {
            commands.extend([
                Command::WatchFolder(dir.clone()),
                Command::ListFolder(dir.clone()),
            ]);
        }
        commands
    }

    /// A folder that cannot be read is reported and leaves the files as they
    /// were; the current image stays, a folder just entered shows the error.
    /// Dot files are dropped unless shown. A current file that vanished from
    /// the folder gives way to the
    /// file after it, or the last one; with no files left the folder's
    /// emptiness is shown until a file appears.
    fn folder_listed(&mut self, dir: &Path, result: Result<Vec<PathBuf>, String>) -> Vec<Command> {
        if self.dir.as_deref() != Some(dir) {
            return self.nav_listed(dir, result);
        }
        let files = match result {
            Ok(files) => files,
            Err(message) => {
                if matches!(self.screen, Screen::Listing { .. }) {
                    self.screen = Screen::Error {
                        path: None,
                        message: cannot_read(dir, &message),
                    };
                }
                self.view.folder_error = Some(message);
                return Vec::new();
            }
        };
        self.view.folder_error = None;
        self.all_files = Some(files.clone());
        let files: Vec<PathBuf> = files
            .into_iter()
            .filter(|f| self.dots || !folder::is_dot(f))
            .collect();
        let vanished = self
            .current
            .as_ref()
            .filter(|c| self.files.contains(c) && !files.contains(c))
            .cloned();
        let empty_before = self.current.is_none();
        self.files = files;
        let replacement = match vanished {
            Some(gone) => {
                let after = |f: &&PathBuf| {
                    folder::natural_cmp(&folder::file_name(f), &folder::file_name(&gone)).is_gt()
                };
                self.files.iter().find(after).or(self.files.last()).cloned()
            }
            None if empty_before => self.files.first().cloned(),
            None => return self.refresh(),
        };
        self.view.key_line.notice = None;
        match replacement {
            Some(path) => self.go_to(path),
            None => {
                self.current = None;
                self.screen = Screen::Error {
                    path: None,
                    message: format!("No images in {}", dir.display()),
                };
                self.refresh()
            }
        }
    }

    /// Keys of every focus area first, then those of the focused one. The
    /// open settings screen takes every key but quit.
    fn key(&mut self, key: KeyPress) -> Vec<Command> {
        if self.view.settings_open {
            match (key.key, key.command) {
                (Key::Q, true) => return vec![Command::Quit],
                (Key::S | Key::Escape, false) => {
                    self.view.settings_open = false;
                    self.view.key_line.keys =
                        focus_keys(self.view.focus, self.view.dir_column.is_some());
                }
                _ => {}
            }
            return Vec::new();
        }
        match (key.key, key.command) {
            (Key::Q, true) => vec![Command::Quit],
            (Key::S, false) => {
                self.view.settings_open = true;
                self.view.key_line.keys = SETTINGS_KEYS.to_vec();
                Vec::new()
            }
            (Key::F, false) => {
                self.view.fullscreen = !self.view.fullscreen;
                vec![Command::SetFullscreen(self.view.fullscreen)]
            }
            (Key::N, false) => self.toggle_column(),
            (Key::B, false) => {
                self.focus_band();
                Vec::new()
            }
            (Key::Tab, false) => {
                match self.view.focus {
                    Focus::Dirs => self.set_focus(Focus::Image),
                    Focus::Image => self.focus_band(),
                    Focus::Band if self.view.dir_column.is_some() => self.set_focus(Focus::Dirs),
                    Focus::Band => self.set_focus(Focus::Image),
                }
                Vec::new()
            }
            (_, true) => Vec::new(),
            _ => match self.view.focus {
                Focus::Dirs => self.dir_key(key.key),
                Focus::Image => self.image_key(key.key),
                Focus::Band => self.band_key(key.key),
            },
        }
    }

    fn set_focus(&mut self, focus: Focus) {
        self.view.focus = focus;
        self.band_highlight = None;
        self.view.key_line.keys = focus_keys(focus, self.view.dir_column.is_some());
    }

    /// Gives the band the focus, its highlight on the current file.
    fn focus_band(&mut self) {
        if self.view.focus != Focus::Band {
            self.band_from = self.view.focus;
            self.set_focus(Focus::Band);
        }
    }

    /// Keys of the band: the arrows move the highlight, `Enter` shows the
    /// highlighted file like a click and returns to the image view, `Esc`
    /// returns to the area the band was focused from.
    fn band_key(&mut self, key: Key) -> Vec<Command> {
        let index = self.view.band_highlight;
        let target = match key {
            Key::Left => index.and_then(|i| i.checked_sub(1)),
            Key::Right => index.and_then(|i| i.checked_add(1)),
            Key::Enter => {
                let path = index
                    .and_then(|i| self.view.band.get(i))
                    .map(|t| t.path.clone());
                let commands = path.map(|p| self.show(p)).unwrap_or_default();
                self.set_focus(Focus::Image);
                return commands;
            }
            Key::Escape => {
                self.set_focus(self.band_from);
                return Vec::new();
            }
            _ => None,
        };
        if let Some(thumb) = target.and_then(|i| self.view.band.get(i)) {
            self.band_highlight = Some(thumb.path.clone());
        }
        Vec::new()
    }

    /// Opens the column on the current folder, focused, or closes it, ending
    /// the one-shot actions, and returns focus to the image view. Nothing to
    /// list before the folder is known.
    fn toggle_column(&mut self) -> Vec<Command> {
        if self.view.dir_column.is_some() {
            let commands = self.reset_one_shot();
            self.view.dir_column = None;
            self.set_focus(Focus::Image);
            return commands;
        }
        let Some(dir) = self.dir.clone() else {
            return Vec::new();
        };
        self.came_from = None;
        self.column_dirs = None;
        self.view.dir_column = Some(DirColumn {
            dir: dir.clone(),
            dirs: Vec::new(),
            highlight: None,
            error: None,
        });
        self.set_focus(Focus::Dirs);
        vec![Command::ListDirs(dir)]
    }

    /// Lists the column's subfolders, keeping the highlight on the folder just
    /// left or the one highlighted so far; or counts a nav folder's subfolders.
    /// Dot folders are dropped unless shown, and never counted.
    fn dirs_listed(&mut self, dir: &Path, result: Result<Vec<PathBuf>, String>) {
        let Some(column) = self.view.dir_column.as_mut().filter(|c| c.dir == dir) else {
            if let (Some(nav), Ok(dirs)) = (self.nav.get_mut(dir), result) {
                nav.folders = Some(dirs.iter().filter(|d| !folder::is_dot(d)).count());
            }
            return;
        };
        self.column_dirs = result.as_ref().ok().cloned();
        match result {
            Ok(dirs) => {
                let dirs: Vec<PathBuf> = dirs
                    .into_iter()
                    .filter(|d| self.dots || !folder::is_dot(d))
                    .collect();
                let highlighted = column.highlight.and_then(|i| column.dirs.get(i)).cloned();
                let index = self
                    .came_from
                    .take()
                    .or(highlighted)
                    .and_then(|keep| dirs.iter().position(|d| *d == keep))
                    .unwrap_or(0);
                column.highlight = (index < dirs.len()).then_some(index);
                column.dirs = dirs;
                column.error = None;
            }
            Err(message) => {
                column.dirs = Vec::new();
                column.highlight = None;
                column.error = Some(message);
            }
        }
    }

    /// Keys of the dir column; moving in it ends the one-shot actions first.
    fn dir_key(&mut self, key: Key) -> Vec<Command> {
        let mut commands = match key {
            Key::Period => {
                let dots = !self.dots;
                return self.set_dots(dots);
            }
            Key::R => return self.start_search(),
            Key::Up
            | Key::Down
            | Key::Left
            | Key::Right
            | Key::Enter
            | Key::Escape
            | Key::Backspace => self.reset_one_shot(),
            _ => Vec::new(),
        };
        commands.extend(self.move_in_column(key));
        commands
    }

    /// Ends the one-shot actions: the search is aborted and dot entries are
    /// hidden again.
    fn reset_one_shot(&mut self) -> Vec<Command> {
        let mut commands = self.abort_search();
        if self.dots {
            commands.extend(self.set_dots(false));
        }
        commands
    }

    /// Shows or hides dot entries in the column and the current folder.
    fn set_dots(&mut self, dots: bool) -> Vec<Command> {
        self.dots = dots;
        let column = self.view.dir_column.as_ref().map(|c| c.dir.clone());
        if let (Some(dir), Some(dirs)) = (column, self.column_dirs.clone()) {
            self.dirs_listed(&dir, Ok(dirs));
        }
        match (self.dir.clone(), self.all_files.clone()) {
            (Some(dir), Some(files)) => self.folder_listed(&dir, Ok(files)),
            _ => Vec::new(),
        }
    }

    fn move_in_column(&mut self, key: Key) -> Vec<Command> {
        let Some(column) = self.view.dir_column.as_mut() else {
            return Vec::new();
        };
        match key {
            Key::Up => {
                if let Some(index) = column.highlight {
                    column.highlight = Some(index.saturating_sub(1));
                }
                Vec::new()
            }
            Key::Down => {
                if let Some(index) = column.highlight
                    && let Some(next) = index.checked_add(1).filter(|i| *i < column.dirs.len())
                {
                    column.highlight = Some(next);
                }
                Vec::new()
            }
            Key::Left | Key::Escape | Key::Backspace => {
                let current = column.dir.clone();
                match current.parent() {
                    Some(parent) => {
                        let parent = parent.to_path_buf();
                        let commands = self.enter(parent);
                        self.came_from = Some(current);
                        commands
                    }
                    None => Vec::new(),
                }
            }
            Key::Right | Key::Enter => {
                let target = column.highlight.and_then(|i| column.dirs.get(i)).cloned();
                match target {
                    Some(dir) => self.enter(dir),
                    None => Vec::new(),
                }
            }
            _ => Vec::new(),
        }
    }

    /// Makes `dir` the current folder, like a start there: all work of the old
    /// folder is cancelled and dropped, and once it is listed its first file
    /// is shown before any neighbour is decoded.
    /// A first image the nav cache holds for `dir` shows at once.
    fn enter(&mut self, dir: PathBuf) -> Vec<Command> {
        // The folder left becomes the parent or a subfolder of the new one, so
        // its first image stays in the nav cache.
        let first = self.files.first().cloned();
        if let (Some(left), Some(first)) = (self.dir.clone(), first) {
            let state = match self.cache.remove(&first) {
                Some(result) => Some(NavState::Done(first, result)),
                None => self
                    .decoding
                    .remove(&first)
                    .then_some(NavState::Decoding(first)),
            };
            if let Some(state) = state {
                let nav = Nav {
                    state,
                    folders: None,
                };
                self.nav.insert(left, nav);
            }
        }
        let mut commands: Vec<Command> = std::mem::take(&mut self.decoding)
            .into_iter()
            .map(Command::CancelDecode)
            .collect();
        self.cache.clear();
        self.files.clear();
        self.all_files = None;
        self.column_dirs = None;
        self.dots = false;
        self.came_from = None;
        self.dir = Some(dir.clone());
        let cached = self.nav.remove(&dir).map(|n| n.state);
        (self.current, self.screen) = match cached {
            Some(NavState::Done(path, result)) => {
                self.cache.insert(path.clone(), result.clone());
                (Some(path.clone()), screen(&path, result))
            }
            Some(NavState::Decoding(path)) => {
                self.decoding.insert(path.clone());
                (Some(path.clone()), Screen::Loading { path })
            }
            _ => (None, Screen::Listing { dir: dir.clone() }),
        };
        self.view.key_line.notice = None;
        self.view.folder_error = None;
        self.view.dir_column = Some(DirColumn {
            dir: dir.clone(),
            dirs: Vec::new(),
            highlight: None,
            error: None,
        });
        commands.extend(self.refresh());
        commands.extend([
            Command::WatchFolder(dir.clone()),
            Command::ListFolder(dir.clone()),
            Command::ListDirs(dir),
        ]);
        commands
    }

    fn image_key(&mut self, key: Key) -> Vec<Command> {
        match key {
            Key::Left => self.step(Step::Previous),
            Key::Right => self.step(Step::Next),
            Key::Plus => {
                self.zoom_by(KEY_ZOOM_STEP, Points::default());
                Vec::new()
            }
            Key::Minus => {
                self.zoom_by(KEY_ZOOM_STEP.recip(), Points::default());
                Vec::new()
            }
            Key::Num0 => {
                self.view.zoom = Zoom::FIT;
                Vec::new()
            }
            Key::Num1 => {
                self.view.zoom.scale = Scale::Exact(1.0);
                Vec::new()
            }
            Key::O => self.open_externally(),
            Key::L => {
                self.rotate(Rotation::left);
                Vec::new()
            }
            Key::R => {
                self.rotate(Rotation::right);
                Vec::new()
            }
            Key::Enter => self.write_rotation(),
            _ => Vec::new(),
        }
    }

    /// The current file while it shows as an image and no rotation of it is
    /// being written.
    fn rotatable(&self) -> Option<&PathBuf> {
        let Screen::Image { path, .. } = &self.screen else {
            return None;
        };
        (self.writing.as_ref() != Some(path)).then_some(path)
    }

    /// Turns the image on screen only, keeping the image point in the middle
    /// of the viewport there.
    fn rotate(&mut self, turn: fn(Rotation) -> Rotation) {
        if self.rotatable().is_none() {
            return;
        }
        let new = turn(self.rotation);
        let zoom = &mut self.view.zoom;
        let (x, y) = (zoom.center_x, zoom.center_y);
        (zoom.center_x, zoom.center_y) = if new == self.rotation.right() {
            (1.0 - y, x)
        } else {
            (y, 1.0 - x)
        };
        self.rotation = new;
        self.view.rotation_error = None;
    }

    /// Writes the pending rotation into the current file.
    fn write_rotation(&mut self) -> Vec<Command> {
        if self.rotation == Rotation::None {
            return Vec::new();
        }
        let Some(path) = self.rotatable().cloned() else {
            return Vec::new();
        };
        self.writing = Some(path.clone());
        self.view.rotation_error = None;
        vec![Command::WriteRotation {
            path,
            rotation: self.rotation,
        }]
    }

    /// A written file is decoded again; its pending rotation goes once the
    /// turned pixels arrive, so the image does not flash unturned. A failed
    /// write keeps the rotation pending and says why.
    fn rotation_written(&mut self, path: PathBuf, result: Result<(), String>) -> Vec<Command> {
        let mut commands = Vec::new();
        match result {
            Ok(()) if self.cache.contains_key(&path) && self.decoding.insert(path.clone()) => {
                commands.push(Command::Decode(path.clone()));
            }
            Ok(()) => {}
            // The message names the file, which may no longer be the current
            // one.
            Err(message) => self.view.rotation_error = Some(message),
        }
        // Without a decode to wait for, the write is over.
        if commands.is_empty() && self.writing.as_ref() == Some(&path) {
            self.writing = None;
        }
        commands
    }

    /// Hands the current file, broken or not, to the app the settings name
    /// for its kind, or the system default when that is blank.
    fn open_externally(&mut self) -> Vec<Command> {
        self.view.open_error = None;
        let Some(path) = self.current.clone() else {
            return Vec::new();
        };
        let settings = &self.view.settings;
        let app = if folder::is_video(&path) {
            &settings.video_app
        } else {
            &settings.image_app
        };
        let app = (!app.trim().is_empty()).then(|| app.clone());
        vec![Command::OpenExternally { path, app }]
    }

    /// The size of the image on screen as shown, if it and the viewport have
    /// a size, so zoom math cannot divide by zero.
    fn zoomable(&self) -> Option<Points> {
        let Screen::Image { image, .. } = &self.view.screen else {
            return None;
        };
        let viewport = self.view.viewport;
        let sized = image.width > 0 && image.height > 0 && viewport.x > 0.0 && viewport.y > 0.0;
        sized.then(|| shown_size(image, self.view.rotation))
    }

    /// Multiplies the scale by `factor`, keeping the image point under
    /// `pointer` (from the viewport's centre) where it is.
    fn zoom_by(&mut self, factor: f32, pointer: Points) {
        let Some(size) = self.zoomable() else {
            return;
        };
        let viewport = self.view.viewport;
        let zoom = self.view.zoom;
        let old = zoom.factor(size, viewport);
        let new = (old * factor).clamp(MIN_SCALE, MAX_SCALE);
        let (x, y) = zoom.shown_center(size, viewport);
        // The image pixel under the pointer, as a fraction, moves by the
        // pointer offset seen at the old minus the new scale.
        let shift = |size: f32, offset: f32| offset * (old.recip() - new.recip()) / size;
        self.view.zoom = Zoom {
            scale: Scale::Exact(new),
            center_x: (x + shift(size.x, pointer.x)).clamp(0.0, 1.0),
            center_y: (y + shift(size.y, pointer.y)).clamp(0.0, 1.0),
        };
    }

    /// Moves the image with the pointer; a fitted image does not move.
    fn pan(&mut self, delta: Points) {
        let Some(size) = self.zoomable() else {
            return;
        };
        let zoom = self.view.zoom;
        let Scale::Exact(scale) = zoom.scale else {
            return;
        };
        let (x, y) = zoom.shown_center(size, self.view.viewport);
        let shift = |size: f32, offset: f32| offset / (scale * size);
        let center_x = (x - shift(size.x, delta.x)).clamp(0.0, 1.0);
        let center_y = (y - shift(size.y, delta.y)).clamp(0.0, 1.0);
        self.view.zoom.center_x = center_x;
        self.view.zoom.center_y = center_y;
    }

    /// Moves to the neighbour file in the folder; at the ends it stays and says
    /// so in the key line.
    fn step(&mut self, step: Step) -> Vec<Command> {
        let Some(index) = self
            .current
            .as_ref()
            .and_then(|c| self.files.iter().position(|f| f == c))
        else {
            return Vec::new();
        };
        let (target, notice) = match step {
            Step::Previous => (index.checked_sub(1), Notice::FirstImage),
            Step::Next => (index.checked_add(1), Notice::LastImage),
        };
        let Some(path) = target.and_then(|i| self.files.get(i)).cloned() else {
            self.view.key_line.notice = Some(notice);
            return Vec::new();
        };
        self.view.key_line.notice = None;
        self.go_to(path)
    }

    /// Jumps to a cached file picked in the band, broken or not.
    fn show(&mut self, path: PathBuf) -> Vec<Command> {
        if let Some(commands) = self.show_found(&path) {
            return commands;
        }
        if !self.cache.contains_key(&path) {
            return Vec::new();
        }
        self.view.key_line.notice = None;
        self.go_to(path)
    }

    /// Makes `path` the current file: a cached one shows at once, otherwise it
    /// shows nothing until its decode arrives.
    fn go_to(&mut self, path: PathBuf) -> Vec<Command> {
        self.current = Some(path.clone());
        self.view.open_error = None;
        let mut commands = Vec::new();
        self.screen = match self.cache.get(&path) {
            Some(result) => screen(&path, result.clone()),
            None => {
                if self.decoding.insert(path.clone()) {
                    commands.push(Command::Decode(path.clone()));
                }
                Screen::Loading { path }
            }
        };
        commands.extend(self.refresh());
        commands
    }

    fn decoded(&mut self, path: PathBuf, result: Result<DecodedImage, String>) -> Vec<Command> {
        let mut commands = self.search_decoded(&path, &result);
        if !self.decoding.remove(&path) {
            self.nav_decoded(path, result);
            return commands;
        }
        let rewritten = self.writing.as_ref() == Some(&path);
        if rewritten {
            self.writing = None;
            if self.current.as_ref() == Some(&path) {
                self.rotation = Rotation::None;
            }
        }
        let waiting = match &self.screen {
            Screen::Loading { path: p } => *p == path,
            Screen::Image { path: p, .. } => rewritten && *p == path,
            _ => false,
        };
        if waiting {
            self.screen = screen(&path, result.clone());
        }
        self.cache.insert(path, result);
        commands.extend(self.refresh());
        commands
    }

    /// The current file and up to `cache_per_side` neighbours on each side, in
    /// folder order; only the current file while the folder is not listed.
    fn window(&self) -> Vec<PathBuf> {
        let Some(current) = &self.current else {
            return Vec::new();
        };
        let Some(index) = self.files.iter().position(|f| f == current) else {
            return vec![current.clone()];
        };
        let first = index.saturating_sub(self.view.settings.cache_per_side);
        let last = index.saturating_add(self.view.settings.cache_per_side);
        let len = last.saturating_sub(first).saturating_add(1);
        self.files.iter().skip(first).take(len).cloned().collect()
    }

    /// The neighbours of the current file, nearest first, next before
    /// previous.
    fn neighbours(&self) -> Vec<PathBuf> {
        let Some(index) = self
            .current
            .as_ref()
            .and_then(|c| self.files.iter().position(|f| f == c))
        else {
            return Vec::new();
        };
        let mut neighbours = Vec::new();
        for distance in 1..=self.view.settings.cache_per_side {
            let next = index.checked_add(distance).and_then(|i| self.files.get(i));
            let previous = index.checked_sub(distance).and_then(|i| self.files.get(i));
            neighbours.extend(next.into_iter().chain(previous).cloned());
        }
        neighbours
    }

    /// Fits cache and decodes to the window around the current file: whatever
    /// left it is dropped or cancelled, and once the current file is on screen
    /// its missing neighbours are asked for.
    fn refresh(&mut self) -> Vec<Command> {
        let window = self.window();
        self.cache.retain(|p, _| window.contains(p));
        let (keep, cancel): (BTreeSet<PathBuf>, BTreeSet<PathBuf>) =
            std::mem::take(&mut self.decoding)
                .into_iter()
                .partition(|p| window.contains(p));
        self.decoding = keep;
        let mut commands: Vec<Command> = cancel.into_iter().map(Command::CancelDecode).collect();
        if !matches!(self.screen, Screen::Loading { .. }) {
            for path in self.neighbours() {
                if !self.cache.contains_key(&path) && self.decoding.insert(path.clone()) {
                    commands.push(Command::Decode(path));
                }
            }
        }
        self.band = window
            .into_iter()
            .filter_map(|path| {
                let image = self.cache.get(&path)?.as_ref().ok().cloned();
                Some(Thumb {
                    image,
                    current: self.current.as_ref() == Some(&path),
                    path,
                })
            })
            .collect();
        commands
    }
}

/// The message for a folder that cannot be read.
fn cannot_read(dir: &Path, message: &str) -> String {
    format!("Cannot read {}: {message}", dir.display())
}

fn screen(path: &Path, result: Result<DecodedImage, String>) -> Screen {
    match result {
        Ok(image) => Screen::Image {
            path: path.to_path_buf(),
            image,
        },
        Err(message) => Screen::Error {
            path: Some(path.to_path_buf()),
            message,
        },
    }
}
