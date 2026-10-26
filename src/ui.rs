//! The egui layer: draws the core's [`View`] and turns input into core
//! [`Event`]s. It holds no behaviour of its own.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{Color32, ColorImage, RichText, TextureHandle, TextureOptions};

use crate::core::{
    DirColumn, EditsView, Event, Focus, InnerImages, Key, KeyPress, MotionKind, Notice, Points,
    Screen, Thumb, Video, VideoStep, View, Zoom,
};
use crate::decode::DecodedImage;
use crate::folder;
use crate::rotate::Rotation;
use crate::settings::{self, Settings};

mod overview;
mod tools;

const BACKGROUND: Color32 = Color32::from_rgb(0x12, 0x12, 0x14);
const TEXT: Color32 = Color32::from_rgb(0xe8, 0xe8, 0xea);
const ERROR_TEXT: Color32 = Color32::from_rgb(0xff, 0xb4, 0xa8);
const DIM_TEXT: Color32 = Color32::from_rgb(0x9a, 0x9a, 0xa0);
const NOTICE_TEXT: Color32 = Color32::from_rgb(0xff, 0xd5, 0x80);
const CURRENT_MARK: Color32 = Color32::from_rgb(0xff, 0xd5, 0x80);
/// The highlighted thumb while the band has the focus, around the current mark.
const BAND_HIGHLIGHT: Color32 = Color32::from_rgb(0x7c, 0xc4, 0xff);
const BROKEN_TILE: Color32 = Color32::from_rgb(0x3a, 0x1e, 0x1c);
/// The badge of an animated or multi-image thumb and its marks.
const ANIMATED_BADGE: Color32 = Color32::from_rgba_premultiplied(0x12, 0x12, 0x14, 0xc0);
const ANIMATED_MARK: Color32 = Color32::from_rgb(0xe8, 0xe8, 0xea);
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
/// The label of the cache size in the settings screen.
const CACHE_LABEL: &str = "Cached images per side";
/// The labels of the external apps in the settings screen.
const IMAGE_APP_LABEL: &str = "App for images";
const VIDEO_APP_LABEL: &str = "App for videos";
/// How the external apps are given, below their fields.
const APP_HINT: &str = if cfg!(target_os = "macos") {
    "An application name such as Preview or VLC, opened with open -a. Empty: the system default."
} else {
    "A command such as gwenview or vlc, the file is added last. Empty: the system default."
};

/// GPU textures of decoded images, uploaded once per path and decode, and
/// once per frame of an animation; a file decoded again, after a rotation was
/// written, is uploaded again.
#[derive(Default)]
pub struct Textures {
    loaded: HashMap<PathBuf, Vec<Uploaded>>,
}

/// A texture and the pixels it was made from.
type Uploaded = (Arc<[u8]>, TextureHandle);

impl Textures {
    fn get(&mut self, ctx: &egui::Context, path: &Path, image: &DecodedImage) -> TextureHandle {
        let loaded = self.loaded.entry(path.to_path_buf()).or_default();
        if let Some((_, t)) = loaded
            .iter()
            .find(|(pixels, _)| Arc::ptr_eq(pixels, &image.rgba))
        {
            return t.clone();
        }
        let size = [image.width as usize, image.height as usize];
        let color = ColorImage::from_rgba_unmultiplied(size, &image.rgba);
        let handle = ctx.load_texture(folder::file_name(path), color, TextureOptions::LINEAR);
        loaded.push((Arc::clone(&image.rgba), handle.clone()));
        handle
    }

    /// Drops every texture whose path `keep` rejects, and those of pixels
    /// nobody but the textures holds any more: an earlier decode or the
    /// frames of an animation left.
    pub fn retain(&mut self, keep: impl Fn(&Path) -> bool) {
        self.loaded.retain(|p, loaded| {
            loaded.retain(|(pixels, _)| Arc::strong_count(pixels) > 1);
            keep(p) && !loaded.is_empty()
        });
    }
}

/// The key presses of this frame and a press of the window's close button,
/// as core events. Keys typed into a text field stay with the field.
pub fn input_events(ctx: &egui::Context) -> Vec<Event> {
    let mut events = Vec::new();
    if ctx.input(|i| i.viewport().close_requested()) {
        events.push(Event::CloseRequested);
    }
    if ctx.text_edit_focused() {
        return events;
    }
    events.extend(ctx.input(|i| {
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
            .collect::<Vec<_>>()
    }));
    events
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
        egui::Key::B => Key::B,
        egui::Key::S => Key::S,
        egui::Key::O => Key::O,
        egui::Key::L => Key::L,
        egui::Key::X => Key::X,
        egui::Key::C => Key::C,
        egui::Key::Space => Key::Space,
        egui::Key::M => Key::M,
        egui::Key::G => Key::G,
        egui::Key::P => Key::P,
        egui::Key::Z => Key::Z,
        _ => return None,
    })
}

