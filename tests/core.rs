//! Seam 1: the app core, driven by events.

use std::path::PathBuf;
use std::sync::Arc;

use pixpixpix::core::{Command, Core, Event, Key, KeyPress, Notice, Screen};
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

fn press(core: &mut Core, key: Key) -> Vec<Command> {
    core.handle(Event::Key(KeyPress::plain(key)))
}

/// Started on `path`, first frame shown, folder listed as `files`.
fn browsing(path: &str, files: &[&str]) -> Core {
    let mut core = started(path);
    let commands = core.handle(Event::FirstFrameShown);
    assert_eq!(commands, [Command::ListFolder(PathBuf::from("/p"))]);
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(files.iter().map(PathBuf::from).collect()),
    });
    core
}

#[test]
fn folder_is_listed_only_after_the_first_frame() {
    let (mut core, commands) = Core::start(Ok(PathBuf::from("/p/a.jpg")));
    assert!(!commands.contains(&Command::ListFolder(PathBuf::from("/p"))));
    core.handle(Event::Decoded {
        path: PathBuf::from("/p/a.jpg"),
        result: Ok(image()),
    });
    assert_eq!(
        core.handle(Event::FirstFrameShown),
        [Command::ListFolder(PathBuf::from("/p"))]
    );
}

#[test]
fn nothing_to_open_lists_no_folder() {
    let (mut core, _) = Core::start(Err("No images in /x".into()));
    assert!(core.handle(Event::FirstFrameShown).is_empty());
}

#[test]
fn right_and_left_step_through_the_listed_order() {
    let mut core = browsing(
        "/p/img2.jpg",
        &["/p/img1.jpg", "/p/img2.jpg", "/p/img10.jpg"],
    );
    assert_eq!(
        press(&mut core, Key::Right),
        [Command::Decode(PathBuf::from("/p/img10.jpg"))]
    );
    assert_eq!(
        core.view().screen,
        Screen::Loading {
            path: PathBuf::from("/p/img10.jpg")
        }
    );
    assert_eq!(
        press(&mut core, Key::Left),
        [Command::Decode(PathBuf::from("/p/img2.jpg"))]
    );
    assert_eq!(
        press(&mut core, Key::Left),
        [Command::Decode(PathBuf::from("/p/img1.jpg"))]
    );
    assert_eq!(core.view().key_line.notice, None);
}

#[test]
fn right_on_the_last_image_stays_and_says_so() {
    let mut core = browsing("/p/b.jpg", &["/p/a.jpg", "/p/b.jpg"]);
    core.handle(Event::Decoded {
        path: PathBuf::from("/p/b.jpg"),
        result: Ok(image()),
    });
    assert!(press(&mut core, Key::Right).is_empty());
    assert_eq!(core.view().key_line.notice, Some(Notice::LastImage));
    assert!(matches!(core.view().screen, Screen::Image { .. }));
}

#[test]
fn left_on_the_first_image_stays_and_says_so() {
    let mut core = browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg"]);
    assert!(press(&mut core, Key::Left).is_empty());
    assert_eq!(core.view().key_line.notice, Some(Notice::FirstImage));
}

#[test]
fn moving_away_from_an_end_clears_the_notice() {
    let mut core = browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg"]);
    press(&mut core, Key::Left);
    press(&mut core, Key::Right);
    assert_eq!(core.view().key_line.notice, None);
}

#[test]
fn arrows_do_nothing_before_the_folder_is_listed() {
    let mut core = started("/p/a.jpg");
    assert!(press(&mut core, Key::Right).is_empty());
    assert!(press(&mut core, Key::Left).is_empty());
    assert_eq!(core.view().key_line.notice, None);
}

#[test]
fn decode_of_the_new_current_file_is_shown() {
    let mut core = browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg"]);
    press(&mut core, Key::Right);
    core.handle(Event::Decoded {
        path: PathBuf::from("/p/b.jpg"),
        result: Ok(image()),
    });
    assert_eq!(
        core.view().screen,
        Screen::Image {
            path: PathBuf::from("/p/b.jpg"),
            image: image()
        }
    );
}

#[test]
fn key_line_lists_the_image_view_keys() {
    let core = started("/p/a.jpg");
    let keys: Vec<&str> = core.view().key_line.keys.iter().map(|k| k.keys).collect();
    let quit = if cfg!(target_os = "macos") {
        "Cmd+Q"
    } else {
        "Ctrl+Q"
    };
    assert_eq!(keys, ["Left/Right", "F", quit]);
}
