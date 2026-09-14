//! Rendering hot-path comparisons against 0.5.1217 (d77a218a1).
use deadlib_present::font;
use std::hint::black_box;
use std::sync::Arc;

#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;
#[allow(dead_code)]
#[path = "../src/style.rs"]
mod style;

macro_rules! appearance_accessors {
    () => {
        pub(super) fn scales(y: f32, cache: &NoteAppearanceCache) -> [f32; 4] {
            [
                hidden_fade_scaled_bounded(y, cache),
                sudden_fade_scaled_bounded(y, cache),
                hidden_fade_scaled_finite(y, cache),
                sudden_fade_scaled_finite(y, cache),
            ]
        }
        pub(super) fn bounds(cache: &NoteAppearanceCache) -> [f32; 4] {
            [
                cache.hidden_start,
                cache.hidden_end,
                cache.sudden_start,
                cache.sudden_end,
            ]
        }
    };
}

#[allow(dead_code)]
mod appearance {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/transforms.rs"));
    appearance_accessors!();
}
#[allow(dead_code)]
mod old_appearance {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/render_math/appearance_baseline.rs"
    ));
    appearance_accessors!();
}
#[path = "render_math/font_baseline.rs"]
mod old_font;

macro_rules! params {
    ($module:ident, $v:expr) => {
        $module::NoteAlphaParams {
            hidden: $v[0],
            hidden_offset: $v[1],
            sudden: $v[2],
            sudden_offset: $v[3],
            stealth: $v[4],
            blink: $v[5],
            random_vanish: $v[6],
        }
    };
}

fn assert_float(actual: f32, expected: f32) {
    assert!(
        actual.to_bits() == expected.to_bits() || (actual.is_nan() && expected.is_nan()),
        "{actual:?} ({:08x}) != {expected:?} ({:08x})",
        actual.to_bits(),
        expected.to_bits()
    );
}

fn compare_appearance(elapsed: f32, mini: f32, values: [f32; 7], queries: &[f32]) {
    let new = appearance::note_appearance_cache(elapsed, mini, params!(appearance, values));
    let old = old_appearance::note_appearance_cache(elapsed, mini, params!(old_appearance, values));
    assert_eq!(std::mem::size_of_val(&new), std::mem::size_of_val(&old));
    let bounds = appearance::bounds(&new);
    assert_eq!(
        bounds.map(f32::to_bits),
        old_appearance::bounds(&old).map(f32::to_bits)
    );
    for y in queries.iter().copied().chain(
        bounds
            .into_iter()
            .flat_map(|v| [v.next_down(), v, v.next_up()]),
    ) {
        for (a, b) in appearance::scales(y, &new)
            .into_iter()
            .zip(old_appearance::scales(y, &old))
        {
            assert_float(a, b);
        }
        assert_float(
            appearance::appearance_note_alpha_cached(y, &new),
            old_appearance::appearance_note_alpha_cached(y, &old),
        );
        let (a, b) = appearance::appearance_note_alpha_glow_cached(y, &new);
        let (c, d) = old_appearance::appearance_note_alpha_glow_cached(y, &old);
        assert_float(a, c);
        assert_float(b, d);
    }
}

#[test]
fn visibility_matches_all_effect_combinations_and_fade_boundaries() {
    for mask in 0..32 {
        let mut values = [0.0; 7];
        for (bit, field) in [0, 2, 4, 5, 6].into_iter().enumerate() {
            if mask & (1 << bit) != 0 {
                values[field] = [1.0, 0.75, 0.2, 0.5, 0.3][bit];
            }
        }
        for mini in [-1.0, -0.0, 0.0, 0.5, 1.0, 2.0, 10.0] {
            for offset in [-2.0, 0.0, 0.25, 1.0e16] {
                values[1] = offset;
                values[3] = -offset;
                compare_appearance(
                    1.25,
                    mini,
                    values,
                    &[
                        -1.0, -0.0, 0.0, 80.0, 120.0, 140.0, 160.0, 180.0, 200.0, 320.0, 10000.0,
                    ],
                );
            }
        }
    }
}

