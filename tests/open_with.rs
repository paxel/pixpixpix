//! Seam 1: the app core opening a file the system asks for, as Finder's
//! "Open With" does on macOS.

use std::path::PathBuf;
use std::sync::Arc;

use pixpixpix::core::{Command, Core, Event, Key, KeyPress, Points, Screen};
use pixpixpix::decode::DecodedImage;
use pixpixpix::startup::NoFile;

fn image() -> DecodedImage {
    DecodedImage {
        width: 100,
        height: 50,
        rgba: Arc::from(vec![0; 100 * 50 * 4]),
    }
}

fn p(name: &str) -> PathBuf {
    PathBuf::from(name)
}

fn decoded(core: &mut Core, path: &str) -> Vec<Command> {
    core.handle(Event::Decoded {
        path: p(path),
        result: Ok(image()),
    })
}

fn shown(core: &Core) -> Option<PathBuf> {
    match &core.view().screen {
        Screen::Image { path, .. } => Some(path.clone()),
        _ => None,
    }
}

/// Showing `/p/a.jpg` of `/p/a.jpg` and `/p/b.jpg`.
fn browsing() -> Core {
    let (mut core, _) = Core::start(Ok(p("/p/a.jpg")));
    decoded(&mut core, "/p/a.jpg");
    core.handle(Event::FirstFrameShown);
    core.handle(Event::FolderListed {
        dir: p("/p"),
        result: Ok(vec![p("/p/a.jpg"), p("/p/b.jpg")]),
    });
    decoded(&mut core, "/p/b.jpg");
    core
}

#[test]
fn a_file_asked_for_at_launch_is_the_one_shown_and_its_folder_read() {
    // Started by Finder without a file, in `/`.
    let (mut core, _) = Core::start(Err(NoFile {
        dir: p("/"),
        message: "No images in /".into(),
    }));
    assert_eq!(
        core.handle(Event::Opened(p("/q/c.jpg"))),
        [Command::Decode(p("/q/c.jpg"))]
    );
    decoded(&mut core, "/q/c.jpg");
    assert_eq!(shown(&core), Some(p("/q/c.jpg")));
    let commands = core.handle(Event::FirstFrameShown);
    assert!(commands.contains(&Command::ListFolder(p("/q"))));
    assert!(!commands.contains(&Command::ListFolder(p("/"))));
}

#[test]
fn a_file_asked_for_later_moves_to_its_folder() {
    let mut core = browsing();
    let commands = core.handle(Event::Opened(p("/q/c.jpg")));
    assert!(commands.contains(&Command::Decode(p("/q/c.jpg"))));
    assert!(commands.contains(&Command::ListFolder(p("/q"))));
    assert!(commands.contains(&Command::WatchFolder(p("/q"))));
    decoded(&mut core, "/q/c.jpg");
    assert_eq!(shown(&core), Some(p("/q/c.jpg")));
}

#[test]
fn a_file_of_the_same_folder_is_just_shown() {
    let mut core = browsing();
    assert!(core.handle(Event::Opened(p("/p/b.jpg"))).is_empty());
    assert_eq!(shown(&core), Some(p("/p/b.jpg")));
}

#[test]
fn the_overview_gives_way_to_the_file() {
    let mut core = browsing();
    core.handle(Event::Key(KeyPress::plain(Key::G)));
    assert!(core.view().overview.is_some());
    core.handle(Event::Opened(p("/p/b.jpg")));
    assert!(core.view().overview.is_none());
    assert_eq!(shown(&core), Some(p("/p/b.jpg")));
}

#[test]
fn unsaved_edits_are_asked_about_first() {
    let mut core = browsing();
    core.handle(Event::Viewport(Points { x: 100.0, y: 50.0 }));
    core.handle(Event::Key(KeyPress::plain(Key::P)));
    core.handle(Event::DrawStart(Points { x: 10.0, y: 10.0 }));
    core.handle(Event::DrawMove(Points { x: 60.0, y: 30.0 }));
    core.handle(Event::DrawEnd);
    assert!(core.handle(Event::Opened(p("/p/b.jpg"))).is_empty());
    assert!(core.view().save_dialog.is_some());
    assert_eq!(shown(&core), Some(p("/p/a.jpg")));
    core.handle(Event::SaveChoice(pixpixpix::core::SaveChoice::Discard));
    assert_eq!(shown(&core), Some(p("/p/b.jpg")));
}
