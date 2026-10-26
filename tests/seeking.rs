//! Seam 1: the app core seeking and stepping through videos, one seek at a
//! time.

use std::path::PathBuf;
use std::sync::Arc;

use pixpixpix::core::{Command, Core, Event, Key, KeyPress, Screen, VideoStep};
use pixpixpix::decode::DecodedImage;

fn gray(value: u8) -> DecodedImage {
    DecodedImage {
        width: 1,
        height: 1,
        rgba: Arc::from(vec![value, value, value, 255]),
    }
}

fn p(name: &str) -> PathBuf {
    PathBuf::from(format!("/p/{name}"))
}

/// Showing the first frame of `a.mp4` of `a.mp4`, `b.jpg`, all decoded.
fn browsing() -> Core {
    let (mut core, _) = Core::start(Ok(p("a.mp4")));
    let decoded = |name: &str| Event::Decoded {
        path: p(name),
        result: Ok(gray(0)),
    };
    core.handle(decoded("a.mp4"));
    core.handle(Event::FirstFrameShown);
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(vec![p("a.mp4"), p("b.jpg")]),
    });
    core.handle(decoded("b.jpg"));
    core
}

/// [`browsing`] with `a.mp4` playing.
fn playing() -> Core {
    let mut core = browsing();
    core.handle(Event::Key(KeyPress::plain(Key::Space)));
    core
}

fn arrow(core: &mut Core, right: bool, modifier: Modifier) -> Vec<Command> {
    let mut key = KeyPress::plain(if right { Key::Right } else { Key::Left });
    modifier(&mut key);
    core.handle(Event::Key(key))
}

/// Sets a modifier of a key press.
type Modifier = fn(&mut KeyPress);

fn shift(key: &mut KeyPress) {
    key.shift = true;
}

fn command(key: &mut KeyPress) {
    key.command = true;
}

fn alt(key: &mut KeyPress) {
    key.alt = true;
}

fn seeked(core: &mut Core, name: &str) -> Vec<Command> {
    core.handle(Event::VideoSeeked(p(name)))
}

fn current(core: &Core) -> Option<PathBuf> {
    match &core.view().screen {
        Screen::Image { path, .. } => Some(path.clone()),
        _ => None,
    }
}

fn playing_now(core: &Core) -> Option<bool> {
    core.view().video.as_ref().map(|v| v.playing)
}

#[test]
fn shift_ctrl_and_alt_arrows_seek_the_playing_video() {
    let cases: [(Modifier, bool, VideoStep); 6] = [
        (shift, true, VideoStep::Seconds(10)),
        (shift, false, VideoStep::Seconds(-10)),
        (command, true, VideoStep::Seconds(30)),
        (command, false, VideoStep::Seconds(-30)),
        (alt, true, VideoStep::Frames(1)),
        (alt, false, VideoStep::Frames(-1)),
    ];
    for (modifier, right, step) in cases {
        let mut core = playing();
        assert_eq!(
            arrow(&mut core, right, modifier),
            [Command::SeekVideo(step)],
            "{step:?}"
        );
        assert_eq!(current(&core), Some(p("a.mp4")));
    }
}

#[test]
fn seconds_keep_playing_and_frame_steps_pause() {
    let mut core = playing();
    arrow(&mut core, true, shift);
    assert_eq!(playing_now(&core), Some(true));
    seeked(&mut core, "a.mp4");
    arrow(&mut core, true, alt);
    assert_eq!(playing_now(&core), Some(false));
    assert_eq!(
        core.handle(Event::Key(KeyPress::plain(Key::Space))),
        [Command::ResumeVideo]
    );
}

#[test]
fn plain_arrows_still_change_the_file() {
    let mut core = playing();
    arrow(&mut core, true, |_| {});
    assert_eq!(current(&core), Some(p("b.jpg")));
}

#[test]
fn modified_arrows_do_nothing_on_an_image() {
    let mut core = browsing();
    arrow(&mut core, true, |_| {});
    for modifier in [shift, command, alt] {
        assert_eq!(arrow(&mut core, false, modifier), []);
        assert_eq!(current(&core), Some(p("b.jpg")));
    }
}

#[test]
fn steps_while_a_seek_runs_only_move_the_target() {
    let mut core = playing();
    assert_eq!(
        arrow(&mut core, true, shift),
        [Command::SeekVideo(VideoStep::Seconds(10))]
    );
    assert_eq!(arrow(&mut core, true, shift), []);
    assert_eq!(arrow(&mut core, true, command), []);
    assert_eq!(arrow(&mut core, false, shift), []);
    assert_eq!(
        seeked(&mut core, "a.mp4"),
        [Command::SeekVideo(VideoStep::Seconds(30))]
    );
    assert_eq!(seeked(&mut core, "a.mp4"), []);
    assert_eq!(
        arrow(&mut core, false, shift),
        [Command::SeekVideo(VideoStep::Seconds(-10))]
    );
}

#[test]
fn steps_that_cancel_out_seek_nothing_more() {
    let mut core = playing();
    arrow(&mut core, true, shift);
    arrow(&mut core, true, shift);
    arrow(&mut core, false, shift);
    assert_eq!(seeked(&mut core, "a.mp4"), []);
    assert_eq!(
        arrow(&mut core, true, shift),
        [Command::SeekVideo(VideoStep::Seconds(10))]
    );
}

