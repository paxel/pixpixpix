//! Seam 1: the app core's overview of `g`: its root, sections streamed from
//! the scan, and thumbnails decoded only for the tiles on screen.

use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use pixpixpix::core::{Command, Core, Event, Key, KeyPress, Overview, Screen};
use pixpixpix::decode::DecodedImage;

fn image() -> DecodedImage {
    DecodedImage {
        width: 1,
        height: 1,
        rgba: Arc::from(vec![0; 4]),
    }
}

fn p(name: &str) -> PathBuf {
    PathBuf::from(format!("/p/{name}"))
}

fn press(core: &mut Core, key: Key) -> Vec<Command> {
    core.handle(Event::Key(KeyPress::plain(key)))
}

fn decoded(core: &mut Core, path: PathBuf) -> Vec<Command> {
    core.handle(Event::Decoded {
        path,
        result: Ok(image()),
    })
}

/// Showing `/p/a.jpg` of `/p/a.jpg` and `/p/b.jpg`, both decoded.
fn browsing() -> Core {
    let (mut core, _) = Core::start(Ok(p("a.jpg")));
    decoded(&mut core, p("a.jpg"));
    core.handle(Event::FirstFrameShown);
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(vec![p("a.jpg"), p("b.jpg")]),
    });
    decoded(&mut core, p("b.jpg"));
    core
}

/// The scan the commands ask for: its number, root and dots.
fn scan_of(commands: &[Command]) -> Option<(u64, PathBuf, bool)> {
    commands.iter().find_map(|c| match c {
        Command::ScanOverview { scan, root, dots } => Some((*scan, root.clone(), *dots)),
        _ => None,
    })
}

/// [`browsing`] with the overview opened by `g`; the number of its scan.
fn opened() -> (Core, Option<u64>) {
    let mut core = browsing();
    let commands = press(&mut core, Key::G);
    (core, scan_of(&commands).map(|(scan, _, _)| scan))
}

fn section(core: &mut Core, scan: u64, dir: &str, files: &[&str]) -> Vec<Command> {
    core.handle(Event::OverviewSection {
        scan,
        dir: PathBuf::from(dir),
        files: files.iter().map(PathBuf::from).collect(),
    })
}

/// `count` files `/p/0.jpg`, `/p/1.jpg`, … as one section.
fn many(core: &mut Core, scan: u64, count: usize) -> Vec<Command> {
    let files: Vec<String> = (0..count).map(|i| format!("/p/{i}.jpg")).collect();
    let files: Vec<&str> = files.iter().map(String::as_str).collect();
    section(core, scan, "/p", &files)
}

fn visible(core: &mut Core, range: Range<usize>) -> Vec<Command> {
    core.handle(Event::OverviewVisible(range))
}

fn thumbnail_decoded(core: &mut Core, index: usize) -> Vec<Command> {
    core.handle(Event::ThumbnailDecoded {
        path: p(&format!("{index}.jpg")),
        result: Ok(image()),
    })
}

fn overview(core: &Core) -> Option<&Overview> {
    core.view().overview.as_ref()
}

/// The tile indices whose thumbnails the commands ask for.
fn decodes(commands: &[Command]) -> Vec<String> {
    commands
        .iter()
        .filter_map(|c| match c {
            Command::DecodeThumbnail(path) => Some(path.display().to_string()),
            _ => None,
        })
        .collect()
}

fn cancels(commands: &[Command]) -> Vec<String> {
    commands
        .iter()
        .filter_map(|c| match c {
            Command::CancelThumbnail(path) => Some(path.display().to_string()),
            _ => None,
        })
        .collect()
}

fn names(range: Range<usize>) -> Vec<String> {
    range.map(|i| format!("/p/{i}.jpg")).collect()
}

fn key_names(core: &Core) -> Vec<&'static str> {
    core.view().key_line.keys.iter().map(|k| k.keys).collect()
}

const QUIT: &str = if cfg!(target_os = "macos") {
    "Cmd+Q"
} else {
    "Ctrl+Q"
};

