//! Seam 3: reading metadata from real files, and writing the capture date
//! into copies of them and removing their location or all EXIF, in a
//! temporary folder.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use image::codecs::png::PngEncoder;
use image::codecs::webp::WebPEncoder;
use image::{DynamicImage, ImageEncoder, ImageFormat, Rgba, RgbaImage};
use pixpixpix::decode::{self, DecodedImage};
use pixpixpix::metadata::{self, Field, Strip};
use pixpixpix::turbojpeg;

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

/// The EXIF of the file, if it has any.
fn exif_of(path: &Path) -> Option<exif::Exif> {
    let mut reader = std::io::BufReader::new(std::fs::File::open(path).ok()?);
    exif::Reader::new().read_from_container(&mut reader).ok()
}

fn gps_fields(path: &Path) -> usize {
    exif_of(path).map_or(0, |exif| {
        exif.fields()
            .filter(|f| f.tag.context() == exif::Context::Gps)
            .count()
    })
}

fn contains(bytes: &[u8], part: &[u8]) -> bool {
    bytes.windows(part.len()).any(|w| w == part)
}

#[test]
fn the_location_goes_and_everything_else_stays() {
    for name in ["dated.jpg", "dated.png", "dated.webp"] {
        let (dir, path) = copy(name).unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(gps_fields(&path) > 0, "{name}");
        assert!(metadata::read(&path).unwrap().has_location, "{name}");
        let extension = path.extension().unwrap().to_str().unwrap();
        let target = dir.path().join(format!("dated_noloc.{extension}"));
        metadata::strip(&path, Strip::Location, &target).unwrap();
        // The original stays as it was.
        assert_eq!(std::fs::read(&path).unwrap(), before, "{name}");
        let after = std::fs::read(&target).unwrap();
        // Cleared where it stood: nothing moved.
        assert_eq!(after.len(), before.len(), "{name}");
        assert_eq!(gps_fields(&target), 0, "{name}");
        let read = metadata::read(&target).unwrap();
        assert!(!read.has_location, "{name}");
        assert!(read.has_exif, "{name}");
        assert_eq!(value(&read.exif, "GPS"), None, "{name}");
        assert_eq!(value(&read.exif, "Camera"), Some("Pixcam P3 Mark II"));
        assert_eq!(value(&read.exif, "Lens"), Some("Pix 50mm F1.8"));
        assert_eq!(value(&read.exif, "Orientation"), Some("1 (upright)"));
        assert_eq!(read.captured.as_deref(), Some("2019-06-07 08:09:10"));
        assert_eq!(decode_file(&target).unwrap(), decode_file(&path).unwrap());
    }
}

#[test]
fn the_png_chunk_without_the_location_has_a_valid_crc() {
    let (dir, path) = copy("dated.png").unwrap();
    let target = dir.path().join("dated_noloc.png");
    metadata::strip(&path, Strip::Location, &target).unwrap();
    let after = std::fs::read(&target).unwrap();
    let kind = after.windows(4).position(|w| w == b"eXIf").unwrap();
    let length = u32::from_be_bytes(after[kind - 4..kind].try_into().unwrap()) as usize;
    let crc_at = kind + 4 + length;
    let crc = u32::from_be_bytes(after[crc_at..crc_at + 4].try_into().unwrap());
    assert_eq!(crc, crc32fast::hash(&after[kind..crc_at]));
}

#[test]
fn all_exif_goes_and_the_pixels_stay() {
    for name in ["dated.jpg", "dated.png", "dated.webp"] {
        let (dir, path) = copy(name).unwrap();
        let extension = path.extension().unwrap().to_str().unwrap();
        let target = dir.path().join(format!("dated_noexif.{extension}"));
        metadata::strip(&path, Strip::AllExif, &target).unwrap();
        assert!(exif_of(&target).is_none(), "{name}");
        let read = metadata::read(&target).unwrap();
        assert!(!read.has_exif && !read.has_location, "{name}");
        assert_eq!(read.captured, None, "{name}");
        assert_eq!(decode_file(&target).unwrap(), decode_file(&path).unwrap());
    }
}

