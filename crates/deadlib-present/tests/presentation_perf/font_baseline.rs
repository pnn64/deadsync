// Frozen from 9e6ab0553 (0.5.1140).
use deadlib_present::font::{FontMap, Glyph};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[inline(always)]
fn empty_ascii_glyphs() -> Box<[Option<Glyph>; 128]> {
    Box::new(std::array::from_fn(|_| None))
}

fn compute_chain_key(start_name: &'static str, fonts: &FontMap) -> u64 {
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
    let mut names = fonts.keys().copied().collect::<Vec<_>>();
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

    let chain_keys = names
        .iter()
        .map(|name| (*name, compute_chain_key(name, fonts)))
        .collect::<Vec<_>>();
    for (name, chain_key) in chain_keys {
        if let Some(font) = fonts.get_mut(name) {
            font.chain_key = chain_key;
        }
    }

    let ascii_tables = names
        .iter()
        .map(|name| (*name, compute_ascii_glyphs(name, fonts)))
        .collect::<Vec<_>>();
    for (name, ascii_glyphs) in ascii_tables {
        if let Some(font) = fonts.get_mut(name) {
            font.ascii_glyphs = ascii_glyphs;
        }
    }
}

fn compute_ascii_glyphs(start_name: &'static str, fonts: &FontMap) -> Box<[Option<Glyph>; 128]> {
    let Some(start_font) = fonts.get(start_name) else {
        return empty_ascii_glyphs();
    };
    let default_glyph = start_font.default_glyph.clone();
    Box::new(std::array::from_fn(|code| {
        let c = code as u8 as char;
        let mut current = Some(start_font);
        while let Some(font) = current {
            if let Some(glyph) = font.glyph_map.get(&c) {
                return Some(glyph.clone());
            }
            current = font.fallback_font_name.and_then(|name| fonts.get(name));
        }
        default_glyph.clone()
    }))
}
