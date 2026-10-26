//! The app core: every behaviour of the viewer as a state machine. It consumes
//! [`Event`]s, keeps the [`View`] the UI draws, and asks the outside world for
//! work through [`Command`]s. It does no I/O itself.

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Decode(PathBuf),
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

/// Everything the UI needs to draw a frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    pub screen: Screen,
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
}

impl Core {
    /// Starts on `start`, the file chosen from the command line, or the reason
    /// there is none. Returns the commands to run right away.
    pub fn start(start: Result<PathBuf, String>) -> (Self, Vec<Command>) {
        let current = start.as_ref().ok().cloned();
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
                fullscreen: false,
                key_line: KeyLine {
                    keys: IMAGE_VIEW_KEYS.to_vec(),
                    notice: None,
                },
            },
            first_frame_shown: false,
            current,
            files: Vec::new(),
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
            Event::Decoded { path, result } => {
                self.decoded(path, result);
                Vec::new()
            }
            Event::FirstFrameShown => self.first_frame(),
            Event::FolderListed { dir, result } => {
                self.folder_listed(&dir, result);
                Vec::new()
            }
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
        self.current = Some(path.clone());
        self.view.screen = Screen::Loading { path: path.clone() };
        vec![Command::Decode(path)]
    }

    fn decoded(&mut self, path: PathBuf, result: Result<DecodedImage, String>) {
        let waiting = matches!(&self.view.screen, Screen::Loading { path: p } if *p == path);
        if !waiting {
            return;
        }
        self.view.screen = match result {
            Ok(image) => Screen::Image { path, image },
            Err(message) => Screen::Error {
                path: Some(path),
                message,
            },
        };
    }
}
