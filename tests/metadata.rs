//! Seam 3: reading metadata from real files and writing the capture date
//! into copies of them, in a temporary folder.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use pixpixpix::decode::{self, DecodedImage};
use pixpixpix::metadata::{self, Field};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// A copy of the fixture `name` in a fresh temporary folder.
fn copy(name: &str) -> std::io::Result<(tempfile::TempDir, PathBuf)> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join(name);
    std::fs::copy(fixture(name), &path)?;
    Ok((dir, path))
}

fn decode_file(path: &Path) -> Result<DecodedImage, String> {
    decode::decode_file(path, &AtomicBool::new(false)).unwrap_or_else(|| Err("cancelled".into()))
}

fn value<'a>(fields: &'a [Field], label: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|f| f.label == label)
        .map(|f| f.value.as_str())
}

/// `bytes` with every `from` replaced by `to`, of the same length.
fn replaced(bytes: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    let mut i = 0;
    while i + from.len() <= out.len() {
        if &out[i..i + from.len()] == from {
            out[i..i + from.len()].copy_from_slice(to);
            i += from.len();
        } else {
            i += 1;
        }
    }
    out
}

#[test]
fn the_camera_fields_of_a_jpeg_are_read() {
    let read = metadata::read(&fixture("dated.jpg")).unwrap();
    assert_eq!(value(&read.file, "File"), Some("dated.jpg"));
    assert!(value(&read.file, "Size").unwrap().ends_with("kB"));
    assert!(value(&read.file, "Modified").is_some());
    assert_eq!(read.captured.as_deref(), Some("2019-06-07 08:09:10"));
    let exif = &read.exif;
    assert_eq!(value(exif, "Camera"), Some("Pixcam P3 Mark II"));
    assert_eq!(value(exif, "Lens"), Some("Pix 50mm F1.8"));
    assert_eq!(value(exif, "Time zone"), Some("+02:00"));
    assert_eq!(value(exif, "Exposure"), Some("1/125 s at f/2.8"));
    assert_eq!(value(exif, "ISO"), Some("200"));
    assert_eq!(value(exif, "Focal length"), Some("50 mm"));
    assert_eq!(value(exif, "GPS"), Some("48.13718, 11.57660"));
    assert_eq!(value(exif, "Orientation"), Some("1 (upright)"));
}

#[test]
fn png_and_webp_carry_the_capture_date_too() {
    for name in ["dated.png", "dated.webp"] {
        let read = metadata::read(&fixture(name)).unwrap();
        assert_eq!(
            read.captured.as_deref(),
            Some("2019-06-07 08:09:10"),
            "{name}"
        );
    }
}

#[test]
fn a_file_without_exif_has_only_its_file_fields() {
    let read = metadata::read(&fixture("sample.bmp")).unwrap();
    assert_eq!(value(&read.file, "File"), Some("sample.bmp"));
    assert_eq!(read.captured, None);
    assert!(read.exif.is_empty());
}

#[test]
fn a_missing_file_cannot_be_read() {
    let error = metadata::read(Path::new("/nonexistent/x.jpg")).unwrap_err();
    assert!(error.starts_with("Cannot read x.jpg"), "{error}");
}

#[test]
fn a_typed_date_must_be_a_full_date() {
    assert_eq!(
        metadata::stored_date("2021-12-24 18:30:00").as_deref(),
        Some("2021:12:24 18:30:00")
    );
    for wrong in [
        "2021-12-24",
        "2021-13-24 18:30:00",
        "2021-02-30 18:30:00",
        "2021-12-24 18:30",
        "2021-12-24 8:30:00",
        "2021:12:24 18:30:00",
        " 2021-12-24 18:30:00",
    ] {
        assert_eq!(metadata::stored_date(wrong), None, "{wrong}");
    }
}

const OLD_ORIGINAL: &[u8] = b"2019:06:07 08:09:10";
const OLD_CHANGED: &[u8] = b"2020:01:02 03:04:05";
const NEW: &[u8] = b"2021:12:24 18:30:00";

/// Writes the new date into a copy of `name`: the dates are the only bytes
/// changed, and the pixels decode the same.
fn writes_only_the_dates(name: &str) -> Result<(Vec<u8>, Vec<u8>), Box<dyn std::error::Error>> {
    let (_dir, path) = copy(name)?;
    let before = std::fs::read(&path)?;
    let pixels = decode_file(&path)?;
    metadata::write_date(&path, "2021:12:24 18:30:00")?;
    let after = std::fs::read(&path)?;
    let expected = replaced(&replaced(&before, OLD_ORIGINAL, NEW), OLD_CHANGED, NEW);
    assert_eq!(decode_file(&path)?, pixels, "{name}");
    let read = metadata::read(&path)?;
    assert_eq!(read.captured.as_deref(), Some("2021-12-24 18:30:00"));
    Ok((expected, after))
}

#[test]
fn the_date_of_a_jpeg_is_written_in_place() {
    let (expected, after) = writes_only_the_dates("dated.jpg").unwrap();
    assert_eq!(after, expected);
}

#[test]
fn the_date_of_a_webp_is_written_in_place() {
    let (expected, after) = writes_only_the_dates("dated.webp").unwrap();
    assert_eq!(after, expected);
}

#[test]
fn the_date_of_a_png_is_written_in_place_with_a_valid_chunk_crc() {
    let (expected, after) = writes_only_the_dates("dated.png").unwrap();
    assert_eq!(after.len(), expected.len());
    // The eXIf chunk's CRC is the one other change; it must match the chunk.
    let kind = after.windows(4).position(|w| w == b"eXIf").unwrap();
    let length = u32::from_be_bytes(after[kind - 4..kind].try_into().unwrap()) as usize;
    let crc_at = kind + 4 + length;
    assert_eq!(after[..crc_at], expected[..crc_at]);
    assert_eq!(after[crc_at + 4..], expected[crc_at + 4..]);
    let crc = u32::from_be_bytes(after[crc_at..crc_at + 4].try_into().unwrap());
    assert_eq!(crc, crc32fast::hash(&after[kind..crc_at]));
}

#[test]
fn a_file_without_a_capture_date_is_left_alone() {
    let (_dir, path) = copy("orientation6.jpg").unwrap();
    let before = std::fs::read(&path).unwrap();
    let error = metadata::write_date(&path, "2021:12:24 18:30:00").unwrap_err();
    assert!(
        error.starts_with("Cannot change the date of orientation6.jpg"),
        "{error}"
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
}
