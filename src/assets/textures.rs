use crate::assets::AssetManager;
pub(crate) use deadlib_assets::generated_texture;
pub use deadlib_assets::{
    TexMeta, TextureHints, apply_texture_hints, direct_texture_key_path, fix_hidden_alpha,
    open_image_fallback, parse_sprite_sheet_dims, parse_texture_hints, register_generated_texture,
    register_texture_dims, sprite_sheet_dims, strip_sprite_hints, texture_dims, texture_handle,
    texture_registry_generation, texture_source_dims_from_real,
    texture_source_frame_dims_from_real,
};
use deadlib_platform::dirs;
use deadlib_present::actors::TextureKeyHandle;
use deadlib_present::texture as present_texture;
use deadlib_render::{INVALID_TEXTURE_HANDLE, SamplerDesc, SamplerWrap};
use deadlib_renderer::Backend;
use image::RgbaImage;
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
    DiscoveredTexture, NONE_TEXTURE_CHOICE_KEY, TextureChoiceLike, TextureDecodeJob,
    TextureDecodeResult, canonical_texture_key_with_asset_roots, decode_texture_jobs_parallel,
    discover_graphic_textures_in_roots, graphic_texture_roots, initial_texture_source_path,
    noteskin_png_texture_entries,
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
    let mut choices: Vec<TextureChoice> =
        discover_graphic_textures(folder, love_first, require_multiframe_hint)
            .into_iter()
            .map(|texture| TextureChoice::new(texture.key, texture.label))
            .collect();
    if include_none {
        choices.push(TextureChoice::new(
            NONE_TEXTURE_CHOICE_KEY.to_string(),
            NONE_TEXTURE_CHOICE_KEY.to_string(),
        ));
    }
    choices
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

pub(crate) fn append_noteskins_pngs_recursive(list: &mut Vec<(String, String)>, folder: &str) {
    let roots = dirs::app_dirs().noteskin_roots();
    list.extend(noteskin_png_texture_entries(&roots, folder, |path| {
        canonical_texture_key(path)
    }));
}

fn append_graphic_textures(
    list: &mut Vec<(String, String)>,
    folder: &str,
    love_first: bool,
    require_multiframe_hint: bool,
) {
    for texture in discover_graphic_textures(folder, love_first, require_multiframe_hint) {
        list.push((texture.key, texture.source_path));
    }
}

fn initial_texture_path(relative_path: &str) -> PathBuf {
    initial_texture_source_path(relative_path, |path| {
        dirs::app_dirs().resolve_asset_path(path)
    })
}

impl AssetManager {
    pub fn load_initial_textures(&mut self, backend: &mut Backend) -> Result<(), AssetError> {
        debug!("Loading initial textures...");

        #[inline(always)]
        fn fallback_rgba() -> RgbaImage {
            let data: [u8; 16] = [
                255, 0, 255, 255, 128, 128, 128, 255, 128, 128, 128, 255, 255, 0, 255, 255,
            ];
            RgbaImage::from_raw(2, 2, data.to_vec()).expect("fallback image")
        }

        let white_img = RgbaImage::from_raw(1, 1, vec![255, 255, 255, 255]).unwrap();
        let white_tex = backend.create_texture(&white_img, SamplerDesc::default())?;
        self.insert_texture("__white".to_string(), white_tex, 1, 1);
        register_texture_dims("__white", 1, 1);
        debug!("Loaded built-in texture: __white");

        let black_img = RgbaImage::from_raw(1, 1, vec![0, 0, 0, 255]).unwrap();
        let black_tex = backend.create_texture(&black_img, SamplerDesc::default())?;
        self.insert_texture("__black".to_string(), black_tex, 1, 1);
        register_texture_dims("__black", 1, 1);
        debug!("Loaded built-in texture: __black");

        let mut textures_to_load: Vec<(String, String)> = deadsync_theme::initial_texture_assets()
            .map(|asset| (asset.key.to_string(), asset.path.to_string()))
            .collect();

        append_noteskins_pngs_recursive(&mut textures_to_load, "noteskins");
        append_graphic_textures(&mut textures_to_load, "judgements", true, true);
        append_graphic_textures(&mut textures_to_load, "hold_judgements", false, true);
        append_graphic_textures(&mut textures_to_load, "held_miss", false, false);

        let texture_jobs: Vec<TextureDecodeJob> = textures_to_load
            .into_iter()
            .map(|(key, relative_path)| TextureDecodeJob {
                key,
                path: initial_texture_path(&relative_path),
            })
            .collect();

        let fallback_image = Arc::new(fallback_rgba());
        for result in decode_texture_jobs_parallel(texture_jobs) {
            match result {
                TextureDecodeResult::Decoded { key, image } => {
                    let sampler = if needs_repeat_sampler(&key) {
                        SamplerDesc {
                            wrap: SamplerWrap::Repeat,
                            ..SamplerDesc::default()
                        }
                    } else if key.starts_with("noteskins/") {
                        parse_texture_hints(&key).sampler_desc()
                    } else {
                        SamplerDesc::default()
                    };
                    let texture = backend.create_texture(&image, sampler)?;
                    register_texture_dims(&key, image.width(), image.height());
                    debug!("Loaded texture: {key}");
                    self.insert_texture(key, texture, image.width(), image.height());
                }
                TextureDecodeResult::Failed { key, message } => {
                    warn!("Failed to load texture for key '{key}': {message}. Using fallback.");
                    let sampler = if needs_repeat_sampler(&key) {
                        SamplerDesc {
                            wrap: SamplerWrap::Repeat,
                            ..SamplerDesc::default()
                        }
                    } else if key.starts_with("noteskins/") {
                        parse_texture_hints(&key).sampler_desc()
                    } else {
                        SamplerDesc::default()
                    };
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

        let path = direct_texture_key_path(texture_key, &key).unwrap_or_else(|| {
            let dirs = dirs::app_dirs();
            let path = dirs.resolve_asset_path(&format!("assets/{key}"));
            if path.is_file() {
                path
            } else {
                dirs.resolve_asset_path(&format!("assets/graphics/{key}"))
            }
        });
        if !path.is_file() {
            warn!("Failed to resolve texture key '{key}' for preload.");
            return;
        }

        let hints = parse_texture_hints(&key);
        let sampler = sampler_override.unwrap_or_else(|| {
            if needs_repeat_sampler(&key) {
                SamplerDesc {
                    wrap: SamplerWrap::Repeat,
                    ..hints.sampler_desc()
                }
            } else {
                hints.sampler_desc()
            }
        });
        match open_image_fallback(&path) {
            Ok(img) => {
                let mut rgba = img.to_rgba8();
                if !hints.is_default() {
                    apply_texture_hints(&mut rgba, &hints);
                }
                fix_hidden_alpha(&mut rgba);
                match backend.create_texture(&rgba, sampler) {
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
                }
            }
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
