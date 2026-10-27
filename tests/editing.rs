//! Seam 1: the app core's toolbar, its drawing tools with undo and redo,
//! the crop, and the save dialog that asks what becomes of the edits.

use std::path::PathBuf;
use std::sync::Arc;

use pixpixpix::core::{
    Color, Command, Core, Event, ImagePoint, Key, KeyPress, Points, Region, SaveChoice, Scale,
    Shape, Size, Tool, Toolbar, ToolbarClick,
};
use pixpixpix::decode::DecodedImage;
use pixpixpix::rotate::Rotation;

fn image() -> DecodedImage {
    DecodedImage {
        width: 100,
        height: 50,
        rgba: Arc::from(vec![0; 100 * 50 * 4]),
    }
}

fn p(name: &str) -> PathBuf {
    PathBuf::from(format!("/p/{name}"))
}

fn press(core: &mut Core, key: Key) -> Vec<Command> {
    core.handle(Event::Key(KeyPress::plain(key)))
}

fn undo_keys(shift: bool) -> Event {
    Event::Key(KeyPress {
        shift,
        ..KeyPress::command(Key::Z)
    })
}

fn at(x: f32, y: f32) -> Points {
    Points { x, y }
}

fn point(x: f32, y: f32) -> ImagePoint {
    ImagePoint { x, y }
}

/// Started on `start`, first frame shown, folder listed as `names`, every
/// file decoded, the image filling a viewport of its own size.
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
            result: Ok(image()),
        });
    }
    core.handle(Event::Viewport(at(100.0, 50.0)));
    core
}

/// Drags from `from` to `to` on the image.
fn drag(core: &mut Core, from: Points, to: Points) {
    core.handle(Event::DrawStart(from));
    core.handle(Event::DrawMove(to));
    core.handle(Event::DrawEnd);
}

fn arrow(from: ImagePoint, to: ImagePoint) -> Shape {
    Shape::Arrow {
        from,
        to,
        color: Color::Red,
        size: Size::Medium,
    }
}

/// Browsing `a.jpg` with the toolbar shown and one arrow drawn.
fn edited(names: &[&str]) -> Core {
    let mut core = browsing("a.jpg", names);
    press(&mut core, Key::P);
    drag(&mut core, at(10.0, 10.0), at(60.0, 30.0));
    core
}

fn shapes(core: &Core) -> Vec<Shape> {
    core.view()
        .edits
        .as_ref()
        .map(|e| e.shapes.clone())
        .unwrap_or_default()
}

fn key_names(core: &Core) -> Vec<&'static str> {
    core.view().key_line.keys.iter().map(|k| k.keys).collect()
}

const UNDO: &str = if cfg!(target_os = "macos") {
    "Cmd+Z"
} else {
    "Ctrl+Z"
};

#[test]
fn p_toggles_the_toolbar() {
    let mut core = browsing("a.jpg", &["a.jpg"]);
    assert_eq!(core.view().toolbar, None);
    assert!(press(&mut core, Key::P).is_empty());
    assert_eq!(
        core.view().toolbar,
        Some(Toolbar {
            tool: Tool::Arrow,
            color: Color::Red,
            size: Size::Medium,
        })
    );
    assert!(key_names(&core).contains(&UNDO));
    press(&mut core, Key::P);
    assert_eq!(core.view().toolbar, None);
    assert!(!key_names(&core).contains(&UNDO));
}

#[test]
fn a_drag_draws_an_arrow_in_image_pixels_shown_while_dragging() {
    let mut core = browsing("a.jpg", &["a.jpg"]);
    press(&mut core, Key::P);
    core.handle(Event::DrawStart(at(10.0, 10.0)));
    core.handle(Event::DrawMove(at(50.0, 25.0)));
    let edits = core.view().edits.clone().unwrap();
    assert_eq!(
        edits.drawing,
        Some(arrow(point(10.0, 10.0), point(50.0, 25.0)))
    );
    assert!(edits.shapes.is_empty());
    core.handle(Event::DrawEnd);
    assert_eq!(shapes(&core), [arrow(point(10.0, 10.0), point(50.0, 25.0))]);
    assert_eq!(core.view().edits.as_ref().unwrap().drawing, None);
}

