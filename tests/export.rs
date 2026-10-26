//! Seam 3: exports on real files: the area scaled to exactly the target
//! size, locked or stretched, in the chosen format, within a size limit and
//! without metadata.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use pixpixpix::annotate;
use pixpixpix::core::{Color, Export, ExportFormat, ImagePoint, Region, Shape};
use pixpixpix::decode::{self, DecodedImage};
use pixpixpix::rotate::Rotation;

const RED: [u8; 4] = [0xff, 0, 0, 0xff];
const BLUE: [u8; 4] = [0, 0, 0xff, 0xff];

/// 200x100, the left half red, the right half blue.
fn halves() -> DecodedImage {
    let mut rgba = Vec::new();
    for _ in 0..100 {
        for x in 0..200 {
            rgba.extend(if x < 100 { RED } else { BLUE });
        }
    }
    DecodedImage {
        width: 200,
        height: 100,
        rgba: Arc::from(rgba),
    }
}

/// Pixels that compress badly, the same on every run.
fn noise(width: u32, height: u32) -> DecodedImage {
    let mut state: u32 = 0x1234_5678;
    let rgba = (0..width * height * 4)
        .map(|i| {
            if i % 4 == 3 {
                return 0xff;
            }
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        })
        .collect::<Vec<u8>>();
    DecodedImage {
        width,
        height,
        rgba: Arc::from(rgba),
    }
}

fn region(left: f32, top: f32, right: f32, bottom: f32) -> Region {
    Region {
        left,
        top,
        right,
        bottom,
    }
}

fn export(width: u32, height: u32, format: ExportFormat, limit_kb: Option<u32>) -> Export {
    Export {
        width,
        height,
        locked: true,
        format,
        limit_kb,
    }
}

fn pixel(image: &DecodedImage, x: u32, y: u32) -> [u8; 4] {
    let at = ((y * image.width + x) * 4) as usize;
    let mut pixel = [0; 4];
    pixel.copy_from_slice(&image.rgba[at..at + 4]);
    pixel
}

fn near(a: [u8; 4], b: [u8; 4]) -> bool {
    a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 8)
}

