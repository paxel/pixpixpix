//! The app core: every behaviour of the viewer as a state machine. It consumes
//! [`Event`]s, keeps the [`View`] the UI draws, and asks the outside world for
//! work through [`Command`]s. It does no I/O itself.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::decode::DecodedImage;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Key(KeyPress),
    /// A decode asked for with [`Command::Decode`] finished.
    Decoded {
        path: PathBuf,
        result: Result<DecodedImage, String>,
    },
    /// The first frame showing an image (or its error) reached the screen.
    FirstFrameShown,
    /// A listing asked for with [`Command::ListFolder`] finished; the files are
    /// in natural order.
    FolderListed {
        dir: PathBuf,
        result: Result<Vec<PathBuf>, String>,
    },
    /// A thumb of the band was clicked.
    Show(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Decode(PathBuf),
    /// The result of an earlier [`Command::Decode`] is no longer needed; it
    /// reports nothing once cancelled.
    CancelDecode(PathBuf),
    /// List the media files of a folder, answered by [`Event::FolderListed`].
    ListFolder(PathBuf),
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

/// Everything the UI needs to draw a frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    pub screen: Screen,
    /// The cached images in folder order.
    pub band: Vec<Thumb>,
    pub fullscreen: bool,
    pub key_line: KeyLine,
}

pub struct Core {
    view: View,
    first_frame_shown: bool,
    /// The file the main area is about, if any.
    current: Option<PathBuf>,
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
            },
            first_frame_shown: false,
            current,
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
            Event::FolderListed { dir, result } => {
                self.folder_listed(&dir, result);
                self.refresh()
            }
            Event::Show(path) => self.show(path),
        }
    }

    /// The first image is on screen: now the folder may be read.
    fn first_frame(&mut self) -> Vec<Command> {
        if self.first_frame_shown {
            return Vec::new();
        }
        self.first_frame_shown = true;
        match self.current.as_deref().and_then(Path::parent) {
            Some(dir) => vec![Command::ListFolder(dir.to_path_buf())],
            None => Vec::new(),
        }
    }

    /// A folder that cannot be read leaves browsing off; the current image
    /// stays.
    fn folder_listed(&mut self, dir: &Path, result: Result<Vec<PathBuf>, String>) {
        let current_dir = self.current.as_deref().and_then(Path::parent);
        if current_dir != Some(dir) {
            return;
        }
        if let Ok(files) = result {
            self.files = files;
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
            _ => Vec::new(),
        }
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