#[test]
fn arrows_take_the_chosen_colour_and_size() {
    let mut core = browsing("a.jpg", &["a.jpg"]);
    press(&mut core, Key::P);
    core.handle(Event::Toolbar(ToolbarClick::Color(Color::Cyan)));
    core.handle(Event::Toolbar(ToolbarClick::Size(Size::Large)));
    drag(&mut core, at(0.0, 0.0), at(20.0, 0.0));
    assert_eq!(
        shapes(&core),
        [Shape::Arrow {
            from: point(0.0, 0.0),
            to: point(20.0, 0.0),
            color: Color::Cyan,
            size: Size::Large,
        }]
    );
}

#[test]
fn drawing_follows_the_zoom_and_the_pending_rotation() {
    let mut core = browsing("a.jpg", &["a.jpg"]);
    press(&mut core, Key::R);
    press(&mut core, Key::P);
    // Turned right, the 50x100 image fits at half size, 37.5 points in.
    drag(&mut core, at(43.75, 25.0), at(37.5, 0.0));
    assert_eq!(shapes(&core), [arrow(point(50.0, 37.5), point(0.0, 50.0))]);
}

#[test]
fn a_drag_beside_the_image_or_without_the_toolbar_draws_nothing() {
    let mut core = browsing("a.jpg", &["a.jpg"]);
    drag(&mut core, at(10.0, 10.0), at(60.0, 30.0));
    assert!(shapes(&core).is_empty());
    press(&mut core, Key::P);
    core.handle(Event::Viewport(at(200.0, 50.0)));
    // The image sits 50 points in.
    drag(&mut core, at(10.0, 10.0), at(60.0, 30.0));
    assert!(shapes(&core).is_empty());
    // A click without a drag is no arrow.
    drag(&mut core, at(60.0, 10.0), at(60.0, 10.0));
    assert!(shapes(&core).is_empty());
}

#[test]
fn the_end_of_a_drag_off_the_image_stays_on_its_edge() {
    let mut core = browsing("a.jpg", &["a.jpg"]);
    press(&mut core, Key::P);
    drag(&mut core, at(10.0, 10.0), at(500.0, -20.0));
    assert_eq!(shapes(&core), [arrow(point(10.0, 10.0), point(100.0, 0.0))]);
}

#[test]
fn undo_and_redo_step_through_the_edits() {
    let mut core = edited(&["a.jpg"]);
    drag(&mut core, at(0.0, 0.0), at(20.0, 20.0));
    assert_eq!(shapes(&core).len(), 2);
    core.handle(undo_keys(false));
    assert_eq!(shapes(&core).len(), 1);
    assert!(core.view().edits.as_ref().unwrap().can_redo);
    core.handle(undo_keys(false));
    assert!(shapes(&core).is_empty());
    assert!(!core.view().edits.as_ref().unwrap().can_undo);
    core.handle(undo_keys(true));
    assert_eq!(shapes(&core).len(), 1);
    core.handle(Event::Toolbar(ToolbarClick::Redo));
    assert_eq!(shapes(&core).len(), 2);
    core.handle(Event::Toolbar(ToolbarClick::Undo));
    assert_eq!(shapes(&core).len(), 1);
    // A new arrow drops what was undone.
    drag(&mut core, at(0.0, 0.0), at(30.0, 30.0));
    assert!(!core.view().edits.as_ref().unwrap().can_redo);
}

#[test]
fn settings_are_reachable_from_the_toolbar() {
    let mut core = browsing("a.jpg", &["a.jpg"]);
    press(&mut core, Key::P);
    core.handle(Event::Toolbar(ToolbarClick::Settings));
    assert!(core.view().settings_open);
}

#[test]
fn enter_with_edits_opens_the_save_dialog_with_its_defaults() {
    let mut core = edited(&["a.jpg", "b.jpg"]);
    assert!(
        core.view()
            .key_line
            .keys
            .iter()
            .any(|k| k.keys == "Enter" && k.action == "save")
    );
    assert!(press(&mut core, Key::Enter).is_empty());
    let dialog = core.view().save_dialog.clone().unwrap();
    assert_eq!(dialog.name, "a_edit.jpg");
    assert_eq!(dialog.overwrite_refused, None);
    assert_eq!(dialog.error, None);
    assert!(!dialog.busy);
    assert_eq!(key_names(&core), ["Enter", "Esc"]);
}

