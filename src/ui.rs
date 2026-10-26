//! The egui layer: draws the core's [`View`] and turns input into core
//! [`Event`]s. It holds no behaviour of its own.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use egui::{Color32, ColorImage, RichText, TextureHandle, TextureOptions};

use crate::core::{
    DirColumn, Event, Focus, Key, KeyLine, KeyPress, Notice, Points, Screen, Thumb, View, Zoom,
};
use crate::decode::DecodedImage;
use crate::folder;

const BACKGROUND: Color32 = Color32::from_rgb(0x12, 0x12, 0x14);
const TEXT: Color32 = Color32::from_rgb(0xe8, 0xe8, 0xea);
const ERROR_TEXT: Color32 = Color32::from_rgb(0xff, 0xb4, 0xa8);
const DIM_TEXT: Color32 = Color32::from_rgb(0x9a, 0x9a, 0xa0);
const NOTICE_TEXT: Color32 = Color32::from_rgb(0xff, 0xd5, 0x80);
const CURRENT_MARK: Color32 = Color32::from_rgb(0xff, 0xd5, 0x80);
const BROKEN_TILE: Color32 = Color32::from_rgb(0x3a, 0x1e, 0x1c);
/// The highlighted folder of the focused dir column, and its text.
const HIGHLIGHT: Color32 = Color32::from_rgb(0xff, 0xd5, 0x80);
const HIGHLIGHT_TEXT: Color32 = Color32::from_rgb(0x12, 0x12, 0x14);
/// The highlighted folder while the image view has the focus.
const HIGHLIGHT_UNFOCUSED: Color32 = Color32::from_rgb(0x3a, 0x3a, 0x40);
/// Width of the dir column.
const DIR_COLUMN_WIDTH: f32 = 220.0;
/// Height of the band's thumbnails.
const THUMB_HEIGHT: f32 = 64.0;
/// Below the error of a file that failed to decode.
const OPEN_HINT: &str = "Press O to open it in another app";

/// GPU textures of decoded images, uploaded once per path.
#[derive(Default)]
pub struct Textures {
    loaded: HashMap<PathBuf, TextureHandle>,
}

impl Textures {
    fn get(&mut self, ctx: &egui::Context, path: &Path, image: &DecodedImage) -> TextureHandle {
        if let Some(t) = self.loaded.get(path) {
            return t.clone();
        }
        let size = [image.width as usize, image.height as usize];
        let color = ColorImage::from_rgba_unmultiplied(size, &image.rgba);
        let handle = ctx.load_texture(folder::file_name(path), color, TextureOptions::LINEAR);
        self.loaded.insert(path.to_path_buf(), handle.clone());
        handle
    }

    /// Drops every texture whose path `keep` rejects.
    pub fn retain(&mut self, keep: impl Fn(&Path) -> bool) {
        self.loaded.retain(|p, _| keep(p));
    }
}

/// The key presses of this frame, as core events. Keys typed into a text field
/// stay with the field.
pub fn input_events(ctx: &egui::Context) -> Vec<Event> {
    if ctx.text_edit_focused() {
        return Vec::new();
    }
    ctx.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => map_key(*key).map(|k| {
                    Event::Key(KeyPress {
                        key: k,
                        command: modifiers.command,
                        shift: modifiers.shift,
                        alt: modifiers.alt,
                    })
                }),
                _ => None,
            })
            .collect()
    })
}

fn map_key(key: egui::Key) -> Option<Key> {
    Some(match key {
        egui::Key::F => Key::F,
        egui::Key::Q => Key::Q,
        egui::Key::Escape => Key::Escape,
        egui::Key::ArrowLeft => Key::Left,
        egui::Key::ArrowRight => Key::Right,
        // `+` is `Shift`+`=` on many layouts.
        egui::Key::Plus | egui::Key::Equals => Key::Plus,
        egui::Key::Minus => Key::Minus,
        egui::Key::Num0 => Key::Num0,
        egui::Key::Num1 => Key::Num1,
        egui::Key::N => Key::N,
        egui::Key::Tab => Key::Tab,
        egui::Key::ArrowUp => Key::Up,
        egui::Key::ArrowDown => Key::Down,
        egui::Key::Enter => Key::Enter,
        egui::Key::Backspace => Key::Backspace,
        egui::Key::Period => Key::Period,
        egui::Key::R => Key::R,
        _ => return None,
    })
}

