//! Seam 3: decoding and folder reading on real fixture files.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use pixpixpix::decode::{self, DecodedImage};
use pixpixpix::{folder, heif, startup};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Decodes `path` with a token that is never set.
fn decode_file(path: &Path) -> Result<DecodedImage, String> {
    decode::decode_file(path, &AtomicBool::new(false)).unwrap_or_else(|| Err("cancelled".into()))
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

fn is_red(p: [u8; 4]) -> bool {
    p[0] > 200 && p[1] < 60 && p[2] < 60
}

fn is_blue(p: [u8; 4]) -> bool {
    p[0] < 60 && p[1] < 60 && p[2] > 200
}

#[test]
fn every_supported_format_decodes_to_red_left_blue_right() {
    for name in [
        "sample.png",
        "sample.jpg",
        "sample.gif",
        "sample.webp",
        "sample.bmp",
        "sample.tiff",
    ] {
        let image = decode_file(&fixture(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!((image.width, image.height), (4, 2), "{name}");
        assert!(
            is_red(pixel(&image, 0, 0)),
            "{name}: {:?}",
            pixel(&image, 0, 0)
        );
        assert!(
            is_blue(pixel(&image, 3, 1)),
            "{name}: {:?}",
            pixel(&image, 3, 1)
        );
    }
}

#[test]
fn exif_orientation_is_applied() {
    // Orientation 6 means "rotate 90° clockwise to display": the red left half
    // ends up on top.
    let image = decode_file(&fixture("orientation6.jpg")).unwrap();
    assert_eq!((image.width, image.height), (2, 4));
    assert!(is_red(pixel(&image, 0, 0)));
    assert!(is_blue(pixel(&image, 1, 3)));
}

#[test]
fn truncated_file_reports_the_decoder_error() {
    let error = decode_file(&fixture("broken.jpg")).unwrap_err();
    assert!(!error.is_empty());
}

#[test]
fn unsupported_content_reports_the_decoder_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.png");
    std::fs::write(&path, b"just some text, not an image").unwrap();
    let error = decode_file(&path).unwrap_err();
    assert!(!error.is_empty());
}

#[test]
fn cancelled_decode_reports_nothing() {
    let cancel = AtomicBool::new(true);
    assert_eq!(decode::decode_file(&fixture("sample.jpg"), &cancel), None);
}

#[test]
fn file_larger_than_one_read_chunk_decodes() {
    // A 400x400 PNG of noise is well over the 256 KiB read chunk.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.png");
    let noise = image::RgbaImage::from_fn(400, 400, |x, y| {
        let v = x
            .wrapping_mul(2_654_435_761)
            .wrapping_add(y.wrapping_mul(40_503));
        image::Rgba(v.to_le_bytes())
    });
    noise.save(&path).unwrap();
    assert!(std::fs::metadata(&path).unwrap().len() > 256 * 1024);
    let image = decode_file(&path).unwrap();
    assert_eq!((image.width, image.height), (400, 400));
    assert_eq!(pixel(&image, 7, 9), noise.get_pixel(7, 9).0);
}

#[test]
fn heic_heif_and_avif_decode_upright_or_name_the_missing_libheif() {
    match heif::load(heif::LIBRARY_NAMES) {
        Ok(_) => {
            for name in ["sample.heic", "sample.avif"] {
                let image = decode_file(&fixture(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
                assert_eq!((image.width, image.height), (4, 2), "{name}");
                assert!(is_red(pixel(&image, 0, 0)), "{name}");
                assert!(is_blue(pixel(&image, 3, 1)), "{name}");
            }
            // Turned 90° clockwise by its `irot` box and, like iPhone photos,
            // EXIF orientation 6 describing the same turn: applied once, the
            // red left half ends up on top.
            let image = decode_file(&fixture("orientation6.heic")).unwrap();
            assert_eq!((image.width, image.height), (2, 4));
            assert!(is_red(pixel(&image, 0, 0)));
            assert!(is_blue(pixel(&image, 1, 3)));
        }
        Err(_) => {
            let error = decode_file(&fixture("sample.heic")).unwrap_err();
            assert!(
                error.starts_with("HEIC/HEIF and AVIF need libheif, which was not found"),
                "{error}"
            );
        }
    }
}

#[test]
fn a_missing_libheif_is_an_error_naming_it() {
    let error = heif::load(&["libpixpixpix-no-such-library.so"])
        .err()
        .unwrap();
    assert!(
        error.starts_with("HEIC/HEIF and AVIF need libheif, which was not found: "),
        "{error}"
    );
}

#[test]
fn heif_is_told_by_content_with_the_extension_as_fallback() {
    let heic = std::fs::read(fixture("sample.heic")).unwrap();
    let avif = std::fs::read(fixture("sample.avif")).unwrap();
    let jpeg = std::fs::read(fixture("sample.jpg")).unwrap();
    assert!(decode::is_heif(Path::new("photo.jpg"), &heic));
    assert!(decode::is_heif(Path::new("photo"), &avif));
    assert!(decode::is_heif(Path::new("broken.HEIC"), b"not an image"));
    assert!(!decode::is_heif(Path::new("misnamed.heic"), &jpeg));
    assert!(!decode::is_heif(Path::new("photo.jpg"), &jpeg));
}

#[test]
fn folder_lists_media_in_natural_order_without_dot_files() {
    let dir = tempfile::tempdir().unwrap();
    for name in [
        "img10.jpg",
        "img2.JPG",
        "IMG1.png",
        ".hidden.jpg",
        "notes.txt",
        "phone.HEIC",
        "camera.hif",
        "still.heif",
        "web.avif",
    ] {
        std::fs::write(dir.path().join(name), b"x").unwrap();
    }
    std::fs::create_dir(dir.path().join("sub.jpg")).unwrap();
    let names = |dots| -> Vec<String> {
        folder::list_media(dir.path(), dots)
            .unwrap()
            .iter()
            .map(|p| folder::file_name(p))
            .collect()
    };
    let visible = [
        "camera.hif",
        "IMG1.png",
        "img2.JPG",
        "img10.jpg",
        "phone.HEIC",
        "still.heif",
        "web.avif",
    ];
    assert_eq!(names(false), visible);
    assert_eq!(names(true)[0], ".hidden.jpg");
    assert_eq!(names(true)[1..], visible);
}

#[test]
fn subfolders_are_listed_in_natural_order_without_dot_folders_or_files() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["day10", "Day2", "day1", ".cache"] {
        std::fs::create_dir(dir.path().join(name)).unwrap();
    }
    std::fs::write(dir.path().join("photo.jpg"), b"x").unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(elsewhere.path(), dir.path().join("linked")).unwrap();
    let names = |dots| -> Vec<String> {
        folder::list_dirs(dir.path(), dots)
            .unwrap()
            .iter()
            .map(|p| folder::file_name(p))
            .collect()
    };
    let expected: &[&str] = if cfg!(unix) {
        &["day1", "Day2", "day10", "linked"]
    } else {
        &["day1", "Day2", "day10"]
    };
    assert_eq!(names(false), expected);
    assert_eq!(names(true).first().map(String::as_str), Some(".cache"));
    assert_eq!(names(true).len(), expected.len() + 1);
}

/// Creates the folders and empty files of `entries` below `root`; names
/// ending in `/` are folders.
fn tree(root: &Path, entries: &[&str]) -> std::io::Result<()> {
    for entry in entries {
        let path = root.join(entry);
        if entry.ends_with('/') {
            std::fs::create_dir_all(path)?;
        } else {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, b"x")?;
        }
    }
    Ok(())
}

/// Searches below `root` with a token that is never set; the found folder
/// relative to `root` and its file names.
fn search(root: &Path, dots: bool) -> Option<(PathBuf, Vec<String>)> {
    let found = folder::search_media(root, dots, &AtomicBool::new(false)).flatten()?;
    let names = found.files.iter().map(|p| folder::file_name(p)).collect();
    Some((found.dir.strip_prefix(root).ok()?.to_path_buf(), names))
}

#[test]
fn search_finds_the_shallowest_folder_with_images() {
    let root = tempfile::tempdir().unwrap();
    tree(
        root.path(),
        &[
            "top.jpg",
            "a/deep/x.jpg",
            "b/notes.txt",
            "c/2.jpg",
            "c/10.jpg",
            "c/.hidden.jpg",
            "d/1.jpg",
        ],
    )
    .unwrap();
    assert_eq!(
        search(root.path(), false),
        Some((PathBuf::from("c"), vec!["2.jpg".into(), "10.jpg".into()]))
    );
}

#[test]
fn search_takes_folders_of_a_level_in_natural_order() {
    let root = tempfile::tempdir().unwrap();
    tree(root.path(), &["b10/x.jpg", "b2/y.jpg", "a/"]).unwrap();
    assert_eq!(
        search(root.path(), false),
        Some((PathBuf::from("b2"), vec!["y.jpg".into()]))
    );
}

#[test]
fn search_skips_dot_folders_unless_asked() {
    let root = tempfile::tempdir().unwrap();
    tree(root.path(), &[".cache/x.jpg", "a/b/y.jpg"]).unwrap();
    assert_eq!(
        search(root.path(), false).map(|(dir, _)| dir),
        Some(PathBuf::from("a/b"))
    );
    assert_eq!(
        search(root.path(), true).map(|(dir, _)| dir),
        Some(PathBuf::from(".cache"))
    );
}

#[cfg(unix)]
#[test]
fn search_does_not_follow_symlinked_folders() {
    let root = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    tree(elsewhere.path(), &["x.jpg"]).unwrap();
    tree(root.path(), &["a/b/y.jpg"]).unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), root.path().join("linked")).unwrap();
    assert_eq!(
        search(root.path(), false).map(|(dir, _)| dir),
        Some(PathBuf::from("a/b"))
    );
}

