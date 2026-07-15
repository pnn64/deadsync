use deadlib_present::actors::{Actor, TextAlign, TextContent};
use deadlib_present::compose::TextLayoutCache;
use deadlib_present::dsl::TextBuilder;
use deadlib_present::font;
use std::collections::HashMap;

const MAX_SCORE_GLYPHS: usize = 11;
const SCORE_GLYPH_TEXT: [&str; 11] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "."];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScoreGlyphs {
    bytes: [u8; MAX_SCORE_GLYPHS],
    len: u8,
}

impl ScoreGlyphs {
    fn from_centi(centi: u32) -> Self {
        let whole = centi / 100;
        let fraction = centi % 100;
        let whole_digits = if whole == 0 {
            1
        } else {
            whole.ilog10() as usize + 1
        };
        let mut bytes = [0; MAX_SCORE_GLYPHS];
        let mut remaining = whole;
        for index in (0..whole_digits).rev() {
            bytes[index] = b'0' + (remaining % 10) as u8;
            remaining /= 10;
        }
        bytes[whole_digits] = b'.';
        bytes[whole_digits + 1] = b'0' + (fraction / 10) as u8;
        bytes[whole_digits + 2] = b'0' + (fraction % 10) as u8;
        Self {
            bytes,
            len: (whole_digits + 3) as u8,
        }
    }

    #[inline(always)]
    fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }

    #[cfg(test)]
    fn as_str(&self) -> &str {
        // Every byte is written from an ASCII digit or punctuation literal.
        std::str::from_utf8(self.as_bytes()).expect("score glyphs must remain ASCII")
    }
}

#[derive(Clone, Copy)]
pub struct ScoreCounterParams {
    pub value: f64,
    pub font: &'static str,
    pub position: [f32; 2],
    pub align: [f32; 2],
    pub text_align: TextAlign,
    pub zoom: f32,
    pub color: [f32; 4],
    pub z: i16,
}

#[inline(always)]
fn quantize_centi(value: f64) -> u32 {
    let value = if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    };
    ((value * 100.0).round()).clamp(0.0, u32::MAX as f64) as u32
}

#[inline(always)]
fn score_glyph_text(byte: u8) -> &'static str {
    match byte {
        b'0'..=b'9' => SCORE_GLYPH_TEXT[(byte - b'0') as usize],
        b'.' => SCORE_GLYPH_TEXT[10],
        _ => "",
    }
}

/// Append a fixed-two-decimal score as static digit and punctuation actors.
///
/// The font's authored integer advances determine every glyph position, so the
/// result matches a single BitmapText actor for proportional and monospace
/// number fonts. Source text has a fixed eleven-glyph domain and score changes
/// perform no string or `Arc<str>` allocation.
pub fn push_score_counter(
    actors: &mut Vec<Actor>,
    fonts: &HashMap<&'static str, font::Font>,
    params: ScoreCounterParams,
) {
    let Some(metrics_font) = fonts.get(params.font) else {
        return;
    };
    let glyphs = ScoreGlyphs::from_centi(quantize_centi(params.value));
    let logical_width = glyphs.as_bytes().iter().fold(0i32, |width, byte| {
        width.saturating_add(
            font::find_glyph(metrics_font, char::from(*byte), fonts)
                .map_or(0, |glyph| glyph.advance_i32),
        )
    });
    let block_width = logical_width.saturating_add(logical_width & 1);
    let width_padding = block_width.saturating_sub(logical_width);
    let line_padding = match params.text_align {
        TextAlign::Left => 0,
        TextAlign::Center => width_padding * ((block_width / 2) & 1),
        TextAlign::Right => width_padding,
    };
    let mut cursor_x = params.position[0] - params.align[0] * block_width as f32 * params.zoom
        + line_padding as f32 * params.zoom;

    actors.reserve(glyphs.as_bytes().len());
    for byte in glyphs.as_bytes() {
        let ch = char::from(*byte);
        let Some(glyph) = font::find_glyph(metrics_font, ch, fonts) else {
            continue;
        };
        let mut actor = TextBuilder::new();
        actor.font(params.font);
        actor.settext(TextContent::Static(score_glyph_text(*byte)));
        actor.align(0.0, params.align[1]);
        actor.horizalign(TextAlign::Left);
        actor.xy(cursor_x, params.position[1]);
        actor.zoom(params.zoom);
        actor.diffuse(params.color);
        actor.z(params.z);
        actors.push(actor.build(0));
        cursor_x += glyph.advance_i32 as f32 * params.zoom;
    }
}

