//! Seam 1: the app core's one-shot dir column action `.` for dot entries.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pixpixpix::core::{Command, Core, Event, Key, KeyPress, Screen, Tag, Toggle};
use pixpixpix::decode::DecodedImage;

fn image() -> DecodedImage {
    DecodedImage {
        width: 1,
        height: 1,
        rgba: Arc::from(vec![0; 4]),
    }
}

fn paths(list: &[&str]) -> Vec<PathBuf> {
    list.iter().map(PathBuf::from).collect()
}

fn press(core: &mut Core, key: Key) -> Vec<Command> {
    core.handle(Event::Key(KeyPress::plain(key)))
}

fn shift(core: &mut Core, key: Key) -> Vec<Command> {
    let mut press = KeyPress::plain(key);
    press.shift = true;
    core.handle(Event::Key(press))
}

fn tags(core: &Core) -> Vec<(Toggle, bool)> {
    core.view()
        .tags
        .iter()
        .map(|Tag { toggle, kept }| (*toggle, *kept))
        .collect()
}

fn decoded(core: &mut Core, path: &str) -> Vec<Command> {
    core.handle(Event::Decoded {
        path: PathBuf::from(path),
        result: Ok(image()),
    })
}

fn files_listed(core: &mut Core, dir: &str, files: &[&str]) -> Vec<Command> {
    core.handle(Event::FolderListed {
        dir: PathBuf::from(dir),
        result: Ok(paths(files)),
    })
}

fn dirs_listed(core: &mut Core, dir: &str, dirs: &[&str]) -> Vec<Command> {
    core.handle(Event::DirsListed {
        dir: PathBuf::from(dir),
        result: Ok(paths(dirs)),
    })
}

fn column_dirs(core: &Core) -> Vec<PathBuf> {
    core.view()
        .dir_column
        .as_ref()
        .map(|c| c.dirs.clone())
        .unwrap_or_default()
}

fn current_dir(core: &Core) -> Option<PathBuf> {
    let column = core.view().dir_column.as_ref()?;
    column.highlight.and_then(|i| column.dirs.get(i)).cloned()
}

fn shows_image(core: &Core, path: &str) -> bool {
    matches!(&core.view().screen, Screen::Image { path: p, .. } if p == Path::new(path))
}

/// Showing `/p/a.jpg` of `/p/a.jpg`, `/p/.b.jpg`, `/p/c.jpg`, all decoded.
fn browsing() -> Core {
    let (mut core, _) = Core::start(Ok(PathBuf::from("/p/a.jpg")));
    decoded(&mut core, "/p/a.jpg");
    core.handle(Event::FirstFrameShown);
    files_listed(&mut core, "/p", &["/p/.b.jpg", "/p/a.jpg", "/p/c.jpg"]);
    decoded(&mut core, "/p/c.jpg");
    core
}

/// [`browsing`] with the column open: `/p` among `/.hidden` and `/q`, and
/// `/p`'s subfolders `.git`, `x` and `y` listed.
fn column_open() -> Core {
    let mut core = browsing();
    press(&mut core, Key::N);
    dirs_listed(&mut core, "/", &["/.hidden", "/p", "/q"]);
    dirs_listed(&mut core, "/p", &["/p/.git", "/p/x", "/p/y"]);
    core
}

#[test]
fn dot_entries_are_hidden_by_default() {
    let mut core = column_open();
    assert_eq!(column_dirs(&core), paths(&["/p", "/q"]));
    press(&mut core, Key::Tab);
    press(&mut core, Key::Right);
    assert!(shows_image(&core, "/p/c.jpg"));
}

#[test]
fn dot_shows_dot_folders_in_the_column_and_dot_files_in_navigation() {
    let mut core = column_open();
    press(&mut core, Key::Period);
    assert_eq!(column_dirs(&core), paths(&["/.hidden", "/p", "/q"]));
    // The highlight stays on the current folder.
    assert_eq!(current_dir(&core), Some(PathBuf::from("/p")));
    press(&mut core, Key::Tab);
    press(&mut core, Key::Left);
    assert!(matches!(
        &core.view().screen,
        Screen::Loading { path } if path == Path::new("/p/.b.jpg")
    ));
}

