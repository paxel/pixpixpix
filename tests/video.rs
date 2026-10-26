//! Seam 1: the app core showing and playing videos.

use std::path::PathBuf;
use std::sync::Arc;

use pixpixpix::core::{Command, Core, Event, Key, KeyPress, Screen, Video};
use pixpixpix::decode::DecodedImage;
use pixpixpix::rotate::Rotation;

/// A 1x1 image of the gray `value`.
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

fn press(core: &mut Core, key: Key) -> Vec<Command> {
    core.handle(Event::Key(KeyPress::plain(key)))
}

/// The first frame of `name` decoded, gray 0.
fn decoded(core: &mut Core, name: &str) -> Vec<Command> {
    core.handle(Event::Decoded {
        path: p(name),
        result: Ok(gray(0)),
    })
}

/// Showing the first frame of `a.mp4` of `a.mp4`, `b.jpg`, `c.mkv`, all
/// decoded.
fn browsing() -> Core {
    let (mut core, _) = Core::start(Ok(p("a.mp4")));
    decoded(&mut core, "a.mp4");
    core.handle(Event::FirstFrameShown);
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(vec![p("a.mp4"), p("b.jpg"), p("c.mkv")]),
    });
    decoded(&mut core, "b.jpg");
    decoded(&mut core, "c.mkv");
    core
}

/// [`browsing`] with `a.mp4` playing.
fn playing() -> Core {
    let mut core = browsing();
    press(&mut core, Key::Space);
    core
}

/// The gray value of the image shown.
fn shown(core: &Core) -> Option<u8> {
    match &core.view().screen {
        Screen::Image { image, .. } => image.rgba.first().copied(),
        _ => None,
    }
}

fn frame(core: &mut Core, name: &str, value: u8) -> Vec<Command> {
    core.handle(Event::VideoFrame {
        path: p(name),
        image: gray(value),
    })
}

/// The keys and actions of the key line, in order.
fn key_line(core: &Core) -> Vec<(&'static str, &'static str)> {
    let keys = &core.view().key_line.keys;
    keys.iter().map(|k| (k.keys, k.action)).collect()
}

fn video(playing: bool, muted: bool) -> Option<Video> {
    Some(Video {
        playing,
        muted,
        error: None,
    })
}

#[test]
fn a_video_shows_its_first_frame_paused() {
    let core = browsing();
    assert_eq!(shown(&core), Some(0));
    assert_eq!(core.view().video, video(false, false));
}

#[test]
fn videos_are_marked_in_the_band() {
    let core = browsing();
    let marks: Vec<(String, bool)> = core
        .view()
        .band
        .iter()
        .map(|t| (pixpixpix::folder::file_name(&t.path), t.video))
        .collect();
    assert_eq!(
        marks,
        [
            ("a.mp4".to_owned(), true),
            ("b.jpg".to_owned(), false),
            ("c.mkv".to_owned(), true)
        ]
    );
}

#[test]
fn a_broken_video_is_an_error_without_playback() {
    let (mut core, _) = Core::start(Ok(p("a.mp4")));
    core.handle(Event::Decoded {
        path: p("a.mp4"),
        result: Err("Videos need libmpv, which was not found: gone".into()),
    });
    assert_eq!(
        core.view().screen,
        Screen::Error {
            path: Some(p("a.mp4")),
            message: "Videos need libmpv, which was not found: gone".into(),
        }
    );
    assert_eq!(core.view().video, None);
    assert_eq!(press(&mut core, Key::Space), []);
}

#[test]
fn space_starts_pauses_and_resumes_the_video() {
    let mut core = browsing();
    assert_eq!(
        press(&mut core, Key::Space),
        [Command::PlayVideo {
            path: p("a.mp4"),
            muted: false,
            paused: false,
        }]
    );
    assert_eq!(core.view().video, video(true, false));
    assert_eq!(press(&mut core, Key::Space), [Command::PauseVideo]);
    assert_eq!(core.view().video, video(false, false));
    assert_eq!(press(&mut core, Key::Space), [Command::ResumeVideo]);
    assert_eq!(core.view().video, video(true, false));
}

#[test]
fn a_click_on_the_video_plays_and_pauses_it() {
    let mut core = browsing();
    assert_eq!(
        core.handle(Event::Clicked),
        [Command::PlayVideo {
            path: p("a.mp4"),
            muted: false,
            paused: false,
        }]
    );
    assert_eq!(core.handle(Event::Clicked), [Command::PauseVideo]);
}

#[test]
fn space_and_clicks_do_nothing_on_an_image() {
    let mut core = browsing();
    press(&mut core, Key::Right);
    assert_eq!(core.view().video, None);
    assert_eq!(press(&mut core, Key::Space), []);
    assert_eq!(core.handle(Event::Clicked), []);
    assert_eq!(press(&mut core, Key::M), []);
}

