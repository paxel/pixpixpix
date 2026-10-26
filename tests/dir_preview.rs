//! Seam 1: the app core's preview of the highlighted folder and its nav cache.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pixpixpix::core::{Command, Core, Event, Key, KeyPress, Screen, Thumb};
use pixpixpix::decode::DecodedImage;
use pixpixpix::startup::NoFile;

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

fn shows_image(core: &Core, path: &str) -> bool {
    matches!(&core.view().screen, Screen::Image { path: p, .. } if p == Path::new(path))
}

fn band(core: &Core) -> Vec<(&Path, bool)> {
    core.view()
        .band
        .iter()
        .map(|t: &Thumb| (t.path.as_path(), t.current))
        .collect()
}

/// Showing `/r/p/a.jpg` of `/r/p/a.jpg`, `/r/p/b.jpg`, both decoded.
fn browsing() -> Core {
    let (mut core, _) = Core::start(Ok(PathBuf::from("/r/p/a.jpg")));
    decoded(&mut core, "/r/p/a.jpg");
    core.handle(Event::FirstFrameShown);
    files_listed(&mut core, "/r/p", &["/r/p/a.jpg", "/r/p/b.jpg"]);
    decoded(&mut core, "/r/p/b.jpg");
    core
}

/// [`browsing`] with the column open on `/r/p` listing `x` and `y`, `x`
/// highlighted.
fn column_open() -> Core {
    let mut core = browsing();
    press(&mut core, Key::N);
    dirs_listed(&mut core, "/r/p", &["/r/p/x", "/r/p/y"]);
    core
}

/// [`column_open`] with the first image of `x` decoded.
fn previewing_x() -> Core {
    let mut core = column_open();
    files_listed(&mut core, "/r/p/x", &["/r/p/x/1.jpg", "/r/p/x/2.jpg"]);
    decoded(&mut core, "/r/p/x/1.jpg");
    core
}

#[test]
fn opening_the_column_lists_the_parent_and_the_highlighted_folder() {
    let mut core = browsing();
    assert_eq!(
        press(&mut core, Key::N),
        [
            Command::ListDirs(PathBuf::from("/r/p")),
            Command::ListFolder(PathBuf::from("/r")),
        ]
    );
    assert_eq!(
        dirs_listed(&mut core, "/r/p", &["/r/p/x", "/r/p/y"]),
        [Command::ListFolder(PathBuf::from("/r/p/x"))]
    );
}

#[test]
fn the_highlighted_folder_is_previewed_with_the_band_following() {
    let mut core = column_open();
    assert_eq!(
        core.view().screen,
        Screen::Listing {
            dir: PathBuf::from("/r/p/x")
        }
    );
    assert_eq!(
        files_listed(&mut core, "/r/p/x", &["/r/p/x/1.jpg", "/r/p/x/2.jpg"]),
        [Command::Decode(PathBuf::from("/r/p/x/1.jpg"))]
    );
    assert_eq!(
        core.view().screen,
        Screen::Loading {
            path: PathBuf::from("/r/p/x/1.jpg")
        }
    );
    assert!(decoded(&mut core, "/r/p/x/1.jpg").is_empty());
    assert!(shows_image(&core, "/r/p/x/1.jpg"));
    assert_eq!(band(&core), [(Path::new("/r/p/x/1.jpg"), true)]);
    // A preview is not entering: the column stays on the current folder.
    assert_eq!(
        core.view().dir_column.as_ref().unwrap().dir,
        Path::new("/r/p")
    );
}

#[test]
fn moving_the_highlight_cancels_the_stale_preview_load() {
    let mut core = column_open();
    files_listed(&mut core, "/r/p/x", &["/r/p/x/1.jpg"]);
    assert_eq!(
        press(&mut core, Key::Down),
        [
            Command::CancelDecode(PathBuf::from("/r/p/x/1.jpg")),
            Command::ListFolder(PathBuf::from("/r/p/y")),
        ]
    );
    assert!(decoded(&mut core, "/r/p/x/1.jpg").is_empty());
    assert_eq!(
        core.view().screen,
        Screen::Listing {
            dir: PathBuf::from("/r/p/y")
        }
    );
}

#[test]
fn a_late_listing_of_the_folder_highlighted_before_changes_nothing() {
    let mut core = column_open();
    press(&mut core, Key::Down);
    assert!(files_listed(&mut core, "/r/p/x", &["/r/p/x/1.jpg"]).is_empty());
    assert_eq!(
        core.view().screen,
        Screen::Listing {
            dir: PathBuf::from("/r/p/y")
        }
    );
}

#[test]
fn moving_back_previews_the_folder_again() {
    let mut core = previewing_x();
    press(&mut core, Key::Down);
    assert_eq!(
        press(&mut core, Key::Up),
        [Command::ListFolder(PathBuf::from("/r/p/x"))]
    );
}

#[test]
fn a_folder_without_images_shows_no_images_and_its_subfolder_count() {
    let mut core = column_open();
    assert_eq!(
        files_listed(&mut core, "/r/p/x", &[]),
        [Command::ListDirs(PathBuf::from("/r/p/x"))]
    );
    assert_eq!(
        core.view().screen,
        Screen::NoImages {
            dir: PathBuf::from("/r/p/x"),
            folders: None
        }
    );
    assert!(core.view().band.is_empty());
    dirs_listed(&mut core, "/r/p/x", &["/r/p/x/1", "/r/p/x/2", "/r/p/x/3"]);
    assert_eq!(
        core.view().screen,
        Screen::NoImages {
            dir: PathBuf::from("/r/p/x"),
            folders: Some(3)
        }
    );
    // The count does not touch the column.
    assert_eq!(
        core.view().dir_column.as_ref().unwrap().dirs,
        paths(&["/r/p/x", "/r/p/y"])
    );
}

