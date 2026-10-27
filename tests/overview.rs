//! Seam 1: the app core's overview of `g`: its root, sections streamed from
//! the scan, and thumbnails decoded only for the tiles on screen.

use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use pixpixpix::core::{Command, Core, Event, Focus, Key, KeyPress, Overview, Reveal, Screen};
use pixpixpix::decode::DecodedImage;
use pixpixpix::rotate::Rotation;
use pixpixpix::settings::Settings;

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
        size: 160,
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
            Command::DecodeThumbnail { path, .. } => Some(path.display().to_string()),
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

const GALLERY_KEYS: [&str; 12] = [
    "Arrows",
    "Enter",
    "+/-",
    "C/Shift+C",
    "L/R",
    "O",
    "X",
    "G",
    "Esc",
    "S",
    "F",
    QUIT,
];

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
    assert_eq!(key_names(&core), GALLERY_KEYS);

    let commands = press(&mut core, Key::G);
    assert_eq!(commands, [Command::CancelScan]);
    assert!(core.view().overview.is_none());
    assert!(matches!(&core.view().screen, Screen::Image { path, .. } if *path == p("a.jpg")));
    assert_eq!(key_names(&core).first(), Some(&"Left/Right"));
}

#[test]
fn the_root_is_the_current_folder_the_dir_column_went_to() {
    let mut core = browsing();
    press(&mut core, Key::N);
    core.handle(Event::DirsListed {
        dir: PathBuf::from("/"),
        result: Ok(vec![PathBuf::from("/p"), PathBuf::from("/q")]),
    });
    press(&mut core, Key::Down);
    let commands = press(&mut core, Key::G);
    assert_eq!(
        scan_of(&commands).map(|(_, root, _)| root),
        Some(PathBuf::from("/q"))
    );
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
fn videos_get_their_first_frame_as_thumbnail() {
    let (mut core, scan) = opened();
    let scan = scan.unwrap();
    section(&mut core, scan, "/p", &["/p/a.mp4", "/p/b.jpg"]);
    assert_eq!(decodes(&visible(&mut core, 0..2)), ["/p/a.mp4", "/p/b.jpg"]);
}

#[test]
fn g_out_cancels_the_thumbnail_decodes_and_keeps_the_scan_for_g_back() {
    let (mut core, scan) = opened();
    let scan = scan.unwrap();
    many(&mut core, scan, 3);
    visible(&mut core, 0..3);
    let commands = press(&mut core, Key::G);
    assert!(!commands.contains(&Command::CancelScan));
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
    assert_eq!(key_names(&core), GALLERY_KEYS);
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

fn selected(core: &Core) -> Option<usize> {
    overview(core).and_then(|o| o.selected)
}

fn reveal(core: &Core) -> Option<Reveal> {
    overview(core).and_then(|o| o.reveal)
}

/// [`opened`] with `/p` holding 0.jpg … 4.jpg and `/p/x` holding 5.jpg and
/// 6.jpg, three tiles a row:
///
/// ```text
/// p:   0 1 2
///      3 4
/// x:   5 6
/// ```
fn grid() -> Core {
    let (mut core, scan) = opened();
    let scan = scan.unwrap_or_default();
    many(&mut core, scan, 5);
    section(&mut core, scan, "/p/x", &["/p/5.jpg", "/p/6.jpg"]);
    core.handle(Event::OverviewColumns(3));
    core
}

#[test]
fn the_first_tile_is_selected_once_it_arrives() {
    let (mut core, scan) = opened();
    assert_eq!(selected(&core), None);
    many(&mut core, scan.unwrap(), 2);
    assert_eq!(selected(&core), Some(0));
}

#[test]
fn arrows_move_the_selection_across_rows_and_sections() {
    let mut core = grid();
    let mut moves = Vec::new();
    for key in [
        Key::Up,
        Key::Left,
        Key::Right,
        Key::Down,
        Key::Down,
        Key::Up,
        Key::Up,
        Key::Right,
        Key::Down,
        Key::Left,
        Key::Right,
        Key::Right,
        Key::Right,
    ] {
        assert!(press(&mut core, key).is_empty());
        moves.push(selected(&core).unwrap());
    }
    // Up and Left at the first tile stay; Down from 1 to 4, then to the
    // next section's same column; Up back; Down from 2 to the shorter row's
    // last tile; Right stops at the last tile.
    assert_eq!(moves, [0, 0, 1, 4, 6, 4, 1, 2, 4, 3, 4, 5, 6]);
    press(&mut core, Key::Right);
    assert_eq!(selected(&core), Some(6));
}

#[test]
fn up_goes_to_the_shorter_last_row_of_the_section_before() {
    let mut core = grid();
    for _ in 0..6 {
        press(&mut core, Key::Right);
    }
    press(&mut core, Key::Up);
    // Column 1 of x's row is above 4; column 1 of p's last row.
    assert_eq!(selected(&core), Some(4));
    let mut core = grid();
    core.handle(Event::OverviewColumns(2));
    for _ in 0..6 {
        press(&mut core, Key::Right);
    }
    press(&mut core, Key::Up);
    // p in rows of two ends with 4 alone; column 1 has none, so 4.
    assert_eq!(selected(&core), Some(4));
}

#[test]
fn a_moved_selection_is_revealed_once() {
    let mut core = grid();
    assert_eq!(reveal(&core), None);
    press(&mut core, Key::Down);
    assert_eq!(reveal(&core), Some(Reveal::Tile(3)));
    core.handle(Event::OverviewRevealed);
    assert_eq!(reveal(&core), None);
    core.handle(Event::OverviewVisible(0..7));
    assert_eq!(reveal(&core), None);
}

/// [`opened`] with `/p` holding a.jpg and b.jpg and `/p/x/y` holding c.jpg.
fn tree() -> (Core, u64) {
    let (mut core, scan) = opened();
    let scan = scan.unwrap_or_default();
    section(&mut core, scan, "/p", &["/p/a.jpg", "/p/b.jpg"]);
    section(&mut core, scan, "/p/x/y", &["/p/x/y/c.jpg"]);
    (core, scan)
}

#[test]
fn enter_opens_the_file_in_its_folder_and_g_returns_to_the_same_spot() {
    let (mut core, scan) = tree();
    visible(&mut core, 0..3);
    press(&mut core, Key::Right);
    press(&mut core, Key::Right);
    let commands = press(&mut core, Key::Enter);
    assert!(core.view().overview.is_none());
    assert!(core.view().dir_column.is_none());
    let c = PathBuf::from("/p/x/y/c.jpg");
    assert!(commands.contains(&Command::Decode(c.clone())));
    assert!(commands.contains(&Command::ListFolder(PathBuf::from("/p/x/y"))));
    assert!(commands.contains(&Command::WatchFolder(PathBuf::from("/p/x/y"))));
    assert!(!commands.contains(&Command::CancelScan));
    assert_eq!(cancels(&commands).len(), 3);
    assert!(matches!(&core.view().screen, Screen::Loading { path } if *path == c));
    assert_eq!(key_names(&core).first(), Some(&"Left/Right"));

    // The scan runs on while the file is shown.
    section(&mut core, scan, "/p/z", &["/p/z/d.jpg"]);
    core.handle(Event::OverviewScanned(scan));
    core.handle(Event::Decoded {
        path: c.clone(),
        result: Ok(image()),
    });

    let commands = press(&mut core, Key::G);
    assert_eq!(scan_of(&commands), None);
    let back = overview(&core).unwrap();
    assert_eq!(back.root, PathBuf::from("/p"));
    assert_eq!(back.selected, Some(2));
    assert_eq!(back.reveal, Some(Reveal::Tile(2)));
    assert_eq!(back.sections.len(), 3);
    assert!(!back.scanning);
    assert!(back.tiles.iter().all(|t| t.thumbnail.is_none()));
    assert_eq!(decodes(&visible(&mut core, 0..4)).len(), 4);
}

#[test]
fn enter_in_the_current_folder_shows_the_file_without_listing_again() {
    let (mut core, _) = tree();
    press(&mut core, Key::Right);
    let commands = press(&mut core, Key::Enter);
    assert!(
        !commands
            .iter()
            .any(|c| matches!(c, Command::ListFolder(_) | Command::WatchFolder(_)))
    );
    assert!(matches!(&core.view().screen, Screen::Image { path, .. } if *path == p("b.jpg")));
}

#[test]
fn a_click_on_a_tile_opens_it() {
    let (mut core, _) = tree();
    core.handle(Event::OpenTile(1));
    assert!(core.view().overview.is_none());
    assert!(matches!(&core.view().screen, Screen::Image { path, .. } if *path == p("b.jpg")));
}

#[test]
fn opening_with_the_dir_column_open_moves_the_column_too() {
    let mut core = browsing();
    press(&mut core, Key::N);
    core.handle(Event::DirsListed {
        dir: PathBuf::from("/p"),
        result: Ok(Vec::new()),
    });
    let commands = press(&mut core, Key::G);
    let (scan, _, _) = scan_of(&commands).unwrap();
    section(&mut core, scan, "/p/x/y", &["/p/x/y/c.jpg"]);
    let commands = press(&mut core, Key::Enter);
    // The column lists the new folder among its siblings.
    assert_eq!(
        core.view().dir_column.as_ref().map(|c| c.dir.clone()),
        Some(PathBuf::from("/p/x"))
    );
    assert!(commands.contains(&Command::ListDirs(PathBuf::from("/p/x"))));
    assert!(commands.contains(&Command::ListDirs(PathBuf::from("/p/x/y"))));
    assert_eq!(core.view().focus, Focus::Image);
}

#[test]
fn g_out_keeps_the_overview_and_the_next_g_brings_it_back() {
    let (mut core, _) = tree();
    let sections = overview(&core).unwrap().sections.len();
    press(&mut core, Key::G);
    assert!(overview(&core).is_none());
    let commands = press(&mut core, Key::G);
    assert!(scan_of(&commands).is_none());
    assert_eq!(overview(&core).unwrap().sections.len(), sections);
}

/// The commands' trash requests.
fn trashes(commands: &[Command]) -> Vec<PathBuf> {
    commands
        .iter()
        .filter_map(|c| match c {
            Command::Trash(path) => Some(path.clone()),
            _ => None,
        })
        .collect()
}

fn trashed(core: &mut Core, path: &str) -> Vec<Command> {
    core.handle(Event::Trashed {
        path: PathBuf::from(path),
        result: Ok(()),
    })
}

#[test]
fn x_trashes_the_selected_file_with_its_live_mov_and_its_tile_goes() {
    let (mut core, scan) = opened();
    let scan = scan.unwrap();
    section(
        &mut core,
        scan,
        "/q",
        &["/q/a.jpg", "/q/IMG_1.HEIC", "/q/IMG_1.MOV", "/q/z.jpg"],
    );
    section(&mut core, scan, "/q/r", &["/q/r/c.jpg"]);
    press(&mut core, Key::Right);
    let commands = press(&mut core, Key::X);
    assert_eq!(
        trashes(&commands),
        [
            PathBuf::from("/q/IMG_1.HEIC"),
            PathBuf::from("/q/IMG_1.MOV")
        ]
    );
    assert!(trashes(&press(&mut core, Key::X)).is_empty());
    assert_eq!(overview(&core).unwrap().tiles.len(), 4);

    trashed(&mut core, "/q/IMG_1.HEIC");
    trashed(&mut core, "/q/IMG_1.MOV");
    let open = overview(&core).unwrap();
    let paths: Vec<&str> = open
        .tiles
        .iter()
        .map(|t| t.path.to_str().unwrap())
        .collect();
    assert_eq!(paths, ["/q/a.jpg", "/q/z.jpg", "/q/r/c.jpg"]);
    assert_eq!(open.sections[0].tiles, 0..2);
    assert_eq!(open.sections[1].tiles, 2..3);
    assert_eq!(open.selected, Some(1));
}

#[test]
fn trashing_the_last_tile_of_a_section_drops_it_and_selects_the_one_before() {
    let (mut core, _) = tree();
    press(&mut core, Key::Right);
    press(&mut core, Key::Right);
    trashes(&press(&mut core, Key::X));
    trashed(&mut core, "/p/x/y/c.jpg");
    let open = overview(&core).unwrap();
    assert_eq!(open.sections.len(), 1);
    assert_eq!(open.selected, Some(1));
}

#[test]
fn a_failed_trash_keeps_the_tile_and_says_why() {
    let (mut core, _) = tree();
    press(&mut core, Key::X);
    core.handle(Event::Trashed {
        path: p("a.jpg"),
        result: Err("Cannot trash a.jpg".into()),
    });
    assert_eq!(overview(&core).unwrap().tiles.len(), 3);
    assert_eq!(
        core.view().trash_error.as_deref(),
        Some("Cannot trash a.jpg")
    );
}

#[test]
fn trashing_the_shown_file_from_the_overview_steps_the_image_view_on() {
    let (mut core, _) = tree();
    press(&mut core, Key::X);
    trashed(&mut core, "/p/a.jpg");
    press(&mut core, Key::G);
    assert!(matches!(&core.view().screen, Screen::Image { path, .. } if *path == p("b.jpg")));
}

#[test]
fn c_copies_a_cached_file_at_once_and_decodes_another_first() {
    let (mut core, _) = tree();
    press(&mut core, Key::Right);
    let commands = press(&mut core, Key::C);
    assert_eq!(
        commands,
        [Command::Copy {
            image: image(),
            rotation: Rotation::None,
            shapes: Vec::new(),
        }]
    );

    press(&mut core, Key::Right);
    let c = PathBuf::from("/p/x/y/c.jpg");
    assert_eq!(press(&mut core, Key::C), [Command::Decode(c.clone())]);
    let commands = core.handle(Event::Decoded {
        path: c.clone(),
        result: Ok(image()),
    });
    assert_eq!(
        commands,
        [Command::Copy {
            image: image(),
            rotation: Rotation::None,
            shapes: Vec::new(),
        }]
    );

    press(&mut core, Key::C);
    core.handle(Event::Decoded {
        path: c,
        result: Err("broken".into()),
    });
    assert_eq!(core.view().copy_error.as_deref(), Some("broken"));
}

#[test]
fn o_opens_the_selected_file_in_the_app_for_its_kind() {
    let (mut core, scan) = opened();
    section(&mut core, scan.unwrap(), "/p", &["/p/a.jpg", "/p/v.mp4"]);
    core.handle(Event::SettingsLoaded(Ok(Settings {
        video_app: "vlc".into(),
        ..Settings::default()
    })));
    press(&mut core, Key::Right);
    assert_eq!(
        press(&mut core, Key::O),
        [Command::OpenExternally {
            path: p("v.mp4"),
            app: Some("vlc".into()),
            earlier: None,
        }]
    );
}

fn sizes(commands: &[Command]) -> Vec<u32> {
    commands
        .iter()
        .filter_map(|c| match c {
            Command::DecodeThumbnail { size, .. } => Some(*size),
            _ => None,
        })
        .collect()
}

#[test]
fn plus_and_minus_step_the_thumbnail_size_remake_the_thumbnails_and_save_it() {
    let (mut core, _) = tree();
    assert_eq!(sizes(&visible(&mut core, 0..3)), [160, 160, 160]);
    let commands = press(&mut core, Key::Plus);
    assert_eq!(core.view().settings.thumbnail_size, 200);
    assert_eq!(cancels(&commands).len(), 3);
    assert_eq!(sizes(&commands), [200, 200, 200]);
    assert_eq!(
        commands.last(),
        Some(&Command::SaveSettings(Settings {
            thumbnail_size: 200,
            ..Settings::default()
        }))
    );
    for _ in 0..5 {
        press(&mut core, Key::Plus);
    }
    assert_eq!(core.view().settings.thumbnail_size, 320);
    assert!(press(&mut core, Key::Plus).is_empty());
    core.handle(Event::ThumbnailWheel { larger: false });
    assert_eq!(core.view().settings.thumbnail_size, 256);
    for _ in 0..6 {
        press(&mut core, Key::Minus);
    }
    assert_eq!(core.view().settings.thumbnail_size, 96);
}

#[test]
fn the_thumbnail_size_comes_from_the_settings() {
    let (mut core, _) = tree();
    core.handle(Event::SettingsLoaded(Ok(Settings {
        thumbnail_size: 96,
        ..Settings::default()
    })));
    assert_eq!(sizes(&visible(&mut core, 0..1)), [96]);
}

#[test]
fn the_wheel_resizes_only_while_the_overview_is_open() {
    let mut core = browsing();
    assert!(
        core.handle(Event::ThumbnailWheel { larger: true })
            .is_empty()
    );
    assert_eq!(core.view().settings.thumbnail_size, 160);
}

/// [`browsing`] with the dir column on `/p`, its subfolders `x` and `z`
/// listed, and the overview opened on the highlighted `x`; the scan's
/// number.
fn contents() -> (Core, u64) {
    let mut core = browsing();
    press(&mut core, Key::N);
    core.handle(Event::DirsListed {
        dir: PathBuf::from("/"),
        result: Ok(vec![PathBuf::from("/p"), PathBuf::from("/q")]),
    });
    core.handle(Event::DirsListed {
        dir: PathBuf::from("/p"),
        result: Ok(vec![p("x"), p("z")]),
    });
    let commands = press(&mut core, Key::G);
    let (scan, root, _) = scan_of(&commands).unwrap_or_default();
    assert_eq!(root, PathBuf::from("/p"));
    (core, scan)
}

#[test]
fn the_overview_takes_the_focus_and_gives_it_back() {
    let (mut core, _) = contents();
    assert_eq!(core.view().focus, Focus::Image);
    press(&mut core, Key::G);
    assert_eq!(core.view().focus, Focus::Dirs);
}

#[test]
fn tab_moves_between_dir_column_and_gallery_and_the_key_line_follows() {
    let (mut core, _) = contents();
    assert_eq!(key_names(&core).last(), Some(&QUIT));
    assert!(key_names(&core).contains(&"Tab"));
    press(&mut core, Key::Tab);
    assert_eq!(core.view().focus, Focus::Dirs);
    assert_eq!(
        key_names(&core),
        [
            "Up/Down",
            "Left/Backspace",
            "Esc",
            "Right/Enter",
            "G",
            "Tab",
            "S",
            "F",
            QUIT
        ]
    );
    press(&mut core, Key::Tab);
    assert_eq!(core.view().focus, Focus::Image);
    assert!(press(&mut core, Key::B).is_empty());
    assert_eq!(core.view().focus, Focus::Image);

    let (mut core, _) = opened();
    press(&mut core, Key::Tab);
    assert_eq!(core.view().focus, Focus::Image);
    assert_eq!(key_names(&core), GALLERY_KEYS);
}

#[test]
fn going_up_in_the_dir_column_re_roots_and_aborts_the_scan() {
    let (mut core, scan) = contents();
    section(&mut core, scan, "/p/x", &["/p/x/a.jpg"]);
    visible(&mut core, 0..1);
    press(&mut core, Key::Tab);
    let commands = press(&mut core, Key::Left);
    assert!(commands.contains(&Command::CancelScan));
    assert_eq!(cancels(&commands), ["/p/x/a.jpg"]);
    let (new, root, _) = scan_of(&commands).unwrap();
    assert_ne!(new, scan);
    assert_eq!(root, PathBuf::from("/"));
    let open = overview(&core).unwrap();
    assert!(open.sections.is_empty());
    assert_eq!(open.selected, None);
    // Sections of the aborted scan are ignored.
    section(&mut core, scan, "/p/x", &["/p/x/b.jpg"]);
    assert!(overview(&core).unwrap().sections.is_empty());
}

#[test]
fn dot_and_r_do_nothing_in_the_dir_column_of_the_overview() {
    let (mut core, _) = contents();
    press(&mut core, Key::Tab);
    assert!(press(&mut core, Key::Period).is_empty());
    assert!(press(&mut core, Key::R).is_empty());
}

#[test]
fn g_outside_the_kept_overview_s_root_drops_it_and_scans_anew() {
    let (mut core, scan) = contents();
    section(&mut core, scan, "/p/x", &["/p/x/a.jpg"]);
    let commands = press(&mut core, Key::Enter);
    assert!(!commands.contains(&Command::CancelScan));
    press(&mut core, Key::Tab);
    press(&mut core, Key::Tab);
    assert_eq!(core.view().focus, Focus::Dirs);
    // Up to the root, outside the kept overview's.
    press(&mut core, Key::Left);
    press(&mut core, Key::Left);
    let commands = press(&mut core, Key::G);
    assert!(commands.contains(&Command::CancelScan));
    assert_eq!(
        scan_of(&commands).map(|(_, root, _)| root),
        Some(PathBuf::from("/"))
    );
}

#[test]
fn a_late_thumbnail_of_the_old_size_waits_for_the_new_one() {
    let (mut core, _) = tree();
    visible(&mut core, 0..3);
    press(&mut core, Key::Plus);
    core.handle(Event::ThumbnailDecoded {
        path: p("a.jpg"),
        size: 160,
        result: Ok(image()),
    });
    assert!(overview(&core).unwrap().tiles[0].thumbnail.is_none());
    core.handle(Event::ThumbnailDecoded {
        path: p("a.jpg"),
        size: 200,
        result: Ok(image()),
    });
    assert!(overview(&core).unwrap().tiles[0].thumbnail.is_some());
}

#[test]
fn c_on_a_file_the_image_view_decodes_copies_even_when_the_view_drops_it() {
    let (mut core, _) = Core::start(Ok(p("a.jpg")));
    decoded(&mut core, p("a.jpg"));
    core.handle(Event::FirstFrameShown);
    let files = ["a.jpg", "b.jpg", "c.jpg", "d.jpg", "e.jpg", "f.jpg"];
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(files.iter().map(|n| p(n)).collect()),
    });
    let scan = scan_of(&press(&mut core, Key::G)).unwrap().0;
    section(&mut core, scan, "/p", &["/p/a.jpg", "/p/f.jpg"]);
    press(&mut core, Key::Right);
    assert_eq!(press(&mut core, Key::C), []);

    // A new file pushes f.jpg out of the image view's cache.
    let commands = core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(["a.jpg", "a2.jpg"]
            .iter()
            .chain(&files[1..])
            .map(|n| p(n))
            .collect()),
    });
    assert!(!commands.contains(&Command::CancelDecode(p("f.jpg"))));
    assert_eq!(
        decoded(&mut core, p("f.jpg")),
        [Command::Copy {
            image: image(),
            rotation: Rotation::None,
            shapes: Vec::new(),
        }]
    );
}

