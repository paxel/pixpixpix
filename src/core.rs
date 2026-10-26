//! The app core: every behaviour of the viewer as a state machine. It consumes
//! [`Event`]s, keeps the [`View`] the UI draws, and asks the outside world for
//! work through [`Command`]s. It does no I/O itself.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::decode::{DecodedImage, Frame};
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
    X,
    C,
    Space,
    M,
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
    /// A [`Command::Trash`] finished.
    Trashed {
        path: PathBuf,
        result: Result<(), String>,
    },
    /// A [`Command::Copy`] finished.
    Copied(Result<(), String>),
    /// The file of a successful [`Event::Decoded`], sent right after it, is
    /// an animation; that event showed its first frame.
    Animated(PathBuf),
    /// A [`Command::DecodeFrames`] finished.
    FramesDecoded {
        path: PathBuf,
        result: Result<Vec<Frame>, String>,
    },
    /// The time has come for the frame [`View::next_frame_in`] announced.
    Tick(Instant),
    /// The file of a successful [`Event::Decoded`], sent right after it,
    /// holds `count` images, more than one; that event showed the first.
    Images {
        path: PathBuf,
        count: usize,
    },
    /// A [`Command::DecodeInner`] finished.
    InnerDecoded {
        path: PathBuf,
        index: usize,
        result: Result<DecodedImage, String>,
    },
    /// The shown image was clicked.
    Clicked,
    /// The video [`Command::PlayVideo`] plays drew a frame.
    VideoFrame {
        path: PathBuf,
        image: DecodedImage,
    },
    /// The video [`Command::PlayVideo`] plays reached its end.
    VideoEnded(PathBuf),
    /// The video [`Command::PlayVideo`] was asked to play cannot be played;
    /// the message is mpv's.
    VideoFailed {
        path: PathBuf,
        message: String,
    },
    /// A step button of the video was clicked.
    StepVideo(VideoStep),
    /// The [`Command::SeekVideo`] running on the video at the path drew its
    /// frame, or failed.
    VideoSeeked(PathBuf),
    /// The file a [`Command::FindMotion`] looked at is a Motion Photo; other
    /// files are not reported.
    MotionPhoto(PathBuf),
}

/// A step through a video.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoStep {
    /// Back or forward by whole seconds.
    Seconds(i32),
    /// Back or forward by frames; one at a time in [`Command::SeekVideo`].
    Frames(i32),
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
    /// Move the file to the system trash, answered by [`Event::Trashed`].
    Trash(PathBuf),
    /// Put the file and its pixels, turned by `rotation`, on the clipboard,
    /// answered by [`Event::Copied`].
    Copy {
        path: PathBuf,
        image: DecodedImage,
        rotation: Rotation,
    },
    /// Decode all frames of an animation, replacing the one decoded so far;
    /// answered by [`Event::FramesDecoded`].
    DecodeFrames(PathBuf),
    /// The running [`Command::DecodeFrames`] is no longer needed; it reports
    /// nothing once cancelled.
    CancelFrames,
    /// Decode image `index` of a multi-image file, counted from 0 for the one
    /// [`Command::Decode`] gives, replacing any such decode running; answered
    /// by [`Event::InnerDecoded`].
    DecodeInner {
        path: PathBuf,
        index: usize,
    },
    /// The running [`Command::DecodeInner`] is no longer needed; it reports
    /// nothing once cancelled.
    CancelInner,
    /// Play the video from the start, with sound unless `muted`, replacing
    /// any video playing; answered by [`Event::VideoFrame`] for each frame,
    /// then [`Event::VideoEnded`] or [`Event::VideoFailed`].
    PlayVideo {
        path: PathBuf,
        muted: bool,
        /// Start paused on the first frame instead.
        paused: bool,
    },
    /// Seek the video of [`Command::PlayVideo`] by the step, a frame step
    /// pausing it; answered by [`Event::VideoSeeked`] once the frame there is
    /// drawn. Never sent while one runs.
    SeekVideo(VideoStep),
    /// Pause the playing video, keeping its frame.
    PauseVideo,
    /// Play the paused video on.
    ResumeVideo,
    /// Mute the playing video, or turn its sound back on.
    MuteVideo(bool),
    /// The video player is no longer needed; it reports nothing more.
    StopVideo,
    /// Find out whether the JPEG is a Motion Photo, answered by
    /// [`Event::MotionPhoto`] if it is.
    FindMotion(PathBuf),
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