#[test]
fn all_exif_goes_from_a_jpeg_as_its_whole_segment() {
    let (dir, path) = copy("dated.jpg").unwrap();
    let target = dir.path().join("dated_noexif.jpg");
    metadata::strip(&path, Strip::AllExif, &target).unwrap();
    let before = std::fs::read(&path).unwrap();
    let start = before.windows(6).position(|w| w == b"Exif\0\0").unwrap() - 4;
    let length = u16::from_be_bytes([before[start + 2], before[start + 3]]) as usize;
    let mut expected = before[..start].to_vec();
    expected.extend_from_slice(&before[start + 2 + length..]);
    assert_eq!(std::fs::read(&target).unwrap(), expected);
}

#[test]
fn a_webp_without_exif_has_its_size_and_flag_fixed() {
    let (dir, path) = copy("dated.webp").unwrap();
    let target = dir.path().join("dated_noexif.webp");
    metadata::strip(&path, Strip::AllExif, &target).unwrap();
    let after = std::fs::read(&target).unwrap();
    assert!(!contains(&after, b"EXIF"));
    let size = u32::from_le_bytes(after[4..8].try_into().unwrap()) as usize;
    assert_eq!(size, after.len() - 8);
    assert_eq!(&after[12..16], b"VP8X");
    assert_eq!(after[20] & 0x08, 0);
}

#[test]
fn a_png_without_exif_has_no_exif_chunk() {
    let (dir, path) = copy("dated.png").unwrap();
    let target = dir.path().join("dated_noexif.png");
    metadata::strip(&path, Strip::AllExif, &target).unwrap();
    assert!(!contains(&std::fs::read(&target).unwrap(), b"eXIf"));
}

/// TIFF-structured EXIF holding only orientation `value`.
fn exif_orientation(value: u8) -> Vec<u8> {
    let mut exif = b"MM\0\x2a\0\0\0\x08\0\x01".to_vec();
    exif.extend_from_slice(&[0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, value, 0, 0]);
    exif.extend_from_slice(&[0, 0, 0, 0]);
    exif
}

fn pixel(image: &DecodedImage, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * image.width + x) * 4) as usize;
    [
        image.rgba[i],
        image.rgba[i + 1],
        image.rgba[i + 2],
        image.rgba[i + 3],
    ]
}

fn is_red([r, g, b, _]: [u8; 4]) -> bool {
    r > 200 && g < 60 && b < 60
}

fn is_blue([r, g, b, _]: [u8; 4]) -> bool {
    r < 60 && g < 60 && b > 200
}

#[test]
fn the_orientation_is_baked_into_reencoded_pixels_before_all_exif_goes() {
    // Stored 4 × 2 red left, blue right; orientation 6 shows it turned
    // clockwise, red on top.
    let stored = decode_file(&fixture("sample.png")).unwrap();
    let rgba = RgbaImage::from_raw(4, 2, stored.rgba.to_vec()).unwrap();
    for format in [ImageFormat::Png, ImageFormat::WebP] {
        let dir = tempfile::tempdir().unwrap();
        let extension = format.extensions_str()[0];
        let path = dir.path().join(format!("turned.{extension}"));
        let mut bytes = Vec::new();
        if format == ImageFormat::Png {
            let mut encoder = PngEncoder::new(&mut bytes);
            encoder.set_exif_metadata(exif_orientation(6)).unwrap();
            encoder
                .write_image(rgba.as_raw(), 4, 2, image::ExtendedColorType::Rgba8)
                .unwrap();
        } else {
            let mut encoder = WebPEncoder::new_lossless(&mut bytes);
            encoder.set_exif_metadata(exif_orientation(6)).unwrap();
            encoder
                .write_image(rgba.as_raw(), 4, 2, image::ExtendedColorType::Rgba8)
                .unwrap();
        }
        std::fs::write(&path, bytes).unwrap();
        let shown = decode_file(&path).unwrap();
        assert_eq!((shown.width, shown.height), (2, 4), "{extension}");

        let target = dir.path().join(format!("turned_noexif.{extension}"));
        metadata::strip(&path, Strip::AllExif, &target).unwrap();
        assert!(exif_of(&target).is_none(), "{extension}");
        let image = decode_file(&target).unwrap();
        assert_eq!(image, shown, "{extension}");
        assert!(is_red(pixel(&image, 0, 0)) && is_blue(pixel(&image, 1, 3)));
        assert_eq!(
            image::guess_format(&std::fs::read(&target).unwrap()).ok(),
            Some(format)
        );
    }
}

