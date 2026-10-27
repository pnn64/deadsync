//! Buffered lookahead must leave suffixes intact and preserve mixed-block data.
use deadlib_image_png::{Error, zlib};
#[path = "support/huffman.rs"]
mod huffman;
mod support;

fn suffix(len: usize, salt: usize) -> Vec<u8> {
    (0..len)
        .map(|n| ((n * 137 + salt * 73) ^ (n >> 2)) as u8)
        .collect()
}

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

fn check(input: Vec<u8>, raw: Vec<u8>, salt: usize) -> usize {
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
                (raw.clone(), input.len())
            );
            count += 1;
        }
    }
    let mut bytes = input.clone();
    bytes.extend(suffix(127, salt));
    for (a, b) in [(bytes.len(), 4096), (1, 1), (7, 31)] {
        assert_eq!(
            support::inflate(&bytes, a, b, raw.len()).unwrap(),
            (raw.clone(), input.len())
        );
    }
    count
}

#[test]
fn one_bit_end_codes_and_huffman_stored_huffman_transitions_at_every_alignment() {
    let (mut files, mut prefixes) = (0, 0);
    let mut fixed = [8; 288];
    fixed[144..256].fill(9);
    fixed[256..280].fill(7);
    let fixed = huffman::Codes::new(&fixed);
    for before in 0..17 {
        for stored in 0..18 {
            for ones in [false, true] {
                let mut wire = huffman::Wire::default();
                let mut raw = vec![164; before];
                wire.put(2, 3);
                for &b in &raw {
                    fixed.symbol(&mut wire, b as usize);
                }
                fixed.symbol(&mut wire, 256);
                let tail = suffix(stored, before);
                wire.stored(&tail, false);
                raw.extend(tail);
                wire.put(3, 3);
                for b in [0, 255, 144, 143, 3, 127, 248, 1] {
                    fixed.symbol(&mut wire, b as usize);
                    raw.push(b);
                }
                fixed.symbol(&mut wire, 256);
                prefixes += check(wire.finish(&raw, ones), raw, files);
                files += 1;
            }
        }
    }
    for length in 0..64 {
        for ones in [false, true] {
            let mut lengths = vec![0; 257];
            lengths[65] = 1;
            lengths[256] = 1;
            let codes = huffman::Codes::new(&lengths);
            let mut wire = huffman::Wire::default();
            huffman::lengths(&mut wire, &lengths, &[0]);
            for _ in 0..length {
                codes.symbol(&mut wire, 65);
            }
            codes.symbol(&mut wire, 256);
            let raw = vec![65; length];
            prefixes += check(wire.finish(&raw, ones), raw, files);
            files += 1;
        }
    }
    assert_eq!((files, prefixes), (740, 45140));
    println!(
        "Mixed/one-bit prefix audit: {files} independent streams, {prefixes} suffix/padding combinations, three streaming fragment shapes each"
    );
}
