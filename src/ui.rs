//! The egui layer: draws the core's [`View`] and turns input into core
//! [`Event`]s. It holds no behaviour of its own.

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{Color32, ColorImage, RichText, TextureHandle, TextureOptions};

use crate::core::{
    About, DefaultRow, Defaults, DirColumn, EditsView, Event, ExportView, Focus, InnerImages, Key,
    KeyPress, MetadataView, MotionKind, Notice, Points, Region, Screen, Thumb, Toggle, Video,
    VideoStep, View, Zoom, unturned,
};
use crate::decode::DecodedImage;
use crate::folder;
use crate::metadata::Strip;
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
/// A band thumb still loading.
const PLACEHOLDER_TILE: Color32 = Color32::from_rgb(0x24, 0x24, 0x28);
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
/// Width of the metadata view.
const METADATA_WIDTH: f32 = 300.0;
/// The label of the capture date field.
const CAPTURED_LABEL: &str = "Captured";
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
                // `Shift`+`.` types `>` or `:`, depending on the layout; the
                // key's place still names it.
                egui::Event::Key {
                    key,
                    physical_key,
                    pressed: true,
                    modifiers,
                    ..
                } => map_key(*key)
                    .or_else(|| (*physical_key == Some(egui::Key::Period)).then_some(Key::Period))
                    .map(|k| {
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
        egui::Key::I => Key::I,
        egui::Key::Questionmark => Key::Question,
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
            show_key_line(ui, view, &mut events);
        });
    if !view.band.is_empty() && view.overview.is_none() {
        egui::Panel::bottom("band")
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::NONE.fill(BACKGROUND).inner_margin(6.0))
            .show(ui, |ui| {
                show_band(ui, &view.band, view.band_highlight, textures, &mut events);
                show_diode(ui, Focus::Band, view);
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
                tools::show_toolbar(
                    ui,
                    toolbar,
                    view.edits.as_ref(),
                    view.export.as_ref(),
                    &mut events,
                );
            });
    }
    if let Some(column) = &view.dir_column {
        egui::Panel::left("dir_column")
            .resizable(false)
            .exact_size(DIR_COLUMN_WIDTH)
            .frame(egui::Frame::NONE.fill(BACKGROUND).inner_margin(6.0))
            .show(ui, |ui| {
                show_dir_column(ui, column, view.focus == Focus::Dirs, &mut events);
                show_diode(ui, Focus::Dirs, view);
            });
    }
    if let Some(metadata) = &view.metadata {
        egui::Panel::right("metadata")
            .resizable(false)
            .exact_size(METADATA_WIDTH)
            .frame(egui::Frame::NONE.fill(BACKGROUND).inner_margin(6.0))
            .show(ui, |ui| show_metadata(ui, metadata, &mut events));
    }
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(BACKGROUND))
        .show(ui, |ui| {
            if let Some(gallery) = &view.overview {
                let side = view.settings.thumbnail_size;
                overview::show_overview(ui, gallery, side, textures, &mut events);
                show_diode(ui, Focus::Image, view);
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
                        // A click with the toolbar shown draws instead.
                        clickable: view.video.is_some() && view.toolbar.is_none(),
                        drawing: view.toolbar.is_some(),
                        edits: view.edits.as_ref(),
                        crop: view.crop,
                        export: view.export.as_ref(),
                    };
                    show_image(ui, &shown, view.zoom, textures, &mut events);
                    if let Some(video) = &view.video {
                        show_video(ui, video, path, view.toolbar.is_none(), &mut events);
                    }
                }
                Screen::Error { path, message } => {
                    show_error(ui, path.as_deref(), message, &mut events);
                }
            }
            if view.toolbar.is_none() && view.save_dialog.is_none() {
                show_edge_arrows(ui, &mut events);
            }
            if let Some(inner) = view.inner {
                show_inner(ui, inner);
            }
            if let Some(kind) = view.motion {
                show_motion(ui, kind);
            }
            show_diode(ui, Focus::Image, view);
        });
    if let (Some(dialog), Screen::Image { path, .. }) = (&view.save_dialog, &view.screen) {
        tools::show_save_dialog(ui.ctx(), &folder::file_name(path), dialog, &mut events);
    }
    if let Some(about) = view.about.as_ref().filter(|_| view.settings_open) {
        show_about(ui.ctx(), about, &mut events);
    } else if view.settings_open {
        show_settings(
            ui.ctx(),
            &view.settings,
            view.settings_error.as_deref(),
            view.defaults.as_ref(),
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

/// The files around the current one as small thumbs, the current one in the
/// middle: ones still loading as a plain placeholder, broken ones as a marked
/// tile, animated ones with a badge, the current one framed and the
/// highlighted one framed wider; a click on a thumb asks the core to show it.
fn show_band(
    ui: &mut egui::Ui,
    band: &[Thumb],
    highlight: Option<usize>,
    textures: &mut Textures,
    events: &mut Vec<Event>,
) {
    let (area, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), THUMB_HEIGHT),
        egui::Sense::hover(),
    );
    let sizes: Vec<egui::Vec2> = band
        .iter()
        .map(|thumb| match &thumb.image {
            Some(image) => fit(
                egui::vec2(image.width as f32, image.height as f32),
                egui::vec2(THUMB_HEIGHT * 2.0, THUMB_HEIGHT),
            ),
            None => egui::Vec2::splat(THUMB_HEIGHT),
        })
        .collect();
    let gap = ui.spacing().item_spacing.x;
    // The current thumb is centred; the others line up beside it, so the ends
    // of a folder leave their side empty.
    let current = band.iter().position(|t| t.current).unwrap_or(0);
    let before: f32 = sizes.iter().take(current).map(|s| s.x + gap).sum();
    let current_width = sizes.get(current).map_or(0.0, |s| s.x);
    let mut left = area.center().x - current_width / 2.0 - before;
    for (index, (thumb, size)) in band.iter().zip(sizes).enumerate() {
        let rect = egui::Rect::from_min_size(egui::pos2(left, area.min.y), size);
        left += size.x + gap;
        let response = ui.interact(
            rect,
            ui.id().with(("thumb", &thumb.path)),
            egui::Sense::click(),
        );
        let name = folder::file_name(&thumb.path);
        let painter = ui.painter_at(area);
        let label = match &thumb.image {
            Some(image) => {
                let texture = textures.get(ui.ctx(), &thumb.path, image);
                egui::Image::new(&texture).paint_at(ui, rect);
                if thumb.video {
                    paint_play(&painter, rect.center(), THUMB_HEIGHT / 5.0);
                    format!("{name} (video)")
                } else if thumb.animated {
                    paint_animated_badge(&painter, rect);
                    format!("{name} (animated)")
                } else if let Some(kind) = thumb.motion {
                    paint_motion_badge(&painter, rect, kind);
                    let kind = motion_name(kind).to_lowercase();
                    format!("{name} ({kind})")
                } else if thumb.images > 1 {
                    paint_images_badge(&painter, rect, thumb.images);
                    format!("{name} ({} images)", thumb.images)
                } else {
                    name
                }
            }
            None if thumb.loading => {
                painter.rect_filled(rect, 2.0, PLACEHOLDER_TILE);
                format!("{name} (loading)")
            }
            None => {
                painter.rect_filled(rect, 2.0, BROKEN_TILE);
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "!",
                    egui::FontId::proportional(32.0),
                    ERROR_TEXT,
                );
                format!("{name} (error)")
            }
        };
        if thumb.current {
            painter.rect_stroke(
                rect.expand(2.0),
                2.0,
                egui::Stroke::new(2.0, CURRENT_MARK),
                egui::StrokeKind::Outside,
            );
        }
        let highlighted = highlight == Some(index);
        if highlighted {
            painter.rect_stroke(
                rect.expand(4.0),
                3.0,
                egui::Stroke::new(2.0, BAND_HIGHLIGHT),
                egui::StrokeKind::Outside,
            );
        }
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Button, true, thumb.current, label.clone())
        });
        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.set_selected(highlighted));
        if response.clicked() {
            events.push(Event::Show(thumb.path.clone()));
        }
    }
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
/// The name of a folder in the dir column; the root's is its path.
fn folder_name(dir: &Path) -> String {
    match folder::file_name(dir) {
        name if name.is_empty() => dir.display().to_string(),
        name => name,
    }
}

