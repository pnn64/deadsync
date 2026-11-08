use opusic_sys::{opus_get_version_string, opus_decoder_create, opus_decoder_destroy};

use core::ptr;
use core::ffi::CStr;

#[test]
fn check_version() {
    let version = unsafe {
        CStr::from_ptr(opus_get_version_string())
    };
    let version = version.to_str().expect("utf-8 string");
    assert_eq!("libopus 1.6.1", version);
}

#[test]
fn check_decoder_symbols() {
    let result = unsafe {
        opus_decoder_create(8000, 2, ptr::null_mut())
    };
    assert!(!result.is_null());
    unsafe {
        opus_decoder_destroy(result);
    }
}
