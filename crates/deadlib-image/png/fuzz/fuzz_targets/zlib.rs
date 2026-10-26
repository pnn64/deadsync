#![no_main]
use libfuzzer_sys::fuzz_target;
use std::io::Read;
fuzz_target!(|bytes: &[u8]| {
    if let Ok(actual) = deadlib_image_png::zlib::decompress(bytes, 1 << 20) {
        let mut reference = Vec::new();
        flate2::read::ZlibDecoder::new(bytes)
            .take((1 << 20) + 1)
            .read_to_end(&mut reference)
            .unwrap();
        assert_eq!(actual, reference);
    }
});
