use crate::assets::AssetManager;
pub(crate) use deadlib_assets::generated_texture;
pub use deadlib_assets::{
    TexMeta, TextureChoice, TextureHints, open_image_fallback, parse_sprite_sheet_dims,
    parse_texture_hints, register_generated_texture, register_texture_dims, sprite_sheet_dims,
    strip_sprite_hints, texture_dims, texture_handle, texture_registry_generation,
    texture_source_dims_from_real, texture_source_frame_dims_from_real,
};
use deadlib_platform::dirs;
use deadlib_render::SamplerDesc;
use deadlib_renderer::Backend;
use log::{debug, warn};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use super::AssetError;
use deadlib_assets::{
    BuiltinTextureImage, GraphicTextureChoiceCache, INITIAL_GRAPHIC_TEXTURES, TextureDecodeResult,
    black_texture_image, canonical_texture_key_with_asset_roots, decode_texture_image,
    decode_texture_jobs_parallel, fallback_texture_image, graphic_texture_roots,
    initial_texture_decode_jobs, initial_texture_sampler, white_texture_image,
};

static GRAPHIC_TEXTURE_CHOICES: GraphicTextureChoiceCache = GraphicTextureChoiceCache::new();

#[inline(always)]
fn needs_repeat_sampler(key: &str) -> bool {
    deadsync_theme::texture_needs_repeat_sampler(key)
}

fn graphics_roots(folder: &str) -> Vec<PathBuf> {
    let dirs = dirs::app_dirs();
    graphic_texture_roots(folder, dirs.portable, &dirs.data_dir, &dirs.exe_dir)
}

pub fn judgment_texture_choices() -> &'static [TextureChoice] {
    GRAPHIC_TEXTURE_CHOICES.judgment_texture_choices(graphics_roots)
}

pub fn hold_judgment_texture_choices() -> &'static [TextureChoice] {
    GRAPHIC_TEXTURE_CHOICES.hold_judgment_texture_choices(graphics_roots)
}

pub fn held_miss_texture_choices() -> &'static [TextureChoice] {
    GRAPHIC_TEXTURE_CHOICES.held_miss_texture_choices(graphics_roots)
}

pub fn resolve_texture_choice<'a>(
    requested: Option<&str>,
    choices: &'a [TextureChoice],
) -> Option<&'a str> {
    deadlib_assets::resolve_texture_choice_key(requested, choices)
}

pub fn resolve_texture_choice_entry<'a>(
    requested: Option<&str>,
    choices: &'a [TextureChoice],
) -> Option<&'a TextureChoice> {
    deadlib_assets::resolve_texture_choice_entry(requested, choices)
}

pub fn canonical_texture_key<P: AsRef<Path>>(p: P) -> String {
    let dirs = dirs::app_dirs();
    canonical_texture_key_with_asset_roots(
        p.as_ref(),
        [dirs.data_dir.join("assets"), dirs.exe_dir.join("assets")],
    )
}

impl AssetManager {
    pub fn load_initial_textures(&mut self, backend: &mut Backend) -> Result<(), AssetError> {
        debug!("Loading initial textures...");

        for BuiltinTextureImage { key, image } in [white_texture_image(), black_texture_image()] {
            let texture = backend.create_texture(&image, SamplerDesc::default())?;
            self.insert_texture(key.to_string(), texture, image.width(), image.height());
            register_texture_dims(key, image.width(), image.height());
            debug!("Loaded built-in texture: {key}");
        }

        let texture_assets = deadsync_theme::initial_texture_assets()
            .map(|asset| (asset.key.to_string(), asset.path.to_string()))
            .collect::<Vec<_>>();
        let texture_jobs = initial_texture_decode_jobs(
            texture_assets,
            &dirs::app_dirs().noteskin_roots(),
            |path| canonical_texture_key(path),
            &INITIAL_GRAPHIC_TEXTURES,
            graphics_roots,
            |path| dirs::app_dirs().resolve_asset_path(path),
        );

        let fallback_image = Arc::new(fallback_texture_image());
        for result in decode_texture_jobs_parallel(texture_jobs) {
            match result {
                TextureDecodeResult::Decoded { key, image } => {
                    let sampler = initial_texture_sampler(&key, needs_repeat_sampler(&key));
                    let texture = backend.create_texture(&image, sampler)?;
                    register_texture_dims(&key, image.width(), image.height());
                    debug!("Loaded texture: {key}");
                    self.insert_texture(key, texture, image.width(), image.height());
                }
                TextureDecodeResult::Failed { key, message } => {
                    warn!("Failed to load texture for key '{key}': {message}. Using fallback.");
                    let sampler = initial_texture_sampler(&key, needs_repeat_sampler(&key));
                    let texture = backend.create_texture(&fallback_image, sampler)?;
                    register_texture_dims(&key, fallback_image.width(), fallback_image.height());
                    self.insert_texture(
                        key,
                        texture,
                        fallback_image.width(),
                        fallback_image.height(),
                    );
                }
            }
        }

        Ok(())
    }

    pub(crate) fn ensure_texture_for_key(&mut self, backend: &mut Backend, texture_key: &str) {
        self.load_texture_key(backend, texture_key, None, false);
    }

    pub(crate) fn ensure_texture_for_key_with_sampler(
        &mut self,
        backend: &mut Backend,
        texture_key: &str,
        sampler: SamplerDesc,
    ) {
        self.load_texture_key(backend, texture_key, Some(sampler), true);
    }

    fn load_texture_key(
        &mut self,
        backend: &mut Backend,
        texture_key: &str,
        sampler_override: Option<SamplerDesc>,
        force_reload: bool,
    ) {
        if texture_key.is_empty() {
            return;
        }
        let key = canonical_texture_key(texture_key);
        if !force_reload && self.has_texture_key(&key) {
            return;
        }
        if let Some(generated) = generated_texture(&key) {
            let sampler = sampler_override.unwrap_or(generated.sampler);
            match backend.create_texture(generated.image.as_ref(), sampler) {
                Ok(texture) => {
                    self.set_texture_for_key(
                        backend,
                        key,
                        texture,
                        generated.image.width(),
                        generated.image.height(),
                    );
                }
                Err(e) => {
                    warn!("Failed to create GPU texture for generated key '{texture_key}': {e}");
                }
            }
            return;
        }
        if key.starts_with("__") {
            return;
        }

        let path = deadlib_assets::texture_key_source_path(texture_key, &key, |path| {
            dirs::app_dirs().resolve_asset_path(path)
        });
        if !path.is_file() {
            warn!("Failed to resolve texture key '{key}' for preload.");
            return;
        }

        let hints = parse_texture_hints(&key);
        let sampler = sampler_override.unwrap_or_else(|| {
            deadlib_assets::texture_key_sampler(&hints, needs_repeat_sampler(&key))
        });
        match decode_texture_image(&path, &hints) {
            Ok(rgba) => match backend.create_texture(&rgba, sampler) {
                Ok(texture) => {
                    self.set_texture_for_key(
                        backend,
                        key.clone(),
                        texture,
                        rgba.width(),
                        rgba.height(),
                    );
                    register_texture_dims(&key, rgba.width(), rgba.height());
                }
                Err(e) => {
                    warn!("Failed to create GPU texture for key '{key}': {e}");
                }
            },
            Err(e) => {
                warn!("Failed to open texture for key '{key}': {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goldstar_uses_repeat_sampler() {
        assert!(needs_repeat_sampler("grades/goldstar (stretch).png"));
    }
}