#[test]
fn single_fade_endpoints_exclude_inactive_modifiers() {
    for active in [0, 2] {
        for intensity in [
            f32::EPSILON.next_up(),
            0.25,
            1.0,
            2.0,
            f32::MAX,
            f32::INFINITY,
        ] {
            for inactive in [0.0, -0.0, -1.0, f32::EPSILON, f32::NEG_INFINITY, f32::NAN] {
                for field in [0, 2, 4, 5, 6] {
                    if field == active {
                        continue;
                    }
                    let mut values = [0.0; 7];
                    values[active] = intensity;
                    values[field] = inactive;
                    compare_appearance(
                        0.5,
                        0.0,
                        values,
                        &[0.0, 80.0, 140.0, 180.0, 320.0, f32::INFINITY, f32::NAN],
                    );
                }
            }
        }
    }
}

fn random(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    *seed
}

#[test]
fn visibility_preserves_nonfinite_subnormal_and_random_inputs() {
    let special = [
        0.0,
        -0.0,
        f32::from_bits(1),
        -f32::from_bits(1),
        f32::MIN_POSITIVE,
        f32::EPSILON,
        f32::MAX,
        -f32::MAX,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
        f32::from_bits(0x7f800001),
        f32::from_bits(0xffc12345),
    ];
    for value in special {
        for field in 0..7 {
            let mut values = [0.75, 0.0, 0.5, 0.0, 0.2, 0.25, 0.1];
            values[field] = value;
            compare_appearance(0.5, 0.0, values, &special);
        }
        compare_appearance(value, value, [1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0], &special);
    }
    let mut seed = 0x1218;
    for _ in 0..2000 {
        let values = std::array::from_fn(|_| f32::from_bits(random(&mut seed) as u32));
        let elapsed = f32::from_bits(random(&mut seed) as u32);
        let mini = f32::from_bits(random(&mut seed) as u32);
        let y = f32::from_bits(random(&mut seed) as u32);
        compare_appearance(elapsed, mini, values, &[y]);
    }
}

fn glyph(advance: i32) -> font::Glyph {
    font::Glyph {
        texture_key: Arc::from("font texture"),
        stroke_texture_key: None,
        tex_rect: [0.0; 4],
        uv_scale: [1.0; 2],
        uv_offset: [0.0; 2],
        size: [8.0, 12.0],
        offset: [0.0, -0.0],
        advance: advance as f32,
        advance_i32: advance,
    }
}

fn fonts() -> font::FontMap {
    let mut out = font::FontMap::default();
    for (i, name) in ["primary", "fallback", "last"].into_iter().enumerate() {
        let mut glyph_map = font::GlyphMap::default();
        for c in 0..128u8 {
            if c as usize % 3 == i {
                glyph_map.insert(char::from(c), glyph(c as i32 % 17 - 3));
            }
        }
        glyph_map.insert(
            ['\u{00e9}', '\u{65e5}', '\u{1f3b5}'][i],
            glyph(13 + i as i32),
        );
        out.insert(
            name,
            font::Font {
                glyph_map,
                ascii_glyphs: Box::new(std::array::from_fn(|_| None)),
                default_glyph: Some(glyph(7)),
                line_spacing: 12,
                height: 10,
                fallback_font_name: [Some("fallback"), Some("last"), None][i],
                cache_tag: 0,
                chain_key: 0,
                default_stroke_color: [0.0; 4],
                stroke_texture_map: Default::default(),
                texture_hints_map: Default::default(),
            },
        );
    }
    font::refresh_chain_keys(&mut out);
    out
}

fn compare_widths(fonts: &font::FontMap, texts: &[String]) {
    for font in fonts.values() {
        for text in texts {
            assert_eq!(
                font::measure_line_width_logical(font, text, fonts),
                old_font::measure_line_width_logical(font, text, fonts),
                "text {text:?}"
            );
        }
    }
}

