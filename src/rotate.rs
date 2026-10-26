//! Writing a rotation into an image file, so it looks the same in every app:
//! JPEG with a lossless transform, other formats re-encoded. The EXIF
//! orientation the viewer applied is baked in too, as the rotation is relative
//! to what was on screen. The file is replaced through a temporary file in the
//! same folder, so a failure leaves the original untouched.

use std::fs::{self, File};
use std::io::{self, Cursor, Write};
use std::path::Path;
use std::sync::OnceLock;

use image::codecs::gif::GifDecoder;
use image::codecs::png::PngDecoder;
use image::codecs::webp::WebPDecoder;
use image::metadata::Orientation;
use image::{AnimationDecoder, DynamicImage, ImageDecoder, ImageFormat, ImageReader};

use crate::turbojpeg::{self, Transform, TurboJpeg};
use crate::{decode, folder, motion, multi};

/// A turn of the shown image in quarter steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Rotation {
    #[default]
    None,
    /// A quarter turn clockwise.
    Right,
    Half,
    /// A quarter turn counterclockwise.
    Left,
}

impl Rotation {
    /// A quarter turn further clockwise.
    pub fn right(self) -> Self {
        match self {
            Self::None => Self::Right,
            Self::Right => Self::Half,
            Self::Half => Self::Left,
            Self::Left => Self::None,
        }
    }

    /// A quarter turn further counterclockwise.
    pub fn left(self) -> Self {
        match self {
            Self::None => Self::Left,
            Self::Left => Self::Half,
            Self::Half => Self::Right,
            Self::Right => Self::None,
        }
    }

    /// Whether width and height trade places.
    pub fn sideways(self) -> bool {
        matches!(self, Self::Right | Self::Left)
    }

    fn quarter_turns(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Right => 1,
            Self::Half => 2,
            Self::Left => 3,
        }
    }
}

/// libturbojpeg, loaded on the first JPEG rotation.
static TURBOJPEG: OnceLock<Result<TurboJpeg, String>> = OnceLock::new();

/// Turns the image in the file at `path` by `rotation` from how the viewer
/// shows it. The error is meant to be shown to the user as is.
pub fn write(path: &Path, rotation: Rotation) -> Result<(), String> {
    rotated(path, rotation)
        .and_then(|bytes| replace(path, &bytes).map_err(|e| e.to_string()))
        .map_err(|e| format!("Cannot rotate {}: {e}", folder::file_name(path)))
}

/// The bytes of the turned file, in the file's own format.
fn rotated(path: &Path, rotation: Rotation) -> Result<Vec<u8>, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    if decode::is_heif(path, &bytes) {
        return Err("rotating HEIC/HEIF and AVIF is not supported".to_owned());
    }
    if multi::count(&bytes) > 1 {
        return Err(
            "rotating files of several images is not supported, the other images would be lost"
                .to_owned(),
        );
    }
    if matches!(motion::find(path), Ok(Some(_))) {
        return Err(
            "rotating Motion Photos is not supported, the embedded video would be lost".to_owned(),
        );
    }
    let format = image::guess_format(&bytes)
        .or_else(|_| ImageFormat::from_path(path))
        .map_err(|e| e.to_string())?;
    match format {
        ImageFormat::Jpeg => rotate_jpeg(&bytes, rotation),
        _ => reencode(&bytes, format, rotation),
    }
}

/// The JPEG turned losslessly, its EXIF orientation reset to upright.
fn rotate_jpeg(bytes: &[u8], rotation: Rotation) -> Result<Vec<u8>, String> {
    let library = TURBOJPEG
        .get_or_init(|| turbojpeg::load(turbojpeg::LIBRARY_NAMES))
        .as_ref()
        .map_err(Clone::clone)?;
    let orientation = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Jpeg)
        .into_decoder()
        .and_then(|mut decoder| decoder.orientation())
        .map_err(|e| e.to_string())?;
    let mut jpeg = library.transform(bytes, jpeg_transform(orientation, rotation))?;
    reset_orientation(&mut jpeg);
    Ok(jpeg)
}

/// The one lossless transform that turns the stored pixels into the shown
/// ones (`orientation`) and then by `rotation`.
pub fn jpeg_transform(orientation: Orientation, rotation: Rotation) -> Transform {
    // Every transform is an optional horizontal flip followed by clockwise
    // quarter turns; the rotation adds turns.
    let (flip, turns): (bool, u8) = match orientation {
        Orientation::NoTransforms => (false, 0),
        Orientation::Rotate90 => (false, 1),
        Orientation::Rotate180 => (false, 2),
        Orientation::Rotate270 => (false, 3),
        Orientation::FlipHorizontal => (true, 0),
        Orientation::Rotate270FlipH => (true, 1),
        Orientation::FlipVertical => (true, 2),
        Orientation::Rotate90FlipH => (true, 3),
    };
    match (flip, turns.wrapping_add(rotation.quarter_turns()) & 3) {
        (false, 0) => Transform::None,
        (false, 1) => Transform::Rotate90,
        (false, 2) => Transform::Rotate180,
        (false, _) => Transform::Rotate270,
        (true, 0) => Transform::FlipHorizontal,
        (true, 1) => Transform::Transverse,
        (true, 2) => Transform::FlipVertical,
        (true, _) => Transform::Transpose,
    }
}