/// Draws the whole window and returns clicks, wheel, drags and viewport size
/// as core events.
pub fn show(ui: &mut egui::Ui, view: &View, textures: &mut Textures) -> Vec<Event> {
    textures.retain(|p| shown(view, p));
    if let Some(wait) = view.next_frame_in {
        ui.ctx().request_repaint_after(wait);
    }
    let mut events = Vec::new();
    egui::Panel::bottom("key_line")
        .resizable(false)
        .show_separator_line(false)
        .frame(egui::Frame::NONE.fill(BACKGROUND).inner_margin(6.0))
        .show(ui, |ui| {
            show_key_line(ui, view);
        });
    if !view.band.is_empty() && view.overview.is_none() {
        egui::Panel::bottom("band")
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::NONE.fill(BACKGROUND).inner_margin(6.0))
            .show(ui, |ui| {
                show_band(ui, &view.band, view.band_highlight, textures, &mut events)
            });
    }
    if let Some(toolbar) = view.toolbar
        && view.overview.is_none()
    {
        egui::Panel::top("toolbar")
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::NONE.fill(BACKGROUND).inner_margin(6.0))
            .show(ui, |ui| {
                tools::show_toolbar(ui, toolbar, view.edits.as_ref(), &mut events);
            });
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
            if let Some(gallery) = &view.overview {
                let side = view.settings.thumbnail_size;
                overview::show_overview(ui, gallery, side, textures, &mut events);
                return;
            }
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
                    let shown = Shown {
                        path,
                        image,
                        rotation: view.rotation,
                        clickable: view.video.is_some(),
                        drawing: view.toolbar.is_some(),
                        edits: view.edits.as_ref(),
                    };
                    show_image(ui, &shown, view.zoom, textures, &mut events);
                    if let Some(video) = &view.video {
                        show_video(ui, video, &mut events);
                    }
                }
                Screen::Error { path, message } => show_error(ui, path.as_deref(), message),
                Screen::NoImages { folders, .. } => show_no_images(ui, *folders),
                Screen::Searching { .. } => show_status(ui, "Searching…"),
                Screen::NothingBelow { .. } => show_status(ui, "No images below"),
            }
            if let Some(inner) = view.inner {
                show_inner(ui, inner);
            }
            if let Some(kind) = view.motion {
                show_motion(ui, kind);
            }
        });
    if let (Some(dialog), Screen::Image { path, .. }) = (&view.save_dialog, &view.screen) {
        tools::show_save_dialog(ui.ctx(), &folder::file_name(path), dialog, &mut events);
    }
    if view.settings_open {
        show_settings(
            ui.ctx(),
            &view.settings,
            view.settings_error.as_deref(),
            &mut events,
        );
    }
    events
}

/// Whether `path` is drawn by `view`, so its texture is still needed. While
/// the overview is open every texture may be; the thumbnails the core drops
/// go with their pixels.
fn shown(view: &View, path: &Path) -> bool {
    let current = matches!(&view.screen, Screen::Image { path: p, .. } if p == path);
    current || view.overview.is_some() || view.band.iter().any(|t| t.path == path)
}