/// A 16 × 16 JPEG, red on the left half and blue on the right, with EXIF
/// orientation 6.
fn turned_jpeg(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let image = RgbaImage::from_fn(16, 16, |x, _| {
        if x < 8 {
            Rgba([255, 0, 0, 255])
        } else {
            Rgba([0, 0, 255, 255])
        }
    });
    let mut plain = std::io::Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .to_rgb8()
        .write_to(&mut plain, ImageFormat::Jpeg)?;
    let plain = plain.into_inner();
    let mut segment = b"Exif\0\0".to_vec();
    segment.extend_from_slice(&exif_orientation(6));
    let mut bytes = plain[..2].to_vec();
    bytes.extend_from_slice(&[0xFF, 0xE1]);
    bytes.extend_from_slice(&((segment.len() + 2) as u16).to_be_bytes());
    bytes.extend_from_slice(&segment);
    bytes.extend_from_slice(&plain[2..]);
    std::fs::write(path, bytes)?;
    Ok(())
}

#[test]
fn a_jpeg_is_turned_losslessly_before_all_exif_goes_or_refused_without_libturbojpeg() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("photo.jpg");
    turned_jpeg(&path).unwrap();
    let before = std::fs::read(&path).unwrap();
    let target = dir.path().join("photo_noexif.jpg");
    match turbojpeg::load(turbojpeg::LIBRARY_NAMES) {
        Ok(_) => {
            metadata::strip(&path, Strip::AllExif, &target).unwrap();
            assert!(exif_of(&target).is_none());
            let image = decode_file(&target).unwrap();
            assert_eq!((image.width, image.height), (16, 16));
            // Turned clockwise as shown: red on top, blue below.
            assert!(is_red(pixel(&image, 8, 2)));
            assert!(is_blue(pixel(&image, 8, 13)));
        }
        Err(_) => {
            let error = metadata::strip(&path, Strip::AllExif, &target).unwrap_err();
            assert!(
                error.starts_with(
                    "Cannot save photo_noexif.jpg: its orientation must be turned into the \
                     pixels first: Lossless JPEG rotation needs libturbojpeg"
                ),
                "{error}"
            );
            assert!(!target.exists());
        }
    }
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

/// `jpeg` with the APP1 segment of `data` right after its start.
fn with_app1(jpeg: &[u8], data: &[u8]) -> Vec<u8> {
    let mut bytes = jpeg[..2].to_vec();
    bytes.extend_from_slice(&[0xFF, 0xE1]);
    bytes.extend_from_slice(&((data.len() + 2) as u16).to_be_bytes());
    bytes.extend_from_slice(data);
    bytes.extend_from_slice(&jpeg[2..]);
    bytes
}

const XMP: &[u8] = b"http://ns.adobe.com/xap/1.0/\0<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\
<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
<rdf:Description xmlns:exif=\"http://ns.adobe.com/exif/1.0/\" \
exif:GPSLatitude=\"48,8.2308N\" exif:GPSLongitude=\"11,34.596E\"/>\
</rdf:RDF></x:xmpmeta>";

#[test]
fn xmp_that_names_a_place_goes_with_the_location_and_with_all_exif() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("placed.jpg");
    let original = std::fs::read(fixture("dated.jpg")).unwrap();
    std::fs::write(&path, with_app1(&original, XMP)).unwrap();
    for strip in [Strip::Location, Strip::AllExif] {
        let target = dir.path().join(format!("placed_{strip:?}.jpg"));
        metadata::strip(&path, strip, &target).unwrap();
        let after = std::fs::read(&target).unwrap();
        assert!(!contains(&after, b"GPSLatitude"), "{strip:?}");
        assert!(
            !contains(&after, b"http://ns.adobe.com/xap/1.0/"),
            "{strip:?}"
        );
        assert_eq!(gps_fields(&target), 0, "{strip:?}");
        assert_eq!(decode_file(&target).unwrap(), decode_file(&path).unwrap());
    }
}

