//! Drawing tool edits into the pixels and writing the edited image: over the
//! file itself through a temporary file, as `rotate` does, or as a new file
//! that never replaces an existing one. The pixels are cropped and turned by
//! the pending rotation after the shapes are drawn, so the file looks like
//! the screen.

use std::fs;
use std::io::{Cursor, Write};
use std::path::Path;

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageFormat, RgbaImage};
use skrifa::instance::{LocationRef, Size as FontSize};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, MetadataProvider};
use tiny_skia::{
    ColorU8, FillRule, IntRect, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform,
};

use crate::core::{Block, ImagePoint, Mark, Shape, cropped, degrees};
use crate::decode::DecodedImage;
use crate::rotate::{self, Rotation};
use crate::{clipboard, decode, folder, motion, multi};

/// The JPEG quality edited images are written with.
const JPEG_QUALITY: u8 = 92;

/// `image` with `shapes` drawn into it, antialiased: first the pixelations,
/// from the pixels as decoded, then the other shapes in order, then the
/// crop that counts, as the screen shows them.
pub fn render(image: &DecodedImage, shapes: &[Shape]) -> Result<DecodedImage, String> {
    if shapes.is_empty() {
        return Ok(image.clone());
    }
    let (width, height) = (image.width, image.height);
    let mut pixmap = Pixmap::new(width, height).ok_or("the image has no pixels to draw on")?;
    if pixmap.pixels().len().saturating_mul(4) != image.rgba.len() {
        return Err("the pixels do not match the image size".to_owned());
    }
    let (rgba, _) = image.rgba.as_chunks::<4>();
    for (pixel, &[r, g, b, a]) in pixmap.pixels_mut().iter_mut().zip(rgba) {
        *pixel = ColorU8::from_rgba(r, g, b, a).premultiply();
    }
    for block in shapes.iter().flat_map(|s| s.blocks(image)) {
        pixelate(&mut pixmap, &block);
    }
    for shape in shapes {
        if let Some(mark) = shape.mark(width, height) {
            draw_mark(&mut pixmap, &mark);
        }
        if let Shape::Text {
            at,
            text,
            color,
            size,
            turn,
        } = shape
        {
            let [r, g, b] = color.rgb();
            let height = size.text_height(width, height);
            draw_text(&mut pixmap, text, *at, height, [r, g, b, 0xff], *turn)?;
        }
    }
    if let Some([left, top, w, h]) = cropped(shapes).and_then(|c| c.pixels(width, height)) {
        let rect = IntRect::from_xywh(
            i32::try_from(left).map_err(|e| e.to_string())?,
            i32::try_from(top).map_err(|e| e.to_string())?,
            w,
            h,
        )
        .ok_or("the crop is outside the image")?;
        pixmap = pixmap
            .clone_rect(rect)
            .ok_or("the crop is outside the image")?;
    }
    let rgba: Vec<u8> = pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let color = pixel.demultiply();
            [color.red(), color.green(), color.blue(), color.alpha()]
        })
        .collect();
    Ok(DecodedImage {
        width: pixmap.width(),
        height: pixmap.height(),
        rgba: rgba.into(),
    })
}

/// Fills `block` of `pixmap` with its colour.
fn pixelate(pixmap: &mut Pixmap, block: &Block) {
    let [r, g, b] = block.rgb;
    let color = ColorU8::from_rgba(r, g, b, 0xff).premultiply();
    let width = pixmap.width() as usize;
    let pixels = pixmap.pixels_mut();
    for y in block.top..block.top.saturating_add(block.height) {
        let start = (y as usize)
            .saturating_mul(width)
            .saturating_add(block.left as usize);
        let end = start.saturating_add(block.width as usize);
        if let Some(row) = pixels.get_mut(start..end) {
            row.fill(color);
        }
    }
}

fn paint(rgba: [u8; 4]) -> Paint<'static> {
    let [r, g, b, a] = rgba;
    let mut paint = Paint::default();
    paint.set_color_rgba8(r, g, b, a);
    paint.anti_alias = true;
    paint
}

/// Draws `mark` into `pixmap`.
fn draw_mark(pixmap: &mut Pixmap, mark: &Mark) {
    match mark {
        Mark::Fill { rgba, polygons } => {
            let paint = paint(*rgba);
            for polygon in polygons {
                if let Some(path) = polyline(polygon, true) {
                    pixmap.fill_path(
                        &path,
                        &paint,
                        FillRule::Winding,
                        Transform::identity(),
                        None,
                    );
                }
            }
        }
        Mark::Line {
            rgba,
            width,
            points,
            closed,
        } => {
            let Some(path) = polyline(points, *closed) else {
                return;
            };
            let (line_cap, line_join) = if *closed {
                (LineCap::Butt, LineJoin::Miter)
            } else {
                (LineCap::Round, LineJoin::Round)
            };
            let stroke = Stroke {
                width: *width,
                line_cap,
                line_join,
                ..Stroke::default()
            };
            pixmap.stroke_path(&path, &paint(*rgba), &stroke, Transform::identity(), None);
        }
    }
}

/// The path through `points`, back to the first when `closed`.
fn polyline(points: &[ImagePoint], closed: bool) -> Option<tiny_skia::Path> {
    let mut builder = PathBuilder::new();
    let mut points = points.iter();
    let first = points.next()?;
    builder.move_to(first.x, first.y);
    for point in points {
        builder.line_to(point.x, point.y);
    }
    if closed {
        builder.close();
    }
    builder.finish()
}

