//! Seam 1: the app core's dir column and moving between folders.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pixpixpix::core::{Command, Core, DirColumn, Event, Focus, Key, KeyPress, Screen};
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

/// Showing `/p/a.jpg` of `/p/a.jpg`, `/p/b.jpg`, `/p/c.jpg`, with `b.jpg`
/// still decoding.
fn browsing() -> Core {
    let (mut core, _) = Core::start(Ok(PathBuf::from("/p/a.jpg")));
    decoded(&mut core, "/p/a.jpg");
    core.handle(Event::FirstFrameShown);
    files_listed(&mut core, "/p", &["/p/a.jpg", "/p/b.jpg", "/p/c.jpg"]);
    decoded(&mut core, "/p/c.jpg");
    core
}

/// [`browsing`] with the column open on `/p` listing `x` and `y`.
fn column_open() -> Core {
    let mut core = browsing();
    press(&mut core, Key::N);
    dirs_listed(&mut core, "/p", &["/p/x", "/p/y"]);
    core
}

fn column(core: &Core) -> Option<&DirColumn> {
    core.view().dir_column.as_ref()
}

fn highlighted(core: &Core) -> Option<&Path> {
    let column = column(core)?;
    column
        .highlight
        .and_then(|i| column.dirs.get(i))
        .map(PathBuf::as_path)
}

fn key_names(core: &Core) -> Vec<&'static str> {
    core.view().key_line.keys.iter().map(|k| k.keys).collect()
}

#[test]
fn n_opens_the_column_on_the_current_folder_focused() {
    let mut core = browsing();
    assert_eq!(
        press(&mut core, Key::N),
        [
            Command::ListDirs(PathBuf::from("/p")),
            // The parent's first image, for the nav cache.
            Command::ListFolder(PathBuf::from("/")),
        ]
    );
    assert_eq!(core.view().focus, Focus::Dirs);
    assert_eq!(column(&core).unwrap().dir, Path::new("/p"));
    dirs_listed(&mut core, "/p", &["/p/x", "/p/y"]);
    assert_eq!(column(&core).unwrap().dirs, paths(&["/p/x", "/p/y"]));
    assert_eq!(highlighted(&core), Some(Path::new("/p/x")));
}

#[test]
fn n_closes_the_column_and_returns_focus_to_the_image_view() {
    let mut core = column_open();
    assert!(press(&mut core, Key::N).is_empty());
    assert_eq!(core.view().dir_column, None);
    assert_eq!(core.view().focus, Focus::Image);
}

#[test]
fn n_before_the_first_frame_does_nothing() {
    let (mut core, _) = Core::start(Ok(PathBuf::from("/p/a.jpg")));
    assert!(press(&mut core, Key::N).is_empty());
    assert_eq!(core.view().dir_column, None);
}

#[test]
fn an_unreadable_folder_shows_the_error_in_the_column() {
    let mut core = browsing();
    press(&mut core, Key::N);
    core.handle(Event::DirsListed {
        dir: PathBuf::from("/p"),
        result: Err("permission denied".into()),
    });
    assert_eq!(
        column(&core).unwrap().error.as_deref(),
        Some("permission denied")
    );
    assert_eq!(highlighted(&core), None);
}

#[test]
fn up_and_down_move_the_highlight_and_stop_at_the_ends() {
    let mut core = column_open();
    press(&mut core, Key::Up);
    assert_eq!(highlighted(&core), Some(Path::new("/p/x")));
    press(&mut core, Key::Down);
    assert_eq!(highlighted(&core), Some(Path::new("/p/y")));
    press(&mut core, Key::Down);
    assert_eq!(highlighted(&core), Some(Path::new("/p/y")));
    press(&mut core, Key::Up);
    assert_eq!(highlighted(&core), Some(Path::new("/p/x")));
}

#[test]
fn keys_go_to_the_focused_area() {
    let mut core = column_open();
    // Right enters the folder instead of stepping to b.jpg.
    press(&mut core, Key::Right);
    assert_eq!(
        core.view().screen,
        Screen::Listing {
            dir: PathBuf::from("/p/x")
        }
    );
    // Zoom keys belong to the image view.
    press(&mut core, Key::Num1);
    assert_eq!(core.view().zoom.scale, pixpixpix::core::Scale::Fit);
}

#[test]
fn entering_a_folder_cancels_the_old_work_and_lists_the_new_one() {
    let mut core = column_open();
    assert_eq!(
        press(&mut core, Key::Down),
        [Command::ListFolder(PathBuf::from("/p/y"))]
    );
    assert_eq!(
        press(&mut core, Key::Enter),
        [
            Command::CancelDecode(PathBuf::from("/p/b.jpg")),
            Command::WatchFolder(PathBuf::from("/p/y")),
            Command::ListFolder(PathBuf::from("/p/y")),
            Command::ListDirs(PathBuf::from("/p/y")),
        ]
    );
    assert!(core.view().band.is_empty());
    assert_eq!(column(&core).unwrap().dir, Path::new("/p/y"));
    assert_eq!(core.view().focus, Focus::Dirs);
}

#[test]
fn entering_a_folder_shows_its_first_image_before_any_neighbour() {
    let mut core = column_open();
    press(&mut core, Key::Right);
    assert_eq!(
        files_listed(&mut core, "/p/x", &["/p/x/1.jpg", "/p/x/2.jpg"]),
        [Command::Decode(PathBuf::from("/p/x/1.jpg"))]
    );
    assert_eq!(
        core.view().screen,
        Screen::Loading {
            path: PathBuf::from("/p/x/1.jpg")
        }
    );
    assert_eq!(
        decoded(&mut core, "/p/x/1.jpg"),
        [
            Command::Decode(PathBuf::from("/p/x/2.jpg")),
            Command::FindMotion(PathBuf::from("/p/x/1.jpg"))
        ]
    );
    assert!(matches!(
        &core.view().screen,
        Screen::Image { path, .. } if path == Path::new("/p/x/1.jpg")
    ));
}

