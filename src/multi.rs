//! Multi-image files the `image` crate reads one picture of: multi-page TIFF
//! and MPO, a JPEG with more pictures behind the first that its MPF index
//! lists. Only whole pictures count: reduced-resolution TIFF pages, MPF
//! thumbnails and the gain maps of Ultra HDR photos, stored the same way, do
//! not. Multi-image HEIC/HEIF is libheif's. Finding the pictures reads only
//! the file's headers.

use std::ops::Range;

/// The most TIFF pages looked at.
const MAX_PAGES: usize = 4096;

/// TIFF tag `NewSubfileType`; its bit 0 marks a reduced-resolution page.
const NEW_SUBFILE_TYPE: u16 = 254;
/// TIFF field type `SHORT`.
const SHORT: u16 = 3;
/// MPF tag `MPEntry`: 16 bytes per picture.
const MP_ENTRY: u16 = 0xB002;
/// MP type classes of whole pictures: multi-frame (panorama, stereo,
/// multi-angle) and baseline primary.
const MP_PICTURE_CLASSES: [u32; 2] = [0x02, 0x03];

/// The pictures of a file, the first being the one the decoder reads anyway.
enum Pictures {
    /// The IFD offsets of the pages.
    Tiff {
        little: bool,
        pages: Vec<u32>,
    },
    /// Where the JPEG of each picture after the first lies.
    Mpo(Vec<Range<usize>>),
    One,
}

impl Pictures {
    fn count(&self) -> usize {
        match self {
            Self::Tiff { pages, .. } => pages.len().max(1),
            Self::Mpo(more) => more.len().saturating_add(1),
            Self::One => 1,
        }
    }
}

/// How many pictures `bytes` hold: more than one for a multi-page TIFF or an
/// MPO.
pub fn count(bytes: &[u8]) -> usize {
    pictures(bytes).count()
}

/// The file the decoder reads for picture `index` of `bytes`, counted from 0,
/// and how many pictures there are. Picture 0 is the file itself.
pub fn picture(mut bytes: Vec<u8>, index: usize) -> Result<(Vec<u8>, usize), String> {
    let pictures = pictures(&bytes);
    let count = pictures.count();
    if index == 0 {
        return Ok((bytes, count));
    }
    let missing = || format!("the file has no image {}", index.saturating_add(1));
    match pictures {
        // The header points at the page wanted as if it were the first.
        Pictures::Tiff { little, pages } => {
            let offset = pages.get(index).ok_or_else(missing)?;
            let raw = if little {
                offset.to_le_bytes()
            } else {
                offset.to_be_bytes()
            };
            bytes
                .get_mut(4..8)
                .ok_or_else(missing)?
                .copy_from_slice(&raw);
            Ok((bytes, count))
        }
        Pictures::Mpo(more) => {
            let range = index.checked_sub(1).and_then(|i| more.get(i));
            let jpeg = range
                .and_then(|r| bytes.get(r.clone()))
                .ok_or_else(missing)?;
            Ok((jpeg.to_vec(), count))
        }
        Pictures::One => Err(missing()),
    }
}

fn pictures(bytes: &[u8]) -> Pictures {
    if let Some((little, pages)) = tiff_pages(bytes) {
        return Pictures::Tiff { little, pages };
    }
    match mpo_pictures(bytes) {
        Some(more) => Pictures::Mpo(more),
        None => Pictures::One,
    }
}

/// The byte order of a TIFF header at `at`: true for little endian.
fn tiff_order(bytes: &[u8], at: usize) -> Option<bool> {
    match bytes.get(at..at.checked_add(4)?)? {
        b"II*\0" => Some(true),
        b"MM\0*" => Some(false),
        _ => None,
    }
}

/// The byte order and the IFD offsets of the whole pages of a classic TIFF;
/// the first page always counts. A damaged chain counts what was found.
fn tiff_pages(bytes: &[u8]) -> Option<(bool, Vec<u32>)> {
    let little = tiff_order(bytes, 0)?;
    let mut pages = Vec::new();
    let mut seen = Vec::new();
    let mut at = read_u32(bytes, 4, little)?;
    while at != 0 && seen.len() < MAX_PAGES && !seen.contains(&at) {
        seen.push(at);
        let Some((reduced, next)) = usize::try_from(at)
            .ok()
            .and_then(|start| tiff_ifd(bytes, start, little))
        else {
            break;
        };
        if pages.is_empty() || !reduced {
            pages.push(at);
        }
        at = next;
    }
    Some((little, pages))
}

