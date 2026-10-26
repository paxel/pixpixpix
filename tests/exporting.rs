//! Seam 1: the app core's export tool: the target size, the area drawn in
//! the ratio of that size or freely, and the save dialog that asks where the
//! export goes.

use std::path::PathBuf;
use std::sync::Arc;

use pixpixpix::core::{
    Command, Core, Event, Export, ExportFormat, Key, KeyPress, Points, Region, SaveChoice, Shape,
    Tool, ToolbarClick,
};
use pixpixpix::decode::DecodedImage;
use pixpixpix::rotate::Rotation;
use pixpixpix::settings::Settings;

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

fn at(x: f32, y: f32) -> Points {
    Points { x, y }
}

fn click(core: &mut Core, click: ToolbarClick) -> Vec<Command> {
    core.handle(Event::Toolbar(click))
}

fn region(left: f32, top: f32, right: f32, bottom: f32) -> Region {
    Region {
        left,
        top,
        right,
        bottom,
    }
}

/// Started on `a.jpg` among `names`, every file decoded and filling a
/// viewport of its own size, so points are image pixels; the toolbar shown
/// with the export tool chosen at `width` × `height`.
fn exporting(names: &[&str], width: u32, height: u32) -> Core {
    let (mut core, _) = Core::start(Ok(p("a.jpg")));
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
    press(&mut core, Key::P);
    click(&mut core, ToolbarClick::Tool(Tool::Export));
    click(&mut core, ToolbarClick::ExportWidth(width));
    click(&mut core, ToolbarClick::ExportHeight(height));
    core
}

fn drag(core: &mut Core, from: Points, to: Points) {
    core.handle(Event::DrawStart(from));
    core.handle(Event::DrawMove(to));
    core.handle(Event::DrawEnd);
}

fn area(core: &Core) -> Option<Region> {
    core.view().export.and_then(|e| e.area)
}

fn export(core: &Core) -> Option<Export> {
    core.view().export.map(|e| e.export)
}

fn assert_near(actual: Option<Region>, expected: Region) {
    let near = |a: f32, b: f32| (a - b).abs() < 0.01;
    assert!(
        actual.is_some_and(|actual| near(actual.left, expected.left)
            && near(actual.top, expected.top)
            && near(actual.right, expected.right)
            && near(actual.bottom, expected.bottom)),
        "{actual:?} is not {expected:?}"
    );
}

#[test]
fn the_export_tool_starts_locked_as_jpeg_at_the_remembered_size() {
    let mut core = exporting(&["a.jpg"], 1920, 1080);
    assert_eq!(
        export(&core).unwrap(),
        Export {
            width: 1920,
            height: 1080,
            locked: true,
            format: ExportFormat::Jpeg,
            limit_kb: None,
        }
    );
    core.handle(Event::SettingsLoaded(Ok(Settings {
        export_width: 512,
        export_height: 300,
        ..Settings::default()
    })));
    assert_eq!(
        (export(&core).unwrap().width, export(&core).unwrap().height),
        (512, 300)
    );
    // Other tools show no export.
    click(&mut core, ToolbarClick::Tool(Tool::Arrow));
    assert_eq!(core.view().export, None);
}

#[test]
fn without_an_area_drawn_the_largest_box_of_the_ratio_in_the_middle_is_exported() {
    let mut core = exporting(&["a.jpg"], 40, 40);
    assert_near(area(&core), region(25.0, 0.0, 75.0, 50.0));
    click(&mut core, ToolbarClick::ExportLocked(false));
    assert!(!export(&core).unwrap().locked);
    assert_near(area(&core), region(0.0, 0.0, 100.0, 50.0));
}

#[test]
fn a_locked_drag_keeps_the_ratio_and_stays_on_the_image() {
    let mut core = exporting(&["a.jpg"], 200, 100);
    drag(&mut core, at(10.0, 10.0), at(60.0, 20.0));
    assert_near(area(&core), region(10.0, 10.0, 60.0, 35.0));
    // Past the right edge both sides shrink, so the ratio holds.
    drag(&mut core, at(30.0, 10.0), at(100.0, 50.0));
    assert_near(area(&core), region(30.0, 10.0, 100.0, 45.0));
    // Up and to the left of where it started.
    drag(&mut core, at(60.0, 40.0), at(40.0, 38.0));
    assert_near(area(&core), region(40.0, 30.0, 60.0, 40.0));
}

#[test]
fn a_free_drag_takes_any_area() {
    let mut core = exporting(&["a.jpg"], 200, 100);
    click(&mut core, ToolbarClick::ExportLocked(false));
    drag(&mut core, at(10.0, 10.0), at(60.0, 20.0));
    assert_near(area(&core), region(10.0, 10.0, 60.0, 20.0));
}

