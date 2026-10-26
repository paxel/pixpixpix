//! The egui layer: draws the core's [`View`] and turns input into core
//! [`Event`]s. It holds no behaviour of its own.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use egui::{Color32, ColorImage, RichText, TextureHandle, TextureOptions};

use crate::core::{Event, Key, KeyPress, Screen, View};
use crate::decode::DecodedImage;
use crate::folder;

const BACKGROUND: Color32 = Color32::from_rgb(0x12, 0x12, 0x14);
const TEXT: Color32 = Color32::from_rgb(0xe8, 0xe8, 0xea);
const ERROR_TEXT: Color32 = Color32::from_rgb(0xff, 0xb4, 0xa8);

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
        _ => return None,
    })
}

/// Draws the whole window.
pub fn show(ui: &mut egui::Ui, view: &View, textures: &mut Textures) {
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(BACKGROUND))
        .show(ui, |ui| match &view.screen {
            Screen::Loading { .. } => {}
            Screen::Image { path, image } => show_image(ui, path, image, textures),
            Screen::Error { path, message } => show_error(ui, path.as_deref(), message),
        });
}

fn show_image(ui: &mut egui::Ui, path: &Path, image: &DecodedImage, textures: &mut Textures) {
    let texture = textures.get(ui.ctx(), path, image);
    let area = ui.max_rect();
    let size = fit(
        egui::vec2(image.width as f32, image.height as f32),
        area.size(),
    );
    let rect = egui::Rect::from_center_size(area.center(), size);
    let widget = egui::Image::new(&texture)
        .fit_to_exact_size(size)
        .alt_text(folder::file_name(path));
    ui.put(rect, widget);
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