/// brighter while the column has the focus.
fn show_dir_column(ui: &mut egui::Ui, column: &DirColumn, focused: bool, events: &mut Vec<Event>) {
    ui.label(
        RichText::new(folder_name(&column.dir))
            .color(TEXT)
            .strong()
            .size(15.0),
    );
    ui.add_space(4.0);
    if let Some(error) = &column.error {
        ui.label(RichText::new(error).color(ERROR_TEXT));
        copy_button(ui, error, Some(&column.dir), events);
    }
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            let font = egui::FontId::proportional(14.0);
            let row_height = ui.ctx().fonts_mut(|f| f.row_height(&font)) + 6.0;
            // Up, unless the current folder is the root, which stands alone.
            let current = column.highlight.and_then(|i| column.dirs.get(i));
            if current.is_some_and(|dir| *dir != column.dir) {
                let size = egui::vec2(ui.available_width(), row_height);
                let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
                if response.hovered() {
                    ui.painter().rect_filled(rect, 3.0, HIGHLIGHT_UNFOCUSED);
                }
                ui.painter().text(
                    egui::pos2(rect.min.x + 6.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    "..",
                    font.clone(),
                    DIM_TEXT,
                );
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "..")
                });
                if response.clicked() {
                    events.push(Event::FolderUp);
                }
            }
            for (index, dir) in column.dirs.iter().enumerate() {
                let name = folder_name(dir);
                let highlighted = column.highlight == Some(index);
                let size = egui::vec2(ui.available_width(), row_height);
                let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
                let fg = match (highlighted, focused) {
                    (true, true) => {
                        ui.painter().rect_filled(rect, 3.0, HIGHLIGHT);
                        HIGHLIGHT_TEXT
                    }
                    (true, false) => {
                        ui.painter().rect_filled(rect, 3.0, HIGHLIGHT_UNFOCUSED);
                        TEXT
                    }
                    (false, _) => {
                        if response.hovered() {
                            ui.painter().rect_filled(rect, 3.0, HIGHLIGHT_UNFOCUSED);
                        }
                        TEXT
                    }
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
                if response.double_clicked() {
                    events.push(Event::FolderDoubleClicked(dir.clone()));
                } else if response.clicked() {
                    events.push(Event::FolderClicked(dir.clone()));
                }
                // Only when needed: scrolling every frame never settles.
                if highlighted && !ui.clip_rect().contains_rect(rect) {
                    ui.scroll_to_rect(rect, None);
                }
            }
        });
}