#[test]
fn search_without_images_below_finds_nothing() {
    let root = tempfile::tempdir().unwrap();
    tree(root.path(), &["top.jpg", "a/b/", "c/notes.txt"]).unwrap();
    assert_eq!(search(root.path(), false), None);
}

#[test]
fn cancelled_search_reports_nothing() {
    let root = tempfile::tempdir().unwrap();
    tree(root.path(), &["a/x.jpg"]).unwrap();
    assert_eq!(
        folder::search_media(root.path(), false, &AtomicBool::new(true)),
        None
    );
}

#[test]
fn start_with_a_file_uses_it_without_reading_the_folder() {
    let file = fixture("sample.png");
    assert_eq!(startup::resolve(Some(&file), Path::new("/")), Ok(file));
}

#[test]
fn start_with_a_folder_or_nothing_uses_its_first_image() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["b10.png", "b9.png"] {
        std::fs::write(dir.path().join(name), b"x").unwrap();
    }
    let first = dir.path().join("b9.png");
    assert_eq!(
        startup::resolve(Some(dir.path()), Path::new("/")),
        Ok(first.clone())
    );
    assert_eq!(startup::resolve(None, dir.path()), Ok(first.clone()));
    assert_eq!(
        startup::resolve(Some(Path::new(".")), dir.path()),
        Ok(dir.path().join(".").join("b9.png"))
    );
}