#[test]
fn up_and_down_in_the_dir_column_switch_the_overview_to_the_sibling_folder() {
    let (mut core, scan) = contents();
    press(&mut core, Key::Tab);
    let commands = press(&mut core, Key::Down);
    assert!(commands.contains(&Command::CancelScan));
    let (new, root, _) = scan_of(&commands).unwrap();
    assert_ne!(new, scan);
    assert_eq!(root, PathBuf::from("/q"));
    assert_eq!(
        overview(&core).map(|o| o.root.clone()),
        Some(PathBuf::from("/q"))
    );
    // The current folder went along: the column is the same as without it.
    let column = core.view().dir_column.clone().unwrap();
    assert_eq!(
        column.highlight.and_then(|i| column.dirs.get(i)).cloned(),
        Some(PathBuf::from("/q"))
    );
}

#[test]
fn g_in_selects_the_tile_of_the_file_shown_once_it_arrives() {
    let mut core = browsing();
    // b.jpg is shown.
    press(&mut core, Key::Right);
    decoded(&mut core, p("b.jpg"));
    let commands = press(&mut core, Key::G);
    let (scan, _, _) = scan_of(&commands).unwrap();
    section(&mut core, scan, "/p", &["/p/a.jpg", "/p/b.jpg"]);
    assert_eq!(overview(&core).unwrap().selected, Some(1));
}

