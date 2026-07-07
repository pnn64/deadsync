use crate::assets::AssetManager;
pub(crate) use deadlib_assets::generated_texture;
pub use deadlib_assets::{
    TexMeta, TextureHints, apply_texture_hints, fix_hidden_alpha, open_image_fallback,
    parse_sprite_sheet_dims, parse_texture_hints, register_generated_texture,
    register_texture_dims, sprite_sheet_dims, strip_sprite_hints, texture_dims, texture_handle,
    texture_registry_generation, texture_source_dims_from_real,
    texture_source_frame_dims_from_real,
};
use deadlib_platform::dirs;
use deadlib_present::actors::TextureKeyHandle;
use deadlib_present::texture as present_texture;
use deadlib_render::{INVALID_TEXTURE_HANDLE, SamplerDesc};
use deadlib_renderer::Backend;
use log::{debug, warn};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

use super::{AssetError, PRESENT_TEXTURE_CONTEXT};
use deadlib_assets::{
    BuiltinTextureImage, DiscoveredTexture, GraphicTextureDiscovery, TextureChoiceLike,
    TextureChoiceSpec, TextureDecodeResult, black_texture_image,
    canonical_texture_key_with_asset_roots, decode_texture_image, decode_texture_jobs_parallel,
    discover_graphic_textures_in_roots, fallback_texture_image, graphic_texture_roots,
    initial_texture_decode_jobs, initial_texture_sampler,
    texture_choices_from_discovered as texture_choice_specs_from_discovered, white_texture_image,
};

pub struct TextureChoice {
    pub key: Arc<str>,
    pub label: String,
    cached_handle: AtomicU64,
    cached_generation: AtomicU64,
}

impl TextureChoice {
    fn new(key: String, label: String) -> Self {
        Self {
            key: Arc::from(key),
            label,
            cached_handle: AtomicU64::new(INVALID_TEXTURE_HANDLE),
            cached_generation: AtomicU64::new(u64::MAX),
        }
    }

    #[inline(always)]
    pub fn texture_key_handle(&self) -> TextureKeyHandle {
        present_texture::cached_texture_key_handle(
            &self.key,
            &self.cached_handle,
            &self.cached_generation,
            &PRESENT_TEXTURE_CONTEXT,
        )
    }
}

impl Clone for TextureChoice {
    fn clone(&self) -> Self {
        Self {
            key: Arc::clone(&self.key),
            label: self.label.clone(),
            cached_handle: AtomicU64::new(self.cached_handle.load(Ordering::Relaxed)),
            cached_generation: AtomicU64::new(self.cached_generation.load(Ordering::Relaxed)),
        }
    }
}

impl core::fmt::Debug for TextureChoice {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TextureChoice")
            .field("key", &self.key)
            .field("label", &self.label)
            .finish()
    }
}

impl PartialEq for TextureChoice {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.label == other.label
    }
}

impl Eq for TextureChoice {}

impl TextureChoiceLike for TextureChoice {
    fn key(&self) -> &str {
        self.key.as_ref()
    }
}

static JUDGMENT_TEXTURE_CHOICES: OnceLock<Vec<TextureChoice>> = OnceLock::new();
static HOLD_JUDGMENT_TEXTURE_CHOICES: OnceLock<Vec<TextureChoice>> = OnceLock::new();
static HELD_MISS_TEXTURE_CHOICES: OnceLock<Vec<TextureChoice>> = OnceLock::new();
const INITIAL_GRAPHIC_TEXTURES: [GraphicTextureDiscovery; 3] = [
    GraphicTextureDiscovery {
        folder: "judgements",
        love_first: true,
        require_multiframe_hint: true,
    },
    GraphicTextureDiscovery {
        folder: "hold_judgements",
        love_first: false,
        require_multiframe_hint: true,
    },
    GraphicTextureDiscovery {
        folder: "held_miss",
        love_first: false,
        require_multiframe_hint: false,
    },
];

#[inline(always)]
fn needs_repeat_sampler(key: &str) -> bool {
    deadsync_theme::texture_needs_repeat_sampler(key)
}

fn graphics_roots(folder: &str) -> Vec<PathBuf> {
    let dirs = dirs::app_dirs();
    graphic_texture_roots(folder, dirs.portable, &dirs.data_dir, &dirs.exe_dir)
}

fn discover_graphic_textures(
    folder: &str,
    love_first: bool,
    require_multiframe_hint: bool,
) -> Vec<DiscoveredTexture> {
    discover_graphic_textures_in_roots(
        folder,
        graphics_roots(folder),
        love_first,
        require_multiframe_hint,
    )
}

fn texture_choices_from_discovered(
    folder: &str,
    love_first: bool,
    include_none: bool,
    require_multiframe_hint: bool,
) -> Vec<TextureChoice> {
    let discovered = discover_graphic_textures(folder, love_first, require_multiframe_hint);
    texture_choice_specs_from_discovered(discovered, include_none)
        .into_iter()
        .map(|TextureChoiceSpec { key, label }| TextureChoice::new(key, label))
        .collect()
}

pub fn judgment_texture_choices() -> &'static [TextureChoice] {
    JUDGMENT_TEXTURE_CHOICES
        .get_or_init(|| texture_choices_from_discovered("judgements", true, true, true))
        .as_slice()
}

pub fn hold_judgment_texture_choices() -> &'static [TextureChoice] {
    HOLD_JUDGMENT_TEXTURE_CHOICES
        .get_or_init(|| texture_choices_from_discovered("hold_judgements", false, true, true))
        .as_slice()
}

pub fn held_miss_texture_choices() -> &'static [TextureChoice] {
    HELD_MISS_TEXTURE_CHOICES
        .get_or_init(|| texture_choices_from_discovered("held_miss", false, true, false))
        .as_slice()
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
