//! The tool edits of the shown image: shapes in image pixels, drawn over the
//! image on screen and into its pixels when saved, with undo and redo. The
//! save dialog that asks what becomes of them.

use std::path::{Path, PathBuf};

use super::{Command, Core, Event, Key, Points, Screen, Zoom};
use crate::decode::DecodedImage;
use crate::folder;
use crate::rotate::Rotation;

/// A tool of the toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Arrow,
    Text,
    Rectangle,
    Ellipse,
    Pen,
    Highlighter,
    Pixelate,
    Crop,
}

impl Tool {
    pub const ALL: [Self; 8] = [
        Self::Arrow,
        Self::Text,
        Self::Rectangle,
        Self::Ellipse,
        Self::Pen,
        Self::Highlighter,
        Self::Pixelate,
        Self::Crop,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Arrow => "Arrow",
            Self::Text => "Text",
            Self::Rectangle => "Rectangle",
            Self::Ellipse => "Ellipse",
            Self::Pen => "Pen",
            Self::Highlighter => "Highlighter",
            Self::Pixelate => "Pixelate",
            Self::Crop => "Crop",
        }
    }
}

/// A colour of the palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Red,
    Yellow,
    Green,
    Cyan,
    White,
    Black,
}

impl Color {
    pub const ALL: [Self; 6] = [
        Self::Red,
        Self::Yellow,
        Self::Green,
        Self::Cyan,
        Self::White,
        Self::Black,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Cyan => "cyan",
            Self::White => "white",
            Self::Black => "black",
        }
    }

    pub fn rgb(self) -> [u8; 3] {
        match self {
            Self::Red => [0xe5, 0x1c, 0x23],
            Self::Yellow => [0xff, 0xd6, 0x00],
            Self::Green => [0x2e, 0xc2, 0x4a],
            Self::Cyan => [0x00, 0xc8, 0xe6],
            Self::White => [0xff, 0xff, 0xff],
            Self::Black => [0x00, 0x00, 0x00],
        }
    }
}

/// A stroke size, relative to the image so marks look alike on any image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    Small,
    Medium,
    Large,
}

/// The thinnest stroke in image pixels.
const MIN_WIDTH: f32 = 2.0;
/// The smallest text in image pixels.
const MIN_TEXT: f32 = 10.0;
/// The smallest pixelation block in image pixels.
const MIN_BLOCK: f32 = 4.0;

impl Size {
    pub const ALL: [Self; 3] = [Self::Small, Self::Medium, Self::Large];

    pub fn name(self) -> &'static str {
        match self {
            Self::Small => "S",
            Self::Medium => "M",
            Self::Large => "L",
        }
    }

    /// The stroke width in pixels on an image of `width` × `height` pixels.
    pub fn width(self, width: u32, height: u32) -> f32 {
        let fraction = match self {
            Self::Small => 0.004,
            Self::Medium => 0.008,
            Self::Large => 0.016,
        };
        (width.max(height) as f32 * fraction).max(MIN_WIDTH)
    }

    /// The font size of text, its em in pixels, on an image of `width` ×
    /// `height` pixels.
    pub fn text_height(self, width: u32, height: u32) -> f32 {
        let fraction = match self {
            Self::Small => 0.015,
            Self::Medium => 0.03,
            Self::Large => 0.06,
        };
        (width.max(height) as f32 * fraction).max(MIN_TEXT)
    }

    /// The side of a pixelation block in pixels on an image of `width` ×
    /// `height` pixels.
    pub fn block(self, width: u32, height: u32) -> u32 {
        (self.width(width, height) * 3.0).round().max(MIN_BLOCK) as u32
    }
}

/// A point in image pixels, of the image as decoded, before any pending turn.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ImagePoint {
    pub x: f32,
    pub y: f32,
}

/// A box in image pixels, of the image as decoded.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Region {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Region {
    /// All of an image of `width` × `height` pixels.
    pub fn whole(width: u32, height: u32) -> Self {
        Self {
            left: 0.0,
            top: 0.0,
            right: width as f32,
            bottom: height as f32,
        }
    }

    /// The box with corners `a` and `b`.
    pub fn spanning(a: ImagePoint, b: ImagePoint) -> Self {
        Self {
            left: a.x.min(b.x),
            top: a.y.min(b.y),
            right: a.x.max(b.x),
            bottom: a.y.max(b.y),
        }
    }

    pub fn width(&self) -> f32 {
        self.right - self.left
    }

    pub fn height(&self) -> f32 {
        self.bottom - self.top
    }

    /// The point within the box nearest to `point`.
    pub fn clamp(&self, point: ImagePoint) -> ImagePoint {
        ImagePoint {
            x: point.x.clamp(self.left, self.right.max(self.left)),
            y: point.y.clamp(self.top, self.bottom.max(self.top)),
        }
    }

    /// The whole pixels of the box on an image of `width` × `height` pixels
    /// as left, top, width and height; nothing for an empty box.
    pub fn pixels(&self, width: u32, height: u32) -> Option<[u32; 4]> {
        let side = |at: f32, size: u32| at.round().clamp(0.0, size as f32) as u32;
        let (left, right) = (side(self.left, width), side(self.right, width));
        let (top, bottom) = (side(self.top, height), side(self.bottom, height));
        let w = right.checked_sub(left).filter(|w| *w > 0)?;
        let h = bottom.checked_sub(top).filter(|h| *h > 0)?;
        Some([left, top, w, h])
    }
}

