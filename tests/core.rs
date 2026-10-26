//! Seam 1: the app core, driven by events.

use std::path::PathBuf;
use std::sync::Arc;

use pixpixpix::core::{Command, Core, Event, Key, KeyPress, Notice, Screen, Thumb};
use pixpixpix::decode::DecodedImage;
use pixpixpix::folder;
use pixpixpix::startup::NoFile;

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
    let (core, commands) = Core::start(Err(NoFile {
        dir: PathBuf::from("/x"),
        message: "No images in /x".into(),
    }));
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
    assert_eq!(
        commands,
        [
            Command::LoadSettings,
            Command::WatchFolder(PathBuf::from("/p")),
            Command::ListFolder(PathBuf::from("/p"))
        ]
    );
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(files.iter().map(PathBuf::from).collect()),
    });
    core
}

#[test]
fn folder_is_listed_and_watched_only_after_the_first_frame() {
    let (mut core, commands) = Core::start(Ok(PathBuf::from("/p/a.jpg")));
    assert_eq!(commands, [Command::Decode(PathBuf::from("/p/a.jpg"))]);
    core.handle(Event::Decoded {
        path: PathBuf::from("/p/a.jpg"),
        result: Ok(image()),
    });
    assert_eq!(
        core.handle(Event::FirstFrameShown),
        [
            Command::LoadSettings,
            Command::WatchFolder(PathBuf::from("/p")),
            Command::ListFolder(PathBuf::from("/p")),
            Command::FindMotion(PathBuf::from("/p/a.jpg"))
        ]
    );
}

#[test]
fn a_folder_with_nothing_to_open_is_watched_and_listed_after_the_first_frame() {
    let (mut core, _) = Core::start(Err(NoFile {
        dir: PathBuf::from("/x"),
        message: "No images in /x".into(),
    }));
    assert_eq!(
        core.handle(Event::FirstFrameShown),
        [
            Command::LoadSettings,
            Command::WatchFolder(PathBuf::from("/x")),
            Command::ListFolder(PathBuf::from("/x"))
        ]
    );
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
    // img2 is still being decoded from the start: no second decode.
    assert!(press(&mut core, Key::Left).is_empty());
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
    assert_eq!(
        keys,
        [
            "Left/Right",
            "+/-",
            "0",
            "1",
            "L/R",
            "Enter",
            "C",
            "O",
            "X",
            "G",
            "N",
            "B",
            "Tab",
            "S",
            "F",
            quit
        ]
    );
}

fn p(name: &str) -> PathBuf {
    PathBuf::from(format!("/p/{name}"))
}

fn decodes(paths: &[&str]) -> Vec<Command> {
    paths.iter().map(|n| Command::Decode(p(n))).collect()
}

fn decoded(core: &mut Core, name: &str) -> Vec<Command> {
    core.handle(Event::Decoded {
        path: p(name),
        result: Ok(image()),
    })
}

/// Fifteen files `f00.jpg` … `f14.jpg`.
fn fifteen() -> Vec<String> {
    (0..15).map(|i| format!("/p/f{i:02}.jpg")).collect()
}

fn browsing_fifteen(start: &str) -> Core {
    let files = fifteen();
    let names: Vec<&str> = files.iter().map(String::as_str).collect();
    browsing(&format!("/p/{start}"), &names)
}

fn band(core: &Core) -> Vec<(String, bool)> {
    core.view()
        .band
        .iter()
        .map(|t| (folder::file_name(&t.path), t.current))
        .collect()
}

#[test]
fn neighbours_are_decoded_only_after_the_current_image_is_shown() {
    let mut core = browsing_fifteen("f07.jpg");
    assert_eq!(
        decoded(&mut core, "f07.jpg"),
        [
            decodes(&[
                "f08.jpg", "f06.jpg", "f09.jpg", "f05.jpg", "f10.jpg", "f04.jpg", "f11.jpg",
                "f03.jpg", "f12.jpg", "f02.jpg"
            ]),
            vec![Command::FindMotion(p("f07.jpg"))]
        ]
        .concat()
    );
}

#[test]
fn listing_after_the_image_is_shown_asks_for_its_neighbours() {
    let mut core = started("/p/b.jpg");
    decoded(&mut core, "b.jpg");
    core.handle(Event::FirstFrameShown);
    let commands = core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(vec![p("a.jpg"), p("b.jpg"), p("c.jpg")]),
    });
    assert_eq!(commands, decodes(&["c.jpg", "a.jpg"]));
}

