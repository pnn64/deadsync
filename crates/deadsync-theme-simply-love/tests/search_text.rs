//! The production text helpers and frozen old bodies share the same benchmark binary.
use deadlib_assets::AssetManager;
use deadlib_present::font::{Font, Glyph, GlyphMap};
use std::hint::black_box;
use std::sync::Arc;

use deadsync_theme_simply_love::screens::components::select_music::select_music_menu::SONG_SEARCH_MAX_LEN;
#[path = "search_text/baseline.rs"]
mod baseline;
#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod perf;
#[path = "../src/screens/components/select_music/select_music_menu/song_search/text.rs"]
mod text;

fn fixture_assets(negative: bool) -> AssetManager {
    let texture: Arc<str> = Arc::from("fixture");
    let glyph = |advance: i32| Glyph {
        texture_key: texture.clone(),
        stroke_texture_key: None,
        tex_rect: [0.0; 4],
        uv_scale: [1.0; 2],
        uv_offset: [0.0; 2],
        size: [8.0, 12.0],
        offset: [0.0; 2],
        advance: advance as f32,
        advance_i32: advance,
    };
    let font = |glyph_map, fallback_font_name| Font {
        glyph_map,
        ascii_glyphs: Box::new(std::array::from_fn(|_| None)),
        default_glyph: Some(glyph(9)),
        line_spacing: 12,
        height: 12,
        fallback_font_name,
        cache_tag: 0,
        chain_key: 0,
        default_stroke_color: [0.0; 4],
        stroke_texture_map: Default::default(),
        texture_hints_map: Default::default(),
    };
    let mut glyphs = GlyphMap::default();
    for code in 0u8..128 {
        glyphs.insert(
            char::from(code),
            glyph(if negative && code % 3 == 0 {
                -17
            } else {
                i32::from(code % 7) + 3
            }),
        );
    }
    let mut fallback = GlyphMap::default();
    for ch in ['\u{65e5}', '\u{03a3}', '\u{00e9}', '\u{1f3b5}'] {
        fallback.insert(ch, glyph(13));
    }
    let mut assets = AssetManager::new();
    assets.register_fonts([
        ("fallback", font(fallback, None)),
        ("miso", font(glyphs, Some("fallback"))),
    ]);
    assets
}

fn generated_texts() -> Vec<String> {
    let parts = [
        "Song",
        " (Easy)",
        "( HARD )",
        "(edit)",
        "(Mix)",
        "(Short Cut)",
        "(",
        ")",
        " ",
        "\t",
        "\n",
        "\u{a0}",
        "\u{2003}",
        "\u{65e5}",
        "e\u{301}",
        "\u{1f3b5}",
        "[123]",
        "[20]",
        "[]",
        "[a[40]",
        "[4x]",
        "[000001]",
        "[",
        "]",
        "\0",
    ];
    let mut state = 123456789u64;
    (0..1024)
        .map(|n| {
            let mut out = String::new();
            for _ in 0..n % 40 {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                out.push_str(parts[(state >> 32) as usize % parts.len()]);
            }
            out
        })
        .collect()
}

#[test]
fn title_cleanup_matches_previous_unicode_whitespace_and_group_rules() {
    for input in generated_texts() {
        let old = baseline::strip_difficulty_parens(&input);
        let new = text::strip_difficulty_parens(&input);
        assert_eq!(new, old, "{input:?}");
        assert_eq!(
            matches!(new, std::borrow::Cow::Borrowed(_)),
            matches!(old, std::borrow::Cow::Borrowed(_))
        );
    }
    for input in [
        "x(Easy)y",
        "x (Easy) y",
        "(Hard)",
        "(Easy",
        "a (Remix)  b",
        "  untouched  ",
        "(x(Easy))",
    ] {
        assert_eq!(
            text::strip_difficulty_parens(input),
            baseline::strip_difficulty_parens(input)
        );
    }
}

#[test]
fn completion_matches_previous_tokens_unicode_and_character_limit() {
    let long_label = "\u{65e5}\u{1f3b5}a".repeat(100);
    let long_token = format!("[{}][20]", "0".repeat(500));
    let mut queries = generated_texts();
    queries.extend([
        long_token,
        "[12][30]".repeat(100),
        "[10]".repeat(19),
        "[10]".repeat(20),
    ]);
    for query in queries {
        for label in [
            "",
            "Hello World",
            "\u{65e5}\u{1f3b5} \u{00e9}",
            long_label.as_str(),
        ] {
            let new = text::song_search_query_completed_with(&query, label);
            assert_eq!(
                new,
                baseline::song_search_query_completed_with(&query, label),
                "{query:?} / {label:?}"
            );
            assert!(new.chars().count() <= SONG_SEARCH_MAX_LEN);
        }
    }
}

