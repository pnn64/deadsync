#![no_main]
use libfuzzer_sys::fuzz_target;
#[path = "../support/reference.rs"]
mod reference;
fuzz_target!(|bytes: &[u8]| {
    if bytes.len() > 1 << 18 {
        return;
    }
    if let Ok((actual, used)) = deadlib_image_png::zlib::decompress_prefix(bytes, 1 << 20) {
        assert!(used <= bytes.len());
        reference::assert_zlib(&bytes[..used], &actual);
        assert_eq!(
            deadlib_image_png::zlib::decompress(&bytes[..used], 1 << 20).unwrap(),
            actual
        );
        if used < bytes.len() {
            assert_eq!(
                deadlib_image_png::zlib::decompress(bytes, 1 << 20),
                Err(deadlib_image_png::Error::Invalid("trailing zlib data"))
            );
        }
        let mut suffixed = bytes[..used].to_vec();
        suffixed.extend_from_slice(b"unused zlib suffix");
        assert_eq!(
            deadlib_image_png::zlib::decompress_prefix(&suffixed, actual.len()).unwrap(),
            (actual, used)
        );
    }
});