#[test]
fn overwrite_replaces_the_file_and_a_new_file_must_not_exist_yet() {
    let (dir, path) = copy("dated.jpg").unwrap();
    let taken = dir.path().join("dated_noloc.jpg");
    std::fs::write(&taken, b"mine").unwrap();
    let error = metadata::strip(&path, Strip::Location, &taken).unwrap_err();
    assert_eq!(
        error,
        "Cannot save dated_noloc.jpg: the file already exists"
    );
    assert_eq!(std::fs::read(&taken).unwrap(), b"mine");

    let pixels = decode_file(&path).unwrap();
    metadata::strip(&path, Strip::Location, &path).unwrap();
    assert_eq!(gps_fields(&path), 0);
    assert_eq!(decode_file(&path).unwrap(), pixels);
    metadata::strip(&path, Strip::AllExif, &path).unwrap();
    assert!(exif_of(&path).is_none());
    assert_eq!(decode_file(&path).unwrap(), pixels);
    // No temporary file is left behind.
    let mut names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, ["dated.jpg", "dated_noloc.jpg"]);
}

#[test]
fn files_that_cannot_be_stripped_safely_are_refused_and_left_untouched() {
    let cases = [
        (
            "sample.heic",
            Strip::Location,
            "HEIC/HEIF and AVIF files cannot be written",
        ),
        (
            "sample.avif",
            Strip::AllExif,
            "HEIC/HEIF and AVIF files cannot be written",
        ),
        (
            "stereo.mpo",
            Strip::AllExif,
            "the other images in the file would keep their metadata",
        ),
        (
            "sample.bmp",
            Strip::AllExif,
            "only JPEG, PNG and WebP files can be changed",
        ),
        ("sample.jpg", Strip::Location, "it has no location"),
        ("sample.jpg", Strip::AllExif, "it has no EXIF"),
    ];
    for (name, strip, reason) in cases {
        let (dir, path) = copy(name).unwrap();
        let before = std::fs::read(&path).unwrap();
        let target = dir.path().join(format!("new_{name}"));
        let error = metadata::strip(&path, strip, &target).unwrap_err();
        assert_eq!(error, format!("Cannot save new_{name}: {reason}"));
        assert!(!target.exists(), "{name}");
        assert_eq!(std::fs::read(&path).unwrap(), before, "{name}");
    }
}

#[test]
fn a_new_file_keeps_the_format_of_the_original() {
    let (dir, path) = copy("dated.jpg").unwrap();
    let target = dir.path().join("dated_noloc.png");
    let error = metadata::strip(&path, Strip::Location, &target).unwrap_err();
    assert_eq!(
        error,
        "Cannot save dated_noloc.png: the name must keep the extension .jpg"
    );
    assert!(!target.exists());
}

#[test]
fn exif_behind_the_mpf_index_of_a_gain_map_is_not_moved() {
    // gainmap.jpg with the EXIF of dated.jpg after its MPF segment.
    let gainmap = std::fs::read(fixture("gainmap.jpg")).unwrap();
    let dated = std::fs::read(fixture("dated.jpg")).unwrap();
    let exif_start = dated.windows(6).position(|w| w == b"Exif\0\0").unwrap() - 4;
    let exif_length = u16::from_be_bytes([dated[exif_start + 2], dated[exif_start + 3]]) as usize;
    let segment = &dated[exif_start..exif_start + 2 + exif_length];
    let mpf = gainmap.windows(4).position(|w| w == b"MPF\0").unwrap() - 4;
    let mpf_end = mpf + 2 + u16::from_be_bytes([gainmap[mpf + 2], gainmap[mpf + 3]]) as usize;
    let mut bytes = gainmap[..mpf_end].to_vec();
    bytes.extend_from_slice(segment);
    bytes.extend_from_slice(&gainmap[mpf_end..]);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hdr.jpg");
    std::fs::write(&path, bytes).unwrap();
    let target = dir.path().join("hdr_noexif.jpg");
    let error = metadata::strip(&path, Strip::AllExif, &target).unwrap_err();
    assert_eq!(
        error,
        "Cannot save hdr_noexif.jpg: the gain map stored behind the image would be lost"
    );
    // Clearing the location moves nothing, so it may.
    metadata::strip(&path, Strip::Location, &dir.path().join("hdr_noloc.jpg")).unwrap();
}