/// Sets the EXIF orientation of `jpeg` to upright, if it has one; everything
/// else stays as it is.
pub fn reset_orientation(jpeg: &mut [u8]) {
    // A JPEG without EXIF orientation has nothing to reset.
    let _ = reset_jpeg_orientation(jpeg);
}

/// Walks the JPEG's header segments to the EXIF one.
fn reset_jpeg_orientation(jpeg: &mut [u8]) -> Option<()> {
    // After the start-of-image marker.
    let mut at: usize = 2;
    loop {
        let [0xFF, kind] = *jpeg.get(at..at.checked_add(2)?)? else {
            return None;
        };
        // Start of scan or end of image: no headers after it.
        if kind == 0xDA || kind == 0xD9 {
            return None;
        }
        let length = read_u16(jpeg, at.checked_add(2)?, false)?;
        let start = at.checked_add(4)?;
        let end = at.checked_add(2)?.checked_add(usize::from(length))?;
        let segment = jpeg.get_mut(start..end)?;
        if kind == 0xE1 && segment.starts_with(b"Exif\0\0") {
            return reset_tiff_orientation(segment.get_mut(6..)?);
        }
        at = end;
    }
}

/// Sets the orientation tag of the first IFD of TIFF-structured EXIF to 1.
fn reset_tiff_orientation(tiff: &mut [u8]) -> Option<()> {
    let little = match tiff.get(0..2)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let ifd = usize::try_from(read_u32(tiff, 4, little)?).ok()?;
    let count = read_u16(tiff, ifd, little)?;
    for index in 0..usize::from(count) {
        let entry = ifd.checked_add(2)?.checked_add(index.checked_mul(12)?)?;
        if read_u16(tiff, entry, little)? == 0x0112 {
            let value = entry.checked_add(8)?;
            let upright = if little {
                1u16.to_le_bytes()
            } else {
                1u16.to_be_bytes()
            };
            tiff.get_mut(value..value.checked_add(2)?)?
                .copy_from_slice(&upright);
            return Some(());
        }
    }
    None
}

fn read_u16(bytes: &[u8], at: usize, little: bool) -> Option<u16> {
    let raw: [u8; 2] = bytes.get(at..at.checked_add(2)?)?.try_into().ok()?;
    Some(if little {
        u16::from_le_bytes(raw)
    } else {
        u16::from_be_bytes(raw)
    })
}

fn read_u32(bytes: &[u8], at: usize, little: bool) -> Option<u32> {
    let raw: [u8; 4] = bytes.get(at..at.checked_add(4)?)?.try_into().ok()?;
    Some(if little {
        u32::from_le_bytes(raw)
    } else {
        u32::from_be_bytes(raw)
    })
}

/// The image decoded upright, turned and encoded again in `format`. Metadata
/// is not carried over, so no orientation tag turns it a second time.
fn reencode(bytes: &[u8], format: ImageFormat, rotation: Rotation) -> Result<Vec<u8>, String> {
    if animated(bytes, format).map_err(|e| e.to_string())? {
        return Err("rotating animated images is not supported".to_owned());
    }
    let mut decoder = ImageReader::with_format(Cursor::new(bytes), format)
        .into_decoder()
        .map_err(|e| e.to_string())?;
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    image.apply_orientation(orientation);
    let image = match rotation {
        Rotation::None => image,
        Rotation::Right => image.rotate90(),
        Rotation::Half => image.rotate180(),
        Rotation::Left => image.rotate270(),
    };
    let mut out = Cursor::new(Vec::new());
    image
        .write_to(&mut out, format)
        .map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

/// Whether the file has more than one frame; re-encoding would keep only the
/// first.
fn animated(bytes: &[u8], format: ImageFormat) -> image::ImageResult<bool> {
    Ok(match format {
        ImageFormat::Gif => {
            GifDecoder::new(Cursor::new(bytes))?
                .into_frames()
                .take(2)
                .count()
                > 1
        }
        ImageFormat::WebP => WebPDecoder::new(Cursor::new(bytes))?.has_animation(),
        ImageFormat::Png => PngDecoder::new(Cursor::new(bytes))?.is_apng()?,
        _ => false,
    })
}

/// Replaces the file at `path` with `bytes` through a temporary file next to
/// it, a dot file no listing shows, with the original's permissions.
fn replace(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temp = path.with_file_name(format!(".{}.pix3-tmp", folder::file_name(path)));
    let result = write_temp(&temp, path, bytes).and_then(|()| fs::rename(&temp, path));
    if result.is_err() {
        // The temporary file may not exist; the first error is what counts.
        let _ = fs::remove_file(&temp);
    }
    result
}

fn write_temp(temp: &Path, original: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = File::create(temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    file.set_permissions(fs::metadata(original)?.permissions())
}
