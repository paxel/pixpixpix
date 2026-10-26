//! Google and Samsung Motion Photos: a JPEG with a short MP4 appended. Where
//! the video lies is found from the XMP of the JPEG (Google's `MotionPhoto`
//! container directory or the older `MicroVideoOffset`) or from Samsung's
//! trailer, reading only the JPEG's headers and the file's end. A video is
//! believed only where an MP4 `ftyp` box starts.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::ops::Range;
use std::path::Path;

use tempfile::NamedTempFile;

/// The namespace an XMP segment starts with.
const XMP_HEADER: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
/// The most JPEG segments looked at before the image data.
const MAX_SEGMENTS: usize = 64;
/// The end of Samsung's trailer: its directory's length, then this.
const SEF_END: &[u8] = b"SEFT";
const SEF_HEADER: &[u8] = b"SEFH";
/// The name of Samsung's trailer block holding the video.
const SEF_VIDEO: &[u8] = b"MotionPhoto_Data";
/// The most entries of Samsung's trailer directory looked at.
const MAX_SEF_ENTRIES: u32 = 256;

/// Where the video of the Motion Photo at `path` lies, in bytes; `None` for
/// any other file.
pub fn find(path: &Path) -> io::Result<Option<Range<u64>>> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let mut candidates = Vec::new();
    if let Some(xmp) = xmp(&mut file)? {
        let xmp = String::from_utf8_lossy(&xmp);
        if let Some(length) = container_video_length(&xmp) {
            candidates.push(length);
        }
        if let Some(offset) = number_after(&xmp, "MicroVideoOffset") {
            candidates.push(offset);
        }
    }
    let mut found = candidates
        .into_iter()
        .filter_map(|length| Some(len.checked_sub(length)?..len))
        .collect::<Vec<_>>();
    if let Some(range) = samsung_video(&mut file, len)? {
        found.push(range);
    }
    for range in found {
        if starts_mp4(&mut file, &range)? {
            return Ok(Some(range));
        }
    }
    Ok(None)
}

/// Writes the video of the Motion Photo at `path` to a new file in `dir`,
/// removed when the returned handle is dropped.
pub fn extract(path: &Path, dir: &Path) -> Result<NamedTempFile, String> {
    let cannot = |e: io::Error| format!("Cannot read the video of the Motion Photo: {e}");
    let range = find(path)
        .map_err(cannot)?
        .ok_or("The file holds no Motion Photo video")?;
    let mut file = File::open(path).map_err(cannot)?;
    file.seek(SeekFrom::Start(range.start)).map_err(cannot)?;
    std::fs::create_dir_all(dir).map_err(cannot)?;
    let mut clip = tempfile::Builder::new()
        .prefix("motion-")
        .suffix(".mp4")
        .tempfile_in(dir)
        .map_err(cannot)?;
    let length = range.end.saturating_sub(range.start);
    io::copy(&mut file.take(length), &mut clip).map_err(cannot)?;
    clip.flush().map_err(cannot)?;
    Ok(clip)
}

/// The XMP packet of a JPEG, found before its image data.
fn xmp(file: &mut File) -> io::Result<Option<Vec<u8>>> {
    let mut start = [0u8; 2];
    file.seek(SeekFrom::Start(0))?;
    if file.read_exact(&mut start).is_err() || start != [0xFF, 0xD8] {
        return Ok(None);
    }
    for _ in 0..MAX_SEGMENTS {
        let mut header = [0u8; 4];
        if file.read_exact(&mut header).is_err() {
            return Ok(None);
        }
        let [0xFF, marker, high, low] = header else {
            return Ok(None);
        };
        // Image data or its end: no XMP before it.
        if matches!(marker, 0xDA | 0xD9) {
            return Ok(None);
        }
        let len = u64::from(u16::from_be_bytes([high, low])).saturating_sub(2);
        if marker == 0xE1 {
            let mut data = Vec::new();
            Read::take(&mut *file, len).read_to_end(&mut data)?;
            if let Some(packet) = data.strip_prefix(XMP_HEADER) {
                return Ok(Some(packet.to_vec()));
            }
        } else {
            file.seek(SeekFrom::Current(i64::try_from(len).unwrap_or(i64::MAX)))?;
        }
    }
    Ok(None)
}

