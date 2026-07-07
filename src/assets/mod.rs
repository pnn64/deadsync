pub mod audio_folder;
pub mod i18n;
#[doc(hidden)]
pub mod present_dsl;
mod textures;
pub mod visual_styles;

use deadlib_platform::dirs;
use deadlib_present::font::Font;
use deadlib_render::{SamplerDesc, TextureHandle, TextureHandleMap};
use deadlib_renderer::{Backend, Texture as RendererTexture};
use image::RgbaImage;
use log::{debug, warn};
use std::collections::HashMap;
use std::path::Path;

pub use self::textures::{
    TexMeta, TextureChoice, TextureHints, canonical_texture_key, held_miss_texture_choices,
    hold_judgment_texture_choices, judgment_texture_choices, open_image_fallback,
    parse_sprite_sheet_dims, parse_texture_hints, register_generated_texture,
    register_texture_dims, resolve_texture_choice, resolve_texture_choice_entry, sprite_sheet_dims,
    strip_sprite_hints, texture_dims, texture_handle, texture_registry_generation,
    texture_source_dims_from_real, texture_source_frame_dims_from_real,
};
pub use deadlib_assets::upload::TextureUploadBudget;
pub use deadlib_assets::{
    ASSET_TEXTURE_CONTEXT as PRESENT_TEXTURE_CONTEXT, AssetTextureContext as PresentTextureContext,
};
pub use deadlib_assets::{AssetError, media_path_key};
use deadlib_assets::{
    FontStore, PreparedFontTexture, TextureStore, font_texture_asset_roots, font_texture_key,
    parse_font_with_asset_context, prepare_font_texture, set_font_fallback,
};
pub use deadsync_theme::{FontRole, machine_font_key, machine_font_key_for_text};

pub struct AssetManager {
    texture_store: TextureStore<RendererTexture>,
    font_store: FontStore,
}

impl AssetManager {
    pub fn new() -> Self {
        Self {
            texture_store: TextureStore::new(),
            font_store: FontStore::new(),
        }
    }

    pub fn register_font(&mut self, name: &'static str, font: Font) {
        self.font_store.register_font(name, font);
    }

