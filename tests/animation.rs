//! Seam 1: the app core playing animated GIF and WebP files.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use pixpixpix::core::{Command, Core, Event, Key, KeyPress, Screen};
use pixpixpix::decode::{DecodedImage, Frame};
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

fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

fn press(core: &mut Core, key: Key) -> Vec<Command> {
    core.handle(Event::Key(KeyPress::plain(key)))
}

/// The first frame of `name` decoded, gray 0, and the file an animation if
/// `animated`.
fn decoded(core: &mut Core, name: &str, animated: bool) -> Vec<Command> {
    let mut commands = core.handle(Event::Decoded {
        path: p(name),
        result: Ok(gray(0)),
    });
    if animated {
        commands.extend(core.handle(Event::Animated(p(name))));
    }
    commands
}

/// Frames gray 0, 1, 2, … lasting `delays`.
fn frames(delays: &[Duration]) -> Vec<Frame> {
    delays
        .iter()
        .zip(0..)
        .map(|(delay, value)| Frame {
            image: gray(value),
            delay: *delay,
        })
        .collect()
}

fn frames_decoded(core: &mut Core, name: &str, delays: &[Duration]) -> Vec<Command> {
    core.handle(Event::FramesDecoded {
        path: p(name),
        result: Ok(frames(delays)),
    })
}

/// The gray value of the image shown.
fn shown(core: &Core) -> Option<u8> {
    match &core.view().screen {
        Screen::Image { image, .. } => image.rgba.first().copied(),
        _ => None,
    }
}

/// Showing the animated `a.gif` of `a.gif`, `b.jpg`, `c.gif`, all decoded,
/// `c.gif` animated too; the frames of `a.gif` are asked for.
fn browsing() -> Core {
    let (mut core, _) = Core::start(Ok(p("a.gif")));
    decoded(&mut core, "a.gif", true);
    core.handle(Event::FirstFrameShown);
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(vec![p("a.gif"), p("b.jpg"), p("c.gif")]),
    });
    decoded(&mut core, "b.jpg", false);
    decoded(&mut core, "c.gif", true);
    core
}

/// [`browsing`] with the frames of `a.gif` in: 100, 50 and 200 ms.
fn playing(start: Instant) -> Core {
    let mut core = browsing();
    frames_decoded(&mut core, "a.gif", &[ms(100), ms(50), ms(200)]);
    core.handle(Event::Tick(start));
    core
}

#[test]
fn frames_are_decoded_only_after_the_first_frame_is_shown() {
    let (mut core, _) = Core::start(Ok(p("a.gif")));
    assert_eq!(decoded(&mut core, "a.gif", true), []);
    assert_eq!(shown(&core), Some(0));
    let commands = core.handle(Event::FirstFrameShown);
    assert!(commands.contains(&Command::DecodeFrames(p("a.gif"))));
    assert_eq!(core.view().next_frame_in, None);
}

#[test]
fn a_still_image_asks_for_no_frames() {
    let (mut core, _) = Core::start(Ok(p("a.gif")));
    decoded(&mut core, "a.gif", false);
    let commands = core.handle(Event::FirstFrameShown);
    assert!(!commands.contains(&Command::DecodeFrames(p("a.gif"))));
}

#[test]
fn the_frames_loop_as_the_clock_goes() {
    let start = Instant::now();
    let mut core = playing(start);
    assert_eq!(shown(&core), Some(0));
    assert_eq!(core.view().next_frame_in, Some(ms(100)));
    let mut at = |millis: u64| {
        core.handle(Event::Tick(start + ms(millis)));
        (shown(&core), core.view().next_frame_in)
    };
    assert_eq!(at(40), (Some(0), Some(ms(60))));
    assert_eq!(at(100), (Some(1), Some(ms(50))));
    assert_eq!(at(170), (Some(2), Some(ms(180))));
    // The loop is 350 ms long.
    assert_eq!(at(360), (Some(0), Some(ms(90))));
    assert_eq!(at(1175), (Some(1), Some(ms(25))));
}

#[test]
fn playing_starts_with_the_first_tick_after_the_frames_arrived() {
    let mut core = browsing();
    frames_decoded(&mut core, "a.gif", &[ms(100), ms(100)]);
    assert_eq!(core.view().next_frame_in, Some(Duration::ZERO));
    let start = Instant::now() + ms(5000);
    core.handle(Event::Tick(start));
    assert_eq!(shown(&core), Some(0));
    core.handle(Event::Tick(start + ms(150)));
    assert_eq!(shown(&core), Some(1));
}

#[test]
fn tiny_delays_are_played_slower_as_browsers_do() {
    let start = Instant::now();
    let mut core = browsing();
    frames_decoded(&mut core, "a.gif", &[Duration::ZERO, ms(10), ms(20)]);
    core.handle(Event::Tick(start));
    assert_eq!(core.view().next_frame_in, Some(ms(100)));
    core.handle(Event::Tick(start + ms(150)));
    assert_eq!(
        (shown(&core), core.view().next_frame_in),
        (Some(1), Some(ms(50)))
    );
    core.handle(Event::Tick(start + ms(205)));
    assert_eq!(
        (shown(&core), core.view().next_frame_in),
        (Some(2), Some(ms(15)))
    );
}

