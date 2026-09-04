use crate::{
    TextureHints, canonical_texture_key_with_asset_roots, decode_texture_image,
    parse_sprite_sheet_dims, parse_texture_hints, texture_hint_doubleres, texture_hint_is_default,
};
use deadlib_present::font::{self, Font, FontLoadData, FontParseError};
use image::RgbaImage;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

struct BorrowedAssetFontTextureContext<'a> {
    asset_roots: &'a [PathBuf],
}

impl font::FontTextureContext for BorrowedAssetFontTextureContext<'_> {
    fn canonical_texture_key(&self, path: &Path) -> String {
        canonical_texture_key_with_asset_roots(path, self.asset_roots)
    }

    fn sprite_sheet_dims(&self, key: &str) -> (u32, u32) {
        parse_sprite_sheet_dims(key)
    }

    fn texture_hint_is_default(&self, raw: &str) -> bool {
        texture_hint_is_default(raw)
    }

    fn texture_hint_doubleres(&self, raw: &str) -> bool {
        texture_hint_doubleres(raw)
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
    asset_roots: impl AsRef<[PathBuf]>,
) -> Result<FontLoadData, FontParseError> {
    let context = BorrowedAssetFontTextureContext {
        asset_roots: asset_roots.as_ref(),
    };
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
            } = parse_font_with_asset_context(&resolved, asset_roots)?;
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

#[must_use]
pub fn font_texture_key(tex_path: &Path, asset_roots: &[PathBuf]) -> String {
    canonical_texture_key_with_asset_roots(tex_path, asset_roots)
}

fn prepare_font_texture_with_key(
    tex_path: &Path,
    key: String,
    texture_hints_map: &HashMap<String, String>,
) -> image::ImageResult<PreparedFontTexture> {
    let hints = texture_hints_map
        .get(&key)
        .map(|s| parse_texture_hints(s))
        .unwrap_or_default();
    let image = decode_texture_image(tex_path, &hints)?;
    Ok(PreparedFontTexture { key, image, hints })
}

pub fn prepare_font_texture(
    tex_path: &Path,
    texture_hints_map: &HashMap<String, String>,
    asset_roots: &[PathBuf],
) -> image::ImageResult<PreparedFontTexture> {
    let key = font_texture_key(tex_path, asset_roots);
    prepare_font_texture_with_key(tex_path, key, texture_hints_map)
}

fn prepare_required_font_textures_with<T, E>(
    required_textures: &[PathBuf],
    asset_roots: &[PathBuf],
    has_texture_key: impl Fn(&str) -> bool,
    mut prepare: impl FnMut(&Path, String) -> Result<T, E>,
) -> Result<Vec<T>, E> {
    let mut prepared = Vec::new();
    for tex_path in required_textures {
        let key = font_texture_key(tex_path, asset_roots);
        if has_texture_key(&key) {
            continue;
        }
        prepared.push(prepare(tex_path, key)?);
    }
    Ok(prepared)
}

pub fn prepare_required_font_textures(
    font: &Font,
    required_textures: &[PathBuf],
    asset_roots: &[PathBuf],
    has_texture_key: impl Fn(&str) -> bool,
) -> image::ImageResult<Vec<PreparedFontTexture>> {
    prepare_required_font_textures_with(
        required_textures,
        asset_roots,
        has_texture_key,
        |tex_path, key| prepare_font_texture_with_key(tex_path, key, &font.texture_hints_map),
    )
}

#[must_use]
pub fn font_texture_asset_roots(data_dir: &Path, exe_dir: &Path) -> Vec<PathBuf> {
    vec![data_dir.join("assets"), exe_dir.join("assets")]
}

pub const fn set_font_fallback(font: &mut Font, fallback_font_name: Option<&'static str>) {
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

        let path = Path::new("/data/assets/fonts/foo.png");
        let borrowed = font_texture_key(path, &roots);

        assert_eq!(borrowed, "fonts/foo.png");
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
            glyph_map: font::GlyphMap::default(),
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
    fn required_font_texture_preparation_carries_selected_keys_in_order() {
        let roots = vec![PathBuf::from("/data/assets")];
        let required = vec![
            PathBuf::from("/data/assets/fonts/first.png"),
            PathBuf::from("/data/assets/fonts/cached.png"),
            PathBuf::from("/data/assets/fonts/last.png"),
        ];

        let prepared = prepare_required_font_textures_with(
            &required,
            &roots,
            |key| key == "fonts/cached.png",
            |path, key| Ok::<_, std::convert::Infallible>((path.to_path_buf(), key)),
        )
        .unwrap();

        assert_eq!(
            prepared,
            [
                (required[0].clone(), "fonts/first.png".to_string()),
                (required[2].clone(), "fonts/last.png".to_string()),
            ]
        );
    }

    #[test]
    fn set_font_fallback_applies_present_fallback_name() {
        let mut font = Font {
            glyph_map: font::GlyphMap::default(),
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
