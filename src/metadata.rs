//! The metadata of a file for the metadata view, writing its capture date,
//! and removing its location or all its EXIF. The date is patched where it
//! stands in the EXIF block, at its fixed length, so every offset stays valid
//! and no pixel is touched. The file is replaced through a temporary file, as
//! a rotation is.

use std::fs::{self, File};
use std::io::{BufReader, Cursor};
use std::ops::Range;
use std::path::Path;
use std::time::SystemTime;

use chrono::{DateTime, Local, NaiveDateTime};
use exif::{Context, Exif, In, Reader, Tag, Value};
use image::metadata::Orientation;
use image::{ImageDecoder, ImageFormat, ImageReader};

use crate::{annotate, decode, folder, motion, multi, rotate};

/// One read-only line of the metadata view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub label: &'static str,
    pub value: String,
}

/// What the metadata view shows of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    /// Name, size and modification time of the file.
    pub file: Vec<Field>,
    /// The capture date as `YYYY-MM-DD HH:MM:SS`, if the file has one.
    pub captured: Option<String>,
    /// Camera, exposure, place and the like; empty without EXIF.
    pub exif: Vec<Field>,
    /// The file has EXIF, shown or not.
    pub has_exif: bool,
    /// The EXIF has GPS tags, shown or not.
    pub has_location: bool,
}

/// How a capture date is typed.
const TYPED: &str = "%Y-%m-%d %H:%M:%S";
/// How EXIF stores a date.
const STORED: &str = "%Y:%m:%d %H:%M:%S";

/// EXIF tags of the dates and where the capture date lives.
const DATE_TIME: u16 = 0x0132;
const EXIF_IFD: u16 = 0x8769;
const DATE_TIME_ORIGINAL: u16 = 0x9003;
const DATE_TIME_DIGITIZED: u16 = 0x9004;
/// The EXIF type of text.
const ASCII: u16 = 2;
/// A stored date with its closing NUL.
const DATE_LENGTH: usize = 20;

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Reads the metadata of the file at `path`. A file without EXIF, or of a
/// format that carries none, has only its file fields. The error is meant to
/// be shown to the user as is.
pub fn read(path: &Path) -> Result<Metadata, String> {
    let cannot = |e: std::io::Error| format!("Cannot read {}: {e}", folder::file_name(path));
    let info = fs::metadata(path).map_err(cannot)?;
    let mut file = vec![
        Field {
            label: "File",
            value: folder::file_name(path),
        },
        Field {
            label: "Size",
            value: size(info.len()),
        },
    ];
    if let Ok(modified) = info.modified() {
        file.push(Field {
            label: "Modified",
            value: local(modified),
        });
    }
    // Videos are no EXIF containers; nothing else is read of them.
    let exif = if folder::is_video(path) {
        None
    } else {
        let mut reader = BufReader::new(File::open(path).map_err(cannot)?);
        Reader::new().read_from_container(&mut reader).ok()
    };
    Ok(Metadata {
        file,
        captured: exif.as_ref().and_then(captured),
        exif: exif.as_ref().map(fields).unwrap_or_default(),
        has_exif: exif.is_some(),
        has_location: exif
            .as_ref()
            .is_some_and(|e| e.fields().any(|f| f.tag.context() == Context::Gps)),
    })
}

/// `bytes` for people: bytes, kB, MB or GB.
fn size(bytes: u64) -> String {
    let value = bytes as f64;
    if bytes < 1000 {
        format!("{bytes} bytes")
    } else if value < 1e6 {
        format!("{:.1} kB", value / 1e3)
    } else if value < 1e9 {
        format!("{:.1} MB", value / 1e6)
    } else {
        format!("{:.1} GB", value / 1e9)
    }
}

fn local(time: SystemTime) -> String {
    DateTime::<Local>::from(time).format(TYPED).to_string()
}

/// The capture date as typed; a date that is no date shows as stored.
fn captured(exif: &Exif) -> Option<String> {
    let stored = text(exif, Tag::DateTimeOriginal)?;
    Some(match NaiveDateTime::parse_from_str(&stored, STORED) {
        Ok(date) => date.format(TYPED).to_string(),
        Err(_) => stored,
    })
}

