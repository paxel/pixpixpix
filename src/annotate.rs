//! Drawing tool edits into the pixels and writing the edited image: over the
//! file itself through a temporary file, as `rotate` does, or as a new file
//! that never replaces an existing one. The pixels are cropped and turned by
//! the pending rotation after the shapes are drawn, so the file looks like
//! the screen, and keeps the original's metadata where its format can hold
//! it. Exports: an area of the edited image scaled to a given size, written
//! without metadata, within a size limit if asked.

use std::fs;
use std::io::{Cursor, Write};
use std::path::Path;

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType as PngFilter, PngEncoder};
use image::codecs::webp::WebPEncoder;
use image::imageops::{self, FilterType};
use image::{DynamicImage, ExtendedColorType, ImageEncoder, ImageFormat, RgbaImage};
use skrifa::instance::{LocationRef, Size as FontSize};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, MetadataProvider};
use tiny_skia::{
    ColorU8, FillRule, IntRect, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform,
};

use crate::core::{Block, Export, ExportFormat, ImagePoint, Mark, Region, Shape, cropped, degrees};
use crate::decode::DecodedImage;
use crate::metadata::{self, Kept};
use crate::rotate::{self, Rotation};
use crate::{clipboard, decode, folder, motion, multi};

/// The JPEG quality edited images are written with.
const JPEG_QUALITY: u8 = 92;
/// The lowest JPEG quality a size limit may lower an export to.
const MIN_JPEG_QUALITY: u8 = 10;

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
/// by `rotation` to `target`, in the format its extension names, with the
/// EXIF, XMP and colour profile of `source` where the format holds them. Over
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
    // A video frame keeps nothing of the video.
    let original = if target == source {
        let original = fs::read(source).map_err(|e| e.to_string())?;
        refuse_overwrite(source, &original, format)?;
        Some(original)
    } else if target.exists() {
        return Err("the file already exists".to_owned());
    } else if folder::is_video(source) {
        None
    } else {
        fs::read(source).ok()
    };
    let kept = original
        .map(|original| metadata::kept(source, &original))
        .unwrap_or_default();
    let edited = clipboard::turned(&render(image, shapes)?, rotation)?;
    let bytes = encode(&edited, format, &kept)?;
    if target == source {
        return rotate::replace(target, &bytes).map_err(|e| e.to_string());
    }
    create(source, target, &bytes)
}

/// Why writing over `source`, held in `bytes`, would lose part of it.
fn refuse_overwrite(source: &Path, bytes: &[u8], format: ImageFormat) -> Result<(), String> {
    if decode::is_heif(source, bytes) {
        return Err("HEIC/HEIF and AVIF files cannot be written".to_owned());
    }
    if multi::count(bytes) > 1 {
        return Err("the other images in the file would be lost".to_owned());
    }
    if matches!(motion::find(source), Ok(Some(_))) {
        return Err("the embedded video would be lost".to_owned());
    }
    if rotate::animated(bytes, format).map_err(|e| e.to_string())? {
        return Err("the other frames of the animation would be lost".to_owned());
    }
    Ok(())
}

/// The image encoded in `format`, with the metadata `kept` that JPEG, PNG
/// and WebP hold; JPEG without alpha.
fn encode(image: &DecodedImage, format: ImageFormat, kept: &Kept) -> Result<Vec<u8>, String> {
    let rgba = RgbaImage::from_raw(image.width, image.height, image.rgba.to_vec())
        .ok_or("the pixels do not match the image size")?;
    let (width, height) = rgba.dimensions();
    let mut out = Vec::new();
    match format {
        ImageFormat::Jpeg => {
            let mut encoder = JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY);
            if kept
                .exif
                .as_deref()
                .is_some_and(|e| !metadata::fits_jpeg(e))
            {
                return Err("its EXIF is too large for a JPEG".to_owned());
            }
            keep(&mut encoder, kept)?;
            let rgb = DynamicImage::ImageRgba8(rgba).to_rgb8();
            encoder
                .write_image(rgb.as_raw(), width, height, ExtendedColorType::Rgb8)
                .map_err(|e| e.to_string())?;
        }
        ImageFormat::Png => {
            let mut encoder = PngEncoder::new(&mut out);
            keep(&mut encoder, kept)?;
            encoder
                .write_image(rgba.as_raw(), width, height, ExtendedColorType::Rgba8)
                .map_err(|e| e.to_string())?;
        }
        ImageFormat::WebP => {
            let mut encoder = image_webp::WebPEncoder::new(&mut out);
            encoder.set_exif_metadata(kept.exif.clone().unwrap_or_default());
            encoder.set_icc_profile(kept.icc.clone().unwrap_or_default());
            encoder.set_xmp_metadata(kept.xmp.clone().unwrap_or_default());
            encoder
                .encode(rgba.as_raw(), width, height, image_webp::ColorType::Rgba8)
                .map_err(|e| e.to_string())?;
            return Ok(out);
        }
        _ => {
            let mut cursor = Cursor::new(Vec::new());
            DynamicImage::ImageRgba8(rgba)
                .write_to(&mut cursor, format)
                .map_err(|e| e.to_string())?;
            return Ok(cursor.into_inner());
        }
    }
    match &kept.xmp {
        Some(xmp) => metadata::with_xmp(&out, format, xmp),
        None => Ok(out),
    }
}