#[test]
fn fitting_matches_previous_with_fallbacks_negative_advances_and_nonfinite_inputs() {
    let long_text = "a b c d ".repeat(40);
    let texts = [
        "",
        "Hello World",
        "\u{65e5}\u{1f3b5} a \u{03a3} e\u{301}",
        "ababababababab",
        long_text.as_str(),
    ];
    for assets in [
        fixture_assets(false),
        fixture_assets(true),
        AssetManager::new(),
    ] {
        for input in texts {
            for zoom in [
                -2.0,
                -0.0,
                0.9,
                1.0,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NAN,
            ] {
                for budget in [
                    -100.0,
                    -0.0,
                    0.5,
                    20.0,
                    150.0,
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                    f32::NAN,
                ] {
                    let new = text::song_search_fit(&assets, input, budget, zoom);
                    let old = baseline::song_search_fit(&assets, input, budget, zoom);
                    assert_eq!(new, old, "{input:?}, budget {budget}, zoom {zoom}");
                    assert_eq!(new.as_ptr(), input.as_ptr());
                }
            }
        }
    }
}

#[test]
fn fitting_handles_every_cut_boundary_for_mixed_utf8_text() {
    let assets = fixture_assets(false);
    let input = "a\u{00e9}\u{65e5}\u{1f3b5} Hello \u{03a3} e\u{301}";
    for budget in -5..200 {
        let budget = budget as f32 * 0.9;
        assert_eq!(
            text::song_search_fit(&assets, input, budget, 0.9),
            baseline::song_search_fit(&assets, input, budget, 0.9)
        );
    }
}

#[test]
fn search_text_allocation_budgets() {
    let assets = fixture_assets(false);
    for input in [
        "Title (Easy) (Remix)  Tail",
        "a(Easy)b",
        "(  ",
        "\u{65e5} (Hard)\t\u{1f3b5}",
    ] {
        perf::assert_churn_budget(1, input.len(), || {
            black_box(text::strip_difficulty_parens(input));
        });
    }
    perf::assert_no_churn(|| {
        black_box(text::strip_difficulty_parens("Plain \u{65e5} Title"));
    });
    for (query, label) in [
        ("[12] hello [170]", "Complete Title"),
        ("", "\u{65e5}\u{1f3b5}"),
        ("[3x]", ""),
        ("[12][13]", ""),
    ] {
        let expected = baseline::song_search_query_completed_with(query, label);
        perf::assert_churn_budget(usize::from(!expected.is_empty()), expected.len(), || {
            black_box(text::song_search_query_completed_with(query, label));
        });
    }
    for budget in [0.0, 50.0, 10000.0] {
        perf::assert_no_churn(|| {
            black_box(text::song_search_fit(
                &assets,
                "A \u{65e5} long \u{1f3b5} completion string",
                budget,
                0.9,
            ));
        });
    }
}

#[test]
#[ignore = "release CPU/allocation benchmark; --ignored --nocapture --test-threads=1"]
fn search_text_bench() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    let titles = [
        ("plain", "A Plain Title".to_string()),
        (
            "annotated",
            "Some (Easy) Song (Challenge)  \t(Club Remix)".to_string(),
        ),
        (
            "unicode",
            "\u{65e5}\u{1f3b5} (Hard)\u{2003}e\u{301} (Short Mix)".repeat(8),
        ),
    ];
    for (case, input) in titles {
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            let run = black_box(if old {
                baseline::strip_difficulty_parens
            } else {
                text::strip_difficulty_parens
            });
            perf::measure_sampled(
                &format!("clean_{case}_{}", if old { "old" } else { "new" }),
                2000,
                input.chars().count(),
                || run(black_box(&input)),
            );
        }
    }
    let long_label = "\u{65e5}\u{1f3b5}a".repeat(100);
    for (case, query, label) in [
        ("plain", "butter", "Butterfly"),
        (
            "filters",
            "[12] [0170] some [9999999999] [35]",
            "Some Complete Title",
        ),
        ("unicode", "[12] \u{65e5} [200]", long_label.as_str()),
        ("invalid", "[[x] [1a] [] [\u{65e5}] query", "Complete Title"),
    ] {
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            let run = black_box(if old {
                baseline::song_search_query_completed_with
            } else {
                text::song_search_query_completed_with
            });
            perf::measure_sampled(
                &format!("completion_{case}_{}", if old { "old" } else { "new" }),
                2000,
                1,
                || run(black_box(query), black_box(label)),
            );
        }
    }
    let assets = fixture_assets(false);
    for (case, input, budget) in [
        ("short", "Butterfly".to_string(), 300.0),
        (
            "clipped",
            "A long completion title that exceeds the query line width".to_string(),
            150.0,
        ),
        ("unicode", "\u{65e5}\u{1f3b5}a e\u{301} ".repeat(12), 150.0),
        ("long", "A long word and ".repeat(60), 300.0),
    ] {
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            let name = format!("fit_{case}_{}", if old { "old" } else { "new" });
            if old {
                let run = black_box(
                    baseline::song_search_fit as fn(&AssetManager, &str, f32, f32) -> String,
                );
                perf::measure_sampled(&name, 1000, input.chars().count(), || {
                    run(
                        black_box(&assets),
                        black_box(&input),
                        black_box(budget),
                        black_box(0.9),
                    )
                });
            } else {
                let run = black_box(
                    text::song_search_fit
                        as for<'a, 'b> fn(&'a AssetManager, &'b str, f32, f32) -> &'b str,
                );
                perf::measure_sampled(&name, 1000, input.chars().count(), || {
                    run(
                        black_box(&assets),
                        black_box(&input),
                        black_box(budget),
                        black_box(0.9),
                    )
                    .to_owned()
                });
            }
        }
    }
}