/// The cached files as small thumbs, broken ones as a marked tile, animated
/// ones with a badge, the current one framed and the highlighted one framed
/// wider; a click on a thumb asks the core to show it.
fn show_band(
    ui: &mut egui::Ui,
    band: &[Thumb],
    highlight: Option<usize>,
    textures: &mut Textures,
    events: &mut Vec<Event>,
) {
    ui.horizontal(|ui| {
        for (index, thumb) in band.iter().enumerate() {
            let name = folder::file_name(&thumb.path);
            let (rect, response, label) = match &thumb.image {
                Some(image) => {
                    let texture = textures.get(ui.ctx(), &thumb.path, image);
                    let pixels = egui::vec2(image.width as f32, image.height as f32);
                    let size = fit(pixels, egui::vec2(THUMB_HEIGHT * 2.0, THUMB_HEIGHT));
                    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
                    egui::Image::new(&texture).paint_at(ui, rect);
                    if thumb.video {
                        paint_play(ui.painter(), rect.center(), THUMB_HEIGHT / 5.0);
                        (rect, response, format!("{name} (video)"))
                    } else if thumb.animated {
                        paint_animated_badge(ui.painter(), rect);
                        (rect, response, format!("{name} (animated)"))
                    } else if let Some(kind) = thumb.motion {
                        paint_motion_badge(ui.painter(), rect, kind);
                        let kind = motion_name(kind).to_lowercase();
                        (rect, response, format!("{name} ({kind})"))
                    } else if thumb.images > 1 {
                        paint_images_badge(ui.painter(), rect, thumb.images);
                        (rect, response, format!("{name} ({} images)", thumb.images))
                    } else {
                        (rect, response, name)
                    }
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
            let highlighted = highlight == Some(index);
            if highlighted {
                ui.painter().rect_stroke(
                    rect.expand(4.0),
                    3.0,
                    egui::Stroke::new(2.0, BAND_HIGHLIGHT),
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
            ui.ctx()
                .accesskit_node_builder(response.id, |node| node.set_selected(highlighted));
            if response.clicked() {
                events.push(Event::Show(thumb.path.clone()));
            }
        }
    });
}

/// The badge of an animated thumb in its bottom left corner: a row of film
/// frames.
fn paint_animated_badge(painter: &egui::Painter, rect: egui::Rect) {
    let badge = egui::Rect::from_min_size(
        egui::pos2(rect.min.x + 3.0, rect.max.y - 13.0),
        egui::vec2(22.0, 10.0),
    );
    painter.rect_filled(badge, 2.0, ANIMATED_BADGE);
    for offset in [2.0, 8.5, 15.0] {
        let left = badge.min.x + offset;
        let frame =
            egui::Rect::from_min_size(egui::pos2(left, badge.min.y + 2.0), egui::vec2(5.0, 6.0));
        painter.rect_filled(frame, 1.0, ANIMATED_MARK);
    }
}

/// A play triangle in a dark disc of `radius` around `center`.
fn paint_play(painter: &egui::Painter, center: egui::Pos2, radius: f32) {
    painter.circle_filled(center, radius, ANIMATED_BADGE);
    let side = radius * 0.5;
    let points = vec![
        egui::pos2(center.x - side * 0.6, center.y - side),
        egui::pos2(center.x + side, center.y),
        egui::pos2(center.x - side * 0.6, center.y + side),
    ];
    painter.add(egui::Shape::convex_polygon(
        points,
        ANIMATED_MARK,
        egui::Stroke::NONE,
    ));
}

/// The badge of a multi-image thumb in its bottom left corner: how many
/// images the file holds.
fn paint_images_badge(painter: &egui::Painter, rect: egui::Rect, images: usize) {
    let badge = egui::Rect::from_min_size(
        egui::pos2(rect.min.x + 3.0, rect.max.y - 15.0),
        egui::vec2(22.0, 12.0),
    );
    painter.rect_filled(badge, 2.0, ANIMATED_BADGE);
    painter.text(
        badge.center(),
        egui::Align2::CENTER_CENTER,
        images.to_string(),
        egui::FontId::proportional(10.0),
        ANIMATED_MARK,
    );
}

/// The badge of a Live Photo or Motion Photo thumb in its bottom left corner.
fn paint_motion_badge(painter: &egui::Painter, rect: egui::Rect, kind: MotionKind) {
    let text = match kind {
        MotionKind::LivePhoto => "LIVE",
        MotionKind::MotionPhoto => "MOTION",
    };
    let badge = egui::Rect::from_min_size(
        egui::pos2(rect.min.x + 3.0, rect.max.y - 15.0),
        egui::vec2(40.0, 12.0),
    );
    painter.rect_filled(badge, 2.0, ANIMATED_BADGE);
    painter.text(
        badge.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(9.0),
        ANIMATED_MARK,
    );
}

/// What a photo with a video of the moment is called.
fn motion_name(kind: MotionKind) -> &'static str {
    match kind {
        MotionKind::LivePhoto => "Live Photo",
        MotionKind::MotionPhoto => "Motion Photo",
    }
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

/// The settings screen over the dimmed window; a changed value is reported
/// with all settings. Closing it is the core's business.
fn show_settings(
    ctx: &egui::Context,
    values: &Settings,
    error: Option<&str>,
    events: &mut Vec<Event>,
) {
    egui::Modal::new(egui::Id::new("settings"))
        .frame(egui::Frame::NONE.fill(BACKGROUND).inner_margin(16.0))
        .show(ctx, |ui| {
            ui.label(RichText::new("Settings").color(TEXT).strong().size(18.0));
            ui.add_space(8.0);
            let mut changed = values.clone();
            ui.add(
                egui::Slider::new(&mut changed.cache_per_side, settings::CACHE_PER_SIDE)
                    .text(RichText::new(CACHE_LABEL).color(TEXT)),
            );
            ui.add_space(8.0);
            app_field(ui, IMAGE_APP_LABEL, &mut changed.image_app);
            app_field(ui, VIDEO_APP_LABEL, &mut changed.video_app);
            ui.label(RichText::new(APP_HINT).color(DIM_TEXT));
            if changed != *values {
                events.push(Event::SettingsChanged(changed));
            }
            if let Some(error) = error {
                ui.add_space(8.0);
                ui.label(RichText::new(error).color(ERROR_TEXT));
            }
        });
}

/// A labelled one-line text field for an external app.
fn app_field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        let label = ui.label(RichText::new(label).color(TEXT));
        ui.text_edit_singleline(value).labelled_by(label.id);
    });
}

fn show_key_line(ui: &mut egui::Ui, view: &View) {
    let key_line = &view.key_line;
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
                Notice::Copied => "copied",
            };
            ui.label(RichText::new(text).color(NOTICE_TEXT));
        }
        if let Some(error) = &view.folder_error {
            let text = format!("Cannot read the folder: {error}");
            ui.label(RichText::new(text).color(ERROR_TEXT));
        }
        if let Some(error) = &view.open_error {
            ui.label(RichText::new(error).color(ERROR_TEXT));
        }
        if let Some(error) = &view.rotation_error {
            ui.label(RichText::new(error).color(ERROR_TEXT));
        }
        if let Some(error) = &view.trash_error {
            ui.label(RichText::new(error).color(ERROR_TEXT));
        }
        if let Some(error) = &view.copy_error {
            ui.label(RichText::new(error).color(ERROR_TEXT));
        }
    });
}

