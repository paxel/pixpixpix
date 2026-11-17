//! Seam 1: the About page on `F1`, over whatever shows, and from the
//! settings screen.

use std::path::PathBuf;
use std::sync::Arc;

use pixpixpix::core::{BinItem, Command, Core, Event, Focus, Key, KeyPress};
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

fn key_names(core: &Core) -> Vec<&'static str> {
    core.view().key_line.keys.iter().map(|k| k.keys).collect()
}

fn action_of(core: &Core, keys: &str) -> Option<&'static str> {
    core.view()
        .key_line
        .keys
        .iter()
        .find(|k| k.keys == keys)
        .map(|k| k.action)
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

/// The screens `s` works on, by name: each core is ready for `F1`.
fn screens() -> Vec<(&'static str, Core)> {
    let mut column = browsing();
    press(&mut column, Key::N);
    let mut band = browsing();
    press(&mut band, Key::B);
    let mut overview = browsing();
    press(&mut overview, Key::G);
    let mut bin = browsing();
    press(&mut bin, Key::T);
    bin.handle(Event::BinListed(Ok(vec![BinItem {
        id: "a".into(),
        file: PathBuf::from("/trash/a.jpg"),
        original: Some(PathBuf::from("/p/a.jpg")),
        deleted: 0,
    }])));
    vec![
        ("image view", browsing()),
        ("dir column", column),
        ("band", band),
        ("overview", overview),
        ("bin view", bin),
    ]
}

#[test]
fn f1_opens_the_about_page_from_every_screen_and_looks_for_the_libraries() {
    for (name, mut core) in screens() {
        assert_eq!(action_of(&core, "F1"), Some("about"), "{name}");
        assert_eq!(action_of(&core, "S"), Some("settings"), "{name}");
        assert_eq!(
            press(&mut core, Key::F1),
            [Command::ProbeLibraries],
            "{name}"
        );
        assert!(core.view().about.is_some(), "{name}");
        // Without the settings screen behind it.
        assert!(!core.view().settings_open, "{name}");
        assert_eq!(key_names(&core).first(), Some(&"Esc/F1"), "{name}");
        assert_eq!(action_of(&core, "Esc/F1"), Some("close"), "{name}");
        assert_eq!(key_names(&core).len(), 2, "{name}");
    }
}

#[test]
fn esc_and_f1_close_it_and_the_screen_and_focus_are_back() {
    for closing in [Key::Escape, Key::F1] {
        for (name, mut core) in screens() {
            let focus = core.view().focus;
            let keys = key_names(&core);
            let overview = core.view().overview.clone();
            let column = core.view().dir_column.clone();
            press(&mut core, Key::F1);
            assert!(press(&mut core, closing).is_empty(), "{name}");
            assert!(core.view().about.is_none(), "{name} {closing:?}");
            assert_eq!(core.view().focus, focus, "{name} {closing:?}");
            assert_eq!(key_names(&core), keys, "{name} {closing:?}");
            assert_eq!(core.view().overview, overview, "{name} {closing:?}");
            assert_eq!(core.view().dir_column, column, "{name} {closing:?}");
        }
    }
}

#[test]
fn the_open_page_takes_every_key_but_quit() {
    let mut core = browsing();
    press(&mut core, Key::F1);
    let shown = core.view().screen.clone();
    for key in [Key::Right, Key::N, Key::B, Key::G, Key::S, Key::X, Key::I] {
        assert!(press(&mut core, key).is_empty(), "{key:?}");
    }
    assert!(core.view().about.is_some());
    assert_eq!(core.view().screen, shown);
    assert_eq!(core.view().focus, Focus::Image);
    assert!(core.view().dir_column.is_none());
    assert!(!core.view().settings_open);
    assert_eq!(
        core.handle(Event::Key(KeyPress::command(Key::Q))),
        [Command::Quit]
    );
}

#[test]
fn f1_in_the_settings_opens_about_and_esc_goes_back_to_them() {
    let mut core = browsing();
    press(&mut core, Key::S);
    assert_eq!(action_of(&core, "F1"), Some("about"));
    assert_eq!(press(&mut core, Key::F1), [Command::ProbeLibraries]);
    assert!(core.view().about.is_some());
    assert!(core.view().settings_open);
    assert_eq!(action_of(&core, "Esc"), Some("back"));
    press(&mut core, Key::Escape);
    assert!(core.view().about.is_none());
    assert!(core.view().settings_open);
    assert_eq!(key_names(&core).first(), Some(&"S/Esc"));
}

#[test]
fn the_back_button_closes_the_page_opened_by_f1() {
    let mut core = browsing();
    press(&mut core, Key::F1);
    core.handle(Event::CloseAbout);
    assert!(core.view().about.is_none());
    assert_eq!(action_of(&core, "+/-"), Some("zoom"));
}

#[test]
fn the_libraries_found_show_on_the_page_opened_by_f1() {
    let mut core = browsing();
    press(&mut core, Key::F1);
    core.handle(Event::LibrariesProbed(Vec::new()));
    assert_eq!(
        core.view().about.as_ref().and_then(|a| a.libraries.clone()),
        Some(Vec::new())
    );
}
