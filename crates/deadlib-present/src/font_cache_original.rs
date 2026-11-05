//! Frozen font kernels from 74a02c1808bd693fe4fcdab41ae7b26be339c54d.
use super::*;
pub(crate) fn compute_chain_key(start_name: &'static str, fonts: &FontMap) -> u64 {
    let mut hasher = DefaultHasher::new();
    let mut current = Some(start_name);
    let mut depth = 0usize;
    let max_depth = fonts.len().max(1);

    while let Some(name) = current {
        name.hash(&mut hasher);
        let Some(font) = fonts.get(name) else {
            break;
        };
        font.cache_tag.hash(&mut hasher);
        current = font.fallback_font_name;
        depth += 1;
        if depth > max_depth {
            break;
        }
    }

    let key = hasher.finish();
    if key == 0 { 1 } else { key }
}
pub fn refresh_chain_keys(fonts: &mut FontMap) {
    let mut names = SmallVec::<[&'static str; 32]>::with_capacity(fonts.len());
    names.extend(fonts.keys().copied());
    names.sort_unstable();

    let mut next_tag = fonts
        .values()
        .map(|font| font.cache_tag)
        .max()
        .unwrap_or(0)
        .wrapping_add(1)
        .max(1);

    for name in &names {
        let Some(font) = fonts.get_mut(name) else {
            continue;
        };
        if font.cache_tag == 0 {
            font.cache_tag = next_tag;
            next_tag = next_tag.wrapping_add(1).max(1);
        }
    }

    // Neither computation reads another font's derived caches, so refresh one
    // font at a time and retain its existing ASCII table allocation.
    for name in names {
        let chain_key = compute_chain_key(name, fonts);
        let ascii_glyphs = compute_ascii_glyphs(name, fonts);
        if let Some(font) = fonts.get_mut(name) {
            font.chain_key = chain_key;
            *font.ascii_glyphs = ascii_glyphs;
        }
    }
}
pub(crate) fn compute_ascii_glyphs(start_name: &'static str, fonts: &FontMap) -> [Option<Glyph>; 128] {
    let Some(start_font) = fonts.get(start_name) else {
        return std::array::from_fn(|_| None);
    };
    let default_glyph = start_font.default_glyph.clone();
    let fallback = start_font
        .fallback_font_name
        .and_then(|name| fonts.get(name));
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
#[inline(always)]
#[must_use]
pub(crate) fn round_half_to_even_i32(v: f32) -> i32 {
    if !v.is_finite() {
        return 0;
    }
    let floor = v.floor();
    let frac = v - floor;
    if frac < 0.5 {
        floor as i32
    } else if frac > 0.5 {
        (floor + 1.0) as i32
    } else {
        let f = floor as i32;
        if (f & 1) == 0 { f } else { f + 1 }
    }
}
