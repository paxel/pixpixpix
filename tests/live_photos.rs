//! Seam 1: the app core with Apple Live Photos and Google/Samsung Motion
//! Photos: pairing, hiding the MOV, trashing both and playing the motion.

use std::path::PathBuf;
use std::sync::Arc;

use pixpixpix::core::{Command, Core, Event, Key, KeyPress, MotionKind, Screen, Video};
use pixpixpix::decode::DecodedImage;
use pixpixpix::folder;

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

fn decoded(core: &mut Core, name: &str) -> Vec<Command> {
    core.handle(Event::Decoded {
        path: p(name),
        result: Ok(gray(0)),
    })
}

fn listed(core: &mut Core, names: &[&str]) -> Vec<Command> {
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(names.iter().map(|n| p(n)).collect()),
    })
}

fn press(core: &mut Core, key: Key) -> Vec<Command> {
    core.handle(Event::Key(KeyPress::plain(key)))
}

fn trashed(core: &mut Core, name: &str, result: Result<(), String>) -> Vec<Command> {
    core.handle(Event::Trashed {
        path: p(name),
        result,
    })
}

/// Started on `start`, first frame shown, folder listed as `names`, every
/// file decoded.
fn browsing(start: &str, names: &[&str]) -> Core {
    let (mut core, _) = Core::start(Ok(p(start)));
    decoded(&mut core, start);
    core.handle(Event::FirstFrameShown);
    listed(&mut core, names);
    for name in names {
        decoded(&mut core, name);
    }
    core
}

fn band(core: &Core) -> Vec<(String, Option<MotionKind>)> {
    core.view()
        .band
        .iter()
        .map(|t| (folder::file_name(&t.path), t.motion))
        .collect()
}

fn shown(core: &Core) -> Option<String> {
    match &core.view().screen {
        Screen::Image { path, .. } => Some(folder::file_name(path)),
        _ => None,
    }
}

/// The gray value of the image shown.
fn shown_gray(core: &Core) -> Option<u8> {
    match &core.view().screen {
        Screen::Image { image, .. } => image.rgba.first().copied(),
        _ => None,
    }
}

fn key_line(core: &Core) -> Vec<(&'static str, &'static str)> {
    let keys = &core.view().key_line.keys;
    keys.iter().map(|k| (k.keys, k.action)).collect()
}

const LIVE: &[&str] = &["IMG_1.HEIC", "IMG_1.MOV", "IMG_2.JPG", "z.mp4"];

#[test]
fn a_live_photo_is_one_entry_and_its_mov_is_hidden() {
    let mut core = browsing("IMG_1.HEIC", LIVE);
    assert_eq!(
        band(&core),
        [
            ("IMG_1.HEIC".to_owned(), Some(MotionKind::LivePhoto)),
            ("IMG_2.JPG".to_owned(), None),
            ("z.mp4".to_owned(), None),
        ]
    );
    assert_eq!(core.view().motion, Some(MotionKind::LivePhoto));
    press(&mut core, Key::Right);
    assert_eq!(shown(&core).as_deref(), Some("IMG_2.JPG"));
    assert_eq!(core.view().motion, None);
    press(&mut core, Key::Left);
    assert_eq!(shown(&core).as_deref(), Some("IMG_1.HEIC"));
}

#[test]
fn pairing_ignores_case_and_wants_the_same_base_name() {
    let core = browsing("img_1.heic", &["img_1.heic", "IMG_1.mov", "IMG_10.MOV"]);
    assert_eq!(
        band(&core),
        [
            ("img_1.heic".to_owned(), Some(MotionKind::LivePhoto)),
            ("IMG_10.MOV".to_owned(), None),
        ]
    );
}

#[test]
fn listings_keep_the_pairing_up_to_date() {
    let mut core = browsing("IMG_1.HEIC", &["IMG_1.HEIC", "IMG_2.JPG"]);
    assert_eq!(core.view().motion, None);
    listed(&mut core, &["IMG_1.HEIC", "IMG_1.MOV", "IMG_2.JPG"]);
    assert_eq!(core.view().motion, Some(MotionKind::LivePhoto));
    assert_eq!(band(&core).len(), 2);
    listed(&mut core, &["IMG_1.HEIC", "IMG_2.JPG"]);
    assert_eq!(core.view().motion, None);
}

#[test]
fn starting_on_the_mov_of_a_live_photo_shows_the_photo() {
    let (mut core, _) = Core::start(Ok(p("IMG_1.MOV")));
    decoded(&mut core, "IMG_1.MOV");
    core.handle(Event::FirstFrameShown);
    let commands = listed(&mut core, LIVE);
    assert!(commands.contains(&Command::Decode(p("IMG_1.HEIC"))));
    decoded(&mut core, "IMG_1.HEIC");
    assert_eq!(shown(&core).as_deref(), Some("IMG_1.HEIC"));
    assert_eq!(core.view().motion, Some(MotionKind::LivePhoto));
}

#[test]
fn x_on_a_live_photo_trashes_the_photo_and_its_mov() {
    let mut core = browsing("IMG_1.HEIC", LIVE);
    let commands = press(&mut core, Key::X);
    assert_eq!(
        commands.get(..2),
        Some(
            [
                Command::Trash(p("IMG_1.HEIC")),
                Command::Trash(p("IMG_1.MOV"))
            ]
            .as_slice()
        )
    );
    assert_eq!(shown(&core).as_deref(), Some("IMG_2.JPG"));
    trashed(&mut core, "IMG_1.HEIC", Ok(()));
    trashed(&mut core, "IMG_1.MOV", Ok(()));
    // The watcher's listing taken before the trash brings neither back.
    assert_eq!(core.view().trash_error, None);
    assert_eq!(band(&core).len(), 2);
}

