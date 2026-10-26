//! Seam 1: the app core following changes of the watched folder.

use std::path::PathBuf;
use std::sync::Arc;

use pixpixpix::core::{Command, Core, Event, Key, KeyPress, Notice, Points, Scale, Screen};
use pixpixpix::decode::DecodedImage;
use pixpixpix::folder;

fn image() -> DecodedImage {
    DecodedImage {
        width: 400,
        height: 200,
        rgba: Arc::from(vec![0; 4]),
    }
}

fn p(name: &str) -> PathBuf {
    PathBuf::from(format!("/p/{name}"))
}

fn decoded(core: &mut Core, name: &str) -> Vec<Command> {
    core.handle(Event::Decoded {
        path: p(name),
        result: Ok(image()),
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

/// Started on `start`, first frame shown, folder listed as `names`.
fn browsing(start: &str, names: &[&str]) -> Core {
    let (mut core, _) = Core::start(Ok(p(start)));
    core.handle(Event::FirstFrameShown);
    listed(&mut core, names);
    core
}

/// Like [`browsing`], with every listed file decoded.
fn all_decoded(start: &str, names: &[&str]) -> Core {
    let mut core = browsing(start, names);
    decoded(&mut core, start);
    for name in names {
        decoded(&mut core, name);
    }
    core
}

fn band(core: &Core) -> Vec<(String, bool)> {
    core.view()
        .band
        .iter()
        .map(|t| (folder::file_name(&t.path), t.current))
        .collect()
}

fn shown(name: &str) -> Screen {
    Screen::Image {
        path: p(name),
        image: image(),
    }
}

#[test]
fn an_added_file_is_decoded_and_reachable() {
    let mut core = all_decoded("a.jpg", &["a.jpg", "b.jpg"]);
    let commands = listed(&mut core, &["a.jpg", "b.jpg", "c.jpg"]);
    assert_eq!(commands, [Command::Decode(p("c.jpg"))]);
    press(&mut core, Key::Right);
    press(&mut core, Key::Right);
    assert_eq!(core.view().screen, Screen::Loading { path: p("c.jpg") });
}

#[test]
fn a_removed_neighbour_leaves_band_and_navigation() {
    let mut core = all_decoded("a.jpg", &["a.jpg", "b.jpg", "c.jpg"]);
    assert!(listed(&mut core, &["a.jpg", "c.jpg"]).is_empty());
    assert_eq!(
        band(&core),
        [("a.jpg".to_owned(), true), ("c.jpg".to_owned(), false)]
    );
    press(&mut core, Key::Right);
    assert_eq!(core.view().screen, shown("c.jpg"));
}

#[test]
fn a_removed_current_file_gives_way_to_the_next() {
    let mut core = all_decoded("b.jpg", &["a.jpg", "b.jpg", "c.jpg"]);
    listed(&mut core, &["a.jpg", "c.jpg"]);
    assert_eq!(core.view().screen, shown("c.jpg"));
    assert_eq!(
        band(&core),
        [("a.jpg".to_owned(), false), ("c.jpg".to_owned(), true)]
    );
}

#[test]
fn a_removed_last_file_gives_way_to_the_one_before() {
    let mut core = all_decoded("c.jpg", &["a.jpg", "b.jpg", "c.jpg"]);
    press(&mut core, Key::Right);
    assert_eq!(core.view().key_line.notice, Some(Notice::LastImage));
    listed(&mut core, &["a.jpg", "b.jpg"]);
    assert_eq!(core.view().screen, shown("b.jpg"));
    assert_eq!(core.view().key_line.notice, None);
}

#[test]
fn a_removed_current_file_still_loading_is_cancelled() {
    let mut core = browsing("a.jpg", &["a.jpg", "b.jpg"]);
    let commands = listed(&mut core, &["b.jpg"]);
    assert_eq!(
        commands,
        [
            Command::Decode(p("b.jpg")),
            Command::CancelDecode(p("a.jpg"))
        ]
    );
    assert_eq!(core.view().screen, Screen::Loading { path: p("b.jpg") });
    assert!(decoded(&mut core, "a.jpg").is_empty());
}

#[test]
fn a_rename_moves_the_file_in_order_and_band() {
    let mut core = all_decoded("a.jpg", &["a.jpg", "b.jpg", "c.jpg"]);
    let commands = listed(&mut core, &["a.jpg", "c.jpg", "d.jpg"]);
    assert_eq!(commands, [Command::Decode(p("d.jpg"))]);
    decoded(&mut core, "d.jpg");
    assert_eq!(
        band(&core),
        [
            ("a.jpg".to_owned(), true),
            ("c.jpg".to_owned(), false),
            ("d.jpg".to_owned(), false)
        ]
    );
    press(&mut core, Key::Right);
    press(&mut core, Key::Right);
    assert_eq!(core.view().screen, shown("d.jpg"));
}

#[test]
fn listing_the_same_files_again_changes_nothing() {
    let mut core = all_decoded("a.jpg", &["a.jpg", "b.jpg"]);
    core.handle(Event::Viewport(Points { x: 200.0, y: 200.0 }));
    press(&mut core, Key::Plus);
    let before = core.view().clone();
    assert!(matches!(before.zoom.scale, Scale::Exact(_)));
    assert!(listed(&mut core, &["a.jpg", "b.jpg"]).is_empty());
    assert_eq!(*core.view(), before);
}

#[test]
fn a_current_file_never_listed_is_kept() {
    let mut core = browsing(".hidden.jpg", &["a.jpg", "b.jpg"]);
    assert_eq!(
        core.view().screen,
        Screen::Loading {
            path: p(".hidden.jpg")
        }
    );
    listed(&mut core, &["a.jpg"]);
    assert_eq!(
        core.view().screen,
        Screen::Loading {
            path: p(".hidden.jpg")
        }
    );
}

#[test]
fn removing_every_file_says_the_folder_is_empty_until_one_appears() {
    let mut core = all_decoded("a.jpg", &["a.jpg"]);
    let commands = listed(&mut core, &[]);
    assert!(commands.is_empty());
    assert_eq!(
        core.view().screen,
        Screen::Error {
            path: None,
            message: "No images in /p".into()
        }
    );
    assert!(core.view().band.is_empty());
    assert_eq!(listed(&mut core, &["b.jpg"]), [Command::Decode(p("b.jpg"))]);
    assert_eq!(core.view().screen, Screen::Loading { path: p("b.jpg") });
}

#[test]
fn a_listing_of_another_folder_is_ignored() {
    let mut core = all_decoded("a.jpg", &["a.jpg"]);
    let commands = core.handle(Event::FolderListed {
        dir: PathBuf::from("/q"),
        result: Ok(Vec::new()),
    });
    assert!(commands.is_empty());
    assert_eq!(core.view().screen, shown("a.jpg"));
}