#[test]
fn text_width_preserves_ascii_unicode_fallbacks_and_mutations() {
    let mut fonts = fonts();
    let mut texts: Vec<String> = (0..128u8).map(|c| char::from(c).to_string()).collect();
    texts.extend(
        [
            "",
            "A",
            "  SCORE 123.45%  ",
            "missing \u{2603}",
            "caf\u{00e9} \u{65e5}\u{1f3b5}",
            "\0\t\r\n",
        ]
        .map(str::to_owned),
    );
    texts.push((0..128u8).map(char::from).cycle().take(4096).collect());
    for phase in 0..4 {
        if phase == 1 {
            fonts.get_mut("last").unwrap().glyph_map.clear();
            fonts.get_mut("primary").unwrap().default_glyph = None;
        } else if phase == 2 {
            fonts
                .get_mut("primary")
                .unwrap()
                .glyph_map
                .insert('A', glyph(-999));
            fonts.get_mut("primary").unwrap().fallback_font_name = Some("missing");
        } else if phase == 3 {
            // ASCII width must read the existing table, including an explicit
            // override that has not been refreshed from the source glyph map.
            fonts.get_mut("primary").unwrap().ascii_glyphs[b'A' as usize] = Some(glyph(37));
        }
        if phase != 3 {
            font::refresh_chain_keys(&mut fonts);
        }
        compare_widths(&fonts, &texts);
    }
}

#[test]
fn text_width_matches_random_unicode_and_signed_advances() {
    let fonts = fonts();
    let alphabet = [
        'A',
        'B',
        ' ',
        '\0',
        '\n',
        '\u{00e9}',
        '\u{0301}',
        '\u{65e5}',
        '\u{1f3b5}',
        '\u{10ffff}',
    ];
    let mut seed = 137;
    for i in 0..500 {
        let text: String = (0..i % 129)
            .map(|_| alphabet[random(&mut seed) as usize % alphabet.len()])
            .collect();
        compare_widths(&fonts, &[text]);
    }
}

#[test]
fn text_width_preserves_integer_overflow_behavior() {
    let mut fonts = fonts();
    let font = fonts.get_mut("primary").unwrap();
    for (code, advance) in [(b'A', i32::MAX), (b'B', i32::MIN), (b'C', 1)] {
        font.ascii_glyphs[code as usize] = Some(glyph(advance));
    }
    let font = &fonts["primary"];
    for text in ["A", "B", "AA", "BB", "AC", "AB", "ABA", "BAC"] {
        let new = std::panic::catch_unwind(|| font::measure_line_width_logical(font, text, &fonts));
        let old =
            std::panic::catch_unwind(|| old_font::measure_line_width_logical(font, text, &fonts));
        assert_eq!(new.is_err(), old.is_err(), "text {text:?}");
        if let (Ok(new), Ok(old)) = (new, old) {
            assert_eq!(new, old, "text {text:?}");
        }
    }
}

#[test]
fn rendering_lookups_remain_allocation_free() {
    let fonts = fonts();
    let font = &fonts["primary"];
    let params = appearance::NoteAlphaParams {
        hidden: 1.0,
        sudden: 1.0,
        ..Default::default()
    };
    let cache = appearance::note_appearance_cache(1.0, 0.0, params);
    perf::assert_no_churn(|| {
        for i in 0..1024 {
            black_box(appearance::appearance_note_alpha_glow_cached(
                i as f32 / 4.0,
                &cache,
            ));
            black_box(font::measure_line_width_logical(
                font,
                black_box("Score 100.00%"),
                &fonts,
            ));
            black_box(font::measure_line_width_logical(
                font,
                black_box("caf\u{00e9} \u{65e5}"),
                &fonts,
            ));
        }
    });
}

