use deadlib_assets::{
    AssetManager, PreparedFontTexture, font_texture_asset_roots, parse_font_asset_specs,
    parse_font_with_asset_dirs, prepare_required_font_textures,
};
use deadlib_platform::dirs;
use deadlib_present::font::Font;
use deadlib_render::Backend;
use log::debug;
use std::path::Path;

fn upload_font_textures(
    assets: &mut AssetManager,
    backend: &mut Backend,
    font: &Font,
    required_textures: &[std::path::PathBuf],
) -> Result<(), deadlib_assets::AssetError> {
    let dirs = dirs::app_dirs();
    let asset_roots = font_texture_asset_roots(&dirs.data_dir, &dirs.exe_dir);
    let textures = prepare_required_font_textures(font, required_textures, &asset_roots, |key| {
        assets.has_texture_key(key)
    })?;
    for PreparedFontTexture { key, image, hints } in textures {
        assets.update_texture_for_key_with_sampler(backend, &key, &image, hints.sampler_desc())?;
        debug!("Loaded font texture: {key}");
    }
    Ok(())
}

pub fn load_font_from_ini_path(
    assets: &mut AssetManager,
    backend: &mut Backend,
    name: &'static str,
    ini_path: &Path,
) -> Result<(), deadlib_assets::AssetError> {
    if assets.has_font(name) {
        return Ok(());
    }
    let dirs = dirs::app_dirs();
    let deadlib_present::font::FontLoadData {
        font,
        required_textures,
    } = parse_font_with_asset_dirs(ini_path, &dirs.data_dir, &dirs.exe_dir)?;
    upload_font_textures(assets, backend, &font, &required_textures)?;
    assets.register_font(name, font);
    debug!("Loaded font '{name}' from '{}'", ini_path.display());
    Ok(())
}

pub fn load_initial_fonts(
    assets: &mut AssetManager,
    backend: &mut Backend,
    fonts: &'static [deadlib_assets::FontAssetSpec],
) -> Result<(), deadlib_assets::AssetError> {
    let dirs = dirs::app_dirs();
    let asset_roots = font_texture_asset_roots(&dirs.data_dir, &dirs.exe_dir);
    let parsed = parse_font_asset_specs(fonts.iter().copied(), &asset_roots, |path| {
        dirs.resolve_asset_path(path)
    })?;
    let mut font_batch = Vec::with_capacity(parsed.len());
    for asset in parsed {
        if let Some(fallback) = asset.font.fallback_font_name {
            debug!(
                "Font '{}' configured to use '{}' as fallback.",
                asset.name, fallback
            );
        }
        upload_font_textures(assets, backend, &asset.font, &asset.required_textures)?;
        font_batch.push((asset.name, asset.font));
        debug!("Loaded font '{}' from '{}'", asset.name, asset.ini_path);
    }
    assets.register_fonts(font_batch);
    Ok(())
}
