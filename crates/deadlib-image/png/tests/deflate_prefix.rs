//! Buffered lookahead must leave suffixes intact and preserve mixed-block data.
use deadlib_image_png::{Error, zlib};
#[path = "support/huffman.rs"]
mod huffman;
#[path = "support/prefix.rs"]
mod prefix;
mod support;
use prefix::suffix;
use std::io::Read;

#[test]
fn every_valid_huffman_fixture_keeps_exact_prefix_trailer_limits_and_truncations() {
    let (mut valid, mut prefixes, mut truncated) = (0, 0, 0);
    for case in huffman::all_cases() {
        let Some(raw) = case.output else { continue };
        valid += 1;
        for len in [0, 1, 3, 4, 7, 8, 9, 15, 16, 31, 32, 63, 64, 127] {
            let mut input = case.bytes.clone();
            input.extend(suffix(len, valid));
            let (actual, used) = zlib::decompress_prefix(&input, raw.len()).unwrap();
            assert_eq!(actual, raw, "{} suffix={len}", case.name);
            assert_eq!(used, case.bytes.len(), "{} suffix={len}", case.name);
            prefixes += 1;
        }
        let mut input = case.bytes.clone();
        input.extend(suffix(127, valid));
        assert_eq!(
            zlib::decompress(&input, raw.len()),
            Err(Error::Invalid("trailing zlib data"))
        );
        for (a, b) in [(input.len(), 4096), (1, 1), (7, 31)] {
            let (actual, used) = support::inflate(&input, a, b, raw.len()).unwrap();
            assert_eq!(actual, raw);
            assert_eq!(used, case.bytes.len());
        }
        for missing in 1..=8.min(case.bytes.len()) {
            assert!(
                zlib::decompress_prefix(&case.bytes[..case.bytes.len() - missing], raw.len())
                    .is_err(),
                "{} missing={missing}",
                case.name
            );
            truncated += 1;
        }
        if !raw.is_empty() {
            assert_eq!(
                zlib::decompress_prefix(&case.bytes, raw.len() - 1),
                Err(Error::LimitExceeded),
                "{}",
                case.name
            );
        }
    }
    assert_eq!((valid, prefixes, truncated), (3091, 43274, 24728));
    println!(
        "Prefix audit: {valid} independent positive Huffman fixtures, {prefixes} suffix placements, {truncated} trailer/body truncations and three streaming fragment shapes each"
    );
}

fn check(input: Vec<u8>, raw: Vec<u8>, salt: usize, name: &str) -> usize {
    let mut count = 0;
    for len in [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 15, 16, 31, 32, 63, 64] {
        for pattern in 0..if len == 0 { 1 } else { 4 } {
            let mut bytes = input.clone();
            bytes.extend(match pattern {
                0 => vec![0; len],
                1 => vec![255; len],
                _ => suffix(len, salt + pattern),
            });
            assert_eq!(
                zlib::decompress_prefix(&bytes, raw.len()).unwrap(),
                (raw.clone(), input.len()),
                "{name} suffix={len} pattern={pattern}"
            );
            count += 1;
        }
    }
    let mut bytes = input.clone();
    bytes.extend(suffix(127, salt));
    for (a, b) in [(bytes.len(), 4096), (1, 1), (7, 31)] {
        assert_eq!(
            support::inflate(&bytes, a, b, raw.len()).unwrap(),
            (raw.clone(), input.len()),
            "{name} input={a} output={b}"
        );
    }
    count
}

#[test]
fn one_bit_end_codes_and_huffman_stored_huffman_transitions_at_every_alignment() {
    let fixtures = prefix::fixtures();
    let files = fixtures.len();
    let prefixes: usize = fixtures
        .into_iter()
        .enumerate()
        .map(|(salt, case)| {
            let mut independent = Vec::new();
            flate2::read::ZlibDecoder::new(case.bytes.as_slice())
                .read_to_end(&mut independent)
                .unwrap();
            assert_eq!(independent, case.raw, "{}", case.name);
            check(case.bytes, case.raw, salt, &case.name)
        })
        .sum();
    assert_eq!((files, prefixes), (740, 45140));
    println!(
        "Mixed/one-bit prefix audit: {files} independent streams, {prefixes} suffix/padding combinations, three streaming fragment shapes each"
    );
}
