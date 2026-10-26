//! The app core: every behaviour of the viewer as a state machine. It consumes
//! [`Event`]s, keeps the [`View`] the UI draws, and asks the outside world for
//! work through [`Command`]s. It does no I/O itself.

use std::path::PathBuf;

use crate::decode::DecodedImage;

/// A key as the core understands it. The platform's primary modifier (`Ctrl` on
/// Linux, `Cmd` on macOS) arrives already mapped to [`KeyPress::command`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    F,
    Q,
    Escape,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Decode(PathBuf),
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

/// Everything the UI needs to draw a frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    pub screen: Screen,
    pub fullscreen: bool,
}

pub struct Core {
    view: View,
    first_frame_shown: bool,
}

impl Core {
    /// Starts on `start`, the file chosen from the command line, or the reason
    /// there is none. Returns the commands to run right away.
    pub fn start(start: Result<PathBuf, String>) -> (Self, Vec<Command>) {
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
            },
            first_frame_shown: false,
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
            Event::FirstFrameShown => {
                self.first_frame_shown = true;
                Vec::new()
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
            _ => Vec::new(),
        }
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