#[test]
fn no_neighbours_before_the_first_frame() {
    let mut core = started("/p/a.jpg");
    assert!(decoded(&mut core, "a.jpg").is_empty());
}

#[test]
fn neighbours_stop_at_the_folder_ends() {
    let mut core = browsing_fifteen("f01.jpg");
    assert_eq!(
        decoded(&mut core, "f01.jpg"),
        [
            decodes(&[
                "f02.jpg", "f00.jpg", "f03.jpg", "f04.jpg", "f05.jpg", "f06.jpg"
            ]),
            vec![Command::FindMotion(p("f01.jpg"))]
        ]
        .concat()
    );
}

#[test]
fn a_broken_current_file_still_preloads_its_neighbours() {
    let mut core = browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg"]);
    let commands = core.handle(Event::Decoded {
        path: p("a.jpg"),
        result: Err("broken".into()),
    });
    assert_eq!(commands, decodes(&["b.jpg"]));
}

#[test]
fn stepping_to_a_cached_image_shows_it_without_decoding() {
    let mut core = browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg"]);
    decoded(&mut core, "a.jpg");
    decoded(&mut core, "b.jpg");
    assert!(press(&mut core, Key::Right).is_empty());
    assert_eq!(
        core.view().screen,
        Screen::Image {
            path: p("b.jpg"),
            image: image()
        }
    );
    assert!(press(&mut core, Key::Left).is_empty());
    assert!(matches!(core.view().screen, Screen::Image { .. }));
}

#[test]
fn stepping_to_a_broken_cached_file_shows_its_error_without_decoding() {
    let mut core = browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg"]);
    decoded(&mut core, "a.jpg");
    core.handle(Event::Decoded {
        path: p("b.jpg"),
        result: Err("broken".into()),
    });
    assert!(press(&mut core, Key::Right).is_empty());
    assert_eq!(
        core.view().screen,
        Screen::Error {
            path: Some(p("b.jpg")),
            message: "broken".into()
        }
    );
}

#[test]
fn stepping_to_an_image_still_decoding_waits_for_it() {
    let mut core = browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg"]);
    decoded(&mut core, "a.jpg");
    assert!(press(&mut core, Key::Right).is_empty());
    assert_eq!(core.view().screen, Screen::Loading { path: p("b.jpg") });
    decoded(&mut core, "b.jpg");
    assert!(matches!(core.view().screen, Screen::Image { .. }));
}

#[test]
fn shown_neighbour_asks_for_the_files_entering_the_window() {
    let mut core = browsing_fifteen("f07.jpg");
    decoded(&mut core, "f07.jpg");
    decoded(&mut core, "f08.jpg");
    let commands = press(&mut core, Key::Right);
    assert_eq!(
        commands,
        [
            Command::CancelDecode(p("f02.jpg")),
            Command::Decode(p("f13.jpg")),
        ]
    );
}

#[test]
fn decodes_leaving_the_window_are_cancelled_and_their_late_results_ignored() {
    let mut core = browsing_fifteen("f00.jpg");
    decoded(&mut core, "f00.jpg");
    let mut cancelled = Vec::new();
    for _ in 0..10 {
        for command in press(&mut core, Key::Right) {
            if let Command::CancelDecode(path) = command {
                cancelled.push(path);
            }
        }
    }
    // Now on f10, still waiting: f00 to f04 left the window undecoded.
    assert_eq!(
        cancelled,
        ["f01.jpg", "f02.jpg", "f03.jpg", "f04.jpg"].map(p)
    );
    assert!(decoded(&mut core, "f03.jpg").is_empty());
    assert!(core.view().band.iter().all(|t| t.path != p("f03.jpg")));
}

#[test]
fn images_leaving_the_window_are_dropped_from_the_cache() {
    let mut core = browsing_fifteen("f00.jpg");
    for name in [
        "f00.jpg", "f01.jpg", "f02.jpg", "f03.jpg", "f04.jpg", "f05.jpg",
    ] {
        decoded(&mut core, name);
    }
    for _ in 0..6 {
        press(&mut core, Key::Right);
    }
    // On f06: f00 left the window and is not in the band any more.
    let names: Vec<String> = band(&core).into_iter().map(|(n, _)| n).collect();
    assert_eq!(
        names,
        ["f01.jpg", "f02.jpg", "f03.jpg", "f04.jpg", "f05.jpg"]
    );
    // Going back has to decode f00 again.
    for _ in 0..6 {
        press(&mut core, Key::Left);
    }
    assert_eq!(core.view().screen, Screen::Loading { path: p("f00.jpg") });
}