/// The metadata of the current file, read-only but for the capture date,
/// whose field reports every change and `Return`, with the buttons that
/// remove the location or all EXIF.
fn show_metadata(ui: &mut egui::Ui, metadata: &MetadataView, events: &mut Vec<Event>) {
    ui.label(RichText::new("Metadata").color(TEXT).strong().size(15.0));
    ui.add_space(4.0);
    if metadata.reading {
        ui.label(RichText::new("Reading…").color(DIM_TEXT));
    }
    let row = |ui: &mut egui::Ui, label: &str, value: &str| {
        ui.label(RichText::new(label).color(DIM_TEXT));
        ui.label(RichText::new(value).color(TEXT));
        ui.end_row();
    };
    egui::Grid::new("metadata_fields")
        .num_columns(2)
        .spacing(egui::vec2(12.0, 4.0))
        .show(ui, |ui| {
            for field in &metadata.file {
                row(ui, field.label, &field.value);
            }
            if metadata.reading {
                return;
            }
            let label = ui.label(RichText::new(CAPTURED_LABEL).color(DIM_TEXT));
            match &metadata.date {
                Some(date) => {
                    let mut typed = date.clone();
                    let field = ui
                        .add_enabled(
                            !metadata.writing,
                            egui::TextEdit::singleline(&mut typed).desired_width(150.0),
                        )
                        .labelled_by(label.id);
                    if typed != *date {
                        events.push(Event::DateEdited(typed));
                    }
                    if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        events.push(Event::DateEntered);
                    }
                }
                None => {
                    ui.label(RichText::new("no capture date").color(TEXT));
                }
            }
            ui.end_row();
            for field in &metadata.exif {
                row(ui, field.label, &field.value);
            }
        });
    if !metadata.reading {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            for (label, enabled, strip) in [
                (
                    "Remove location",
                    metadata.can_remove_location,
                    Strip::Location,
                ),
                ("Remove all EXIF", metadata.can_remove_exif, Strip::AllExif),
            ] {
                let color = if enabled { TEXT } else { DIM_TEXT };
                let button = egui::Button::new(RichText::new(label).color(color));
                if ui.add_enabled(enabled, button).clicked() {
                    events.push(Event::RemoveMetadata(strip));
                }
            }
        });
    }
    if let Some(error) = &metadata.error {
        ui.add_space(4.0);
        ui.label(RichText::new(error).color(ERROR_TEXT));
        copy_button(ui, error, None, events);
    }
}