/// The image of the main area and how it is turned.
struct Shown<'a> {
    path: &'a Path,
    image: &'a DecodedImage,
    rotation: Rotation,
    /// A click on it is reported, as for a video.
    clickable: bool,
    /// A drag draws, for the toolbar is shown, instead of panning.
    drawing: bool,
    /// The edits drawn over it.
    edits: Option<&'a EditsView>,
}

/// Draws the visible part of the image, turned, as the core places it, and
/// its edits over it; the wheel over it zooms, dragging pans or draws and a
/// click is reported if wanted.
fn show_image(
    ui: &mut egui::Ui,
    shown: &Shown<'_>,
    zoom: Zoom,
    textures: &mut Textures,
    events: &mut Vec<Event>,
) {
    let area = ui.max_rect();
    let response = ui.interact(area, ui.id().with("image"), egui::Sense::drag());
    let from_area = |pos: egui::Pos2| Points {
        x: pos.x - area.min.x,
        y: pos.y - area.min.y,
    };
    if shown.drawing {
        // The drag starts where the button went down, a little before egui
        // calls it a drag.
        let origin = ui.input(|i| i.pointer.press_origin());
        if response.drag_started()
            && let Some(origin) = origin
        {
            events.push(Event::DrawStart(from_area(origin)));
        }
        if response.dragged()
            && let Some(pos) = response.interact_pointer_pos()
        {
            events.push(Event::DrawMove(from_area(pos)));
        }
        if response.drag_stopped() {
            events.push(Event::DrawEnd);
        }
    } else {
        let drag = response.drag_delta();
        if drag != egui::Vec2::ZERO {
            events.push(Event::Pan(Points {
                x: drag.x,
                y: drag.y,
            }));
        }
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
    let placed = zoom.placement(shown.image, shown.rotation, viewport);
    let full = egui::Rect::from_min_size(
        egui::pos2(area.min.x + placed.left, area.min.y + placed.top),
        egui::vec2(placed.width, placed.height),
    );
    let visible = full.intersect(area);
    if !visible.is_positive() {
        return;
    }
    // The corners of the visible part, each with its spot in the texture.
    let texture = textures.get(ui.ctx(), shown.path, shown.image);
    let mut mesh = egui::Mesh::with_texture(texture.id());
    for corner in [
        visible.left_top(),
        visible.right_top(),
        visible.right_bottom(),
        visible.left_bottom(),
    ] {
        let u = (corner.x - full.min.x) / full.width();
        let v = (corner.y - full.min.y) / full.height();
        mesh.vertices.push(egui::epaint::Vertex {
            pos: corner,
            uv: texture_point(shown.rotation, u, v),
            color: Color32::WHITE,
        });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(mesh);
    if let Some(edits) = shown.edits {
        let size = (shown.image.width, shown.image.height);
        tools::paint_edits(&ui.painter_at(area), edits, size, shown.rotation, full);
    }
    let name = folder::file_name(shown.path);
    let sense = if shown.clickable {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let response = ui.allocate_rect(visible, sense);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Image, true, &name));
    if response.clicked() {
        events.push(Event::Clicked);
    }
}

/// The step buttons of a video not playing, in order.
const VIDEO_STEPS: [(&str, VideoStep); 6] = [
    ("-30s", VideoStep::Seconds(-30)),
    ("-10s", VideoStep::Seconds(-10)),
    ("-1 frame", VideoStep::Frames(-1)),
    ("+1 frame", VideoStep::Frames(1)),
    ("+10s", VideoStep::Seconds(10)),
    ("+30s", VideoStep::Seconds(30)),
];

/// Over a video's frame: a big play triangle and step buttons while it is not
/// playing, which a click plays, and why it cannot be played.
fn show_video(ui: &mut egui::Ui, video: &Video, events: &mut Vec<Event>) {
    let area = ui.max_rect();
    if !video.playing {
        // A row in the middle, above the place of the error.
        let rect = egui::Rect::from_center_size(
            egui::pos2(area.center().x, area.max.y - 78.0),
            egui::vec2(VIDEO_STEPS.len() as f32 * 72.0, 32.0),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            ui.horizontal(|ui| {
                for (label, step) in VIDEO_STEPS {
                    let button = egui::Button::new(RichText::new(label).color(TEXT))
                        .fill(ANIMATED_BADGE)
                        .min_size(egui::vec2(64.0, 28.0));
                    if ui.add(button).clicked() {
                        events.push(Event::StepVideo(step));
                    }
                }
            });
        });
    }
    if !video.playing {
        let radius = (area.width().min(area.height()) / 8.0).clamp(24.0, 64.0);
        let rect = egui::Rect::from_center_size(area.center(), egui::Vec2::splat(radius * 2.0));
        paint_play(ui.painter(), area.center(), radius);
        let response = ui.interact(rect, ui.id().with("play"), egui::Sense::click());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Play"));
        if response.clicked() {
            events.push(Event::Clicked);
        }
    }
    if let Some(error) = &video.error {
        let rect = egui::Rect::from_min_max(egui::pos2(area.min.x, area.max.y - 56.0), area.max);
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(error)
                        .color(ERROR_TEXT)
                        .background_color(ANIMATED_BADGE)
                        .size(15.0),
                );
                ui.label(
                    RichText::new(OPEN_HINT)
                        .color(DIM_TEXT)
                        .background_color(ANIMATED_BADGE),
                );
            });
        });
    }
}