/// Why the last key did nothing, or what it did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notice {
    FirstImage,
    LastImage,
    /// The last `c` put the image on the clipboard.
    Copied,
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
        keys: "C",
        action: "copy",
    },
    KeyHint {
        keys: "O",
        action: "open in another app",
    },
    KeyHint {
        keys: "X",
        action: "trash",
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

/// Image view keys of a multi-image file, listed after the first.
const INNER_KEYS: KeyHint = KeyHint {
    keys: "Up/Down",
    action: "images in the file",
};

/// Image view keys of a Live Photo or Motion Photo whose video is not
/// started, listed after the first.
const MOTION_KEYS: KeyHint = KeyHint {
    keys: "Space",
    action: "play motion",
};

/// Image view keys of a video, listed after the first; they say what a press
/// does now.
fn video_keys(video: &Video) -> [KeyHint; 5] {
    [
        KeyHint {
            keys: "Space",
            action: if video.playing { "pause" } else { "play" },
        },
        KeyHint {
            keys: "M",
            action: if video.muted { "unmute" } else { "mute" },
        },
        KeyHint {
            keys: "Shift+Left/Right",
            action: "10 s",
        },
        KeyHint {
            keys: if cfg!(target_os = "macos") {
                "Cmd+Left/Right"
            } else {
                "Ctrl+Left/Right"
            },
            action: "30 s",
        },
        KeyHint {
            keys: "Alt+Left/Right",
            action: "frame",
        },
    ]
}

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
    /// The file is an animation.
    pub animated: bool,
    /// How many images the file holds; more than one for a multi-image file.
    pub images: usize,
    /// The file is a video, shown by its first frame.
    pub video: bool,
    /// The file is a photo with a video of the moment.
    pub motion: Option<MotionKind>,
}

/// The kind of a photo with a video of the moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionKind {
    /// An Apple Live Photo: the photo and a MOV of the same name.
    LivePhoto,
    /// A Google or Samsung Motion Photo: a JPEG with the video inside.
    MotionPhoto,
}

/// The video shown in the main area.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Video {
    pub playing: bool,
    pub muted: bool,
    /// Why it cannot be played, as mpv says.
    pub error: Option<String>,
}

/// Which of the images of a multi-image file is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InnerImages {
    /// Counted from 0.
    pub index: usize,
    pub count: usize,
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
    /// Why the last file could not be moved to the trash.
    pub trash_error: Option<String>,
    /// Why the last `c` could not put the image on the clipboard.
    pub copy_error: Option<String>,
    /// While the main area plays an animation: when its next frame is due,
    /// to be announced with [`Event::Tick`]. Zero before the first tick.
    pub next_frame_in: Option<Duration>,
    /// While the main area shows a multi-image file: which of its images.
    pub inner: Option<InnerImages>,
    /// While the main area shows a video, or plays the video of a Live Photo
    /// or Motion Photo.
    pub video: Option<Video>,
    /// While the main area shows a Live Photo or Motion Photo.
    pub motion: Option<MotionKind>,
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
    /// Files sent to the trash and not reported back yet; listings leave them
    /// out, so a listing taken before the trash cannot bring them back.
    trashing: BTreeSet<PathBuf>,
    /// Files whose last decode found an animation.
    animated: BTreeSet<PathBuf>,
    /// The animation of the current file, once the first frame is shown.
    animation: Option<Animation>,
    /// How many images the files whose last decode found more than one hold.
    images: BTreeMap<PathBuf, usize>,
    /// The image shown of the current file while it holds more than one.
    inner: Option<Inner>,
    /// The current file while it is a video shown by a frame.
    video: Option<CurrentVideo>,
    /// Whether videos play without sound; kept from video to video.
    muted: bool,
    /// The Live Photos of the folder from its last listing, photo to MOV; the
    /// MOVs are left out of [`Core::files`].
    live: BTreeMap<PathBuf, PathBuf>,
    /// Cached JPEGs asked about being a Motion Photo.
    probed: BTreeSet<PathBuf>,
    /// The cached files found to be Motion Photos.
    motion_photos: BTreeSet<PathBuf>,
}