#[test]
fn a_mov_that_cannot_be_trashed_comes_back_on_its_own_and_says_so() {
    let mut core = browsing("IMG_1.HEIC", LIVE);
    press(&mut core, Key::X);
    trashed(&mut core, "IMG_1.HEIC", Ok(()));
    trashed(
        &mut core,
        "IMG_1.MOV",
        Err("Cannot trash IMG_1.MOV: denied".into()),
    );
    assert_eq!(
        core.view().trash_error.as_deref(),
        Some("Cannot trash IMG_1.MOV: denied")
    );
    decoded(&mut core, "IMG_1.MOV");
    let names: Vec<String> = band(&core).into_iter().map(|(name, _)| name).collect();
    assert!(names.contains(&"IMG_1.MOV".to_owned()));
    assert!(!names.contains(&"IMG_1.HEIC".to_owned()));
}

#[test]
fn when_both_files_cannot_be_trashed_both_failures_show() {
    let mut core = browsing("IMG_1.HEIC", LIVE);
    press(&mut core, Key::X);
    trashed(
        &mut core,
        "IMG_1.HEIC",
        Err("Cannot trash IMG_1.HEIC: denied".into()),
    );
    trashed(
        &mut core,
        "IMG_1.MOV",
        Err("Cannot trash IMG_1.MOV: denied".into()),
    );
    assert_eq!(
        core.view().trash_error.as_deref(),
        Some("Cannot trash IMG_1.HEIC: denied\nCannot trash IMG_1.MOV: denied")
    );
    // Both are back as one Live Photo.
    decoded(&mut core, "IMG_1.HEIC");
    assert_eq!(
        band(&core).first(),
        Some(&("IMG_1.HEIC".to_owned(), Some(MotionKind::LivePhoto)))
    );
}

#[test]
fn space_plays_the_mov_of_a_live_photo_over_its_still() {
    let mut core = browsing("IMG_1.HEIC", LIVE);
    assert_eq!(core.view().video, None);
    assert_eq!(key_line(&core).get(1), Some(&("Space", "play motion")));
    assert_eq!(
        press(&mut core, Key::Space),
        [Command::PlayVideo {
            path: p("IMG_1.MOV"),
            muted: false,
            paused: false,
        }]
    );
    assert_eq!(
        core.view().video,
        Some(Video {
            playing: true,
            muted: false,
            error: None,
        })
    );
    core.handle(Event::VideoFrame {
        path: p("IMG_1.MOV"),
        image: gray(7),
    });
    assert_eq!(shown(&core).as_deref(), Some("IMG_1.HEIC"));
    assert_eq!(shown_gray(&core), Some(7));
    assert!(!key_line(&core).contains(&("L/R", "rotate")));
    assert_eq!(
        core.handle(Event::VideoEnded(p("IMG_1.MOV"))),
        [Command::StopVideo]
    );
    assert_eq!(shown_gray(&core), Some(0));
    assert_eq!(core.view().video, None);
    assert!(key_line(&core).contains(&("L/R", "rotate")));
}

#[test]
fn a_motion_photo_that_cannot_play_shows_why() {
    let mut core = browsing("IMG_1.HEIC", LIVE);
    press(&mut core, Key::Space);
    core.handle(Event::VideoFailed {
        path: p("IMG_1.MOV"),
        message: "no mpv".into(),
    });
    assert_eq!(
        core.view().video,
        Some(Video {
            playing: false,
            muted: false,
            error: Some("no mpv".into()),
        })
    );
}

#[test]
fn jpegs_are_asked_about_motion_only_after_the_first_frame() {
    let (mut core, _) = Core::start(Ok(p("a.jpg")));
    assert!(!decoded(&mut core, "a.jpg").contains(&Command::FindMotion(p("a.jpg"))));
    let commands = core.handle(Event::FirstFrameShown);
    assert!(commands.contains(&Command::FindMotion(p("a.jpg"))));
    let commands = listed(&mut core, &["a.jpg", "b.heic", "b.mov", "c.png", "d.jpg"]);
    assert!(!commands.contains(&Command::FindMotion(p("a.jpg"))));
    let mut asked = Vec::new();
    for name in ["b.heic", "c.png", "d.jpg"] {
        asked.extend(
            decoded(&mut core, name)
                .into_iter()
                .filter(|c| matches!(c, Command::FindMotion(_))),
        );
    }
    assert_eq!(asked, [Command::FindMotion(p("d.jpg"))]);
}

#[test]
fn a_motion_photo_is_marked_and_space_plays_it() {
    let mut core = browsing("a.jpg", &["a.jpg", "b.jpg"]);
    core.handle(Event::MotionPhoto(p("b.jpg")));
    assert_eq!(
        band(&core),
        [
            ("a.jpg".to_owned(), None),
            ("b.jpg".to_owned(), Some(MotionKind::MotionPhoto)),
        ]
    );
    assert!(press(&mut core, Key::Space).is_empty());
    press(&mut core, Key::Right);
    assert_eq!(core.view().motion, Some(MotionKind::MotionPhoto));
    assert_eq!(
        press(&mut core, Key::Space),
        [Command::PlayVideo {
            path: p("b.jpg"),
            muted: false,
            paused: false,
        }]
    );
    core.handle(Event::VideoFrame {
        path: p("b.jpg"),
        image: gray(9),
    });
    assert_eq!(shown_gray(&core), Some(9));
    assert_eq!(press(&mut core, Key::Left), [Command::StopVideo]);
    assert_eq!(core.view().video, None);
}
