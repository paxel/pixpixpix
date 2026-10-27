//! Seam 3: videos play in a helper process, so a crash in libmpv fails only
//! the video. The real helper is this package's binary; a fake one is a shell.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use pixpixpix::helper;
use pixpixpix::video::{Playback, Report};

const WAIT: Duration = Duration::from_secs(30);

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// The real helper: the app binary started as one.
fn real_helper() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pixpixpix"));
    command.args([helper::FLAG, helper::PLAY]);
    command
}

/// A fake helper running the shell `script`, with `$0` set to `arg`.
fn fake_helper(script: &str, arg: &Path) -> Command {
    let mut command = Command::new("sh");
    command.arg("-c").arg(script).arg(arg);
    command
}

/// Plays `path` muted through `command`, the reports arriving on the receiver.
fn play(command: Command, path: PathBuf) -> (Playback, Receiver<Report>) {
    let (sender, reports) = channel();
    let playback = Playback::start_with(command, path, true, false, move |report| {
        // The test may be over already.
        let _ = sender.send(report);
    });
    (playback, reports)
}

/// The reports until the playback ends or fails.
fn until_over(reports: &Receiver<Report>) -> Vec<Report> {
    let mut seen = Vec::new();
    while let Ok(report) = reports.recv_timeout(WAIT) {
        let over = matches!(report, Report::Ended | Report::Failed(_));
        seen.push(report);
        if over {
            break;
        }
    }
    seen
}

fn failure(reports: &[Report]) -> Option<&str> {
    match reports.last() {
        Some(Report::Failed(message)) => Some(message),
        _ => None,
    }
}

#[test]
fn the_helper_plays_a_video_frame_by_frame_to_its_end() {
    let (_playback, reports) = play(real_helper(), fixture("clip.mp4"));
    let seen = until_over(&reports);
    if let Some(message) = failure(&seen) {
        // Without libmpv the reason crosses over instead.
        assert!(message.starts_with("Videos need libmpv"), "{message}");
        return;
    }
    let frames: Vec<_> = seen
        .iter()
        .filter_map(|r| match r {
            Report::Frame(image, _) => Some((image.width, image.height)),
            _ => None,
        })
        .collect();
    assert!(frames.len() > 1, "{frames:?}");
    assert!(frames.iter().all(|size| *size == (64, 32)), "{frames:?}");
    assert_eq!(seen.last(), Some(&Report::Ended));
}

#[test]
fn a_file_the_helper_cannot_play_fails_with_the_reason() {
    let dir = tempfile::tempdir().unwrap();
    let video = dir.path().join("clip.mp4");
    std::fs::write(&video, b"not a video").unwrap();
    let (_playback, reports) = play(real_helper(), video);
    let seen = until_over(&reports);
    let message = failure(&seen).unwrap();
    assert!(!message.is_empty());
    assert!(!message.contains("crashed"), "{message}");
}

#[test]
fn a_helper_that_dies_fails_only_its_video() {
    let (_playback, reports) = play(
        fake_helper("kill -9 $$", Path::new("helper")),
        fixture("clip.mp4"),
    );
    let seen = until_over(&reports);
    let message = failure(&seen).unwrap();
    assert!(message.starts_with("the video player crashed"), "{message}");
    // The app goes on: the next video gets its own helper.
    let (_next, reports) = play(
        fake_helper("cat > /dev/null", Path::new("helper")),
        fixture("clip.mp4"),
    );
    assert!(reports.recv_timeout(Duration::from_millis(200)).is_err());
}

#[test]
fn dropping_the_playback_ends_the_helper_without_a_failure() {
    let dir = tempfile::tempdir().unwrap();
    let ended = dir.path().join("ended");
    let (playback, reports) = play(
        fake_helper("cat > /dev/null; touch \"$0\"", &ended),
        fixture("clip.mp4"),
    );
    drop(playback);
    let started = Instant::now();
    while !ended.exists() {
        assert!(started.elapsed() < WAIT, "the helper still runs");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(reports.recv_timeout(Duration::from_millis(500)).is_err());
}