/// The read-only EXIF fields worth showing, those the file has.
fn fields(exif: &Exif) -> Vec<Field> {
    let camera = match (text(exif, Tag::Make), text(exif, Tag::Model)) {
        (Some(make), Some(model)) if model.starts_with(&make) => Some(model),
        (Some(make), Some(model)) => Some(format!("{make} {model}")),
        (make, model) => make.or(model),
    };
    let exposure = [Tag::ExposureTime, Tag::FNumber]
        .into_iter()
        .filter_map(|tag| shown(exif, tag))
        .collect::<Vec<_>>()
        .join(" at ");
    [
        ("Camera", camera),
        ("Lens", text(exif, Tag::LensModel)),
        ("Time zone", text(exif, Tag::OffsetTimeOriginal)),
        ("Exposure", (!exposure.is_empty()).then_some(exposure)),
        ("ISO", shown(exif, Tag::PhotographicSensitivity)),
        ("Focal length", shown(exif, Tag::FocalLength)),
        ("GPS", position(exif)),
        ("Orientation", orientation(exif)),
    ]
    .into_iter()
    .filter_map(|(label, value)| {
        Some(Field {
            label,
            value: value?,
        })
    })
    .collect()
}

/// A text field without its closing NULs and blanks.
fn text(exif: &Exif, tag: Tag) -> Option<String> {
    let field = exif.get_field(tag, In::PRIMARY)?;
    let Value::Ascii(parts) = &field.value else {
        return None;
    };
    let text = String::from_utf8_lossy(parts.first()?);
    let text = text.trim_matches(|c: char| c == '\0' || c.is_whitespace());
    (!text.is_empty()).then(|| text.to_owned())
}

/// A field as kamadak-exif shows it, with its unit.
fn shown(exif: &Exif, tag: Tag) -> Option<String> {
    let field = exif.get_field(tag, In::PRIMARY)?;
    Some(field.display_value().with_unit(exif).to_string())
}

/// Latitude and longitude in degrees, south and west negative.
fn position(exif: &Exif) -> Option<String> {
    let degrees = |tag: Tag, negative: &str| -> Option<f64> {
        let Value::Rational(parts) = &exif.get_field(tag, In::PRIMARY)?.value else {
            return None;
        };
        let part = |i: usize| parts.get(i).map_or(0.0, |r| r.to_f64());
        let value = part(0) + part(1) / 60.0 + part(2) / 3600.0;
        let reference = match tag {
            Tag::GPSLatitude => Tag::GPSLatitudeRef,
            _ => Tag::GPSLongitudeRef,
        };
        let negated = text(exif, reference).is_some_and(|r| r == negative);
        Some(if negated { -value } else { value })
    };
    let latitude = degrees(Tag::GPSLatitude, "S")?;
    let longitude = degrees(Tag::GPSLongitude, "W")?;
    Some(format!("{latitude:.5}, {longitude:.5}"))
}

fn orientation(exif: &Exif) -> Option<String> {
    let value = exif
        .get_field(Tag::Orientation, In::PRIMARY)?
        .value
        .get_uint(0)?;
    let meaning = match value {
        1 => "upright",
        2 => "mirrored",
        3 => "turned 180°",
        4 => "mirrored upside down",
        5 => "mirrored and turned 90° clockwise",
        6 => "turned 90° clockwise",
        7 => "mirrored and turned 90° counterclockwise",
        8 => "turned 90° counterclockwise",
        _ => return Some(value.to_string()),
    };
    Some(format!("{value} ({meaning})"))
}

/// A typed capture date `YYYY-MM-DD HH:MM:SS` as EXIF stores it, or none if
/// it is not such a date.
pub fn stored_date(typed: &str) -> Option<String> {
    let date = NaiveDateTime::parse_from_str(typed, TYPED).ok()?;
    // Exactly as typed, so the stored date keeps its fixed length.
    (date.format(TYPED).to_string() == typed).then(|| date.format(STORED).to_string())
}