#[test]
fn start_with_an_empty_folder_names_it_and_explains_why() {
    let dir = tempfile::tempdir().unwrap();
    let none = startup::resolve(Some(dir.path()), Path::new("/")).unwrap_err();
    assert_eq!(none.dir, dir.path());
    assert!(none.message.starts_with("No images in"), "{}", none.message);
}

/// Frames of 4x2 pixels in one color each, lasting their delay in ms.
const FRAMES: [([u8; 4], u32); 3] = [
    ([255, 0, 0, 255], 100),
    ([0, 0, 255, 255], 10),
    ([0, 255, 0, 255], 250),
];

/// An animated GIF of [`FRAMES`].
fn animated_gif() -> image::ImageResult<Vec<u8>> {
    use image::codecs::gif::GifEncoder;
    let mut bytes = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut bytes);
        for (color, delay) in FRAMES {
            let buffer = image::RgbaImage::from_pixel(4, 2, image::Rgba(color));
            let delay = image::Delay::from_numer_denom_ms(delay, 1);
            encoder.encode_frame(image::Frame::from_parts(buffer, 0, 0, delay))?;
        }
    }
    Ok(bytes)
}

/// An animated WebP of [`FRAMES`], put together by hand: the `image` crate
/// only writes still WebP, so each frame is a still's lossless bitstream.
fn animated_webp() -> image::ImageResult<Vec<u8>> {
    use image::codecs::webp::WebPEncoder;
    fn chunk(name: &[u8], data: &[u8]) -> Vec<u8> {
        let mut out = name.to_vec();
        out.extend((data.len() as u32).to_le_bytes());
        out.extend(data);
        if data.len() % 2 == 1 {
            out.push(0);
        }
        out
    }
    fn u24(value: u32) -> [u8; 3] {
        let [a, b, c, _] = value.to_le_bytes();
        [a, b, c]
    }
    let mut vp8x = vec![0x02 | 0x10, 0, 0, 0];
    vp8x.extend(u24(3));
    vp8x.extend(u24(1));
    let mut body = b"WEBP".to_vec();
    body.extend(chunk(b"VP8X", &vp8x));
    body.extend(chunk(b"ANIM", &[0, 0, 0, 0, 0, 0]));
    for (color, delay) in FRAMES {
        let buffer = image::RgbaImage::from_pixel(4, 2, image::Rgba(color));
        let mut still = Vec::new();
        WebPEncoder::new_lossless(&mut still).encode(
            &buffer,
            4,
            2,
            image::ExtendedColorType::Rgba8,
        )?;
        // The still's only chunk, VP8L, follows the 12 byte RIFF header.
        let mut frame = Vec::new();
        frame.extend(u24(0));
        frame.extend(u24(0));
        frame.extend(u24(3));
        frame.extend(u24(1));
        frame.extend(u24(delay));
        frame.push(0);
        frame.extend(still.get(12..).unwrap_or_default());
        body.extend(chunk(b"ANMF", &frame));
    }
    let mut bytes = b"RIFF".to_vec();
    bytes.extend((body.len() as u32).to_le_bytes());
    bytes.extend(body);
    Ok(bytes)
}