/// Hands the EXIF and colour profile of `kept` to `encoder`.
fn keep(encoder: &mut impl ImageEncoder, kept: &Kept) -> Result<(), String> {
    if let Some(exif) = &kept.exif {
        encoder
            .set_exif_metadata(exif.clone())
            .map_err(|e| e.to_string())?;
    }
    if let Some(icc) = &kept.icc {
        encoder
            .set_icc_profile(icc.clone())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Writes a new file at `target` through a temporary file next to it, with
/// the permissions of `source`, failing if `target` appeared meanwhile.
pub(crate) fn create(source: &Path, target: &Path, bytes: &[u8]) -> Result<(), String> {
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

/// The pixels an export of `area` of `image` makes: `shapes` drawn into it
/// as on screen, but no crop, the area cut out, turned by `rotation` and
/// scaled to exactly the size of `export`.
pub fn exported(
    image: &DecodedImage,
    rotation: Rotation,
    shapes: &[Shape],
    area: Region,
    export: &Export,
) -> Result<DecodedImage, String> {
    let shapes: Vec<Shape> = shapes
        .iter()
        .filter(|s| !matches!(s, Shape::Crop(_)))
        .cloned()
        .collect();
    let drawn = rgba_image(&render(image, &shapes)?)?;
    let [left, top, width, height] = area
        .pixels(drawn.width(), drawn.height())
        .ok_or("the area to export is empty")?;
    let cut = imageops::crop_imm(&drawn, left, top, width, height).to_image();
    let cut = DecodedImage::from_dynamic(DynamicImage::ImageRgba8(cut));
    let turned = rgba_image(&clipboard::turned(&cut, rotation)?)?;
    let scaled = imageops::resize(&turned, export.width, export.height, FilterType::Lanczos3);
    Ok(DecodedImage::from_dynamic(DynamicImage::ImageRgba8(scaled)))
}

fn rgba_image(image: &DecodedImage) -> Result<RgbaImage, String> {
    RgbaImage::from_raw(image.width, image.height, image.rgba.to_vec())
        .ok_or_else(|| "the pixels do not match the image size".to_owned())
}

/// `image` encoded as `export` asks, without metadata: within its size
/// limit by lowering the JPEG quality or raising the PNG compression; WebP is
/// written lossless only, without a limit.
pub fn encode_export(image: &DecodedImage, export: &Export) -> Result<Vec<u8>, String> {
    let rgba = rgba_image(image)?;
    let limit = export.limit_kb.unwrap_or(0);
    let fits = |bytes: &[u8]| {
        export.limit_kb.is_none_or(|kb| {
            u64::try_from(bytes.len()).is_ok_and(|len| len <= u64::from(kb).saturating_mul(1024))
        })
    };
    match export.format {
        ExportFormat::Jpeg => {
            let rgb = DynamicImage::ImageRgba8(rgba).to_rgb8();
            let jpeg = |quality: u8| -> Result<Vec<u8>, String> {
                let mut out = Vec::new();
                JpegEncoder::new_with_quality(&mut out, quality)
                    .encode_image(&rgb)
                    .map_err(|e| e.to_string())?;
                Ok(out)
            };
            let best = jpeg(JPEG_QUALITY)?;
            if fits(&best) {
                return Ok(best);
            }
            // The highest quality that fits, found by halving the range.
            let (mut low, mut high) = (MIN_JPEG_QUALITY, JPEG_QUALITY.saturating_sub(1));
            let mut found = None;
            while low <= high {
                let middle =
                    low.saturating_add(high.saturating_sub(low).checked_div(2).unwrap_or(0));
                let bytes = jpeg(middle)?;
                if fits(&bytes) {
                    found = Some(bytes);
                    low = middle.saturating_add(1);
                } else if middle == MIN_JPEG_QUALITY {
                    break;
                } else {
                    high = middle.saturating_sub(1);
                }
            }
            found.ok_or_else(|| {
                format!("it does not fit in {limit} KB even at the lowest JPEG quality")
            })
        }
        ExportFormat::Png => {
            for compression in [CompressionType::Default, CompressionType::Best] {
                let mut out = Vec::new();
                PngEncoder::new_with_quality(&mut out, compression, PngFilter::Adaptive)
                    .write_image(
                        rgba.as_raw(),
                        rgba.width(),
                        rgba.height(),
                        ExtendedColorType::Rgba8,
                    )
                    .map_err(|e| e.to_string())?;
                if fits(&out) {
                    return Ok(out);
                }
            }
            Err(format!(
                "it does not fit in {limit} KB even at the best PNG compression"
            ))
        }
        ExportFormat::WebP => {
            let mut out = Vec::new();
            WebPEncoder::new_lossless(&mut out)
                .write_image(
                    rgba.as_raw(),
                    rgba.width(),
                    rgba.height(),
                    ExtendedColorType::Rgba8,
                )
                .map_err(|e| e.to_string())?;
            Ok(out)
        }
    }
}

/// Writes the export of `area` of `image` of the file `source` as the new
/// file `target`, whose name must end in the extension of its format. The
/// error is meant to be shown to the user as is.
pub fn save_export(
    source: &Path,
    image: &DecodedImage,
    rotation: Rotation,
    shapes: &[Shape],
    area: Region,
    export: &Export,
    target: &Path,
) -> Result<(), String> {
    let write = || -> Result<(), String> {
        let format = match export.format {
            ExportFormat::Jpeg => ImageFormat::Jpeg,
            ExportFormat::Png => ImageFormat::Png,
            ExportFormat::WebP => ImageFormat::WebP,
        };
        if ImageFormat::from_path(target).ok() != Some(format) {
            return Err(format!(
                "the name must end in .{} for {}",
                export.format.extension(),
                export.format.name()
            ));
        }
        if target.exists() {
            return Err("the file already exists".to_owned());
        }
        let bytes = encode_export(&exported(image, rotation, shapes, area, export)?, export)?;
        create(source, target, &bytes)
    };
    write().map_err(|e| format!("Cannot save {}: {e}", folder::file_name(target)))
}