/// Draws the whole window and returns clicks, wheel, drags and viewport size
/// as core events.
pub fn show(ui: &mut egui::Ui, view: &View, textures: &mut Textures) -> Vec<Event> {
    textures.retain(|p| shown(view, p));
    let mut events = Vec::new();
    egui::Panel::bottom("key_line")
        .resizable(false)
        .show_separator_line(false)
        .frame(egui::Frame::NONE.fill(BACKGROUND).inner_margin(6.0))
        .show(ui, |ui| {
            show_key_line(ui, &view.key_line, view.folder_error.as_deref());
        });
    if !view.band.is_empty() {
        egui::Panel::bottom("band")
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::NONE.fill(BACKGROUND).inner_margin(6.0))
            .show(ui, |ui| show_band(ui, &view.band, textures, &mut events));
    }
    if let Some(column) = &view.dir_column {
        egui::Panel::left("dir_column")
            .resizable(false)
            .exact_size(DIR_COLUMN_WIDTH)
            .frame(egui::Frame::NONE.fill(BACKGROUND).inner_margin(6.0))
            .show(ui, |ui| {
                show_dir_column(ui, column, view.focus == Focus::Dirs)
            });
    }
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(BACKGROUND))
        .show(ui, |ui| {
            let size = ui.max_rect().size();
            let viewport = Points {
                x: size.x,
                y: size.y,
            };
            if viewport != view.viewport {
                events.push(Event::Viewport(viewport));
            }
            match &view.screen {
                Screen::Loading { .. } | Screen::Listing { .. } => {}
                Screen::Image { path, image } => {
                    show_image(ui, path, image, view.zoom, textures, &mut events);
                }
                Screen::Error { path, message } => show_error(ui, path.as_deref(), message),
                Screen::NoImages { folders, .. } => show_no_images(ui, *folders),
                Screen::Searching { .. } => show_status(ui, "Searching…"),
                Screen::NothingBelow { .. } => show_status(ui, "No images below"),
            }
        });
    events
}

/// Whether `path` is drawn by `view`, so its texture is still needed.
fn shown(view: &View, path: &Path) -> bool {
    let current = matches!(&view.screen, Screen::Image { path: p, .. } if p == path);
    current || view.band.iter().any(|t| t.path == path)
}

/// The cached files as small thumbs, broken ones as a marked tile, the current
/// one framed; a click on a thumb asks the core to show it.
fn show_band(ui: &mut egui::Ui, band: &[Thumb], textures: &mut Textures, events: &mut Vec<Event>) {
    ui.horizontal(|ui| {
        for thumb in band {
            let name = folder::file_name(&thumb.path);
            let (rect, response, label) = match &thumb.image {
                Some(image) => {
                    let texture = textures.get(ui.ctx(), &thumb.path, image);
                    let pixels = egui::vec2(image.width as f32, image.height as f32);
                    let size = fit(pixels, egui::vec2(THUMB_HEIGHT * 2.0, THUMB_HEIGHT));
                    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
                    egui::Image::new(&texture).paint_at(ui, rect);
                    (rect, response, name)
                }
                None => {
                    let size = egui::Vec2::splat(THUMB_HEIGHT);
                    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
                    let painter = ui.painter();
                    painter.rect_filled(rect, 2.0, BROKEN_TILE);
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "!",
                        egui::FontId::proportional(32.0),
                        ERROR_TEXT,
                    );
                    (rect, response, format!("{name} (error)"))
                }
            };
            if thumb.current {
                ui.painter().rect_stroke(
                    rect.expand(2.0),
                    2.0,
                    egui::Stroke::new(2.0, CURRENT_MARK),
                    egui::StrokeKind::Outside,
                );
            }
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::Button,
                    true,
                    thumb.current,
                    label.clone(),
                )
            });
            if response.clicked() {
                events.push(Event::Show(thumb.path.clone()));
            }
        }
    });
}

/// The current folder's name over its subfolders, the highlighted one painted
/// brighter while the column has the focus.
fn show_dir_column(ui: &mut egui::Ui, column: &DirColumn, focused: bool) {
    let header = match folder::file_name(&column.dir) {
        name if name.is_empty() => column.dir.display().to_string(),
        name => name,
    };
    ui.label(RichText::new(header).color(TEXT).strong().size(15.0));
    ui.add_space(4.0);
    if let Some(error) = &column.error {
        ui.label(RichText::new(error).color(ERROR_TEXT));
    }
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            let font = egui::FontId::proportional(14.0);
            let row_height = ui.ctx().fonts_mut(|f| f.row_height(&font)) + 6.0;
            for (index, dir) in column.dirs.iter().enumerate() {
                let name = folder::file_name(dir);
                let highlighted = column.highlight == Some(index);
                let size = egui::vec2(ui.available_width(), row_height);
                let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
                let fg = match (highlighted, focused) {
                    (true, true) => {
                        ui.painter().rect_filled(rect, 3.0, HIGHLIGHT);
                        HIGHLIGHT_TEXT
                    }
                    (true, false) => {
                        ui.painter().rect_filled(rect, 3.0, HIGHLIGHT_UNFOCUSED);
                        TEXT
                    }
                    (false, _) => TEXT,
                };
                ui.painter().text(
                    egui::pos2(rect.min.x + 6.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    &name,
                    font.clone(),
                    fg,
                );
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::WidgetType::SelectableLabel,
                        true,
                        highlighted,
                        name.clone(),
                    )
                });
                // Only when needed: scrolling every frame never settles.
                if highlighted && !ui.clip_rect().contains_rect(rect) {
                    ui.scroll_to_rect(rect, None);
                }
            }
        });
}