/// The video of the main area.
#[derive(Debug)]
struct CurrentVideo {
    /// The file shown.
    path: PathBuf,
    /// What plays: the file itself, or the MOV of a Live Photo.
    source: PathBuf,
    /// The file is a Live Photo or Motion Photo, a still until played.
    motion: bool,
    /// A player was asked for and has not ended or failed.
    started: bool,
    playing: bool,
    /// Why it could not be played.
    error: Option<String>,
    /// A [`Command::SeekVideo`] runs.
    seeking: bool,
    /// The step still to take once the running seek is done.
    pending: Option<VideoStep>,
}

impl CurrentVideo {
    /// The seek to run now: none while one runs; frames one at a time.
    fn next_seek(&mut self) -> Option<Command> {
        if self.seeking {
            return None;
        }
        let (now, left) = match self.pending.take()? {
            VideoStep::Seconds(0) | VideoStep::Frames(0) => return None,
            VideoStep::Seconds(s) => (VideoStep::Seconds(s), None),
            VideoStep::Frames(f) => {
                let one = f.signum();
                let rest = f.saturating_sub(one);
                (
                    VideoStep::Frames(one),
                    (rest != 0).then_some(VideoStep::Frames(rest)),
                )
            }
        };
        self.pending = left;
        self.seeking = true;
        Some(Command::SeekVideo(now))
    }
}

impl VideoStep {
    /// The same step the other way.
    fn back(self) -> Self {
        match self {
            Self::Seconds(s) => Self::Seconds(s.saturating_neg()),
            Self::Frames(f) => Self::Frames(f.saturating_neg()),
        }
    }
}

/// The image shown of a multi-image file.
#[derive(Debug)]
struct Inner {
    path: PathBuf,
    count: usize,
    index: usize,
    /// Its decode is running; the image shown before stays meanwhile.
    decoding: bool,
}

/// Shorter frame delays than this are taken as [`SLOW_DELAY`], as browsers do.
const MIN_DELAY: Duration = Duration::from_millis(20);
const SLOW_DELAY: Duration = Duration::from_millis(100);

/// The animation of the current file.
#[derive(Debug)]
enum Animation {
    /// Waiting for its frames; the first one is shown meanwhile.
    Decoding(PathBuf),
    /// Looping since the first tick after the frames arrived.
    Playing {
        path: PathBuf,
        frames: Vec<Frame>,
        start: Option<Instant>,
        /// When the frame shown gives way, from the last tick.
        next_in: Duration,
    },
    /// Its frames could not be decoded; the first one stays.
    Still(PathBuf),
}