/// One edit of the image.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Arrow {
        from: ImagePoint,
        to: ImagePoint,
        color: Color,
        size: Size,
    },
    /// The outline of the box between two corners.
    Rectangle {
        from: ImagePoint,
        to: ImagePoint,
        color: Color,
        size: Size,
    },
    /// The outline of the ellipse in the box between two corners.
    Ellipse {
        from: ImagePoint,
        to: ImagePoint,
        color: Color,
        size: Size,
    },
    /// A freehand line through the points.
    Pen {
        points: Vec<ImagePoint>,
        color: Color,
        size: Size,
    },
    /// A semi-transparent box between two corners.
    Highlighter {
        from: ImagePoint,
        to: ImagePoint,
        color: Color,
    },
    /// The box between two corners in blocks of one colour each, made from
    /// the pixels as decoded.
    Pixelate {
        from: ImagePoint,
        to: ImagePoint,
        size: Size,
    },
    /// A line of text from its top left corner, upright on the image turned
    /// by `turn`, as shown when it was placed.
    Text {
        at: ImagePoint,
        text: String,
        color: Color,
        size: Size,
        turn: Rotation,
    },
    /// Only this part of the image is kept, in whole pixels; the last crop
    /// counts.
    Crop(Region),
}

/// How opaque the highlighter is.
const HIGHLIGHTER_ALPHA: u8 = 0x60;
/// The corners of an ellipse.
const ELLIPSE_POINTS: usize = 96;

/// How a shape other than text, pixelation and crop is drawn, in image
/// pixels; the screen and the saved file both draw these, so they look alike.
#[derive(Debug, Clone, PartialEq)]
pub enum Mark {
    /// Filled polygons, each one convex.
    Fill {
        rgba: [u8; 4],
        polygons: Vec<Vec<ImagePoint>>,
    },
    /// A line of `width` through the points: open with round ends and joins,
    /// or closed with sharp corners.
    Line {
        rgba: [u8; 4],
        width: f32,
        points: Vec<ImagePoint>,
        closed: bool,
    },
}

/// One block of a pixelation, in image pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block {
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
    /// The mean colour of its pixels, opaque.
    pub rgb: [u8; 3],
}

fn opaque(color: Color) -> [u8; 4] {
    let [r, g, b] = color.rgb();
    [r, g, b, 0xff]
}

impl Shape {
    /// The shape on an image of `width` × `height` pixels; nothing for an
    /// arrow without length and for text, pixelation and crop.
    pub fn mark(&self, width: u32, height: u32) -> Option<Mark> {
        match self {
            Self::Arrow {
                from,
                to,
                color,
                size,
            } => arrow(*from, *to, size.width(width, height)).map(|polygons| Mark::Fill {
                rgba: opaque(*color),
                polygons,
            }),
            Self::Rectangle {
                from,
                to,
                color,
                size,
            } => Some(Mark::Line {
                rgba: opaque(*color),
                width: size.width(width, height),
                points: vec![*from, point(to.x, from.y), *to, point(from.x, to.y)],
                closed: true,
            }),
            Self::Ellipse {
                from,
                to,
                color,
                size,
            } => {
                let (cx, cy) = ((from.x + to.x) / 2.0, (from.y + to.y) / 2.0);
                let (rx, ry) = ((to.x - from.x).abs() / 2.0, (to.y - from.y).abs() / 2.0);
                let points = (0..ELLIPSE_POINTS)
                    .map(|i| {
                        let a = i as f32 / ELLIPSE_POINTS as f32 * std::f32::consts::TAU;
                        point(cx + rx * a.cos(), cy + ry * a.sin())
                    })
                    .collect();
                Some(Mark::Line {
                    rgba: opaque(*color),
                    width: size.width(width, height),
                    points,
                    closed: true,
                })
            }
            Self::Pen {
                points,
                color,
                size,
            } => Some(Mark::Line {
                rgba: opaque(*color),
                width: size.width(width, height),
                points: points.clone(),
                closed: false,
            }),
            Self::Highlighter { from, to, color } => {
                let [r, g, b] = color.rgb();
                Some(Mark::Fill {
                    rgba: [r, g, b, HIGHLIGHTER_ALPHA],
                    polygons: vec![vec![*from, point(to.x, from.y), *to, point(from.x, to.y)]],
                })
            }
            Self::Pixelate { .. } | Self::Text { .. } | Self::Crop(_) => None,
        }
    }