#[test]
fn locked_corners_keep_the_ratio_and_free_sides_move_alone() {
    let mut core = exporting(&["a.jpg"], 200, 100);
    drag(&mut core, at(10.0, 10.0), at(60.0, 35.0));
    // The bottom right corner, the top left one staying.
    drag(&mut core, at(60.0, 35.0), at(80.0, 45.0));
    assert_near(area(&core), region(10.0, 10.0, 80.0, 45.0));
    // While locked the middle of a side is no handle: a new area starts.
    drag(&mut core, at(80.0, 27.5), at(90.0, 30.0));
    assert_near(area(&core), region(80.0, 27.5, 90.0, 32.5));
    click(&mut core, ToolbarClick::ExportLocked(false));
    drag(&mut core, at(10.0, 10.0), at(60.0, 35.0));
    drag(&mut core, at(60.0, 22.5), at(90.0, 40.0));
    assert_near(area(&core), region(10.0, 10.0, 90.0, 35.0));
}

#[test]
fn a_new_size_refits_the_area_drawn_while_locked() {
    let mut core = exporting(&["a.jpg"], 200, 100);
    drag(&mut core, at(10.0, 10.0), at(60.0, 35.0));
    click(&mut core, ToolbarClick::ExportWidth(100));
    assert_near(area(&core), region(22.5, 10.0, 47.5, 35.0));
    // Free again, the area drawn is back as drawn.
    click(&mut core, ToolbarClick::ExportLocked(false));
    assert_near(area(&core), region(10.0, 10.0, 60.0, 35.0));
}

#[test]
fn a_pending_quarter_turn_turns_the_ratio_too() {
    let mut core = exporting(&["a.jpg"], 200, 100);
    press(&mut core, Key::R);
    // 2:1 as shown is 1:2 in the pixels as decoded.
    assert_near(area(&core), region(37.5, 0.0, 62.5, 50.0));
}

#[test]
fn the_area_stays_within_a_crop() {
    let mut core = exporting(&["a.jpg"], 200, 100);
    click(&mut core, ToolbarClick::Tool(Tool::Crop));
    drag(&mut core, at(10.0, 10.0), at(60.0, 30.0));
    click(&mut core, ToolbarClick::Tool(Tool::Export));
    click(&mut core, ToolbarClick::ExportLocked(false));
    assert_near(area(&core), region(10.0, 10.0, 60.0, 30.0));
}

#[test]
fn exporting_asks_for_a_new_name_with_the_size_and_remembers_the_size() {
    let mut core = exporting(&["a.jpg"], 512, 256);
    let commands = click(&mut core, ToolbarClick::Export);
    assert_eq!(
        commands,
        [Command::SaveSettings(Settings {
            export_width: 512,
            export_height: 256,
            ..Settings::default()
        })]
    );
    let dialog = core.view().save_dialog.clone().unwrap();
    assert_eq!(dialog.name, "a_512x256.jpg");
    assert!(dialog.export);
    assert!(dialog.overwrite_refused.is_some());
    // Overwrite and Undo do nothing; Back leaves.
    assert!(
        core.handle(Event::SaveChoice(SaveChoice::Overwrite))
            .is_empty()
    );
    assert!(core.view().save_dialog.is_some());
    press(&mut core, Key::Escape);
    assert_eq!(core.view().save_dialog, None);
    // The same size again is not saved again.
    assert!(click(&mut core, ToolbarClick::Export).is_empty());
}

#[test]
fn the_name_takes_the_extension_of_the_format() {
    let mut core = exporting(&["a.jpg"], 64, 32);
    for (format, name) in [
        (ExportFormat::Png, "a_64x32.png"),
        (ExportFormat::WebP, "a_64x32.webp"),
        (ExportFormat::Jpeg, "a_64x32.jpg"),
    ] {
        click(&mut core, ToolbarClick::ExportFormat(format));
        click(&mut core, ToolbarClick::Export);
        assert_eq!(core.view().save_dialog.as_ref().unwrap().name, name);
        press(&mut core, Key::Escape);
    }
}

#[test]
fn enter_exports_the_area_with_the_edits_and_keeps_them() {
    let mut core = exporting(&["a.jpg"], 200, 100);
    click(&mut core, ToolbarClick::Tool(Tool::Arrow));
    drag(&mut core, at(5.0, 5.0), at(30.0, 30.0));
    let shapes = core.view().edits.as_ref().unwrap().shapes.clone();
    assert_eq!(shapes.len(), 1);
    click(&mut core, ToolbarClick::Tool(Tool::Export));
    drag(&mut core, at(10.0, 10.0), at(60.0, 20.0));
    click(&mut core, ToolbarClick::ExportLimit(Some(300)));
    click(&mut core, ToolbarClick::Export);
    let commands = press(&mut core, Key::Enter);
    let target = p("a_200x100.jpg");
    assert_eq!(
        commands,
        [Command::Export {
            source: p("a.jpg"),
            image: image(),
            rotation: Rotation::None,
            shapes: shapes.clone(),
            area: area_of(&commands).unwrap(),
            export: Export {
                width: 200,
                height: 100,
                locked: true,
                format: ExportFormat::Jpeg,
                limit_kb: Some(300),
            },
            target: target.clone(),
        }]
    );
    assert_near(area_of(&commands), region(10.0, 10.0, 60.0, 35.0));
    // Once more while it runs does nothing.
    assert!(press(&mut core, Key::Enter).is_empty());
    core.handle(Event::EditsSaved {
        target,
        result: Ok(()),
    });
    assert_eq!(core.view().save_dialog, None);
    assert_eq!(core.view().edits.as_ref().unwrap().shapes, shapes);
}

