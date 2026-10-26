//! Seam 3: Live Photo pairing in a real folder, and finding and copying the
//! video of Motion Photos built from a real JPEG.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use pixpixpix::{decode, folder, motion};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// The start of an MP4: an `ftyp` box, then some bytes.
fn mp4() -> Vec<u8> {
    let mut bytes = vec![0, 0, 0, 16];
    bytes.extend(b"ftypisom\0\0\0\0");
    bytes.extend((0..200u8).collect::<Vec<_>>());
    bytes
}

/// A JPEG segment with `marker` holding `data`.
fn segment(marker: u8, data: &[u8]) -> Vec<u8> {
    let len = (data.len() + 2) as u16;
    let mut bytes = vec![0xFF, marker];
    bytes.extend(len.to_be_bytes());
    bytes.extend(data);
    bytes
}

/// `sample.jpg` with an APP2 segment of `filler` bytes, an XMP segment
/// holding `xmp` and then `tail` appended.
fn jpeg_with(filler: usize, xmp: &str, tail: &[u8]) -> std::io::Result<Vec<u8>> {
    let jpeg = fs::read(fixture("sample.jpg"))?;
    let mut bytes = jpeg[..2].to_vec();
    if filler > 0 {
        bytes.extend(segment(0xE2, &vec![0; filler]));
    }
    let mut packet = b"http://ns.adobe.com/xap/1.0/\0".to_vec();
    packet.extend(xmp.as_bytes());
    bytes.extend(segment(0xE1, &packet));
    bytes.extend(&jpeg[2..]);
    bytes.extend(tail);
    Ok(bytes)
}

fn write(dir: &Path, name: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
    let path = dir.join(name);
    fs::write(&path, bytes)?;
    Ok(path)
}

/// Google's current format: a container directory naming the video's length.
fn container_xmp(length: usize) -> String {
    format!(
        r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF><rdf:Description GCamera:MotionPhoto="1">
<Container:Directory><rdf:Seq>
<rdf:li rdf:parseType="Resource"><Container:Item Item:Mime="image/jpeg" Item:Semantic="Primary" Item:Length="0" Item:Padding="0"/></rdf:li>
<rdf:li rdf:parseType="Resource"><Container:Item Item:Mime="video/mp4" Item:Semantic="MotionPhoto" Item:Length="{length}"/></rdf:li>
</rdf:Seq></Container:Directory></rdf:Description></rdf:RDF></x:xmpmeta>"#
    )
}

#[test]
fn a_google_motion_photo_is_found_through_its_container_directory() {
    let dir = tempfile::tempdir().unwrap();
    let video = mp4();
    let bytes = jpeg_with(0, &container_xmp(video.len()), &video).unwrap();
    let path = write(dir.path(), "PXL_1.MP.jpg", &bytes).unwrap();
    let len = bytes.len() as u64;
    assert_eq!(
        motion::find(&path).unwrap(),
        Some(len - video.len() as u64..len)
    );
}

#[test]
fn an_older_google_motion_photo_is_found_through_its_micro_video_offset() {
    let dir = tempfile::tempdir().unwrap();
    let video = mp4();
    let xmp = format!(
        r#"<rdf:Description GCamera:MicroVideo="1" GCamera:MicroVideoVersion="1" GCamera:MicroVideoOffset="{}"/>"#,
        video.len()
    );
    // The XMP lies beyond the first 64 KiB.
    let bytes = jpeg_with(65_000, &xmp, &video).unwrap();
    let path = write(dir.path(), "MVIMG_1.jpg", &bytes).unwrap();
    let len = bytes.len() as u64;
    assert_eq!(
        motion::find(&path).unwrap(),
        Some(len - video.len() as u64..len)
    );
}

