//! The toolbar, the edits over the image and the save dialog with its
//! panicked cat.

use egui::{Color32, RichText, Stroke, vec2};

use super::{DIM_TEXT, HIGHLIGHT, TEXT};
use crate::core::{
    Color, EditsView, Event, SaveChoice, SaveDialog, Size, Tool, Toolbar, ToolbarClick, turned,
};
use crate::rotate::Rotation;

/// The undo and redo keys as the toolbar names them.
const UNDO_HINT: &str = if cfg!(target_os = "macos") {
    "Undo (Cmd+Z)"
} else {
    "Undo (Ctrl+Z)"
};
const REDO_HINT: &str = if cfg!(target_os = "macos") {
    "Redo (Cmd+Shift+Z)"
} else {
    "Redo (Ctrl+Shift+Z)"
};

/// The side of a colour swatch.
const SWATCH: f32 = 20.0;

pub fn color32(color: Color) -> Color32 {
    let [r, g, b] = color.rgb();
    Color32::from_rgb(r, g, b)
}

/// One row: the tools, the palette, the sizes, undo, redo and settings.
pub fn show_toolbar(
    ui: &mut egui::Ui,
    toolbar: Toolbar,
    edits: Option<&EditsView>,
    events: &mut Vec<Event>,
) {
    let text = |s: &str| RichText::new(s).color(TEXT);
    ui.horizontal(|ui| {
        for tool in Tool::ALL {
            let button = egui::Button::selectable(toolbar.tool == tool, text(tool.name()));
            if ui.add(button).clicked() {
                events.push(Event::Toolbar(ToolbarClick::Tool(tool)));
            }
        }
        ui.add_space(12.0);
        for color in Color::ALL {
            if swatch(ui, color, toolbar.color == color).clicked() {
                events.push(Event::Toolbar(ToolbarClick::Color(color)));
            }
        }
        ui.add_space(12.0);
        for size in Size::ALL {
            let button = egui::Button::selectable(toolbar.size == size, text(size.name()));
            if ui.add(button).clicked() {
                events.push(Event::Toolbar(ToolbarClick::Size(size)));
            }
        }
        ui.add_space(12.0);
        let (can_undo, can_redo) = edits.map_or((false, false), |e| (e.can_undo, e.can_redo));
        if ui
            .add_enabled(can_undo, egui::Button::new(text("Undo")))
            .on_hover_text(UNDO_HINT)
            .clicked()
        {
            events.push(Event::Toolbar(ToolbarClick::Undo));
        }
        if ui
            .add_enabled(can_redo, egui::Button::new(text("Redo")))
            .on_hover_text(REDO_HINT)
            .clicked()
        {
            events.push(Event::Toolbar(ToolbarClick::Redo));
        }
        ui.add_space(12.0);
        if ui.button(text("Settings")).clicked() {
            events.push(Event::Toolbar(ToolbarClick::Settings));
        }
    });
}

/// A square of the colour, framed when chosen.
fn swatch(ui: &mut egui::Ui, color: Color, chosen: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::Vec2::splat(SWATCH), egui::Sense::click());
    let painter = ui.painter();
    painter.rect_filled(rect.shrink(2.0), 3.0, color32(color));
    let frame = if chosen {
        Stroke::new(2.0, HIGHLIGHT)
    } else {
        Stroke::new(1.0, DIM_TEXT)
    };
    painter.rect_stroke(rect, 3.0, frame, egui::StrokeKind::Inside);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, chosen, color.name())
    });
    response
}

/// Draws the edits over the image of `width` × `height` pixels drawn turned
/// by `rotation` into `full`.
pub fn paint_edits(
    painter: &egui::Painter,
    edits: &EditsView,
    (width, height): (u32, u32),
    rotation: Rotation,
    full: egui::Rect,
) {
    let (w, h) = (width as f32, height as f32);
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let to_screen = |x: f32, y: f32| {
        let (u, v) = turned(rotation, x / w, y / h);
        egui::pos2(
            full.min.x + u * full.width(),
            full.min.y + v * full.height(),
        )
    };
    for shape in edits.shapes.iter().chain(&edits.drawing) {
        let Some(outline) = shape.outline(width, height) else {
            continue;
        };
        let fill = color32(outline.color);
        for polygon in outline.polygons {
            let points = polygon.iter().map(|p| to_screen(p.x, p.y)).collect();
            painter.add(egui::Shape::convex_polygon(points, fill, Stroke::NONE));
        }
    }
}

