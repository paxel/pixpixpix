//! Seam 3: first frames of videos come from a shared helper process; a crash
//! of it fails only the file that crashes a helper on its own. The real helper
//! is this package's binary; a crashing one is a shell that kills itself.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::channel;
use std::time::Duration;

use pixpixpix::decode::DecodedImage;
use pixpixpix::grabber::{CRASHED, Grabber};
use pixpixpix::helper;

const WAIT: Duration = Duration::from_secs(60);

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn real_helper() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pixpixpix"));
    command.args([helper::FLAG, helper::GRAB]);
    command
}

/// A helper that dies after a second, enough for the grabs to reach it.
fn crashing_helper() -> Command {
    let mut command = Command::new("sh");
    command.args(["-c", "sleep 1; kill -9 $$"]);
    command
}

/// Grabs `files` at once, each from a thread of its own, through a grabber
/// whose `n`th helper started (from 0) is `helper(n)`.
fn grab_all(
    helper: impl Fn(usize) -> Command + Send + Sync + 'static,
    files: &[PathBuf],
) -> Vec<Option<Result<DecodedImage, String>>> {
    let started = AtomicUsize::new(0);
    let grabber = Grabber::new(move || Ok(helper(started.fetch_add(1, Ordering::SeqCst))));
    let (sender, results) = channel();
    for file in files {
        let (grabber, sender, file) = (Arc::clone(&grabber), sender.clone(), file.clone());
        std::thread::spawn(move || {
            let result = grabber.grab(&file, &AtomicBool::new(false));
            // The test may be over already.
            let _ = sender.send(result);
        });
    }
    files
        .iter()
        .map(|_| results.recv_timeout(WAIT).ok().flatten())
        .collect()
}

fn crashed(result: &Option<Result<DecodedImage, String>>) -> bool {
    matches!(result, Some(Err(message)) if message == CRASHED)
}

#[test]
fn the_helper_grabs_the_first_frame() {
    let results = grab_all(|_| real_helper(), &[fixture("clip.mp4")]);
    match &results[0] {
        Some(Ok(image)) => assert_eq!((image.width, image.height), (64, 32)),
        // Without libmpv the reason crosses over instead.
        Some(Err(message)) => assert!(message.starts_with("Videos need libmpv"), "{message}"),
        None => panic!("no answer"),
    }
}

#[test]
fn after_a_crash_the_grabs_are_retried_and_get_their_frames() {
    let files = [
        fixture("clip.mp4"),
        fixture("clip.mp4"),
        fixture("clip.mp4"),
    ];
    let results = grab_all(
        |n| {
            if n == 0 {
                crashing_helper()
            } else {
                real_helper()
            }
        },
        &files,
    );
    assert!(results.iter().all(Option::is_some), "{results:?}");
    assert!(!results.iter().any(crashed), "{results:?}");
}

#[test]
fn only_the_file_that_crashes_a_helper_alone_fails() {
    let files = [
        fixture("clip.mp4"),
        fixture("clip.mp4"),
        fixture("clip.mp4"),
    ];
    // The shared helper crashes, and so does the first retry.
    let results = grab_all(
        |n| {
            if n <= 1 {
                crashing_helper()
            } else {
                real_helper()
            }
        },
        &files,
    );
    assert!(results.iter().all(Option::is_some), "{results:?}");
    assert_eq!(
        results.iter().filter(|r| crashed(r)).count(),
        1,
        "{results:?}"
    );
}

#[test]
fn new_grabs_after_a_crash_get_a_new_helper() {
    let started = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&started);
    let grabber = Grabber::new(move || {
        Ok(if count.fetch_add(1, Ordering::SeqCst) == 0 {
            crashing_helper()
        } else {
            real_helper()
        })
    });
    let first = grabber.grab(&fixture("clip.mp4"), &AtomicBool::new(false));
    assert!(!crashed(&first), "{first:?}");
    let second = grabber.grab(&fixture("clip.mp4"), &AtomicBool::new(false));
    assert!(!crashed(&second), "{second:?}");
    assert!(started.load(Ordering::SeqCst) >= 3);
}

#[test]
fn a_cancelled_grab_gives_nothing() {
    let grabber = Grabber::new(|| {
        let mut command = Command::new("sh");
        command.args(["-c", "cat > /dev/null"]);
        Ok(command)
    });
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        flag.store(true, Ordering::Relaxed);
    });
    assert_eq!(grabber.grab(&fixture("clip.mp4"), &cancel), None);
}

#[test]
fn a_rotated_video_gives_its_first_frame_upright() {
    let results = grab_all(|_| real_helper(), &[fixture("rotated.mp4")]);
    let image = match &results[0] {
        Some(Ok(image)) => image.clone(),
        // Without libmpv the reason crosses over instead.
        Some(Err(message)) => {
            assert!(message.starts_with("Videos need libmpv"), "{message}");
            return;
        }
        None => panic!("no answer"),
    };
    // Stored 64 by 32, turned a quarter; ffmpeg's upright frame to compare.
    let upright = image::open(fixture("rotated_first.png"))
        .unwrap()
        .to_rgba8();
    assert_eq!((image.width, image.height), upright.dimensions());
    let difference: u64 = image
        .rgba
        .iter()
        .zip(upright.as_raw())
        .map(|(a, b)| u64::from(a.abs_diff(*b)))
        .sum();
    let mean = difference / image.rgba.len() as u64;
    assert!(mean < 24, "mean difference {mean}");
}

#[test]
fn a_video_whose_codec_fails_names_the_codec() {
    // `clip.mp4` with its H.264 track labelled HEVC.
    let results = grab_all(|_| real_helper(), &[fixture("undecodable.mp4")]);
    let Some(Err(message)) = &results[0] else {
        panic!("{results:?}");
    };
    assert!(
        message.starts_with("The hevc video cannot be decoded")
            || message.starts_with("Videos need libmpv"),
        "{message}"
    );
}
