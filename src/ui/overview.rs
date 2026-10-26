//! The overview: sections of tiles in a scroll area, laid out only where they
//! are on screen, so a tree of many thousand files stays fast.

use egui::{Color32, Rect, RichText};

use super::{BROKEN_TILE, ERROR_TEXT, TEXT, Textures, fit, paint_play, show_status};
use crate::core::{Event, Overview, Tile};
use crate::folder;

/// The square a thumbnail is fitted into.
const TILE: f32 = 160.0;
/// The line of the file name below a thumbnail.
const NAME_HEIGHT: f32 = 20.0;
/// The space between tiles, both ways.
const GAP: f32 = 10.0;
/// The line of a section's folder.
const HEADER_HEIGHT: f32 = 30.0;
/// A tile whose thumbnail is not decoded, or a video.
const PLACEHOLDER: Color32 = Color32::from_rgb(0x2a, 0x2a, 0x30);

/// Draws the sections on screen and reports which tiles those are, when that
/// changed.
pub(super) fn show_overview(
    ui: &mut egui::Ui,
    overview: &Overview,
    textures: &mut Textures,
    events: &mut Vec<Event>,
) {
    if overview.sections.is_empty() {
        let text = if overview.scanning {
            "Scanning…"
        } else {
            "No images"
        };
        show_status(ui, text);
        return;
    }
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show_viewport(ui, |ui, viewport| {
            let width = ui.available_width();
            let columns = ((width + GAP) / (TILE + GAP)).floor().max(1.0) as usize;
            let row = TILE + NAME_HEIGHT + GAP;
            let height = |tiles: usize| HEADER_HEIGHT + tiles.div_ceil(columns) as f32 * row;
            let total: f32 = overview
                .sections
                .iter()
                .map(|s| height(s.tiles.len()))
                .sum();
            ui.set_height(total);
            let origin = ui.max_rect().min;
            let mut top = 0.0;
            let mut visible: Option<std::ops::Range<usize>> = None;
            for section in &overview.sections {
                let bottom = top + height(section.tiles.len());
                if bottom < viewport.min.y || top > viewport.max.y {
                    top = bottom;
                    continue;
                }
                let header = Rect::from_min_size(
                    egui::pos2(origin.x, origin.y + top),
                    egui::vec2(width, HEADER_HEIGHT),
                );
                ui.scope_builder(egui::UiBuilder::new().max_rect(header), |ui| {
                    ui.label(
                        RichText::new(&section.title)
                            .color(TEXT)
                            .strong()
                            .size(15.0),
                    );
                });
                let rows_top = top + HEADER_HEIGHT;
                let first_row = ((viewport.min.y - rows_top) / row).floor().max(0.0) as usize;
                let last_row = ((viewport.max.y - rows_top) / row).ceil().max(0.0) as usize;
                let first = first_row.saturating_mul(columns);
                let last = last_row.saturating_mul(columns).min(section.tiles.len());
                for offset in first..last {
                    let index = section.tiles.start.saturating_add(offset);
                    let Some(tile) = overview.tiles.get(index) else {
                        break;
                    };
                    let row_index = offset.checked_div(columns).unwrap_or(0);
                    let column = offset.checked_rem(columns).unwrap_or(0);
                    let min = egui::pos2(
                        origin.x + column as f32 * (TILE + GAP),
                        origin.y + rows_top + row_index as f32 * row,
                    );
                    show_tile(ui, index, tile, min, textures);
                    visible = Some(match visible {
                        Some(range) => range.start..index.saturating_add(1),
                        None => index..index.saturating_add(1),
                    });
                }
                top = bottom;
            }
            let visible = visible.unwrap_or(0..0);
            if visible != overview.visible {
                events.push(Event::OverviewVisible(visible));
            }
        });
}

/// One tile with its top left corner at `min`: the thumbnail, a placeholder
/// while it is not decoded (with a play mark for a video), or an error
/// marker, and the file name below.
fn show_tile(
    ui: &mut egui::Ui,
    index: usize,
    tile: &Tile,
    min: egui::Pos2,
    textures: &mut Textures,
) {
    let name = folder::file_name(&tile.path);
    let area = Rect::from_min_size(min, egui::Vec2::splat(TILE));
    let painter = ui.painter();
    match &tile.thumbnail {
        Some(Ok(image)) => {
            let texture = textures.get(ui.ctx(), &tile.path, image);
            let pixels = egui::vec2(image.width as f32, image.height as f32);
            let rect = Rect::from_center_size(area.center(), fit(pixels, area.size()));
            egui::Image::new(&texture).paint_at(ui, rect);
            let response = ui.interact(rect, ui.id().with(("tile", index)), egui::Sense::hover());
            response
                .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Image, true, &name));
        }
        Some(Err(_)) => {
            painter.rect_filled(area, 2.0, BROKEN_TILE);
            painter.text(
                area.center(),
                egui::Align2::CENTER_CENTER,
                "!",
                egui::FontId::proportional(40.0),
                ERROR_TEXT,
            );
        }
        None => {
            painter.rect_filled(area, 2.0, PLACEHOLDER);
            if tile.video {
                paint_play(painter, area.center(), TILE / 6.0);
            }
        }
    }
    let label = Rect::from_min_size(
        egui::pos2(area.min.x, area.max.y),
        egui::vec2(TILE, NAME_HEIGHT),
    );
    ui.put(
        label,
        egui::Label::new(RichText::new(name).color(TEXT).size(12.0)).truncate(),
    );
}
