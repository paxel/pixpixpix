//! Seam 1: the pane keys `n`, `b`, `i` and `p`. The dir column and the band
//! take the focus: a press shows and focuses a hidden one, focuses a shown
//! one, and hides the focused one. Info and the tools are shown and hidden
//! from every focus area, and the focus stays.

use std::path::PathBuf;
use std::sync::Arc;

use pixpixpix::core::{Command, Core, Event, Focus, Key, KeyPress};
use pixpixpix::decode::DecodedImage;
use pixpixpix::settings::Settings;

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

/// Showing `/p/b.jpg` of `/p/a.jpg` … `/p/c.jpg`.
fn browsing() -> Core {
    let (mut core, _) = Core::start(Ok(PathBuf::from("/p/b.jpg")));
    core.handle(Event::Decoded {
        path: PathBuf::from("/p/b.jpg"),
        result: Ok(image()),
    });
    core.handle(Event::FirstFrameShown);
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(["/p/a.jpg", "/p/b.jpg", "/p/c.jpg"]
            .iter()
            .map(PathBuf::from)
            .collect()),
    });
    core
}

/// [`browsing`] with the focus moved to `focus`, the dir column open for
/// [`Focus::Dirs`] only.
fn focused(focus: Focus) -> Core {
    let mut core = browsing();
    match focus {
        Focus::Image => {}
        Focus::Dirs => {
            press(&mut core, Key::N);
        }
        Focus::Band => {
            press(&mut core, Key::B);
        }
    }
    assert_eq!(core.view().focus, focus);
    core
}

fn action_of(core: &Core, keys: &str) -> Option<&'static str> {
    core.view()
        .key_line
        .keys
        .iter()
        .find(|k| k.keys == keys)
        .map(|k| k.action)
}

const AREAS: [Focus; 3] = [Focus::Image, Focus::Dirs, Focus::Band];

#[test]
fn n_shows_and_focuses_the_hidden_column() {
    for from in [Focus::Image, Focus::Band] {
        let mut core = focused(from);
        press(&mut core, Key::N);
        assert!(core.view().dir_column.is_some(), "{from:?}");
        assert_eq!(core.view().focus, Focus::Dirs, "{from:?}");
    }
}

#[test]
fn n_focuses_the_shown_column_from_the_image_view_and_the_band() {
    for from in [Focus::Image, Focus::Band] {
        let mut core = focused(Focus::Dirs);
        press(&mut core, Key::Tab);
        if from == Focus::Band {
            press(&mut core, Key::Tab);
        }
        assert_eq!(core.view().focus, from);
        assert!(press(&mut core, Key::N).is_empty(), "{from:?}");
        assert!(core.view().dir_column.is_some(), "{from:?}");
        assert_eq!(core.view().focus, Focus::Dirs, "{from:?}");
    }
}

#[test]
fn n_hides_the_focused_column_and_focuses_the_image_view() {
    let mut core = focused(Focus::Dirs);
    press(&mut core, Key::N);
    assert_eq!(core.view().dir_column, None);
    assert_eq!(core.view().focus, Focus::Image);
}

#[test]
fn b_focuses_the_shown_band_from_the_column_and_hides_it_from_inside() {
    let mut core = focused(Focus::Dirs);
    assert!(press(&mut core, Key::B).is_empty());
    assert_eq!(core.view().focus, Focus::Band);
    assert!(!core.view().settings.band_hidden);
    press(&mut core, Key::B);
    assert!(core.view().settings.band_hidden);
    assert_eq!(core.view().focus, Focus::Image);
}

#[test]
fn b_shows_and_focuses_the_hidden_band_from_the_column() {
    let mut core = focused(Focus::Dirs);
    core.handle(Event::SettingsLoaded(Ok(Settings {
        band_hidden: true,
        ..Settings::default()
    })));
    press(&mut core, Key::B);
    assert!(!core.view().settings.band_hidden);
    assert_eq!(core.view().focus, Focus::Band);
}