#[test]
fn a_selection_moved_before_the_shown_file_arrives_stays() {
    let mut core = browsing();
    press(&mut core, Key::Right);
    decoded(&mut core, p("b.jpg"));
    let commands = press(&mut core, Key::G);
    let (scan, _, _) = scan_of(&commands).unwrap();
    section(&mut core, scan, "/p/x", &["/p/x/1.jpg", "/p/x/2.jpg"]);
    press(&mut core, Key::Right);
    section(&mut core, scan, "/p", &["/p/a.jpg", "/p/b.jpg"]);
    assert_eq!(overview(&core).unwrap().selected, Some(1));
}

#[test]
fn g_out_shows_the_selected_tile_and_the_overview_is_tagged_while_open() {
    let mut core = browsing();
    let commands = press(&mut core, Key::G);
    assert_eq!(
        core.view().tags,
        [pixpixpix::core::Tag {
            toggle: pixpixpix::core::Toggle::Overview,
            kept: false
        }]
    );
    let (scan, _, _) = scan_of(&commands).unwrap();
    section(&mut core, scan, "/p", &["/p/a.jpg", "/p/b.jpg"]);
    press(&mut core, Key::Right);
    press(&mut core, Key::G);
    assert!(core.view().tags.is_empty());
    assert!(matches!(
        &core.view().screen,
        Screen::Image { path, .. } if *path == p("b.jpg")
    ));
}

