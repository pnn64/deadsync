use super::wrap_text_with_measure;
use crate::perf::{assert_churn_budget, measure_sampled};
use deadlib_present::font::{self, Font, FontMap, Glyph, GlyphMap};
use std::cell::RefCell;
use std::hint::black_box;
use std::sync::Arc;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/ui_text/wrap_baseline.rs"
    ));
}

fn width(text: &str) -> f32 {
    text.chars()
        .map(|c| if c.is_ascii() { 1 } else { 2 })
        .sum::<i32>() as f32
}

#[test]
fn wrapping_preserves_expected_breaks_and_whitespace() {
    for (source, limit, expected) in [
        ("one two three", 7.0, "one two\nthree"),
        ("abcdefg", 3.0, "abc\ndef\ng"),
        ("\n\n  one\t two \n\nthree\n", 7.0, "one two\n\nthree\n"),
        (
            "\u{65e5}\u{672c}\u{8a9e} abc",
            4.0,
            "\u{65e5}\u{672c}\n\u{8a9e}\nabc",
        ),
        ("\n \t\r\n", 4.0, "\n \t\r\n"),
        ("ab cd", 0.0, "a\nb\nc\nd"),
        ("a\u{a0}b", 8.0, "a b"),
    ] {
        assert_eq!(wrap_text_with_measure(source, limit, width), expected);
    }
}

#[test]
fn wrapping_matches_parent_text_and_measurement_sequence() {
    let alphabet = [
        'a',
        'W',
        ' ',
        '\t',
        '\n',
        '\r',
        '\u{65e5}',
        '\u{1f3b5}',
        '\u{301}',
        '\u{2003}',
    ];
    let mut seed = 17_u64;
    for count in [0, 1, 3, 22, 127, 513] {
        for _ in 0..12 {
            let text: String = (0..count)
                .map(|_| {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    alphabet[(seed >> 32) as usize % alphabet.len()]
                })
                .collect();
            for limit in [
                -1.0,
                0.0,
                1.0,
                7.0,
                20.5,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NAN,
            ] {
                for zoom in [0.0, 0.72, -1.0, f32::NAN] {
                    let old_calls = RefCell::new(Vec::new());
                    let new_calls = RefCell::new(Vec::new());
                    let old = baseline::wrap_text_with_measure(&text, limit, |value| {
                        old_calls.borrow_mut().push(value.to_owned());
                        width(value) * zoom
                    });
                    let new = wrap_text_with_measure(&text, limit, |value| {
                        new_calls.borrow_mut().push(value.to_owned());
                        width(value) * zoom
                    });
                    assert_eq!(new, old, "source {text:?}, limit {limit}, zoom {zoom}");
                    assert_eq!(new_calls.into_inner(), old_calls.into_inner());
                }
            }
        }
    }
}

#[test]
fn hard_breaking_has_bounded_allocation_churn() {
    let source = "a".repeat(2048) + &" ".repeat(2048);
    // Trailing whitespace leaves enough output capacity for inserted newlines.
    // Only the output and line buffers allocate, despite 683 hard breaks.
    assert_churn_budget(2, 16_384, || {
        let output = wrap_text_with_measure(&source, 3.0, width);
        assert_eq!(output.lines().count(), 683);
        assert!(output.lines().all(|line| line.len() <= 3));
    });
}

fn fonts() -> FontMap {
    let glyph_map = (0..128_u8)
        .map(char::from)
        .chain(['\u{65e5}', '\u{672c}', '\u{8a9e}', '\u{1f3b5}'])
        .map(|ch| {
            let advance = if ch == ' ' {
                4
            } else if ch.is_ascii() {
                7
            } else {
                14
            };
            (
                ch,
                Glyph {
                    texture_key: Arc::from("miso"),
                    stroke_texture_key: None,
                    tex_rect: [0.0; 4],
                    uv_scale: [1.0; 2],
                    uv_offset: [0.0; 2],
                    size: [7.0, 16.0],
                    offset: [0.0; 2],
                    advance: advance as f32,
                    advance_i32: advance,
                },
            )
        })
        .collect::<GlyphMap>();
    let mut fonts = FontMap::default();
    fonts.insert(
        "miso",
        Font {
            glyph_map,
            ascii_glyphs: Box::new(std::array::from_fn(|_| None)),
            default_glyph: None,
            height: 16,
            line_spacing: 16,
            fallback_font_name: None,
            cache_tag: 0,
            chain_key: 0,
            default_stroke_color: [0.0; 4],
            stroke_texture_map: Default::default(),
            texture_hints_map: Default::default(),
        },
    );
    font::refresh_chain_keys(&mut fonts);
    fonts
}

#[test]
#[ignore = "manual old/new CPU-cycle and allocation benchmark"]
fn ui_text_wrap_bench() {
    let fonts = fonts();
    let font = &fonts["miso"];
    let measure = |s: &str| font::measure_line_width_logical(font, s, &fonts) as f32 * 0.72;
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (name, text, limit) in [
        ("empty", String::new(), 240.0),
        ("short", "Change the music volume.".to_owned(), 240.0),
        ("paragraph", "Choose the audio output device and adjust the volume. Changes apply when leaving this screen. ".repeat(6), 240.0),
        ("path", "C:/Songs/LongPackName/".repeat(48), 240.0),
        ("unicode", "\u{65e5}\u{672c}\u{8a9e}\u{1f3b5}".repeat(128), 120.0),
    ] {
        for old in if reverse { [false, true] } else { [true, false] } {
            let label = format!("wrap_{name}/{}", if old { "old" } else { "new" });
            if old {
                measure_sampled(&label, 256, text.chars().count(), || baseline::wrap_text_with_measure(black_box(&text), black_box(limit), &measure));
            } else {
                measure_sampled(&label, 256, text.chars().count(), || wrap_text_with_measure(black_box(&text), black_box(limit), &measure));
            }
        }
    }
}