#[test]
fn m_toggles_the_sound_kept_from_video_to_video() {
    let mut core = browsing();
    assert_eq!(press(&mut core, Key::M), []);
    assert_eq!(core.view().video, video(false, true));
    assert_eq!(
        press(&mut core, Key::Space),
        [Command::PlayVideo {
            path: p("a.mp4"),
            muted: true,
            paused: false,
        }]
    );
    assert_eq!(press(&mut core, Key::M), [Command::MuteVideo(false)]);
    assert_eq!(press(&mut core, Key::M), [Command::MuteVideo(true)]);
    press(&mut core, Key::Right);
    press(&mut core, Key::Right);
    assert_eq!(core.view().video, video(false, true));
}

#[test]
fn frames_of_the_playing_video_are_shown() {
    let mut core = playing();
    frame(&mut core, "a.mp4", 7);
    assert_eq!(shown(&core), Some(7));
    press(&mut core, Key::Space);
    assert_eq!(shown(&core), Some(7));
}

#[test]
fn frames_of_a_video_not_playing_are_ignored() {
    let mut core = browsing();
    frame(&mut core, "a.mp4", 7);
    assert_eq!(shown(&core), Some(0));
    press(&mut core, Key::Space);
    frame(&mut core, "c.mkv", 9);
    assert_eq!(shown(&core), Some(0));
}

#[test]
fn moving_on_stops_the_video_and_coming_back_shows_its_first_frame() {
    let mut core = playing();
    frame(&mut core, "a.mp4", 7);
    let commands = press(&mut core, Key::Right);
    assert!(commands.contains(&Command::StopVideo), "{commands:?}");
    assert_eq!(core.view().video, None);
    frame(&mut core, "a.mp4", 8);
    assert_eq!(press(&mut core, Key::Left), []);
    assert_eq!(shown(&core), Some(0));
    assert_eq!(core.view().video, video(false, false));
    assert_eq!(
        press(&mut core, Key::Space),
        [Command::PlayVideo {
            path: p("a.mp4"),
            muted: false,
            paused: false,
        }]
    );
}

#[test]
fn moving_on_from_a_video_never_played_stops_nothing() {
    let mut core = browsing();
    let commands = press(&mut core, Key::Right);
    assert!(!commands.contains(&Command::StopVideo), "{commands:?}");
}

#[test]
fn clicking_another_band_thumb_stops_the_video() {
    let mut core = playing();
    let commands = core.handle(Event::Show(p("c.mkv")));
    assert!(commands.contains(&Command::StopVideo), "{commands:?}");
    assert_eq!(core.view().video, video(false, false));
}

#[test]
fn the_end_shows_the_first_frame_ready_to_play_anew() {
    let mut core = playing();
    frame(&mut core, "a.mp4", 7);
    assert_eq!(
        core.handle(Event::VideoEnded(p("a.mp4"))),
        [Command::StopVideo]
    );
    assert_eq!(shown(&core), Some(0));
    assert_eq!(core.view().video, video(false, false));
    assert_eq!(
        press(&mut core, Key::Space),
        [Command::PlayVideo {
            path: p("a.mp4"),
            muted: false,
            paused: false,
        }]
    );
}

#[test]
fn a_failure_shows_the_first_frame_with_mpvs_message() {
    let mut core = playing();
    core.handle(Event::VideoFailed {
        path: p("a.mp4"),
        message: "unrecognized file format".into(),
    });
    assert_eq!(shown(&core), Some(0));
    assert_eq!(
        core.view().video,
        Some(Video {
            playing: false,
            muted: false,
            error: Some("unrecognized file format".into()),
        })
    );
    press(&mut core, Key::Space);
    assert_eq!(core.view().video, video(true, false));
}

#[test]
fn reports_of_a_video_left_are_ignored() {
    let mut core = playing();
    press(&mut core, Key::Right);
    press(&mut core, Key::Right);
    assert_eq!(core.handle(Event::VideoEnded(p("a.mp4"))), []);
    core.handle(Event::VideoFailed {
        path: p("a.mp4"),
        message: "late".into(),
    });
    assert_eq!(core.view().video, video(false, false));
}

#[test]
fn the_key_line_lists_the_video_keys_while_a_video_is_shown() {
    let mut core = browsing();
    let keys = key_line(&core);
    assert_eq!(
        keys.get(1..3),
        Some(&[("Space", "play"), ("M", "mute")][..])
    );
    press(&mut core, Key::Space);
    press(&mut core, Key::M);
    let keys = key_line(&core);
    assert_eq!(
        keys.get(1..3),
        Some(&[("Space", "pause"), ("M", "unmute")][..])
    );
    press(&mut core, Key::Right);
    assert!(!key_line(&core).iter().any(|(k, _)| *k == "Space"));
}

#[test]
fn a_video_cannot_be_turned() {
    let mut core = browsing();
    press(&mut core, Key::R);
    assert_eq!(core.view().rotation, Rotation::None);
    assert_eq!(press(&mut core, Key::Enter), []);
}
