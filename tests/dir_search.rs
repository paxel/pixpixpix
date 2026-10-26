//! Seam 1: the app core's one-shot dir column actions, `.` for dot entries and
//! `r` for the search below the column's folder.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pixpixpix::core::{Command, Core, Event, Focus, Key, KeyPress, Screen};
use pixpixpix::decode::DecodedImage;
use pixpixpix::folder::Found;

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

fn searched(
    core: &mut Core,
    dir: &str,
    dots: bool,
    found: Option<(&str, &[&str])>,
) -> Vec<Command> {
    core.handle(Event::Searched {
        dir: PathBuf::from(dir),
        dots,
        found: found.map(|(dir, files)| Found {
            dir: PathBuf::from(dir),
            files: paths(files),
        }),
    })
}

fn search(dir: &str, dots: bool) -> Command {
    Command::Search {
        dir: PathBuf::from(dir),
        dots,
    }
}

fn decode(path: &str) -> Command {
    Command::Decode(PathBuf::from(path))
}

fn column_dirs(core: &Core) -> Vec<PathBuf> {
    core.view()
        .dir_column
        .as_ref()
        .map(|c| c.dirs.clone())
        .unwrap_or_default()
}

fn shows_image(core: &Core, path: &str) -> bool {
    matches!(&core.view().screen, Screen::Image { path: p, .. } if p == Path::new(path))
}

