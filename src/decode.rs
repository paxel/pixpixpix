//! Turning a file into pixels. Orientation from EXIF is applied, so every view
//! shows the image upright.

use std::fs::File;
use std::io::{Cursor, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};

use crate::folder;
use crate::heif::{self, Heif};

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

/// How much is read between two looks at the cancellation token.
const CHUNK: u64 = 256 * 1024;

/// Decodes the file at `path`, or gives up with `None` once `cancel` is set:
/// the file is read in chunks with a look at the token between them, then
/// decoded from memory. The error is the decoder's message, meant to be shown
/// to the user as is.
pub fn decode_file(path: &Path, cancel: &AtomicBool) -> Option<Result<DecodedImage, String>> {
    let cancelled = || cancel.load(Ordering::Relaxed);
    let bytes = match read(path, &cancelled) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return None,
        Err(e) => return Some(Err(e.to_string())),
    };
    if cancelled() {
        return None;
    }
    let result = decode_bytes(path, bytes);
    if cancelled() {
        return None;
    }
    Some(result)
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

/// Decodes the bytes of the file at `path`; the file content decides the
/// format, its extension is the fallback.
fn decode_bytes(path: &Path, bytes: Vec<u8>) -> Result<DecodedImage, String> {
    if is_heif(path, &bytes) {
        return HEIF
            .get_or_init(|| heif::load(heif::LIBRARY_NAMES))
            .as_ref()
            .map_err(Clone::clone)?
            .decode(&bytes);
    }
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
    Ok(DecodedImage::from_dynamic(image))
}
