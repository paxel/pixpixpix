//! Turning a file into pixels. Orientation from EXIF is applied, so every view
//! shows the image upright.

use std::fs::File;
use std::io::{Cursor, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use image::codecs::gif::GifDecoder;
use image::codecs::webp::WebPDecoder;
use image::{AnimationDecoder, DynamicImage, ImageDecoder, ImageFormat, ImageReader};

use crate::heif::{self, Heif};
use crate::{folder, multi};

/// Decoded RGBA pixels, shared cheaply between the core and the UI.
#[derive(Clone, PartialEq, Eq)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    /// Unpremultiplied RGBA, row by row.
    pub rgba: Arc<[u8]>,
}

impl std::fmt::Debug for DecodedImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DecodedImage({}x{})", self.width, self.height)
    }
}

impl DecodedImage {
    pub fn from_dynamic(image: DynamicImage) -> Self {
        let rgba = image.into_rgba8();
        Self {
            width: rgba.width(),
            height: rgba.height(),
            rgba: Arc::from(rgba.into_raw()),
        }
    }
}

/// The first frame of a file and whether more follow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirstFrame {
    pub image: DecodedImage,
    /// An animated GIF or WebP.
    pub animated: bool,
    /// How many images the file holds: more than one for a multi-page TIFF,
    /// an MPO or a multi-image HEIC/HEIF.
    pub images: usize,
}

/// One picture of an animation and how long it shows, as the file says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub image: DecodedImage,
    pub delay: Duration,
}

/// How much is read between two looks at the cancellation token.
const CHUNK: u64 = 256 * 1024;

/// The most pixel bytes all frames of an animation may take together; a
/// larger one stays at its first frame.
const MAX_ANIMATION_BYTES: usize = 512 * 1024 * 1024;

/// Decodes the file at `path`, or gives up with `None` once `cancel` is set:
/// the file is read in chunks with a look at the token between them, then
/// decoded from memory. The error is the decoder's message, meant to be shown
/// to the user as is. An animation gives its first frame.
pub fn decode_file(path: &Path, cancel: &AtomicBool) -> Option<Result<DecodedImage, String>> {
    decode_first_frame(path, cancel).map(|result| result.map(|first| first.image))
}

/// Like [`decode_file`], telling an animation and a multi-image file apart;
/// finding out reads only the file's headers.
pub fn decode_first_frame(path: &Path, cancel: &AtomicBool) -> Option<Result<FirstFrame, String>> {
    let cancelled = || cancel.load(Ordering::Relaxed);
    let bytes = match read(path, &cancelled) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return None,
        Err(e) => return Some(Err(e.to_string())),
    };
    if cancelled() {
        return None;
    }
    let animated = is_animated(&bytes);
    let result = decode_bytes(path, bytes, 0).map(|(image, images)| FirstFrame {
        image,
        animated,
        images,
    });
    if cancelled() {
        return None;
    }
    Some(result)
}

/// Like [`decode_file`], for image `index` of a multi-image file, counted
/// from 0 for the one [`decode_file`] gives.
pub fn decode_inner(
    path: &Path,
    index: usize,
    cancel: &AtomicBool,
) -> Option<Result<DecodedImage, String>> {
    let cancelled = || cancel.load(Ordering::Relaxed);
    let bytes = match read(path, &cancelled) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return None,
        Err(e) => return Some(Err(e.to_string())),
    };
    if cancelled() {
        return None;
    }
    let result = decode_bytes(path, bytes, index).map(|(image, _)| image);
    if cancelled() {
        return None;
    }
    Some(result)
}

/// Decodes every frame of the animated GIF or WebP at `path`, each the size
/// of the first and turned like it, or gives up with `None` once `cancel` is
/// set; the token is looked at between frames too.
pub fn decode_frames(path: &Path, cancel: &AtomicBool) -> Option<Result<Vec<Frame>, String>> {
    let cancelled = || cancel.load(Ordering::Relaxed);
    let bytes = match read(path, &cancelled) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return None,
        Err(e) => return Some(Err(e.to_string())),
    };
    let cursor = Cursor::new(bytes);
    match image::guess_format(cursor.get_ref()) {
        Ok(ImageFormat::Gif) => match GifDecoder::new(cursor) {
            Ok(decoder) => frames(decoder, &cancelled),
            Err(e) => Some(Err(e.to_string())),
        },
        Ok(ImageFormat::WebP) => match WebPDecoder::new(cursor) {
            Ok(decoder) => frames(decoder, &cancelled),
            Err(e) => Some(Err(e.to_string())),
        },
        _ => Some(Err("not an animated GIF or WebP".to_owned())),
    }
}