/// Which image of a multi-image file is shown, in the top right corner.
fn show_inner(ui: &mut egui::Ui, inner: InnerImages) {
    let text = format!("{} / {}", inner.index.saturating_add(1), inner.count);
    let area = ui.max_rect();
    let rect = egui::Rect::from_min_size(
        egui::pos2(area.max.x - 80.0, area.min.y + 6.0),
        egui::vec2(74.0, 20.0),
    );
    ui.put(
        rect,
        egui::Label::new(
            RichText::new(text)
                .color(TEXT)
                .background_color(ANIMATED_BADGE),
        )
        .selectable(false),
    );
}

/// That the image shown is a Live Photo or Motion Photo, in the top left
/// corner.
fn show_motion(ui: &mut egui::Ui, kind: MotionKind) {
    let area = ui.max_rect();
    let rect = egui::Rect::from_min_size(
        egui::pos2(area.min.x + 6.0, area.min.y + 6.0),
        egui::vec2(100.0, 20.0),
    );
    ui.put(
        rect,
        egui::Label::new(
            RichText::new(motion_name(kind))
                .color(TEXT)
                .background_color(ANIMATED_BADGE),
        )
        .selectable(false),
    );
}

/// The texture point shown at `(u, v)` of an image turned by `rotation`, all
/// as fractions of width and height.
fn texture_point(rotation: Rotation, u: f32, v: f32) -> egui::Pos2 {
    match rotation {
        Rotation::None => egui::pos2(u, v),
        Rotation::Right => egui::pos2(v, 1.0 - u),
        Rotation::Half => egui::pos2(1.0 - u, 1.0 - v),
        Rotation::Left => egui::pos2(1.0 - v, u),
    }
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