/// Whether the IFD at `start` is a reduced-resolution page, and the offset of
/// the next one.
fn tiff_ifd(bytes: &[u8], start: usize, little: bool) -> Option<(bool, u32)> {
    let entries = usize::from(read_u16(bytes, start, little)?);
    let first = start.checked_add(2)?;
    let mut reduced = false;
    for i in 0..entries {
        let entry = first.checked_add(i.checked_mul(12)?)?;
        if read_u16(bytes, entry, little)? == NEW_SUBFILE_TYPE {
            let value = entry.checked_add(8)?;
            let kind = if read_u16(bytes, entry.checked_add(2)?, little)? == SHORT {
                u32::from(read_u16(bytes, value, little)?)
            } else {
                read_u32(bytes, value, little)?
            };
            reduced = kind & 1 == 1;
        }
    }
    let next = read_u32(bytes, first.checked_add(entries.checked_mul(12)?)?, little)?;
    Some((reduced, next))
}

/// Where the whole pictures after the first lie in a JPEG with an MPF index
/// before its image data.
fn mpo_pictures(bytes: &[u8]) -> Option<Vec<Range<usize>>> {
    if bytes.get(..2)? != [0xFF, 0xD8] {
        return None;
    }
    let mut at = 2usize;
    loop {
        if *bytes.get(at)? != 0xFF {
            return None;
        }
        match *bytes.get(at.checked_add(1)?)? {
            // Fill byte.
            0xFF => at = at.checked_add(1)?,
            // Image data or its end: no index before it.
            0xDA | 0xD9 => return None,
            marker => {
                let len = usize::from(read_u16(bytes, at.checked_add(2)?, false)?);
                let data = at.checked_add(4)?;
                if marker == 0xE2 && bytes.get(data..data.checked_add(4)?)? == b"MPF\0" {
                    return mpf_pictures(bytes, data.checked_add(4)?);
                }
                at = at.checked_add(2)?.checked_add(len)?;
            }
        }
    }
}

/// The whole pictures after the first that the MPF index with its TIFF-like
/// header at `header` lists; their offsets count from that header.
fn mpf_pictures(bytes: &[u8], header: usize) -> Option<Vec<Range<usize>>> {
    let little = tiff_order(bytes, header)?;
    let ifd = header
        .checked_add(usize::try_from(read_u32(bytes, header.checked_add(4)?, little)?).ok()?)?;
    let entries = usize::from(read_u16(bytes, ifd, little)?);
    let mut list = None;
    for i in 0..entries {
        let entry = ifd.checked_add(2)?.checked_add(i.checked_mul(12)?)?;
        if read_u16(bytes, entry, little)? == MP_ENTRY {
            let size = usize::try_from(read_u32(bytes, entry.checked_add(4)?, little)?).ok()?;
            let offset = usize::try_from(read_u32(bytes, entry.checked_add(8)?, little)?).ok()?;
            list = Some((header.checked_add(offset)?, size.checked_div(16)?));
        }
    }
    let (list, count) = list?;
    let mut more = Vec::new();
    for i in 1..count {
        let entry = list.checked_add(i.checked_mul(16)?)?;
        let attribute = read_u32(bytes, entry, little)?;
        let size = usize::try_from(read_u32(bytes, entry.checked_add(4)?, little)?).ok()?;
        let offset = usize::try_from(read_u32(bytes, entry.checked_add(8)?, little)?).ok()?;
        let start = header.checked_add(offset)?;
        let range = start..start.checked_add(size)?;
        let whole = MP_PICTURE_CLASSES.contains(&((attribute >> 16) & 0xFF));
        let jpeg = bytes
            .get(range.clone())
            .is_some_and(|b| b.starts_with(&[0xFF, 0xD8]));
        if whole && jpeg {
            more.push(range);
        }
    }
    Some(more)
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
