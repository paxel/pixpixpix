//! Videos through libmpv, loaded on the first one: the first frame of a file,
//! and playback in a helper process that reports frame after frame. The video of a Motion Photo
//! plays from a copy in the cache folder, removed when it stops.

use std::cell::RefCell;
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use crate::core::VideoStep;
use crate::decode::DecodedImage;
use crate::grabber;
use crate::helper::{self, Reader, Writer};
use crate::mpv::{self, Mpv, Player, PlayerEvent};
use crate::rotate::Rotation;
use crate::{clipboard, folder, motion};

/// libmpv, loaded on the first video.
static MPV: OnceLock<Result<Mpv, String>> = OnceLock::new();

fn mpv() -> Result<&'static Mpv, String> {
    MPV.get_or_init(|| mpv::load(&mpv::library_paths()))
        .as_ref()
        .map_err(Clone::clone)
}

/// Frames are drawn at most this wide and high, keeping the aspect ratio:
/// drawing in software, larger ones cannot keep up.
const MAX_SIDE: u32 = 1920;

/// How often a wait for the first frame looks at the cancellation token.
const POLL: Duration = Duration::from_millis(50);

/// The display size of the video once known, the size frames are drawn at,
/// how they are turned then, and the codec to name when they cannot be.
#[derive(Default)]
struct Size {
    width: Option<u32>,
    height: Option<u32>,
    rotation: Rotation,
    codec: Option<String>,
}

impl Size {
    /// Takes in a size change; anything else is left to the caller.
    fn update(&mut self, event: &PlayerEvent) {
        match event {
            PlayerEvent::Width(width) => self.width = *width,
            PlayerEvent::Height(height) => self.height = *height,
            PlayerEvent::Rotation(degrees) => {
                self.rotation = match degrees % 360 {
                    90 => Rotation::Right,
                    180 => Rotation::Half,
                    270 => Rotation::Left,
                    _ => Rotation::None,
                };
            }
            PlayerEvent::Codec(codec) => self.codec = Some(codec.clone()),
            PlayerEvent::Ended(_)
            | PlayerEvent::Position(_)
            | PlayerEvent::FrameLength(_)
            | PlayerEvent::Duration(_)
            | PlayerEvent::EndReached(_)
            | PlayerEvent::Restarted
            | PlayerEvent::CommandFailed(_)
            | PlayerEvent::Other => {}
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

    /// Why the video ended without a picture, from mpv's `message`: once
    /// the codec is known, the file was read and its codec failed.
    fn failure(&self, message: Option<String>) -> String {
        match (&self.codec, message) {
            (Some(codec), Some(message)) => {
                format!("The {codec} video cannot be decoded: {message}")
            }
            (Some(codec), None) => format!("The {codec} video cannot be decoded"),
            (None, Some(message)) => message,
            (None, None) => "the file has no picture".to_owned(),
        }
    }

    /// Draws the current frame at [`Size::target`], turned upright.
    fn render(&self, player: &Player, (width, height): (u32, u32)) -> Result<DecodedImage, String> {
        let frame = player.render(width, height)?;
        if self.rotation == Rotation::None {
            return Ok(frame);
        }
        clipboard::turned(&frame, self.rotation)
    }
}

/// The first frame of the video at `path`, or `None` once `cancel` is set,
/// grabbed by the shared helper process. The error is mpv's message, why mpv
/// is missing, or that the file crashed the helper.
pub fn first_frame(path: &Path, cancel: &AtomicBool) -> Option<Result<DecodedImage, String>> {
    grabber::shared().grab(path, cancel)
}

/// [`first_frame`] with libmpv in this process: the helper's work. There is
/// no time limit; only `cancel` ends the wait.
pub fn grab_here(path: &Path, cancel: &AtomicBool) -> Option<Result<DecodedImage, String>> {
    let (wakeup, woken) = channel();
    let options = [
        ("pause", "yes"),
        ("aid", "no"),
        ("osd-level", "0"),
        // mpv's software rendering crashes on rotated videos; they are
        // turned here instead.
        ("video-rotate", "no"),
    ];
    let player = match mpv().and_then(|mpv| mpv.player(&options, wakeup)) {
        Ok(player) => player,
        Err(message) => return Some(Err(message)),
    };
    if let Err(message) = player.load_file(path) {
        return Some(Err(message));
    }
    let mut size = Size::default();
    let mut ready = false;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        while let Some(event) = player.next_event() {
            if let PlayerEvent::Ended(error) = event {
                return Some(Err(size.failure(error)));
            }
            size.update(&event);
        }
        ready |= player.frame_ready();
        if ready && let Some(target) = size.target() {
            return Some(size.render(&player, target));
        }
        if let Err(RecvTimeoutError::Disconnected) = woken.recv_timeout(POLL) {
            return Some(Err("mpv stopped".to_owned()));
        }
    }
}

/// What a playback reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Report {
    /// A frame drawn, with the playback position it shows.
    Frame(DecodedImage, Duration),
    /// The video played to its end.
    Ended,
    /// The seek asked for drew its frame, or failed.
    Seeked,
    /// A jump past the end paused the video on its last frame.
    Paused,
    /// The video cannot be played, with mpv's message or why mpv is missing.
    Failed(String),
}