    /// The blocks of a pixelation of `image`; none for other shapes.
    pub fn blocks(&self, image: &DecodedImage) -> Vec<Block> {
        let Self::Pixelate { from, to, size } = self else {
            return Vec::new();
        };
        let Some([left, top, width, height]) =
            Region::spanning(*from, *to).pixels(image.width, image.height)
        else {
            return Vec::new();
        };
        let side = size.block(image.width, image.height);
        let (right, bottom) = (left.saturating_add(width), top.saturating_add(height));
        let mut blocks = Vec::new();
        for y in (top..bottom).step_by(side as usize) {
            for x in (left..right).step_by(side as usize) {
                let block = Block {
                    left: x,
                    top: y,
                    width: side.min(right.saturating_sub(x)),
                    height: side.min(bottom.saturating_sub(y)),
                    rgb: [0; 3],
                };
                blocks.push(Block {
                    rgb: mean(image, &block),
                    ..block
                });
            }
        }
        blocks
    }
}

/// The mean colour of the pixels of `block` in `image`.
fn mean(image: &DecodedImage, block: &Block) -> [u8; 3] {
    let mut sum = [0u64; 3];
    let mut count = 0u64;
    let row = |y: u32| -> Option<&[u8]> {
        let start = (y as usize)
            .checked_mul(image.width as usize)?
            .checked_add(block.left as usize)?
            .checked_mul(4)?;
        let end = start.checked_add((block.width as usize).checked_mul(4)?)?;
        image.rgba.get(start..end)
    };
    for y in block.top..block.top.saturating_add(block.height) {
        let Some(row) = row(y) else {
            continue;
        };
        for &[r, g, b, _] in row.as_chunks::<4>().0 {
            for (total, value) in sum.iter_mut().zip([r, g, b]) {
                *total = total.saturating_add(u64::from(value));
            }
            count = count.saturating_add(1);
        }
    }
    sum.map(|total| total.checked_div(count).unwrap_or(0) as u8)
}

fn point(x: f32, y: f32) -> ImagePoint {
    ImagePoint { x, y }
}

/// The polygons of an arrow from `from` to `to` with a shaft of `stroke`;
/// nothing for an arrow without length.
fn arrow(from: ImagePoint, to: ImagePoint, stroke: f32) -> Option<Vec<Vec<ImagePoint>>> {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let length = dx.hypot(dy);
    if length < 1.0 {
        return None;
    }
    let (ux, uy) = (dx / length, dy / length);
    // The normal, to the left of the direction.
    let (nx, ny) = (-uy, ux);
    let head = (stroke * 4.0).min(length);
    let spread = head * 0.65;
    let base = ImagePoint {
        x: to.x - ux * head,
        y: to.y - uy * head,
    };
    let side = |p: ImagePoint, by: f32| ImagePoint {
        x: p.x + nx * by,
        y: p.y + ny * by,
    };
    let half = stroke / 2.0;
    let mut polygons = vec![vec![side(base, spread), to, side(base, -spread)]];
    if head < length {
        polygons.insert(
            0,
            vec![
                side(from, half),
                side(base, half),
                side(base, -half),
                side(from, -half),
            ],
        );
    }
    Some(polygons)
}

/// The crop that counts among `shapes`, if any.
pub fn cropped(shapes: &[Shape]) -> Option<Region> {
    shapes.iter().rev().find_map(|shape| match shape {
        Shape::Crop(region) => Some(*region),
        _ => None,
    })
}

/// How far `rotation` turns clockwise, in degrees.
pub fn degrees(rotation: Rotation) -> f32 {
    match rotation {
        Rotation::None => 0.0,
        Rotation::Right => 90.0,
        Rotation::Half => 180.0,
        Rotation::Left => 270.0,
    }
}

/// The toolbar's choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Toolbar {
    pub tool: Tool,
    pub color: Color,
    pub size: Size,
}

impl Toolbar {
    pub(super) const DEFAULT: Self = Self {
        tool: Tool::Arrow,
        color: Color::Red,
        size: Size::Medium,
    };
}

/// A click in the toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarClick {
    Tool(Tool),
    Color(Color),
    Size(Size),
    Undo,
    Redo,
    Settings,
}

