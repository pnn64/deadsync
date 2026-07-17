use deadlib_present::font::{self, Font, FontMap};

pub struct FontStore {
    fonts: FontMap,
}

impl FontStore {
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

    #[inline(always)]
    pub fn has_font(&self, name: &str) -> bool {
        self.fonts.contains_key(name)
    }

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
    use std::collections::HashMap;

    fn test_font() -> Font {
        Font {
            glyph_map: HashMap::new(),
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

    #[test]
    fn register_font_refreshes_cache_state() {
        let mut store = FontStore::new();

        store.register_font("test", test_font());

        let font = store.fonts().get("test").unwrap();
        assert_ne!(font.cache_tag, 456);
        assert_ne!(font.chain_key, 123);
    }
}