/// Asks of the thread driving the player.
enum Ask {
    /// What to play; the first message to a helper.
    Start {
        path: PathBuf,
        muted: bool,
        paused: bool,
    },
    /// News from mpv.
    Wake,
    Pause(bool),
    Mute(bool),
    Seek(VideoStep),
    Stop,
}

/// What the running seek waits for before it is done.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Awaiting {
    /// mpv to restart playback at the new position.
    Restart,
    /// The frame there to be drawn.
    Frame,
}

/// A video playing in a helper process; dropping it stops the video and
/// ends the helper.
pub struct Playback {
    asks: Sender<Ask>,
    /// Set once dropped, so the helper ending then is no failure.
    stopped: Arc<AtomicBool>,
}

impl Playback {
    /// Plays the video at `path` from the start, or shows its first frame if
    /// `paused`, with sound unless `muted`, reporting to `report` from
    /// another thread until stopped, the end or a failure. A photo at `path`
    /// plays the video of the Motion Photo it is. libmpv runs in a helper
    /// process, so a crash in it fails only this video.
    pub fn start(
        path: PathBuf,
        muted: bool,
        paused: bool,
        report: impl Fn(Report) + Send + 'static,
    ) -> Self {
        match helper::command(helper::PLAY) {
            Ok(command) => Self::start_with(command, path, muted, paused, report),
            Err(error) => {
                report(Report::Failed(format!(
                    "the video player cannot start: {error}"
                )));
                Self::stopped()
            }
        }
    }

    /// [`Playback::start`] with `command` as the helper.
    pub fn start_with(
        command: Command,
        path: PathBuf,
        muted: bool,
        paused: bool,
        report: impl Fn(Report) + Send + 'static,
    ) -> Self {
        let mut child = match helper::spawn(command) {
            Ok(child) => child,
            Err(error) => {
                report(Report::Failed(format!(
                    "the video player cannot start: {error}"
                )));
                return Self::stopped();
            }
        };
        let (Some(mut stdin), Some(mut stdout)) = (child.stdin.take(), child.stdout.take()) else {
            report(Report::Failed("the video player cannot start".to_owned()));
            return Self::stopped();
        };
        let (asks, inbox) = channel();
        // The start goes first; the helper has nothing to do before it.
        let _ = asks.send(Ask::Start {
            path,
            muted,
            paused,
        });
        std::thread::spawn(move || {
            for ask in inbox {
                let stop = matches!(ask, Ask::Stop);
                if helper::write_message(&mut stdin, &encode_ask(&ask)).is_err() || stop {
                    break;
                }
            }
            // Closing stdin ends the helper.
        });
        let stopped = Arc::new(AtomicBool::new(false));
        let done = Arc::clone(&stopped);
        std::thread::spawn(move || {
            let mut over = false;
            while let Ok(Some(message)) = helper::read_message(&mut stdout) {
                let Some(decoded) = decode_report(&message) else {
                    break;
                };
                over = matches!(decoded, Report::Ended | Report::Failed(_));
                report(decoded);
                if over {
                    break;
                }
            }
            drop(stdout);
            let status = child.wait();
            if !over && !done.load(Ordering::Relaxed) {
                let how = status.map_or_else(|e| e.to_string(), |s| s.to_string());
                report(Report::Failed(format!("the video player crashed ({how})")));
            }
        });
        Self { asks, stopped }
    }

