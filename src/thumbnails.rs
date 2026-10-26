//! The thumbnail cache of the freedesktop Thumbnail Managing Standard: the
//! overview reads thumbnails other programs made and writes its own, so a
//! second visit decodes nothing. macOS has no shared cache; there an own
//! directory with the same layout is used.

use std::fs::{DirBuilder, File};
use std::io::{BufReader, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::UNIX_EPOCH;

use image::DynamicImage;

use crate::decode::{self, DecodedImage};

/// The size dirs of the standard with the longest side their thumbnails
/// fit in, smallest first.
const SIZES: [(&str, u32); 4] = [
    ("normal", 128),
    ("large", 256),
    ("x-large", 512),
    ("xx-large", 1024),
];

const URI_KEY: &str = "Thumb::URI";
const MTIME_KEY: &str = "Thumb::MTime";
const SIZE_KEY: &str = "Thumb::Size";

/// Where the cache lives: `$XDG_CACHE_HOME/thumbnails` (`~/.cache/thumbnails`
/// without it), on macOS `~/Library/Caches/pixpixpix/thumbnails`.
pub fn default_root() -> Option<PathBuf> {
    let cache = directories::BaseDirs::new()?.cache_dir().to_path_buf();
    if cfg!(target_os = "macos") {
        Some(cache.join("pixpixpix").join("thumbnails"))
    } else {
        Some(cache.join("thumbnails"))
    }
}

/// The `file://` URI of `path` made absolute, its bytes escaped as GLib
/// does, so the hash matches the thumbnails of GNOME programs.
pub fn uri(path: &Path) -> Option<String> {
    let path = std::path::absolute(path).ok()?;
    let mut uri = String::from("file://");
    for &byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~!$&'()*+,=:@".contains(&byte) {
            uri.push(char::from(byte));
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    Some(uri)
}

/// The size dir in `root` for thumbnails of at most `max` pixels a side, and
/// the size its thumbnails are made in: the smallest that holds `max`.
fn size_dir(root: &Path, max: u32) -> (PathBuf, u32) {
    let (name, size) = SIZES
        .iter()
        .find(|(_, size)| *size >= max)
        .unwrap_or(&("xx-large", 1024));
    (root.join(name), *size)
}

/// The cache file in `root` for the thumbnail of `path` of at most `max`
/// pixels a side: the MD5 of its URI in hex, in the matching size dir.
pub fn cache_path(root: &Path, path: &Path, max: u32) -> Option<PathBuf> {
    let (dir, _) = size_dir(root, max);
    Some(dir.join(format!("{:x}.png", md5::compute(uri(path)?))))
}

/// The thumbnail of `path` of at most `max` pixels a side, or `None` once
/// `cancel` is set. A valid cached one in `root` is used; otherwise the file
/// is decoded in the size of its size dir and the result cached. Without a
/// root, or when the file cannot be stated, the file is just decoded.
pub fn thumbnail(
    root: Option<&Path>,
    path: &Path,
    max: u32,
    cancel: &AtomicBool,
) -> Option<Result<DecodedImage, String>> {
    let cached = root.and_then(|root| Some((root, uri(path)?, stamp(path)?)));
    let Some((root, uri, stamp)) = cached else {
        return decode::decode_thumbnail(path, max, cancel);
    };
    let (dir, size) = size_dir(root, max);
    let file = dir.join(format!("{:x}.png", md5::compute(&uri)));
    if let Some(image) = read(&file, &uri, stamp) {
        return Some(Ok(decode::shrink(image, max)));
    }
    let image = match decode::decode_thumbnail(path, size, cancel)? {
        Ok(image) => image,
        Err(e) => return Some(Err(e)),
    };
    // Files in the cache are shown as they are, never thumbnailed.
    if !path.starts_with(root) {
        // A cache that cannot be written only costs the next visit a decode.
        let _ = write(&dir, &file, &image, &uri, stamp);
    }
    Some(Ok(decode::shrink(image, max)))
}

/// What a thumbnail is checked against: the file's modification time in
/// whole seconds and its size in bytes.
#[derive(Clone, Copy)]
struct Stamp {
    mtime: u64,
    size: u64,
}

/// The stamp of the file at `path`; `None` when it cannot be stated or was
/// modified before 1970.
fn stamp(path: &Path) -> Option<Stamp> {
    let metadata = std::fs::metadata(path).ok()?;
    let mtime = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    Some(Stamp {
        mtime: mtime.as_secs(),
        size: metadata.len(),
    })
}

/// The thumbnail in `file` if it is of `uri` with `stamp`: URI and MTime
/// must match, the size only if it is given.
fn read(file: &Path, uri: &str, stamp: Stamp) -> Option<DecodedImage> {
    let mut decoder = png::Decoder::new(BufReader::new(File::open(file).ok()?));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut pixels = vec![0; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut pixels).ok()?;
    // Text may follow the pixels; it is known only once all is read.
    reader.finish().ok()?;
    let text = |key: &str| {
        reader
            .info()
            .uncompressed_latin1_text
            .iter()
            .find(|chunk| chunk.keyword == key)
            .map(|chunk| chunk.text.as_str())
    };
    let valid = text(URI_KEY) == Some(uri)
        && text(MTIME_KEY).and_then(|t| t.parse().ok()) == Some(stamp.mtime)
        && text(SIZE_KEY).is_none_or(|t| t.parse().ok() == Some(stamp.size));
    if !valid {
        return None;
    }
    pixels.truncate(frame.buffer_size());
    let (width, height) = (frame.width, frame.height);
    let image = match frame.color_type {
        png::ColorType::Grayscale => {
            DynamicImage::ImageLuma8(image::GrayImage::from_raw(width, height, pixels)?)
        }
        png::ColorType::GrayscaleAlpha => {
            DynamicImage::ImageLumaA8(image::GrayAlphaImage::from_raw(width, height, pixels)?)
        }
        png::ColorType::Rgb => {
            DynamicImage::ImageRgb8(image::RgbImage::from_raw(width, height, pixels)?)
        }
        png::ColorType::Rgba => {
            DynamicImage::ImageRgba8(image::RgbaImage::from_raw(width, height, pixels)?)
        }
        png::ColorType::Indexed => return None,
    };
    Some(DecodedImage::from_dynamic(image))
}

/// Writes `image` as the thumbnail `file` in `dir` for `uri` with `stamp`:
/// into a temporary file next to it first, renamed into place when complete,
/// so readers never see half a thumbnail. Dirs are made private to the user,
/// as the thumbnail file is.
fn write(
    dir: &Path,
    file: &Path,
    image: &DecodedImage,
    uri: &str,
    stamp: Stamp,
) -> Result<(), String> {
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(|e| e.to_string())?;
    let mut temporary = tempfile::Builder::new()
        .prefix("pixpixpix-")
        .suffix(".png")
        .tempfile_in(dir)
        .map_err(|e| e.to_string())?;
    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    for (key, value) in [
        (URI_KEY, uri.to_owned()),
        (MTIME_KEY, stamp.mtime.to_string()),
        (SIZE_KEY, stamp.size.to_string()),
    ] {
        encoder
            .add_text_chunk(key.to_owned(), value)
            .map_err(|e| e.to_string())?;
    }
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer
        .write_image_data(&image.rgba)
        .map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    temporary.write_all(&png).map_err(|e| e.to_string())?;
    temporary.persist(file).map_err(|e| e.to_string())?;
    Ok(())
}