pub fn prewarm_score_counter_layout(
    cache: &mut TextLayoutCache,
    fonts: &HashMap<&'static str, font::Font>,
    font_name: &'static str,
) {
    for glyph in SCORE_GLYPH_TEXT {
        cache.prewarm_text(fonts, font_name, glyph, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::compose::build_screen;
    use deadlib_present::font::{Font, Glyph};
    use deadlib_present::space::Metrics;
    use deadlib_render::ObjectType;
    use glam::Vec4;
    use std::sync::Arc;

    #[test]
    fn score_glyphs_match_fixed_two_decimal_formatting() {
        for (value, expected) in [
            (-1.0, "0.00"),
            (f64::NAN, "0.00"),
            (0.0, "0.00"),
            (0.005, "0.01"),
            (81.91, "81.91"),
            (100.0, "100.00"),
            (u32::MAX as f64 / 100.0, "42949672.95"),
        ] {
            assert_eq!(
                ScoreGlyphs::from_centi(quantize_centi(value)).as_str(),
                expected
            );
        }
    }

    #[test]
    fn segmented_score_matches_full_text_geometry_for_variable_width_font() {
        let fonts = test_fonts();
        for (align, text_align) in [
            ([0.0, 0.0], TextAlign::Left),
            ([0.5, 0.5], TextAlign::Center),
            ([1.0, 1.0], TextAlign::Right),
        ] {
            let params = ScoreCounterParams {
                value: 81.91,
                font: "numbers",
                position: [320.0, 240.0],
                align,
                text_align,
                zoom: 0.25,
                color: [0.5, 0.75, 1.0, 1.0],
                z: 90,
            };
            let mut segmented = Vec::new();
            push_score_counter(&mut segmented, &fonts, params);

            let mut full = TextBuilder::new();
            full.font(params.font);
            full.settext(TextContent::Static("81.91"));
            full.align(params.align[0], params.align[1]);
            full.horizalign(params.text_align);
            full.xy(params.position[0], params.position[1]);
            full.zoom(params.zoom);
            full.diffuse(params.color);
            full.z(params.z);

            let full_vertices = composed_vertices(&[full.build(0)], &fonts);
            let segmented_vertices = composed_vertices(&segmented, &fonts);
            assert_eq!(segmented_vertices.len(), full_vertices.len());
            for (actual, expected) in segmented_vertices.iter().zip(full_vertices) {
                for (actual, expected) in actual.iter().zip(expected) {
                    assert!((actual - expected).abs() <= 1e-5);
                }
            }
        }
    }

    #[test]
    fn score_prewarm_has_eleven_layouts() {
        let fonts = test_fonts();
        let mut cache = TextLayoutCache::new(64);
        cache.begin_frame_stats(true);
        prewarm_score_counter_layout(&mut cache, &fonts, "numbers");
        assert_eq!(cache.frame_stats().owned_entries, 11);
    }

    fn composed_vertices(actors: &[Actor], fonts: &HashMap<&'static str, Font>) -> Vec<[f32; 5]> {
        let metrics = Metrics {
            left: 0.0,
            right: 640.0,
            top: 480.0,
            bottom: 0.0,
        };
        build_screen(actors, [0.0, 0.0, 0.0, 1.0], &metrics, fonts, 0.0)
            .objects
            .into_iter()
            .flat_map(|object| match object.object_type {
                ObjectType::TexturedMesh {
                    instance, vertices, ..
                } => vertices
                    .iter()
                    .map(|vertex| {
                        let point = instance.transform()
                            * Vec4::new(vertex.pos[0], vertex.pos[1], vertex.pos[2], 1.0);
                        [point.x, point.y, point.z, vertex.uv[0], vertex.uv[1]]
                    })
                    .collect::<Vec<_>>(),
                _ => Vec::new(),
            })
            .collect()
    }

    fn test_fonts() -> HashMap<&'static str, Font> {
        let texture: Arc<str> = Arc::from("score-counter-test");
        let mut glyph_map = HashMap::new();
        for (index, ch) in "0123456789.".chars().enumerate() {
            let advance = if matches!(ch, '1' | '.') { 15 } else { 37 };
            glyph_map.insert(
                ch,
                Glyph {
                    texture_key: Arc::clone(&texture),
                    stroke_texture_key: None,
                    tex_rect: [0.0, 0.0, 1.0, 1.0],
                    uv_scale: [0.05, 1.0],
                    uv_offset: [index as f32 * 0.05, 0.0],
                    size: [advance as f32, 48.0],
                    offset: [0.0, -42.0],
                    advance: advance as f32,
                    advance_i32: advance,
                },
            );
        }
        let ascii_glyphs = Box::new(std::array::from_fn(|index| {
            char::from_u32(index as u32).and_then(|ch| glyph_map.get(&ch).cloned())
        }));
        HashMap::from([(
            "numbers",
            Font {
                glyph_map,
                ascii_glyphs,
                default_glyph: None,
                line_spacing: 48,
                height: 48,
                fallback_font_name: None,
                cache_tag: 1,
                chain_key: 1,
                default_stroke_color: [0.0; 4],
                stroke_texture_map: HashMap::new(),
                texture_hints_map: HashMap::new(),
            },
        )])
    }
}