impl Animation {
    fn path(&self) -> &Path {
        match self {
            Self::Decoding(path) | Self::Playing { path, .. } | Self::Still(path) => path,
        }
    }
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
                    animated: false,
                    images: 1,
                    video: false,
                    motion: None,
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
                trash_error: None,
                copy_error: None,
                next_frame_in: None,
                inner: None,
                video: None,
                motion: None,
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
            trashing: BTreeSet::new(),
            animated: BTreeSet::new(),
            animation: None,
            images: BTreeMap::new(),
            inner: None,
            video: None,
            muted: false,
            live: BTreeMap::new(),
            probed: BTreeSet::new(),
            motion_photos: BTreeSet::new(),
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
        commands.extend(self.sync_animation());
        commands.extend(self.sync_inner());
        commands.extend(self.sync_motion());
        commands.extend(self.sync_video());
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
            Event::Trashed { path, result } => self.trashed(&path, result),
            Event::Copied(result) => {
                match result {
                    Ok(()) => self.view.key_line.notice = Some(Notice::Copied),
                    Err(message) => self.view.copy_error = Some(message),
                }
                Vec::new()
            }
            Event::Animated(path) => {
                self.animated.insert(path);
                Vec::new()
            }
            Event::FramesDecoded { path, result } => {
                self.frames_decoded(path, result);
                Vec::new()
            }
            Event::Tick(now) => {
                self.tick(now);
                Vec::new()
            }
            Event::Images { path, count } => {
                self.images.insert(path, count);
                Vec::new()
            }
            Event::InnerDecoded {
                path,
                index,
                result,
            } => {
                self.inner_decoded(&path, index, result);
                Vec::new()
            }
            Event::Clicked if self.view.video.is_some() => self.toggle_play(),
            Event::Clicked => Vec::new(),
            Event::VideoFrame { path, image } => {
                self.video_frame(path, image);
                Vec::new()
            }
            Event::VideoEnded(path) => self.video_stopped(&path, None),
            Event::VideoFailed { path, message } => self.video_stopped(&path, Some(message)),
            Event::StepVideo(step) => self.step_video(step),
            Event::VideoSeeked(path) => self.video_seeked(&path),
            Event::MotionPhoto(path) => {
                if self.probed.contains(&path) {
                    self.motion_photos.insert(path);
                }
                Vec::new()
            }
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
        self.view.inner = match (&preview, &self.inner) {
            (None, Some(inner)) => Some(InnerImages {
                index: inner.index,
                count: inner.count,
            }),
            _ => None,
        };
        self.view.next_frame_in = match (&preview, &self.animation) {
            (None, Some(Animation::Playing { next_in, .. })) => Some(*next_in),
            _ => None,
        };
        // A still of a Live Photo or Motion Photo shows no player.
        self.view.video = match (&preview, &self.video) {
            (None, Some(video)) if !video.motion || video.started || video.error.is_some() => {
                Some(Video {
                    playing: video.playing,
                    muted: self.muted,
                    error: video.error.clone(),
                })
            }
            _ => None,
        };
        let shown = match (&preview, &self.screen) {
            (None, Screen::Image { path, .. }) => Some(path),
            _ => None,
        };
        self.view.motion = shown.and_then(|path| self.motion_kind(path));
        (self.view.screen, self.view.band) =
            preview.unwrap_or_else(|| (self.screen.clone(), self.band.clone()));
        for thumb in &mut self.view.band {
            thumb.animated = thumb.image.is_some() && self.animated.contains(&thumb.path);
            thumb.images = match (&thumb.image, self.images.get(&thumb.path)) {
                (Some(_), Some(count)) => *count,
                _ => 1,
            };
            thumb.video = thumb.image.is_some() && folder::is_video(&thumb.path);
        }
        let kinds: Vec<Option<MotionKind>> = self
            .view
            .band
            .iter()
            .map(|t| t.image.as_ref().and_then(|_| self.motion_kind(&t.path)))
            .collect();
        for (thumb, kind) in self.view.band.iter_mut().zip(kinds) {
            thumb.motion = kind;
        }
        if self.view.focus == Focus::Image && !self.view.settings_open {
            let mut keys = focus_keys(Focus::Image, self.view.dir_column.is_some());
            if self.view.inner.is_some() {
                keys.insert(1, INNER_KEYS);
            }
            if let Some(video) = &self.view.video {
                keys.splice(1..1, video_keys(video));
            } else if self.view.motion.is_some() {
                keys.insert(1, MOTION_KEYS);
            }
            if self.plays_video() {
                keys.retain(|k| !matches!(k.keys, "L/R" | "Enter"));
            }
            self.view.key_line.keys = keys;
        }
        self.publish_band_highlight();
    }

    /// What kind of photo with a video of the moment `path` is, if any.
    fn motion_kind(&self, path: &Path) -> Option<MotionKind> {
        if self.live.contains_key(path) {
            Some(MotionKind::LivePhoto)
        } else if self.motion_photos.contains(path) {
            Some(MotionKind::MotionPhoto)
        } else {
            None
        }
    }

    /// Whether the main area shows a video or plays the video of a photo, so
    /// there is nothing to turn.
    fn plays_video(&self) -> bool {
        self.video.as_ref().is_some_and(|v| !v.motion || v.started)
    }

    /// Once the first image is on screen, asks of every cached JPEG whether
    /// it is a Motion Photo, once while it stays cached; a Live Photo is not
    /// asked.
    fn sync_motion(&mut self) -> Vec<Command> {
        let cache = &self.cache;
        self.probed.retain(|p| cache.contains_key(p));
        self.motion_photos.retain(|p| cache.contains_key(p));
        if !self.first_frame_shown {
            return Vec::new();
        }
        let wanted: Vec<PathBuf> = cache
            .iter()
            .filter(|(path, result)| {
                result.is_ok()
                    && folder::is_jpeg(path)
                    && !self.live.contains_key(*path)
                    && !self.probed.contains(*path)
            })
            .map(|(path, _)| path.clone())
            .collect();
        self.probed.extend(wanted.iter().cloned());
        wanted.into_iter().map(Command::FindMotion).collect()
    }

    /// Fits the inner images to the current file: one holding several starts
    /// at its first; the decode of an image of a file left is cancelled.
    fn sync_inner(&mut self) -> Vec<Command> {
        let wanted = self
            .current
            .as_ref()
            .and_then(|path| Some((path, *self.images.get(path)?)));
        let kept = self.inner.as_ref().map(|i| (&i.path, i.count));
        if kept == wanted {
            return Vec::new();
        }
        let wanted = wanted.map(|(path, count)| Inner {
            path: path.clone(),
            count,
            index: 0,
            decoding: false,
        });
        self.leave_inner(wanted)
    }

    /// Replaces the inner images, cancelling a decode for the old ones.
    fn leave_inner(&mut self, new: Option<Inner>) -> Vec<Command> {
        match std::mem::replace(&mut self.inner, new) {
            Some(Inner { decoding: true, .. }) => vec![Command::CancelInner],
            _ => Vec::new(),
        }
    }

    /// Steps through the images of a multi-image file, stopping at the ends:
    /// the first shows from the cache at once, any other once decoded.
    fn step_inner(&mut self, step: Step) -> Vec<Command> {
        let Some(inner) = self.inner.as_mut() else {
            return Vec::new();
        };
        let target = match step {
            Step::Previous => inner.index.checked_sub(1),
            Step::Next => inner.index.checked_add(1).filter(|i| *i < inner.count),
        };
        let Some(index) = target else {
            return Vec::new();
        };
        inner.index = index;
        let path = inner.path.clone();
        if let Some(result) = self.cache.get(&path).filter(|_| index == 0) {
            self.screen = screen(&path, result.clone());
            let cancel = std::mem::replace(&mut inner.decoding, false);
            return if cancel {
                vec![Command::CancelInner]
            } else {
                Vec::new()
            };
        }
        inner.decoding = true;
        vec![Command::DecodeInner { path, index }]
    }

    /// An image of a multi-image file arrived: it shows if it is still the
    /// one wanted.
    fn inner_decoded(&mut self, path: &Path, index: usize, result: Result<DecodedImage, String>) {
        let Some(inner) = self
            .inner
            .as_mut()
            .filter(|i| i.decoding && i.path == path && i.index == index)
        else {
            return;
        };
        inner.decoding = false;
        self.screen = screen(path, result);
    }

    /// Fits the video to the current file: a video shown by its first frame
    /// may be played; the player of a file left is stopped.
    fn sync_video(&mut self) -> Vec<Command> {
        let wanted = self
            .current
            .as_ref()
            .filter(
                |current| matches!(&self.screen, Screen::Image { path, .. } if path == *current),
            )
            .and_then(|path| {
                let source = if folder::is_video(path) || self.motion_photos.contains(path) {
                    path
                } else {
                    self.live.get(path)?
                };
                Some((path, source))
            });
        if self.video.as_ref().map(|v| (&v.path, &v.source)) == wanted {
            return Vec::new();
        }
        let new = wanted.map(|(path, source)| CurrentVideo {
            path: path.clone(),
            source: source.clone(),
            motion: !folder::is_video(path),
            started: false,
            playing: false,
            error: None,
            seeking: false,
            pending: None,
        });
        match std::mem::replace(&mut self.video, new) {
            Some(CurrentVideo { started: true, .. }) => vec![Command::StopVideo],
            _ => Vec::new(),
        }
    }

    /// Plays or pauses the video shown; the first play starts a player.
    fn toggle_play(&mut self) -> Vec<Command> {
        let Some(video) = self.video.as_mut() else {
            return Vec::new();
        };
        video.error = None;
        video.playing = !video.playing;
        // A frame step would pause again right after resuming.
        if video.playing && matches!(video.pending, Some(VideoStep::Frames(_))) {
            video.pending = None;
        }
        match (video.playing, video.started) {
            (false, _) => vec![Command::PauseVideo],
            (true, true) => vec![Command::ResumeVideo],
            (true, false) => {
                video.started = true;
                vec![Command::PlayVideo {
                    path: video.source.clone(),
                    muted: self.muted,
                    paused: false,
                }]
            }
        }
    }

    /// Steps through the video shown: at once when no seek runs, else once the
    /// running one is done. A video never started gets a paused player first;
    /// a frame step pauses.
    fn step_video(&mut self, step: VideoStep) -> Vec<Command> {
        let muted = self.muted;
        let Some(video) = self.video.as_mut() else {
            return Vec::new();
        };
        if matches!(step, VideoStep::Frames(_)) {
            video.playing = false;
        }
        let mut commands = Vec::new();
        if !video.started {
            video.started = true;
            video.error = None;
            commands.push(Command::PlayVideo {
                path: video.source.clone(),
                muted,
                paused: true,
            });
        }
        video.pending = Some(match (video.pending, step) {
            (Some(VideoStep::Seconds(a)), VideoStep::Seconds(b)) => {
                VideoStep::Seconds(a.saturating_add(b))
            }
            (Some(VideoStep::Frames(a)), VideoStep::Frames(b)) => {
                VideoStep::Frames(a.saturating_add(b))
            }
            _ => step,
        });
        commands.extend(video.next_seek());
        commands
    }

    /// The running seek is done; the step waiting, if any, is taken.
    fn video_seeked(&mut self, path: &Path) -> Vec<Command> {
        let Some(video) = self
            .video
            .as_mut()
            .filter(|v| v.started && v.seeking && v.source == path)
        else {
            return Vec::new();
        };
        video.seeking = false;
        video.next_seek().into_iter().collect()
    }

    /// Turns the sound of videos off or on while a video is shown.
    fn toggle_mute(&mut self) -> Vec<Command> {
        // A still Live or Motion Photo has no sound to mute.
        let Some(video) = self.video.as_ref().filter(|v| !v.motion || v.started) else {
            return Vec::new();
        };
        self.muted = !self.muted;
        if video.started {
            vec![Command::MuteVideo(self.muted)]
        } else {
            Vec::new()
        }
    }

    /// A frame of the video playing replaces the one shown.
    fn video_frame(&mut self, path: PathBuf, image: DecodedImage) {
        let Some(video) = self
            .video
            .as_ref()
            .filter(|v| v.started && v.source == path)
        else {
            return;
        };
        if let Screen::Image {
            path: p,
            image: shown,
        } = &mut self.screen
            && *p == video.path
        {
            *shown = image;
        }
    }

    /// The video played to its end, or failed with `error`: it shows its
    /// first frame, or the still of its photo, again, ready to play anew.
    fn video_stopped(&mut self, source: &Path, error: Option<String>) -> Vec<Command> {
        let Some(video) = self
            .video
            .as_mut()
            .filter(|v| v.started && v.source == source)
        else {
            return Vec::new();
        };
        let path = video.path.as_path();
        video.started = false;
        video.playing = false;
        video.error = error;
        video.seeking = false;
        video.pending = None;
        if let Some(Ok(image)) = self.cache.get(path) {
            self.screen = Screen::Image {
                path: path.to_path_buf(),
                image: image.clone(),
            };
        }
        vec![Command::StopVideo]
    }

    /// Fits the animation to the current file: an animated one shown as an
    /// image gets its frames decoded once the first frame is on screen; the
    /// frames of a file left are dropped, their decode cancelled.
    fn sync_animation(&mut self) -> Vec<Command> {
        let wanted = self.current.clone().filter(|current| {
            self.first_frame_shown
                && self.animated.contains(current)
                && matches!(&self.screen, Screen::Image { path, .. } if path == current)
        });
        if self.animation.as_ref().map(Animation::path) == wanted.as_deref() {
            return Vec::new();
        }
        let mut commands = Vec::new();
        if let Some(Animation::Decoding(_)) = self.animation.take() {
            commands.push(Command::CancelFrames);
        }
        if let Some(path) = wanted {
            commands.push(Command::DecodeFrames(path.clone()));
            self.animation = Some(Animation::Decoding(path));
        }
        commands
    }

    /// The frames of the current file arrived: they play from the next tick,
    /// the first being the image already shown. Without a second frame the
    /// first one stays.
    fn frames_decoded(&mut self, path: PathBuf, result: Result<Vec<Frame>, String>) {
        if !matches!(&self.animation, Some(Animation::Decoding(p)) if *p == path) {
            return;
        }
        self.animation = Some(match result {
            Ok(mut frames) if frames.len() > 1 => {
                for frame in &mut frames {
                    if frame.delay < MIN_DELAY {
                        frame.delay = SLOW_DELAY;
                    }
                }
                // The first frame is not held twice.
                if let (Some(first), Some(Ok(shown))) = (frames.first_mut(), self.cache.get(&path))
                {
                    first.image = shown.clone();
                }
                Animation::Playing {
                    path,
                    frames,
                    start: None,
                    next_in: Duration::ZERO,
                }
            }
            _ => Animation::Still(path),
        });
    }

    /// Shows the frame due at `now`: the frames loop, the first tick starts
    /// them.
    fn tick(&mut self, now: Instant) {
        let Some(Animation::Playing {
            path,
            frames,
            start,
            next_in,
        }) = &mut self.animation
        else {
            return;
        };
        let start = *start.get_or_insert(now);
        let total = frames
            .iter()
            .fold(Duration::ZERO, |sum, f| sum.saturating_add(f.delay));
        let elapsed = now.saturating_duration_since(start).as_nanos();
        let mut into = elapsed.checked_rem(total.as_nanos()).unwrap_or(0);
        let mut shown = frames.first();
        *next_in = Duration::ZERO;
        for frame in frames.iter() {
            let delay = frame.delay.as_nanos();
            if into < delay {
                shown = Some(frame);
                let left = delay.saturating_sub(into);
                *next_in = Duration::from_nanos(u64::try_from(left).unwrap_or(u64::MAX));
                break;
            }
            into = into.saturating_sub(delay);
        }
        if let (Some(frame), Screen::Image { path: p, image }) = (shown, &mut self.screen)
            && p == path
        {
            *image = frame.image.clone();
        }
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
    /// Dot files are dropped unless shown, files on their way to the trash
    /// always, the MOV of a Live Photo too. A current file that vanished from
    /// the folder gives way to the file after it, or the last one, a MOV to
    /// its photo; with no files left the folder's emptiness is shown until a
    /// file appears.
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
        let mut files: Vec<PathBuf> = files
            .into_iter()
            .filter(|f| (self.dots || !folder::is_dot(f)) && !self.trashing.contains(f))
            .collect();
        self.live = folder::live_photos(&files);
        files.retain(|f| !self.live.values().any(|mov| mov == f));
        let photo = self.current.as_ref().and_then(|c| {
            let mut pairs = self.live.iter();
            pairs
                .find(|(_, mov)| *mov == c)
                .map(|(photo, _)| photo.clone())
        });
        let vanished = self
            .current
            .as_ref()
            .filter(|c| self.files.contains(c) && !files.contains(c))
            .cloned();
        let empty_before = self.current.is_none();
        self.files = files;
        let replacement = match (photo, vanished) {
            (Some(photo), _) => Some(photo),
            (None, Some(gone)) => {
                let after = |f: &&PathBuf| {
                    folder::natural_cmp(&folder::file_name(f), &folder::file_name(&gone)).is_gt()
                };
                self.files.iter().find(after).or(self.files.last()).cloned()
            }
            (None, None) if empty_before => self.files.first().cloned(),
            (None, None) => return self.refresh(),
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
            (Key::Left | Key::Right, _)
                if self.view.focus == Focus::Image && (key.command || key.shift || key.alt) =>
            {
                let forward = key.key == Key::Right;
                let step = match (key.alt, key.command) {
                    (true, _) => VideoStep::Frames(1),
                    (false, true) => VideoStep::Seconds(30),
                    (false, false) => VideoStep::Seconds(10),
                };
                self.step_video(if forward { step } else { step.back() })
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
        self.live.clear();
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
            Key::Up => self.step_inner(Step::Previous),
            Key::Down => self.step_inner(Step::Next),
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
            Key::X => self.trash(),
            Key::C => self.copy(),
            Key::Space => self.toggle_play(),
            Key::M => self.toggle_mute(),
            _ => Vec::new(),
        }
    }

    /// Sends the current file, broken or not, to the trash, with the MOV of
    /// a Live Photo, and shows the next one, or the previous one after the
    /// last, at once. A folder left without
    /// files shows so and opens the dir column, focused. Nothing to trash
    /// before the folder is listed.
    fn trash(&mut self) -> Vec<Command> {
        let (Some(path), Some(dir), Some(listing)) = (
            self.current.clone(),
            self.dir.clone(),
            self.all_files.clone(),
        ) else {
            return Vec::new();
        };
        if !self.files.contains(&path) {
            return Vec::new();
        }
        self.view.trash_error = None;
        let files: Vec<PathBuf> = self.live.get(&path).cloned().into_iter().collect();
        let files = [vec![path], files].concat();
        self.trashing.extend(files.iter().cloned());
        let mut commands: Vec<Command> = files.into_iter().map(Command::Trash).collect();
        // The listing without the file steps on as if the watcher reported it.
        commands.extend(self.folder_listed(&dir, Ok(listing)));
        if self.current.is_none() {
            if self.view.dir_column.is_some() {
                self.set_focus(Focus::Dirs);
            } else {
                commands.extend(self.toggle_column());
            }
        }
        commands
    }

    /// Puts the current image on the clipboard as shown, its pending rotation
    /// included. A file that does not show as an image has nothing to copy.
    fn copy(&mut self) -> Vec<Command> {
        let Screen::Image { path, image } = &self.screen else {
            return Vec::new();
        };
        let command = Command::Copy {
            path: path.clone(),
            image: image.clone(),
            rotation: self.rotation,
        };
        self.view.key_line.notice = None;
        self.view.copy_error = None;
        vec![command]
    }

    /// A trashed file is gone from the folder for good. A file that could not
    /// be trashed is back in navigation, and the failure is shown, after any
    /// other of the same `x`.
    fn trashed(&mut self, path: &Path, result: Result<(), String>) -> Vec<Command> {
        self.trashing.remove(path);
        if let Err(message) = result {
            let before = self.view.trash_error.take();
            let commands = match (self.dir.clone(), self.all_files.clone()) {
                (Some(dir), Some(files)) => self.folder_listed(&dir, Ok(files)),
                _ => Vec::new(),
            };
            self.view.trash_error = Some(match before {
                Some(before) => format!("{before}\n{message}"),
                None => message,
            });
            return commands;
        }
        if let Some(files) = &mut self.all_files {
            files.retain(|f| f != path);
        }
        Vec::new()
    }

    /// The current file while it shows as an image and no rotation of it is
    /// being written. A video cannot be turned, nor a photo while its video
    /// plays.
    fn rotatable(&self) -> Option<&PathBuf> {
        let Screen::Image { path, .. } = &self.screen else {
            return None;
        };
        (self.writing.as_ref() != Some(path) && !self.plays_video()).then_some(path)
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
        self.view.trash_error = None;
        self.view.copy_error = None;
        // The screen shows the file's first image again.
        let mut commands = self.leave_inner(None);
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
        // An animation or a multi-image file says so right after this.
        self.animated.remove(&path);
        self.images.remove(&path);
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
                    animated: false,
                    images: 1,
                    video: false,
                    motion: None,
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
