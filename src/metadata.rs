//! The metadata of a file for the metadata view, and writing its capture
//! date. The date is patched where it stands in the EXIF block, at its fixed
//! length, so every offset stays valid and no pixel is touched. The file is
//! replaced through a temporary file, as a rotation is.

use std::fs::{self, File};
use std::io::{BufReader, Cursor};
use std::path::Path;
use std::time::SystemTime;

use chrono::{DateTime, Local, NaiveDateTime};
use exif::{Exif, In, Reader, Tag, Value};

use crate::{folder, rotate};

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
