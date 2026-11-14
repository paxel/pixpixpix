//! A multi-page TIFF or an MPO is split into its pictures by offsets read from
//! the file itself: any file has at least one picture, picture 0 is the file
//! unchanged, and every other picture either comes out with the same count or
//! is refused, never a panic.
#![no_main]

use libfuzzer_sys::fuzz_target;
use pixpixpix::multi;

fuzz_target!(|input: (u8, Vec<u8>)| {
    let (wanted, bytes) = input;
    let count = multi::count(&bytes);
    assert!(count >= 1);

    let first = multi::picture(bytes.clone(), 0);
    assert_eq!(first, Ok((bytes.clone(), count)));

    let index = usize::from(wanted) % count.saturating_add(1);
    if let Ok((_, again)) = multi::picture(bytes, index) {
        assert_eq!(again, count);
    }
});
