//! The tool edits of the shown image: shapes in image pixels, drawn over the
//! image on screen and into its pixels when saved, with undo and redo. The
//! save dialog that asks what becomes of them.

use std::path::{Path, PathBuf};

use super::{Command, Core, Event, Key, Points, Screen};
use crate::folder;

/// A tool of the toolbar. Later tools join this list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Arrow,
}

impl Tool {
    pub const ALL: [Self; 1] = [Self::Arrow];

    pub fn name(self) -> &'static str {
        match self {
            Self::Arrow => "Arrow",
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
}

/// A point in image pixels, of the image as decoded, before any pending turn.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ImagePoint {
    pub x: f32,
    pub y: f32,
}

/// One edit of the image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    Arrow {
        from: ImagePoint,
        to: ImagePoint,
        color: Color,
        size: Size,
    },
}

/// The filled polygons a shape is made of, in image pixels; the screen and
/// the saved file both draw these, so they look alike.
#[derive(Debug, Clone, PartialEq)]
pub struct Outline {
    pub color: Color,
    /// Each one convex.
    pub polygons: Vec<Vec<ImagePoint>>,
}

impl Shape {
    /// The shape on an image of `width` × `height` pixels; nothing for an
    /// arrow without length.
    pub fn outline(&self, width: u32, height: u32) -> Option<Outline> {
        let Self::Arrow {
            from,
            to,
            color,
            size,
        } = *self;
        let stroke = size.width(width, height);
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
        Some(Outline { color, polygons })
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
    /// The shape being drawn.
    pub drawing: Option<Shape>,
    pub can_undo: bool,
    pub can_redo: bool,
}

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
                | Event::Toolbar(_) => Some(Vec::new()),
                Event::SaveChoice(choice) => Some(self.choose(*choice)),
                _ => None,
            };
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
            ToolbarClick::Tool(tool) => self.toolbar.tool = tool,
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
    /// if it is on the image; clamped to the image when `clamp`.
    fn image_point(&self, at: Points, clamp: bool) -> Option<ImagePoint> {
        let (_, image) = self.drawable()?;
        let placed = self
            .view
            .zoom
            .placement(image, self.rotation, self.view.viewport);
        if placed.width <= 0.0 || placed.height <= 0.0 {
            return None;
        }
        let mut u = (at.x - placed.left) / placed.width;
        let mut v = (at.y - placed.top) / placed.height;
        if clamp {
            (u, v) = (u.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
        } else if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
            return None;
        }
        let (x, y) = unturned(self.rotation, u, v);
        Some(ImagePoint {
            x: x * image.width as f32,
            y: y * image.height as f32,
        })
    }

    /// A drag on the image starts a shape there.
    pub(super) fn draw_start(&mut self, at: Points) {
        let Some(point) = self.image_point(at, false) else {
            return;
        };
        let Some(path) = self.current.clone() else {
            return;
        };
        let Toolbar { tool, color, size } = self.toolbar;
        let shape = match tool {
            Tool::Arrow => Shape::Arrow {
                from: point,
                to: point,
                color,
                size,
            },
        };
        let edits = match &mut self.edits {
            Some(edits) if edits.path == path => edits,
            other => other.insert(Edits {
                path,
                shapes: Vec::new(),
                undone: Vec::new(),
                drawing: None,
            }),
        };
        edits.drawing = Some(shape);
    }

    /// The drag goes on: the shape follows it, within the image.
    pub(super) fn draw_move(&mut self, at: Points) {
        let point = self.image_point(at, true);
        if let (Some(point), Some(Shape::Arrow { to, .. })) =
            (point, self.edits.as_mut().and_then(|e| e.drawing.as_mut()))
        {
            *to = point;
        }
    }

    /// The drag ended: a shape with a length joins the edits.
    pub(super) fn draw_end(&mut self) {
        let Some(edits) = self.edits.as_mut() else {
            return;
        };
        let Some(shape) = edits.drawing.take() else {
            return;
        };
        let Shape::Arrow { from, to, .. } = shape;
        if (to.x - from.x).hypot(to.y - from.y) >= 1.0 {
            edits.shapes.push(shape);
            edits.undone.clear();
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
        self.view.edits = self
            .edits
            .as_ref()
            .filter(|_| !previewing)
            .map(|e| EditsView {
                shapes: e.shapes.clone(),
                drawing: e.drawing,
                can_undo: !e.shapes.is_empty(),
                can_redo: !e.undone.is_empty(),
            });
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
