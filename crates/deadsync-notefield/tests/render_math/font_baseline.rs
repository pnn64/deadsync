// Frozen from 0.5.1217 (d77a218a1).
use deadlib_present::font::{Font, FontMap, Glyph};

#[must_use]
pub fn find_glyph<'a>(start_font: &'a Font, c: char, all_fonts: &'a FontMap) -> Option<&'a Glyph> {
    if c.is_ascii() {
        return start_font.ascii_glyphs[c as usize].as_ref();
    }
    if start_font.fallback_font_name.is_none() {
        return start_font
            .glyph_map
            .get(&c)
            .or(start_font.default_glyph.as_ref());
    }

    let mut current_font = Some(start_font);
    while let Some(font) = current_font {
        // Check the current font's glyph map.
        if let Some(glyph) = font.glyph_map.get(&c) {
            return Some(glyph);
        }
        // If not found, move to the next font in the chain.
        current_font = font.fallback_font_name.and_then(|name| all_fonts.get(name));
    }
    // If the character was not found in any font in the chain,
    // return the default glyph of the *original* starting font.
    start_font.default_glyph.as_ref()
}

/// `StepMania` parity: calculates the logical width of a line by summing the integer advances.
#[inline(always)]
#[must_use]
pub fn measure_line_width_logical(font: &Font, text: &str, all_fonts: &FontMap) -> i32 {
    text.chars()
        .map(|c| find_glyph(font, c, all_fonts).map_or(0, |glyph| glyph.advance_i32))
        .sum()
}
