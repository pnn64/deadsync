use super::*;
use crate::{
    perf,
    xml::content_perf::{baseline, pair, stats_xml},
};
use std::hint::black_box;

// Original read_stats plain-file conversion from 29b11efb8, with its owned
// fs::read result supplied explicitly so benchmarks exclude filesystem work.
fn baseline_bytes(bytes: Vec<u8>) -> String {
    String::from_utf8_lossy(&bytes).into_owned()
}

#[test]
fn owned_stats_bytes_preserve_lossy_utf8_and_reuse_valid_storage() {
    let text = stats_xml(16, true, true);
    let bytes = text.clone().into_bytes();
    let pointer = bytes.as_ptr();
    let mut decoded = String::new();
    perf::assert_no_churn(|| decoded = decode_stats_bytes(bytes));
    assert_eq!(decoded, text);
    assert_eq!(decoded.as_ptr(), pointer);
    for bytes in [
        vec![],
        vec![0],
        vec![0xc0, 0xaf],
        vec![0xed, 0xa0, 0x80],
        vec![0xf0, 0x9f],
        vec![0xff, b'a', 0xfe],
        b"\xef\xbb\xbf<Stats/>".to_vec(),
    ] {
        assert_eq!(baseline_bytes(bytes.clone()), decode_stats_bytes(bytes));
    }
    let mut seed = 53u64;
    for count in 0..1024 {
        let bytes: Vec<_> = (0..count % 257)
            .map(|_| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                (seed >> 32) as u8
            })
            .collect();
        assert_eq!(baseline_bytes(bytes.clone()), decode_stats_bytes(bytes));
    }
}

#[test]
fn stats_parse_and_owned_score_extraction_match_legacy() {
    for count in [0, 1, 32] {
        for pretty in [false, true] {
            for entities in [false, true] {
                let mut bytes = stats_xml(count, pretty, entities).into_bytes();
                for invalid in [false, true] {
                    if invalid {
                        let guid = bytes.windows(4).position(|part| part == b"guid").unwrap();
                        bytes[guid] = 0xff;
                    }
                    let old = baseline_bytes(bytes.clone());
                    let new = decode_stats_bytes(bytes.clone());
                    assert_eq!(old, new);
                    let old_root = baseline::parse(&old).map_err(|e| e.to_string());
                    let new_root = xml::parse(&new).map_err(|e| e.to_string());
                    assert_eq!(old_root, new_root);
                    if let (Ok(old), Ok(new)) = (old_root, new_root) {
                        assert_eq!(parse_general_data(&old), parse_general_data(&new));
                        assert_eq!(
                            format!("{:?}", parse_song_scores_owned(old)),
                            format!("{:?}", parse_song_scores_owned(new))
                        );
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "manual release benchmark; seven batches and separate allocation accounting"]
fn profile_import_bench_bytes() {
    for count in [0, 1, 128, 2048] {
        let input = stats_xml(count, true, false).into_bytes();
        for invalid in [false, true] {
            let mut input = input.clone();
            if invalid {
                let middle = input.len() / 2;
                input[middle] = 0xff;
            }
            pair(
                &format!("bytes_{count}_{}", if invalid { "lossy" } else { "valid" }),
                if count > 128 {
                    32
                } else if count > 1 {
                    256
                } else {
                    8192
                },
                input.len(),
                || baseline_bytes(black_box(&input).clone()),
                || decode_stats_bytes(black_box(&input).clone()),
            );
        }
    }
    for count in [1, 128, 512] {
        let bytes = stats_xml(count, true, true).into_bytes();
        pair(
            &format!("import_{count}"),
            if count > 1 { 32 } else { 8192 },
            count,
            || {
                let s = baseline_bytes(black_box(&bytes).clone());
                let root = baseline::parse(&s).unwrap();
                let general = parse_general_data(&root);
                (general, parse_song_scores_owned(root))
            },
            || {
                let s = decode_stats_bytes(black_box(&bytes).clone());
                let root = xml::parse(&s).unwrap();
                let general = parse_general_data(&root);
                (general, parse_song_scores_owned(root))
            },
        );
    }
}