const SAVE_FILL: Color32 = Color32::from_rgb(0xbf, 0xe6, 0xb4);
const SAVE_TEXT: Color32 = Color32::from_rgb(0x17, 0x3d, 0x12);
const SAVE_EDGE: Color32 = Color32::from_rgb(0x5b, 0xa8, 0x4a);
const OVERWRITE_FILL: Color32 = Color32::from_rgb(0xf6, 0xc2, 0xbd);
const OVERWRITE_TEXT: Color32 = Color32::from_rgb(0x5a, 0x14, 0x10);
const OVERWRITE_EDGE: Color32 = Color32::from_rgb(0xd9, 0x6a, 0x5e);
const PLAIN_FILL: Color32 = Color32::from_rgb(0xe4, 0xe8, 0xee);
const PLAIN_EDGE: Color32 = Color32::from_rgb(0xcf, 0xd6, 0xdf);
const INK: Color32 = Color32::from_rgb(0x1d, 0x27, 0x33);
const SUBTLE_INK: Color32 = Color32::from_rgb(0x4a, 0x57, 0x68);
const DIALOG_ERROR: Color32 = Color32::from_rgb(0xb0, 0x24, 0x1c);

fn dialog_button(text: &str, fg: Color32, fill: Color32, edge: Stroke) -> egui::Button<'_> {
    egui::Button::new(RichText::new(text).color(fg).strong().size(15.0))
        .fill(fill)
        .stroke(edge)
        .corner_radius(8.0)
        .min_size(vec2(0.0, 36.0))
}

