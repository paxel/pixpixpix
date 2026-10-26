//! The toolbar, the edits and the export area over the image and the save
//! dialog with its panicked cat.

use egui::{Color32, RichText, Stroke, vec2};

use super::{DIM_TEXT, HIGHLIGHT, TEXT};
use crate::core::{
    Color, EditsView, Event, ExportFormat, ExportView, HANDLE_REACH, Mark, Region, SaveChoice,
    SaveDialog, Shape, Size, Tool, Toolbar, ToolbarClick, degrees, turned,
};
use crate::decode::DecodedImage;
use crate::metadata::Strip;
use crate::rotate::Rotation;
use crate::settings::EXPORT_SIDE;

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

/// One row, wrapped in a narrow window: the tools, the palette and the
/// sizes, or the export tool's choices, undo, redo and settings.
pub fn show_toolbar(
    ui: &mut egui::Ui,
    toolbar: Toolbar,
    edits: Option<&EditsView>,
    export: Option<&ExportView>,
    events: &mut Vec<Event>,
) {
    let text = |s: &str| RichText::new(s).color(TEXT);
    ui.horizontal_wrapped(|ui| {
        for tool in Tool::ALL {
            let button = egui::Button::selectable(toolbar.tool == tool, text(tool.name()));
            if ui.add(button).clicked() {
                events.push(Event::Toolbar(ToolbarClick::Tool(tool)));
            }
        }
        ui.add_space(12.0);
        if let Some(export) = export {
            show_export_choices(ui, export, events);
        } else {
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

/// The export tool's choices: the size in pixels, the ratio lock, the
/// format, the size limit and the button that exports.
fn show_export_choices(ui: &mut egui::Ui, view: &ExportView, events: &mut Vec<Event>) {
    let text = |s: &str| RichText::new(s).color(TEXT);
    let export = view.export;
    let mut number =
        |ui: &mut egui::Ui, label: &str, value: u32, range, click: fn(u32) -> ToolbarClick| {
            let label = ui.label(text(label));
            let mut changed = value;
            ui.add(egui::DragValue::new(&mut changed).range(range))
                .labelled_by(label.id);
            if changed != value {
                events.push(Event::Toolbar(click(changed)));
            }
        };
    number(
        ui,
        "Width",
        export.width,
        EXPORT_SIDE,
        ToolbarClick::ExportWidth,
    );
    number(
        ui,
        "Height",
        export.height,
        EXPORT_SIDE,
        ToolbarClick::ExportHeight,
    );
    // WebP is written lossless only: a limit could not lower its quality.
    if export.format != ExportFormat::WebP {
        number(
            ui,
            "Max KB",
            export.limit_kb.unwrap_or(0),
            0..=u32::MAX,
            |kb| ToolbarClick::ExportLimit(Some(kb)),
        );
    }
    ui.add_space(12.0);
    let lock = egui::Button::selectable(export.locked, text("Lock ratio"));
    if ui
        .add(lock)
        .on_hover_text("Off: any area is stretched to the size")
        .clicked()
    {
        events.push(Event::Toolbar(ToolbarClick::ExportLocked(!export.locked)));
    }
    ui.add_space(12.0);
    for format in ExportFormat::ALL {
        let button = egui::Button::selectable(export.format == format, text(format.name()));
        if ui.add(button).clicked() {
            events.push(Event::Toolbar(ToolbarClick::ExportFormat(format)));
        }
    }
    ui.add_space(12.0);
    if ui
        .add_enabled(
            view.area.is_some(),
            egui::Button::new(text("Export\u{2026}")),
        )
        .clicked()
    {
        events.push(Event::Toolbar(ToolbarClick::Export));
    }
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

/// An image as drawn: the part `region` of it, turned by `rotation`, into
/// `rect`.
pub struct Placed<'a> {
    pub image: &'a DecodedImage,
    pub region: Region,
    pub rotation: Rotation,
    pub rect: egui::Rect,
}

impl Placed<'_> {
    /// Where the image point `(x, y)` shows.
    fn to_screen(&self, x: f32, y: f32) -> egui::Pos2 {
        let region = self.region;
        let (u, v) = turned(
            self.rotation,
            (x - region.left) / region.width(),
            (y - region.top) / region.height(),
        );
        egui::pos2(
            self.rect.min.x + u * self.rect.width(),
            self.rect.min.y + v * self.rect.height(),
        )
    }

    /// Points on screen per image pixel.
    fn scale(&self) -> f32 {
        let width = if self.rotation.sideways() {
            self.region.height()
        } else {
            self.region.width()
        };
        self.rect.width() / width
    }

    /// The screen box of the image box `region`.
    pub fn box_of(&self, region: Region) -> egui::Rect {
        egui::Rect::from_two_pos(
            self.to_screen(region.left, region.top),
            self.to_screen(region.right, region.bottom),
        )
    }
}

fn color_of([r, g, b, a]: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// Draws the edits over the image as [`crate::annotate::render`] draws them
/// into the file: first the pixelations, then the other shapes in order; text
/// being typed shows in its field instead. While cropping, the part kept is
/// framed with its handles.
pub fn paint_edits(painter: &egui::Painter, edits: &EditsView, placed: &Placed<'_>) {
    let region = placed.region;
    if region.width() <= 0.0 || region.height() <= 0.0 {
        return;
    }
    let shapes = || edits.shapes.iter().chain(&edits.drawing);
    for block in shapes().flat_map(|s| s.blocks(placed.image)) {
        let (left, top) = (block.left as f32, block.top as f32);
        let rect = placed.box_of(Region {
            left,
            top,
            right: left + block.width as f32,
            bottom: top + block.height as f32,
        });
        let [r, g, b] = block.rgb;
        painter.rect_filled(rect, 0.0, Color32::from_rgb(r, g, b));
    }
    let scale = placed.scale();
    for shape in edits.shapes.iter() {
        paint_shape(painter, shape, placed, scale);
    }
    if let Some(shape) = &edits.drawing
        && !matches!(shape, Shape::Text { .. })
    {
        paint_shape(painter, shape, placed, scale);
    }
    if let Some(crop) = edits.cropping {
        paint_crop(painter, placed.box_of(crop), true);
    }
}

/// Draws one shape other than a pixelation and a crop.
fn paint_shape(painter: &egui::Painter, shape: &Shape, placed: &Placed<'_>, scale: f32) {
    let (width, height) = (placed.image.width, placed.image.height);
    let screen = |points: &[crate::core::ImagePoint]| -> Vec<egui::Pos2> {
        points.iter().map(|p| placed.to_screen(p.x, p.y)).collect()
    };
    match shape.mark(width, height) {
        Some(Mark::Fill { rgba, polygons }) => {
            for polygon in polygons {
                painter.add(egui::Shape::convex_polygon(
                    screen(&polygon),
                    color_of(rgba),
                    Stroke::NONE,
                ));
            }
        }
        Some(Mark::Line {
            rgba,
            width,
            points,
            closed,
        }) => {
            let stroke = Stroke::new(width * scale, color_of(rgba));
            let points = screen(&points);
            if closed {
                painter.add(egui::Shape::closed_line(points, stroke));
            } else {
                // Round ends and joins, as in the file.
                for point in &points {
                    painter.circle_filled(*point, stroke.width / 2.0, stroke.color);
                }
                painter.add(egui::Shape::line(points, stroke));
            }
        }
        None => {}
    }
    if let Shape::Text {
        at,
        text,
        color,
        size,
        turn,
    } = shape
    {
        let font = egui::FontId::proportional(size.text_height(width, height) * scale);
        let galley = painter.layout_no_wrap(text.clone(), font, color32(*color));
        let angle = (degrees(placed.rotation) - degrees(*turn)).to_radians();
        let at = placed.to_screen(at.x, at.y);
        painter.add(egui::epaint::TextShape::new(at, galley, color32(*color)).with_angle(angle));
    }
}

/// The crop or export box `rect`: the rest dimmed, a frame and its handles,
/// the middles of its `sides` too.
pub fn paint_crop(painter: &egui::Painter, rect: egui::Rect, sides: bool) {
    let clip = painter.clip_rect();
    let dim = Color32::from_black_alpha(140);
    for outside in [
        egui::Rect::from_x_y_ranges(clip.x_range(), clip.min.y..=rect.min.y),
        egui::Rect::from_x_y_ranges(clip.x_range(), rect.max.y..=clip.max.y),
        egui::Rect::from_x_y_ranges(clip.min.x..=rect.min.x, rect.y_range()),
        egui::Rect::from_x_y_ranges(rect.max.x..=clip.max.x, rect.y_range()),
    ] {
        if outside.is_positive() {
            painter.rect_filled(outside, 0.0, dim);
        }
    }
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.5, Color32::WHITE),
        egui::StrokeKind::Middle,
    );
    for x in [rect.min.x, rect.center().x, rect.max.x] {
        for y in [rect.min.y, rect.center().y, rect.max.y] {
            let (middle_x, middle_y) = (x == rect.center().x, y == rect.center().y);
            if (middle_x && middle_y) || ((middle_x || middle_y) && !sides) {
                continue;
            }
            let handle =
                egui::Rect::from_center_size(egui::pos2(x, y), egui::Vec2::splat(HANDLE_REACH));
            painter.rect_filled(handle, 0.0, Color32::WHITE);
            painter.rect_stroke(
                handle,
                0.0,
                Stroke::new(1.0, Color32::BLACK),
                egui::StrokeKind::Inside,
            );
        }
    }
}

/// The field text is typed into where it was placed, in its size and
/// colour; `Enter`, `Esc` or a click elsewhere end it. Keys typed into it
/// stay with it, since the field keeps them from [`super::input_events`].
pub fn show_text_field(
    ctx: &egui::Context,
    edits: &EditsView,
    placed: &Placed<'_>,
    events: &mut Vec<Event>,
) {
    let Some(Shape::Text {
        at,
        text,
        color,
        size,
        ..
    }) = &edits.drawing
    else {
        return;
    };
    let (width, height) = (placed.image.width, placed.image.height);
    let font = egui::FontId::proportional(size.text_height(width, height) * placed.scale());
    egui::Area::new(egui::Id::new("text-tool"))
        .fixed_pos(placed.to_screen(at.x, at.y))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let mut typed = text.clone();
            let shown = ui
                .painter()
                .layout_no_wrap(typed.clone(), font.clone(), color32(*color));
            let field = ui.add(
                egui::TextEdit::singleline(&mut typed)
                    .font(font.clone())
                    .text_color(color32(*color))
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::ZERO)
                    .desired_width(shown.size().x + font.size)
                    .hint_text("Text"),
            );
            if typed != *text {
                events.push(Event::TextEdited(typed));
            }
            if field.lost_focus() {
                events.push(Event::TextDone);
            } else if !field.has_focus() && ui.memory(|m| m.focused().is_none()) {
                field.request_focus();
            }
        });
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
/// `file`. Overwrite stands apart on the left as the destructive choice,
/// absent for a video frame. A
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
                let (title, note) = if let Some(strip) = dialog.strip {
                    match strip {
                        Strip::Location => (
                            format!("Remove the location from \u{201c}{file}\u{201d}?"),
                            "The GPS tags go; the pixels and all other metadata stay.",
                        ),
                        Strip::AllExif => (
                            format!("Remove all EXIF from \u{201c}{file}\u{201d}?"),
                            "The orientation is turned into the pixels first.",
                        ),
                    }
                } else if dialog.export {
                    (
                        format!("Export \u{201c}{file}\u{201d}?"),
                        "The export is a new file, without metadata.",
                    )
                } else {
                    (
                        format!("Save your edits of \u{201c}{file}\u{201d}?"),
                        "The edits are only on screen so far. Undo drops them for good.",
                    )
                };
                ui.vertical(|ui| {
                    ui.add_space(10.0);
                    ui.label(RichText::new(title).size(20.0).strong().color(INK));
                    ui.add_space(6.0);
                    ui.label(RichText::new(note).size(14.0).color(SUBTLE_INK));
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
                let text = if dialog.frame {
                    format!("The frame is saved as a new image: {reason}.")
                } else {
                    format!("Overwrite is not possible: {reason}.")
                };
                ui.label(RichText::new(text).size(13.0).color(SUBTLE_INK));
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
                // A video frame never offers to overwrite the video.
                if !dialog.frame {
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
                }
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
                    if !dialog.export && dialog.strip.is_none() {
                        choose(ui, enabled, plain("Undo edits"), SaveChoice::Discard);
                    }
                    let back = if dialog.strip.is_some() {
                        "Back"
                    } else {
                        "Back to editing"
                    };
                    choose(ui, enabled, plain(back), SaveChoice::Back);
                });
            });
            ui.add_space(10.0);
            let hint = if dialog.busy {
                "Saving\u{2026}"
            } else if dialog.strip.is_some() {
                "Enter: save as new   \u{b7}   Esc: back"
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