#[test]
fn shift_c_copies_the_selected_file() {
    let mut core = browsing();
    let commands = press(&mut core, Key::G);
    let (scan, _, _) = scan_of(&commands).unwrap();
    section(&mut core, scan, "/p", &["/p/a.jpg", "/p/b.jpg"]);
    press(&mut core, Key::Right);
    let mut shift_c = KeyPress::plain(Key::C);
    shift_c.shift = true;
    assert_eq!(
        core.handle(Event::Key(shift_c)),
        [Command::CopyFile(p("b.jpg"))]
    );
}

#[test]
fn esc_in_the_gallery_or_its_column_shows_the_selected_tile_in_the_image_view() {
    for in_column in [false, true] {
        let (mut core, scan) = contents();
        section(&mut core, scan, "/p", &["/p/a.jpg", "/p/b.jpg"]);
        press(&mut core, Key::Right);
        if in_column {
            press(&mut core, Key::Tab);
            assert_eq!(core.view().focus, Focus::Dirs);
        }
        press(&mut core, Key::Escape);
        assert!(overview(&core).is_none(), "{in_column}");
        assert_eq!(core.view().focus, Focus::Image, "{in_column}");
        assert!(matches!(
            &core.view().screen,
            Screen::Image { path, .. } if *path == p("b.jpg")
        ));
    }
}