/// The settings screen over the dimmed window; a changed value is reported
/// with all settings. Closing it is the core's business.
fn show_settings(
    ctx: &egui::Context,
    values: &Settings,
    error: Option<&str>,
    defaults: Option<&Defaults>,
    events: &mut Vec<Event>,
) {
    egui::Modal::new(egui::Id::new("settings"))
        .frame(
            egui::Frame::NONE
                .fill(BACKGROUND)
                .inner_margin(24.0)
                .corner_radius(8.0),
        )
        .show(ctx, |ui| {
            ui.set_width(SETTINGS_WIDTH);
            ui.label(RichText::new("Settings").color(TEXT).strong().size(20.0));
            ui.add_space(8.0);
            let mut changed = values.clone();
            settings_section(ui, "Browsing", |ui| {
                settings_grid(ui, "browsing", |ui| {
                    let label = ui.label(RichText::new(CACHE_LABEL).color(TEXT));
                    ui.add(egui::Slider::new(
                        &mut changed.cache_per_side,
                        settings::CACHE_PER_SIDE,
                    ))
                    .labelled_by(label.id);
                    ui.end_row();
                });
            });
            settings_section(ui, "Other apps", |ui| {
                settings_grid(ui, "apps", |ui| {
                    app_field(ui, IMAGE_APP_LABEL, &mut changed.image_app);
                    app_field(ui, VIDEO_APP_LABEL, &mut changed.video_app);
                });
                ui.add_space(6.0);
                ui.label(RichText::new(APP_HINT).color(DIM_TEXT));
            });
            if changed != *values {
                events.push(Event::SettingsChanged(changed));
            }
            if let Some(error) = error {
                ui.add_space(12.0);
                ui.label(RichText::new(error).color(ERROR_TEXT));
            }
            settings_section(ui, "Default apps", |ui| {
                show_defaults(ui, defaults, &values.earlier_defaults, events);
            });
            settings_section(ui, "About", |ui| {
                let link = egui::Button::new(RichText::new("About pixpixpix").color(TEXT));
                if ui.add(link).clicked() {
                    events.push(Event::OpenAbout);
                }
            });
        });
}

/// The default app of every file type: switches for all images and all
/// videos, then a row per type offering pixpixpix or the app it replaced.
fn show_defaults(
    ui: &mut egui::Ui,
    defaults: Option<&Defaults>,
    earlier: &BTreeMap<String, String>,
    events: &mut Vec<Event>,
) {
    let Some(defaults) = defaults else {
        ui.label(RichText::new("Looking\u{2026}").color(DIM_TEXT));
        return;
    };
    if defaults.read_only {
        ui.label(
            RichText::new("The Flatpak cannot change default apps; the system settings can.")
                .color(DIM_TEXT),
        );
    }
    let enabled = !defaults.read_only;
    ui.horizontal(|ui| {
        for (video, label) in [
            (false, "pixpixpix for all images"),
            (true, "pixpixpix for all videos"),
        ] {
            let rows: Vec<&DefaultRow> =
                defaults.rows.iter().filter(|r| r.video == video).collect();
            let mut all = rows.iter().all(|r| r.ours);
            let response = ui.add_enabled(
                enabled,
                egui::Checkbox::new(&mut all, RichText::new(label).color(TEXT)),
            );
            if response.changed() {
                events.push(Event::SetDefaults {
                    names: rows.iter().map(|r| r.name.clone()).collect(),
                    ours: all,
                });
            }
        }
    });
    ui.add_space(6.0);
    let name = |app: &String| {
        defaults
            .names
            .get(app)
            .cloned()
            .unwrap_or_else(|| app.clone())
    };
    egui::ScrollArea::vertical()
        .max_height(220.0)
        .show(ui, |ui| {
            settings_grid(ui, "defaults", |ui| {
                for row in &defaults.rows {
                    let label = ui.label(RichText::new(&row.name).color(TEXT));
                    let shown = match (&row.current, row.ours) {
                        (_, true) => "pixpixpix".to_owned(),
                        (Some(app), false) => name(app),
                        (None, false) => "none".to_owned(),
                    };
                    let other = earlier.get(&row.name).filter(|_| row.ours).map(name);
                    ui.add_enabled_ui(enabled, |ui| {
                        egui::ComboBox::from_id_salt(("default", &row.name))
                            .selected_text(RichText::new(&shown).color(TEXT))
                            .width(220.0)
                            .show_ui(ui, |ui| {
                                if ui.selectable_label(row.ours, "pixpixpix").clicked() && !row.ours
                                {
                                    events.push(Event::SetDefaults {
                                        names: vec![row.name.clone()],
                                        ours: true,
                                    });
                                }
                                if let Some(other) = &other
                                    && ui.selectable_label(false, other).clicked()
                                {
                                    events.push(Event::SetDefaults {
                                        names: vec![row.name.clone()],
                                        ours: false,
                                    });
                                }
                            })
                            .response
                            .labelled_by(label.id);
                    });
                    ui.end_row();
                }
            });
        });
    if let Some(error) = &defaults.error {
        ui.label(RichText::new(error).color(ERROR_TEXT));
        copy_button(ui, error, None, events);
    }
}