/// The save dialog over the dimmed window: what becomes of the edits of
/// `file`. Overwrite stands apart on the left as the destructive choice. A
/// key typed into the name field reaches the core from here, since the field
/// keeps it from [`super::input_events`].
pub fn show_save_dialog(
    ctx: &egui::Context,
    file: &str,
    dialog: &SaveDialog,
    events: &mut Vec<Event>,
) {
    let frame = egui::Frame::window(&ctx.global_style())
        .fill(Color32::WHITE)
        .inner_margin(28.0)
        .corner_radius(14.0);
    let modal = egui::Modal::new(egui::Id::new("save-edits"))
        .frame(frame)
        .backdrop_color(Color32::from_black_alpha(160))
        .show(ctx, |ui| {
            ui.set_width(620.0);
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(112.0, 112.0), egui::Sense::hover());
                panic_cat(ui.painter(), rect);
                ui.add_space(20.0);
                ui.vertical(|ui| {
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(format!("Save your edits of \u{201c}{file}\u{201d}?"))
                            .size(20.0)
                            .strong()
                            .color(INK),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(
                            "The arrows are only on screen so far. Undo drops them for good.",
                        )
                        .size(14.0)
                        .color(SUBTLE_INK),
                    );
                });
            });
            ui.add_space(18.0);
            ui.horizontal(|ui| {
                let label = ui.label(RichText::new("New file").color(INK).size(14.0));
                let mut name = dialog.name.clone();
                let field = ui
                    .add_enabled(
                        !dialog.busy,
                        egui::TextEdit::singleline(&mut name)
                            .desired_width(360.0)
                            .text_color(INK)
                            .background_color(PLAIN_FILL),
                    )
                    .labelled_by(label.id);
                if name != dialog.name {
                    events.push(Event::SaveName(name));
                }
                if field.lost_focus() {
                    let (enter, escape) = ui.input(|i| {
                        (
                            i.key_pressed(egui::Key::Enter),
                            i.key_pressed(egui::Key::Escape),
                        )
                    });
                    if enter {
                        events.push(Event::SaveChoice(SaveChoice::SaveAsNew));
                    } else if escape {
                        events.push(Event::SaveChoice(SaveChoice::Back));
                    }
                }
            });
            if let Some(reason) = &dialog.overwrite_refused {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!("Overwrite is not possible: {reason}."))
                        .size(13.0)
                        .color(SUBTLE_INK),
                );
            }
            if let Some(error) = &dialog.error {
                ui.add_space(6.0);
                ui.label(RichText::new(error).size(13.0).color(DIALOG_ERROR));
            }
            ui.add_space(20.0);
            let enabled = !dialog.busy;
            let mut choose = |ui: &mut egui::Ui, on: bool, button, choice| {
                if ui.add_enabled(on, button).clicked() {
                    events.push(Event::SaveChoice(choice));
                }
            };
            ui.horizontal(|ui| {
                choose(
                    ui,
                    enabled && dialog.overwrite_refused.is_none(),
                    dialog_button(
                        "Overwrite",
                        OVERWRITE_TEXT,
                        OVERWRITE_FILL,
                        Stroke::new(1.0, OVERWRITE_EDGE),
                    ),
                    SaveChoice::Overwrite,
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    choose(
                        ui,
                        enabled,
                        dialog_button(
                            "Save as new",
                            SAVE_TEXT,
                            SAVE_FILL,
                            Stroke::new(2.0, SAVE_EDGE),
                        ),
                        SaveChoice::SaveAsNew,
                    );
                    let plain =
                        |text| dialog_button(text, INK, PLAIN_FILL, Stroke::new(1.0, PLAIN_EDGE));
                    choose(ui, enabled, plain("Copy to clipboard"), SaveChoice::Copy);
                    choose(ui, enabled, plain("Undo edits"), SaveChoice::Discard);
                    choose(ui, enabled, plain("Back to editing"), SaveChoice::Back);
                });
            });
            ui.add_space(10.0);
            let hint = if dialog.busy {
                "Saving\u{2026}"
            } else {
                "Enter: save as new   \u{b7}   Esc: back to editing"
            };
            ui.label(RichText::new(hint).size(12.0).color(SUBTLE_INK));
        });
    // A click on the dimmed window goes back to editing; Esc is the core's.
    if modal.backdrop_response.clicked() {
        events.push(Event::SaveChoice(SaveChoice::Back));
    }
}