fn show_key_line(ui: &mut egui::Ui, key_line: &KeyLine, folder_error: Option<&str>) {
    ui.horizontal(|ui| {
        for hint in &key_line.keys {
            ui.label(RichText::new(hint.keys).color(TEXT).strong());
            ui.label(RichText::new(hint.action).color(DIM_TEXT));
            ui.add_space(12.0);
        }
        if let Some(notice) = key_line.notice {
            let text = match notice {
                Notice::FirstImage => "first image",
                Notice::LastImage => "last image",
            };
            ui.label(RichText::new(text).color(NOTICE_TEXT));
        }
        if let Some(error) = folder_error {
            let text = format!("Cannot read the folder: {error}");
            ui.label(RichText::new(text).color(ERROR_TEXT));
        }
    });
}

/// Draws the visible part of the image as the core places it; the wheel over
/// it zooms and dragging pans.
fn show_image(
    ui: &mut egui::Ui,
    path: &Path,
    image: &DecodedImage,
    zoom: Zoom,
    textures: &mut Textures,
    events: &mut Vec<Event>,
) {
    let area = ui.max_rect();
    let response = ui.interact(area, ui.id().with("image"), egui::Sense::drag());
    let drag = response.drag_delta();
    if drag != egui::Vec2::ZERO {
        events.push(Event::Pan(Points {
            x: drag.x,
            y: drag.y,
        }));
    }
    if let Some(pointer) = ui.ctx().pointer_hover_pos()
        && ui.rect_contains_pointer(area)
    {
        let delta = ui.input(|i| i.smooth_scroll_delta.y);
        if delta != 0.0 {
            let center = area.center();
            events.push(Event::Wheel {
                delta,
                pointer: Points {
                    x: pointer.x - center.x,
                    y: pointer.y - center.y,
                },
            });
        }
    }

    let viewport = Points {
        x: area.width(),
        y: area.height(),
    };
    let placed = zoom.placement(image, viewport);
    let full = egui::Rect::from_min_size(
        egui::pos2(area.min.x + placed.left, area.min.y + placed.top),
        egui::vec2(placed.width, placed.height),
    );
    let shown = full.intersect(area);
    if !shown.is_positive() {
        return;
    }
    // The part of the texture inside the area.
    let uv = egui::Rect::from_min_max(
        egui::pos2(
            (shown.min.x - full.min.x) / full.width(),
            (shown.min.y - full.min.y) / full.height(),
        ),
        egui::pos2(
            (shown.max.x - full.min.x) / full.width(),
            (shown.max.y - full.min.y) / full.height(),
        ),
    );
    let texture = textures.get(ui.ctx(), path, image);
    let widget = egui::Image::new(&texture)
        .uv(uv)
        .fit_to_exact_size(shown.size())
        .alt_text(folder::file_name(path));
    ui.put(shown, widget);
}

/// The largest size with the image's aspect ratio that fits `area`, never
/// larger than the image itself.
fn fit(image: egui::Vec2, area: egui::Vec2) -> egui::Vec2 {
    if image.x <= 0.0 || image.y <= 0.0 {
        return egui::Vec2::ZERO;
    }
    let scale = (area.x / image.x).min(area.y / image.y).min(1.0);
    egui::vec2(image.x * scale, image.y * scale)
}

fn show_error(ui: &mut egui::Ui, path: Option<&Path>, message: &str) {
    ui.centered_and_justified(|ui| {
        ui.vertical_centered(|ui| {
            if let Some(path) = path {
                ui.label(
                    RichText::new(folder::file_name(path))
                        .color(TEXT)
                        .size(18.0),
                );
            }
            ui.label(RichText::new(message).color(ERROR_TEXT).size(15.0));
            if path.is_some() {
                ui.label(RichText::new(OPEN_HINT).color(DIM_TEXT));
            }
        });
    });
}

/// The placeholder of a previewed folder without media, with its subfolder
/// count once known.
fn show_no_images(ui: &mut egui::Ui, folders: Option<usize>) {
    let text = match folders {
        None => "No images".to_owned(),
        Some(1) => "No images · 1 folder".to_owned(),
        Some(n) => format!("No images · {n} folders"),
    };
    show_status(ui, &text);
}

/// A dim line in the middle of the main area.
fn show_status(ui: &mut egui::Ui, text: &str) {
    ui.centered_and_justified(|ui| {
        ui.label(RichText::new(text).color(DIM_TEXT).size(15.0));
    });
}
