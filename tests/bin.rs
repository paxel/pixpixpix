//! Seam 1: the bin view on `t`, with the trash's listing given as events.

use std::path::PathBuf;
use std::sync::Arc;

use pixpixpix::core::{BinItem, Command, Core, Event, Key, KeyPress, Notice, Tag, Toggle};
use pixpixpix::decode::DecodedImage;

fn image() -> DecodedImage {
    DecodedImage {
        width: 1,
        height: 1,
        rgba: Arc::from(vec![0; 4]),
    }
}

fn press(core: &mut Core, key: Key) -> Vec<Command> {
    core.handle(Event::Key(KeyPress::plain(key)))
}

fn showing() -> Core {
    let (mut core, _) = Core::start(Ok(PathBuf::from("/p/a.jpg")));
    core.handle(Event::Decoded {
        path: PathBuf::from("/p/a.jpg"),
        result: Ok(image()),
    });
    core.handle(Event::FirstFrameShown);
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(vec![PathBuf::from("/p/a.jpg")]),
    });
    core
}

/// Noon UTC on day `day` of October 2026: the same day in every timezone
/// within half a day of UTC.
fn october(day: i64) -> i64 {
    1_790_812_800 + (day - 1) * 86_400 + 43_200
}

fn item(name: &str, deleted: i64) -> BinItem {
    BinItem {
        id: format!("/t/info/{name}.trashinfo").into(),
        file: PathBuf::from(format!("/t/files/{name}")),
        original: Some(PathBuf::from(format!("/p/{name}"))),
        deleted,
    }
}

/// [`showing`] with the bin open on b.jpg and c.jpg trashed on the 25th and
/// d.jpg on the 20th.
fn bin() -> Core {
    let mut core = showing();
    assert_eq!(press(&mut core, Key::T), [Command::ListBin]);
    core.handle(Event::BinListed(Ok(vec![
        item("d.jpg", october(20)),
        item("b.jpg", october(25)),
        item("c.jpg", october(25) + 60),
    ])));
    core
}

fn tiles(core: &Core) -> Vec<PathBuf> {
    core.view()
        .overview
        .as_ref()
        .map(|o| o.tiles.iter().map(|t| t.path.clone()).collect())
        .unwrap_or_default()
}

#[test]
fn t_lists_the_trash_by_day_newest_first() {
    let core = bin();
    let open = core.view().overview.clone().unwrap();
    assert!(open.bin);
    let titles: Vec<&str> = open.sections.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(titles, ["2026-10-25", "2026-10-20"]);
    assert_eq!(
        tiles(&core),
        ["/t/files/c.jpg", "/t/files/b.jpg", "/t/files/d.jpg"].map(PathBuf::from)
    );
    assert_eq!(open.selected, Some(0));
    assert_eq!(
        core.view().tags,
        [Tag {
            toggle: Toggle::Bin,
            kept: false
        }]
    );
}

#[test]
fn enter_shows_the_selected_file_large_and_back() {
    let mut core = bin();
    assert_eq!(
        press(&mut core, Key::Enter),
        [Command::Decode(PathBuf::from("/t/files/c.jpg"))]
    );
    core.handle(Event::Decoded {
        path: PathBuf::from("/t/files/c.jpg"),
        result: Ok(image()),
    });
    let preview = core.view().bin_preview.clone().unwrap();
    assert_eq!(preview.image, Some(Ok(image())));
    press(&mut core, Key::Escape);
    assert_eq!(core.view().bin_preview, None);
    assert!(core.view().overview.is_some());
}

#[test]
fn u_restores_the_selected_file_and_its_tile_goes() {
    let mut core = bin();
    press(&mut core, Key::Right);
    assert_eq!(
        press(&mut core, Key::U),
        [Command::RestoreFromBin(item("b.jpg", october(25)))]
    );
    let commands = core.handle(Event::BinRestored {
        file: PathBuf::from("/t/files/b.jpg"),
        result: Ok(()),
    });
    // b.jpg is back in the folder shown, read again.
    assert!(commands.contains(&Command::ListFolder(PathBuf::from("/p"))));
    assert_eq!(
        tiles(&core),
        ["/t/files/c.jpg", "/t/files/d.jpg"].map(PathBuf::from)
    );
    assert_eq!(core.view().key_line.notice, Some(Notice::Restored));
}

#[test]
fn a_failed_restore_says_why_and_the_file_stays() {
    let mut core = bin();
    press(&mut core, Key::U);
    core.handle(Event::BinRestored {
        file: PathBuf::from("/t/files/c.jpg"),
        result: Err("Cannot restore c.jpg: a file of that name is back there".into()),
    });
    assert_eq!(tiles(&core).len(), 3);
    assert_eq!(
        core.view().trash_error.as_deref(),
        Some("Cannot restore c.jpg: a file of that name is back there")
    );
}

#[test]
fn t_and_esc_close_the_bin_and_nothing_deletes_for_good() {
    let mut core = bin();
    assert!(press(&mut core, Key::X).is_empty());
    assert!(press(&mut core, Key::G).is_empty());
    press(&mut core, Key::T);
    assert_eq!(core.view().overview, None);
    let mut core = bin();
    press(&mut core, Key::Escape);
    assert_eq!(core.view().overview, None);
}
