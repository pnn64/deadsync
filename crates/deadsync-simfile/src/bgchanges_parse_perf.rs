use super::*;
use crate::metadata_perf::{compare, measure};
use std::hint::black_box;
mod original {
    include!("bgchanges_parse_original.rs");
}

#[test]
fn dot_first_non_media_scan_preserves_substrings_case_and_utf8() {
    for prefix in ["", "a", "..", "folder/", "\u{00e9}/\u{66f2}/"] {
        for suffix in [
            "",
            ".",
            ".i",
            ".in",
            ".ini",
            ".ini.png",
            ".XMLextra",
            ".Ini",
            ".XmL",
            ".xMl",
            ".mkv",
            "x.ini/y",
            ".\u{0130}ni",
            ".\u{00e9}ml",
        ] {
            let field = format!("{prefix}{suffix}");
            assert_eq!(
                bgchange_field_rejects_non_media(&field),
                original::bgchange_field_rejects_non_media(&field),
                "{field}"
            );
        }
    }
    for extension in ["ini", "xml"] {
        for mask in 0..8 {
            for position in 0..128 {
                let suffix: String = extension
                    .bytes()
                    .enumerate()
                    .map(|(i, b)| {
                        if mask & (1 << i) != 0 {
                            b.to_ascii_uppercase() as char
                        } else {
                            b as char
                        }
                    })
                    .collect();
                let field = format!(
                    "{}.{}{}",
                    "a".repeat(position),
                    suffix,
                    "z".repeat(127 - position)
                );
                assert!(bgchange_field_rejects_non_media(&field));
            }
        }
    }
}

#[test]
fn dot_first_non_media_scan_matches_generated_paths() {
    let alphabet = b"a./INXmlinixmL-0129";
    let mut seed = 0x73a9f527u32;
    for len in [0, 1, 2, 3, 4, 8, 32, 128, 1024] {
        for _ in 0..512 {
            let input: String = (0..len)
                .map(|_| {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    alphabet[seed as usize % alphabet.len()] as char
                })
                .collect();
            assert_eq!(
                bgchange_field_rejects_non_media(&input),
                original::bgchange_field_rejects_non_media(&input)
            );
        }
    }
}

#[test]
#[ignore = "paired release benchmark; run alone with --nocapture"]
fn benchmark_parse_non_media_scan() {
    for (label, input) in [
        ("empty", String::new()),
        ("tiny", "ab".into()),
        ("movie", "background.mp4".into()),
        ("path-128", format!("{}.mp4", "a".repeat(124))),
        ("path-4096", format!("{}.mp4", "a".repeat(4092))),
        ("early-reject", format!(".Ini{}", "a".repeat(124))),
        ("late-reject", format!("{}.XML", "a".repeat(124))),
        ("dot-heavy", ".mp4".repeat(32)),
        ("unicode", "\u{66f2}/\u{00e9}/background.png".into()),
    ] {
        let (_, a) = measure(|| original::bgchange_field_rejects_non_media(&input));
        let (_, b) = measure(|| bgchange_field_rejects_non_media(&input));
        println!("ALLOC media/{label}: original {a:?}, current {b:?}");
        compare(
            &format!("media/{label}"),
            1024,
            || {
                black_box(original::bgchange_field_rejects_non_media(black_box(
                    &input,
                )));
            },
            || {
                black_box(bgchange_field_rejects_non_media(black_box(&input)));
            },
        );
    }
}
