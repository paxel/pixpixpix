//! The egui layer: draws the core's [`View`] and turns input into core
//! [`Event`]s. It holds no behaviour of its own.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use egui::{Color32, ColorImage, RichText, TextureHandle, TextureOptions};

use crate::core::{Event, Key, KeyLine, KeyPress, Notice, Points, Screen, Thumb, View, Zoom};
use crate::decode::DecodedImage;
use crate::folder;

const BACKGROUND: Color32 = Color32::from_rgb(0x12, 0x12, 0x14);
const TEXT: Color32 = Color32::from_rgb(0xe8, 0xe8, 0xea);
const ERROR_TEXT: Color32 = Color32::from_rgb(0xff, 0xb4, 0xa8);
const DIM_TEXT: Color32 = Color32::from_rgb(0x9a, 0x9a, 0xa0);
const NOTICE_TEXT: Color32 = Color32::from_rgb(0xff, 0xd5, 0x80);
const CURRENT_MARK: Color32 = Color32::from_rgb(0xff, 0xd5, 0x80);
/// Height of the band's thumbnails.
const THUMB_HEIGHT: f32 = 64.0;

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
        .show(ui, |ui| show_key_line(ui, &view.key_line));
    if !view.band.is_empty() {
        egui::Panel::bottom("band")
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::NONE.fill(BACKGROUND).inner_margin(6.0))
            .show(ui, |ui| show_band(ui, &view.band, textures, &mut events));
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
                Screen::Loading { .. } => {}
                Screen::Image { path, image } => {
                    show_image(ui, path, image, view.zoom, textures, &mut events);
                }
                Screen::Error { path, message } => show_error(ui, path.as_deref(), message),
            }
        });
    events
}

/// Whether `path` is drawn by `view`, so its texture is still needed.
fn shown(view: &View, path: &Path) -> bool {
    let current = matches!(&view.screen, Screen::Image { path: p, .. } if p == path);
    current || view.band.iter().any(|t| t.path == path)
}

/// The cached images as small thumbs, the current one framed; a click on a
/// thumb asks the core to show it.
fn show_band(ui: &mut egui::Ui, band: &[Thumb], textures: &mut Textures, events: &mut Vec<Event>) {
    ui.horizontal(|ui| {
        for thumb in band {
            let texture = textures.get(ui.ctx(), &thumb.path, &thumb.image);
            let image = egui::vec2(thumb.image.width as f32, thumb.image.height as f32);
            let size = fit(image, egui::vec2(THUMB_HEIGHT * 2.0, THUMB_HEIGHT));
            let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
            egui::Image::new(&texture).paint_at(ui, rect);
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
                    folder::file_name(&thumb.path),
                )
            });
            if response.clicked() {
                events.push(Event::Show(thumb.path.clone()));
            }
        }
    });
}

fn show_key_line(ui: &mut egui::Ui, key_line: &KeyLine) {
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
        });
    });
}