#[test]
fn the_first_frame_stays_the_image_already_decoded() {
    let start = Instant::now();
    let mut core = playing(start);
    core.handle(Event::Tick(start + ms(400)));
    let Screen::Image { image, .. } = &core.view().screen else {
        panic!("no image shown");
    };
    let first = core.view().band.first().unwrap().image.as_ref().unwrap();
    assert!(Arc::ptr_eq(&image.rgba, &first.rgba));
}

#[test]
fn frames_that_cannot_be_decoded_leave_the_first_frame() {
    let mut core = browsing();
    core.handle(Event::FramesDecoded {
        path: p("a.gif"),
        result: Err("too large to animate".into()),
    });
    core.handle(Event::Tick(Instant::now()));
    assert_eq!(shown(&core), Some(0));
    assert_eq!(core.view().next_frame_in, None);
}

#[test]
fn leaving_the_file_cancels_its_frames_and_a_late_result_is_ignored() {
    let mut core = browsing();
    let commands = press(&mut core, Key::Right);
    assert!(commands.contains(&Command::CancelFrames));
    frames_decoded(&mut core, "a.gif", &[ms(100), ms(100)]);
    assert_eq!(core.view().next_frame_in, None);
}

#[test]
fn leaving_a_playing_file_stops_it_and_coming_back_decodes_the_frames_again() {
    let start = Instant::now();
    let mut core = playing(start);
    let commands = press(&mut core, Key::Right);
    assert!(!commands.contains(&Command::CancelFrames));
    assert_eq!(core.view().next_frame_in, None);
    let commands = press(&mut core, Key::Left);
    assert!(commands.contains(&Command::DecodeFrames(p("a.gif"))));
    assert_eq!(shown(&core), Some(0));
}

#[test]
fn stepping_to_an_animated_neighbour_asks_for_its_frames() {
    let mut core = browsing();
    press(&mut core, Key::Right);
    let commands = press(&mut core, Key::Right);
    assert!(commands.contains(&Command::DecodeFrames(p("c.gif"))));
}

#[test]
fn stepping_between_still_images_asks_for_no_frames() {
    let (mut core, _) = Core::start(Ok(p("a.jpg")));
    decoded(&mut core, "a.jpg", false);
    core.handle(Event::FirstFrameShown);
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(vec![p("a.jpg"), p("b.jpg")]),
    });
    decoded(&mut core, "b.jpg", false);
    let commands = press(&mut core, Key::Right);
    assert_eq!(commands, []);
}

#[test]
fn band_thumbs_of_animations_are_marked() {
    let core = browsing();
    let marked: Vec<(&Path, bool)> = core
        .view()
        .band
        .iter()
        .map(|t| (t.path.as_path(), t.animated))
        .collect();
    assert_eq!(
        marked,
        [
            (p("a.gif").as_path(), true),
            (p("b.jpg").as_path(), false),
            (p("c.gif").as_path(), true)
        ]
    );
}

#[test]
fn a_file_decoded_again_is_animated_only_if_said_again() {
    let mut core = browsing();
    // c.gif leaves the window and comes back, now a still image.
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(vec![p("a.gif"), p("b.jpg")]),
    });
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(vec![p("a.gif"), p("b.jpg"), p("c.gif")]),
    });
    decoded(&mut core, "c.gif", false);
    let thumb = core.view().band.get(2).unwrap();
    assert_eq!(
        (thumb.path.as_path(), thumb.animated),
        (p("c.gif").as_path(), false)
    );
}

#[test]
fn c_copies_the_frame_shown() {
    let start = Instant::now();
    let mut core = playing(start);
    core.handle(Event::Tick(start + ms(120)));
    let commands = press(&mut core, Key::C);
    assert_eq!(
        commands,
        [Command::Copy {
            path: Some(p("a.gif")),
            image: gray(1),
            rotation: Rotation::None,
            shapes: Vec::new(),
        }]
    );
}

#[test]
fn entering_a_folder_whose_animated_first_image_is_preloaded_plays_it() {
    let (mut core, _) = Core::start(Ok(PathBuf::from("/r/p/a.jpg")));
    core.handle(Event::Decoded {
        path: PathBuf::from("/r/p/a.jpg"),
        result: Ok(gray(0)),
    });
    core.handle(Event::FirstFrameShown);
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/r/p"),
        result: Ok(vec![PathBuf::from("/r/p/a.jpg")]),
    });
    press(&mut core, Key::N);
    core.handle(Event::DirsListed {
        dir: PathBuf::from("/r/p"),
        result: Ok(vec![PathBuf::from("/r/p/x")]),
    });
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/r/p/x"),
        result: Ok(vec![PathBuf::from("/r/p/x/1.gif")]),
    });
    core.handle(Event::Decoded {
        path: PathBuf::from("/r/p/x/1.gif"),
        result: Ok(gray(0)),
    });
    // The preview does not play.
    let commands = core.handle(Event::Animated(PathBuf::from("/r/p/x/1.gif")));
    assert!(!commands.contains(&Command::DecodeFrames(PathBuf::from("/r/p/x/1.gif"))));
    let commands = press(&mut core, Key::Right);
    assert!(commands.contains(&Command::DecodeFrames(PathBuf::from("/r/p/x/1.gif"))));
}