/// The edits of the shown image as the UI draws them.
#[derive(Debug, Clone, PartialEq)]
pub struct EditsView {
    /// The finished shapes, oldest first.
    pub shapes: Vec<Shape>,
    /// The shape being drawn; text while it is typed.
    pub drawing: Option<Shape>,
    pub can_undo: bool,
    pub can_redo: bool,
    /// While the crop tool shows the whole image: the part kept, with drag
    /// handles.
    pub cropping: Option<Region>,
}

/// How far from a crop handle, in points on screen, a drag still grabs it.
pub const HANDLE_REACH: f32 = 10.0;

/// What the save dialog shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveDialog {
    /// The name "Save as new" writes, next to the file.
    pub name: String,
    /// Why "Overwrite" is not offered for this file.
    pub overwrite_refused: Option<String>,
    /// Why the last choice failed.
    pub error: Option<String>,
    /// A save or copy runs; the dialog waits for it.
    pub busy: bool,
}

/// A choice in the save dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveChoice {
    Overwrite,
    SaveAsNew,
    Copy,
    /// Drop the edits.
    Discard,
    /// Close the dialog and keep editing.
    Back,
}

/// The edits of one file.
#[derive(Debug)]
pub(super) struct Edits {
    path: PathBuf,
    shapes: Vec<Shape>,
    /// Undone shapes, the last undone last.
    undone: Vec<Shape>,
    drawing: Option<Shape>,
    /// How the crop being drawn follows the drag.
    crop_drag: Option<CropDrag>,
}

/// A drag of the crop box: the sides that move span from `anchor` to the
/// pointer.
#[derive(Debug, Clone, Copy)]
struct CropDrag {
    anchor: ImagePoint,
    moves_x: bool,
    moves_y: bool,
}

/// The save dialog's state.
#[derive(Debug)]
pub(super) struct Dialog {
    name: String,
    overwrite_refused: Option<String>,
    error: Option<String>,
    busy: Option<Busy>,
}

#[derive(Debug, PartialEq)]
enum Busy {
    /// Writing the edited image to this file.
    Saving(PathBuf),
    Copying,
}

impl Core {
    /// Edits with at least one shape for the current file.
    fn unsaved(&self) -> Option<&Edits> {
        self.edits
            .as_ref()
            .filter(|e| !e.shapes.is_empty() && self.current.as_ref() == Some(&e.path))
    }

    /// The image the tools draw on, if they can: a still image shown as the
    /// current file in the image view, its rotation not being written.
    fn drawable(&self) -> Option<(&Path, &crate::decode::DecodedImage)> {
        if !self.toolbar_open
            || self.dialog.is_some()
            || self.view.settings_open
            || self.view.overview.is_some()
            || self.view.focus != super::Focus::Image
        {
            return None;
        }
        let Screen::Image { path, image } = &self.screen else {
            return None;
        };
        let still = !folder::is_video(path)
            && !self.animated.contains(path)
            && self.video.as_ref().is_none_or(|v| !v.started)
            && self.writing.as_ref() != Some(path);
        (still && self.current.as_ref() == Some(path)).then_some((path.as_path(), image))
    }

    /// Answers `event` here when it belongs to the save dialog or would leave
    /// unsaved edits behind; the latter waits while the dialog asks first.
    pub(super) fn guard(&mut self, event: &Event) -> Option<Vec<Command>> {
        if self.dialog.is_some() {
            return match event {
                Event::Key(key) if !key.command => match key.key {
                    Key::Enter => Some(self.choose(SaveChoice::SaveAsNew)),
                    Key::Escape => Some(self.choose(SaveChoice::Back)),
                    _ => Some(Vec::new()),
                },
                Event::CloseRequested => {
                    self.parked = Some(Event::CloseRequested);
                    Some(vec![Command::KeepOpen])
                }
                Event::Key(_)
                | Event::Show(_)
                | Event::Clicked
                | Event::Pan(_)
                | Event::DrawStart(_)
                | Event::DrawMove(_)
                | Event::DrawEnd
                | Event::Toolbar(_)
                | Event::TextEdited(_)
                | Event::TextDone => Some(Vec::new()),
                Event::SaveChoice(choice) => Some(self.choose(*choice)),
                _ => None,
            };
        }
        // A key or a move means the text field let go; its text is done.
        if matches!(
            event,
            Event::Key(_) | Event::Show(_) | Event::CloseRequested | Event::Toolbar(_)
        ) {
            self.commit_text();
        }
        self.unsaved()?;
        // The video of a Live Photo or Motion Photo would replace the still
        // drawn on.
        let video = self.video.is_some();
        let ask = match event {
            Event::Show(_) | Event::CloseRequested => true,
            Event::Key(key) if self.view.settings_open => key.command && key.key == Key::Q,
            Event::Key(key) if key.command => {
                key.key == Key::Q || (video && matches!(key.key, Key::Left | Key::Right))
            }
            Event::Key(key) => match key.key {
                Key::Left | Key::Right => video || (!key.shift && !key.alt),
                Key::Space => video,
                Key::Up | Key::Down => self.inner.is_some(),
                Key::X | Key::G | Key::N | Key::B | Key::Tab => true,
                Key::Enter => {
                    self.open_dialog();
                    return Some(Vec::new());
                }
                _ => false,
            },
            _ => false,
        };
        if !ask {
            return None;
        }
        self.parked = Some(event.clone());
        self.open_dialog();
        Some(if *event == Event::CloseRequested {
            vec![Command::KeepOpen]
        } else {
            Vec::new()
        })
    }