/// The gallery of `/p` with `/p` holding a.jpg, b.jpg, c.jpg and `/p/x`
/// holding 1.jpg, b.jpg selected.
fn gallery_with_sections() -> Core {
    let mut core = browsing();
    let commands = press(&mut core, Key::G);
    let (scan, _, _) = scan_of(&commands).unwrap_or_default();
    section(&mut core, scan, "/p", &["/p/a.jpg", "/p/b.jpg", "/p/c.jpg"]);
    section(&mut core, scan, "/p/x", &["/p/x/1.jpg"]);
    press(&mut core, Key::Right);
    core
}

fn tile_names(core: &Core) -> Vec<String> {
    overview(core)
        .map(|o| {
            o.tiles
                .iter()
                .map(|t| pixpixpix::folder::file_name(&t.path))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn u_in_the_gallery_puts_the_tile_back_in_its_section_selected() {
    let mut core = gallery_with_sections();
    press(&mut core, Key::X);
    core.handle(Event::Trashed {
        path: p("b.jpg"),
        result: Ok(()),
    });
    assert_eq!(tile_names(&core), ["a.jpg", "c.jpg", "1.jpg"]);
    assert_eq!(
        press(&mut core, Key::U),
        [Command::Restore(vec![p("b.jpg")])]
    );
    core.handle(Event::Restored {
        paths: vec![p("b.jpg")],
        result: Ok(()),
    });
    let open = overview(&core).unwrap();
    assert_eq!(tile_names(&core), ["a.jpg", "b.jpg", "c.jpg", "1.jpg"]);
    assert_eq!(open.selected, Some(1));
    assert_eq!(open.sections[0].tiles, 0..3);
    assert_eq!(open.sections[1].tiles, 3..4);
}

#[test]
fn a_restored_tile_brings_back_the_section_its_folder_lost() {
    let mut core = gallery_with_sections();
    for _ in 0..2 {
        press(&mut core, Key::Right);
    }
    press(&mut core, Key::X);
    core.handle(Event::Trashed {
        path: PathBuf::from("/p/x/1.jpg"),
        result: Ok(()),
    });
    assert_eq!(overview(&core).unwrap().sections.len(), 1);
    press(&mut core, Key::U);
    core.handle(Event::Restored {
        paths: vec![PathBuf::from("/p/x/1.jpg")],
        result: Ok(()),
    });
    let open = overview(&core).unwrap();
    assert_eq!(open.sections.len(), 2);
    assert_eq!(open.sections[1].dir, PathBuf::from("/p/x"));
    assert_eq!(open.sections[1].tiles, 3..4);
    assert_eq!(open.selected, Some(3));
}

fn shift(key: Key) -> Event {
    let mut press = KeyPress::plain(key);
    press.shift = true;
    Event::Key(press)
}

#[test]
fn shift_x_asks_and_esc_keeps_the_files() {
    let mut core = gallery_with_sections();
    assert!(core.handle(shift(Key::X)).is_empty());
    assert_eq!(
        core.view().trash_section,
        Some(pixpixpix::core::TrashSection {
            dir: PathBuf::from("/p"),
            count: 3
        })
    );
    // Other keys wait for the answer.
    assert!(press(&mut core, Key::Right).is_empty());
    assert!(press(&mut core, Key::Escape).is_empty());
    assert_eq!(core.view().trash_section, None);
    assert!(overview(&core).is_some());
}

#[test]
fn shift_x_trashes_only_the_folder_and_one_u_brings_it_all_back() {
    let mut core = gallery_with_sections();
    core.handle(shift(Key::X));
    let all = vec![p("a.jpg"), p("b.jpg"), p("c.jpg")];
    assert_eq!(
        press(&mut core, Key::Enter),
        all.iter().cloned().map(Command::Trash).collect::<Vec<_>>()
    );
    for path in &all {
        core.handle(Event::Trashed {
            path: path.clone(),
            result: Ok(()),
        });
    }
    assert_eq!(tile_names(&core), ["1.jpg"]);
    assert_eq!(press(&mut core, Key::U), [Command::Restore(all.clone())]);
    core.handle(Event::Restored {
        paths: all,
        result: Ok(()),
    });
    assert_eq!(tile_names(&core), ["a.jpg", "b.jpg", "c.jpg", "1.jpg"]);
    let open = overview(&core).unwrap();
    assert_eq!(open.selected, Some(0));
    assert_eq!(open.sections[0].tiles, 0..3);
    assert_eq!(open.sections[1].tiles, 3..4);
}

#[test]
fn l_and_r_in_the_gallery_write_the_turn_and_redraw_the_tile() {
    let mut core = gallery_with_sections();
    visible(&mut core, 0..4);
    assert_eq!(
        press(&mut core, Key::R),
        [Command::WriteRotation {
            path: p("b.jpg"),
            rotation: Rotation::Right
        }]
    );
    assert_eq!(
        press(&mut core, Key::L),
        [Command::WriteRotation {
            path: p("b.jpg"),
            rotation: Rotation::Left
        }]
    );
    let commands = core.handle(Event::RotationWritten {
        path: p("b.jpg"),
        result: Ok(()),
    });
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, Command::DecodeThumbnail { path, .. } if *path == p("b.jpg"))),
        "{commands:?}"
    );
}

#[test]
fn a_video_in_the_gallery_cannot_be_turned() {
    let mut core = browsing();
    let commands = press(&mut core, Key::G);
    let (scan, _, _) = scan_of(&commands).unwrap();
    section(&mut core, scan, "/p", &["/p/v.mp4"]);
    assert!(press(&mut core, Key::R).is_empty());
    assert_eq!(
        core.view().rotation_error.as_deref(),
        Some("Cannot rotate v.mp4: a video cannot be turned")
    );
}