/// Sets the capture date of the file at `path` to `stored`, as
/// [`stored_date`] gives it: DateTimeOriginal, and DateTimeDigitized and
/// DateTime where the file has them. Nothing else of the file changes. The
/// error is meant to be shown to the user as is.
pub fn write_date(path: &Path, stored: &str) -> Result<(), String> {
    dated(path, stored)
        .and_then(|bytes| rotate::replace(path, &bytes).map_err(|e| e.to_string()))
        .map_err(|e| format!("Cannot change the date of {}: {e}", folder::file_name(path)))
}

/// The bytes of the file with its dates patched.
fn dated(path: &Path, stored: &str) -> Result<Vec<u8>, String> {
    let mut value = stored.as_bytes().to_vec();
    value.push(0);
    if value.len() != DATE_LENGTH {
        return Err(format!("{stored} is no date"));
    }
    let mut bytes = fs::read(path).map_err(|e| e.to_string())?;
    let exif = Reader::new()
        .read_from_container(&mut Cursor::new(&bytes))
        .map_err(|e| format!("no EXIF to change: {e}"))?;
    let block = exif.buf();
    if block.is_empty() {
        return Err(BROKEN.to_owned());
    }
    // The block is a copy of a piece of the file; it is patched where it is.
    let start = bytes
        .windows(block.len())
        .position(|w| w == block)
        .ok_or("its EXIF is not stored in one piece")?;
    let end = start.checked_add(block.len()).ok_or(BROKEN)?;
    patch_dates(bytes.get_mut(start..end).ok_or(BROKEN)?, &value)?;
    if bytes.starts_with(PNG_SIGNATURE) {
        fix_png_crc(&mut bytes, start, end).ok_or(BROKEN)?;
    }
    Ok(bytes)
}

const BROKEN: &str = "its EXIF is broken";

/// Overwrites the dates in the TIFF-structured EXIF block `tiff` with
/// `value`; the capture date must be there.
fn patch_dates(tiff: &mut [u8], value: &[u8]) -> Result<(), String> {
    let little = match tiff.get(0..2) {
        Some(b"II") => true,
        Some(b"MM") => false,
        _ => return Err(BROKEN.to_owned()),
    };
    let ifd0 = read_u32(tiff, 4, little).ok_or(BROKEN)?;
    let exif_ifd =
        entry(tiff, ifd0, EXIF_IFD, little).and_then(|e| read_u32(tiff, e.checked_add(8)?, little));
    let original = exif_ifd.and_then(|ifd| entry(tiff, ifd, DATE_TIME_ORIGINAL, little));
    let Some(original) = original else {
        return Err("it has no capture date".to_owned());
    };
    let digitized = exif_ifd.and_then(|ifd| entry(tiff, ifd, DATE_TIME_DIGITIZED, little));
    let changed = entry(tiff, ifd0, DATE_TIME, little);
    for at in [Some(original), digitized, changed].into_iter().flatten() {
        patch(tiff, at, value, little).ok_or("its dates are not stored as usual")?;
    }
    Ok(())
}

/// Where the entry of `tag` starts in the IFD at `ifd`, if it is there.
fn entry(tiff: &[u8], ifd: usize, tag: u16, little: bool) -> Option<usize> {
    let count = read_u16(tiff, ifd, little)?;
    (0..usize::from(count)).find_map(|index| {
        let at = ifd.checked_add(2)?.checked_add(index.checked_mul(12)?)?;
        (read_u16(tiff, at, little)? == tag).then_some(at)
    })
}

/// Overwrites the value of the date entry at `at` with `value`, if it is
/// text of exactly that length.
fn patch(tiff: &mut [u8], at: usize, value: &[u8], little: bool) -> Option<()> {
    let kind = read_u16(tiff, at.checked_add(2)?, little)?;
    let count = read_u32(tiff, at.checked_add(4)?, little)?;
    if kind != ASCII || count != DATE_LENGTH {
        return None;
    }
    let offset = read_u32(tiff, at.checked_add(8)?, little)?;
    tiff.get_mut(offset..offset.checked_add(DATE_LENGTH)?)?
        .copy_from_slice(value);
    Some(())
}