#[test]
fn the_new_name_skips_names_taken() {
    let mut core = edited(&["a.jpg", "a_edit.jpg", "a_edit2.jpg"]);
    press(&mut core, Key::Enter);
    assert_eq!(
        core.view().save_dialog.as_ref().unwrap().name,
        "a_edit3.jpg"
    );
}

#[test]
fn a_heic_is_saved_as_new_png_and_not_overwritten() {
    let mut core = browsing("a.heic", &["a.heic"]);
    press(&mut core, Key::P);
    drag(&mut core, at(10.0, 10.0), at(60.0, 30.0));
    press(&mut core, Key::Enter);
    let dialog = core.view().save_dialog.clone().unwrap();
    assert_eq!(dialog.name, "a_edit.png");
    assert!(dialog.overwrite_refused.is_some());
    assert!(
        core.handle(Event::SaveChoice(SaveChoice::Overwrite))
            .is_empty()
    );
}

#[test]
fn a_multi_image_file_is_not_overwritten() {
    let mut core = browsing("a.tiff", &["a.tiff"]);
    core.handle(Event::Images {
        path: p("a.tiff"),
        count: 3,
    });
    press(&mut core, Key::P);
    drag(&mut core, at(10.0, 10.0), at(60.0, 30.0));
    press(&mut core, Key::Enter);
    assert!(
        core.view()
            .save_dialog
            .as_ref()
            .unwrap()
            .overwrite_refused
            .is_some()
    );
}

fn save(core: &Core, target: &str, rotation: Rotation) -> Command {
    Command::SaveEdits {
        source: p("a.jpg"),
        image: image(),
        rotation,
        shapes: shapes(core),
        target: p(target),
    }
}

#[test]
fn enter_in_the_dialog_saves_as_new_once() {
    let mut core = edited(&["a.jpg"]);
    press(&mut core, Key::R);
    press(&mut core, Key::Enter);
    let expected = save(&core, "a_edit.jpg", Rotation::Right);
    assert_eq!(press(&mut core, Key::Enter), [expected]);
    assert!(core.view().save_dialog.as_ref().unwrap().busy);
    // Busy: a second Enter writes nothing more.
    assert!(press(&mut core, Key::Enter).is_empty());
    core.handle(Event::EditsSaved {
        target: p("a_edit.jpg"),
        result: Ok(()),
    });
    assert_eq!(core.view().save_dialog, None);
    assert_eq!(core.view().edits, None);
    // The original stays as it was, its turn still pending.
    assert_eq!(core.view().rotation, Rotation::Right);
}

#[test]
fn the_new_name_can_be_edited() {
    let mut core = edited(&["a.jpg"]);
    press(&mut core, Key::Enter);
    core.handle(Event::SaveName("../x.png".into()));
    assert!(
        core.handle(Event::SaveChoice(SaveChoice::SaveAsNew))
            .is_empty()
    );
    assert!(core.view().save_dialog.as_ref().unwrap().error.is_some());
    core.handle(Event::SaveName("shot.png".into()));
    let expected = save(&core, "shot.png", Rotation::None);
    assert_eq!(
        core.handle(Event::SaveChoice(SaveChoice::SaveAsNew)),
        [expected]
    );
}

#[test]
fn a_failed_save_keeps_the_dialog_and_the_edits() {
    let mut core = edited(&["a.jpg"]);
    press(&mut core, Key::Enter);
    press(&mut core, Key::Enter);
    core.handle(Event::EditsSaved {
        target: p("a_edit.jpg"),
        result: Err("Cannot save a_edit.jpg: disk full".into()),
    });
    let dialog = core.view().save_dialog.clone().unwrap();
    assert_eq!(
        dialog.error.as_deref(),
        Some("Cannot save a_edit.jpg: disk full")
    );
    assert!(!dialog.busy);
    assert_eq!(shapes(&core).len(), 1);
}

