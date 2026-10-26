//! Seam 1: the app core stepping through the images of multi-image files.

use std::path::PathBuf;
use std::sync::Arc;

use pixpixpix::core::{Command, Core, Event, InnerImages, Key, KeyPress, Screen};
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

/// The first image of `name` decoded, gray 0, holding `images` images.
fn decoded(core: &mut Core, name: &str, images: usize) {
    core.handle(Event::Decoded {
        path: p(name),
        result: Ok(gray(0)),
    });
    if images > 1 {
        core.handle(Event::Images {
            path: p(name),
            count: images,
        });
    }
}

fn inner_decoded(core: &mut Core, name: &str, index: usize, value: u8) -> Vec<Command> {
    core.handle(Event::InnerDecoded {
        path: p(name),
        index,
        result: Ok(gray(value)),
    })
}

/// The gray value of the image shown.
fn shown(core: &Core) -> Option<u8> {
    match &core.view().screen {
        Screen::Image { image, .. } => image.rgba.first().copied(),
        _ => None,
    }
}

fn inner(index: usize, count: usize) -> Option<InnerImages> {
    Some(InnerImages { index, count })
}

/// Showing `a.tiff` of three pages, of `a.tiff`, `b.jpg` and `c.mpo`, all
/// decoded, `c.mpo` holding two images.
fn browsing() -> Core {
    let (mut core, _) = Core::start(Ok(p("a.tiff")));
    decoded(&mut core, "a.tiff", 3);
    core.handle(Event::FirstFrameShown);
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(vec![p("a.tiff"), p("b.jpg"), p("c.mpo")]),
    });
    decoded(&mut core, "b.jpg", 1);
    decoded(&mut core, "c.mpo", 2);
    core
}

#[test]
fn a_multi_image_file_shows_its_first_image_and_which_it_is() {
    let core = browsing();
    assert_eq!(shown(&core), Some(0));
    assert_eq!(core.view().inner, inner(0, 3));
}

#[test]
fn a_single_image_file_shows_no_hint() {
    let mut core = browsing();
    press(&mut core, Key::Right);
    assert_eq!(core.view().inner, None);
    let thumbs: Vec<usize> = core.view().band.iter().map(|t| t.images).collect();
    assert_eq!(thumbs, [3, 1, 2]);
}

#[test]
fn down_decodes_the_next_image_and_shows_it_once_decoded() {
    let mut core = browsing();
    let commands = press(&mut core, Key::Down);
    assert_eq!(
        commands,
        [Command::DecodeInner {
            path: p("a.tiff"),
            index: 1,
        }]
    );
    assert_eq!(core.view().inner, inner(1, 3));
    // The image shown before stays until the next one is in.
    assert_eq!(shown(&core), Some(0));
    inner_decoded(&mut core, "a.tiff", 1, 1);
    assert_eq!(shown(&core), Some(1));
}

#[test]
fn up_goes_back_to_the_first_image_without_a_decode() {
    let mut core = browsing();
    press(&mut core, Key::Down);
    inner_decoded(&mut core, "a.tiff", 1, 1);
    assert_eq!(press(&mut core, Key::Up), []);
    assert_eq!(shown(&core), Some(0));
    assert_eq!(core.view().inner, inner(0, 3));
}

#[test]
fn stepping_stops_at_both_ends() {
    let mut core = browsing();
    assert_eq!(press(&mut core, Key::Up), []);
    assert_eq!(core.view().inner, inner(0, 3));
    press(&mut core, Key::Down);
    inner_decoded(&mut core, "a.tiff", 1, 1);
    press(&mut core, Key::Down);
    inner_decoded(&mut core, "a.tiff", 2, 2);
    assert_eq!(press(&mut core, Key::Down), []);
    assert_eq!(core.view().inner, inner(2, 3));
    assert_eq!(shown(&core), Some(2));
}

#[test]
fn a_late_image_no_longer_wanted_is_ignored() {
    let mut core = browsing();
    press(&mut core, Key::Down);
    press(&mut core, Key::Down);
    inner_decoded(&mut core, "a.tiff", 1, 1);
    assert_eq!(shown(&core), Some(0));
    inner_decoded(&mut core, "a.tiff", 2, 2);
    assert_eq!(shown(&core), Some(2));
}

