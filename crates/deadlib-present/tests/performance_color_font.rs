use deadlib_present::color::Color;
use deadlib_present::font::{self, Font, FontMap, Glyph};

#[test]
fn config_hex_preserves_channels_prefixes_and_invalid_inputs() {
    for byte in 0..=255_u32 {
        let rgba = [
            byte as f32 / 255.0,
            (255 - byte) as f32 / 255.0,
            17.0 / 255.0,
            128.0 / 255.0,
        ];
        for prefix in ["", "#", "###"] {
            for (text, alpha) in [
                (format!("{prefix}{byte:02x}{:02X}11", 255 - byte), 1.0),
                (format!("{prefix}80{byte:02X}{:02x}11", 255 - byte), rgba[3]),
            ] {
                for text in [text.clone(), format!("\u{2003}{text}\t\n")] {
                    assert_eq!(
                        Color::from_hex(&text).unwrap().to_rgba(),
                        [rgba[0], rgba[1], rgba[2], alpha]
                    );
                }
            }
        }
    }
    for text in [
        "",
        "#",
        "12345",
        "1234567",
        "123456789",
        "+12345",
        "+1234567",
        "-12345",
        "0x1234",
        "0X123456",
        "12 456",
        "12345g",
        "12\u{e9}34",
        "\u{ff11}2345",
        "# #123456",
        "1234\n56",
    ] {
        assert_eq!(Color::from_hex(text), None, "{text:?}");
    }
}

#[test]
fn config_hex_format_preserves_rounding_nonfinite_and_alpha_rules() {
    let values = [
        f32::NEG_INFINITY,
        -1.0,
        -0.0,
        0.0,
        0.5 / 255.0,
        1.5 / 255.0,
        0.125,
        0.5,
        254.49 / 255.0,
        254.5 / 255.0,
        1.0,
        2.0,
        f32::INFINITY,
        f32::NAN,
    ];
    for r in values {
        for g in values {
            for b in values {
                for a in values {
                    let color = Color::from_rgba([r, g, b, a]);
                    let channel = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                    let (r, g, b, a) = (channel(r), channel(g), channel(b), channel(a));
                    let expected = if a == 255 {
                        format!("#{r:02X}{g:02X}{b:02X}")
                    } else {
                        format!("#{a:02X}{r:02X}{g:02X}{b:02X}")
                    };
                    assert_eq!(color.to_hex(), expected);
                }
            }
        }
    }
}

fn glyph(id: i32) -> Glyph {
    Glyph {
        texture_key: format!("texture/{id}").into(),
        stroke_texture_key: Some(format!("stroke/{id}").into()),
        tex_rect: [1.0, 2.0, 3.0, 4.0],
        uv_scale: [0.5; 2],
        uv_offset: [0.25; 2],
        size: [8.0, 16.0],
        offset: [-1.0, 2.0],
        advance: id as f32,
        advance_i32: id,
    }
}

fn font(
    codes: impl Iterator<Item = u8>,
    id: i32,
    fallback: Option<&'static str>,
    default: bool,
) -> Font {
    Font {
        glyph_map: codes
            .map(|code| (char::from(code), glyph(id + i32::from(code))))
            .collect(),
        ascii_glyphs: Box::new(std::array::from_fn(|_| None)),
        default_glyph: default.then(|| glyph(id - 1)),
        line_spacing: 16,
        height: 16,
        fallback_font_name: fallback,
        cache_tag: 0,
        chain_key: 0,
        default_stroke_color: [0.0; 4],
        stroke_texture_map: Default::default(),
        texture_hints_map: Default::default(),
    }
}

fn check_ascii_tables(fonts: &mut FontMap) {
    font::refresh_chain_keys(fonts);
    for start in fonts.values() {
        for code in 0..128_u8 {
            let mut current = Some(start);
            let mut expected = start.default_glyph.as_ref();
            while let Some(font) = current {
                if let Some(glyph) = font.glyph_map.get(&char::from(code)) {
                    expected = Some(glyph);
                    break;
                }
                current = font.fallback_font_name.and_then(|name| fonts.get(name));
            }
            assert_eq!(
                format!("{:?}", font::find_glyph(start, char::from(code), fonts)),
                format!("{expected:?}")
            );
        }
    }
}

#[test]
fn ascii_cache_preserves_fallback_precedence_missing_fonts_and_refresh() {
    for default in [false, true] {
        let mut fonts = FontMap::from_iter([
            ("primary", font(32..96, 1000, Some("secondary"), default)),
            ("secondary", font(64..127, 2000, Some("last"), true)),
            ("last", font(0..16, 3000, Some("absent"), true)),
            ("empty", font(0..0, 4000, None, false)),
        ]);
        check_ascii_tables(&mut fonts);
        let key = fonts["primary"].chain_key;
        fonts.get_mut("primary").unwrap().glyph_map.clear();
        fonts.get_mut("secondary").unwrap().fallback_font_name = None;
        check_ascii_tables(&mut fonts);
        assert_ne!(fonts["primary"].chain_key, key);
    }
    // Complete cycles terminate as soon as every character has a glyph.
    let mut fonts = FontMap::from_iter([
        ("primary", font(0..64, 1000, Some("secondary"), true)),
        ("secondary", font(64..128, 2000, Some("primary"), false)),
    ]);
    check_ascii_tables(&mut fonts);
}
