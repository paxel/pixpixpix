//! The overview: sections of tiles in a scroll area, laid out only where they
//! are on screen, so a tree of many thousand files stays fast.

use egui::{Color32, Rect, RichText};

use super::{
    BAND_HIGHLIGHT, BROKEN_TILE, ERROR_TEXT, TEXT, Textures, fit, paint_motion_badge, paint_play,
    show_status,
};
use crate::core::{Event, MotionKind, Overview, Reveal, Section, Tile};
use crate::folder;

/// The line of the file name below a thumbnail.
const NAME_HEIGHT: f32 = 20.0;
/// The space between tiles, both ways.
const GAP: f32 = 10.0;
/// The line of a section's folder.
const HEADER_HEIGHT: f32 = 30.0;
/// A tile whose thumbnail is not decoded, or a video without one.
const PLACEHOLDER: Color32 = Color32::from_rgb(0x2a, 0x2a, 0x30);
/// The points a touchpad scrolls with `Ctrl` held for one thumbnail size.
const TOUCHPAD_STEP: f32 = 50.0;

/// The grid of tiles of `side` points in a width of `columns` tiles.
#[derive(Clone, Copy)]
struct Grid {
    side: f32,
    columns: usize,
}

impl Grid {
    /// The height of a row: a tile, its name and the gap below.
    fn row(self) -> f32 {
        self.side + NAME_HEIGHT + GAP
    }

    /// The height of a section of `tiles` tiles with its heading.
    fn height(self, tiles: usize) -> f32 {
        HEADER_HEIGHT + tiles.div_ceil(self.columns) as f32 * self.row()
    }

    /// The top of each section from the top of the first, and the bottom of
    /// the last.
    fn tops(self, sections: &[Section]) -> Vec<f32> {
        let mut top = 0.0;
        let mut tops = vec![top];
        for section in sections {
            top += self.height(section.tiles.len());
            tops.push(top);
        }
        tops
    }

    /// Where tile `offset` of a section whose rows start at `rows_top` lies,
    /// from the top left of the content.
    fn tile(self, rows_top: f32, offset: usize) -> egui::Vec2 {
        let row = offset.checked_div(self.columns).unwrap_or(0);
        let column = offset.checked_rem(self.columns).unwrap_or(0);
        egui::vec2(
            column as f32 * (self.side + GAP),
            rows_top + row as f32 * self.row(),
        )
    }
}