    fn open_dialog(&mut self) {
        let Some(path) = self.current.clone() else {
            return;
        };
        self.view.settings_open = false;
        self.dialog = Some(Dialog {
            name: self.new_name(&path),
            overwrite_refused: self.overwrite_refused(&path),
            error: None,
            busy: None,
        });
    }

    /// `<name>_edit.<ext>` next to `path`, or `_edit2`, `_edit3`… when that is
    /// taken. A format that cannot be written becomes PNG.
    fn new_name(&self, path: &Path) -> String {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let extension = match path.extension() {
            Some(_) if folder::is_heif(path) => "png".to_owned(),
            Some(e) => e.to_string_lossy().into_owned(),
            None => "png".to_owned(),
        };
        let taken = |name: &str| {
            let candidate = path.with_file_name(name);
            self.all_files.iter().flatten().any(|f| *f == candidate)
        };
        let mut number = 1u32;
        loop {
            let suffix = if number == 1 {
                String::new()
            } else {
                number.to_string()
            };
            let name = format!("{stem}_edit{suffix}.{extension}");
            if !taken(&name) || number == u32::MAX {
                return name;
            }
            number = number.saturating_add(1);
        }
    }

    /// Why the edits cannot be written into the file itself, as far as known.
    fn overwrite_refused(&self, path: &Path) -> Option<String> {
        if folder::is_heif(path) {
            Some("HEIC/HEIF and AVIF files cannot be written".to_owned())
        } else if self.images.get(path).is_some_and(|n| *n > 1) {
            Some("the other images in the file would be lost".to_owned())
        } else if self.motion_photos.contains(path) {
            Some("the embedded video would be lost".to_owned())
        } else {
            None
        }
    }

    /// Acts on a choice of the save dialog; a save or copy is waited for.
    fn choose(&mut self, choice: SaveChoice) -> Vec<Command> {
        let Some(dialog) = self.dialog.as_mut().filter(|d| d.busy.is_none()) else {
            return Vec::new();
        };
        let (Some(edits), Screen::Image { path, image }) = (&self.edits, &self.screen) else {
            return self.finish();
        };
        let (path, image, shapes) = (path.clone(), image.clone(), edits.shapes.clone());
        match choice {
            SaveChoice::Back => {
                self.dialog = None;
                self.parked = None;
                Vec::new()
            }
            SaveChoice::Discard => self.finish(),
            SaveChoice::Overwrite if dialog.overwrite_refused.is_some() => Vec::new(),
            SaveChoice::Overwrite | SaveChoice::SaveAsNew => {
                let target = if choice == SaveChoice::Overwrite {
                    path.clone()
                } else {
                    let name = dialog.name.trim();
                    if name.is_empty() || name.contains(['/', '\\']) {
                        dialog.error = Some("Enter a file name without folders".to_owned());
                        return Vec::new();
                    }
                    path.with_file_name(name)
                };
                dialog.error = None;
                dialog.busy = Some(Busy::Saving(target.clone()));
                if target == path {
                    self.writing = Some(path.clone());
                }
                vec![Command::SaveEdits {
                    source: path,
                    image,
                    rotation: self.rotation,
                    shapes,
                    target,
                }]
            }
            SaveChoice::Copy => {
                dialog.error = None;
                dialog.busy = Some(Busy::Copying);
                self.view.copy_error = None;
                vec![Command::Copy {
                    path,
                    image,
                    rotation: self.rotation,
                    shapes,
                }]
            }
        }
    }

    /// The edits are dealt with: they go, and the action the dialog
    /// interrupted goes on.
    fn finish(&mut self) -> Vec<Command> {
        self.edits = None;
        self.dialog = None;
        match self.parked.take() {
            Some(event) => self.dispatch(event),
            None => Vec::new(),
        }
    }