/// Draws `text` into `pixmap` in egui's default font, `height` pixels to the
/// em, from its top left corner `at`, turned against `turn` so that it is
/// upright once the image is turned by it.
fn draw_text(
    pixmap: &mut Pixmap,
    text: &str,
    at: ImagePoint,
    height: f32,
    rgba: [u8; 4],
    turn: Rotation,
) -> Result<(), String> {
    let font = FontRef::new(epaint_default_fonts::UBUNTU_LIGHT).map_err(|e| e.to_string())?;
    let size = FontSize::new(height);
    let location = LocationRef::default();
    let advances = font.glyph_metrics(size, location);
    let outlines = font.outline_glyphs();
    let charmap = font.charmap();
    let mut pen = GlyphPen {
        builder: PathBuilder::new(),
        x: 0.0,
        baseline: font.metrics(size, location).ascent,
    };
    for c in text.chars() {
        let Some(glyph) = charmap.map(c) else {
            continue;
        };
        if let Some(outline) = outlines.get(glyph) {
            // A glyph that cannot be drawn stays blank.
            let _ = outline.draw(DrawSettings::unhinted(size, location), &mut pen);
        }
        pen.x += advances.advance_width(glyph).unwrap_or(0.0);
    }
    let Some(path) = pen.builder.finish() else {
        return Ok(());
    };
    let transform = Transform::from_translate(at.x, at.y).pre_rotate(-degrees(turn));
    pixmap.fill_path(&path, &paint(rgba), FillRule::Winding, transform, None);
    Ok(())
}

/// Collects glyph outlines, given upwards from the baseline, into a path
/// downwards from the top of the line.
struct GlyphPen {
    builder: PathBuilder,
    /// Where the glyph starts.
    x: f32,
    baseline: f32,
}

impl OutlinePen for GlyphPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.builder.move_to(self.x + x, self.baseline - y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.builder.line_to(self.x + x, self.baseline - y);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.builder.quad_to(
            self.x + cx0,
            self.baseline - cy0,
            self.x + x,
            self.baseline - y,
        );
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.builder.cubic_to(
            self.x + cx0,
            self.baseline - cy0,
            self.x + cx1,
            self.baseline - cy1,
            self.x + x,
            self.baseline - y,
        );
    }

    fn close(&mut self) {
        self.builder.close();
    }
}

/// Writes `image` of the file `source` with `shapes` drawn into it and turned
/// by `rotation` to `target`, in the format its extension names. Over
/// `source` itself only when nothing else in the file would be lost; another
/// `target` must not exist yet. The error is meant to be shown to the user as
/// is.
pub fn save(
    source: &Path,
    image: &DecodedImage,
    rotation: Rotation,
    shapes: &[Shape],
    target: &Path,
) -> Result<(), String> {
    write(source, image, rotation, shapes, target)
        .map_err(|e| format!("Cannot save {}: {e}", folder::file_name(target)))
}

fn write(
    source: &Path,
    image: &DecodedImage,
    rotation: Rotation,
    shapes: &[Shape],
    target: &Path,
) -> Result<(), String> {
    let format = ImageFormat::from_path(target)
        .map_err(|_| "the file name has no image format pixpixpix can write".to_owned())?;
    if target == source {
        refuse_overwrite(source, format)?;
    } else if target.exists() {
        return Err("the file already exists".to_owned());
    }
    let edited = clipboard::turned(&render(image, shapes)?, rotation)?;
    let bytes = encode(&edited, format)?;
    if target == source {
        return rotate::replace(target, &bytes).map_err(|e| e.to_string());
    }
    create(source, target, &bytes)
}

/// Why writing over `source` would lose part of it.
fn refuse_overwrite(source: &Path, format: ImageFormat) -> Result<(), String> {
    let bytes = fs::read(source).map_err(|e| e.to_string())?;
    if decode::is_heif(source, &bytes) {
        return Err("HEIC/HEIF and AVIF files cannot be written".to_owned());
    }
    if multi::count(&bytes) > 1 {
        return Err("the other images in the file would be lost".to_owned());
    }
    if matches!(motion::find(source), Ok(Some(_))) {
        return Err("the embedded video would be lost".to_owned());
    }
    if rotate::animated(&bytes, format).map_err(|e| e.to_string())? {
        return Err("the other frames of the animation would be lost".to_owned());
    }
    Ok(())
}

/// The image encoded in `format`; JPEG without alpha.
fn encode(image: &DecodedImage, format: ImageFormat) -> Result<Vec<u8>, String> {
    let rgba = RgbaImage::from_raw(image.width, image.height, image.rgba.to_vec())
        .ok_or("the pixels do not match the image size")?;
    let image = DynamicImage::ImageRgba8(rgba);
    let mut out = Cursor::new(Vec::new());
    let result = match format {
        ImageFormat::Jpeg => {
            JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY).encode_image(&image.to_rgb8())
        }
        _ => image.write_to(&mut out, format),
    };
    result.map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

/// Writes a new file at `target` through a temporary file next to it, with
/// the permissions of `source`, failing if `target` appeared meanwhile.
fn create(source: &Path, target: &Path, bytes: &[u8]) -> Result<(), String> {
    let dir = target
        .parent()
        .ok_or("the file has no folder to be written to")?;
    let mut temp = tempfile::Builder::new()
        .prefix(".")
        .suffix(".pix3-tmp")
        .tempfile_in(dir)
        .map_err(|e| e.to_string())?;
    temp.write_all(bytes).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    if let Ok(metadata) = fs::metadata(source) {
        temp.as_file()
            .set_permissions(metadata.permissions())
            .map_err(|e| e.to_string())?;
    }
    temp.persist_noclobber(target)
        .map(|_| ())
        .map_err(|e| e.error.to_string())
}
