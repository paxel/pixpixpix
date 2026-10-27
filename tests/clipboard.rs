//! Seam 3: the clipboard adapter's content: turned pixels, PNG and file
//! references. Owning the system clipboard needs a display and is not tested
//! here.

use std::path::Path;
use std::sync::Arc;

use pixpixpix::clipboard::{self, Content};
use pixpixpix::decode::DecodedImage;
use pixpixpix::rotate::Rotation;

/// A 3x2 image whose pixels are numbered 0 to 5 row by row in their red
/// channel:
///
/// ```text
/// 0 1 2
/// 3 4 5
/// ```
fn numbered() -> DecodedImage {
    DecodedImage {
        width: 3,
        height: 2,
        rgba: (0..6u8)
            .flat_map(|n| [n, 0, 0, 255])
            .collect::<Vec<_>>()
            .into(),
    }
}

/// The red channel of every pixel, row by row.
fn reds(image: &DecodedImage) -> Vec<u8> {
    image.rgba.chunks(4).map(|px| px[0]).collect()
}

#[test]
fn no_rotation_keeps_the_pixels() {
    let image = clipboard::turned(&numbered(), Rotation::None).unwrap();
    assert_eq!((image.width, image.height), (3, 2));
    assert_eq!(reds(&image), [0, 1, 2, 3, 4, 5]);
}

#[test]
fn right_turns_a_quarter_clockwise() {
    let image = clipboard::turned(&numbered(), Rotation::Right).unwrap();
    assert_eq!((image.width, image.height), (2, 3));
    assert_eq!(reds(&image), [3, 0, 4, 1, 5, 2]);
}

#[test]
fn half_turns_upside_down() {
    let image = clipboard::turned(&numbered(), Rotation::Half).unwrap();
    assert_eq!((image.width, image.height), (3, 2));
    assert_eq!(reds(&image), [5, 4, 3, 2, 1, 0]);
}

#[test]
fn left_turns_a_quarter_counterclockwise() {
    let image = clipboard::turned(&numbered(), Rotation::Left).unwrap();
    assert_eq!((image.width, image.height), (2, 3));
    assert_eq!(reds(&image), [2, 5, 1, 4, 0, 3]);
}

#[test]
fn pixels_that_do_not_fit_the_size_are_an_error() {
    let image = DecodedImage {
        width: 3,
        height: 2,
        rgba: Arc::from(vec![0; 4]),
    };
    assert!(clipboard::turned(&image, Rotation::Right).is_err());
}

#[test]
fn file_content_holds_only_the_file() {
    let Content {
        png,
        uri_list,
        gnome_files,
        text,
    } = clipboard::file_content(Path::new("/p/cat.jpg")).unwrap();
    assert!(png.is_empty());
    assert_eq!(uri_list, "file:///p/cat.jpg\r\n");
    assert_eq!(gnome_files, "copy\nfile:///p/cat.jpg");
    assert_eq!(text, "");
}

#[test]
fn file_uris_are_percent_encoded() {
    assert_eq!(
        clipboard::file_uri(Path::new("/p/Sommer 2024/Käse #1.jpg")).unwrap(),
        "file:///p/Sommer%202024/K%C3%A4se%20%231.jpg"
    );
}

#[test]
fn relative_paths_become_absolute_uris() {
    let cwd = std::env::current_dir().unwrap();
    assert_eq!(
        clipboard::file_uri(Path::new("cat.jpg")).unwrap(),
        clipboard::file_uri(&cwd.join("cat.jpg")).unwrap()
    );
}