/// Recomputes the CRC of the PNG `eXIf` chunk whose data is
/// `bytes[start..end]`.
fn fix_png_crc(bytes: &mut [u8], start: usize, end: usize) -> Option<()> {
    let kind = start.checked_sub(4)?;
    if bytes.get(kind..start)? != b"eXIf" {
        return None;
    }
    let crc = crc32fast::hash(bytes.get(kind..end)?);
    bytes
        .get_mut(end..end.checked_add(4)?)?
        .copy_from_slice(&crc.to_be_bytes());
    Some(())
}

/// What a removal takes out of a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strip {
    /// The GPS tags; everything else stays.
    Location,
    /// All EXIF, the orientation turned into the pixels first.
    AllExif,
}

/// EXIF tag of the GPS IFD.
const GPS_IFD: u16 = 0x8825;
/// The most IFDs of the chain looked at.
const MAX_IFDS: usize = 8;
/// What JPEG APP1 segments start with.
const JPEG_EXIF: &[u8] = b"Exif\0\0";
const JPEG_XMP: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
const JPEG_XMP_EXTENSION: &[u8] = b"http://ns.adobe.com/xmp/extension/\0";
const JPEG_MPF: &[u8] = b"MPF\0";
/// The keyword of the PNG `iTXt` chunk that holds XMP.
const PNG_XMP: &[u8] = b"XML:com.adobe.xmp\0";
/// The flags of the WebP `VP8X` chunk for EXIF and XMP.
const WEBP_EXIF_FLAG: u8 = 0x08;
const WEBP_XMP_FLAG: u8 = 0x04;
const BROKEN_FILE: &str = "the file is broken";

/// Writes the file at `source` without what `strip` removes to `target`:
/// over `source` itself, or as a new file that must not exist yet, in the
/// format of `source`. The pixels are not touched, unless all EXIF goes from
/// a file that is not upright; a JPEG is then turned losslessly. XMP that
/// names a place goes too. The error is meant to be shown to the user as is.
pub fn strip(source: &Path, strip: Strip, target: &Path) -> Result<(), String> {
    let write = || -> Result<(), String> {
        let bytes = fs::read(source).map_err(|e| e.to_string())?;
        let format = strippable(source, &bytes)?;
        if ImageFormat::from_path(target).ok() != Some(format) {
            let extension = source
                .extension()
                .map(|e| e.to_string_lossy().into_owned())
                .unwrap_or_default();
            return Err(format!("the name must keep the extension .{extension}"));
        }
        if target != source && target.exists() {
            return Err("the file already exists".to_owned());
        }
        let bytes = match strip {
            Strip::Location => without_location(bytes, format)?,
            Strip::AllExif => without_exif(bytes, format)?,
        };
        if target == source {
            rotate::replace(target, &bytes).map_err(|e| e.to_string())
        } else {
            annotate::create(source, target, &bytes)
        }
    };
    write().map_err(|e| format!("Cannot save {}: {e}", folder::file_name(target)))
}

/// The format of the file, if its metadata can be removed without leaving
/// some behind.
fn strippable(path: &Path, bytes: &[u8]) -> Result<ImageFormat, String> {
    if decode::is_heif(path, bytes) {
        return Err("HEIC/HEIF and AVIF files cannot be written".to_owned());
    }
    if multi::count(bytes) > 1 {
        return Err("the other images in the file would keep their metadata".to_owned());
    }
    if matches!(motion::find(path), Ok(Some(_))) {
        return Err("the embedded video would keep its metadata".to_owned());
    }
    match image::guess_format(bytes) {
        Ok(format @ (ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP)) => Ok(format),
        _ => Err("only JPEG, PNG and WebP files can be changed".to_owned()),
    }
}

