//! The video of a Motion Photo is found from the XMP and Samsung's trailer,
//! whose offsets and lengths come from the file: a range that is found lies
//! inside the file.
#![no_main]

use std::io::Write;

use libfuzzer_sys::fuzz_target;
use pixpixpix::motion;

fuzz_target!(|bytes: Vec<u8>| {
    let Ok(mut file) = tempfile::NamedTempFile::new() else {
        return;
    };
    if file.write_all(&bytes).and_then(|()| file.flush()).is_err() {
        return;
    }
    if let Ok(Some(range)) = motion::find(file.path()) {
        let len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        assert!(range.start <= range.end, "{range:?}");
        assert!(range.end <= len, "{range:?} in {len} bytes");
    }
});
