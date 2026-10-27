//! Seam 1: the app core's `o`: handing the current file to another app.

use std::path::PathBuf;
use std::sync::Arc;

use pixpixpix::core::{Command, Core, Event, Key, KeyPress};
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

/// Showing `a.jpg` of a folder with `a.jpg` and `b.jpg`.
fn showing() -> Core {
    let path = PathBuf::from("/p/a.jpg");
    let (mut core, _) = Core::start(Ok(path.clone()));
    core.handle(Event::Decoded {
        path: path.clone(),
        result: Ok(image()),
    });
    core.handle(Event::FirstFrameShown);
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(vec![path, PathBuf::from("/p/b.jpg")]),
    });
    core
}

fn with_apps(core: &mut Core, image_app: &str, video_app: &str) {
    core.handle(Event::SettingsLoaded(Ok(Settings {
        image_app: image_app.into(),
        video_app: video_app.into(),
        ..Settings::default()
    })));
}

fn open(path: &str, app: Option<&str>) -> Command {
    Command::OpenExternally {
        path: PathBuf::from(path),
        app: app.map(str::to_owned),
        earlier: None,
    }
}

#[test]
fn o_opens_the_current_file_with_the_system_default() {
    let mut core = showing();
    assert_eq!(press(&mut core, Key::O), [open("/p/a.jpg", None)]);
}

#[test]
fn o_uses_the_image_app_for_images_and_the_video_app_for_videos() {
    let mut core = showing();
    with_apps(&mut core, "gwenview", "vlc --fullscreen");
    assert_eq!(
        press(&mut core, Key::O),
        [open("/p/a.jpg", Some("gwenview"))]
    );

    let (mut core, _) = Core::start(Ok(PathBuf::from("/p/clip.MP4")));
    with_apps(&mut core, "gwenview", "vlc --fullscreen");
    assert_eq!(
        press(&mut core, Key::O),
        [open("/p/clip.MP4", Some("vlc --fullscreen"))]
    );
}

#[test]
fn a_blank_app_means_the_system_default() {
    let mut core = showing();
    with_apps(&mut core, "  ", "vlc");
    assert_eq!(press(&mut core, Key::O), [open("/p/a.jpg", None)]);
}

#[test]
fn o_opens_a_broken_file_too() {
    let (mut core, _) = Core::start(Ok(PathBuf::from("/p/bad.jpg")));
    core.handle(Event::Decoded {
        path: PathBuf::from("/p/bad.jpg"),
        result: Err("unexpected end of file".into()),
    });
    assert_eq!(press(&mut core, Key::O), [open("/p/bad.jpg", None)]);
}

#[test]
fn o_without_a_file_does_nothing() {
    let (mut core, _) = Core::start(Err(pixpixpix::startup::NoFile {
        dir: PathBuf::from("/p"),
        message: "No images in /p".into(),
    }));
    assert!(press(&mut core, Key::O).is_empty());
}

#[test]
fn o_belongs_to_the_image_view() {
    let mut core = showing();
    press(&mut core, Key::B);
    assert!(press(&mut core, Key::O).is_empty());
    press(&mut core, Key::Escape);
    press(&mut core, Key::S);
    assert!(press(&mut core, Key::O).is_empty());
}

#[test]
fn a_failure_is_shown_until_the_next_o_or_file() {
    let mut core = showing();
    press(&mut core, Key::O);
    let commands = core.handle(Event::OpenFailed("Cannot start nope: not found".into()));
    assert!(commands.is_empty());
    assert_eq!(
        core.view().open_error.as_deref(),
        Some("Cannot start nope: not found")
    );
    press(&mut core, Key::O);
    assert_eq!(core.view().open_error, None);

    core.handle(Event::OpenFailed("gwenview failed".into()));
    core.handle(Event::Decoded {
        path: PathBuf::from("/p/b.jpg"),
        result: Ok(image()),
    });
    press(&mut core, Key::Right);
    assert_eq!(core.view().open_error, None);
}

#[test]
fn changing_an_app_only_saves_the_settings() {
    let mut core = showing();
    let changed = Settings {
        image_app: "gwenview".into(),
        ..Settings::default()
    };
    assert_eq!(
        core.handle(Event::SettingsChanged(changed.clone())),
        [Command::SaveSettings(changed)]
    );
}

#[test]
fn o_hands_over_the_app_pixpixpix_replaced_for_the_type() {
    let mut core = showing();
    core.handle(Event::SettingsLoaded(Ok(Settings {
        earlier_defaults: [("JPEG".to_owned(), "gwenview.desktop".to_owned())].into(),
        ..Settings::default()
    })));
    assert_eq!(
        press(&mut core, Key::O),
        [Command::OpenExternally {
            path: PathBuf::from("/p/a.jpg"),
            app: None,
            earlier: Some("gwenview.desktop".into()),
        }]
    );
}
