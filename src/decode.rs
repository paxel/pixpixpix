//! Turning a file into pixels. Orientation from EXIF is applied, so every view
//! shows the image upright.

use std::path::Path;
use std::sync::Arc;

use image::{DynamicImage, ImageDecoder, ImageReader};

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

/// Decodes the file at `path`; the error is the decoder's message, meant to be
/// shown to the user as is.
pub fn decode_file(path: &Path) -> Result<DecodedImage, String> {
    let mut decoder = ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .into_decoder()
        .map_err(|e| e.to_string())?;
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    image.apply_orientation(orientation);
    Ok(DecodedImage::from_dynamic(image))
}
