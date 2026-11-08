use super::normalized_background_lua_color;
use crate::bgchanges::{
    bgchange_field_rejects_non_media, parse_bgchange_color, split_bgchange_sets_like_itg,
};
use crate::metadata_perf::measure as allocations;
use std::hint::black_box;

#[path = "../../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "background_fields_original.rs"]
mod original;

#[derive(Debug, PartialEq, Eq)]
enum ColorResult {
    Value(Option<[u32; 4]>),
    Panic(String),
}

fn color_result(work: impl FnOnce() -> Option<[f32; 4]> + std::panic::UnwindSafe) -> ColorResult {
    match std::panic::catch_unwind(work) {
        Ok(value) => ColorResult::Value(value.map(|color| color.map(f32::to_bits))),
        Err(payload) => ColorResult::Panic(
            payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| {
                    payload
                        .downcast_ref::<&str>()
                        .map(|text| (*text).to_owned())
                })
                .expect("string slice diagnostics retain a string payload"),
        ),
    }
}

fn check_color(text: &str) {
    assert_eq!(
        color_result(|| parse_bgchange_color(text)),
        color_result(|| original::parse_bgchange_color(text)),
        "{text:?}"
    );
}

#[test]
fn media_scan_preserves_embedded_matches_case_and_utf8() {
    for prefix in [
        "",
        "folder/",
        ".",
        "a.".repeat(64).as_str(),
        "\u{96ea}/\u{e9}/",
    ] {
        for suffix in [
            "",
            ".",
            ".i",
            ".in",
            ".ini",
            ".XMLextra",
            ".ini.png",
            ".mp4",
            ".\u{130}ni",
        ] {
            let text = format!("{prefix}{suffix}");
            assert_eq!(
                bgchange_field_rejects_non_media(&text),
                original::bgchange_field_rejects_non_media(&text),
                "{text:?}"
            );
        }
    }
    for expected in ["ini", "xml"] {
        for mask in 0..8 {
            let suffix: String = expected
                .bytes()
                .enumerate()
                .map(|(i, byte)| {
                    char::from(if mask & (1 << i) == 0 {
                        byte
                    } else {
                        byte.to_ascii_uppercase()
                    })
                })
                .collect();
            for offset in 0..130 {
                let text = format!("{}.{}tail", "x".repeat(offset), suffix);
                assert!(bgchange_field_rejects_non_media(&text));
            }
        }
    }
    let alphabet = [
        'a', '.', '/', 'I', 'N', 'X', 'm', 'l', '\0', '\u{e9}', '\u{96ea}', '\u{130}',
    ];
    let mut state = 0x9e3779b9u32;
    for len in [0, 1, 3, 4, 8, 31, 64, 257, 1024] {
        for _ in 0..256 {
            let text: String = (0..len)
                .map(|_| {
                    state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                    alphabet[state as usize % alphabet.len()]
                })
                .collect();
            assert_eq!(
                bgchange_field_rejects_non_media(&text),
                original::bgchange_field_rejects_non_media(&text)
            );
        }
    }
}

#[test]
fn colors_preserve_hex_bytes_float_bits_and_malformed_unicode_panics() {
    for alpha in [false, true] {
        for position in 0..if alpha { 4 } else { 3 } {
            for byte in 0..=u8::MAX {
                let mut color = [17, 85, 170, 255];
                color[position] = byte;
                let [r, g, b, a] = color;
                let text = if alpha {
                    format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
                } else {
                    format!("#{r:02x}{g:02x}{b:02x}")
                };
                check_color(&text);
                check_color(&text.to_ascii_uppercase());
                check_color(&format!("\u{2003}{text}\r\n"));
            }
        }
    }
    for text in ["#12abEF", "#12abEF45"] {
        for position in 1..text.len() {
            for byte in 0..=127 {
                let mut value = text.as_bytes().to_vec();
                value[position] = byte;
                check_color(std::str::from_utf8(&value).unwrap());
            }
        }
    }
    for text in [
        "#1\u{e9}^00",
        "#1\u{e9}0000",
        "#12\u{e9}00",
        "#1\u{96ea}00",
        "#123\u{e9}0",
        "#12345\u{e9}0",
    ] {
        check_color(text);
    }
    for text in [
        "",
        "#",
        "#123",
        "#12345",
        "#1234567",
        "#123456789",
        "bad",
        "1",
        "1^2",
        "1^2^3",
        "1,2,3,4",
        "1^^2,,3^",
        "1,2^3,4",
        "1^2^3^4^5",
        " NaN ^ -inf ^ inf ^ -0 ",
        "0.0000001^1e20^3.4028235e38",
        "\u{2003}1 ^ 2 ^ 3\u{2003}",
    ] {
        check_color(text);
    }
}