    /// A playback that never started; asks go nowhere.
    fn stopped() -> Self {
        Self {
            asks: channel().0,
            stopped: Arc::new(AtomicBool::new(true)),
        }
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

    /// Seeks by `step`, reported by [`Report::Seeked`] once the frame there
    /// is drawn. Asked before the first frame, it waits for it.
    pub fn seek(&self, step: VideoStep) {
        self.ask(Ask::Seek(step));
    }

    fn ask(&self, ask: Ask) {
        // The helper is gone after the end or a failure; nothing to ask then.
        let _ = self.asks.send(ask);
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        self.ask(Ask::Stop);
    }
}

/// The helper's side of [`Playback`]: plays what the app's first message
/// asks, reports on stdout and takes asks on stdin until it closes.
pub fn serve_playback() -> ExitCode {
    let mut stdin = std::io::stdin();
    let Ok(Some(first)) = helper::read_message(&mut stdin) else {
        return ExitCode::FAILURE;
    };
    let Some(Ask::Start {
        path,
        muted,
        paused,
    }) = decode_ask(&first)
    else {
        return ExitCode::FAILURE;
    };
    let (asks, inbox) = channel();
    let wakeup = asks.clone();
    std::thread::spawn(move || {
        while let Ok(Some(message)) = helper::read_message(&mut stdin) {
            let Some(ask) = decode_ask(&message) else {
                break;
            };
            if asks.send(ask).is_err() {
                break;
            }
        }
        // The app is gone or done with the video.
        let _ = asks.send(Ask::Stop);
    });
    let stdout = RefCell::new(std::io::stdout().lock());
    let report = |report: Report| {
        // The app may be gone; the closed stdin stops the video then.
        let _ = helper::write_message(&mut *stdout.borrow_mut(), &encode_report(&report));
    };
    if let Err(message) = play(&path, [muted, paused], wakeup, &inbox, &report) {
        report(Report::Failed(message));
    }
    ExitCode::SUCCESS
}

const START: u8 = 0;
const PAUSE: u8 = 1;
const MUTE: u8 = 2;
const SEEK_SECONDS: u8 = 3;
const SEEK_FRAMES: u8 = 4;
const STOP: u8 = 5;

fn encode_ask(ask: &Ask) -> Vec<u8> {
    match ask {
        Ask::Start {
            path,
            muted,
            paused,
        } => Writer::new(START)
            .u8(u8::from(*muted))
            .u8(u8::from(*paused))
            .rest(helper::path_bytes(path)),
        Ask::Pause(paused) => Writer::new(PAUSE).u8(u8::from(*paused)).done(),
        Ask::Mute(muted) => Writer::new(MUTE).u8(u8::from(*muted)).done(),
        Ask::Seek(VideoStep::Seconds(seconds)) => Writer::new(SEEK_SECONDS).i32(*seconds).done(),
        Ask::Seek(VideoStep::Frames(frames)) => Writer::new(SEEK_FRAMES).i32(*frames).done(),
        // Never sent: the helper's own.
        Ask::Wake | Ask::Stop => Writer::new(STOP).done(),
    }
}

fn decode_ask(message: &[u8]) -> Option<Ask> {
    let mut reader = Reader::new(message);
    Some(match reader.u8()? {
        START => Ask::Start {
            muted: reader.u8()? != 0,
            paused: reader.u8()? != 0,
            path: helper::path_from(reader.rest()),
        },
        PAUSE => Ask::Pause(reader.u8()? != 0),
        MUTE => Ask::Mute(reader.u8()? != 0),
        SEEK_SECONDS => Ask::Seek(VideoStep::Seconds(reader.i32()?)),
        SEEK_FRAMES => Ask::Seek(VideoStep::Frames(reader.i32()?)),
        STOP => Ask::Stop,
        _ => return None,
    })
}

const FRAME: u8 = 0;
const ENDED: u8 = 1;
const SEEKED: u8 = 2;
const FAILED: u8 = 3;
const PAUSED: u8 = 4;

fn encode_report(report: &Report) -> Vec<u8> {
    match report {
        Report::Frame(image, position) => Writer::new(FRAME)
            .u64(u64::try_from(position.as_nanos()).unwrap_or(u64::MAX))
            .image(image),
        Report::Ended => Writer::new(ENDED).done(),
        Report::Seeked => Writer::new(SEEKED).done(),
        Report::Paused => Writer::new(PAUSED).done(),
        Report::Failed(message) => Writer::new(FAILED).rest(message.as_bytes()),
    }
}

fn decode_report(message: &[u8]) -> Option<Report> {
    let mut reader = Reader::new(message);
    Some(match reader.u8()? {
        FRAME => {
            let position = Duration::from_nanos(reader.u64()?);
            Report::Frame(reader.image()?, position)
        }
        ENDED => Report::Ended,
        SEEKED => Report::Seeked,
        PAUSED => Report::Paused,
        FAILED => Report::Failed(String::from_utf8_lossy(reader.rest()).into_owned()),
        _ => return None,
    })
}

/// Plays until stopped, the end or a failure, then lets the player go.
fn play(
    path: &Path,
    [muted, paused]: [bool; 2],
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
    let options = [
        ("pause", flag(paused)),
        ("osd-level", "0"),
        ("mute", flag(muted)),
        // As in `grab_here`.
        ("video-rotate", "no"),
        // A jump past the end stays on the last frame; the end of playing
        // through is told by `eof-reached`.
        ("keep-open", "yes"),
    ];
    // Declared before the player, so the copy goes once the player is gone.
    let clip = if folder::is_video(path) {
        None
    } else {
        Some(motion::extract(path, &clip_dir())?)
    };
    let player = mpv()?.player(&options, wakeup)?;
    player.load_file(clip.as_ref().map_or(path, |c| c.path()))?;
    let mut size = Size::default();
    // The playback position as last reported.
    let mut position = Duration::ZERO;
    // Where the seeks go from, and how far they may.
    let mut timing = Timing::default();
    let mut ready = false;
    // A frame was drawn, so the file is loaded and can be seeked.
    let mut drawn = false;
    // A seek asked before that.
    let mut early = None;
    let mut seeking = None;
    let mut paused = paused;
    // Seeked, and not played on since: reaching the end then stays there,
    // paused.
    let mut jumped = false;
    // Paused on the last frame after a jump past it.
    let mut at_end = false;
    // Starts a seek; one that cannot start or has nowhere to go is done at
    // once.
    let seek = |step, timing: Timing, at_end| match start_seek(&player, step, timing, at_end) {
        Ok(Some(seek)) => Some(seek),
        Ok(None) | Err(_) => {
            report(Report::Seeked);
            None
        }
    };
    loop {
        let mut asked = vec![inbox.recv().map_err(|_| "mpv stopped".to_owned())?];
        asked.extend(inbox.try_iter());
        for ask in asked {
            match ask {
                Ask::Wake | Ask::Start { .. } => {}
                Ask::Pause(false) if at_end => {
                    // Playing on from the last frame plays from the start.
                    player.command(&[c"seek", c"0", c"absolute+exact"])?;
                    player.set(c"pause", c"no")?;
                    (paused, jumped) = (false, false);
                }
                Ask::Pause(pause) => {
                    player.set(c"pause", c_flag(pause))?;
                    paused = pause;
                    // Playing on to the end is no jump.
                    jumped &= pause;
                }
                Ask::Mute(muted) => player.set(c"mute", c_flag(muted))?,
                Ask::Seek(step) if drawn => {
                    jumped = true;
                    paused |= matches!(step, VideoStep::Frames(_));
                    seeking = seek(step, Timing { position, ..timing }, at_end);
                }
                Ask::Seek(step) => early = Some(step),
                Ask::Stop => return Ok(()),
            }
        }
        while let Some(event) = player.next_event() {
            match event {
                // Only the sound played: the picture could not be decoded.
                PlayerEvent::Ended(None) | PlayerEvent::EndReached(true) if !drawn => {
                    return Err(size.failure(None));
                }
                PlayerEvent::Ended(None) => {
                    report(Report::Ended);
                    return Ok(());
                }
                PlayerEvent::Ended(Some(message)) => return Err(size.failure(Some(message))),
                PlayerEvent::EndReached(true) if jumped => {
                    (at_end, paused) = (true, true);
                    player.set(c"pause", c"yes")?;
                    report(Report::Paused);
                }
                PlayerEvent::EndReached(true) => {
                    report(Report::Ended);
                    return Ok(());
                }
                PlayerEvent::EndReached(false) => at_end = false,
                PlayerEvent::Restarted
                    if let Some(seek) = seeking.as_mut()
                        && seek.awaiting == Awaiting::Restart =>
                {
                    seek.awaiting = Awaiting::Frame;
                }
                PlayerEvent::CommandFailed(_) if seeking.is_some() => {
                    seeking = None;
                    report(Report::Seeked);
                }
                PlayerEvent::Position(Some(at)) => position = at,
                PlayerEvent::FrameLength(length) => timing.frame = length,
                PlayerEvent::Duration(length) => timing.length = length,
                event => size.update(&event),
            }
        }
        ready |= player.frame_ready();
        if ready && let Some(target) = size.target() {
            ready = false;
            drawn = true;
            report(Report::Frame(size.render(&player, target)?, position));
            if let Some(seek) = seeking.filter(|s| s.awaiting == Awaiting::Frame) {
                seeking = None;
                if seek.to_end && !paused {
                    player.set(c"pause", c"yes")?;
                    paused = true;
                    report(Report::Paused);
                }
                report(Report::Seeked);
            } else if seeking.is_none() && !paused {
                jumped = false;
            }
            if let Some(step) = early.take() {
                jumped = true;
                paused |= matches!(step, VideoStep::Frames(_));
                seeking = seek(step, Timing { position, ..timing }, at_end);
            }
        }
    }
}

/// Where the videos of Motion Photos are copied to play: the cache folder,
/// or the system's temporary folder without one.
fn clip_dir() -> PathBuf {
    directories::ProjectDirs::from("io.github", "paxel", "pixpixpix")
        .map_or_else(std::env::temp_dir, |dirs| dirs.cache_dir().to_path_buf())
}

/// How long a frame shows when the file does not say.
const FRAME_LENGTH: Duration = Duration::from_millis(33);

/// Where a seek starts and how far it may go.
#[derive(Clone, Copy, Default)]
struct Timing {
    /// The position of the frame shown.
    position: Duration,
    /// How long a frame shows, once the file states it.
    frame: Option<Duration>,
    /// The length of the file, once known.
    length: Option<Duration>,
}

/// A seek running.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Seek {
    awaiting: Awaiting,
    /// It lands on the last frame, where the video pauses.
    to_end: bool,
}