/// The file without its GPS tags, and without XMP that names a place. The
/// GPS IFD is cleared and unlinked where it stands, so every other offset
/// stays valid.
fn without_location(mut bytes: Vec<u8>, format: ImageFormat) -> Result<Vec<u8>, String> {
    let mut found = false;
    if let Ok(exif) = Reader::new().read_from_container(&mut Cursor::new(&bytes)) {
        let block = exif.buf();
        if !block.is_empty() {
            // The block is a copy of a piece of the file; it is changed where
            // it is.
            let start = bytes
                .windows(block.len())
                .position(|w| w == block)
                .ok_or("its EXIF is not stored in one piece")?;
            let end = start.checked_add(block.len()).ok_or(BROKEN)?;
            found = remove_gps(bytes.get_mut(start..end).ok_or(BROKEN)?).ok_or(BROKEN)?;
            if found && format == ImageFormat::Png {
                fix_png_crc(&mut bytes, start, end).ok_or(BROKEN)?;
            }
        }
    }
    let parts = parts(&bytes, format).ok_or(BROKEN_FILE)?;
    let placed = located_xmp(&bytes, &parts);
    if !found && placed.is_empty() {
        return Err("it has no location".to_owned());
    }
    without(&bytes, format, &parts, &placed)
}

/// The file without EXIF, and without XMP that names a place; a file that
/// is not upright is turned upright first.
fn without_exif(bytes: Vec<u8>, format: ImageFormat) -> Result<Vec<u8>, String> {
    let orientation = ImageReader::with_format(Cursor::new(&bytes), format)
        .into_decoder()
        .and_then(|mut decoder| decoder.orientation())
        .map_err(|e| e.to_string())?;
    let upright = orientation == Orientation::NoTransforms;
    let bytes = if upright {
        bytes
    } else {
        let parts = parts(&bytes, format).ok_or(BROKEN_FILE)?;
        if parts.iter().any(|p| p.kind == Kind::Mpf) {
            return Err(GAIN_MAP_LOST.to_owned());
        }
        rotate::upright(&bytes, format)
            .map_err(|e| format!("its orientation must be turned into the pixels first: {e}"))?
    };
    let parts = parts(&bytes, format).ok_or(BROKEN_FILE)?;
    let mut removed = located_xmp(&bytes, &parts);
    removed.extend(parts.iter().filter(|p| p.kind == Kind::Exif));
    if upright && !removed.iter().any(|p| p.kind == Kind::Exif) {
        return Err("it has no EXIF".to_owned());
    }
    without(&bytes, format, &parts, &removed)
}

const GAIN_MAP_LOST: &str = "the gain map stored behind the image would be lost";

/// What a part of a file holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Exif,
    Xmp,
    /// The JPEG MPF index, which points at pictures behind the first.
    Mpf,
    Other,
}

/// A JPEG header segment or a PNG or WebP chunk.
#[derive(Debug)]
struct Part {
    /// All of it, as removed.
    whole: Range<usize>,
    /// What it holds.
    data: Range<usize>,
    kind: Kind,
}

/// The parts of the file: a JPEG's segments up to its image data, a PNG's or
/// WebP's chunks. Nothing if they do not fit the file.
fn parts(bytes: &[u8], format: ImageFormat) -> Option<Vec<Part>> {
    match format {
        ImageFormat::Jpeg => jpeg_parts(bytes),
        ImageFormat::Png => png_parts(bytes),
        _ => webp_parts(bytes),
    }
}

fn jpeg_parts(bytes: &[u8]) -> Option<Vec<Part>> {
    let mut parts = Vec::new();
    // After the start-of-image marker.
    let mut at: usize = 2;
    loop {
        let [0xFF, marker] = *bytes.get(at..at.checked_add(2)?)? else {
            return None;
        };
        // Start of scan or end of image: no headers after it.
        if marker == 0xDA || marker == 0xD9 {
            return Some(parts);
        }
        let length = read_u16(bytes, at.checked_add(2)?, false)?;
        let data = at.checked_add(4)?..at.checked_add(2)?.checked_add(usize::from(length))?;
        let held = bytes.get(data.clone())?;
        let kind = match marker {
            0xE1 if held.starts_with(JPEG_EXIF) => Kind::Exif,
            0xE1 if held.starts_with(JPEG_XMP) || held.starts_with(JPEG_XMP_EXTENSION) => Kind::Xmp,
            0xE2 if held.starts_with(JPEG_MPF) => Kind::Mpf,
            _ => Kind::Other,
        };
        let end = data.end;
        parts.push(Part {
            whole: at..end,
            data,
            kind,
        });
        at = end;
    }
}

