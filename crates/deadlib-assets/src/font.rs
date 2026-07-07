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

#[derive(Clone, Copy)]
pub struct FontAssetSpec {
    pub name: &'static str,
    pub ini_path: &'static str,
    pub fallback_font_name: Option<&'static str>,
}

pub struct ParsedFontAsset {
    pub name: &'static str,
    pub ini_path: &'static str,
    pub font: Font,
    pub required_textures: Vec<PathBuf>,
}

pub fn parse_font_with_asset_context(
    ini_path: &Path,
    asset_roots: Vec<PathBuf>,
) -> Result<FontLoadData, FontParseError> {
    let context = AssetFontTextureContext::new(asset_roots);
    font::parse_with_texture_context(&ini_path.to_string_lossy(), &context)
}

pub fn parse_font_with_asset_dirs(
    ini_path: &Path,
    data_dir: &Path,
    exe_dir: &Path,
) -> Result<FontLoadData, FontParseError> {
    parse_font_with_asset_context(ini_path, font_texture_asset_roots(data_dir, exe_dir))
}

pub fn parse_font_asset_specs(
    specs: impl IntoIterator<Item = FontAssetSpec>,
    asset_roots: &[PathBuf],
    resolve_asset_path: impl Fn(&str) -> PathBuf,
) -> Result<Vec<ParsedFontAsset>, FontParseError> {
    specs
        .into_iter()
        .map(|spec| {
            let resolved = resolve_asset_path(spec.ini_path);
            let FontLoadData {
                mut font,
                required_textures,
            } = parse_font_with_asset_context(&resolved, asset_roots.to_vec())?;
            set_font_fallback(&mut font, spec.fallback_font_name);
            Ok(ParsedFontAsset {
                name: spec.name,
                ini_path: spec.ini_path,
                font,
                required_textures,
            })
        })
        .collect()
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

pub fn prepare_required_font_textures(
    font: &Font,
    required_textures: &[PathBuf],
    asset_roots: &[PathBuf],
    has_texture_key: impl Fn(&str) -> bool,
) -> image::ImageResult<Vec<PreparedFontTexture>> {
    let mut prepared = Vec::new();
    for tex_path in required_textures {
        let key = font_texture_key(tex_path, asset_roots);
        if has_texture_key(&key) {
            continue;
        }
        prepared.push(prepare_font_texture(
            tex_path,
            &font.texture_hints_map,
            asset_roots,
        )?);
    }
    Ok(prepared)
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
    fn parse_font_asset_specs_accepts_empty_catalog() {
        let parsed = parse_font_asset_specs([], &[PathBuf::from("/data/assets")], |path| {
            PathBuf::from(path)
        })
        .unwrap();

        assert!(parsed.is_empty());
    }

    #[test]
    fn prepare_required_font_textures_skips_existing_keys() {
        let font = Font {
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
        let roots = vec![PathBuf::from("/data/assets")];
        let required = vec![PathBuf::from("/data/assets/fonts/missing.png")];

        let prepared = prepare_required_font_textures(&font, &required, &roots, |key| {
            key == "fonts/missing.png"
        })
        .unwrap();

        assert!(prepared.is_empty());
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