/// Where pixpixpix lives.
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
/// The band the name nods to.
const BAND_WIKIPEDIA: &str = "https://en.wikipedia.org/wiki/Pigs_Pigs_Pigs_Pigs_Pigs_Pigs_Pigs";
const BAND_SPOTIFY: &str = "https://open.spotify.com/artist/1F7QDWyZTLGzkyGLgFjEhU";

/// The About page: the icon, name, version, license and links, and the
/// optional libraries, with buttons to copy it all and to go back.
fn show_about(ctx: &egui::Context, about: &About, events: &mut Vec<Event>) {
    egui::Modal::new(egui::Id::new("about"))
        .frame(
            egui::Frame::NONE
                .fill(BACKGROUND)
                .inner_margin(24.0)
                .corner_radius(8.0),
        )
        .show(ctx, |ui| {
            ui.set_width(SETTINGS_WIDTH);
            ui.horizontal(|ui| {
                if let Some(icon) = icon_texture(ui.ctx()) {
                    ui.add(egui::Image::new(&icon).fit_to_exact_size(egui::Vec2::splat(72.0)));
                }
                ui.vertical(|ui| {
                    ui.label(RichText::new("pixpixpix").color(TEXT).strong().size(22.0));
                    ui.label(
                        RichText::new(format!(
                            "Version {} \u{b7} MIT license",
                            env!("CARGO_PKG_VERSION")
                        ))
                        .color(DIM_TEXT),
                    );
                    ui.hyperlink_to("GitHub", REPOSITORY);
                });
            });
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new("The name nods to the band Pigs Pigs Pigs Pigs Pigs Pigs Pigs:")
                        .color(TEXT),
                );
                ui.hyperlink_to("Wikipedia", BAND_WIKIPEDIA);
                ui.hyperlink_to("Spotify", BAND_SPOTIFY);
            });
            settings_section(ui, "Libraries", |ui| match &about.libraries {
                None => {
                    ui.label(RichText::new("Looking\u{2026}").color(DIM_TEXT));
                }
                Some(libraries) => {
                    settings_grid(ui, "libraries", |ui| {
                        for library in libraries {
                            ui.label(RichText::new(&library.name).color(TEXT).strong());
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&library.needed_for).color(DIM_TEXT));
                                match &library.found {
                                    Some(path) => {
                                        ui.label(RichText::new(path).color(TEXT));
                                    }
                                    None => {
                                        ui.label(RichText::new("missing").color(ERROR_TEXT));
                                        ui.label(RichText::new(&library.hint).color(DIM_TEXT));
                                    }
                                }
                            });
                            ui.end_row();
                        }
                    });
                }
            });
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                if ui.button(RichText::new("Copy").color(TEXT)).clicked() {
                    events.push(Event::CopyText(about_text(about)));
                }
                if ui.button(RichText::new("Back").color(TEXT)).clicked() {
                    events.push(Event::CloseAbout);
                }
            });
        });
}