fn png_parts(bytes: &[u8]) -> Option<Vec<Part>> {
    let mut parts = Vec::new();
    let mut at = PNG_SIGNATURE.len();
    while at < bytes.len() {
        let length = read_u32(bytes, at, false)?;
        let kind_at = at.checked_add(4)?;
        let chunk = bytes.get(kind_at..kind_at.checked_add(4)?)?;
        let data = kind_at.checked_add(4)?..kind_at.checked_add(4)?.checked_add(length)?;
        // The CRC follows the data.
        let end = data.end.checked_add(4)?;
        bytes.get(at..end)?;
        let kind = match chunk {
            b"eXIf" => Kind::Exif,
            b"iTXt" if bytes.get(data.clone())?.starts_with(PNG_XMP) => Kind::Xmp,
            _ => Kind::Other,
        };
        let last = chunk == b"IEND";
        parts.push(Part {
            whole: at..end,
            data,
            kind,
        });
        if last {
            break;
        }
        at = end;
    }
    Some(parts)
}

fn webp_parts(bytes: &[u8]) -> Option<Vec<Part>> {
    let mut parts = Vec::new();
    // After the RIFF header.
    let mut at: usize = 12;
    while at.checked_add(8)? <= bytes.len() {
        let chunk = bytes.get(at..at.checked_add(4)?)?;
        let length = read_u32(bytes, at.checked_add(4)?, true)?;
        let data = at.checked_add(8)?..at.checked_add(8)?.checked_add(length)?;
        // Chunks are padded to an even length; the last one's pad may be
        // missing.
        let end = data
            .end
            .checked_add(length & 1)?
            .min(bytes.len().max(data.end));
        bytes.get(data.clone())?;
        let kind = match chunk {
            b"EXIF" => Kind::Exif,
            b"XMP " => Kind::Xmp,
            _ => Kind::Other,
        };
        parts.push(Part {
            whole: at..end,
            data,
            kind,
        });
        at = end;
    }
    Some(parts)
}

/// The XMP parts that name a place.
fn located_xmp<'a>(bytes: &[u8], parts: &'a [Part]) -> Vec<&'a Part> {
    parts
        .iter()
        .filter(|p| {
            p.kind == Kind::Xmp
                && bytes
                    .get(p.data.clone())
                    .is_some_and(|xmp| xmp.windows(3).any(|w| w == b"GPS"))
        })
        .collect()
}

/// The file without the parts `removed`; a WebP's size and flags follow.
fn without(
    bytes: &[u8],
    format: ImageFormat,
    parts: &[Part],
    removed: &[&Part],
) -> Result<Vec<u8>, String> {
    // The MPF index counts from where it stands: what follows it must stay.
    let mpf = parts.iter().find(|p| p.kind == Kind::Mpf);
    if let Some(mpf) = mpf
        && removed.iter().any(|p| p.whole.start > mpf.whole.start)
    {
        return Err(GAIN_MAP_LOST.to_owned());
    }
    let mut removed = removed.to_vec();
    removed.sort_by_key(|p| p.whole.start);
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    for part in &removed {
        out.extend_from_slice(bytes.get(at..part.whole.start).ok_or(BROKEN_FILE)?);
        at = part.whole.end;
    }
    out.extend_from_slice(bytes.get(at..).ok_or(BROKEN_FILE)?);
    if format == ImageFormat::WebP {
        let flags = removed.iter().fold(0, |flags, p| match p.kind {
            Kind::Exif => flags | WEBP_EXIF_FLAG,
            Kind::Xmp => flags | WEBP_XMP_FLAG,
            _ => flags,
        });
        fix_webp(&mut out, flags).ok_or(BROKEN_FILE)?;
    }
    Ok(out)
}

/// Sets the RIFF size of the WebP `webp` to its length and clears `flags` in
/// its `VP8X` chunk, if it has one.
fn fix_webp(webp: &mut [u8], flags: u8) -> Option<()> {
    let size = u32::try_from(webp.len().checked_sub(8)?).ok()?;
    webp.get_mut(4..8)?.copy_from_slice(&size.to_le_bytes());
    if webp.get(12..16)? == b"VP8X" {
        let first = webp.get_mut(20)?;
        *first &= !flags;
    }
    Some(())
}

