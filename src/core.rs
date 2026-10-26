//! The app core: every behaviour of the viewer as a state machine. It consumes
//! [`Event`]s, keeps the [`View`] the UI draws, and asks the outside world for
//! work through [`Command`]s. It does no I/O itself.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::decode::DecodedImage;
use crate::folder;

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
    Image {
        path: PathBuf,
        image: DecodedImage,
    },
    /// `path` is absent when there was nothing to open at all.
    Error {
        path: Option<PathBuf>,
        message: String,
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
        keys: "F",
        action: "fullscreen",
    },
    KeyHint {
        keys: QUIT_KEYS,
        action: "quit",
    },
];

/// One image of the band.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumb {
    pub path: PathBuf,
    pub image: DecodedImage,
    pub current: bool,
}

/// How many neighbours per side are kept decoded unless configured otherwise.
const DEFAULT_CACHE_PER_SIDE: usize = 5;

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

    /// Screen points per image pixel for `image` in `viewport`.
    fn factor(&self, image: &DecodedImage, viewport: Points) -> f32 {
        match self.scale {
            Scale::Fit => fit_scale(image, viewport),
            Scale::Exact(scale) => scale,
        }
    }

    pub fn placement(&self, image: &DecodedImage, viewport: Points) -> Placement {
        let scale = self.factor(image, viewport);
        let width = image.width as f32 * scale;
        let height = image.height as f32 * scale;
        Placement {
            left: offset(width, viewport.x, self.center_x),
            top: offset(height, viewport.y, self.center_y),
            width,
            height,
        }
    }

    /// The centre actually shown: axes the viewport fully shows are centred.
    fn shown_center(&self, image: &DecodedImage, viewport: Points) -> (f32, f32) {
        let scale = self.factor(image, viewport);
        let shown = |size: u32, view: f32, center: f32| {
            if size as f32 * scale <= view {
                0.5
            } else {
                center
            }
        };
        (
            shown(image.width, viewport.x, self.center_x),
            shown(image.height, viewport.y, self.center_y),
        )
    }
}

/// The largest scale that fits `image` into `viewport`, at most 100 %.
fn fit_scale(image: &DecodedImage, viewport: Points) -> f32 {
    if image.width == 0 || image.height == 0 {
        return 0.0;
    }
    (viewport.x / image.width as f32)
        .min(viewport.y / image.height as f32)
        .min(1.0)
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
    /// The cached images in folder order.
    pub band: Vec<Thumb>,
    pub fullscreen: bool,
    pub key_line: KeyLine,
    pub zoom: Zoom,
    /// The size of the area the image is shown in; zero until the UI reports
    /// it.
    pub viewport: Points,
}

pub struct Core {
    view: View,
    first_frame_shown: bool,
    /// The file the main area is about, if any.
    current: Option<PathBuf>,
    /// The folder listed and watched, once the first frame is shown.
    dir: Option<PathBuf>,
    /// The media files of the current file's folder, in natural order; empty
    /// until listed.
    files: Vec<PathBuf>,
    /// How many neighbours per side of the current file are kept decoded.
    cache_per_side: usize,
    /// Decode results of the files in the window around the current one.
    /// Failures are kept too, so a broken file is not decoded again and again.
    cache: BTreeMap<PathBuf, Result<DecodedImage, String>>,
    /// Decodes asked for and neither answered nor cancelled yet.
    decoding: BTreeSet<PathBuf>,
}

impl Core {
    /// Starts on `start`, the file chosen from the command line, or the reason
    /// there is none. Returns the commands to run right away.
    pub fn start(start: Result<PathBuf, String>) -> (Self, Vec<Command>) {
        let current = start.as_ref().ok().cloned();
        let decoding = current.iter().cloned().collect();
        let (screen, commands) = match start {
            Ok(path) => (
                Screen::Loading { path: path.clone() },
                vec![Command::Decode(path)],
            ),
            Err(message) => (
                Screen::Error {
                    path: None,
                    message,
                },
                Vec::new(),
            ),
        };
        let core = Self {
            view: View {
                screen,
                band: Vec::new(),
                fullscreen: false,
                key_line: KeyLine {
                    keys: IMAGE_VIEW_KEYS.to_vec(),
                    notice: None,
                },
                zoom: Zoom::FIT,
                viewport: Points::default(),
            },
            first_frame_shown: false,
            current,
            dir: None,
            files: Vec::new(),
            cache_per_side: DEFAULT_CACHE_PER_SIDE,
            cache: BTreeMap::new(),
            decoding,
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
        }
    }