fn band(core: &Core) -> Vec<(&Path, bool)> {
    core.view()
        .band
        .iter()
        .map(|t| (t.path.as_path(), t.current))
        .collect()
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

/// [`browsing`] with the column open on `/p` listing `.git`, `x` and `y`, `x`
/// highlighted.
fn column_open() -> Core {
    let mut core = browsing();
    press(&mut core, Key::N);
    dirs_listed(&mut core, "/p", &["/p/.git", "/p/x", "/p/y"]);
    core
}

/// [`column_open`] with `r` pressed and `/p/y/z` found with four files.
fn found() -> Core {
    let mut core = column_open();
    press(&mut core, Key::R);
    searched(
        &mut core,
        "/p",
        false,
        Some(("/p/y/z", &["/p/y/z/1.jpg", "/p/y/z/2.jpg", "/p/y/z/3.jpg"])),
    );
    core
}

#[test]
fn dot_entries_are_hidden_by_default() {
    let mut core = column_open();
    assert_eq!(column_dirs(&core), paths(&["/p/x", "/p/y"]));
    // The preview skips the highlighted folder's dot files too.
    assert_eq!(
        files_listed(&mut core, "/p/x", &["/p/x/.a.jpg", "/p/x/1.jpg"]),
        [decode("/p/x/1.jpg")]
    );
    press(&mut core, Key::Tab);
    press(&mut core, Key::Right);
    assert!(shows_image(&core, "/p/c.jpg"));
}

#[test]
fn a_folder_with_only_dot_folders_counts_none() {
    let mut core = column_open();
    files_listed(&mut core, "/p/x", &[]);
    dirs_listed(&mut core, "/p/x", &["/p/x/.git"]);
    assert_eq!(
        core.view().screen,
        Screen::NoImages {
            dir: PathBuf::from("/p/x"),
            folders: Some(0)
        }
    );
}

#[test]
fn dot_shows_dot_folders_in_the_column_and_dot_files_in_navigation() {
    let mut core = column_open();
    press(&mut core, Key::Period);
    assert_eq!(column_dirs(&core), paths(&["/p/.git", "/p/x", "/p/y"]));
    // The highlight stays on the folder highlighted so far.
    let column = core.view().dir_column.clone().unwrap();
    assert_eq!(column.highlight, Some(1));
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
    assert_eq!(column_dirs(&core), paths(&["/p/x", "/p/y"]));
    press(&mut core, Key::Tab);
    assert!(shows_image(&core, "/p/a.jpg"));
}

#[test]
fn any_navigation_resets_dots_and_aborts_the_search() {
    for key in [
        Key::Up,
        Key::Down,
        Key::Left,
        Key::Right,
        Key::Enter,
        Key::Escape,
        Key::Backspace,
        Key::N,
    ] {
        let mut core = column_open();
        press(&mut core, Key::Period);
        press(&mut core, Key::R);
        let commands = press(&mut core, key);
        assert_eq!(commands.first(), Some(&Command::CancelSearch), "{key:?}");
        if key == Key::N {
            press(&mut core, Key::N);
        }
        let dir = core.view().dir_column.as_ref().unwrap().dir.clone();
        let dir = dir.to_str().unwrap();
        assert_eq!(press(&mut core, Key::R), [search(dir, false)], "{key:?}");
    }
}

#[test]
fn moving_the_highlight_hides_dot_folders_again() {
    let mut core = column_open();
    press(&mut core, Key::Period);
    press(&mut core, Key::Down);
    assert_eq!(column_dirs(&core), paths(&["/p/x", "/p/y"]));
}

#[test]
fn r_searches_below_the_column_folder_once() {
    let mut core = column_open();
    assert_eq!(press(&mut core, Key::R), [search("/p", false)]);
    assert_eq!(
        core.view().screen,
        Screen::Searching {
            dir: PathBuf::from("/p")
        }
    );
    assert!(core.view().band.is_empty());
    assert!(press(&mut core, Key::R).is_empty());
}

#[test]
fn r_after_dot_searches_dot_folders_too() {
    let mut core = column_open();
    press(&mut core, Key::Period);
    assert_eq!(press(&mut core, Key::R), [search("/p", true)]);
}

#[test]
fn nothing_found_says_so() {
    let mut core = column_open();
    press(&mut core, Key::R);
    assert!(searched(&mut core, "/p", false, None).is_empty());
    assert_eq!(
        core.view().screen,
        Screen::NothingBelow {
            dir: PathBuf::from("/p")
        }
    );
}

#[test]
fn a_stale_result_is_ignored() {
    let mut core = column_open();
    assert!(searched(&mut core, "/p", false, Some(("/p/y", &["/p/y/1.jpg"]))).is_empty());
    press(&mut core, Key::Period);
    press(&mut core, Key::R);
    assert!(searched(&mut core, "/p", false, Some(("/p/y", &["/p/y/1.jpg"]))).is_empty());
    assert_eq!(
        core.view().screen,
        Screen::Searching {
            dir: PathBuf::from("/p")
        }
    );
}

#[test]
fn the_found_folder_decodes_its_first_file_first_then_the_rest() {
    let mut core = column_open();
    press(&mut core, Key::R);
    let files: Vec<String> = (1..=8).map(|i| format!("/p/y/z/{i}.jpg")).collect();
    let files: Vec<&str> = files.iter().map(String::as_str).collect();
    assert_eq!(
        searched(&mut core, "/p", false, Some(("/p/y/z", &files))),
        [decode("/p/y/z/1.jpg")]
    );
    assert_eq!(
        core.view().screen,
        Screen::Loading {
            path: PathBuf::from("/p/y/z/1.jpg")
        }
    );
    // The cache limit: the first file and five after it.
    assert_eq!(
        decoded(&mut core, "/p/y/z/1.jpg"),
        (2..=6)
            .map(|i| decode(&format!("/p/y/z/{i}.jpg")))
            .collect::<Vec<_>>()
    );
    assert!(shows_image(&core, "/p/y/z/1.jpg"));
    decoded(&mut core, "/p/y/z/3.jpg");
    assert_eq!(
        band(&core),
        [
            (Path::new("/p/y/z/1.jpg"), true),
            (Path::new("/p/y/z/3.jpg"), false)
        ]
    );
}

#[test]
fn the_search_shows_only_while_the_column_has_the_focus() {
    let mut core = found();
    decoded(&mut core, "/p/y/z/1.jpg");
    press(&mut core, Key::Tab);
    assert!(shows_image(&core, "/p/a.jpg"));
    // The band reached from the image view shows the current folder too.
    press(&mut core, Key::Tab);
    assert!(shows_image(&core, "/p/a.jpg"));
    press(&mut core, Key::Tab);
    assert!(shows_image(&core, "/p/y/z/1.jpg"));
}

#[test]
fn picking_a_found_image_moves_to_its_folder_and_shows_it() {
    let mut core = found();
    decoded(&mut core, "/p/y/z/1.jpg");
    decoded(&mut core, "/p/y/z/2.jpg");
    let commands = core.handle(Event::Show(PathBuf::from("/p/y/z/2.jpg")));
    assert!(commands.contains(&Command::CancelDecode(PathBuf::from("/p/y/z/3.jpg"))));
    assert!(commands.contains(&Command::ListFolder(PathBuf::from("/p/y/z"))));
    assert!(!commands.contains(&decode("/p/y/z/2.jpg")));
    assert_eq!(
        core.view().dir_column.as_ref().unwrap().dir,
        Path::new("/p/y/z")
    );
    assert_eq!(core.view().focus, Focus::Image);
    assert!(shows_image(&core, "/p/y/z/2.jpg"));
    files_listed(&mut core, "/p/y/z", &["/p/y/z/1.jpg", "/p/y/z/2.jpg"]);
    press(&mut core, Key::Left);
    assert!(matches!(
        &core.view().screen,
        Screen::Loading { path } if path == Path::new("/p/y/z/1.jpg")
    ));
}

#[test]
fn a_found_highlighted_folder_waits_for_the_nav_decode() {
    let mut core = column_open();
    assert_eq!(
        files_listed(&mut core, "/p/x", &["/p/x/1.jpg", "/p/x/2.jpg"]),
        [decode("/p/x/1.jpg")]
    );
    press(&mut core, Key::R);
    assert!(
        searched(
            &mut core,
            "/p",
            false,
            Some(("/p/x", &["/p/x/1.jpg", "/p/x/2.jpg"]))
        )
        .is_empty()
    );
    assert_eq!(decoded(&mut core, "/p/x/1.jpg"), [decode("/p/x/2.jpg")]);
    assert_eq!(band(&core), [(Path::new("/p/x/1.jpg"), true)]);
    // Aborting cancels only what the search asked for.
    let commands = press(&mut core, Key::Down);
    assert!(commands.contains(&Command::CancelDecode(PathBuf::from("/p/x/2.jpg"))));
}

#[test]
fn aborting_leaves_a_nav_decode_running() {
    let mut core = column_open();
    files_listed(&mut core, "/p/x", &["/p/x/1.jpg"]);
    press(&mut core, Key::R);
    searched(&mut core, "/p", false, Some(("/p/x", &["/p/x/1.jpg"])));
    // Up moves nowhere, so the nav cache keeps waiting for x.
    assert!(press(&mut core, Key::Up).is_empty());
}

#[test]
fn a_found_image_already_in_the_nav_cache_shows_at_once() {
    let mut core = column_open();
    files_listed(&mut core, "/p/x", &["/p/x/1.jpg"]);
    decoded(&mut core, "/p/x/1.jpg");
    press(&mut core, Key::R);
    assert!(searched(&mut core, "/p", false, Some(("/p/x", &["/p/x/1.jpg"]))).is_empty());
    assert!(shows_image(&core, "/p/x/1.jpg"));
}

#[test]
fn dot_and_r_do_nothing_in_the_image_view() {
    let mut core = column_open();
    press(&mut core, Key::Tab);
    assert!(press(&mut core, Key::R).is_empty());
    assert!(press(&mut core, Key::Period).is_empty());
    assert!(shows_image(&core, "/p/a.jpg"));
    press(&mut core, Key::Tab);
    assert_eq!(column_dirs(&core), paths(&["/p/x", "/p/y"]));
}
