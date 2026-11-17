//! Seam 1: what the items of the macOS app menu send to the core, and what
//! the core does with it.

use std::path::PathBuf;
use std::sync::Arc;

use pixpixpix::core::{Command, Core, Event, Key, KeyPress};
use pixpixpix::decode::DecodedImage;
use pixpixpix::menu::Item;

/// Showing `/p/a.jpg`.
fn browsing() -> Core {
    let (mut core, _) = Core::start(Ok(PathBuf::from("/p/a.jpg")));
    core.handle(Event::Decoded {
        path: PathBuf::from("/p/a.jpg"),
        result: Ok(DecodedImage {
            width: 1,
            height: 1,
            rgba: Arc::from(vec![0; 4]),
        }),
    });
    core.handle(Event::FirstFrameShown);
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(vec![PathBuf::from("/p/a.jpg")]),
    });
    core
}

#[test]
fn the_items_send_the_keys_that_do_the_same() {
    assert_eq!(Item::About.event(), Event::Key(KeyPress::plain(Key::F1)));
    assert_eq!(Item::Settings.event(), Event::Key(KeyPress::plain(Key::S)));
    assert_eq!(Item::Quit.event(), Event::Key(KeyPress::command(Key::Q)));
}

#[test]
fn the_items_are_titled_and_chosen_as_a_mac_expects() {
    assert_eq!(Item::About.title(), "About pixpixpix");
    assert_eq!(Item::Settings.title(), "Settings…");
    assert_eq!(Item::Quit.title(), "Quit pixpixpix");
    assert_eq!(Item::About.key_equivalent(), "");
    assert_eq!(Item::Settings.key_equivalent(), ",");
    assert_eq!(Item::Quit.key_equivalent(), "q");
}

#[test]
fn about_opens_the_about_page() {
    let mut core = browsing();
    assert_eq!(core.handle(Item::About.event()), [Command::ProbeLibraries]);
    assert!(core.view().about.is_some());
    assert!(!core.view().settings_open);
}

#[test]
fn settings_opens_the_settings_screen() {
    let mut core = browsing();
    core.handle(Item::Settings.event());
    assert!(core.view().settings_open);
}

#[test]
fn quit_quits_as_the_key_does() {
    let mut core = browsing();
    assert_eq!(core.handle(Item::Quit.event()), [Command::Quit]);
}