#[test]
fn g_opens_the_overview_of_the_current_folder_and_closes_it() {
    let mut core = browsing();
    let commands = press(&mut core, Key::G);
    assert_eq!(
        scan_of(&commands).map(|(_, root, dots)| (root, dots)),
        Some((PathBuf::from("/p"), false))
    );
    let open = overview(&core).unwrap();
    assert_eq!(open.root, PathBuf::from("/p"));
    assert!(open.scanning);
    assert!(open.sections.is_empty());
    assert_eq!(key_names(&core), ["G", "S", "F", QUIT]);

    let commands = press(&mut core, Key::G);
    assert_eq!(commands, [Command::CancelScan]);
    assert!(core.view().overview.is_none());
    assert!(matches!(&core.view().screen, Screen::Image { path, .. } if *path == p("a.jpg")));
    assert_eq!(key_names(&core).first(), Some(&"Left/Right"));
}

#[test]
fn the_root_is_the_folder_highlighted_in_the_dir_column() {
    let mut core = browsing();
    press(&mut core, Key::N);
    core.handle(Event::DirsListed {
        dir: PathBuf::from("/p"),
        result: Ok(vec![p("x"), p("y")]),
    });
    press(&mut core, Key::Down);
    let commands = press(&mut core, Key::G);
    assert_eq!(scan_of(&commands).map(|(_, root, _)| root), Some(p("y")));
}

#[test]
fn a_dir_column_without_subfolders_makes_its_folder_the_root() {
    let mut core = browsing();
    press(&mut core, Key::N);
    core.handle(Event::DirsListed {
        dir: PathBuf::from("/p"),
        result: Ok(Vec::new()),
    });
    let commands = press(&mut core, Key::G);
    assert_eq!(
        scan_of(&commands).map(|(_, root, _)| root),
        Some(PathBuf::from("/p"))
    );
}

#[test]
fn dot_entries_shown_with_dot_are_scanned_too() {
    let mut core = browsing();
    press(&mut core, Key::N);
    core.handle(Event::DirsListed {
        dir: PathBuf::from("/p"),
        result: Ok(vec![p(".git"), p("x")]),
    });
    press(&mut core, Key::Period);
    let commands = press(&mut core, Key::G);
    assert_eq!(scan_of(&commands).map(|(_, _, dots)| dots), Some(true));
}

#[test]
fn sections_show_in_the_order_found_with_placeholders_and_names() {
    let (mut core, scan) = opened();
    let scan = scan.unwrap();
    section(&mut core, scan, "/p", &["/p/a.jpg", "/p/b.mp4"]);
    section(&mut core, scan, "/p/x/y", &["/p/x/y/c.jpg"]);
    core.handle(Event::OverviewScanned(scan));
    let open = overview(&core).unwrap();
    let titles: Vec<&str> = open.sections.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(titles, ["p", "x/y"]);
    assert_eq!(open.sections[0].tiles, 0..2);
    assert_eq!(open.sections[1].tiles, 2..3);
    let tiles: Vec<(PathBuf, bool, bool)> = open
        .tiles
        .iter()
        .map(|t| (t.path.clone(), t.video, t.thumbnail.is_none()))
        .collect();
    assert_eq!(
        tiles,
        [
            (p("a.jpg"), false, true),
            (p("b.mp4"), true, true),
            (p("x/y/c.jpg"), false, true),
        ]
    );
    assert!(!open.scanning);
}

#[test]
fn the_mov_of_a_live_photo_is_left_out() {
    let (mut core, scan) = opened();
    let scan = scan.unwrap();
    section(&mut core, scan, "/p", &["/p/IMG_1.HEIC", "/p/IMG_1.MOV"]);
    let paths: Vec<PathBuf> = overview(&core)
        .unwrap()
        .tiles
        .iter()
        .map(|t| t.path.clone())
        .collect();
    assert_eq!(paths, [p("IMG_1.HEIC")]);
}

