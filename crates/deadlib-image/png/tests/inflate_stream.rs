mod support;
use deadlib_image_png::{
    Error,
    zlib::{self, Compression, Inflater, StreamStatus},
};
use std::io::Write;

fn reference(input: &[u8], level: u32) -> Vec<u8> {
    let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(level));
    enc.write_all(input).unwrap();
    enc.finish().unwrap()
}

#[test]
fn every_input_output_fragment_combination() {
    let mut random = 0x109b_4131u32;
    let cases = [
        Vec::new(),
        vec![1],
        b"abcabcabcabcabcabcabcabc".to_vec(),
        vec![255; 1024],
        (0..4097)
            .map(|_| {
                random ^= random << 13;
                random ^= random >> 17;
                random ^= random << 5;
                random as u8
            })
            .collect(),
    ];
    for bytes in cases {
        let encoded = [
            reference(&bytes, 0),
            reference(&bytes, 1),
            reference(&bytes, 6),
            reference(&bytes, 9),
            zlib::compress(&bytes, Compression::Stored).unwrap(),
            zlib::compress(&bytes, Compression::Fast).unwrap(),
        ];
        for compressed in encoded {
            for input_chunk in [1, 2, 3, 7, 31, 1024] {
                for output_chunk in [1, 2, 3, 7, 258, 4096] {
                    let (actual, used) =
                        support::inflate(&compressed, input_chunk, output_chunk, bytes.len())
                            .unwrap();
                    assert_eq!(actual, bytes);
                    assert_eq!(used, compressed.len());
                }
            }
        }
    }
}

#[test]
fn ring_wrap_history_and_long_matches() {
    let mut raw: Vec<u8> = (0..32768).map(|n| ((n * 73) ^ (n / 13)) as u8).collect();
    raw.extend_from_within(..32768);
    raw.extend_from_within(..32768);
    raw.extend_from_slice(&[7; 65536]);
    for compressed in [
        reference(&raw, 9),
        zlib::compress(&raw, Compression::Fast).unwrap(),
        zlib::compress(&raw, Compression::Stored).unwrap(),
    ] {
        for (input, output) in [(1, 1), (1, 4096), (4096, 1), (7, 31), (32768, 65536)] {
            let (actual, used) = support::inflate(&compressed, input, output, raw.len()).unwrap();
            assert_eq!(actual, raw);
            assert_eq!(used, compressed.len());
        }
    }
}

#[test]
fn exact_small_output_limits_and_power_of_two_boundaries() {
    let mut random = 0x109b_4131u32;
    for len in [
        0usize, 1, 2, 3, 7, 8, 9, 15, 16, 17, 31, 32, 33, 255, 256, 257, 1023, 1024, 1025, 4095,
        4096, 4097, 16383, 16384, 16385, 32767, 32768, 32769,
    ] {
        let mut raw: Vec<_> = (0..len.div_ceil(2))
            .map(|_| {
                random ^= random << 13;
                random ^= random >> 17;
                random ^= random << 5;
                random as u8
            })
            .collect();
        raw.extend_from_within(..len - raw.len());
        for level in [0, 9] {
            let compressed = reference(&raw, level);
            let mut trailing = compressed.clone();
            trailing.extend_from_slice(b"sentinel");
            for (input, output) in [(1, 1), (7, 31), (4096, 65536)] {
                let (actual, used) = support::inflate(&trailing, input, output, len).unwrap();
                assert_eq!(actual, raw, "length={len} level={level}");
                assert_eq!(used, compressed.len());
            }
            if len != 0 {
                assert_eq!(
                    support::inflate(&compressed, 7, 31, len - 1),
                    Err(Error::LimitExceeded)
                );
            }
        }
    }
}

#[test]
fn zero_capacity_prefix_truncations_and_terminal_errors() {
    let raw = b"one byte at a time";
    let compressed = reference(raw, 6);
    for end in 0..compressed.len() {
        assert!(
            support::inflate(&compressed[..end], 1, 1, raw.len()).is_err(),
            "{end}"
        );
    }
    assert_eq!(
        support::inflate(&compressed, 1, 1, raw.len() - 1),
        Err(Error::LimitExceeded)
    );
    let mut trailing = compressed.clone();
    trailing.extend_from_slice(b"sentinel");
    let (actual, used) = support::inflate(&trailing, 65536, 65536, raw.len()).unwrap();
    assert_eq!(actual, raw);
    assert_eq!(used, compressed.len());
    let mut inflater = Inflater::new(raw.len()).unwrap();
    let mut pos = 0;
    let mut actual = Vec::new();
    loop {
        let progress = inflater.process(&trailing[pos..], &mut []).unwrap();
        pos += progress.consumed;
        if progress.status == StreamStatus::Done {
            break;
        }
        assert_eq!(progress.status, StreamStatus::NeedOutput);
        let mut byte = [0];
        let p = inflater.process(&trailing[pos..], &mut byte).unwrap();
        pos += p.consumed;
        actual.extend_from_slice(&byte[..p.written]);
        if p.status == StreamStatus::Done {
            break;
        }
    }
    assert_eq!(actual, raw);
    assert_eq!(pos, compressed.len());
    let mut bad = compressed;
    *bad.last_mut().unwrap() ^= 1;
    let mut inflater = Inflater::new(100).unwrap();
    let error = inflater.process(&bad, &mut [0; 100]).unwrap_err();
    assert_eq!(error, Error::Checksum);
    assert_eq!(inflater.process(&[], &mut []), Err(error.clone()));
    assert_eq!(inflater.finish(), Err(error));
}