#[test]
fn held_frame_steps_run_one_frame_at_a_time() {
    let mut core = playing();
    assert_eq!(
        arrow(&mut core, true, alt),
        [Command::SeekVideo(VideoStep::Frames(1))]
    );
    assert_eq!(arrow(&mut core, true, alt), []);
    assert_eq!(arrow(&mut core, true, alt), []);
    assert_eq!(
        seeked(&mut core, "a.mp4"),
        [Command::SeekVideo(VideoStep::Frames(1))]
    );
    assert_eq!(
        seeked(&mut core, "a.mp4"),
        [Command::SeekVideo(VideoStep::Frames(1))]
    );
    assert_eq!(seeked(&mut core, "a.mp4"), []);
}

#[test]
fn the_newest_kind_of_step_replaces_a_waiting_one() {
    let mut core = playing();
    arrow(&mut core, true, shift);
    arrow(&mut core, true, alt);
    arrow(&mut core, false, alt);
    arrow(&mut core, false, alt);
    assert_eq!(
        seeked(&mut core, "a.mp4"),
        [Command::SeekVideo(VideoStep::Frames(-1))]
    );
    arrow(&mut core, false, command);
    assert_eq!(
        seeked(&mut core, "a.mp4"),
        [Command::SeekVideo(VideoStep::Seconds(-30))]
    );
    assert_eq!(seeked(&mut core, "a.mp4"), []);
}

#[test]
fn a_step_on_a_video_never_started_starts_it_paused_first() {
    let mut core = browsing();
    assert_eq!(
        arrow(&mut core, true, shift),
        [
            Command::PlayVideo {
                path: p("a.mp4"),
                muted: false,
                paused: true,
            },
            Command::SeekVideo(VideoStep::Seconds(10)),
        ]
    );
    assert_eq!(playing_now(&core), Some(false));
    core.handle(Event::VideoFrame {
        path: p("a.mp4"),
        image: gray(5),
    });
    assert!(matches!(
        &core.view().screen,
        Screen::Image { image, .. } if image.rgba.first() == Some(&5)
    ));
    assert_eq!(
        core.handle(Event::Key(KeyPress::plain(Key::Space))),
        [Command::ResumeVideo]
    );
}

#[test]
fn the_step_buttons_step_like_the_keys() {
    let mut core = browsing();
    core.handle(Event::Key(KeyPress::plain(Key::Space)));
    core.handle(Event::Key(KeyPress::plain(Key::Space)));
    assert_eq!(
        core.handle(Event::StepVideo(VideoStep::Seconds(-30))),
        [Command::SeekVideo(VideoStep::Seconds(-30))]
    );
    assert_eq!(core.handle(Event::StepVideo(VideoStep::Frames(1))), []);
    assert_eq!(
        seeked(&mut core, "a.mp4"),
        [Command::SeekVideo(VideoStep::Frames(1))]
    );
}

#[test]
fn the_step_buttons_do_nothing_on_an_image() {
    let mut core = browsing();
    arrow(&mut core, true, |_| {});
    assert_eq!(core.handle(Event::StepVideo(VideoStep::Seconds(10))), []);
}

#[test]
fn completions_of_another_video_are_ignored() {
    let mut core = playing();
    arrow(&mut core, true, shift);
    arrow(&mut core, true, shift);
    assert_eq!(seeked(&mut core, "c.mkv"), []);
    assert_eq!(
        seeked(&mut core, "a.mp4"),
        [Command::SeekVideo(VideoStep::Seconds(10))]
    );
}

#[test]
fn the_end_or_leaving_forgets_a_running_seek() {
    let mut core = playing();
    arrow(&mut core, true, shift);
    arrow(&mut core, true, shift);
    core.handle(Event::VideoEnded(p("a.mp4")));
    assert_eq!(seeked(&mut core, "a.mp4"), []);

    let mut core = playing();
    arrow(&mut core, true, shift);
    arrow(&mut core, true, shift);
    arrow(&mut core, true, |_| {});
    arrow(&mut core, false, |_| {});
    assert_eq!(seeked(&mut core, "a.mp4"), []);
    assert_eq!(
        arrow(&mut core, true, shift),
        [
            Command::PlayVideo {
                path: p("a.mp4"),
                muted: false,
                paused: true,
            },
            Command::SeekVideo(VideoStep::Seconds(10)),
        ]
    );
}

#[test]
fn the_key_line_lists_the_seek_keys_but_not_rotation_for_a_video() {
    let core = browsing();
    let keys: Vec<(&str, &str)> = core
        .view()
        .key_line
        .keys
        .iter()
        .map(|k| (k.keys, k.action))
        .collect();
    let thirty = if cfg!(target_os = "macos") {
        "Cmd+Left/Right"
    } else {
        "Ctrl+Left/Right"
    };
    assert_eq!(
        keys.get(3..6),
        Some(
            &[
                ("Shift+Left/Right", "10 s"),
                (thirty, "30 s"),
                ("Alt+Left/Right", "frame"),
            ][..]
        )
    );
    assert!(!keys.iter().any(|(k, _)| *k == "L/R" || *k == "Enter"));
}

#[test]
fn resuming_drops_a_waiting_frame_step() {
    let mut core = playing();
    arrow(&mut core, true, shift);
    arrow(&mut core, true, alt);
    assert_eq!(
        core.handle(Event::Key(KeyPress::plain(Key::Space))),
        [Command::ResumeVideo]
    );
    assert_eq!(playing_now(&core), Some(true));
    assert_eq!(seeked(&mut core, "a.mp4"), []);
    assert_eq!(playing_now(&core), Some(true));
}