/// The animated GIF and WebP of [`FRAMES`], written to `dir`.
fn animations(dir: &Path) -> Vec<PathBuf> {
    let gif = dir.join("moving.gif");
    let webp = dir.join("moving.webp");
    for (path, bytes) in [(&gif, animated_gif()), (&webp, animated_webp())] {
        let written = bytes.map_err(std::io::Error::other);
        assert!(written.and_then(|b| std::fs::write(path, b)).is_ok());
    }
    vec![gif, webp]
}

fn decode_first_frame(path: &Path) -> Option<Result<decode::FirstFrame, String>> {
    decode::decode_first_frame(path, &AtomicBool::new(false))
}

#[test]
fn animated_gif_and_webp_give_their_first_frame_and_say_they_are_animated() {
    let dir = tempfile::tempdir().unwrap();
    for path in animations(dir.path()) {
        let first = decode_first_frame(&path).unwrap().unwrap();
        assert!(first.animated, "{}", path.display());
        assert_eq!((first.image.width, first.image.height), (4, 2));
        assert!(is_red(pixel(&first.image, 3, 1)), "{}", path.display());
        assert_eq!(decode_file(&path).unwrap(), first.image);
    }
}

#[test]
fn still_images_are_not_animated() {
    for name in ["sample.gif", "sample.webp", "sample.png", "sample.jpg"] {
        let first = decode_first_frame(&fixture(name)).unwrap().unwrap();
        assert!(!first.animated, "{name}");
    }
}