/// The frames of `decoder`, upright, or `None` once cancelled.
fn frames<'a>(
    mut decoder: impl ImageDecoder + AnimationDecoder<'a>,
    cancelled: &impl Fn() -> bool,
) -> Option<Result<Vec<Frame>, String>> {
    let orientation = match decoder.orientation() {
        Ok(orientation) => orientation,
        Err(e) => return Some(Err(e.to_string())),
    };
    let mut frames = Vec::new();
    let mut bytes = 0usize;
    for frame in decoder.into_frames() {
        if cancelled() {
            return None;
        }
        let frame = match frame {
            Ok(frame) => frame,
            Err(e) => return Some(Err(e.to_string())),
        };
        let delay = Duration::from(frame.delay());
        let mut image = DynamicImage::ImageRgba8(frame.into_buffer());
        image.apply_orientation(orientation);
        let image = DecodedImage::from_dynamic(image);
        bytes = bytes.saturating_add(image.rgba.len());
        if bytes > MAX_ANIMATION_BYTES {
            return Some(Err("too large to animate".to_owned()));
        }
        frames.push(Frame { image, delay });
    }
    Some(Ok(frames))
}

/// Whether `bytes` are an animated GIF or WebP.
fn is_animated(bytes: &[u8]) -> bool {
    match image::guess_format(bytes) {
        Ok(ImageFormat::Gif) => gif_images(bytes) > 1,
        Ok(ImageFormat::WebP) => {
            WebPDecoder::new(Cursor::new(bytes)).is_ok_and(|d| d.has_animation())
        }
        _ => false,
    }
}

/// How many images the GIF in `bytes` holds, counted up to two by walking
/// its blocks without decoding any; a damaged file counts what was found.
fn gif_images(bytes: &[u8]) -> usize {
    let mut images = 0usize;
    // The header and screen descriptor, then the global colour table.
    let Some(mut at) = bytes.get(10).and_then(|flags| skip_table(*flags, 13)) else {
        return images;
    };
    while images < 2 {
        let next = match bytes.get(at) {
            // An extension: label, then data sub-blocks.
            Some(0x21) => at.checked_add(2).and_then(|at| skip_blocks(bytes, at)),
            // An image: descriptor, local colour table, code size, data.
            Some(0x2C) => {
                images = images.saturating_add(1);
                at.checked_add(9)
                    .and_then(|flags| bytes.get(flags))
                    .and_then(|flags| skip_table(*flags, at.checked_add(10)?))
                    .and_then(|at| skip_blocks(bytes, at.checked_add(1)?))
            }
            _ => None,
        };
        match next {
            Some(next) => at = next,
            None => break,
        }
    }
    images
}

/// Skips the colour table at `at` that a GIF descriptor's `flags` announce.
fn skip_table(flags: u8, at: usize) -> Option<usize> {
    if flags & 0x80 == 0 {
        return Some(at);
    }
    let size = 3usize.checked_shl(u32::from(flags & 0x07).checked_add(1)?)?;
    at.checked_add(size)
}

/// Skips the GIF data sub-blocks starting at `at`, up to their terminator.
fn skip_blocks(bytes: &[u8], mut at: usize) -> Option<usize> {
    loop {
        let len = usize::from(*bytes.get(at)?);
        at = at.checked_add(1)?;
        if len == 0 {
            return Some(at);
        }
        at = at.checked_add(len)?;
    }
}

/// The whole file, or `None` when cancelled while reading.
fn read(path: &Path, cancelled: &impl Fn() -> bool) -> std::io::Result<Option<Vec<u8>>> {
    let mut file = File::open(path)?;
    let mut bytes = Vec::new();
    loop {
        if cancelled() {
            return Ok(None);
        }
        if file.by_ref().take(CHUNK).read_to_end(&mut bytes)? == 0 {
            return Ok(Some(bytes));
        }
    }
}

/// libheif, loaded on the first HEIC/HEIF or AVIF file.
static HEIF: OnceLock<Result<Heif, String>> = OnceLock::new();

/// True when the file at `path` with content `bytes` is HEIF or AVIF: the
/// content decides, the extension is the fallback.
pub fn is_heif(path: &Path, bytes: &[u8]) -> bool {
    heif::is_heif(bytes) || (folder::is_heif(path) && image::guess_format(bytes).is_err())
}

/// Decodes image `index` of the bytes of the file at `path` and counts the
/// file's images; the file content decides the format, its extension is the
/// fallback.
fn decode_bytes(
    path: &Path,
    bytes: Vec<u8>,
    index: usize,
) -> Result<(DecodedImage, usize), String> {
    if is_heif(path, &bytes) {
        return HEIF
            .get_or_init(|| heif::load(heif::LIBRARY_NAMES))
            .as_ref()
            .map_err(Clone::clone)?
            .decode(&bytes, index);
    }
    let (bytes, images) = multi::picture(bytes, index)?;
    let mut reader = ImageReader::new(Cursor::new(bytes));
    if let Ok(format) = ImageFormat::from_path(path) {
        reader.set_format(format);
    }
    let mut decoder = reader
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .into_decoder()
        .map_err(|e| e.to_string())?;
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    image.apply_orientation(orientation);
    Ok((DecodedImage::from_dynamic(image), images))
}
