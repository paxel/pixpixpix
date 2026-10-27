//! Seam 1: the app core's `x`: moving the current file to the trash.

use std::path::PathBuf;
use std::sync::Arc;

use pixpixpix::core::{Command, Core, Event, Focus, Key, KeyPress, Screen};
use pixpixpix::decode::DecodedImage;
use pixpixpix::folder;
use pixpixpix::rotate::Rotation;

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

fn trashed(core: &mut Core, name: &str, result: Result<(), String>) -> Vec<Command> {
    core.handle(Event::Trashed {
        path: p(name),
        result,
    })
}

fn press(core: &mut Core, key: Key) -> Vec<Command> {
    core.handle(Event::Key(KeyPress::plain(key)))
}

/// Started on `start`, first frame shown, folder listed as `names`, every
/// file decoded.
fn browsing(start: &str, names: &[&str]) -> Core {
    let (mut core, _) = Core::start(Ok(p(start)));
    core.handle(Event::FirstFrameShown);
    listed(&mut core, names);
    decoded(&mut core, start);
    for name in names {
        decoded(&mut core, name);
    }
    core
}

fn band(core: &Core) -> Vec<String> {
    core.view()
        .band
        .iter()
        .map(|t| folder::file_name(&t.path))
        .collect()
}

fn shown(name: &str) -> Screen {
    Screen::Image {
        path: p(name),
        image: image(),
    }
}

fn no_images() -> Screen {
    Screen::Error {
        path: None,
        message: "No images in /p".into(),
    }
}

#[test]
fn x_trashes_the_file_and_shows_the_next_at_once() {
    let mut core = browsing("b.jpg", &["a.jpg", "b.jpg", "c.jpg"]);
    assert_eq!(press(&mut core, Key::X), [Command::Trash(p("b.jpg"))]);
    assert_eq!(core.view().screen, shown("c.jpg"));
    assert_eq!(band(&core), ["a.jpg", "c.jpg"]);
    assert!(trashed(&mut core, "b.jpg", Ok(())).is_empty());
    assert_eq!(core.view().screen, shown("c.jpg"));
    assert_eq!(core.view().trash_error, None);
}

#[test]
fn trashing_the_last_file_shows_the_previous_one() {
    let mut core = browsing("c.jpg", &["a.jpg", "b.jpg", "c.jpg"]);
    assert_eq!(press(&mut core, Key::X), [Command::Trash(p("c.jpg"))]);
    assert_eq!(core.view().screen, shown("b.jpg"));
}

#[test]
fn trashing_the_only_file_says_no_images_and_opens_the_dir_column() {
    let mut core = browsing("a.jpg", &["a.jpg"]);
    assert_eq!(
        press(&mut core, Key::X),
        [
            Command::Trash(p("a.jpg")),
            Command::ListDirs(PathBuf::from("/")),
            Command::ListDirs(PathBuf::from("/p")),
        ]
    );
    assert_eq!(core.view().screen, no_images());
    assert!(core.view().band.is_empty());
    assert_eq!(core.view().focus, Focus::Dirs);
    assert_eq!(
        core.view().dir_column.as_ref().map(|c| c.dir.clone()),
        Some(PathBuf::from("/"))
    );
    core.handle(Event::DirsListed {
        dir: PathBuf::from("/p"),
        result: Ok(Vec::new()),
    });
    assert_eq!(core.view().screen, no_images());
    // The watcher's report of the empty folder changes nothing.
    assert!(listed(&mut core, &[]).is_empty());
    assert_eq!(core.view().screen, no_images());
    assert_eq!(core.view().focus, Focus::Dirs);
}

#[test]
fn trashing_the_only_file_focuses_an_open_dir_column() {
    let mut core = browsing("a.jpg", &["a.jpg"]);
    press(&mut core, Key::N);
    press(&mut core, Key::Tab);
    assert_eq!(core.view().focus, Focus::Image);
    assert_eq!(press(&mut core, Key::X), [Command::Trash(p("a.jpg"))]);
    assert_eq!(core.view().focus, Focus::Dirs);
    assert!(core.view().dir_column.is_some());
}