    /// The first image is on screen: now the folder may be read and watched.
    /// The watch starts first, so no change between the two is missed.
    fn first_frame(&mut self) -> Vec<Command> {
        if self.first_frame_shown {
            return Vec::new();
        }
        self.first_frame_shown = true;
        self.dir = self
            .current
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf);
        match &self.dir {
            Some(dir) => vec![
                Command::WatchFolder(dir.clone()),
                Command::ListFolder(dir.clone()),
            ],
            None => Vec::new(),
        }
    }

    /// A folder that cannot be read leaves browsing off; the current image
    /// stays. A current file that vanished from the folder gives way to the
    /// file after it, or the last one; with no files left the folder's
    /// emptiness is shown until a file appears.
    fn folder_listed(&mut self, dir: &Path, result: Result<Vec<PathBuf>, String>) -> Vec<Command> {
        if self.dir.as_deref() != Some(dir) {
            return Vec::new();
        }
        let Ok(files) = result else {
            return Vec::new();
        };
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
                self.view.screen = Screen::Error {
                    path: None,
                    message: format!("No images in {}", dir.display()),
                };
                self.refresh()
            }
        }
    }

    fn key(&mut self, key: KeyPress) -> Vec<Command> {
        match (key.key, key.command) {
            (Key::Q, true) => vec![Command::Quit],
            (Key::F, false) => {
                self.view.fullscreen = !self.view.fullscreen;
                vec![Command::SetFullscreen(self.view.fullscreen)]
            }
            (Key::Left, false) => self.step(Step::Previous),
            (Key::Right, false) => self.step(Step::Next),
            (Key::Plus, false) => {
                self.zoom_by(KEY_ZOOM_STEP, Points::default());
                Vec::new()
            }
            (Key::Minus, false) => {
                self.zoom_by(KEY_ZOOM_STEP.recip(), Points::default());
                Vec::new()
            }
            (Key::Num0, false) => {
                self.view.zoom = Zoom::FIT;
                Vec::new()
            }
            (Key::Num1, false) => {
                self.view.zoom.scale = Scale::Exact(1.0);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// The image on screen, if it and the viewport have a size, so zoom
    /// math cannot divide by zero.
    fn zoomable(&self) -> Option<&DecodedImage> {
        let Screen::Image { image, .. } = &self.view.screen else {
            return None;
        };
        let viewport = self.view.viewport;
        let sized = image.width > 0 && image.height > 0 && viewport.x > 0.0 && viewport.y > 0.0;
        sized.then_some(image)
    }

    /// Multiplies the scale by `factor`, keeping the image point under
    /// `pointer` (from the viewport's centre) where it is.
    fn zoom_by(&mut self, factor: f32, pointer: Points) {
        let Some(image) = self.zoomable() else {
            return;
        };
        let viewport = self.view.viewport;
        let zoom = self.view.zoom;
        let old = zoom.factor(image, viewport);
        let new = (old * factor).clamp(MIN_SCALE, MAX_SCALE);
        let (x, y) = zoom.shown_center(image, viewport);
        // The image pixel under the pointer, as a fraction, moves by the
        // pointer offset seen at the old minus the new scale.
        let shift = |size: u32, offset: f32| offset * (old.recip() - new.recip()) / size as f32;
        self.view.zoom = Zoom {
            scale: Scale::Exact(new),
            center_x: (x + shift(image.width, pointer.x)).clamp(0.0, 1.0),
            center_y: (y + shift(image.height, pointer.y)).clamp(0.0, 1.0),
        };
    }

    /// Moves the image with the pointer; a fitted image does not move.
    fn pan(&mut self, delta: Points) {
        let Some(image) = self.zoomable() else {
            return;
        };
        let zoom = self.view.zoom;
        let Scale::Exact(scale) = zoom.scale else {
            return;
        };
        let (x, y) = zoom.shown_center(image, self.view.viewport);
        let shift = |size: u32, offset: f32| offset / (scale * size as f32);
        let center_x = (x - shift(image.width, delta.x)).clamp(0.0, 1.0);
        let center_y = (y - shift(image.height, delta.y)).clamp(0.0, 1.0);
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

    /// Jumps to a cached image picked in the band.
    fn show(&mut self, path: PathBuf) -> Vec<Command> {
        if !matches!(self.cache.get(&path), Some(Ok(_))) {
            return Vec::new();
        }
        self.view.key_line.notice = None;
        self.go_to(path)
    }

    /// Makes `path` the current file: a cached one shows at once, otherwise it
    /// shows nothing until its decode arrives.
    fn go_to(&mut self, path: PathBuf) -> Vec<Command> {
        self.current = Some(path.clone());
        let mut commands = Vec::new();
        self.view.screen = match self.cache.get(&path) {
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
        if !self.decoding.remove(&path) {
            return Vec::new();
        }
        let waiting = matches!(&self.view.screen, Screen::Loading { path: p } if *p == path);
        if waiting {
            self.view.screen = screen(&path, result.clone());
        }
        self.cache.insert(path, result);
        self.refresh()
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
        let first = index.saturating_sub(self.cache_per_side);
        let last = index.saturating_add(self.cache_per_side);
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
        for distance in 1..=self.cache_per_side {
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
        if !matches!(self.view.screen, Screen::Loading { .. }) {
            for path in self.neighbours() {
                if !self.cache.contains_key(&path) && self.decoding.insert(path.clone()) {
                    commands.push(Command::Decode(path));
                }
            }
        }
        self.view.band = window
            .into_iter()
            .filter_map(|path| match self.cache.get(&path) {
                Some(Ok(image)) => Some(Thumb {
                    image: image.clone(),
                    current: self.current.as_ref() == Some(&path),
                    path,
                }),
                _ => None,
            })
            .collect();
        commands
    }
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
