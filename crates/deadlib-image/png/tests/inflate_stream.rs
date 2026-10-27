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