#[test]
fn esc_goes_back_to_editing_with_the_edits() {
    let mut core = edited(&["a.jpg", "b.jpg"]);
    press(&mut core, Key::Right);
    assert!(core.view().save_dialog.is_some());
    assert!(press(&mut core, Key::Escape).is_empty());
    assert_eq!(core.view().save_dialog, None);
    assert_eq!(shapes(&core).len(), 1);
    // The interrupted step is dropped.
    assert_eq!(
        core.view().band.iter().find(|t| t.current).unwrap().path,
        p("a.jpg")
    );
}

#[test]
fn overwrite_writes_the_file_and_decodes_it_again() {
    let mut core = edited(&["a.jpg"]);
    press(&mut core, Key::Enter);
    let expected = save(&core, "a.jpg", Rotation::None);
    assert_eq!(
        core.handle(Event::SaveChoice(SaveChoice::Overwrite)),
        [expected]
    );
    let commands = core.handle(Event::EditsSaved {
        target: p("a.jpg"),
        result: Ok(()),
    });
    assert_eq!(commands, [Command::Decode(p("a.jpg"))]);
    assert_eq!(core.view().save_dialog, None);
    assert_eq!(core.view().edits, None);
}

#[test]
fn undo_in_the_dialog_drops_the_edits() {
    let mut core = edited(&["a.jpg"]);
    press(&mut core, Key::Enter);
    assert!(
        core.handle(Event::SaveChoice(SaveChoice::Discard))
            .is_empty()
    );
    assert_eq!(core.view().save_dialog, None);
    assert_eq!(core.view().edits, None);
}

#[test]
fn copy_in_the_dialog_copies_the_edited_image() {
    let mut core = edited(&["a.jpg", "b.jpg"]);
    press(&mut core, Key::Right);
    let expected = Command::Copy {
        image: image(),
        rotation: Rotation::None,
        shapes: shapes(&core),
    };
    assert_eq!(core.handle(Event::SaveChoice(SaveChoice::Copy)), [expected]);
    // The step waits for the copy, then goes on.
    core.handle(Event::Copied(Ok(())));
    assert_eq!(core.view().save_dialog, None);
    assert_eq!(
        core.view().band.iter().find(|t| t.current).unwrap().path,
        p("b.jpg")
    );
}

#[test]
fn c_copies_the_image_with_its_edits() {
    let mut core = edited(&["a.jpg"]);
    let expected = Command::Copy {
        image: image(),
        rotation: Rotation::None,
        shapes: shapes(&core),
    };
    assert_eq!(press(&mut core, Key::C), [expected]);
}

#[test]
fn leaving_the_image_asks_first_and_goes_on_after() {
    let mut core = edited(&["a.jpg", "b.jpg"]);
    assert!(press(&mut core, Key::Right).is_empty());
    assert!(core.view().save_dialog.is_some());
    assert_eq!(
        core.view().band.iter().find(|t| t.current).unwrap().path,
        p("a.jpg")
    );
    core.handle(Event::SaveChoice(SaveChoice::Discard));
    assert_eq!(
        core.view().band.iter().find(|t| t.current).unwrap().path,
        p("b.jpg")
    );
    assert_eq!(core.view().edits, None);
}

#[test]
fn every_way_out_of_the_image_asks_first() {
    let keys = [
        Key::Left,
        Key::Right,
        Key::X,
        Key::G,
        Key::N,
        Key::B,
        Key::Tab,
    ];
    for key in keys {
        let mut core = edited(&["a.jpg", "b.jpg"]);
        assert!(press(&mut core, key).is_empty(), "{key:?}");
        assert!(core.view().save_dialog.is_some(), "{key:?}");
    }
    let mut core = edited(&["a.jpg", "b.jpg"]);
    assert!(core.handle(Event::Show(p("b.jpg"))).is_empty());
    assert!(core.view().save_dialog.is_some());
}

#[test]
fn keys_that_stay_on_the_image_do_not_ask() {
    let mut core = edited(&["a.jpg", "b.jpg"]);
    for key in [Key::Plus, Key::L, Key::P, Key::F] {
        press(&mut core, key);
    }
    assert_eq!(core.view().save_dialog, None);
    assert_eq!(shapes(&core).len(), 1);
}