#[test]
fn normalized_colors_preserve_rounding_capacity_and_legacy_input_diagnostics() {
    for field in [
        None,
        Some(""),
        Some("bad"),
        Some("#123456"),
        Some("#12345678"),
        Some("#1\u{e9}^00"),
        Some("\u{2003}0.5^0^1\u{2003}"),
        Some("NaN^inf^-inf^-0"),
    ] {
        let before = std::panic::catch_unwind(|| original::normalized_background_lua_color(field));
        let after = std::panic::catch_unwind(|| normalized_background_lua_color(field));
        match (before, after) {
            (Ok(before), Ok(after)) => {
                assert_eq!(after, before);
                assert_eq!(after.capacity(), before.capacity());
            }
            (Err(before), Err(after)) => assert_eq!(
                before.downcast_ref::<String>(),
                after.downcast_ref::<String>()
            ),
            _ => panic!("normalization outcome changed for {field:?}"),
        }
    }
    for index in -2..=512 {
        for delimiter in [',', '^'] {
            let component = (index as f32 + 0.5) / 255.0;
            let text = format!("{component}{delimiter}0.5{delimiter}1{delimiter}{component}");
            let before = original::normalized_background_lua_color(Some(&text));
            let after = normalized_background_lua_color(Some(&text));
            assert_eq!(after, before, "{text:?}");
            assert_eq!(after.capacity(), before.capacity());
        }
    }
}

fn metadata_tag(count: usize) -> String {
    (0..count).map(|i| format!("{}=Backgrounds/Pack/movie-{i:03}.mp4=1=0=0=1=StretchNoLoop==CrossFade=#12abEF=0.5^0.25^1^0.75", i * 16))
        .collect::<Vec<_>>().join(",")
}

#[test]
fn composed_fields_preserve_media_rejection_and_both_color_columns() {
    let text = metadata_tag(128);
    for fields in split_bgchange_sets_like_itg(&text, &[]) {
        assert_eq!(
            bgchange_field_rejects_non_media(&fields[1]),
            original::bgchange_field_rejects_non_media(&fields[1])
        );
        for index in [9, 10] {
            check_color(&fields[index]);
            assert_eq!(
                normalized_background_lua_color(Some(&fields[index])),
                original::normalized_background_lua_color(Some(&fields[index]))
            );
        }
    }
}

fn measure(label: &str, repetitions: usize, mut work: impl FnMut()) {
    let (_, counts) = allocations(&mut work);
    println!("{label}: allocation {counts:?}");
    bench::measure(label, repetitions, work);
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_background_fields() {
    for (label, prefix, suffix) in [
        ("short", "background".to_owned(), ".mp4"),
        ("path128", "a".repeat(124), ".mp4"),
        ("path1024", "a".repeat(1020), ".mp4"),
        ("early", ".Ini".to_owned(), "background"),
        ("late", "a".repeat(124), ".XML"),
        ("dotted", "folder.name/".repeat(12), ".png"),
        ("unicode", "\u{96ea}/\u{e9}/Backgrounds".to_owned(), ".png"),
        ("tiny", String::new(), "ab"),
    ] {
        let values: Vec<_> = (0..256).map(|i| format!("{prefix}{i}{suffix}")).collect();
        measure(
            &format!("media/{label}"),
            if label == "path1024" { 8 } else { 64 },
            || {
                for value in black_box(&values) {
                    black_box(bgchange_field_rejects_non_media(black_box(value)));
                }
            },
        );
    }
    for (label, values) in [
        (
            "rgb",
            (0..256)
                .map(|i| format!("#{i:02x}80ff"))
                .collect::<Vec<_>>(),
        ),
        (
            "rgba",
            (0..256).map(|i| format!("#1280ff{i:02x}")).collect(),
        ),
        ("upper", (0..256).map(|i| format!("#{i:02X}ABEF")).collect()),
        (
            "invalid",
            (0..256).map(|i| format!("#{i:02x}80fg")).collect(),
        ),
        ("caret", vec!["0.5^0.25^1^0.75".to_owned(); 256]),
        ("comma", vec!["0.5,0.25,1,0.75".to_owned(); 256]),
        ("mixed", vec!["0.5^0.25,1^0.75".to_owned(); 256]),
        (
            "unicode",
            vec!["\u{2003}0.5^0.25^1\u{2003}".to_owned(); 256],
        ),
    ] {
        measure(&format!("color/{label}"), 64, || {
            for value in black_box(&values) {
                black_box(parse_bgchange_color(black_box(value)));
            }
        });
        measure(&format!("normalize/{label}"), 32, || {
            for value in black_box(&values) {
                black_box(normalized_background_lua_color(Some(black_box(value))));
            }
        });
    }
    let tag = metadata_tag(128);
    measure("compose/media", 8, || {
        for fields in split_bgchange_sets_like_itg(black_box(&tag), &[]) {
            if !bgchange_field_rejects_non_media(black_box(&fields[1])) {
                for index in [9, 10] {
                    black_box(parse_bgchange_color(black_box(&fields[index])));
                }
            }
        }
    });
    measure("compose/keys", 8, || {
        for fields in split_bgchange_sets_like_itg(black_box(&tag), &[]) {
            for index in [9, 10] {
                black_box(normalized_background_lua_color(Some(black_box(
                    &fields[index],
                ))));
            }
        }
    });
}