#[test]
fn sections_of_an_earlier_scan_are_ignored() {
    let (mut core, old) = opened();
    let old = old.unwrap();
    press(&mut core, Key::G);
    let commands = press(&mut core, Key::G);
    let (scan, _, _) = scan_of(&commands).unwrap();
    assert_ne!(scan, old);
    section(&mut core, old, "/p", &["/p/a.jpg"]);
    core.handle(Event::OverviewScanned(old));
    assert!(overview(&core).unwrap().sections.is_empty());
    assert!(overview(&core).unwrap().scanning);
}

#[test]
fn only_visible_tiles_are_decoded_four_at_a_time() {
    let (mut core, scan) = opened();
    let scan = scan.unwrap();
    assert!(decodes(&many(&mut core, scan, 30)).is_empty());
    assert_eq!(decodes(&visible(&mut core, 0..12)), names(0..4));
    assert_eq!(decodes(&thumbnail_decoded(&mut core, 1)), names(4..5));
    assert!(overview(&core).unwrap().tiles[1].thumbnail.is_some());
    assert!(overview(&core).unwrap().tiles[0].thumbnail.is_none());
}

#[test]
fn scrolling_cancels_and_drops_thumbnails_off_screen() {
    let (mut core, scan) = opened();
    let scan = scan.unwrap();
    many(&mut core, scan, 30);
    visible(&mut core, 0..12);
    thumbnail_decoded(&mut core, 0);
    let commands = visible(&mut core, 20..30);
    assert_eq!(cancels(&commands), names(1..5));
    let mut asked = decodes(&commands);
    asked.sort();
    assert_eq!(asked, names(20..24));
    assert!(overview(&core).unwrap().tiles[0].thumbnail.is_none());
    // A cancelled decode that reports anyway is not kept.
    thumbnail_decoded(&mut core, 2);
    assert!(overview(&core).unwrap().tiles[2].thumbnail.is_none());
}

#[test]
fn a_section_arriving_on_screen_gets_its_thumbnails() {
    let (mut core, scan) = opened();
    let scan = scan.unwrap();
    visible(&mut core, 0..10);
    assert_eq!(decodes(&many(&mut core, scan, 3)), names(0..3));
}

#[test]
fn videos_keep_their_placeholder() {
    let (mut core, scan) = opened();
    let scan = scan.unwrap();
    section(&mut core, scan, "/p", &["/p/a.mp4", "/p/b.jpg"]);
    assert_eq!(decodes(&visible(&mut core, 0..2)), ["/p/b.jpg"]);
}

#[test]
fn closing_cancels_the_scan_and_the_thumbnail_decodes() {
    let (mut core, scan) = opened();
    let scan = scan.unwrap();
    many(&mut core, scan, 3);
    visible(&mut core, 0..3);
    let commands = press(&mut core, Key::G);
    assert!(commands.contains(&Command::CancelScan));
    assert_eq!(cancels(&commands), names(0..3));
}

#[test]
fn other_keys_do_nothing_while_the_overview_is_open() {
    let (mut core, _) = opened();
    assert!(press(&mut core, Key::Right).is_empty());
    assert!(press(&mut core, Key::N).is_empty());
    assert!(core.view().dir_column.is_none());
    press(&mut core, Key::G);
    press(&mut core, Key::Right);
    assert!(matches!(&core.view().screen, Screen::Image { path, .. } if *path == p("b.jpg")));
}

#[test]
fn closing_the_settings_screen_keeps_the_overview_keys() {
    let (mut core, _) = opened();
    press(&mut core, Key::S);
    press(&mut core, Key::S);
    assert_eq!(key_names(&core), ["G", "S", "F", QUIT]);
}

#[test]
fn a_playing_video_is_paused_by_g() {
    let (mut core, _) = Core::start(Ok(p("v.mp4")));
    decoded(&mut core, p("v.mp4"));
    core.handle(Event::FirstFrameShown);
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(vec![p("v.mp4")]),
    });
    press(&mut core, Key::Space);
    let commands = press(&mut core, Key::G);
    assert!(commands.contains(&Command::PauseVideo));
}