    /// A [`Command::SaveEdits`] finished: the edits are saved, or the dialog
    /// says why not. A file written over is decoded again.
    pub(super) fn edits_saved(
        &mut self,
        target: PathBuf,
        result: Result<(), String>,
    ) -> Vec<Command> {
        let Some(dialog) = self
            .dialog
            .as_mut()
            .filter(|d| d.busy == Some(Busy::Saving(target.clone())))
        else {
            return Vec::new();
        };
        dialog.busy = None;
        let overwritten = self.writing.as_ref() == Some(&target);
        match result {
            Ok(()) => {
                let mut commands = if overwritten {
                    self.rotation_written(target, Ok(()))
                } else {
                    Vec::new()
                };
                commands.extend(self.finish());
                commands
            }
            Err(message) => {
                dialog.error = Some(message);
                if overwritten {
                    self.writing = None;
                }
                Vec::new()
            }
        }
    }

    /// A [`Command::Copy`] finished; one the dialog waits for ends it.
    pub(super) fn edits_copied(&mut self, result: &Result<(), String>) -> Vec<Command> {
        let Some(dialog) = self
            .dialog
            .as_mut()
            .filter(|d| d.busy == Some(Busy::Copying))
        else {
            return Vec::new();
        };
        dialog.busy = None;
        match result {
            Ok(()) => self.finish(),
            Err(message) => {
                dialog.error = Some(message.clone());
                Vec::new()
            }
        }
    }

    /// The name "Save as new" writes was edited.
    pub(super) fn rename(&mut self, name: String) {
        if let Some(dialog) = self.dialog.as_mut().filter(|d| d.busy.is_none()) {
            dialog.name = name;
        }
    }

    /// `p`: shows or hides the toolbar.
    pub(super) fn toggle_toolbar(&mut self) {
        self.toolbar_open = !self.toolbar_open;
    }

    pub(super) fn toolbar_click(&mut self, click: ToolbarClick) {
        match click {
            ToolbarClick::Tool(tool) => {
                self.commit_text();
                if let Some(edits) = self.edits.as_mut() {
                    edits.drawing = None;
                    edits.crop_drag = None;
                }
                self.toolbar.tool = tool;
            }
            ToolbarClick::Color(color) => self.toolbar.color = color,
            ToolbarClick::Size(size) => self.toolbar.size = size,
            ToolbarClick::Undo => self.undo(),
            ToolbarClick::Redo => self.redo(),
            ToolbarClick::Settings => self.open_settings(),
        }
    }

    /// Takes back the last shape.
    pub(super) fn undo(&mut self) {
        if let Some(edits) = self.edits.as_mut()
            && let Some(shape) = edits.shapes.pop()
        {
            edits.undone.push(shape);
        }
    }

    /// Brings back the last shape taken back.
    pub(super) fn redo(&mut self) {
        if let Some(edits) = self.edits.as_mut()
            && let Some(shape) = edits.undone.pop()
        {
            edits.shapes.push(shape);
        }
    }

    /// The image point under `at`, given from the viewport's top left corner,
    /// wherever it is, with the part of the image shown and the points on
    /// screen per image pixel.
    fn pointed(&self, at: Points) -> Option<(ImagePoint, Region, f32)> {
        let (_, image) = self.drawable()?;
        let region = self
            .view
            .crop
            .unwrap_or(Region::whole(image.width, image.height));
        let placed = self
            .view
            .zoom
            .region_placement(region, self.rotation, self.view.viewport);
        let shown_width = if self.rotation.sideways() {
            region.height()
        } else {
            region.width()
        };
        if placed.width <= 0.0 || placed.height <= 0.0 || shown_width <= 0.0 {
            return None;
        }
        let u = (at.x - placed.left) / placed.width;
        let v = (at.y - placed.top) / placed.height;
        let (x, y) = unturned(self.rotation, u, v);
        let point = ImagePoint {
            x: region.left + x * region.width(),
            y: region.top + y * region.height(),
        };
        Some((point, region, placed.width / shown_width))
    }

    /// A drag on the image starts a shape there, or grabs a handle of the
    /// crop box; a click with the text tool places text there. Text being
    /// typed is done.
    pub(super) fn draw_start(&mut self, at: Points) {
        let Some((point, region, scale)) = self.pointed(at) else {
            return;
        };
        let Some(path) = self.current.clone() else {
            return;
        };
        let Toolbar { tool, color, size } = self.toolbar;
        let turn = self.rotation;
        let edits = match &mut self.edits {
            Some(edits) if edits.path == path => edits,
            other => other.insert(Edits {
                path,
                shapes: Vec::new(),
                undone: Vec::new(),
                drawing: None,
                crop_drag: None,
            }),
        };
        if matches!(edits.drawing, Some(Shape::Text { .. })) {
            edits.commit();
            if tool == Tool::Text {
                return;
            }
        }
        if tool == Tool::Crop {
            let current = cropped(&edits.shapes).unwrap_or(region);
            if let Some(drag) = grab(current, point, HANDLE_REACH / scale) {
                edits.drawing = Some(Shape::Crop(current));
                edits.crop_drag = Some(drag);
                return;
            }
        }
        if region.clamp(point) != point {
            return;
        }
        let (from, to) = (point, point);
        edits.drawing = Some(match tool {
            Tool::Arrow => Shape::Arrow {
                from,
                to,
                color,
                size,
            },
            Tool::Text => Shape::Text {
                at: point,
                text: String::new(),
                color,
                size,
                turn,
            },
            Tool::Rectangle => Shape::Rectangle {
                from,
                to,
                color,
                size,
            },
            Tool::Ellipse => Shape::Ellipse {
                from,
                to,
                color,
                size,
            },
            Tool::Pen => Shape::Pen {
                points: vec![point],
                color,
                size,
            },
            Tool::Highlighter => Shape::Highlighter { from, to, color },
            Tool::Pixelate => Shape::Pixelate { from, to, size },
            Tool::Crop => {
                edits.crop_drag = Some(CropDrag {
                    anchor: point,
                    moves_x: true,
                    moves_y: true,
                });
                Shape::Crop(Region::spanning(point, point))
            }
        });
    }

