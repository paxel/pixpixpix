//! Resetting the EXIF orientation walks the JPEG segments and the TIFF entries
//! inside the EXIF block, writing in place: the length never changes, and a
//! second reset changes nothing more.
#![no_main]

use libfuzzer_sys::fuzz_target;
use pixpixpix::rotate;

fuzz_target!(|bytes: Vec<u8>| {
    let mut once = bytes.clone();
    rotate::reset_orientation(&mut once);
    assert_eq!(once.len(), bytes.len());

    let mut twice = once.clone();
    rotate::reset_orientation(&mut twice);
    assert_eq!(twice, once);
});