#[test]
fn an_unreadable_highlighted_folder_shows_the_error() {
    let mut core = column_open();
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/r/p/x"),
        result: Err("permission denied".into()),
    });
    assert_eq!(
        core.view().screen,
        Screen::Error {
            path: None,
            message: "Cannot read /r/p/x: permission denied".into()
        }
    );
}

#[test]
fn the_image_view_shows_the_current_image_and_tab_back_previews_again() {
    let mut core = previewing_x();
    press(&mut core, Key::Tab);
    assert!(shows_image(&core, "/r/p/a.jpg"));
    assert_eq!(
        band(&core),
        [
            (Path::new("/r/p/a.jpg"), true),
            (Path::new("/r/p/b.jpg"), false)
        ]
    );
    // The image view still steps through the current folder.
    press(&mut core, Key::Right);
    assert!(shows_image(&core, "/r/p/b.jpg"));
    // Back through the band, which shows the current folder too.
    press(&mut core, Key::Tab);
    assert!(shows_image(&core, "/r/p/b.jpg"));
    press(&mut core, Key::Tab);
    assert!(shows_image(&core, "/r/p/x/1.jpg"));
}

#[test]
fn closing_the_column_cancels_the_nav_cache_and_shows_the_current_image() {
    let mut core = column_open();
    files_listed(&mut core, "/r/p/x", &["/r/p/x/1.jpg"]);
    assert_eq!(
        press(&mut core, Key::N),
        [Command::CancelDecode(PathBuf::from("/r/p/x/1.jpg"))]
    );
    assert!(shows_image(&core, "/r/p/a.jpg"));
}

#[test]
fn entering_the_previewed_folder_shows_its_first_image_at_once() {
    let mut core = previewing_x();
    let commands = press(&mut core, Key::Right);
    assert!(!commands.contains(&Command::Decode(PathBuf::from("/r/p/x/1.jpg"))));
    assert!(shows_image(&core, "/r/p/x/1.jpg"));
    assert_eq!(
        core.view().dir_column.as_ref().unwrap().dir,
        Path::new("/r/p/x")
    );
    assert_eq!(
        files_listed(&mut core, "/r/p/x", &["/r/p/x/1.jpg", "/r/p/x/2.jpg"]),
        [Command::Decode(PathBuf::from("/r/p/x/2.jpg"))]
    );
}

#[test]
fn entering_while_the_preview_decodes_keeps_that_decode() {
    let mut core = column_open();
    files_listed(&mut core, "/r/p/x", &["/r/p/x/1.jpg"]);
    let commands = press(&mut core, Key::Enter);
    assert!(!commands.contains(&Command::CancelDecode(PathBuf::from("/r/p/x/1.jpg"))));
    assert_eq!(
        core.view().screen,
        Screen::Loading {
            path: PathBuf::from("/r/p/x/1.jpg")
        }
    );
    decoded(&mut core, "/r/p/x/1.jpg");
    assert!(shows_image(&core, "/r/p/x/1.jpg"));
}

#[test]
fn going_up_shows_the_parents_first_image_at_once() {
    let mut core = column_open();
    assert_eq!(
        files_listed(&mut core, "/r", &["/r/top.jpg"]),
        [Command::Decode(PathBuf::from("/r/top.jpg"))]
    );
    decoded(&mut core, "/r/top.jpg");
    press(&mut core, Key::Left);
    assert!(shows_image(&core, "/r/top.jpg"));
    // The folder left is highlighted and previewed from the nav cache.
    let commands = dirs_listed(&mut core, "/r", &["/r/p", "/r/q"]);
    assert!(!commands.contains(&Command::ListFolder(PathBuf::from("/r/p"))));
    assert!(shows_image(&core, "/r/p/a.jpg"));
}

#[test]
fn entering_keeps_the_first_image_of_the_folder_left_as_the_parent() {
    let mut core = previewing_x();
    let commands = press(&mut core, Key::Right);
    assert!(!commands.contains(&Command::ListFolder(PathBuf::from("/r/p"))));
    // The old folder's watcher may still report; it changes nothing.
    assert!(files_listed(&mut core, "/r/p", &["/r/p/a.jpg"]).is_empty());
    press(&mut core, Key::Left);
    assert!(shows_image(&core, "/r/p/a.jpg"));
}

#[test]
fn a_repeated_listing_keeps_the_highlight() {
    let mut core = column_open();
    press(&mut core, Key::Down);
    dirs_listed(&mut core, "/r/p", &["/r/p/x", "/r/p/y"]);
    let column = core.view().dir_column.as_ref().unwrap();
    assert_eq!(column.highlight, Some(1));
}

#[test]
fn a_folder_with_nothing_to_open_gets_a_dir_column() {
    let (mut core, _) = Core::start(Err(NoFile {
        dir: PathBuf::from("/e"),
        message: "No images in /e".into(),
    }));
    assert!(press(&mut core, Key::N).is_empty());
    core.handle(Event::FirstFrameShown);
    assert_eq!(
        press(&mut core, Key::N),
        [
            Command::ListDirs(PathBuf::from("/e")),
            Command::ListFolder(PathBuf::from("/")),
        ]
    );
    dirs_listed(&mut core, "/e", &["/e/sub"]);
    press(&mut core, Key::Right);
    assert_eq!(
        core.view().dir_column.as_ref().unwrap().dir,
        Path::new("/e/sub")
    );
}