    /// The drag goes on: the shape follows it, within the image.
    pub(super) fn draw_move(&mut self, at: Points) {
        let Some((point, region, _)) = self.pointed(at) else {
            return;
        };
        let point = region.clamp(point);
        let Some(edits) = self.edits.as_mut() else {
            return;
        };
        let drag = edits.crop_drag;
        match edits.drawing.as_mut() {
            Some(
                Shape::Arrow { to, .. }
                | Shape::Rectangle { to, .. }
                | Shape::Ellipse { to, .. }
                | Shape::Highlighter { to, .. }
                | Shape::Pixelate { to, .. },
            ) => *to = point,
            Some(Shape::Pen { points, .. }) => {
                if points
                    .last()
                    .is_none_or(|last| (point.x - last.x).hypot(point.y - last.y) >= 1.0)
                {
                    points.push(point);
                }
            }
            Some(Shape::Crop(crop)) => {
                if let Some(drag) = drag {
                    if drag.moves_x {
                        crop.left = drag.anchor.x.min(point.x);
                        crop.right = drag.anchor.x.max(point.x);
                    }
                    if drag.moves_y {
                        crop.top = drag.anchor.y.min(point.y);
                        crop.bottom = drag.anchor.y.max(point.y);
                    }
                }
            }
            Some(Shape::Text { .. }) | None => {}
        }
    }

    /// The drag ended: a shape with a size joins the edits, a crop box in
    /// whole pixels. Text stays to be typed.
    pub(super) fn draw_end(&mut self) {
        let whole = self
            .drawable()
            .map(|(_, image)| Region::whole(image.width, image.height));
        let Some(edits) = self.edits.as_mut() else {
            return;
        };
        edits.crop_drag = None;
        match edits.drawing.take() {
            Some(text @ Shape::Text { .. }) => edits.drawing = Some(text),
            Some(Shape::Crop(crop)) => {
                let crop = Region {
                    left: crop.left.round(),
                    top: crop.top.round(),
                    right: crop.right.round(),
                    bottom: crop.bottom.round(),
                };
                if crop.width() >= 1.0
                    && crop.height() >= 1.0
                    && Some(crop) != cropped(&edits.shapes).or(whole)
                {
                    edits.add(Shape::Crop(crop));
                }
            }
            Some(shape) => edits.add(shape),
            None => {}
        }
    }

    /// The text being typed changed.
    pub(super) fn text_edited(&mut self, typed: String) {
        if let Some(Shape::Text { text, .. }) = self.edits.as_mut().and_then(|e| e.drawing.as_mut())
        {
            *text = typed;
        }
    }

    /// The text being typed is done: unless empty, it joins the edits.
    pub(super) fn commit_text(&mut self) {
        if let Some(edits) = self.edits.as_mut()
            && matches!(edits.drawing, Some(Shape::Text { .. }))
        {
            edits.commit();
        }
    }

    /// Edits of a file no longer shown go; so does a dialog about them that
    /// waits for nothing.
    pub(super) fn sync_edits(&mut self) {
        if self
            .edits
            .as_ref()
            .is_some_and(|e| self.current.as_ref() != Some(&e.path))
        {
            self.edits = None;
            if self.dialog.as_ref().is_some_and(|d| d.busy.is_none()) {
                self.dialog = None;
                self.parked = None;
            }
        }
    }