/// Asks mpv for the seek, returning what it waits for, or `None` when there
/// is nowhere to go. Every seek is exact, showing the first frame at or after
/// its time, and stays between the first and the last frame: a frame step
/// aims just before the frame after or before the shown one, and pauses.
fn start_seek(
    player: &Player,
    step: VideoStep,
    timing: Timing,
    at_end: bool,
) -> Result<Option<Seek>, String> {
    let frame = timing.frame.unwrap_or(FRAME_LENGTH);
    let last = timing.length.map(|length| length.saturating_sub(frame));
    let early = Duration::from_millis(1);
    let target = match step {
        VideoStep::Frames(frames) => {
            player.set(c"pause", c"yes")?;
            if frames > 0 {
                if at_end {
                    return Ok(None);
                }
                timing.position.saturating_add(frame).saturating_sub(early)
            } else if timing.position.is_zero() {
                return Ok(None);
            } else {
                timing.position.saturating_sub(frame).saturating_sub(early)
            }
        }
        VideoStep::Seconds(seconds) => {
            let by = Duration::from_secs(u64::from(seconds.unsigned_abs()));
            if seconds < 0 {
                timing.position.saturating_sub(by)
            } else {
                timing.position.saturating_add(by)
            }
        }
    };
    let (target, to_end) = match last {
        Some(last) if target >= last => {
            if timing.position >= last && matches!(step, VideoStep::Frames(1..)) {
                return Ok(None);
            }
            (last, true)
        }
        _ => (target, false),
    };
    let target = CString::new(format!("{:.6}", target.as_secs_f64())).map_err(|e| e.to_string())?;
    player.command(&[c"seek", &target, c"absolute+exact"])?;
    Ok(Some(Seek {
        awaiting: Awaiting::Restart,
        to_end,
    }))
}

fn flag(on: bool) -> &'static str {
    if on { "yes" } else { "no" }
}

fn c_flag(on: bool) -> &'static std::ffi::CStr {
    if on { c"yes" } else { c"no" }
}