#[test]
fn every_frame_of_an_animation_decodes_at_full_size_with_its_delay() {
    let dir = tempfile::tempdir().unwrap();
    for path in animations(dir.path()) {
        let frames = decode::decode_frames(&path, &AtomicBool::new(false))
            .unwrap()
            .unwrap();
        let name = path.display();
        assert_eq!(frames.len(), 3, "{name}");
        let delays: Vec<u128> = frames.iter().map(|f| f.delay.as_millis()).collect();
        assert_eq!(delays, [100, 10, 250], "{name}");
        for frame in &frames {
            assert_eq!((frame.image.width, frame.image.height), (4, 2), "{name}");
        }
        let colors: Vec<[u8; 4]> = frames.iter().map(|f| pixel(&f.image, 2, 1)).collect();
        assert!(is_red(colors[0]), "{name}");
        assert!(is_blue(colors[1]), "{name}");
        let [r, g, b, _] = colors[2];
        assert!(r < 60 && g > 200 && b < 60, "{name}");
    }
}

#[test]
fn a_cancelled_frames_decode_reports_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = animations(dir.path()).remove(0);
    assert_eq!(decode::decode_frames(&path, &AtomicBool::new(true)), None);
}

#[test]
fn frames_of_a_still_image_are_an_error() {
    let result = decode::decode_frames(&fixture("sample.png"), &AtomicBool::new(false)).unwrap();
    assert!(result.is_err());
}

/// Decodes image `index` of `path` with a token that is never set.
fn decode_inner(path: &Path, index: usize) -> Result<DecodedImage, String> {
    decode::decode_inner(path, index, &AtomicBool::new(false))
        .unwrap_or_else(|| Err("cancelled".into()))
}

fn is_green(p: [u8; 4]) -> bool {
    p[0] < 60 && p[1] > 200 && p[2] < 60
}

/// The multi-image fixtures, each a red/blue image and a green one; the
/// HEIC only with libheif.
fn multi_image_fixtures() -> Vec<&'static str> {
    let mut names = vec!["pages.tiff", "stereo.mpo"];
    if heif::load(heif::LIBRARY_NAMES).is_ok() {
        names.push("images.heic");
    }
    names
}

#[test]
fn multi_image_files_count_their_whole_images() {
    // pages.tiff has a third, reduced-resolution page, which does not count.
    for name in multi_image_fixtures() {
        let first = decode_first_frame(&fixture(name)).unwrap().unwrap();
        assert_eq!(first.images, 2, "{name}");
        assert!(!first.animated, "{name}");
        assert!(is_red(pixel(&first.image, 0, 0)), "{name}");
    }
}

#[test]
fn single_image_files_count_one() {
    // gainmap.jpg's MPF index lists a second JPEG of undefined type, as Ultra
    // HDR photos list their gain map: not a picture of its own.
    for name in [
        "sample.png",
        "sample.jpg",
        "sample.gif",
        "sample.webp",
        "sample.bmp",
        "sample.tiff",
        "orientation6.jpg",
        "gainmap.jpg",
    ] {
        let first = decode_first_frame(&fixture(name)).unwrap().unwrap();
        assert_eq!(first.images, 1, "{name}");
    }
}

#[test]
fn inner_images_decode_by_index() {
    for name in multi_image_fixtures() {
        let path = fixture(name);
        assert_eq!(decode_inner(&path, 0), decode_file(&path), "{name}");
        let second = decode_inner(&path, 1).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!((second.width, second.height), (4, 2), "{name}");
        assert!(is_green(pixel(&second, 3, 1)), "{name}");
        assert_eq!(
            decode_inner(&path, 2),
            Err("the file has no image 3".to_owned()),
            "{name}"
        );
    }
}

#[test]
fn a_single_image_file_has_no_second_image() {
    let error = decode_inner(&fixture("sample.png"), 1).unwrap_err();
    assert_eq!(error, "the file has no image 2");
}

#[test]
fn a_cancelled_inner_decode_reports_nothing() {
    let path = fixture("pages.tiff");
    assert_eq!(decode::decode_inner(&path, 1, &AtomicBool::new(true)), None);
}

#[test]
fn mpo_files_are_listed() {
    assert!(folder::is_image(Path::new("stereo.MPO")));
}