#[test]
#[ignore = "manual release comparison; run serially with --nocapture"]
fn benchmark_render_math() {
    let versions = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    for (label, values, y0, span) in [
        (
            "hidden-fade",
            [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            120.0,
            39.9,
        ),
        (
            "sudden-fade",
            [0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0],
            160.0,
            39.9,
        ),
        (
            "hidden-wide",
            [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            0.0,
            320.0,
        ),
        (
            "sudden-wide",
            [0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0],
            0.0,
            320.0,
        ),
        ("combined", [1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0], 80.0, 160.0),
        ("general", [0.5, 0.0, 0.75, 0.0, 0.2, 0.5, 0.1], 80.0, 160.0),
        ("identity", [0.0; 7], 0.0, 320.0),
        ("stealth", [0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0], 0.0, 320.0),
        (
            "hidden-outside",
            [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            320.0,
            320.0,
        ),
        (
            "sudden-outside",
            [0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0],
            320.0,
            320.0,
        ),
    ] {
        let new = appearance::note_appearance_cache(1.25, 0.0, params!(appearance, values));
        let old = old_appearance::note_appearance_cache(1.25, 0.0, params!(old_appearance, values));
        let ys: Vec<_> = (0..512)
            .map(|i| y0 + (i % 127) as f32 / 127.0 * span)
            .collect();
        compare_appearance(1.25, 0.0, values, &ys);
        if matches!(
            label,
            "hidden-fade" | "sudden-fade" | "combined" | "general" | "identity"
        ) {
            for previous in versions {
                perf::measure_sampled(
                    &format!("cache/{label}/{}", if previous { "old" } else { "new" }),
                    4096,
                    32,
                    || {
                        if previous {
                            for _ in 0..32 {
                                black_box(old_appearance::note_appearance_cache(
                                    black_box(1.25),
                                    black_box(0.0),
                                    black_box(params!(old_appearance, values)),
                                ));
                            }
                        } else {
                            for _ in 0..32 {
                                black_box(appearance::note_appearance_cache(
                                    black_box(1.25),
                                    black_box(0.0),
                                    black_box(params!(appearance, values)),
                                ));
                            }
                        }
                    },
                );
            }
        }
        for previous in versions {
            perf::measure_sampled(
                &format!(
                    "visibility/{label}/{}",
                    if previous { "old" } else { "new" }
                ),
                1024,
                ys.len(),
                || {
                    if previous {
                        let old = black_box(&old);
                        for &y in black_box(&ys) {
                            black_box(old_appearance::appearance_note_alpha_cached(
                                black_box(y),
                                old,
                            ));
                        }
                    } else {
                        let new = black_box(&new);
                        for &y in black_box(&ys) {
                            black_box(appearance::appearance_note_alpha_cached(black_box(y), new));
                        }
                    }
                },
            );
        }
    }
    let fonts = fonts();
    let font = &fonts["primary"];
    for (label, text) in [
        ("empty", String::new()),
        ("single", "A".to_owned()),
        ("label", "Score 100.00%".to_owned()),
        ("ascii-64", "Step timing 0123".repeat(4)),
        ("ascii-1024", "Step timing 0123".repeat(64)),
        (
            "unicode",
            "caf\u{00e9} \u{65e5}\u{1f3b5} \u{2603}".repeat(8),
        ),
        (
            "mixed-long",
            format!("{}\u{65e5}", "Step timing 0123".repeat(64)),
        ),
    ] {
        assert_eq!(
            font::measure_line_width_logical(font, &text, &fonts),
            old_font::measure_line_width_logical(font, &text, &fonts)
        );
        for previous in versions {
            perf::measure_sampled(
                &format!("width/{label}/{}", if previous { "old" } else { "new" }),
                2048,
                32,
                || {
                    let font = black_box(font);
                    let fonts = black_box(&fonts);
                    let text = black_box(&text);
                    if previous {
                        for _ in 0..32 {
                            black_box(old_font::measure_line_width_logical(
                                black_box(font),
                                black_box(text),
                                black_box(fonts),
                            ));
                        }
                    } else {
                        for _ in 0..32 {
                            black_box(font::measure_line_width_logical(
                                black_box(font),
                                black_box(text),
                                black_box(fonts),
                            ));
                        }
                    }
                },
            );
        }
    }
}
