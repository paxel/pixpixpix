//! Seam 3: the optional libraries are looked for by a helper process, never
//! loaded into the app for it.

use pixpixpix::libraries;

#[test]
fn every_library_is_found_with_its_place_or_missing_with_a_hint() {
    let found = libraries::probe();
    let names: Vec<&str> = found.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["libmpv", "libheif", "libturbojpeg"]);
    for library in &found {
        assert!(!library.needed_for.is_empty());
        match &library.found {
            Some(place) => assert!(!place.is_empty()),
            None => assert!(!library.hint.is_empty(), "{}", library.name),
        }
    }
    #[cfg(target_os = "linux")]
    {
        let maps = std::fs::read_to_string("/proc/self/maps").unwrap();
        assert!(!maps.contains("libmpv.so"));
        if let Some(Some(place)) = found.first().map(|l| &l.found) {
            assert!(place.contains("libmpv"), "{place}");
        }
    }
}
