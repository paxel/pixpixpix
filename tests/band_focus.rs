//! Seam 1: the app core's band focus, `b`, and `Tab` cycling the areas.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pixpixpix::core::{Command, Core, Event, Focus, Key, KeyPress, Screen};
use pixpixpix::decode::DecodedImage;
use pixpixpix::settings::Settings;

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

fn shows_image(core: &Core, path: &str) -> bool {
    matches!(&core.view().screen, Screen::Image { path: p, .. } if p == Path::new(path))
}

/// The path of the highlighted band thumb.
fn highlighted(core: &Core) -> Option<&Path> {
    let view = core.view();
    view.band_highlight
        .and_then(|i| view.band.get(i))
        .map(|t| t.path.as_path())
}

fn key_names(core: &Core) -> Vec<&'static str> {
    core.view().key_line.keys.iter().map(|k| k.keys).collect()
}

/// Showing `/p/b.jpg` of `/p/a.jpg` … `/p/d.jpg`, all decoded.
fn browsing() -> Core {
    let (mut core, _) = Core::start(Ok(PathBuf::from("/p/b.jpg")));
    decoded(&mut core, "/p/b.jpg");
    core.handle(Event::FirstFrameShown);
    files_listed(
        &mut core,
        "/p",
        &["/p/a.jpg", "/p/b.jpg", "/p/c.jpg", "/p/d.jpg"],
    );
    for path in ["/p/a.jpg", "/p/c.jpg", "/p/d.jpg"] {
        decoded(&mut core, path);
    }
    core
}

/// [`browsing`] with the column open on `/p` listing `x` and `y`.
fn column_open() -> Core {
    let mut core = browsing();
    press(&mut core, Key::N);
    core.handle(Event::DirsListed {
        dir: PathBuf::from("/p"),
        result: Ok(paths(&["/p/x", "/p/y"])),
    });
    core
}

#[test]
fn b_focuses_the_band_with_the_current_file_highlighted() {
    let mut core = browsing();
    assert_eq!(highlighted(&core), None);
    assert!(press(&mut core, Key::B).is_empty());
    assert_eq!(core.view().focus, Focus::Band);
    assert_eq!(highlighted(&core), Some(Path::new("/p/b.jpg")));
}

#[test]
fn left_and_right_move_the_highlight_not_the_image_and_stop_at_the_ends() {
    let mut core = browsing();
    press(&mut core, Key::B);
    press(&mut core, Key::Right);
    assert_eq!(highlighted(&core), Some(Path::new("/p/c.jpg")));
    assert!(shows_image(&core, "/p/b.jpg"));
    press(&mut core, Key::Right);
    press(&mut core, Key::Right);
    assert_eq!(highlighted(&core), Some(Path::new("/p/d.jpg")));
    for _ in 0..4 {
        press(&mut core, Key::Left);
    }
    assert_eq!(highlighted(&core), Some(Path::new("/p/a.jpg")));
    assert_eq!(core.view().key_line.notice, None);
}

#[test]
fn enter_shows_the_highlighted_file_and_returns_to_the_image_view() {
    let mut core = browsing();
    press(&mut core, Key::B);
    press(&mut core, Key::Right);
    press(&mut core, Key::Right);
    press(&mut core, Key::Enter);
    assert!(shows_image(&core, "/p/d.jpg"));
    assert_eq!(core.view().focus, Focus::Image);
    assert_eq!(highlighted(&core), None);
    // The arrows step through the folder again.
    press(&mut core, Key::Left);
    assert!(shows_image(&core, "/p/c.jpg"));
}

#[test]
fn focusing_the_band_again_starts_at_the_current_file() {
    let mut core = browsing();
    press(&mut core, Key::B);
    press(&mut core, Key::Right);
    press(&mut core, Key::Escape);
    assert_eq!(core.view().focus, Focus::Image);
    press(&mut core, Key::B);
    assert_eq!(highlighted(&core), Some(Path::new("/p/b.jpg")));
}

#[test]
fn the_highlight_falls_back_to_the_current_file_when_its_file_leaves() {
    let mut core = browsing();
    press(&mut core, Key::B);
    press(&mut core, Key::Right);
    files_listed(&mut core, "/p", &["/p/a.jpg", "/p/b.jpg", "/p/d.jpg"]);
    assert_eq!(highlighted(&core), Some(Path::new("/p/b.jpg")));
}

