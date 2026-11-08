//! Positive zlib oracle. Libdeflate deliberately accepts some malformed trees;
//! strict negative assertions remain in the native/C conformance suites.
#[track_caller]
pub fn assert_zlib(input: &[u8], expected: &[u8]) {
    let mut output = vec![0; expected.len()];
    let written = libdeflater::Decompressor::new()
        .zlib_decompress(input, &mut output)
        .expect("libdeflate rejected a native-accepted stream");
    assert_eq!(written, expected.len(), "libdeflate output length");
    assert!(output == expected, "libdeflate sample mismatch");
}