/// Clears and unlinks the GPS IFD of the TIFF-structured EXIF block `tiff`;
/// whether it had one, or nothing if the block is broken.
fn remove_gps(tiff: &mut [u8]) -> Option<bool> {
    let little = match tiff.get(0..2)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let mut ifd = read_u32(tiff, 4, little)?;
    let mut found = false;
    for _ in 0..MAX_IFDS {
        if ifd == 0 {
            break;
        }
        if let Some(at) = entry(tiff, ifd, GPS_IFD, little) {
            let gps = read_u32(tiff, at.checked_add(8)?, little)?;
            clear_ifd(tiff, gps, little)?;
            remove_entry(tiff, ifd, at, little)?;
            found = true;
        }
        let count = usize::from(read_u16(tiff, ifd, little)?);
        ifd = read_u32(
            tiff,
            ifd.checked_add(2)?.checked_add(count.checked_mul(12)?)?,
            little,
        )?;
    }
    Some(found)
}

/// Overwrites the IFD at `ifd` and the values it points at with zeros.
fn clear_ifd(tiff: &mut [u8], ifd: usize, little: bool) -> Option<()> {
    let count = usize::from(read_u16(tiff, ifd, little)?);
    for index in 0..count {
        let at = ifd.checked_add(2)?.checked_add(index.checked_mul(12)?)?;
        let kind = read_u16(tiff, at.checked_add(2)?, little)?;
        let values = read_u32(tiff, at.checked_add(4)?, little)?;
        let size = type_size(kind)?.checked_mul(values)?;
        // Up to four bytes stand in the entry itself.
        if size > 4 {
            let offset = read_u32(tiff, at.checked_add(8)?, little)?;
            tiff.get_mut(offset..offset.checked_add(size)?)?.fill(0);
        }
    }
    // The entries and the offset of the next IFD.
    let end = ifd.checked_add(6)?.checked_add(count.checked_mul(12)?)?;
    tiff.get_mut(ifd..end)?.fill(0);
    Some(())
}

/// The size in bytes of one value of the TIFF field type `kind`.
fn type_size(kind: u16) -> Option<usize> {
    match kind {
        1 | 2 | 6 | 7 => Some(1),
        3 | 8 => Some(2),
        4 | 9 | 11 => Some(4),
        5 | 10 | 12 => Some(8),
        _ => None,
    }
}

/// Takes the entry at `at` out of the IFD at `ifd`: the later entries and
/// the offset of the next IFD move up, and the space freed is zeroed.
fn remove_entry(tiff: &mut [u8], ifd: usize, at: usize, little: bool) -> Option<()> {
    let count = read_u16(tiff, ifd, little)?;
    let end = ifd
        .checked_add(6)?
        .checked_add(usize::from(count).checked_mul(12)?)?;
    let rest = tiff.get_mut(at..end)?;
    // The entry itself and at least the offset of the next IFD.
    let kept = rest.len().checked_sub(12).filter(|k| *k >= 4)?;
    rest.copy_within(12.., 0);
    rest.get_mut(kept..)?.fill(0);
    let fewer = count.checked_sub(1)?;
    let raw = if little {
        fewer.to_le_bytes()
    } else {
        fewer.to_be_bytes()
    };
    tiff.get_mut(ifd..ifd.checked_add(2)?)?
        .copy_from_slice(&raw);
    Some(())
}

fn read_u16(bytes: &[u8], at: usize, little: bool) -> Option<u16> {
    let raw: [u8; 2] = bytes.get(at..at.checked_add(2)?)?.try_into().ok()?;
    Some(if little {
        u16::from_le_bytes(raw)
    } else {
        u16::from_be_bytes(raw)
    })
}

fn read_u32(bytes: &[u8], at: usize, little: bool) -> Option<usize> {
    let raw: [u8; 4] = bytes.get(at..at.checked_add(4)?)?.try_into().ok()?;
    let value = if little {
        u32::from_le_bytes(raw)
    } else {
        u32::from_be_bytes(raw)
    };
    usize::try_from(value).ok()
}