#[test]
fn i_shows_and_hides_info_from_every_area_and_the_focus_stays() {
    for area in AREAS {
        let mut core = focused(area);
        assert_eq!(
            press(&mut core, Key::I),
            [Command::ReadMetadata(PathBuf::from("/p/b.jpg"))],
            "{area:?}"
        );
        assert!(core.view().metadata.is_some(), "{area:?}");
        assert_eq!(core.view().focus, area, "{area:?}");
        press(&mut core, Key::I);
        assert!(core.view().metadata.is_none(), "{area:?}");
        assert_eq!(core.view().focus, area, "{area:?}");
    }
}

#[test]
fn p_shows_and_hides_the_tools_from_every_area_and_the_focus_stays() {
    for area in AREAS {
        let mut core = focused(area);
        press(&mut core, Key::P);
        assert!(core.view().toolbar.is_some(), "{area:?}");
        assert_eq!(core.view().focus, area, "{area:?}");
        press(&mut core, Key::P);
        assert!(core.view().toolbar.is_none(), "{area:?}");
        assert_eq!(core.view().focus, area, "{area:?}");
    }
}

#[test]
fn the_hints_of_the_pane_keys_say_what_a_press_does_now() {
    for area in AREAS {
        let mut core = focused(area);
        let folders = if area == Focus::Dirs {
            "hide folders"
        } else {
            "folders"
        };
        let band = if area == Focus::Band {
            "hide preloaded files"
        } else {
            "preloaded files"
        };
        assert_eq!(action_of(&core, "N"), Some(folders), "{area:?}");
        assert_eq!(action_of(&core, "B"), Some(band), "{area:?}");
        assert_eq!(action_of(&core, "I"), Some("info"), "{area:?}");
        assert_eq!(action_of(&core, "P"), Some("tools"), "{area:?}");
        press(&mut core, Key::I);
        press(&mut core, Key::P);
        assert_eq!(action_of(&core, "I"), Some("hide info"), "{area:?}");
        assert_eq!(action_of(&core, "P"), Some("hide tools"), "{area:?}");
        press(&mut core, Key::I);
        press(&mut core, Key::P);
        assert_eq!(action_of(&core, "I"), Some("info"), "{area:?}");
        assert_eq!(action_of(&core, "P"), Some("tools"), "{area:?}");
    }
}

#[test]
fn the_folders_hint_says_folders_while_the_shown_column_is_not_focused() {
    let mut core = focused(Focus::Dirs);
    press(&mut core, Key::Tab);
    assert_eq!(action_of(&core, "N"), Some("folders"));
    press(&mut core, Key::Tab);
    assert_eq!(core.view().focus, Focus::Band);
    assert_eq!(action_of(&core, "N"), Some("folders"));
}

#[test]
fn a_hidden_band_reads_preloaded_files_in_every_area() {
    for area in [Focus::Image, Focus::Dirs] {
        let mut core = focused(area);
        core.handle(Event::SettingsLoaded(Ok(Settings {
            band_hidden: true,
            ..Settings::default()
        })));
        assert_eq!(action_of(&core, "B"), Some("preloaded files"), "{area:?}");
    }
}

#[test]
fn only_the_band_is_remembered_at_the_next_start() {
    let mut core = browsing();
    press(&mut core, Key::N);
    press(&mut core, Key::I);
    press(&mut core, Key::P);
    press(&mut core, Key::B);
    let saved = press(&mut core, Key::B);
    assert_eq!(
        saved,
        [Command::SaveSettings(Settings {
            band_hidden: true,
            ..Settings::default()
        })]
    );
    // A new start with what was saved.
    let mut core = browsing();
    core.handle(Event::SettingsLoaded(Ok(Settings {
        band_hidden: true,
        ..Settings::default()
    })));
    assert!(core.view().settings.band_hidden);
    assert_eq!(core.view().dir_column, None);
    assert!(core.view().metadata.is_none());
    assert!(core.view().toolbar.is_none());
}