#[test]
fn quitting_waits_for_the_save() {
    let mut core = edited(&["a.jpg"]);
    assert!(
        core.handle(Event::Key(KeyPress::command(Key::Q)))
            .is_empty()
    );
    assert!(core.view().save_dialog.is_some());
    let commands = press(&mut core, Key::Enter);
    assert!(!commands.contains(&Command::Quit));
    let commands = core.handle(Event::EditsSaved {
        target: p("a_edit.jpg"),
        result: Ok(()),
    });
    assert_eq!(commands, [Command::Quit]);
}

#[test]
fn a_failed_save_does_not_quit() {
    let mut core = edited(&["a.jpg"]);
    core.handle(Event::Key(KeyPress::command(Key::Q)));
    press(&mut core, Key::Enter);
    let commands = core.handle(Event::EditsSaved {
        target: p("a_edit.jpg"),
        result: Err("no".into()),
    });
    assert!(commands.is_empty());
    assert!(core.view().save_dialog.is_some());
}

#[test]
fn the_close_button_keeps_the_window_while_edits_are_unsaved() {
    let mut core = browsing("a.jpg", &["a.jpg"]);
    assert_eq!(core.handle(Event::CloseRequested), [Command::Quit]);
    let mut core = edited(&["a.jpg"]);
    assert_eq!(core.handle(Event::CloseRequested), [Command::KeepOpen]);
    assert!(core.view().save_dialog.is_some());
    assert_eq!(
        core.handle(Event::SaveChoice(SaveChoice::Discard)),
        [Command::Quit]
    );
}

#[test]
fn a_playing_video_is_not_drawn_on() {
    let mut core = browsing("v.mp4", &["v.mp4"]);
    press(&mut core, Key::P);
    press(&mut core, Key::Space);
    assert!(core.view().video.as_ref().unwrap().playing);
    drag(&mut core, at(10.0, 10.0), at(60.0, 30.0));
    assert!(shapes(&core).is_empty());
}

#[test]
fn playing_the_video_of_a_live_photo_with_edits_asks_first() {
    let presses = [
        KeyPress::plain(Key::Space),
        KeyPress {
            shift: true,
            ..KeyPress::plain(Key::Right)
        },
        KeyPress {
            alt: true,
            ..KeyPress::plain(Key::Left)
        },
        KeyPress::command(Key::Right),
    ];
    for key in presses {
        let mut core = browsing("a.jpg", &["a.jpg", "a.mov"]);
        press(&mut core, Key::P);
        drag(&mut core, at(10.0, 10.0), at(60.0, 30.0));
        assert_eq!(shapes(&core).len(), 1);
        let commands = core.handle(Event::Key(key));
        assert!(
            !commands
                .iter()
                .any(|c| matches!(c, Command::PlayVideo { .. })),
            "{key:?}"
        );
        assert!(core.view().save_dialog.is_some(), "{key:?}");
        assert_eq!(shapes(&core).len(), 1, "{key:?}");
    }
}

fn choose(core: &mut Core, tool: Tool) {
    core.handle(Event::Toolbar(ToolbarClick::Tool(tool)));
}

fn region(left: f32, top: f32, right: f32, bottom: f32) -> Region {
    Region {
        left,
        top,
        right,
        bottom,
    }
}

/// A click on the image, as the UI reports it.
fn click(core: &mut Core, on: Points) {
    core.handle(Event::DrawStart(on));
    core.handle(Event::DrawEnd);
}

fn drawing(core: &Core) -> Option<Shape> {
    core.view().edits.as_ref().and_then(|e| e.drawing.clone())
}

#[test]
fn every_tool_can_be_chosen() {
    let mut core = browsing("a.jpg", &["a.jpg"]);
    press(&mut core, Key::P);
    for tool in Tool::ALL {
        choose(&mut core, tool);
        assert_eq!(core.view().toolbar.unwrap().tool, tool);
    }
    assert_eq!(Tool::ALL.len(), 9);
}

