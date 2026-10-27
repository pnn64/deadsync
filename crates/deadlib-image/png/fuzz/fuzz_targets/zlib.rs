#![no_main]
use libfuzzer_sys::fuzz_target;
#[path = "../support/reference.rs"]
mod reference;
fuzz_target!(|bytes: &[u8]| {
    if bytes.len() > 1 << 18 {
        return;
    }
    if let Ok(actual) = deadlib_image_png::zlib::decompress(bytes, 1 << 20) {
        reference::assert_zlib(bytes, &actual);
    }
});