#[test]
fn going_back_to_the_first_image_cancels_a_running_decode() {
    let mut core = browsing();
    press(&mut core, Key::Down);
    assert_eq!(press(&mut core, Key::Up), [Command::CancelInner]);
    inner_decoded(&mut core, "a.tiff", 1, 1);
    assert_eq!(shown(&core), Some(0));
}

#[test]
fn an_image_that_fails_shows_its_error_and_stepping_goes_on() {
    let mut core = browsing();
    press(&mut core, Key::Down);
    core.handle(Event::InnerDecoded {
        path: p("a.tiff"),
        index: 1,
        result: Err("broken page".into()),
    });
    assert_eq!(
        core.view().screen,
        Screen::Error {
            path: Some(p("a.tiff")),
            message: "broken page".into(),
        }
    );
    press(&mut core, Key::Up);
    assert_eq!(shown(&core), Some(0));
}

#[test]
fn the_index_resets_when_the_file_changes() {
    let mut core = browsing();
    press(&mut core, Key::Down);
    inner_decoded(&mut core, "a.tiff", 1, 1);
    press(&mut core, Key::Right);
    press(&mut core, Key::Right);
    assert_eq!(core.view().inner, inner(0, 2));
    press(&mut core, Key::Left);
    press(&mut core, Key::Left);
    assert_eq!(core.view().inner, inner(0, 3));
    assert_eq!(shown(&core), Some(0));
}

#[test]
fn leaving_the_file_cancels_its_running_decode() {
    let mut core = browsing();
    press(&mut core, Key::Down);
    let commands = press(&mut core, Key::Right);
    assert!(commands.contains(&Command::CancelInner));
    inner_decoded(&mut core, "a.tiff", 1, 1);
    assert_eq!(core.view().inner, None);
}

#[test]
fn clicking_the_current_thumb_shows_the_first_image_again() {
    let mut core = browsing();
    press(&mut core, Key::Down);
    inner_decoded(&mut core, "a.tiff", 1, 1);
    core.handle(Event::Show(p("a.tiff")));
    assert_eq!(core.view().inner, inner(0, 3));
    assert_eq!(shown(&core), Some(0));
}

#[test]
fn up_and_down_do_nothing_in_a_single_image_file() {
    let mut core = browsing();
    press(&mut core, Key::Right);
    assert_eq!(press(&mut core, Key::Down), []);
    assert_eq!(press(&mut core, Key::Up), []);
}

#[test]
fn c_copies_the_image_shown() {
    let mut core = browsing();
    press(&mut core, Key::Down);
    inner_decoded(&mut core, "a.tiff", 1, 1);
    assert_eq!(
        press(&mut core, Key::C),
        [Command::Copy {
            path: Some(p("a.tiff")),
            image: gray(1),
            rotation: Rotation::None,
            shapes: Vec::new(),
        }]
    );
}

#[test]
fn the_key_line_lists_up_and_down_only_for_a_multi_image_file() {
    let mut core = browsing();
    let keys =
        |core: &Core| -> Vec<&str> { core.view().key_line.keys.iter().map(|k| k.keys).collect() };
    assert_eq!(keys(&core).get(1), Some(&"Up/Down"));
    press(&mut core, Key::Right);
    assert!(!keys(&core).contains(&"Up/Down"));
}

#[test]
fn a_previewed_folder_shows_no_hint() {
    let (mut core, _) = Core::start(Ok(PathBuf::from("/r/p/a.tiff")));
    core.handle(Event::Decoded {
        path: PathBuf::from("/r/p/a.tiff"),
        result: Ok(gray(0)),
    });
    core.handle(Event::Images {
        path: PathBuf::from("/r/p/a.tiff"),
        count: 2,
    });
    core.handle(Event::FirstFrameShown);
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/r/p"),
        result: Ok(vec![PathBuf::from("/r/p/a.tiff")]),
    });
    assert_eq!(core.view().inner, inner(0, 2));
    press(&mut core, Key::N);
    core.handle(Event::DirsListed {
        dir: PathBuf::from("/r/p"),
        result: Ok(vec![PathBuf::from("/r/p/x")]),
    });
    assert_eq!(core.view().inner, None);
}