#[test]
fn box_tools_draw_between_the_corners_in_image_pixels_each_one_step() {
    let mut core = browsing("a.jpg", &["a.jpg"]);
    press(&mut core, Key::P);
    core.handle(Event::Toolbar(ToolbarClick::Color(Color::Green)));
    let (from, to) = (point(25.0, 12.5), point(75.0, 25.0));
    for tool in [
        Tool::Rectangle,
        Tool::Ellipse,
        Tool::Highlighter,
        Tool::Pixelate,
    ] {
        choose(&mut core, tool);
        drag(&mut core, at(25.0, 12.5), at(75.0, 25.0));
        // A line is no box.
        drag(&mut core, at(25.0, 12.5), at(75.0, 12.5));
    }
    let (color, size) = (Color::Green, Size::Medium);
    assert_eq!(
        shapes(&core),
        [
            Shape::Rectangle {
                from,
                to,
                color,
                size
            },
            Shape::Ellipse {
                from,
                to,
                color,
                size
            },
            Shape::Highlighter { from, to, color },
            Shape::Pixelate { from, to, size },
        ]
    );
    for left in (0..4).rev() {
        core.handle(undo_keys(false));
        assert_eq!(shapes(&core).len(), left);
    }
    core.handle(undo_keys(true));
    assert!(matches!(shapes(&core)[..], [Shape::Rectangle { .. }]));
}

#[test]
fn the_pen_follows_the_drag() {
    let mut core = browsing("a.jpg", &["a.jpg"]);
    press(&mut core, Key::P);
    choose(&mut core, Tool::Pen);
    core.handle(Event::DrawStart(at(10.0, 10.0)));
    core.handle(Event::DrawMove(at(50.0, 25.0)));
    // Less than a pixel on adds nothing.
    core.handle(Event::DrawMove(at(50.2, 25.2)));
    core.handle(Event::DrawMove(at(500.0, 20.0)));
    core.handle(Event::DrawEnd);
    assert_eq!(
        shapes(&core),
        [Shape::Pen {
            points: vec![point(10.0, 10.0), point(50.0, 25.0), point(100.0, 20.0)],
            color: Color::Red,
            size: Size::Medium,
        }]
    );
    // A click is no line.
    click(&mut core, at(30.0, 30.0));
    assert_eq!(shapes(&core).len(), 1);
}

#[test]
fn text_is_placed_by_a_click_typed_and_done_as_one_step() {
    let mut core = browsing("a.jpg", &["a.jpg"]);
    press(&mut core, Key::R);
    press(&mut core, Key::P);
    choose(&mut core, Tool::Text);
    // Turned right, the 50x100 image fits at half size, 37.5 points in.
    click(&mut core, at(43.75, 25.0));
    core.handle(Event::TextEdited("hi".to_owned()));
    let text = Shape::Text {
        at: point(50.0, 37.5),
        text: "hi".to_owned(),
        color: Color::Red,
        size: Size::Medium,
        turn: Rotation::Right,
    };
    assert_eq!(drawing(&core), Some(text.clone()));
    assert!(shapes(&core).is_empty());
    core.handle(Event::TextDone);
    assert_eq!(shapes(&core), [text]);
    assert_eq!(drawing(&core), None);
    core.handle(undo_keys(false));
    assert!(shapes(&core).is_empty());
}

#[test]
fn a_click_elsewhere_ends_the_text_and_empty_text_is_dropped() {
    let mut core = browsing("a.jpg", &["a.jpg"]);
    press(&mut core, Key::P);
    choose(&mut core, Tool::Text);
    click(&mut core, at(10.0, 10.0));
    core.handle(Event::TextEdited("one".to_owned()));
    // The click that ends it places no new text.
    click(&mut core, at(50.0, 30.0));
    assert_eq!(shapes(&core).len(), 1);
    assert_eq!(drawing(&core), None);
    click(&mut core, at(50.0, 30.0));
    core.handle(Event::TextEdited("  ".to_owned()));
    core.handle(Event::TextDone);
    assert_eq!(shapes(&core).len(), 1);
}

#[test]
fn text_being_typed_is_kept_when_the_save_dialog_asks() {
    let mut core = browsing("a.jpg", &["a.jpg", "b.jpg"]);
    press(&mut core, Key::P);
    choose(&mut core, Tool::Text);
    click(&mut core, at(10.0, 10.0));
    core.handle(Event::TextEdited("x".to_owned()));
    press(&mut core, Key::Right);
    assert!(core.view().save_dialog.is_some());
    assert_eq!(shapes(&core).len(), 1);
    // The dialog takes no text.
    core.handle(Event::TextEdited("y".to_owned()));
    assert_eq!(drawing(&core), None);
}