#[test]
fn late_results_of_the_old_folder_change_nothing() {
    let mut core = column_open();
    press(&mut core, Key::Right);
    assert!(decoded(&mut core, "/p/b.jpg").is_empty());
    assert!(files_listed(&mut core, "/p", &["/p/a.jpg"]).is_empty());
    dirs_listed(&mut core, "/p", &["/p/x", "/p/y"]);
    assert!(core.view().band.is_empty());
    assert_eq!(column(&core).unwrap().dir, Path::new("/p/x"));
    assert!(column(&core).unwrap().dirs.is_empty());
    assert_eq!(
        core.view().screen,
        Screen::Listing {
            dir: PathBuf::from("/p/x")
        }
    );
}

#[test]
fn entering_highlights_the_first_subfolder() {
    let mut core = column_open();
    press(&mut core, Key::Right);
    dirs_listed(&mut core, "/p/x", &["/p/x/m", "/p/x/n"]);
    assert_eq!(highlighted(&core), Some(Path::new("/p/x/m")));
}

#[test]
fn right_without_subfolders_does_nothing() {
    let mut core = browsing();
    press(&mut core, Key::N);
    dirs_listed(&mut core, "/p", &[]);
    assert!(press(&mut core, Key::Right).is_empty());
    assert!(press(&mut core, Key::Enter).is_empty());
    assert_eq!(column(&core).unwrap().dir, Path::new("/p"));
}

#[test]
fn a_folder_without_media_says_so() {
    let mut core = column_open();
    press(&mut core, Key::Right);
    assert!(files_listed(&mut core, "/p/x", &[]).is_empty());
    assert_eq!(
        core.view().screen,
        Screen::Error {
            path: None,
            message: "No images in /p/x".into()
        }
    );
}

#[test]
fn left_escape_and_backspace_go_to_the_parent_highlighting_the_folder_left() {
    for key in [Key::Left, Key::Escape, Key::Backspace] {
        let mut core = column_open();
        press(&mut core, Key::Down);
        press(&mut core, Key::Right);
        assert_eq!(column(&core).unwrap().dir, Path::new("/p/y"));
        assert_eq!(
            press(&mut core, key),
            [
                Command::WatchFolder(PathBuf::from("/p")),
                Command::ListFolder(PathBuf::from("/p")),
                Command::ListDirs(PathBuf::from("/p")),
                // The nav cache: the new parent and the folder left.
                Command::ListFolder(PathBuf::from("/")),
                Command::ListFolder(PathBuf::from("/p/y")),
                Command::FindMotion(PathBuf::from("/p/a.jpg")),
            ],
            "{key:?}"
        );
        dirs_listed(&mut core, "/p", &["/p/x", "/p/y"]);
        assert_eq!(highlighted(&core), Some(Path::new("/p/y")), "{key:?}");
    }
}

#[test]
fn the_root_has_no_parent() {
    let (mut core, _) = Core::start(Ok(PathBuf::from("/a.jpg")));
    decoded(&mut core, "/a.jpg");
    core.handle(Event::FirstFrameShown);
    press(&mut core, Key::N);
    assert!(press(&mut core, Key::Left).is_empty());
    assert_eq!(column(&core).unwrap().dir, Path::new("/"));
}

#[test]
fn moving_between_folders_keeps_fullscreen() {
    let mut core = column_open();
    press(&mut core, Key::F);
    press(&mut core, Key::Right);
    press(&mut core, Key::Left);
    assert!(core.view().fullscreen);
}

#[test]
fn tab_reaches_the_column_only_while_it_is_open() {
    let mut core = browsing();
    press(&mut core, Key::Tab);
    assert_eq!(core.view().focus, Focus::Band);
    press(&mut core, Key::Tab);
    assert_eq!(core.view().focus, Focus::Image);
    press(&mut core, Key::N);
    press(&mut core, Key::Tab);
    assert_eq!(core.view().focus, Focus::Image);
    // The image view has the arrows again.
    press(&mut core, Key::Right);
    assert!(matches!(
        &core.view().screen,
        Screen::Loading { path } if path == Path::new("/p/b.jpg")
    ));
    press(&mut core, Key::Tab);
    press(&mut core, Key::Tab);
    assert_eq!(core.view().focus, Focus::Dirs);
}

#[test]
fn the_key_line_lists_the_keys_of_the_focused_area() {
    let quit = if cfg!(target_os = "macos") {
        "Cmd+Q"
    } else {
        "Ctrl+Q"
    };
    let mut core = browsing();
    press(&mut core, Key::N);
    assert_eq!(
        key_names(&core),
        [
            "Up/Down",
            "Left/Esc/Backspace",
            "Right/Enter",
            "./Shift+.",
            "G",
            "N",
            "B",
            "Tab",
            "S",
            "F",
            quit
        ]
    );
    press(&mut core, Key::Tab);
    assert_eq!(
        key_names(&core),
        [
            "Left/Right",
            "+/-",
            "0",
            "1",
            "L/R",
            "Enter",
            "C",
            "O",
            "I",
            "X",
            "P",
            "G",
            "N",
            "B",
            "Tab",
            "S",
            "F",
            quit
        ]
    );
}