#[test]
fn a_failed_trash_shows_why_and_the_file_comes_back() {
    let mut core = browsing("a.jpg", &["a.jpg", "b.jpg"]);
    press(&mut core, Key::X);
    assert_eq!(core.view().screen, shown("b.jpg"));
    let commands = trashed(
        &mut core,
        "a.jpg",
        Err("Cannot trash a.jpg: Permission denied".into()),
    );
    assert_eq!(commands, [Command::Decode(p("a.jpg"))]);
    assert_eq!(
        core.view().trash_error.as_deref(),
        Some("Cannot trash a.jpg: Permission denied")
    );
    assert_eq!(core.view().screen, shown("b.jpg"));
    decoded(&mut core, "a.jpg");
    assert_eq!(band(&core), ["a.jpg", "b.jpg"]);
    press(&mut core, Key::Left);
    assert_eq!(core.view().screen, shown("a.jpg"));
    assert_eq!(core.view().trash_error, None);
}

#[test]
fn a_failed_trash_of_the_only_file_shows_it_again() {
    let mut core = browsing("a.jpg", &["a.jpg"]);
    press(&mut core, Key::X);
    assert_eq!(core.view().screen, no_images());
    let commands = trashed(&mut core, "a.jpg", Err("Cannot trash a.jpg: busy".into()));
    assert_eq!(commands, [Command::Decode(p("a.jpg"))]);
    assert_eq!(core.view().screen, Screen::Loading { path: p("a.jpg") });
    assert_eq!(
        core.view().trash_error.as_deref(),
        Some("Cannot trash a.jpg: busy")
    );
}

#[test]
fn the_watcher_reporting_the_removal_does_not_step_again() {
    let mut core = browsing("a.jpg", &["a.jpg", "b.jpg", "c.jpg"]);
    press(&mut core, Key::X);
    assert!(listed(&mut core, &["b.jpg", "c.jpg"]).is_empty());
    assert_eq!(core.view().screen, shown("b.jpg"));
    trashed(&mut core, "a.jpg", Ok(()));
    assert!(listed(&mut core, &["b.jpg", "c.jpg"]).is_empty());
    assert_eq!(core.view().screen, shown("b.jpg"));
    assert_eq!(band(&core), ["b.jpg", "c.jpg"]);
}

#[test]
fn a_listing_taken_before_the_trash_does_not_bring_the_file_back() {
    let mut core = browsing("a.jpg", &["a.jpg", "b.jpg"]);
    press(&mut core, Key::X);
    assert!(listed(&mut core, &["a.jpg", "b.jpg"]).is_empty());
    assert_eq!(band(&core), ["b.jpg"]);
    press(&mut core, Key::Left);
    assert_eq!(core.view().screen, shown("b.jpg"));
}

#[test]
fn a_trashed_file_stays_gone_without_the_watcher() {
    let mut core = browsing("a.jpg", &["a.jpg", "b.jpg"]);
    press(&mut core, Key::X);
    trashed(&mut core, "a.jpg", Ok(()));
    // `.` in the dir column looks at the last listing again.
    press(&mut core, Key::N);
    press(&mut core, Key::Period);
    press(&mut core, Key::Tab);
    press(&mut core, Key::Left);
    assert_eq!(core.view().screen, shown("b.jpg"));
    assert_eq!(band(&core), ["b.jpg"]);
}

#[test]
fn the_pending_rotation_goes_with_the_file() {
    let mut core = browsing("a.jpg", &["a.jpg", "b.jpg"]);
    press(&mut core, Key::R);
    assert_eq!(core.view().rotation, Rotation::Right);
    press(&mut core, Key::X);
    assert_eq!(core.view().rotation, Rotation::None);
}

#[test]
fn a_broken_file_can_be_trashed() {
    let (mut core, _) = Core::start(Ok(p("a.jpg")));
    core.handle(Event::FirstFrameShown);
    listed(&mut core, &["a.jpg", "b.jpg"]);
    core.handle(Event::Decoded {
        path: p("a.jpg"),
        result: Err("unexpected end of file".into()),
    });
    decoded(&mut core, "b.jpg");
    assert_eq!(press(&mut core, Key::X), [Command::Trash(p("a.jpg"))]);
    assert_eq!(core.view().screen, shown("b.jpg"));
}

#[test]
fn x_waits_for_the_folder_listing() {
    let (mut core, _) = Core::start(Ok(p("a.jpg")));
    decoded(&mut core, "a.jpg");
    assert!(press(&mut core, Key::X).is_empty());
    core.handle(Event::FirstFrameShown);
    assert!(press(&mut core, Key::X).is_empty());
    assert_eq!(core.view().screen, shown("a.jpg"));
}

