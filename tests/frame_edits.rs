//! Seam 1: the app core's tools on a paused video frame, saved as an image.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use pixpixpix::core::{
    Command, Core, Event, Key, KeyPress, Points, SaveChoice, ToolbarClick, VideoStep,
};
use pixpixpix::decode::DecodedImage;

/// A 100x50 image of the gray `value`.
fn image(value: u8) -> DecodedImage {
    DecodedImage {
        width: 100,
        height: 50,
        rgba: Arc::from(vec![value; 100 * 50 * 4]),
    }
}

fn p(name: &str) -> PathBuf {
    PathBuf::from(format!("/p/{name}"))
}

fn press(core: &mut Core, key: Key) -> Vec<Command> {
    core.handle(Event::Key(KeyPress::plain(key)))
}

fn at(x: f32, y: f32) -> Points {
    Points { x, y }
}

/// Started on `start`, first frame shown, folder listed as `names`, every
/// file decoded gray 0, the image filling a viewport of its own size.
fn browsing(start: &str, names: &[&str]) -> Core {
    let (mut core, _) = Core::start(Ok(p(start)));
    core.handle(Event::FirstFrameShown);
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(names.iter().map(|n| p(n)).collect()),
    });
    for name in names {
        core.handle(Event::Decoded {
            path: p(name),
            result: Ok(image(0)),
        });
    }
    core.handle(Event::Viewport(at(100.0, 50.0)));
    core
}

/// Drags from (10, 10) to (60, 30) on the image.
fn drag(core: &mut Core) {
    core.handle(Event::DrawStart(at(10.0, 10.0)));
    core.handle(Event::DrawMove(at(60.0, 30.0)));
    core.handle(Event::DrawEnd);
}

fn shapes(core: &Core) -> usize {
    core.view().edits.as_ref().map_or(0, |e| e.shapes.len())
}

/// Steps `clip.mp4` one frame on to the frame of the gray `value` at
/// `position`, paused there.
fn step_to(core: &mut Core, value: u8, position: Duration) {
    core.handle(Event::StepVideo(VideoStep::Frames(1)));
    core.handle(Event::VideoFrame {
        path: p("clip.mp4"),
        image: image(value),
        position,
    });
    core.handle(Event::VideoSeeked(p("clip.mp4")));
}

/// Browsing `clip.mp4` of `clip.mp4` and `b.jpg` paused at its first frame,
/// with the toolbar shown and one arrow drawn.
fn edited() -> Core {
    let mut core = browsing("clip.mp4", &["clip.mp4", "b.jpg"]);
    press(&mut core, Key::P);
    drag(&mut core);
    core
}

#[test]
fn p_opens_the_tools_on_the_first_frame_of_a_video() {
    let mut core = browsing("clip.mp4", &["clip.mp4"]);
    assert!(press(&mut core, Key::P).is_empty());
    assert!(core.view().toolbar.is_some());
    drag(&mut core);
    assert_eq!(shapes(&core), 1);
}

#[test]
fn p_on_a_playing_video_pauses_it_and_its_frame_is_drawn_on() {
    let mut core = browsing("clip.mp4", &["clip.mp4"]);
    press(&mut core, Key::Space);
    core.handle(Event::VideoFrame {
        path: p("clip.mp4"),
        image: image(4),
        position: Duration::from_secs(2),
    });
    assert_eq!(press(&mut core, Key::P), [Command::PauseVideo]);
    assert!(core.view().toolbar.is_some());
    assert!(!core.view().video.as_ref().unwrap().playing);
    drag(&mut core);
    assert_eq!(shapes(&core), 1);
}

#[test]
fn a_frame_a_seek_is_about_to_replace_is_not_drawn_on() {
    let mut core = browsing("clip.mp4", &["clip.mp4"]);
    press(&mut core, Key::P);
    core.handle(Event::StepVideo(VideoStep::Seconds(10)));
    drag(&mut core);
    assert_eq!(shapes(&core), 0);
    core.handle(Event::VideoSeeked(p("clip.mp4")));
    drag(&mut core);
    assert_eq!(shapes(&core), 1);
}

#[test]
fn the_paused_video_of_a_live_photo_is_not_drawn_on() {
    let mut core = browsing("a.jpg", &["a.jpg", "a.mov"]);
    press(&mut core, Key::Space);
    press(&mut core, Key::Space);
    assert!(!core.view().video.as_ref().unwrap().playing);
    press(&mut core, Key::P);
    drag(&mut core);
    assert_eq!(shapes(&core), 0);
}

#[test]
fn the_save_dialog_of_a_frame_offers_no_overwrite() {
    let mut core = edited();
    press(&mut core, Key::Enter);
    let dialog = core.view().save_dialog.clone().unwrap();
    assert!(dialog.frame);
    assert!(dialog.overwrite_refused.is_some());
    assert!(
        core.handle(Event::SaveChoice(SaveChoice::Overwrite))
            .is_empty()
    );
    assert!(core.view().save_dialog.is_some());
}