#[test]
fn tab_cycles_image_view_and_band_while_the_column_is_closed() {
    let mut core = browsing();
    press(&mut core, Key::Tab);
    assert_eq!(core.view().focus, Focus::Band);
    press(&mut core, Key::Tab);
    assert_eq!(core.view().focus, Focus::Image);
}

#[test]
fn tab_cycles_column_image_view_and_band() {
    let mut core = column_open();
    assert_eq!(core.view().focus, Focus::Dirs);
    press(&mut core, Key::Tab);
    assert_eq!(core.view().focus, Focus::Image);
    press(&mut core, Key::Tab);
    assert_eq!(core.view().focus, Focus::Band);
    // Reached from the image view, the band is the current folder's.
    assert_eq!(highlighted(&core), Some(Path::new("/p/b.jpg")));
    press(&mut core, Key::Tab);
    assert_eq!(core.view().focus, Focus::Dirs);
}

#[test]
fn escape_returns_to_the_image_view_keeping_the_file_and_the_band() {
    for from_column in [true, false] {
        let mut core = column_open();
        if !from_column {
            press(&mut core, Key::Tab);
        }
        let shown = core.view().screen.clone();
        press(&mut core, Key::B);
        press(&mut core, Key::Right);
        press(&mut core, Key::Escape);
        assert_eq!(core.view().focus, Focus::Image, "{from_column}");
        assert_eq!(core.view().screen, shown, "{from_column}");
        assert_eq!(core.view().band_highlight, None, "{from_column}");
        assert!(!core.view().band.is_empty(), "{from_column}");
    }
}

#[test]
fn the_key_line_lists_the_band_keys_while_it_has_the_focus() {
    let quit = if cfg!(target_os = "macos") {
        "Cmd+Q"
    } else {
        "Ctrl+Q"
    };
    let mut core = browsing();
    press(&mut core, Key::B);
    assert_eq!(
        key_names(&core),
        [
            "Left/Right",
            "Enter",
            "Esc",
            "B",
            "Tab",
            "S",
            "F",
            quit,
            "?"
        ]
    );
    assert_eq!(core.view().key_line.keys[3].action, "hide preloaded files");
    assert_eq!(core.view().key_line.keys[4].action, "image view");
    let mut core = column_open();
    press(&mut core, Key::B);
    assert_eq!(core.view().key_line.keys[4].action, "folders");
}

fn action_of(core: &Core, keys: &str) -> Option<&'static str> {
    core.view()
        .key_line
        .keys
        .iter()
        .find(|k| k.keys == keys)
        .map(|k| k.action)
}

fn band_hidden(hidden: bool) -> Command {
    Command::SaveSettings(Settings {
        band_hidden: hidden,
        ..Settings::default()
    })
}

#[test]
fn b_in_the_band_hides_it_and_b_again_shows_and_focuses_it() {
    let mut core = browsing();
    press(&mut core, Key::B);
    assert_eq!(press(&mut core, Key::B), [band_hidden(true)]);
    assert!(core.view().settings.band_hidden);
    assert_eq!(core.view().focus, Focus::Image);
    assert_eq!(action_of(&core, "B"), Some("show preloaded files"));
    assert_eq!(press(&mut core, Key::B), [band_hidden(false)]);
    assert!(!core.view().settings.band_hidden);
    assert_eq!(core.view().focus, Focus::Band);
}

#[test]
fn tab_skips_a_hidden_band() {
    let mut core = browsing();
    press(&mut core, Key::B);
    press(&mut core, Key::B);
    press(&mut core, Key::Tab);
    assert_eq!(core.view().focus, Focus::Image);
    assert_eq!(action_of(&core, "Tab"), None);
    let mut core = column_open();
    core.handle(Event::SettingsLoaded(Ok(Settings {
        band_hidden: true,
        ..Settings::default()
    })));
    press(&mut core, Key::Tab);
    assert_eq!(core.view().focus, Focus::Image);
    assert_eq!(action_of(&core, "Tab"), Some("folders"));
    press(&mut core, Key::Tab);
    assert_eq!(core.view().focus, Focus::Dirs);
}

#[test]
fn a_band_hidden_before_stays_hidden_after_a_restart() {
    let mut core = browsing();
    core.handle(Event::SettingsLoaded(Ok(Settings {
        band_hidden: true,
        ..Settings::default()
    })));
    assert!(core.view().settings.band_hidden);
    assert_eq!(action_of(&core, "B"), Some("show preloaded files"));
}
