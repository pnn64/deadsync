#![no_main]
use deadlib_image_png::zlib::{Inflater, StreamStatus, decompress_prefix};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|bytes: &[u8]| {
    let limit = 1 << 16;
    let expected = decompress_prefix(bytes, limit);
    let mut inflater = Inflater::new(limit).unwrap();
    let mut offset = 0;
    let mut actual = Vec::new();
    let mut output = [0; 257];
    let input_chunk = 1 + usize::from(bytes.first().copied().unwrap_or(0) % 31);
    let output_chunk = 1 + usize::from(bytes.last().copied().unwrap_or(0));
    loop {
        let end = bytes.len().min(offset + input_chunk);
        let progress = match inflater.process(&bytes[offset..end], &mut output[..output_chunk]) {
            Ok(p) => p,
            Err(e) => {
                assert!(expected.is_err(), "stream rejected valid zlib: {e}");
                assert_eq!(inflater.process(&[], &mut []), Err(e));
                return;
            }
        };
        offset += progress.consumed;
        actual.extend_from_slice(&output[..progress.written]);
        match progress.status {
            StreamStatus::Done => {
                assert_eq!(expected.unwrap(), (actual, offset));
                inflater.finish().unwrap();
                return;
            }
            StreamStatus::NeedInput if offset == bytes.len() => {
                assert!(expected.is_err());
                assert!(inflater.finish().is_err());
                return;
            }
            StreamStatus::NeedInput => assert_eq!(offset, end),
            StreamStatus::NeedOutput => (),
        }
    }
});