#[test]
fn x_belongs_to_the_image_view() {
    let mut core = browsing("a.jpg", &["a.jpg", "b.jpg"]);
    press(&mut core, Key::B);
    assert!(press(&mut core, Key::X).is_empty());
    press(&mut core, Key::Escape);
    press(&mut core, Key::N);
    assert!(press(&mut core, Key::X).is_empty());
    press(&mut core, Key::N);
    press(&mut core, Key::S);
    assert!(press(&mut core, Key::X).is_empty());
    assert_eq!(core.view().screen, shown("a.jpg"));
}

#[test]
fn the_key_line_lists_x() {
    let core = browsing("a.jpg", &["a.jpg"]);
    let keys = &core.view().key_line.keys;
    assert!(keys.iter().any(|k| k.keys == "X" && k.action == "trash"));
}

fn restored(core: &mut Core, names: &[&str], result: Result<(), String>) -> Vec<Command> {
    core.handle(Event::Restored {
        paths: names.iter().map(|n| p(n)).collect(),
        result,
    })
}

fn current(core: &Core) -> Option<PathBuf> {
    match &core.view().screen {
        Screen::Image { path, .. } | Screen::Loading { path } => Some(path.clone()),
        _ => None,
    }
}

fn hint(core: &Core, keys: &str) -> Option<&'static str> {
    core.view()
        .key_line
        .keys
        .iter()
        .find(|k| k.keys == keys)
        .map(|k| k.action)
}

#[test]
fn u_puts_the_trashed_files_back_newest_first() {
    let mut core = browsing("a.jpg", &["a.jpg", "b.jpg", "c.jpg"]);
    assert_eq!(hint(&core, "U"), None);
    press(&mut core, Key::X);
    trashed(&mut core, "a.jpg", Ok(()));
    press(&mut core, Key::X);
    trashed(&mut core, "b.jpg", Ok(()));
    assert_eq!(hint(&core, "U"), Some("undo trash"));
    assert_eq!(
        press(&mut core, Key::U),
        [Command::Restore(vec![p("b.jpg")])]
    );
    assert_eq!(
        press(&mut core, Key::U),
        [Command::Restore(vec![p("a.jpg")])]
    );
    assert!(press(&mut core, Key::U).is_empty());
    assert_eq!(hint(&core, "U"), None);
}

#[test]
fn a_restored_file_shows_and_its_folder_is_read_again() {
    let mut core = browsing("a.jpg", &["a.jpg", "b.jpg"]);
    press(&mut core, Key::X);
    trashed(&mut core, "a.jpg", Ok(()));
    assert_eq!(current(&core), Some(p("b.jpg")));
    press(&mut core, Key::U);
    let commands = restored(&mut core, &["a.jpg"], Ok(()));
    assert!(commands.contains(&Command::ListFolder(PathBuf::from("/p"))));
    assert_eq!(current(&core), Some(p("a.jpg")));
}

#[test]
fn a_file_restored_after_moving_on_shows_in_its_own_folder() {
    let mut core = browsing("a.jpg", &["a.jpg", "b.jpg"]);
    press(&mut core, Key::X);
    trashed(&mut core, "a.jpg", Ok(()));
    core.handle(Event::Opened(PathBuf::from("/q/z.jpg")));
    press(&mut core, Key::U);
    let commands = restored(&mut core, &["a.jpg"], Ok(()));
    assert!(commands.contains(&Command::ListFolder(PathBuf::from("/p"))));
    assert_eq!(current(&core), Some(p("a.jpg")));
}

#[test]
fn a_file_that_was_not_trashed_is_not_put_back() {
    let mut core = browsing("a.jpg", &["a.jpg", "b.jpg"]);
    press(&mut core, Key::X);
    trashed(&mut core, "a.jpg", Err("Cannot trash a.jpg: no".into()));
    assert!(press(&mut core, Key::U).is_empty());
}

#[test]
fn a_failed_restore_says_why() {
    let mut core = browsing("a.jpg", &["a.jpg", "b.jpg"]);
    press(&mut core, Key::X);
    trashed(&mut core, "a.jpg", Ok(()));
    press(&mut core, Key::U);
    restored(
        &mut core,
        &["a.jpg"],
        Err("Cannot restore a.jpg: a file of that name is back there".into()),
    );
    assert_eq!(
        core.view().trash_error.as_deref(),
        Some("Cannot restore a.jpg: a file of that name is back there")
    );
}