    pub const fn fonts(&self) -> &HashMap<&'static str, Font> {
        self.font_store.fonts()
    }

    #[inline(always)]
    pub fn textures(&self) -> &TextureHandleMap<RendererTexture> {
        self.texture_store.textures()
    }

    #[inline(always)]
    pub fn has_texture_key(&self, key: &str) -> bool {
        self.texture_store.has_texture_key(key)
    }

    #[inline(always)]
    pub fn has_uploaded_texture_key(&self, key: &str) -> bool {
        self.texture_store.has_uploaded_texture_key(key)
    }

    #[inline(always)]
    pub(crate) fn has_pending_texture_upload(&self, key: &str) -> bool {
        self.texture_store.has_pending_texture_upload(key)
    }

    pub fn take_textures(&mut self) -> TextureHandleMap<RendererTexture> {
        self.texture_store.take_textures()
    }

    pub fn with_fonts<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&HashMap<&'static str, Font>) -> R,
    {
        self.font_store.with_fonts(f)
    }

    pub fn with_font<F, R>(&self, name: &str, f: F) -> Option<R>
    where
        F: FnOnce(&Font) -> R,
    {
        self.font_store.with_font(name, f)
    }

    fn register_parsed_font(
        &mut self,
        backend: &mut Backend,
        name: &'static str,
        font: Font,
        required_textures: &[std::path::PathBuf],
    ) -> Result<(), AssetError> {
        let dirs = dirs::app_dirs();
        let asset_roots = font_texture_asset_roots(&dirs.data_dir, &dirs.exe_dir);
        for tex_path in required_textures {
            let key = font_texture_key(tex_path, &asset_roots);
            if self.has_texture_key(&key) {
                continue;
            }
            let PreparedFontTexture { key, image, hints } =
                prepare_font_texture(tex_path, &font.texture_hints_map, &asset_roots)?;
            let texture = backend.create_texture(&image, hints.sampler_desc())?;
            register_texture_dims(&key, image.width(), image.height());
            self.insert_texture(key.clone(), texture, image.width(), image.height());
            debug!("Loaded font texture: {key}");
        }
        self.register_font(name, font);
        Ok(())
    }

    pub(crate) fn load_font_from_ini_path(
        &mut self,
        backend: &mut Backend,
        name: &'static str,
        ini_path: &Path,
    ) -> Result<(), AssetError> {
        if self.font_store.has_font(name) {
            return Ok(());
        }
        let dirs = dirs::app_dirs();
        let deadlib_present::font::FontLoadData {
            font,
            required_textures,
        } = parse_font_with_asset_context(
            ini_path,
            font_texture_asset_roots(&dirs.data_dir, &dirs.exe_dir),
        )?;
        self.register_parsed_font(backend, name, font, &required_textures)?;
        debug!("Loaded font '{name}' from '{}'", ini_path.display());
        Ok(())
    }

    pub(crate) fn reserve_texture_handle(&mut self, key: String) -> TextureHandle {
        self.texture_store.reserve_texture_handle(key)
    }

    pub(crate) fn insert_texture(
        &mut self,
        key: String,
        texture: RendererTexture,
        width: u32,
        height: u32,
    ) -> Option<RendererTexture> {
        self.texture_store
            .insert_texture(key, texture, width, height)
    }

    pub(crate) fn remove_texture(&mut self, key: &str) -> Option<(TextureHandle, RendererTexture)> {
        self.texture_store.remove_texture(key)
    }

    pub(crate) fn retire_texture(
        &mut self,
        backend: &mut Backend,
        handle: TextureHandle,
        texture: RendererTexture,
    ) {
        let mut textures = TextureHandleMap::default();
        textures.insert(handle, texture);
        backend.retire_textures(&mut textures);
    }

    pub(crate) fn set_texture_for_key(
        &mut self,
        backend: &mut Backend,
        key: String,
        texture: RendererTexture,
        width: u32,
        height: u32,
    ) -> TextureHandle {
        let (handle, old) = self
            .texture_store
            .set_texture_for_key(key, texture, width, height);
        if let Some(old) = old {
            self.retire_texture(backend, handle, old);
        }
        handle
    }

    pub(crate) fn update_texture_for_key(
        &mut self,
        backend: &mut Backend,
        key: &str,
        rgba: &RgbaImage,
    ) -> Result<(), AssetError> {
        if let Some(texture) =
            self.texture_store
                .uploaded_texture_mut(key, rgba.width(), rgba.height())
        {
            backend.update_texture(texture, rgba)?;
            return Ok(());
        }

        let texture = backend.create_texture(rgba, SamplerDesc::default())?;
        self.set_texture_for_key(
            backend,
            key.to_string(),
            texture,
            rgba.width(),
            rgba.height(),
        );
        register_texture_dims(key, rgba.width(), rgba.height());
        Ok(())
    }

    pub(crate) fn update_texture_for_key_with_sampler(
        &mut self,
        backend: &mut Backend,
        key: &str,
        rgba: &RgbaImage,
        sampler: SamplerDesc,
    ) -> Result<(), AssetError> {
        let texture = backend.create_texture(rgba, sampler)?;
        self.set_texture_for_key(
            backend,
            key.to_string(),
            texture,
            rgba.width(),
            rgba.height(),
        );
        register_texture_dims(key, rgba.width(), rgba.height());
        Ok(())
    }

    pub(crate) fn queue_texture_upload(&mut self, key: String, image: RgbaImage) {
        self.texture_store.queue_texture_upload(key, image);
    }

    pub(crate) fn queue_pending_generated_textures(&mut self) {
        self.texture_store.queue_pending_generated_textures();
    }

    pub(crate) fn drain_texture_uploads(
        &mut self,
        backend: &mut Backend,
        budget: TextureUploadBudget,
    ) {
        let mut drained_uploads = 0usize;
        let mut drained_bytes = 0usize;
        while let Some((key, upload)) =
            self.texture_store
                .pop_next_upload(budget, drained_uploads, drained_bytes)
        {
            drained_uploads = drained_uploads.saturating_add(1);
            drained_bytes = drained_bytes.saturating_add(upload.bytes);

            let mut updated = false;
            if let Some(texture) = self.texture_store.uploaded_texture_mut(
                &key,
                upload.image.width(),
                upload.image.height(),
            ) {
                match backend.update_texture(texture, upload.image.as_ref()) {
                    Ok(()) => {
                        updated = true;
                    }
                    Err(e) => {
                        warn!("Failed to update queued GPU texture for key '{key}': {e}");
                    }
                }
            }
            if updated {
                continue;
            }

            match backend.create_texture(upload.image.as_ref(), upload.sampler) {
                Ok(texture) => {
                    self.set_texture_for_key(
                        backend,
                        key,
                        texture,
                        upload.image.width(),
                        upload.image.height(),
                    );
                }
                Err(e) => {
                    warn!("Failed to create queued GPU texture for key '{key}': {e}");
                }
            }
        }
    }

    pub(crate) fn load_initial_fonts(&mut self, backend: &mut Backend) -> Result<(), AssetError> {
        let dirs = dirs::app_dirs();
        let asset_roots = font_texture_asset_roots(&dirs.data_dir, &dirs.exe_dir);
        for spec in deadsync_theme::initial_font_assets() {
            let resolved = dirs.resolve_asset_path(spec.ini_path);
            let deadlib_present::font::FontLoadData {
                mut font,
                required_textures,
            } = parse_font_with_asset_context(&resolved, asset_roots.clone())?;

            set_font_fallback(&mut font, spec.fallback_font_name);
            if let Some(fallback) = font.fallback_font_name {
                debug!(
                    "Font '{}' configured to use '{}' as fallback.",
                    spec.name, fallback
                );
            }
            self.register_parsed_font(backend, spec.name, font, &required_textures)?;
            debug!("Loaded font '{}' from '{}'", spec.name, spec.ini_path);
        }
        Ok(())
    }

    pub fn load_initial_assets(&mut self, backend: &mut Backend) -> Result<(), AssetError> {
        self.load_initial_textures(backend)?;
        self.load_initial_fonts(backend)?;
        Ok(())
    }
}

/// Convenience wrapper that reads the active [`crate::config::MachineFont`]
/// from the global config and resolves the role.
#[inline]
pub fn current_machine_font_key(role: FontRole) -> &'static str {
    machine_font_key(crate::config::get().machine_font, role)
}

/// Convenience wrapper that reads the active [`crate::config::MachineFont`]
/// from the global config and applies the wholesale-fallback policy.
#[inline]
pub fn current_machine_font_key_for_text(role: FontRole, text: &str) -> &'static str {
    machine_font_key_for_text(crate::config::get().machine_font, role, text)
}

impl Default for AssetManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank_rgba(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_pixel(width, height, image::Rgba([0, 0, 0, 0]))
    }

    #[test]
    fn remove_texture_cancels_pending_upload_for_reserved_handle() {
        let mut assets = AssetManager::new();
        assets.queue_texture_upload("queued".to_string(), blank_rgba(2, 2));

        assert!(assets.has_texture_key("queued"));
        assert!(assets.has_pending_texture_upload("queued"));

        assert!(assets.remove_texture("queued").is_none());
        assert!(!assets.has_texture_key("queued"));
        assert!(!assets.has_pending_texture_upload("queued"));
    }
}
