//! Drawing tool edits into the pixels and writing the edited image: over the
//! file itself through a temporary file, as `rotate` does, or as a new file
//! that never replaces an existing one. The pixels are turned by the pending
//! rotation after the shapes are drawn, so the file looks like the screen.

use std::fs;
use std::io::{Cursor, Write};
use std::path::Path;

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageFormat, RgbaImage};
use tiny_skia::{ColorU8, FillRule, Paint, PathBuilder, Pixmap, Transform};

use crate::core::Shape;
use crate::decode::DecodedImage;
use crate::rotate::{self, Rotation};
use crate::{clipboard, decode, folder, motion, multi};

/// The JPEG quality edited images are written with.
const JPEG_QUALITY: u8 = 92;

/// `image` with `shapes` drawn into it, antialiased.
pub fn render(image: &DecodedImage, shapes: &[Shape]) -> Result<DecodedImage, String> {
    if shapes.is_empty() {
        return Ok(image.clone());
    }
    let mut pixmap =
        Pixmap::new(image.width, image.height).ok_or("the image has no pixels to draw on")?;
    if pixmap.pixels().len().saturating_mul(4) != image.rgba.len() {
        return Err("the pixels do not match the image size".to_owned());
    }
    let (rgba, _) = image.rgba.as_chunks::<4>();
    for (pixel, &[r, g, b, a]) in pixmap.pixels_mut().iter_mut().zip(rgba) {
        *pixel = ColorU8::from_rgba(r, g, b, a).premultiply();
    }
    for shape in shapes {
        let Some(outline) = shape.outline(image.width, image.height) else {
            continue;
        };
        let [r, g, b] = outline.color.rgb();
        let mut paint = Paint::default();
        paint.set_color_rgba8(r, g, b, 0xff);
        paint.anti_alias = true;
        for polygon in &outline.polygons {
            let mut builder = PathBuilder::new();
            let mut points = polygon.iter();
            let Some(first) = points.next() else {
                continue;
            };
            builder.move_to(first.x, first.y);
            for point in points {
                builder.line_to(point.x, point.y);
            }
            builder.close();
            if let Some(path) = builder.finish() {
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
    let rgba: Vec<u8> = pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let color = pixel.demultiply();
            [color.red(), color.green(), color.blue(), color.alpha()]
        })
        .collect();
    Ok(DecodedImage {
        width: image.width,
        height: image.height,
        rgba: rgba.into(),
    })
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