/// What the About page's copy button puts on the clipboard.
pub fn about_text(about: &About) -> String {
    let mut text = format!(
        "{}\nMIT license\n{REPOSITORY}\n\nLibraries:\n",
        crate::report::system()
    );
    for library in about.libraries.iter().flatten() {
        let found = match &library.found {
            Some(path) => path.clone(),
            None => format!("missing ({})", library.hint),
        };
        text.push_str(&format!(
            "{} ({}): {found}\n",
            library.name, library.needed_for
        ));
    }
    text
}

/// The app's icon as a texture, made once.
fn icon_texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let id = egui::Id::new("app icon");
    if let Some(icon) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return Some(icon);
    }
    let png = include_bytes!("../assets/icon/icon_256.png");
    let image = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .ok()?
        .to_rgba8();
    let size = [
        usize::try_from(image.width()).ok()?,
        usize::try_from(image.height()).ok()?,
    ];
    let pixels = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    let texture = ctx.load_texture("app icon", pixels, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, texture.clone()));
    Some(texture)
}

/// The width of the settings screen's content.
const SETTINGS_WIDTH: f32 = 520.0;

/// The width of the labels' column of the settings screen.
const SETTINGS_LABELS: f32 = 170.0;

/// A section of the settings screen: a divider, its title and `add`.
fn settings_section(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
    ui.label(RichText::new(title).color(TEXT).strong().size(15.0));
    ui.add_space(8.0);
    add(ui);
}

/// The rows of a settings section, labels on the left, controls lined up on
/// the right.
fn settings_grid(ui: &mut egui::Ui, id: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing([16.0, 10.0])
        .min_col_width(SETTINGS_LABELS)
        .show(ui, add);
}

/// A labelled one-line text field for an external app, as a row of a
/// settings grid.
fn app_field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    let label = ui.label(RichText::new(label).color(TEXT));
    ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY))
        .labelled_by(label.id);
    ui.end_row();
}

/// The button under or beside an error that copies it, with what a search
/// or a bug report needs besides, to the clipboard.
fn copy_button(ui: &mut egui::Ui, message: &str, path: Option<&Path>, events: &mut Vec<Event>) {
    let button = egui::Button::new(RichText::new("Copy").color(TEXT).size(12.0)).small();
    let response = ui.add(button).on_hover_text(
        "Copy the error with the version and the system, for a search or a bug report",
    );
    if response.clicked() {
        events.push(Event::CopyError {
            message: message.to_owned(),
            path: path.map(Path::to_path_buf),
        });
    }
}

/// The name of the area `focus` stands for, as the key line and the
/// diodes call it.
fn area_name(focus: Focus, view: &View) -> &'static str {
    match focus {
        Focus::Dirs => "folders",
        Focus::Image if view.overview.is_some() => "gallery",
        Focus::Image => "image view",
        Focus::Band => "preloaded files",
    }
}

/// The diode in the bottom right corner of the area `focus` stands for, lit
/// while it has the focus.
fn show_diode(ui: &mut egui::Ui, focus: Focus, view: &View) {
    let area = ui.max_rect();
    let center = egui::pos2(area.max.x - 8.0, area.max.y - 8.0);
    let rect = egui::Rect::from_center_size(center, egui::Vec2::splat(10.0));
    let lit = view.focus == focus;
    let name = area_name(focus, view);
    let response = ui.interact(rect, ui.id().with(("diode", name)), egui::Sense::hover());
    let fill = if lit {
        CURRENT_MARK
    } else {
        HIGHLIGHT_UNFOCUSED
    };
    ui.painter().circle_filled(center, 4.0, fill);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Label, true, lit, format!("{name} focus"))
    });
    ui.ctx()
        .accesskit_node_builder(response.id, |node| node.set_selected(lit));
}

/// The character of `text` at `x` points from its left on one line.
fn char_at(ui: &egui::Ui, text: &RichText, x: f32) -> usize {
    let galley = egui::WidgetText::from(text.clone()).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Body,
    );
    galley.cursor_from_pos(egui::vec2(x, 0.0)).index.0
}

/// The width `text` takes on one line.
fn text_width(ui: &egui::Ui, text: &RichText) -> f32 {
    let galley = egui::WidgetText::from(text.clone()).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Body,
    );
    galley.size().x
}

