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

/// [`browsing`] with the column open: `/p` among `/o` and `/q`, and its
/// subfolders `x` and `y` listed.
fn column_open() -> Core {
    let mut core = browsing();
    press(&mut core, Key::N);
    dirs_listed(&mut core, "/", &["/o", "/p", "/q"]);
    dirs_listed(&mut core, "/p", &["/p/x", "/p/y"]);
    core
}

fn shows_image(core: &Core, path: &str) -> bool {
    matches!(&core.view().screen, Screen::Image { path: p, .. } if p == Path::new(path))
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
fn n_opens_the_column_on_the_level_of_the_current_folder_focused() {
    let mut core = browsing();
    assert_eq!(
        press(&mut core, Key::N),
        [
            Command::ListDirs(PathBuf::from("/")),
            // The subfolders, for Right.
            Command::ListDirs(PathBuf::from("/p")),
        ]
    );
    assert_eq!(core.view().focus, Focus::Dirs);
    assert_eq!(column(&core).unwrap().dir, Path::new("/"));
    dirs_listed(&mut core, "/", &["/o", "/p", "/q"]);
    assert_eq!(column(&core).unwrap().dirs, paths(&["/o", "/p", "/q"]));
    assert_eq!(highlighted(&core), Some(Path::new("/p")));
}

#[test]
fn opening_the_column_or_tabbing_into_it_changes_nothing_on_screen() {
    let mut core = column_open();
    assert!(shows_image(&core, "/p/a.jpg"));
    let band = core.view().band.clone();
    press(&mut core, Key::Tab);
    press(&mut core, Key::Tab);
    press(&mut core, Key::Tab);
    assert_eq!(core.view().focus, Focus::Dirs);
    assert!(shows_image(&core, "/p/a.jpg"));
    assert_eq!(core.view().band, band);
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
fn an_unreadable_level_shows_the_error_in_the_column() {
    let mut core = browsing();
    press(&mut core, Key::N);
    core.handle(Event::DirsListed {
        dir: PathBuf::from("/"),
        result: Err("permission denied".into()),
    });
    assert_eq!(
        column(&core).unwrap().error.as_deref(),
        Some("permission denied")
    );
    assert_eq!(highlighted(&core), None);
}

#[test]
fn up_and_down_go_to_the_folder_above_or_below_and_stop_at_the_ends() {
    let mut core = column_open();
    press(&mut core, Key::Up);
    assert_eq!(highlighted(&core), Some(Path::new("/o")));
    assert_eq!(
        core.view().screen,
        Screen::Listing {
            dir: PathBuf::from("/o")
        }
    );
    assert!(press(&mut core, Key::Up).is_empty());
    press(&mut core, Key::Down);
    press(&mut core, Key::Down);
    assert_eq!(highlighted(&core), Some(Path::new("/q")));
    assert!(press(&mut core, Key::Down).is_empty());
    assert_eq!(column(&core).unwrap().dir, Path::new("/"));
}

#[test]
fn keys_go_to_the_focused_area() {
    let mut core = column_open();
    // Right enters the first subfolder instead of stepping to b.jpg.
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
fn going_to_a_folder_cancels_the_old_work_and_lists_the_new_one() {
    let mut core = column_open();
    assert_eq!(
        press(&mut core, Key::Down),
        [
            Command::CancelDecode(PathBuf::from("/p/b.jpg")),
            Command::WatchFolder(PathBuf::from("/q")),
            Command::ListFolder(PathBuf::from("/q")),
            Command::ListDirs(PathBuf::from("/q")),
        ]
    );
    assert!(core.view().band.is_empty());
    assert_eq!(column(&core).unwrap().dir, Path::new("/"));
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
    assert!(shows_image(&core, "/p/x/1.jpg"));
}

#[test]
fn right_and_enter_go_into_the_first_subfolder_listing_it_among_its_siblings() {
    for key in [Key::Right, Key::Enter] {
        let mut core = column_open();
        assert_eq!(
            press(&mut core, key),
            [
                Command::CancelDecode(PathBuf::from("/p/b.jpg")),
                Command::WatchFolder(PathBuf::from("/p/x")),
                Command::ListFolder(PathBuf::from("/p/x")),
                Command::ListDirs(PathBuf::from("/p/x")),
            ],
            "{key:?}"
        );
        assert_eq!(column(&core).unwrap().dir, Path::new("/p"), "{key:?}");
        assert_eq!(column(&core).unwrap().dirs, paths(&["/p/x", "/p/y"]));
        assert_eq!(highlighted(&core), Some(Path::new("/p/x")), "{key:?}");
    }
}

#[test]
fn late_results_of_the_old_folder_change_nothing() {
    let mut core = column_open();
    press(&mut core, Key::Right);
    assert!(decoded(&mut core, "/p/b.jpg").is_empty());
    assert!(files_listed(&mut core, "/p", &["/p/a.jpg"]).is_empty());
    assert!(core.view().band.is_empty());
    assert_eq!(highlighted(&core), Some(Path::new("/p/x")));
    assert_eq!(
        core.view().screen,
        Screen::Listing {
            dir: PathBuf::from("/p/x")
        }
    );
}

#[test]
fn right_without_subfolders_does_nothing() {
    let mut core = browsing();
    press(&mut core, Key::N);
    // Not listed yet.
    assert!(press(&mut core, Key::Right).is_empty());
    dirs_listed(&mut core, "/p", &[]);
    assert!(press(&mut core, Key::Right).is_empty());
    assert!(press(&mut core, Key::Enter).is_empty());
    assert!(shows_image(&core, "/p/a.jpg"));
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
fn left_escape_and_backspace_go_to_the_parent_highlighting_it() {
    for key in [Key::Left, Key::Escape, Key::Backspace] {
        let mut core = column_open();
        press(&mut core, Key::Right);
        assert_eq!(
            press(&mut core, key),
            [
                Command::WatchFolder(PathBuf::from("/p")),
                Command::ListFolder(PathBuf::from("/p")),
                Command::ListDirs(PathBuf::from("/")),
            ],
            "{key:?}"
        );
        dirs_listed(&mut core, "/", &["/o", "/p", "/q"]);
        assert_eq!(highlighted(&core), Some(Path::new("/p")), "{key:?}");
        // The subfolders were known already: Right goes back in.
        press(&mut core, Key::Right);
        assert_eq!(highlighted(&core), Some(Path::new("/p/x")), "{key:?}");
    }
}

#[test]
fn the_root_stands_alone_and_has_no_parent() {
    let (mut core, _) = Core::start(Ok(PathBuf::from("/a.jpg")));
    decoded(&mut core, "/a.jpg");
    core.handle(Event::FirstFrameShown);
    assert_eq!(
        press(&mut core, Key::N),
        [Command::ListDirs(PathBuf::from("/"))]
    );
    assert_eq!(column(&core).unwrap().dirs, paths(&["/"]));
    assert_eq!(highlighted(&core), Some(Path::new("/")));
    assert!(press(&mut core, Key::Left).is_empty());
    dirs_listed(&mut core, "/", &["/p"]);
    press(&mut core, Key::Right);
    assert_eq!(highlighted(&core), Some(Path::new("/p")));
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

#[test]
fn clicking_a_folder_goes_there_and_focuses_the_column() {
    let mut core = column_open();
    press(&mut core, Key::Tab);
    assert_eq!(core.view().focus, Focus::Image);
    core.handle(Event::FolderClicked(PathBuf::from("/q")));
    assert_eq!(core.view().focus, Focus::Dirs);
    assert_eq!(highlighted(&core), Some(Path::new("/q")));
    assert_eq!(
        core.view().screen,
        Screen::Listing {
            dir: PathBuf::from("/q")
        }
    );
    // The current folder again changes nothing.
    assert!(
        core.handle(Event::FolderClicked(PathBuf::from("/q")))
            .is_empty()
    );
}

#[test]
fn double_clicking_a_folder_goes_into_its_first_subfolder() {
    let mut core = column_open();
    core.handle(Event::FolderDoubleClicked(PathBuf::from("/p")));
    assert_eq!(highlighted(&core), Some(Path::new("/p/x")));
    // A folder whose subfolders are not listed yet waits for them.
    let mut core = column_open();
    core.handle(Event::FolderClicked(PathBuf::from("/q")));
    core.handle(Event::FolderDoubleClicked(PathBuf::from("/q")));
    assert_eq!(highlighted(&core), Some(Path::new("/q")));
    dirs_listed(&mut core, "/q", &["/q/m", "/q/n"]);
    assert_eq!(highlighted(&core), Some(Path::new("/q/m")));
}

#[test]
fn the_up_row_goes_to_the_parent() {
    let mut core = column_open();
    press(&mut core, Key::Right);
    assert_eq!(
        core.handle(Event::FolderUp),
        [
            Command::WatchFolder(PathBuf::from("/p")),
            Command::ListFolder(PathBuf::from("/p")),
            Command::ListDirs(PathBuf::from("/")),
        ]
    );
}
