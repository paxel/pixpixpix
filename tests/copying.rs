//! Seam 1: the app core's `c`: putting the current image and its file on the
//! clipboard.

use std::path::PathBuf;
use std::sync::Arc;

use pixpixpix::core::{Command, Core, Event, Focus, Key, KeyPress, Notice};
use pixpixpix::decode::DecodedImage;
use pixpixpix::rotate::Rotation;

fn image() -> DecodedImage {
    DecodedImage {
        width: 2,
        height: 1,
        rgba: Arc::from(vec![1, 2, 3, 4, 5, 6, 7, 8]),
    }
}

fn p(name: &str) -> PathBuf {
    PathBuf::from(format!("/p/{name}"))
}

fn press(core: &mut Core, key: Key) -> Vec<Command> {
    core.handle(Event::Key(KeyPress::plain(key)))
}

/// Started on `start`, first frame shown, folder listed as `names`, every
/// file decoded.
fn browsing(start: &str, names: &[&str]) -> Core {
    let (mut core, _) = Core::start(Ok(p(start)));
    core.handle(Event::FirstFrameShown);
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(names.iter().map(|n| p(n)).collect()),
    });
    for name in names {
        core.handle(Event::Decoded {
            path: p(name),
            result: Ok(image()),
        });
    }
    core
}

fn copy(name: &str, rotation: Rotation) -> Command {
    Command::Copy {
        path: p(name),
        image: image(),
        rotation,
    }
}

#[test]
fn c_copies_the_shown_image_and_its_file() {
    let mut core = browsing("a.jpg", &["a.jpg", "b.jpg"]);
    assert_eq!(press(&mut core, Key::C), [copy("a.jpg", Rotation::None)]);
}

#[test]
fn the_copy_carries_the_pending_rotation() {
    let mut core = browsing("a.jpg", &["a.jpg"]);
    press(&mut core, Key::R);
    assert_eq!(press(&mut core, Key::C), [copy("a.jpg", Rotation::Right)]);
    press(&mut core, Key::L);
    press(&mut core, Key::L);
    assert_eq!(press(&mut core, Key::C), [copy("a.jpg", Rotation::Left)]);
}

#[test]
fn a_rotation_being_written_is_still_copied_as_shown() {
    let mut core = browsing("a.jpg", &["a.jpg"]);
    press(&mut core, Key::R);
    press(&mut core, Key::Enter);
    assert_eq!(press(&mut core, Key::C), [copy("a.jpg", Rotation::Right)]);
}

#[test]
fn nothing_is_copied_without_an_image() {
    let (mut core, _) = Core::start(Ok(p("a.jpg")));
    assert!(press(&mut core, Key::C).is_empty());
    core.handle(Event::Decoded {
        path: p("a.jpg"),
        result: Err("broken".into()),
    });
    assert!(press(&mut core, Key::C).is_empty());
}

#[test]
fn c_copies_only_in_the_image_view() {
    let mut core = browsing("a.jpg", &["a.jpg", "b.jpg"]);
    press(&mut core, Key::B);
    assert_eq!(core.view().focus, Focus::Band);
    assert!(press(&mut core, Key::C).is_empty());
    press(&mut core, Key::N);
    assert_eq!(core.view().focus, Focus::Dirs);
    assert!(press(&mut core, Key::C).is_empty());
}

#[test]
fn a_finished_copy_says_copied() {
    let mut core = browsing("a.jpg", &["a.jpg", "b.jpg"]);
    press(&mut core, Key::C);
    assert!(core.handle(Event::Copied(Ok(()))).is_empty());
    assert_eq!(core.view().key_line.notice, Some(Notice::Copied));
    assert_eq!(core.view().copy_error, None);
    press(&mut core, Key::Right);
    assert_eq!(core.view().key_line.notice, None);
}

#[test]
fn a_failed_copy_is_shown_until_the_next_image_or_copy() {
    let mut core = browsing("a.jpg", &["a.jpg", "b.jpg"]);
    press(&mut core, Key::C);
    core.handle(Event::Copied(Err("Cannot copy a.jpg: no display".into())));
    assert_eq!(
        core.view().copy_error.as_deref(),
        Some("Cannot copy a.jpg: no display")
    );
    assert_eq!(core.view().key_line.notice, None);
    press(&mut core, Key::C);
    assert_eq!(core.view().copy_error, None);
    core.handle(Event::Copied(Err("Cannot copy a.jpg: no display".into())));
    press(&mut core, Key::Right);
    assert_eq!(core.view().copy_error, None);
}