fn show_key_line(ui: &mut egui::Ui, view: &View, events: &mut Vec<Event>) {
    let key_line = &view.key_line;
    ui.horizontal_wrapped(|ui| {
        // Where the keys go.
        ui.label(
            RichText::new(area_name(view.focus, view))
                .color(CURRENT_MARK)
                .strong(),
        );
        ui.add_space(12.0);
        if view.settings.hints_hidden {
            // Where the hints were: `?` brings them back.
            ui.label(RichText::new("?").color(DIM_TEXT).strong());
            ui.add_space(12.0);
        } else {
            for hint in &key_line.keys {
                let key = RichText::new(hint.keys).color(TEXT).strong();
                let action = RichText::new(hint.action).color(DIM_TEXT);
                // A hint never breaks between its key and its action: it
                // starts a new line when it does not fit whole.
                let width = text_width(ui, &key) + text_width(ui, &action);
                let needed = width + ui.spacing().item_spacing.x;
                if ui.available_width() < needed && ui.cursor().min.x > ui.max_rect().min.x {
                    ui.end_row();
                }
                // A click presses the key under the pointer; on the action,
                // the key of a hint with one.
                let presses = hint.presses();
                let response = ui.add(
                    egui::Label::new(key.clone())
                        .wrap_mode(egui::TextWrapMode::Extend)
                        .sense(egui::Sense::click()),
                );
                if response.clicked()
                    && let Some(pos) = response.interact_pointer_pos()
                {
                    let at = char_at(ui, &key, pos.x - response.rect.min.x);
                    let press = presses
                        .iter()
                        .find(|(chars, _)| chars.contains(&at))
                        .or(presses.last())
                        .and_then(|(_, press)| *press);
                    events.extend(press.map(Event::Key));
                }
                let one = match presses.as_slice() {
                    [(_, Some(press))] => Some(*press),
                    _ => None,
                };
                let sense = if one.is_some() {
                    egui::Sense::click()
                } else {
                    egui::Sense::hover()
                };
                let response = ui.add(
                    egui::Label::new(action)
                        .wrap_mode(egui::TextWrapMode::Extend)
                        .sense(sense),
                );
                if response.clicked() {
                    events.extend(one.map(Event::Key));
                }
                ui.add_space(12.0);
            }
        }
        for tag in &view.tags {
            let text = match (tag.toggle, tag.kept) {
                (Toggle::DotEntries, false) => "dot entries shown",
                (Toggle::DotEntries, true) => "dot entries shown, kept",
                (Toggle::Overview, _) => "overview",
            };
            ui.label(
                RichText::new(text)
                    .color(HIGHLIGHT_TEXT)
                    .background_color(HIGHLIGHT),
            );
        }
        if let Some(notice) = key_line.notice {
            let text = match notice {
                Notice::FirstImage => "first image",
                Notice::LastImage => "last image",
                Notice::Copied => "copied",
            };
            ui.label(RichText::new(text).color(NOTICE_TEXT));
        }
        let shown = match &view.screen {
            Screen::Image { path, .. } | Screen::Loading { path } => Some(path.as_path()),
            Screen::Error { path, .. } => path.as_deref(),
            Screen::Listing { .. } => None,
        };
        if let Some(error) = &view.folder_error {
            let text = format!("Cannot read the folder: {error}");
            ui.label(RichText::new(&text).color(ERROR_TEXT));
            copy_button(ui, &text, None, events);
        }
        let errors = [
            &view.open_error,
            &view.rotation_error,
            &view.trash_error,
            &view.copy_error,
        ];
        for error in errors.into_iter().flatten() {
            ui.label(RichText::new(error).color(ERROR_TEXT));
            copy_button(ui, error, shown, events);
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
    /// The part of it shown after a crop.
    crop: Option<Region>,
    /// The export tool, its area framed over it.
    export: Option<&'a ExportView>,
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
    // A click draws too, to place text.
    let sense = if shown.drawing {
        egui::Sense::click_and_drag()
    } else {
        egui::Sense::drag()
    };
    let response = ui.interact(area, ui.id().with("image"), sense);
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
        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            events.push(Event::DrawStart(from_area(pos)));
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
    let (width, height) = (shown.image.width, shown.image.height);
    let region = shown.crop.unwrap_or(Region::whole(width, height));
    let placed = zoom.region_placement(region, shown.rotation, viewport);
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
            uv: texture_point(shown.rotation, u, v, region, (width, height)),
            color: Color32::WHITE,
        });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(mesh);
    let placed = tools::Placed {
        image: shown.image,
        region,
        rotation: shown.rotation,
        rect: full,
    };
    if let Some(edits) = shown.edits {
        tools::paint_edits(&ui.painter_at(visible), edits, &placed);
        tools::show_text_field(ui.ctx(), edits, &placed, events);
    }
    if let Some(export) = shown.export
        && let Some(area) = export.area
    {
        let rect = placed.box_of(area);
        tools::paint_crop(&ui.painter_at(visible), rect, !export.export.locked);
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

/// How close to the left or right edge of the main area the pointer brings
/// up the arrow to the previous or next file, in points.
const EDGE: f32 = 96.0;

/// The arrow to the previous or next file, while the pointer is near the
/// left or right edge of the main area; a click on it steps there.
fn show_edge_arrows(ui: &mut egui::Ui, events: &mut Vec<Event>) {
    let area = ui.max_rect();
    let Some(pointer) = ui.input(|i| i.pointer.hover_pos()) else {
        return;
    };
    for (forward, label) in [(false, "Previous"), (true, "Next")] {
        let x = if forward {
            area.max.x - EDGE / 2.0
        } else {
            area.min.x + EDGE / 2.0
        };
        let near = if forward {
            pointer.x >= area.max.x - EDGE
        } else {
            pointer.x <= area.min.x + EDGE
        };
        if !near || !area.contains(pointer) {
            continue;
        }
        let rect =
            egui::Rect::from_center_size(egui::pos2(x, area.center().y), egui::Vec2::splat(44.0));
        let response = ui.interact(rect, ui.id().with(label), egui::Sense::click());
        let painter = ui.painter();
        painter.circle_filled(rect.center(), 22.0, ANIMATED_BADGE);
        let (tip, back) = if forward { (8.0, -6.0) } else { (-8.0, 6.0) };
        let center = rect.center();
        let at = |dx: f32, dy: f32| egui::pos2(center.x + dx, center.y + dy);
        let stroke = egui::Stroke::new(3.0, TEXT);
        painter.line_segment([at(back, -10.0), at(tip, 0.0)], stroke);
        painter.line_segment([at(back, 10.0), at(tip, 0.0)], stroke);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
        if response.clicked() {
            events.push(Event::Navigate { forward });
        }
    }
}

/// Over a video's frame: a big play triangle, unless the tools draw there,
/// and step buttons while it is not playing, which a click plays, and why it
/// cannot be played.
fn show_video(
    ui: &mut egui::Ui,
    video: &Video,
    path: &Path,
    triangle: bool,
    events: &mut Vec<Event>,
) {
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
    if !video.playing && triangle {
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
        let rect = egui::Rect::from_min_max(egui::pos2(area.min.x, area.max.y - 84.0), area.max);
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(error)
                        .color(ERROR_TEXT)
                        .background_color(ANIMATED_BADGE)
                        .size(15.0),
                );
                copy_button(ui, error, Some(path), events);
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

/// The texture point shown at `(u, v)` of the part `region` of an image of
/// `width` × `height` pixels turned by `rotation`; `(u, v)` and the point are
/// fractions of width and height.
fn texture_point(
    rotation: Rotation,
    u: f32,
    v: f32,
    region: Region,
    (width, height): (u32, u32),
) -> egui::Pos2 {
    let (x, y) = unturned(rotation, u, v);
    egui::pos2(
        (region.left + x * region.width()) / width as f32,
        (region.top + y * region.height()) / height as f32,
    )
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

fn show_error(ui: &mut egui::Ui, path: Option<&Path>, message: &str, events: &mut Vec<Event>) {
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
            copy_button(ui, message, path, events);
            if path.is_some() {
                ui.label(RichText::new(OPEN_HINT).color(DIM_TEXT));
            }
        });
    });
}

/// A dim line in the middle of the main area.
fn show_status(ui: &mut egui::Ui, text: &str) {
    ui.centered_and_justified(|ui| {
        ui.label(RichText::new(text).color(DIM_TEXT).size(15.0));
    });
}
