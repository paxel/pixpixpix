//! Seam 1: the app core, driven by events.

use std::path::PathBuf;
use std::sync::Arc;

use pixpixpix::core::{Command, Core, Event, Key, KeyPress, Screen};
use pixpixpix::decode::DecodedImage;

fn image() -> DecodedImage {
    DecodedImage {
        width: 1,
        height: 1,
        rgba: Arc::from(vec![1, 2, 3, 255]),
    }
}

fn started(path: &str) -> Core {
    let (core, commands) = Core::start(Ok(PathBuf::from(path)));
    assert_eq!(commands, [Command::Decode(PathBuf::from(path))]);
    core
}

#[test]
fn start_decodes_the_file_and_waits_for_it() {
    let core = started("/p/a.jpg");
    assert_eq!(
        core.view().screen,
        Screen::Loading {
            path: PathBuf::from("/p/a.jpg")
        }
    );
}

#[test]
fn decoded_image_is_shown() {
    let mut core = started("/p/a.jpg");
    core.handle(Event::Decoded {
        path: PathBuf::from("/p/a.jpg"),
        result: Ok(image()),
    });
    assert_eq!(
        core.view().screen,
        Screen::Image {
            path: PathBuf::from("/p/a.jpg"),
            image: image()
        }
    );
}

#[test]
fn decode_error_is_shown_with_its_message() {
    let mut core = started("/p/a.jpg");
    core.handle(Event::Decoded {
        path: PathBuf::from("/p/a.jpg"),
        result: Err("broken".into()),
    });
    assert_eq!(
        core.view().screen,
        Screen::Error {
            path: Some(PathBuf::from("/p/a.jpg")),
            message: "broken".into()
        }
    );
}

#[test]
fn decode_of_another_file_is_ignored() {
    let mut core = started("/p/a.jpg");
    core.handle(Event::Decoded {
        path: PathBuf::from("/p/b.jpg"),
        result: Ok(image()),
    });
    assert!(matches!(core.view().screen, Screen::Loading { .. }));
}

#[test]
fn nothing_to_open_shows_the_reason_and_decodes_nothing() {
    let (core, commands) = Core::start(Err("No images in /x".into()));
    assert!(commands.is_empty());
    assert_eq!(
        core.view().screen,
        Screen::Error {
            path: None,
            message: "No images in /x".into()
        }
    );
}

#[test]
fn f_toggles_fullscreen() {
    let mut core = started("/p/a.jpg");
    let on = core.handle(Event::Key(KeyPress::plain(Key::F)));
    assert_eq!(on, [Command::SetFullscreen(true)]);
    assert!(core.view().fullscreen);
    let off = core.handle(Event::Key(KeyPress::plain(Key::F)));
    assert_eq!(off, [Command::SetFullscreen(false)]);
    assert!(!core.view().fullscreen);
}

#[test]
fn command_q_quits() {
    let mut core = started("/p/a.jpg");
    assert_eq!(
        core.handle(Event::Key(KeyPress::command(Key::Q))),
        [Command::Quit]
    );
}

#[test]
fn plain_q_and_escape_do_not_quit() {
    let mut core = started("/p/a.jpg");
    assert!(core.handle(Event::Key(KeyPress::plain(Key::Q))).is_empty());
    assert!(
        core.handle(Event::Key(KeyPress::plain(Key::Escape)))
            .is_empty()
    );
}

#[test]
fn first_frame_is_recorded() {
    let mut core = started("/p/a.jpg");
    assert!(!core.first_frame_shown());
    core.handle(Event::FirstFrameShown);
    assert!(core.first_frame_shown());
}