fn area_of(commands: &[Command]) -> Option<Region> {
    match commands {
        [Command::Export { area, .. }] => Some(*area),
        _ => None,
    }
}

#[test]
fn a_failed_export_keeps_the_dialog_with_the_reason() {
    let mut core = exporting(&["a.jpg"], 200, 100);
    click(&mut core, ToolbarClick::Export);
    press(&mut core, Key::Enter);
    core.handle(Event::EditsSaved {
        target: p("a_200x100.jpg"),
        result: Err("Cannot save a_200x100.jpg: too big".into()),
    });
    let dialog = core.view().save_dialog.clone().unwrap();
    assert_eq!(
        dialog.error.as_deref(),
        Some("Cannot save a_200x100.jpg: too big")
    );
    assert!(!dialog.busy);
}

#[test]
fn copy_puts_the_export_on_the_clipboard() {
    let mut core = exporting(&["a.jpg"], 200, 100);
    click(&mut core, ToolbarClick::Export);
    let commands = core.handle(Event::SaveChoice(SaveChoice::Copy));
    let [Command::CopyExport { area, export, .. }] = commands.as_slice() else {
        panic!("no copy: {commands:?}");
    };
    assert_near(Some(*area), region(0.0, 0.0, 100.0, 50.0));
    assert_eq!((export.width, export.height), (200, 100));
    core.handle(Event::Copied(Ok(())));
    assert_eq!(core.view().save_dialog, None);
}

#[test]
fn the_area_drawn_belongs_to_its_file() {
    let mut core = exporting(&["a.jpg", "b.jpg"], 200, 100);
    click(&mut core, ToolbarClick::ExportLocked(false));
    drag(&mut core, at(10.0, 10.0), at(60.0, 20.0));
    press(&mut core, Key::Right);
    assert_near(area(&core), region(0.0, 0.0, 100.0, 50.0));
    press(&mut core, Key::Left);
    assert_near(area(&core), region(0.0, 0.0, 100.0, 50.0));
}

#[test]
fn an_export_with_unsaved_edits_does_not_drop_them_on_undo() {
    let mut core = exporting(&["a.jpg"], 200, 100);
    click(&mut core, ToolbarClick::Tool(Tool::Arrow));
    drag(&mut core, at(5.0, 5.0), at(30.0, 30.0));
    click(&mut core, ToolbarClick::Tool(Tool::Export));
    click(&mut core, ToolbarClick::Export);
    core.handle(Event::SaveChoice(SaveChoice::Discard));
    assert_eq!(core.view().save_dialog, None);
    assert!(matches!(
        core.view().edits.as_ref().unwrap().shapes.as_slice(),
        [Shape::Arrow { .. }]
    ));
}

#[test]
fn closing_the_window_drops_an_export_not_started_and_waits_for_a_running_one() {
    let mut core = exporting(&["a.jpg"], 200, 100);
    click(&mut core, ToolbarClick::Export);
    assert_eq!(core.handle(Event::CloseRequested), [Command::Quit]);
    let mut core = exporting(&["a.jpg"], 200, 100);
    click(&mut core, ToolbarClick::Export);
    press(&mut core, Key::Enter);
    assert_eq!(core.handle(Event::CloseRequested), [Command::KeepOpen]);
    let commands = core.handle(Event::EditsSaved {
        target: p("a_200x100.jpg"),
        result: Ok(()),
    });
    assert_eq!(commands, [Command::Quit]);
}

#[test]
fn closing_the_window_from_an_export_asks_about_unsaved_edits() {
    let mut core = exporting(&["a.jpg"], 200, 100);
    click(&mut core, ToolbarClick::Tool(Tool::Arrow));
    drag(&mut core, at(5.0, 5.0), at(30.0, 30.0));
    click(&mut core, ToolbarClick::Tool(Tool::Export));
    click(&mut core, ToolbarClick::Export);
    // The close drops the export and asks about the edits instead.
    assert_eq!(core.handle(Event::CloseRequested), [Command::KeepOpen]);
    assert!(!core.view().save_dialog.as_ref().unwrap().export);
}

#[test]
fn a_click_off_the_handles_keeps_the_area() {
    let mut core = exporting(&["a.jpg"], 200, 100);
    drag(&mut core, at(10.0, 10.0), at(60.0, 35.0));
    // Inside, and on the middle of a side, which is no handle while locked.
    for spot in [at(35.0, 22.0), at(60.0, 22.5)] {
        core.handle(Event::DrawStart(spot));
        core.handle(Event::DrawEnd);
        assert_near(area(&core), region(10.0, 10.0, 60.0, 35.0));
    }
}