#[test]
fn a_samsung_motion_photo_is_found_through_its_trailer() {
    let dir = tempfile::tempdir().unwrap();
    let video = mp4();
    let mut bytes = fs::read(fixture("sample.jpg")).unwrap();
    let block_start = bytes.len();
    let mut block = vec![0, 0, 0x30, 0x0A];
    block.extend(16u32.to_le_bytes());
    block.extend(b"MotionPhoto_Data");
    block.extend(&video);
    bytes.extend(&block);
    let mut directory = b"SEFH".to_vec();
    directory.extend(107u32.to_le_bytes());
    directory.extend(1u32.to_le_bytes());
    directory.extend([0, 0, 0x30, 0x0A]);
    directory.extend((block.len() as u32).to_le_bytes());
    directory.extend((block.len() as u32).to_le_bytes());
    bytes.extend(&directory);
    bytes.extend((directory.len() as u32).to_le_bytes());
    bytes.extend(b"SEFT");
    let path = write(dir.path(), "20240101_120000.jpg", &bytes).unwrap();
    let start = (block_start + 8 + 16) as u64;
    assert_eq!(
        motion::find(&path).unwrap(),
        Some(start..start + video.len() as u64)
    );
}

#[test]
fn a_plain_jpeg_is_no_motion_photo() {
    assert_eq!(motion::find(&fixture("sample.jpg")).unwrap(), None);
    assert_eq!(motion::find(&fixture("sample.png")).unwrap(), None);
}

#[test]
fn xmp_claiming_a_video_where_there_is_none_is_not_believed() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = jpeg_with(0, &container_xmp(100), &[7; 300]).unwrap();
    let path = write(dir.path(), "fake.jpg", &bytes).unwrap();
    assert_eq!(motion::find(&path).unwrap(), None);
}

#[test]
fn a_motion_photo_still_decodes_as_its_jpeg() {
    let dir = tempfile::tempdir().unwrap();
    let video = mp4();
    let path = write(
        dir.path(),
        "PXL_1.MP.jpg",
        &jpeg_with(0, &container_xmp(video.len()), &video).unwrap(),
    )
    .unwrap();
    let image = decode::decode_file(&path, &AtomicBool::new(false))
        .unwrap()
        .unwrap();
    assert_eq!((image.width, image.height), (4, 2));
}

#[test]
fn the_video_is_copied_whole_and_the_copy_removed_when_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let video = mp4();
    let path = write(
        dir.path(),
        "PXL_1.MP.jpg",
        &jpeg_with(0, &container_xmp(video.len()), &video).unwrap(),
    )
    .unwrap();
    let cache = dir.path().join("cache");
    let clip = motion::extract(&path, &cache).unwrap();
    assert_eq!(fs::read(clip.path()).unwrap(), video);
    assert_eq!(clip.path().extension().unwrap(), "mp4");
    let copy = clip.path().to_path_buf();
    drop(clip);
    assert!(!copy.exists());
    assert!(motion::extract(&fixture("sample.jpg"), &cache).is_err());
}

#[test]
fn live_photos_pair_photo_and_mov_of_the_same_name_ignoring_case() {
    let dir = tempfile::tempdir().unwrap();
    for name in [
        "IMG_1.HEIC",
        "IMG_1.MOV",
        "IMG_2.mov",
        "img_3.jpg",
        "IMG_3.mov",
        "IMG_4.png",
        "IMG_4.MOV",
    ] {
        write(dir.path(), name, b"").unwrap();
    }
    let files = folder::list_media(dir.path(), false).unwrap();
    let pairs: Vec<(String, String)> = folder::live_photos(&files)
        .iter()
        .map(|(photo, mov)| (folder::file_name(photo), folder::file_name(mov)))
        .collect();
    assert_eq!(
        pairs,
        [
            ("IMG_1.HEIC".to_owned(), "IMG_1.MOV".to_owned()),
            ("img_3.jpg".to_owned(), "IMG_3.mov".to_owned()),
        ]
    );
    let shown: Vec<String> = folder::without_live_videos(files)
        .iter()
        .map(|f| folder::file_name(f))
        .collect();
    assert_eq!(
        shown,
        [
            "IMG_1.HEIC",
            "IMG_2.mov",
            "img_3.jpg",
            "IMG_4.MOV",
            "IMG_4.png"
        ]
    );
}