    /// Puts toolbar, edits and dialog into the view; edits show only over
    /// their own file.
    pub(super) fn publish_edits(&mut self, previewing: bool) {
        self.view.toolbar = self.toolbar_open.then_some(self.toolbar);
        // The crop tool shows the whole image, with the part kept framed.
        let cropping = self.toolbar_open && self.toolbar.tool == Tool::Crop;
        let edits = self.edits.as_ref().filter(|_| !previewing);
        self.view.edits = edits.map(|e| EditsView {
            shapes: e.shapes.clone(),
            drawing: e.drawing.clone(),
            can_undo: !e.shapes.is_empty(),
            can_redo: !e.undone.is_empty(),
            cropping: match &e.drawing {
                _ if !cropping => None,
                Some(Shape::Crop(crop)) => Some(*crop),
                _ => cropped(&e.shapes),
            },
        });
        let crop = edits.filter(|_| !cropping).and_then(|e| cropped(&e.shapes));
        // Zoom and position are of the part shown; another part fits anew.
        if crop != self.view.crop {
            self.view.crop = crop;
            self.view.zoom = Zoom::FIT;
        }
        self.view.save_dialog = self.dialog.as_ref().map(|d| SaveDialog {
            name: d.name.clone(),
            overwrite_refused: d.overwrite_refused.clone(),
            error: d.error.clone(),
            busy: d.busy.is_some(),
        });
    }

    /// Whether the edits of the current file are unsaved.
    pub(super) fn has_unsaved(&self) -> bool {
        self.unsaved().is_some()
    }
}

impl Edits {
    /// A finished shape joins the edits if it has a size, and nothing is
    /// left to redo.
    fn add(&mut self, shape: Shape) {
        let sized = |from: &ImagePoint, to: &ImagePoint| {
            (to.x - from.x).abs() >= 1.0 && (to.y - from.y).abs() >= 1.0
        };
        let keep = match &shape {
            Shape::Arrow { from, to, .. } => (to.x - from.x).hypot(to.y - from.y) >= 1.0,
            Shape::Rectangle { from, to, .. }
            | Shape::Ellipse { from, to, .. }
            | Shape::Highlighter { from, to, .. }
            | Shape::Pixelate { from, to, .. } => sized(from, to),
            Shape::Pen { points, .. } => points.len() > 1,
            Shape::Text { text, .. } => !text.trim().is_empty(),
            Shape::Crop(_) => true,
        };
        if keep {
            self.shapes.push(shape);
            self.undone.clear();
        }
    }

    /// The shape being drawn joins the edits as it is.
    fn commit(&mut self) {
        if let Some(shape) = self.drawing.take() {
            self.add(shape);
        }
    }
}

/// The drag of the crop box `crop` by the handle within `reach` of `at`
/// nearest to it, if any: a corner moves two sides, an edge's middle one.
fn grab(crop: Region, at: ImagePoint, reach: f32) -> Option<CropDrag> {
    let middle = |a: f32, b: f32| (a + b) / 2.0;
    // Each handle's place and, if it moves that side, the opposite side.
    let xs = [
        (crop.left, Some(crop.right)),
        (middle(crop.left, crop.right), None),
        (crop.right, Some(crop.left)),
    ];
    let ys = [
        (crop.top, Some(crop.bottom)),
        (middle(crop.top, crop.bottom), None),
        (crop.bottom, Some(crop.top)),
    ];
    let mut nearest: Option<(f32, CropDrag)> = None;
    for (x, anchor_x) in xs {
        for (y, anchor_y) in ys {
            if anchor_x.is_none() && anchor_y.is_none() {
                continue;
            }
            let distance = (at.x - x).hypot(at.y - y);
            if distance <= reach && nearest.is_none_or(|(d, _)| distance < d) {
                let drag = CropDrag {
                    anchor: point(anchor_x.unwrap_or(x), anchor_y.unwrap_or(y)),
                    moves_x: anchor_x.is_some(),
                    moves_y: anchor_y.is_some(),
                };
                nearest = Some((distance, drag));
            }
        }
    }
    nearest.map(|(_, drag)| drag)
}

/// The point of the image as decoded at `(u, v)` of the image shown turned by
/// `rotation`, all as fractions of width and height.
pub fn unturned(rotation: crate::rotate::Rotation, u: f32, v: f32) -> (f32, f32) {
    use crate::rotate::Rotation;
    match rotation {
        Rotation::None => (u, v),
        Rotation::Right => (v, 1.0 - u),
        Rotation::Half => (1.0 - u, 1.0 - v),
        Rotation::Left => (1.0 - v, u),
    }
}

/// Where the point `(x, y)` of the image as decoded shows on the image turned
/// by `rotation`, all as fractions of width and height; the inverse of
/// [`unturned`].
pub fn turned(rotation: crate::rotate::Rotation, x: f32, y: f32) -> (f32, f32) {
    use crate::rotate::Rotation;
    match rotation {
        Rotation::None => (x, y),
        Rotation::Right => (1.0 - y, x),
        Rotation::Half => (1.0 - x, 1.0 - y),
        Rotation::Left => (y, 1.0 - x),
    }
}
