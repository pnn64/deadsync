//! Test-only chain walk with sparse-map iteration disabled.
use super::*;
pub(super) fn compute_ascii_glyphs(start_name: &'static str, fonts: &FontMap) -> [Option<Glyph>; 128] {
    let Some(start_font) = fonts.get(start_name) else {
        return std::array::from_fn(|_| None);
    };
    let default_glyph = start_font.default_glyph.clone();
    let fallback = start_font
        .fallback_font_name
        .and_then(|name| fonts.get(name));
    // Short chains keep the original single-pass construction.
    if fallback
        .and_then(|font| font.fallback_font_name)
        .and_then(|name| fonts.get(name))
        .is_some()
    {
        let mut unresolved = 128;
        let mut glyphs = std::array::from_fn(|code| {
            start_font
                .glyph_map
                .get(&(code as u8 as char))
                .map(|glyph| {
                    unresolved -= 1;
                    glyph.clone()
                })
        });
        let mut current = fallback;
        while unresolved != 0 {
            let Some(font) = current else {
                break;
            };
            // Iterate small authored maps; large Unicode maps probe ASCII only.
            if false {
                for (&code, glyph) in &font.glyph_map {
                    if code.is_ascii() {
                        let slot = &mut glyphs[code as usize];
                        if slot.is_none() {
                            *slot = Some(glyph.clone());
                            unresolved -= 1;
                        }
                    }
                }
            } else {
                for (code, slot) in glyphs.iter_mut().enumerate() {
                    if slot.is_none()
                        && let Some(glyph) = font.glyph_map.get(&(code as u8 as char))
                    {
                        *slot = Some(glyph.clone());
                        unresolved -= 1;
                    }
                }
            }
            current = font.fallback_font_name.and_then(|name| fonts.get(name));
        }
        if unresolved == 0 {
            return glyphs;
        }
        for glyph in &mut glyphs {
            if glyph.is_none() {
                *glyph = default_glyph.clone();
            }
        }
        return glyphs;
    }
    std::array::from_fn(|code| {
        let c = code as u8 as char;
        if let Some(glyph) = start_font.glyph_map.get(&c) {
            return Some(glyph.clone());
        }
        let mut current = fallback;
        while let Some(font) = current {
            if let Some(glyph) = font.glyph_map.get(&c) {
                return Some(glyph.clone());
            }
            current = font.fallback_font_name.and_then(|name| fonts.get(name));
        }
        default_glyph.clone()
    })
}