fn decoded(path: &Path) -> Option<DecodedImage> {
    decode::decode_file(path, &AtomicBool::new(false))?.ok()
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn every_format_has_exactly_the_target_size() {
    let dir = tempfile::tempdir().unwrap();
    for format in ExportFormat::ALL {
        let target = dir.path().join(format!("a_37x23.{}", format.extension()));
        annotate::save_export(
            &fixture("sample.jpg"),
            &halves(),
            Rotation::None,
            &[],
            region(10.0, 5.0, 150.0, 95.0),
            &export(37, 23, format, None),
            &target,
        )
        .unwrap();
        let written = image::open(&target).unwrap();
        assert_eq!((written.width(), written.height()), (37, 23), "{format:?}");
        let guessed = image::ImageFormat::from_path(&target).unwrap();
        assert_eq!(
            image::guess_format(&fs::read(&target).unwrap()).unwrap(),
            guessed
        );
    }
}

#[test]
fn an_area_of_the_ratio_keeps_its_content_and_another_one_is_stretched() {
    let square = export(50, 50, ExportFormat::Png, None);
    // The left square of the image: red only.
    let locked = annotate::exported(
        &halves(),
        Rotation::None,
        &[],
        region(0.0, 0.0, 100.0, 100.0),
        &square,
    )
    .unwrap();
    assert_eq!((locked.width, locked.height), (50, 50));
    for (x, y) in [(0, 0), (49, 0), (25, 25), (49, 49)] {
        assert!(near(pixel(&locked, x, y), RED), "{x},{y}");
    }
    // The whole 2:1 image squeezed into the square: half red, half blue.
    let free = annotate::exported(
        &halves(),
        Rotation::None,
        &[],
        region(0.0, 0.0, 200.0, 100.0),
        &Export {
            locked: false,
            ..square
        },
    )
    .unwrap();
    assert_eq!((free.width, free.height), (50, 50));
    assert!(near(pixel(&free, 5, 25), RED));
    assert!(near(pixel(&free, 44, 25), BLUE));
}

#[test]
fn the_pending_turn_is_applied_before_scaling() {
    let exported = annotate::exported(
        &halves(),
        Rotation::Right,
        &[],
        region(0.0, 0.0, 200.0, 100.0),
        &export(50, 100, ExportFormat::Png, None),
    )
    .unwrap();
    assert_eq!((exported.width, exported.height), (50, 100));
    // Turned right, the left half is on top.
    assert!(near(pixel(&exported, 25, 10), RED));
    assert!(near(pixel(&exported, 25, 90), BLUE));
}

#[test]
fn the_edits_are_drawn_in_but_a_crop_does_not_move_the_area() {
    let shapes = [
        Shape::Highlighter {
            from: ImagePoint { x: 0.0, y: 0.0 },
            to: ImagePoint { x: 50.0, y: 100.0 },
            color: Color::Yellow,
        },
        Shape::Crop(region(100.0, 0.0, 200.0, 100.0)),
    ];
    let exported = annotate::exported(
        &halves(),
        Rotation::None,
        &shapes,
        region(0.0, 0.0, 100.0, 100.0),
        &export(100, 100, ExportFormat::Png, None),
    )
    .unwrap();
    assert!(!near(pixel(&exported, 10, 50), RED));
    assert!(near(pixel(&exported, 90, 50), RED));
}

#[test]
fn a_jpeg_size_limit_is_met_by_lowering_the_quality() {
    let image = noise(256, 256);
    let unlimited =
        annotate::encode_export(&image, &export(256, 256, ExportFormat::Jpeg, None)).unwrap();
    let limit_kb = u32::try_from(unlimited.len() / 1024 / 2).unwrap();
    let limited = annotate::encode_export(
        &image,
        &export(256, 256, ExportFormat::Jpeg, Some(limit_kb)),
    )
    .unwrap();
    assert!(limited.len() <= limit_kb as usize * 1024);
    let back = image::load_from_memory(&limited).unwrap();
    assert_eq!((back.width(), back.height()), (256, 256));
    let error = annotate::encode_export(&image, &export(256, 256, ExportFormat::Jpeg, Some(1)))
        .unwrap_err();
    assert!(error.contains("does not fit in 1 KB"), "{error}");
}

#[test]
fn png_limits_are_only_checked() {
    let image = noise(128, 128);
    let generous = export(128, 128, ExportFormat::Png, Some(1024));
    assert!(annotate::encode_export(&image, &generous).is_ok());
    let error =
        annotate::encode_export(&image, &export(128, 128, ExportFormat::Png, Some(1))).unwrap_err();
    assert!(error.contains("does not fit in 1 KB"), "{error}");
}

#[test]
fn webp_has_no_size_limit() {
    let image = noise(128, 128);
    assert!(
        annotate::encode_export(&image, &export(128, 128, ExportFormat::WebP, Some(1))).is_ok()
    );
}

/// Whether `needle` occurs in `bytes`.
fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn exports_carry_no_metadata() {
    let source = fixture("orientation6.jpg");
    assert!(contains(&fs::read(&source).unwrap(), b"Exif\0\0"));
    let image = decoded(&source).unwrap();
    let dir = tempfile::tempdir().unwrap();
    for format in ExportFormat::ALL {
        let target = dir.path().join(format!("o.{}", format.extension()));
        annotate::save_export(
            &source,
            &image,
            Rotation::None,
            &[],
            Region::whole(image.width, image.height),
            &export(16, 16, format, None),
            &target,
        )
        .unwrap();
        let bytes = fs::read(&target).unwrap();
        for tag in [
            b"Exif".as_slice(),
            b"ICC_PROFILE",
            b"eXIf",
            b"iCCP",
            b"tEXt",
            b"iTXt",
            b"zTXt",
            b"EXIF",
            b"ICCP",
            b"XMP ",
            b"http://ns.adobe.com/xap",
        ] {
            assert!(
                !contains(&bytes, tag),
                "{format:?} has {}",
                String::from_utf8_lossy(tag)
            );
        }
    }
}

#[test]
fn the_name_must_match_the_format_and_be_new() {
    let dir = tempfile::tempdir().unwrap();
    let save = |name: &str, format| {
        annotate::save_export(
            &fixture("sample.jpg"),
            &halves(),
            Rotation::None,
            &[],
            region(0.0, 0.0, 200.0, 100.0),
            &export(20, 10, format, None),
            &dir.path().join(name),
        )
    };
    let error = save("a.png", ExportFormat::Jpeg).unwrap_err();
    assert!(error.contains("must end in .jpg"), "{error}");
    assert!(!dir.path().join("a.png").exists());
    save("a.jpeg", ExportFormat::Jpeg).unwrap();
    let error = save("a.jpeg", ExportFormat::Jpeg).unwrap_err();
    assert!(error.contains("already exists"), "{error}");
}