/// The length of the item of Google's container directory whose semantic is
/// `MotionPhoto`: the video at the end of the file.
fn container_video_length(xmp: &str) -> Option<u64> {
    let at = xmp.find("Semantic=\"MotionPhoto\"")?;
    let (before, after) = xmp.split_at(at);
    let open = before.rfind('<')?;
    let close = after.find('>')?;
    let tag = xmp.get(open..at.checked_add(close)?)?;
    number_after(tag, "Item:Length")
}

/// The number written as the value of `name`, as an attribute (`name="12"`)
/// or an element (`<ns:name>12</ns:name>`).
fn number_after(text: &str, name: &str) -> Option<u64> {
    let at = text.find(name)?.checked_add(name.len())?;
    let rest = text.get(at..)?;
    let value = rest.trim_start_matches(|c: char| c.is_whitespace() || "=\"'>".contains(c));
    let digits = value
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(value.len());
    value.get(..digits)?.parse().ok()
}

/// Where the video of Samsung's trailer lies: its directory at the end of the
/// file points back at a block named `MotionPhoto_Data` that holds it.
fn samsung_video(file: &mut File, len: u64) -> io::Result<Option<Range<u64>>> {
    let Some(end) = read_at(file, len.checked_sub(8), 8)? else {
        return Ok(None);
    };
    let (dir_len, magic) = end.split_at(4);
    if magic != SEF_END {
        return Ok(None);
    }
    let dir_start = len
        .checked_sub(8)
        .and_then(|e| e.checked_sub(u64::from(u32_le(dir_len)?)));
    let Some(dir) = read_at(file, dir_start, 12)? else {
        return Ok(None);
    };
    let (Some(dir_start), Some(SEF_HEADER), Some(count)) =
        (dir_start, dir.get(..4), dir.get(8..12).and_then(u32_le))
    else {
        return Ok(None);
    };
    for index in 0..count.min(MAX_SEF_ENTRIES) {
        let at = u64::from(index)
            .checked_mul(12)
            .and_then(|i| i.checked_add(dir_start)?.checked_add(12));
        let Some(entry) = read_at(file, at, 12)? else {
            return Ok(None);
        };
        let (Some(back), Some(size)) = (
            entry.get(4..8).and_then(u32_le),
            entry.get(8..12).and_then(u32_le),
        ) else {
            return Ok(None);
        };
        let block = dir_start.checked_sub(u64::from(back));
        let Some(head) = read_at(file, block, 8)? else {
            continue;
        };
        let Some(name_len) = head.get(4..8).and_then(u32_le) else {
            continue;
        };
        if usize::try_from(name_len).ok() != Some(SEF_VIDEO.len()) {
            continue;
        }
        let name_at = block.and_then(|b| b.checked_add(8));
        if read_at(file, name_at, SEF_VIDEO.len())?.as_deref() != Some(SEF_VIDEO) {
            continue;
        }
        let start = name_at.and_then(|n| n.checked_add(u64::from(name_len)));
        let end = block.and_then(|b| b.checked_add(u64::from(size)));
        if let (Some(start), Some(end)) = (start, end)
            && start < end
            && end <= len
        {
            return Ok(Some(start..end));
        }
    }
    Ok(None)
}

/// Whether an MP4 `ftyp` box starts the range.
fn starts_mp4(file: &mut File, range: &Range<u64>) -> io::Result<bool> {
    let head = read_at(file, Some(range.start), 8)?;
    Ok(head.is_some_and(|h| h.get(4..8) == Some(b"ftyp".as_slice())))
}

/// `len` bytes at `at`; `None` when they are not all in the file.
fn read_at(file: &mut File, at: Option<u64>, len: usize) -> io::Result<Option<Vec<u8>>> {
    let Some(at) = at else {
        return Ok(None);
    };
    file.seek(SeekFrom::Start(at))?;
    let mut bytes = vec![0u8; len];
    match file.read_exact(&mut bytes) {
        Ok(()) => Ok(Some(bytes)),
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(None),
        Err(e) => Err(e),
    }
}

fn u32_le(bytes: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}