#[test]
fn the_crop_tool_frames_the_part_kept_and_other_tools_show_only_it() {
    let mut core = browsing("a.jpg", &["a.jpg"]);
    press(&mut core, Key::P);
    choose(&mut core, Tool::Crop);
    drag(&mut core, at(10.2, 9.8), at(60.0, 30.4));
    let crop = region(10.0, 10.0, 60.0, 30.0);
    assert_eq!(shapes(&core), [Shape::Crop(crop)]);
    assert_eq!(core.view().edits.as_ref().unwrap().cropping, Some(crop));
    assert_eq!(core.view().crop, None);
    press(&mut core, Key::Plus);
    choose(&mut core, Tool::Arrow);
    assert_eq!(core.view().crop, Some(crop));
    assert_eq!(core.view().edits.as_ref().unwrap().cropping, None);
    // The 50x20 part fits anew, at 100 %, 25 and 15 points in.
    assert_eq!(core.view().zoom.scale, Scale::Fit);
    drag(&mut core, at(25.0, 15.0), at(75.0, 35.0));
    assert_eq!(
        shapes(&core)[1],
        arrow(point(10.0, 10.0), point(60.0, 30.0))
    );
    // Drawing stays within the part shown.
    drag(&mut core, at(5.0, 5.0), at(30.0, 30.0));
    assert_eq!(shapes(&core).len(), 2);
}

#[test]
fn crop_handles_move_corners_and_sides_each_one_step() {
    let mut core = browsing("a.jpg", &["a.jpg"]);
    press(&mut core, Key::P);
    choose(&mut core, Tool::Crop);
    drag(&mut core, at(10.0, 10.0), at(60.0, 30.0));
    // The bottom right corner, grabbed a few points off.
    drag(&mut core, at(63.0, 33.0), at(80.0, 40.0));
    assert_eq!(
        shapes(&core)[1],
        Shape::Crop(region(10.0, 10.0, 80.0, 40.0))
    );
    // The middle of the left side moves only that side.
    drag(&mut core, at(10.0, 25.0), at(20.0, 0.0));
    assert_eq!(
        shapes(&core)[2],
        Shape::Crop(region(20.0, 10.0, 80.0, 40.0))
    );
    // Far from any handle a new box starts.
    drag(&mut core, at(40.0, 20.0), at(50.0, 30.0));
    assert_eq!(
        shapes(&core)[3],
        Shape::Crop(region(40.0, 20.0, 50.0, 30.0))
    );
    core.handle(undo_keys(false));
    assert_eq!(
        core.view().edits.as_ref().unwrap().cropping,
        Some(region(20.0, 10.0, 80.0, 40.0))
    );
}

#[test]
fn without_a_crop_the_image_corners_are_handles() {
    let mut core = browsing("a.jpg", &["a.jpg"]);
    press(&mut core, Key::P);
    choose(&mut core, Tool::Crop);
    drag(&mut core, at(1.0, 1.0), at(20.0, 10.0));
    assert_eq!(
        shapes(&core),
        [Shape::Crop(region(20.0, 10.0, 100.0, 50.0))]
    );
    // A click changes nothing.
    click(&mut core, at(50.0, 30.0));
    assert_eq!(shapes(&core).len(), 1);
}

#[test]
fn undoing_a_crop_shows_the_whole_image_again() {
    let mut core = browsing("a.jpg", &["a.jpg"]);
    press(&mut core, Key::P);
    choose(&mut core, Tool::Crop);
    drag(&mut core, at(10.0, 10.0), at(60.0, 30.0));
    choose(&mut core, Tool::Arrow);
    assert!(core.view().crop.is_some());
    core.handle(undo_keys(false));
    assert_eq!(core.view().crop, None);
    // The save gets the crop with the other shapes.
    core.handle(undo_keys(true));
    press(&mut core, Key::Enter);
    let commands = core.handle(Event::SaveChoice(SaveChoice::SaveAsNew));
    assert!(matches!(
        &commands[..],
        [Command::SaveEdits { shapes, .. }] if shapes == &[Shape::Crop(region(10.0, 10.0, 60.0, 30.0))]
    ));
}