/// Draws the sections on screen, the selected tile framed, and reports which
/// tiles those are and how many fit in a row, when that changed. What the
/// core asks to reveal is scrolled to once; `Ctrl`+wheel resizes the tiles,
/// a click opens one.
pub(super) fn show_overview(
    ui: &mut egui::Ui,
    overview: &Overview,
    side: u32,
    textures: &mut Textures,
    events: &mut Vec<Event>,
) {
    // One step per notch of a wheel, or per `TOUCHPAD_STEP` points a touchpad
    // scrolls; egui's zoom is smoothed over frames.
    let id = egui::Id::new("thumbnail touchpad");
    let mut scrolled: f32 = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let mut turns = Vec::new();
    ui.input(|i| {
        for event in &i.events {
            let egui::Event::MouseWheel {
                unit,
                delta,
                modifiers,
                ..
            } = event
            else {
                continue;
            };
            if !modifiers.command || delta.y == 0.0 {
                continue;
            }
            if *unit != egui::MouseWheelUnit::Point {
                scrolled = 0.0;
                turns.push(delta.y > 0.0);
                continue;
            }
            if scrolled.signum() != delta.y.signum() {
                scrolled = 0.0;
            }
            scrolled += delta.y;
            while scrolled.abs() >= TOUCHPAD_STEP {
                turns.push(scrolled > 0.0);
                scrolled -= TOUCHPAD_STEP.copysign(scrolled);
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(id, scrolled));
    events.extend(
        turns
            .into_iter()
            .map(|larger| Event::ThumbnailWheel { larger }),
    );
    if overview.sections.is_empty() {
        let text = match (overview.scanning, overview.bin) {
            (true, false) => "Scanning…",
            (false, false) => "No images",
            (true, true) => "Reading the trash…",
            (false, true) => "No images in the trash",
        };
        show_status(ui, text);
        return;
    }
    let side = side as f32;
    egui::ScrollArea::vertical()
        .id_salt(&overview.root)
        .auto_shrink(false)
        .show_viewport(ui, |ui, viewport| {
            let width = ui.available_width();
            let columns = ((width + GAP) / (side + GAP)).floor().max(1.0) as usize;
            if columns != overview.columns {
                events.push(Event::OverviewColumns(columns));
            }
            let grid = Grid { side, columns };
            let tops = grid.tops(&overview.sections);
            ui.set_height(tops.last().copied().unwrap_or(0.0));
            let origin = ui.max_rect().min;
            if let Some(reveal) = overview.reveal {
                if let Some((rect, align)) = revealed(overview, grid, &tops, reveal) {
                    ui.scroll_to_rect(rect.translate(origin.to_vec2()), align);
                }
                events.push(Event::OverviewRevealed);
            }
            let mut visible: Option<std::ops::Range<usize>> = None;
            for (section, (top, bottom)) in overview
                .sections
                .iter()
                .zip(tops.iter().zip(tops.iter().skip(1)))
            {
                let (top, bottom) = (*top, *bottom);
                if bottom < viewport.min.y || top > viewport.max.y {
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
                let first_row =
                    ((viewport.min.y - rows_top) / grid.row()).floor().max(0.0) as usize;
                let last_row = ((viewport.max.y - rows_top) / grid.row()).ceil().max(0.0) as usize;
                let first = first_row.saturating_mul(columns);
                let last = last_row.saturating_mul(columns).min(section.tiles.len());
                for offset in first..last {
                    let index = section.tiles.start.saturating_add(offset);
                    let Some(tile) = overview.tiles.get(index) else {
                        break;
                    };
                    let at = grid.tile(rows_top, offset);
                    let min = egui::pos2(origin.x + at.x, origin.y + at.y);
                    let selected = overview.selected == Some(index);
                    if show_tile(ui, index, tile, min, side, selected, textures) {
                        events.push(Event::OpenTile(index));
                    }
                    visible = Some(match visible {
                        Some(range) => range.start..index.saturating_add(1),
                        None => index..index.saturating_add(1),
                    });
                }
            }
            let visible = visible.unwrap_or(0..0);
            if visible != overview.visible {
                events.push(Event::OverviewVisible(visible));
            }
        });
}

/// Where `reveal` lies from the top left of the content and how to align it:
/// a tile, as little as needed.
fn revealed(
    overview: &Overview,
    grid: Grid,
    tops: &[f32],
    reveal: Reveal,
) -> Option<(Rect, Option<egui::Align>)> {
    match reveal {
        Reveal::Tile(index) => {
            let section = overview
                .sections
                .iter()
                .position(|s| s.tiles.contains(&index))?;
            let start = overview.sections.get(section)?.tiles.start;
            let rows_top = tops.get(section)? + HEADER_HEIGHT;
            let min = grid.tile(rows_top, index.checked_sub(start)?);
            let size = egui::vec2(grid.side, grid.side + NAME_HEIGHT);
            Some((Rect::from_min_size(min.to_pos2(), size), None))
        }
    }
}

/// One tile of `side` points with its top left corner at `min`: the
/// thumbnail, a placeholder while it is not decoded, or an error marker; a
/// video carries a play mark, a Live Photo its badge; the file name below,
/// and a frame while selected. Whether it was clicked.
fn show_tile(
    ui: &mut egui::Ui,
    index: usize,
    tile: &Tile,
    min: egui::Pos2,
    side: f32,
    selected: bool,
    textures: &mut Textures,
) -> bool {
    let name = folder::file_name(&tile.path);
    let area = Rect::from_min_size(min, egui::Vec2::splat(side));
    let painter = ui.painter();
    let mut marked = area;
    let label = match &tile.thumbnail {
        Some(Ok(image)) => {
            let texture = textures.get(ui.ctx(), &tile.path, image);
            let pixels = egui::vec2(image.width as f32, image.height as f32);
            let rect = Rect::from_center_size(area.center(), fit(pixels, area.size()));
            egui::Image::new(&texture).paint_at(ui, rect);
            let response = ui.interact(rect, ui.id().with(("tile", index)), egui::Sense::hover());
            response
                .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Image, true, &name));
            marked = rect;
            None
        }
        Some(Err(_)) if !tile.video => {
            painter.rect_filled(area, 2.0, BROKEN_TILE);
            painter.text(
                area.center(),
                egui::Align2::CENTER_CENTER,
                "!",
                egui::FontId::proportional(40.0),
                ERROR_TEXT,
            );
            Some("error")
        }
        // A video without a first frame, libmpv missing perhaps, stays a
        // placeholder.
        _ => {
            painter.rect_filled(area, 2.0, PLACEHOLDER);
            None
        }
    };
    let label = if tile.video {
        paint_play(ui.painter(), marked.center(), side / 6.0);
        Some("video")
    } else if tile.motion == Some(MotionKind::LivePhoto) {
        paint_motion_badge(ui.painter(), marked, MotionKind::LivePhoto);
        Some("live photo")
    } else {
        label
    };
    if selected {
        ui.painter().rect_stroke(
            area.expand(3.0),
            3.0,
            egui::Stroke::new(3.0, BAND_HIGHLIGHT),
            egui::StrokeKind::Outside,
        );
    }
    let response = ui.interact(area, ui.id().with(("open", index)), egui::Sense::click());
    let text = match label {
        Some(kind) => format!("{name} ({kind})"),
        None => name.clone(),
    };
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, text.clone())
    });
    ui.ctx()
        .accesskit_node_builder(response.id, |node| node.set_selected(selected));
    let name_line = Rect::from_min_size(
        egui::pos2(area.min.x, area.max.y),
        egui::vec2(side, NAME_HEIGHT),
    );
    ui.put(
        name_line,
        egui::Label::new(RichText::new(name).color(TEXT).size(12.0)).truncate(),
    );
    response.clicked()
}
