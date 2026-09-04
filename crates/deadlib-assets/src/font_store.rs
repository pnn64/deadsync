use deadlib_present::font::{self, Font, FontMap};

pub struct FontStore {
    fonts: FontMap,
}

impl FontStore {
    #[must_use]
    pub fn new() -> Self {
        Self {
            fonts: FontMap::default(),
        }
    }

    pub fn register_font(&mut self, name: &'static str, mut font: Font) {
        font.cache_tag = 0;
        font.chain_key = 0;
        self.fonts.insert(name, font);
        font::refresh_chain_keys(&mut self.fonts);
    }

    pub fn register_fonts(&mut self, fonts: impl IntoIterator<Item = (&'static str, Font)>) {
        let fonts = fonts.into_iter();
        self.fonts.reserve(fonts.size_hint().0);
        let mut changed = false;
        for (name, mut font) in fonts {
            font.cache_tag = 0;
            font.chain_key = 0;
            self.fonts.insert(name, font);
            changed = true;
        }
        if changed {
            font::refresh_chain_keys(&mut self.fonts);
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn has_font(&self, name: &str) -> bool {
        self.fonts.contains_key(name)
    }

    #[must_use]
    pub const fn fonts(&self) -> &FontMap {
        &self.fonts
    }

    pub fn with_fonts<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&FontMap) -> R,
    {
        f(&self.fonts)
    }

    pub fn with_font<F, R>(&self, name: &str, f: F) -> Option<R>
    where
        F: FnOnce(&Font) -> R,
    {
        self.fonts.get(name).map(f)
    }
}

impl Default for FontStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::font::{Glyph, GlyphMap, find_glyph};
    use std::{collections::HashMap, sync::Arc};

    fn test_font() -> Font {
        Font {
            glyph_map: GlyphMap::default(),
            ascii_glyphs: Box::new(std::array::from_fn(|_| None)),
            default_glyph: None,
            line_spacing: 0,
            height: 0,
            fallback_font_name: None,
            cache_tag: 456,
            chain_key: 123,
            default_stroke_color: [0.0; 4],
            stroke_texture_map: HashMap::new(),
            texture_hints_map: HashMap::new(),
        }
    }

    fn glyph(advance_i32: i32) -> Glyph {
        Glyph {
            texture_key: Arc::from("test"),
            stroke_texture_key: None,
            tex_rect: [0.0; 4],
            uv_scale: [1.0; 2],
            uv_offset: [0.0; 2],
            size: [1.0; 2],
            offset: [0.0; 2],
            advance: advance_i32 as f32,
            advance_i32,
        }
    }

    #[test]
    fn register_font_refreshes_cache_state() {
        let mut store = FontStore::new();

        store.register_font("test", test_font());

        let font = store.fonts().get("test").unwrap();
        assert_ne!(font.cache_tag, 456);
        assert_ne!(font.chain_key, 123);
    }

    #[test]
    fn batch_registration_matches_immediate_fallback_resolution() {
        let mut primary = test_font();
        primary.fallback_font_name = Some("fallback");
        primary.default_glyph = Some(glyph(3));
        let mut fallback = test_font();
        fallback.glyph_map.insert('A', glyph(7));
        fallback.glyph_map.insert('\u{65e5}', glyph(11));

        let mut immediate = FontStore::new();
        immediate.register_font("primary", primary.clone());
        immediate.register_font("fallback", fallback.clone());
        let mut batched = FontStore::new();
        batched.register_fonts([("primary", primary), ("fallback", fallback)]);

        for name in ["primary", "fallback"] {
            let old = immediate.fonts().get(name).unwrap();
            let new = batched.fonts().get(name).unwrap();
            assert_ne!(new.cache_tag, 0);
            assert_ne!(new.chain_key, 0);
            for character in ['A', '\u{65e5}', '?'] {
                assert_eq!(
                    find_glyph(new, character, batched.fonts()).map(|glyph| glyph.advance_i32),
                    find_glyph(old, character, immediate.fonts()).map(|glyph| glyph.advance_i32),
                );
            }
        }
    }
}