#[test]
fn save_as_new_is_prefilled_with_the_frame_position_as_png() {
    let mut core = edited();
    press(&mut core, Key::Enter);
    let name = core.view().save_dialog.as_ref().unwrap().name.clone();
    assert_eq!(name, "clip_00-00-00.000.png");

    let mut core = browsing("clip.mp4", &["clip.mp4"]);
    press(&mut core, Key::P);
    step_to(&mut core, 7, Duration::from_millis(3_723_456));
    drag(&mut core);
    press(&mut core, Key::Enter);
    let name = core.view().save_dialog.as_ref().unwrap().name.clone();
    assert_eq!(name, "clip_01-02-03.456.png");
    let commands = press(&mut core, Key::Enter);
    let [
        Command::SaveEdits {
            source,
            image: saved,
            target,
            ..
        },
    ] = commands.as_slice()
    else {
        panic!("{commands:?}");
    };
    assert_eq!(source, &p("clip.mp4"));
    assert_eq!(saved, &image(7));
    assert_eq!(target, &p("clip_01-02-03.456.png"));
}

#[test]
fn a_taken_frame_name_is_numbered() {
    let mut core = browsing("clip.mp4", &["clip.mp4", "clip_00-00-00.000.png"]);
    press(&mut core, Key::P);
    drag(&mut core);
    press(&mut core, Key::Enter);
    let name = core.view().save_dialog.as_ref().unwrap().name.clone();
    assert_eq!(name, "clip_00-00-00.000_2.png");
}

#[test]
fn a_video_played_to_its_end_names_its_first_frame() {
    let mut core = browsing("clip.mp4", &["clip.mp4"]);
    step_to(&mut core, 7, Duration::from_secs(5));
    core.handle(Event::VideoEnded(p("clip.mp4")));
    press(&mut core, Key::P);
    drag(&mut core);
    press(&mut core, Key::Enter);
    let name = core.view().save_dialog.as_ref().unwrap().name.clone();
    assert_eq!(name, "clip_00-00-00.000.png");
}

#[test]
fn playing_seeking_or_leaving_a_frame_with_edits_asks_first() {
    let modified = |shift, alt, command| {
        Event::Key(KeyPress {
            shift,
            alt,
            command,
            ..KeyPress::plain(Key::Right)
        })
    };
    let events = [
        Event::Key(KeyPress::plain(Key::Space)),
        Event::Clicked,
        Event::StepVideo(VideoStep::Seconds(10)),
        Event::StepVideo(VideoStep::Frames(-1)),
        modified(true, false, false),
        modified(false, true, false),
        modified(false, false, true),
        Event::Key(KeyPress::plain(Key::Right)),
        Event::Show(p("b.jpg")),
    ];
    for event in events {
        let mut core = edited();
        let commands = core.handle(event.clone());
        assert!(commands.is_empty(), "{event:?}: {commands:?}");
        assert!(core.view().save_dialog.is_some(), "{event:?}");
        assert_eq!(shapes(&core), 1, "{event:?}");
    }
}

#[test]
fn the_key_line_offers_enter_to_save_frame_edits() {
    let core = browsing("clip.mp4", &["clip.mp4"]);
    let keys = |core: &Core| -> Vec<(&str, &str)> {
        core.view()
            .key_line
            .keys
            .iter()
            .map(|k| (k.keys, k.action))
            .collect()
    };
    assert!(!keys(&core).iter().any(|k| k.0 == "Enter"));
    let core = edited();
    assert!(keys(&core).contains(&("Enter", "save")));
}

#[test]
fn an_export_of_a_frame_offers_no_overwrite() {
    let mut core = browsing("clip.mp4", &["clip.mp4"]);
    press(&mut core, Key::P);
    core.handle(Event::Toolbar(ToolbarClick::ExportWidth(64)));
    core.handle(Event::Toolbar(ToolbarClick::ExportHeight(32)));
    core.handle(Event::Toolbar(ToolbarClick::Export));
    let dialog = core.view().save_dialog.clone().unwrap();
    assert!(dialog.export);
    assert!(dialog.frame);
    assert_eq!(dialog.name, "clip_64x32.jpg");
}

#[test]
fn a_frame_is_copied_as_a_picture_and_shift_c_copies_the_video_file() {
    let mut core = edited();
    assert!(matches!(
        press(&mut core, Key::C).as_slice(),
        [Command::Copy { .. }]
    ));
    press(&mut core, Key::Enter);
    assert!(matches!(
        core.handle(Event::SaveChoice(SaveChoice::Copy)).as_slice(),
        [Command::Copy { .. }]
    ));
    let mut core = browsing("clip.mp4", &["clip.mp4"]);
    let mut shift_c = KeyPress::plain(Key::C);
    shift_c.shift = true;
    assert_eq!(
        core.handle(Event::Key(shift_c)),
        [Command::CopyFile(p("clip.mp4"))]
    );
}