#[test]
fn dot_again_hides_them_and_moves_off_a_dot_file() {
    let mut core = column_open();
    press(&mut core, Key::Period);
    press(&mut core, Key::Tab);
    press(&mut core, Key::Left);
    decoded(&mut core, "/p/.b.jpg");
    press(&mut core, Key::Tab);
    press(&mut core, Key::Tab);
    press(&mut core, Key::Period);
    assert_eq!(column_dirs(&core), paths(&["/p", "/q"]));
    press(&mut core, Key::Tab);
    assert!(shows_image(&core, "/p/a.jpg"));
}

#[test]
fn right_goes_into_the_first_subfolder_shown() {
    let mut core = column_open();
    press(&mut core, Key::Right);
    assert_eq!(current_dir(&core), Some(PathBuf::from("/p/x")));
    let mut core = column_open();
    press(&mut core, Key::Period);
    press(&mut core, Key::Right);
    assert_eq!(current_dir(&core), Some(PathBuf::from("/p/.git")));
}

#[test]
fn leaving_the_level_resets_dots() {
    for key in [
        Key::Left,
        Key::Right,
        Key::Enter,
        Key::Escape,
        Key::Backspace,
        Key::N,
    ] {
        let mut core = column_open();
        press(&mut core, Key::Period);
        assert_eq!(tags(&core), [(Toggle::DotEntries, false)], "{key:?}");
        press(&mut core, key);
        assert!(tags(&core).is_empty(), "{key:?}");
    }
}

#[test]
fn going_to_a_sibling_keeps_dot_folders() {
    for key in [Key::Up, Key::Down] {
        let mut core = column_open();
        press(&mut core, Key::Period);
        press(&mut core, key);
        assert_eq!(
            column_dirs(&core),
            paths(&["/.hidden", "/p", "/q"]),
            "{key:?}"
        );
        assert_eq!(tags(&core), [(Toggle::DotEntries, false)], "{key:?}");
    }
}

#[test]
fn shift_dot_keeps_dot_entries_in_every_folder_entered() {
    let mut core = column_open();
    shift(&mut core, Key::Period);
    assert_eq!(tags(&core), [(Toggle::DotEntries, true)]);
    assert_eq!(column_dirs(&core), paths(&["/.hidden", "/p", "/q"]));
    press(&mut core, Key::Right);
    dirs_listed(&mut core, "/p", &["/p/.git", "/p/x", "/p/y"]);
    assert_eq!(column_dirs(&core), paths(&["/p/.git", "/p/x", "/p/y"]));
    press(&mut core, Key::Left);
    dirs_listed(&mut core, "/", &["/.hidden", "/p", "/q"]);
    assert_eq!(column_dirs(&core), paths(&["/.hidden", "/p", "/q"]));
    // `.` ends it.
    press(&mut core, Key::Period);
    assert!(tags(&core).is_empty());
    assert_eq!(column_dirs(&core), paths(&["/p", "/q"]));
}

#[test]
fn r_and_shift_r_do_nothing_in_the_column() {
    let mut core = column_open();
    assert!(press(&mut core, Key::R).is_empty());
    assert!(shift(&mut core, Key::R).is_empty());
    assert!(tags(&core).is_empty());
}

#[test]
fn dot_does_nothing_in_the_image_view() {
    let mut core = column_open();
    press(&mut core, Key::Tab);
    assert!(press(&mut core, Key::Period).is_empty());
    assert!(shows_image(&core, "/p/a.jpg"));
    press(&mut core, Key::Tab);
    assert_eq!(column_dirs(&core), paths(&["/p", "/q"]));
}