/// pixpixpix's own panicked cat: a grey tabby holding its head with both
/// paws, one ear bent, spiral eyes and its tail standing on end, drawn into
/// `rect`.
pub fn panic_cat(painter: &egui::Painter, rect: egui::Rect) {
    use egui::{Pos2, Shape};
    let s = rect.width().min(rect.height()) / 120.0;
    let o = rect.min;
    let p = |x: f32, y: f32| Pos2::new(o.x + x * s, o.y + y * s);
    let ink = Color32::from_rgb(0x22, 0x26, 0x30);
    let fur = Color32::from_rgb(0x9a, 0xa4, 0xb4);
    let belly = Color32::from_rgb(0xd8, 0xde, 0xe6);
    let stripe = Color32::from_rgb(0x5c, 0x66, 0x78);
    let pink = Color32::from_rgb(0xf0, 0x9c, 0xb0);
    let outline = Stroke::new(3.0 * s, ink);
    let ellipse = |cx: f32, cy: f32, rx: f32, ry: f32, deg: f32| -> Vec<Pos2> {
        let (sin, cos) = deg.to_radians().sin_cos();
        (0..32)
            .map(|i| {
                let a = i as f32 / 32.0 * std::f32::consts::TAU;
                let (x, y) = (rx * a.cos(), ry * a.sin());
                p(cx + x * cos - y * sin, cy + x * sin + y * cos)
            })
            .collect()
    };
    // A bristling tail behind, standing up on the right.
    let tail: Vec<Pos2> = [
        (96.0, 104.0),
        (104.0, 86.0),
        (101.0, 80.0),
        (108.0, 74.0),
        (104.0, 66.0),
        (112.0, 60.0),
        (106.0, 52.0),
        (114.0, 44.0),
        (104.0, 46.0),
        (98.0, 58.0),
        (100.0, 66.0),
        (94.0, 74.0),
        (96.0, 82.0),
        (88.0, 100.0),
    ]
    .iter()
    .map(|(x, y)| p(*x, *y))
    .collect();
    painter.add(Shape::closed_line(tail, outline));
    // The body, sitting.
    painter.add(Shape::convex_polygon(
        ellipse(60.0, 100.0, 30.0, 18.0, 0.0),
        fur,
        outline,
    ));
    painter.add(Shape::convex_polygon(
        ellipse(60.0, 102.0, 15.0, 12.0, 0.0),
        belly,
        Stroke::NONE,
    ));
    // Arms up behind the head, to the paws.
    for (a, b) in [((26.0, 34.0), (34.0, 92.0)), ((94.0, 34.0), (86.0, 92.0))] {
        painter.line_segment([p(a.0, a.1), p(b.0, b.1)], Stroke::new(8.0 * s, ink));
        painter.line_segment([p(a.0, a.1), p(b.0, b.1)], Stroke::new(5.0 * s, fur));
    }
    // Ears: the left one stands, the right one is bent over.
    painter.add(Shape::convex_polygon(
        vec![p(30.0, 14.0), p(26.0, 46.0), p(50.0, 34.0)],
        fur,
        outline,
    ));
    painter.add(Shape::convex_polygon(
        vec![p(32.0, 24.0), p(30.0, 40.0), p(42.0, 34.0)],
        pink,
        Stroke::NONE,
    ));
    painter.add(Shape::convex_polygon(
        vec![p(70.0, 32.0), p(96.0, 26.0), p(100.0, 40.0), p(90.0, 44.0)],
        fur,
        outline,
    ));
    // The head.
    painter.add(Shape::convex_polygon(
        ellipse(60.0, 60.0, 36.0, 30.0, 0.0),
        fur,
        outline,
    ));
    // Tabby stripes on the forehead.
    for x in [52.0, 60.0, 68.0] {
        painter.line_segment([p(x, 32.0), p(x, 41.0)], Stroke::new(3.0 * s, stripe));
    }
    // Spiral eyes.
    for cx in [45.0, 75.0] {
        painter.circle(p(cx, 56.0), 10.0 * s, Color32::WHITE, outline);
        let spiral: Vec<Pos2> = (0..40)
            .map(|i| {
                let t = i as f32 / 40.0;
                let a = t * std::f32::consts::TAU * 2.5;
                let r = 1.0 + t * 7.0;
                p(cx + r * a.cos(), 56.0 + r * a.sin())
            })
            .collect();
        painter.add(Shape::line(spiral, Stroke::new(1.6 * s, ink)));
    }
    // A screaming square mouth with a tongue.
    painter.add(Shape::convex_polygon(
        vec![p(53.0, 70.0), p(67.0, 70.0), p(65.0, 84.0), p(55.0, 84.0)],
        Color32::from_rgb(0x4a, 0x18, 0x22),
        Stroke::new(2.5 * s, ink),
    ));
    painter.add(Shape::convex_polygon(
        ellipse(60.0, 81.0, 4.5, 2.5, 0.0),
        pink,
        Stroke::NONE,
    ));
    // Paws clutching the top of the head.
    for (cx, deg) in [(30.0, 30.0), (90.0, -30.0)] {
        painter.add(Shape::convex_polygon(
            ellipse(cx, 30.0, 10.0, 13.0, deg),
            belly,
            outline,
        ));
    }
    // Shock lines around the head.
    for (a, b) in [
        ((14.0, 18.0), (6.0, 10.0)),
        ((60.0, 8.0), (60.0, 0.0)),
        ((106.0, 18.0), (114.0, 10.0)),
        ((8.0, 60.0), (0.0, 60.0)),
    ] {
        painter.line_segment([p(a.0, a.1), p(b.0, b.1)], Stroke::new(2.5 * s, ink));
    }
}
