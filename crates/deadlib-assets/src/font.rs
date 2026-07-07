use crate::{
    TextureHints, canonical_texture_key_with_asset_roots, decode_texture_image,
    parse_sprite_sheet_dims, parse_texture_hints,
};
use deadlib_present::font::{self, Font, FontLoadData, FontParseError};
use image::RgbaImage;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

pub struct AssetFontTextureContext {
    asset_roots: Vec<PathBuf>,
}

impl AssetFontTextureContext {
    pub fn new(asset_roots: Vec<PathBuf>) -> Self {
        Self { asset_roots }
    }
}

impl font::FontTextureContext for AssetFontTextureContext {
    fn canonical_texture_key(&self, path: &Path) -> String {
        canonical_texture_key_with_asset_roots(path, self.asset_roots.iter().cloned())
    }

    fn sprite_sheet_dims(&self, key: &str) -> (u32, u32) {
        parse_sprite_sheet_dims(key)
    }

    fn texture_hint_is_default(&self, raw: &str) -> bool {
        parse_texture_hints(raw).is_default()
    }

    fn texture_hint_doubleres(&self, raw: &str) -> bool {
        parse_texture_hints(raw).doubleres
    }
}

pub struct PreparedFontTexture {
    pub key: String,
    pub image: RgbaImage,
    pub hints: TextureHints,
}

pub fn parse_font_with_asset_context(
    ini_path: &Path,
    asset_roots: Vec<PathBuf>,
) -> Result<FontLoadData, FontParseError> {
    let context = AssetFontTextureContext::new(asset_roots);
    font::parse_with_texture_context(&ini_path.to_string_lossy(), &context)
}

pub fn font_texture_key(tex_path: &Path, asset_roots: &[PathBuf]) -> String {
    canonical_texture_key_with_asset_roots(tex_path, asset_roots.iter().cloned())
}

pub fn prepare_font_texture(
    tex_path: &Path,
    texture_hints_map: &HashMap<String, String>,
    asset_roots: &[PathBuf],
) -> image::ImageResult<PreparedFontTexture> {
    let key = font_texture_key(tex_path, asset_roots);
    let hints = texture_hints_map
        .get(&key)
        .map(|s| parse_texture_hints(s))
        .unwrap_or_default();
    let image = decode_texture_image(tex_path, &hints)?;
    Ok(PreparedFontTexture { key, image, hints })
}

pub fn font_texture_asset_roots(data_dir: &Path, exe_dir: &Path) -> Vec<PathBuf> {
    vec![data_dir.join("assets"), exe_dir.join("assets")]
}

pub fn set_font_fallback(font: &mut Font, fallback_font_name: Option<&'static str>) {
    if let Some(fallback) = fallback_font_name {
        font.fallback_font_name = Some(fallback);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_texture_asset_roots_include_data_and_exe_assets() {
        let roots = font_texture_asset_roots(Path::new("/data"), Path::new("/exe"));

        assert_eq!(
            roots,
            [PathBuf::from("/data/assets"), PathBuf::from("/exe/assets")]
        );
    }

    #[test]
    fn font_texture_key_strips_known_asset_roots() {
        let roots = vec![PathBuf::from("/data/assets"), PathBuf::from("/exe/assets")];

        assert_eq!(
            font_texture_key(Path::new("/data/assets/fonts/foo.png"), &roots),
            "fonts/foo.png"
        );
    }

    #[test]
    fn set_font_fallback_applies_present_fallback_name() {
        let mut font = Font {
            glyph_map: HashMap::new(),
            ascii_glyphs: Box::new(std::array::from_fn(|_| None)),
            default_glyph: None,
            line_spacing: 0,
            height: 0,
            fallback_font_name: None,
            cache_tag: 0,
            chain_key: 0,
            default_stroke_color: [0.0; 4],
            stroke_texture_map: HashMap::new(),
            texture_hints_map: HashMap::new(),
        };

        set_font_fallback(&mut font, Some("miso"));

        assert_eq!(font.fallback_font_name, Some("miso"));
    }
}