#[test]
fn band_shows_the_cached_images_in_folder_order_with_the_current_one_marked() {
    let mut core = browsing("/p/b.jpg", &["/p/a.jpg", "/p/b.jpg", "/p/c.jpg"]);
    decoded(&mut core, "b.jpg");
    assert_eq!(band(&core), [("b.jpg".to_owned(), true)]);
    decoded(&mut core, "c.jpg");
    decoded(&mut core, "a.jpg");
    assert_eq!(
        band(&core),
        [
            ("a.jpg".to_owned(), false),
            ("b.jpg".to_owned(), true),
            ("c.jpg".to_owned(), false)
        ]
    );
    assert_eq!(
        core.view().band.first(),
        Some(&Thumb {
            path: p("a.jpg"),
            image: Some(image()),
            current: false,
            animated: false,
            images: 1,
            video: false,
            motion: None
        })
    );
}

#[test]
fn broken_files_are_in_the_band_without_an_image() {
    let mut core = browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg"]);
    decoded(&mut core, "a.jpg");
    core.handle(Event::Decoded {
        path: p("b.jpg"),
        result: Err("broken".into()),
    });
    assert_eq!(
        band(&core),
        [("a.jpg".to_owned(), true), ("b.jpg".to_owned(), false)]
    );
    assert_eq!(
        core.view().band.get(1),
        Some(&Thumb {
            path: p("b.jpg"),
            image: None,
            current: false,
            animated: false,
            images: 1,
            video: false,
            motion: None
        })
    );
}

#[test]
fn clicking_a_broken_band_thumb_shows_its_error() {
    let mut core = browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg"]);
    decoded(&mut core, "a.jpg");
    core.handle(Event::Decoded {
        path: p("b.jpg"),
        result: Err("broken".into()),
    });
    assert!(core.handle(Event::Show(p("b.jpg"))).is_empty());
    assert_eq!(
        core.view().screen,
        Screen::Error {
            path: Some(p("b.jpg")),
            message: "broken".into()
        }
    );
    assert_eq!(
        band(&core),
        [("a.jpg".to_owned(), false), ("b.jpg".to_owned(), true)]
    );
}

#[test]
fn an_unreadable_folder_is_reported_and_the_image_stays() {
    let mut core = started("/p/a.jpg");
    decoded(&mut core, "a.jpg");
    core.handle(Event::FirstFrameShown);
    let commands = core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Err("permission denied".into()),
    });
    assert!(commands.is_empty());
    assert_eq!(
        core.view().folder_error.as_deref(),
        Some("permission denied")
    );
    assert!(matches!(core.view().screen, Screen::Image { .. }));
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(vec![p("a.jpg")]),
    });
    assert_eq!(core.view().folder_error, None);
}

#[test]
fn clicking_a_band_thumb_shows_that_image() {
    let mut core = browsing_fifteen("f07.jpg");
    decoded(&mut core, "f07.jpg");
    decoded(&mut core, "f04.jpg");
    let commands = core.handle(Event::Show(p("f04.jpg")));
    assert_eq!(
        core.view().screen,
        Screen::Image {
            path: p("f04.jpg"),
            image: image()
        }
    );
    // The window moved three to the left.
    assert_eq!(
        commands,
        [
            Command::CancelDecode(p("f10.jpg")),
            Command::CancelDecode(p("f11.jpg")),
            Command::CancelDecode(p("f12.jpg")),
            Command::Decode(p("f01.jpg")),
            Command::Decode(p("f00.jpg")),
        ]
    );
    assert!(band(&core).contains(&("f04.jpg".to_owned(), true)));
}

#[test]
fn showing_an_image_that_is_not_cached_does_nothing() {
    let mut core = browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg"]);
    decoded(&mut core, "a.jpg");
    assert!(core.handle(Event::Show(p("b.jpg"))).is_empty());
    assert!(matches!(&core.view().screen, Screen::Image { path, .. } if *path == p("a.jpg")));
}
