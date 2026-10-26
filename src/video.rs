//! Videos through libmpv, loaded on the first one: the first frame of a file,
//! and playback that reports frame after frame.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant};

use crate::decode::DecodedImage;
use crate::mpv::{self, Mpv, PlayerEvent};

/// libmpv, loaded on the first video.
static MPV: OnceLock<Result<Mpv, String>> = OnceLock::new();

fn mpv() -> Result<&'static Mpv, String> {
    MPV.get_or_init(|| mpv::load(mpv::LIBRARY_NAMES))
        .as_ref()
        .map_err(Clone::clone)
}

/// Frames are drawn at most this wide and high, keeping the aspect ratio:
/// drawing in software, larger ones cannot keep up.
const MAX_SIDE: u32 = 1920;

/// How long the first frame may take before the file counts as broken.
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(10);

/// How often a wait for the first frame looks at the cancellation token.
const POLL: Duration = Duration::from_millis(50);

/// The display size of the video once known, and the size frames are drawn
/// at.
#[derive(Default)]
struct Size {
    width: Option<u32>,
    height: Option<u32>,
}

impl Size {
    /// Takes in a size change; anything else is left to the caller.
    fn update(&mut self, event: &PlayerEvent) {
        match event {
            PlayerEvent::Width(width) => self.width = *width,
            PlayerEvent::Height(height) => self.height = *height,
            PlayerEvent::Ended(_) | PlayerEvent::Other => {}
        }
    }

    /// The size frames are drawn at: the display size, shrunk to fit
    /// [`MAX_SIDE`].
    fn target(&self) -> Option<(u32, u32)> {
        let (width, height) = (self.width?, self.height?);
        let longest = width.max(height);
        if longest <= MAX_SIDE {
            return Some((width, height));
        }
        let scale = |side: u32| {
            let scaled = u64::from(side)
                .checked_mul(u64::from(MAX_SIDE))
                .and_then(|s| s.checked_div(u64::from(longest)))
                .and_then(|s| u32::try_from(s).ok());
            scaled.unwrap_or(MAX_SIDE).max(1)
        };
        Some((scale(width), scale(height)))
    }
}

/// The first frame of the video at `path`, or `None` once `cancel` is set. The
/// error is mpv's message, or why mpv is missing.
pub fn first_frame(path: &Path, cancel: &AtomicBool) -> Option<Result<DecodedImage, String>> {
    let (wakeup, woken) = channel();
    let options = [("pause", "yes"), ("aid", "no"), ("osd-level", "0")];
    let player = match mpv().and_then(|mpv| mpv.player(&options, wakeup)) {
        Ok(player) => player,
        Err(message) => return Some(Err(message)),
    };
    if let Err(message) = player.load_file(path) {
        return Some(Err(message));
    }
    let started = Instant::now();
    let mut size = Size::default();
    let mut ready = false;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        if started.elapsed() > FIRST_FRAME_TIMEOUT {
            return Some(Err("mpv found no picture in the file".to_owned()));
        }
        while let Some(event) = player.next_event() {
            if let PlayerEvent::Ended(error) = &event {
                let message = error
                    .clone()
                    .unwrap_or_else(|| "the file has no picture".into());
                return Some(Err(message));
            }
            size.update(&event);
        }
        ready |= player.frame_ready();
        if ready && let Some((width, height)) = size.target() {
            return Some(player.render(width, height));
        }
        if let Err(RecvTimeoutError::Disconnected) = woken.recv_timeout(POLL) {
            return Some(Err("mpv stopped".to_owned()));
        }
    }
}

/// What a playback reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Report {
    Frame(DecodedImage),
    /// The video played to its end.
    Ended,
    /// The video cannot be played, with mpv's message or why mpv is missing.
    Failed(String),
}

/// Asks of the thread driving the player.
enum Ask {
    /// News from mpv.
    Wake,
    Pause(bool),
    Mute(bool),
    Stop,
}

/// A video playing on its own thread; dropping it stops the video.
pub struct Playback {
    asks: Sender<Ask>,
}

impl Playback {
    /// Plays the video at `path` from the start, with sound unless `muted`,
    /// reporting to `report` from the playing thread until stopped, the end or
    /// a failure.
    pub fn start(path: PathBuf, muted: bool, report: impl Fn(Report) + Send + 'static) -> Self {
        let (asks, inbox) = channel();
        let wakeup = asks.clone();
        std::thread::spawn(move || {
            if let Err(message) = play(&path, muted, wakeup, &inbox, &report) {
                report(Report::Failed(message));
            }
        });
        Self { asks }
    }

    pub fn pause(&self) {
        self.ask(Ask::Pause(true));
    }

    pub fn resume(&self) {
        self.ask(Ask::Pause(false));
    }

    pub fn mute(&self, muted: bool) {
        self.ask(Ask::Mute(muted));
    }

    fn ask(&self, ask: Ask) {
        // The thread is gone after the end or a failure; nothing to ask then.
        let _ = self.asks.send(ask);
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        self.ask(Ask::Stop);
    }
}

/// Plays until stopped, the end or a failure, then lets the player go.
fn play(
    path: &Path,
    muted: bool,
    asks: Sender<Ask>,
    inbox: &Receiver<Ask>,
    report: &impl Fn(Report),
) -> Result<(), String> {
    let (wakeup, woken) = channel();
    // mpv's wakeups become asks, so one channel is waited on.
    std::thread::spawn(move || {
        for () in woken {
            if asks.send(Ask::Wake).is_err() {
                return;
            }
        }
    });
    let options = [("pause", "no"), ("osd-level", "0"), ("mute", flag(muted))];
    let player = mpv()?.player(&options, wakeup)?;
    player.load_file(path)?;
    let mut size = Size::default();
    let mut ready = false;
    loop {
        let mut asked = vec![inbox.recv().map_err(|_| "mpv stopped".to_owned())?];
        asked.extend(inbox.try_iter());
        for ask in asked {
            match ask {
                Ask::Wake => {}
                Ask::Pause(paused) => player.set(c"pause", c_flag(paused))?,
                Ask::Mute(muted) => player.set(c"mute", c_flag(muted))?,
                Ask::Stop => return Ok(()),
            }
        }
        while let Some(event) = player.next_event() {
            match event {
                PlayerEvent::Ended(None) => {
                    report(Report::Ended);
                    return Ok(());
                }
                PlayerEvent::Ended(Some(message)) => return Err(message),
                event => size.update(&event),
            }
        }
        ready |= player.frame_ready();
        if ready && let Some((width, height)) = size.target() {
            ready = false;
            report(Report::Frame(player.render(width, height)?));
        }
    }
}

fn flag(on: bool) -> &'static str {
    if on { "yes" } else { "no" }
}

fn c_flag(on: bool) -> &'static std::ffi::CStr {
    if on { c"yes" } else { c"no" }
}
