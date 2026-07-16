use deadlib_assets::{
    AssetStore, PreparedFontTexture, TextureUploadAction, TextureUploadDrainError,
    font_texture_asset_roots, parse_font_asset_specs, parse_font_with_asset_dirs,
    prepare_required_font_textures, register_texture_dims,
};
use deadlib_platform::dirs;
use deadlib_present::font::Font;
use deadlib_render::{SamplerDesc, TextureHandle, TextureHandleMap};
use deadlib_renderer::{Backend, Texture as RendererTexture};
use deadsync_theme::ThemeAssetManifest;
use image::RgbaImage;
use log::{debug, warn};
use std::collections::HashMap;
use std::path::Path;

pub struct AssetManager {
    pub(crate) store: AssetStore<RendererTexture>,
    pub(crate) texture_needs_repeat_sampler: fn(&str) -> bool,
}

impl AssetManager {
    pub fn new() -> Self {
        Self {
            store: AssetStore::new(),
            texture_needs_repeat_sampler: |_| false,
        }
    }

    pub fn register_font(&mut self, name: &'static str, font: Font) {
        self.store.register_font(name, font);
    }

    pub const fn fonts(&self) -> &HashMap<&'static str, Font> {
        self.store.fonts()
    }

    #[inline(always)]
    pub fn textures(&self) -> &TextureHandleMap<RendererTexture> {
        self.store.textures()
    }

    #[inline(always)]
    pub fn has_texture_key(&self, key: &str) -> bool {
        self.store.has_texture_key(key)
    }

    #[inline(always)]
    pub fn has_uploaded_texture_key(&self, key: &str) -> bool {
        self.store.has_uploaded_texture_key(key)
    }

    #[inline(always)]
    pub fn has_pending_texture_upload(&self, key: &str) -> bool {
        self.store.has_pending_texture_upload(key)
    }

    pub fn take_textures(&mut self) -> TextureHandleMap<RendererTexture> {
        self.store.take_textures()
    }

    pub fn with_fonts<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&HashMap<&'static str, Font>) -> R,
    {
        self.store.with_fonts(f)
    }

    pub fn with_font<F, R>(&self, name: &str, f: F) -> Option<R>
    where
        F: FnOnce(&Font) -> R,
    {
        self.store.with_font(name, f)
    }

    fn register_parsed_font(
        &mut self,
        backend: &mut Backend,
        name: &'static str,
        font: Font,
        required_textures: &[std::path::PathBuf],
    ) -> Result<(), deadlib_assets::AssetError> {
        let dirs = dirs::app_dirs();
        let asset_roots = font_texture_asset_roots(&dirs.data_dir, &dirs.exe_dir);
        let textures =
            prepare_required_font_textures(&font, required_textures, &asset_roots, |key| {
                self.has_texture_key(key)
            })?;
        for PreparedFontTexture { key, image, hints } in textures {
            let texture = backend.create_texture(&image, hints.sampler_desc())?;
            register_texture_dims(&key, image.width(), image.height());
            self.insert_texture(key.clone(), texture, image.width(), image.height());
            debug!("Loaded font texture: {key}");
        }
        self.register_font(name, font);
        Ok(())
    }

    pub fn load_font_from_ini_path(
        &mut self,
        backend: &mut Backend,
        name: &'static str,
        ini_path: &Path,
    ) -> Result<(), deadlib_assets::AssetError> {
        if self.store.has_font(name) {
            return Ok(());
        }
        let dirs = dirs::app_dirs();
        let deadlib_present::font::FontLoadData {
            font,
            required_textures,
        } = parse_font_with_asset_dirs(ini_path, &dirs.data_dir, &dirs.exe_dir)?;
        self.register_parsed_font(backend, name, font, &required_textures)?;
        debug!("Loaded font '{name}' from '{}'", ini_path.display());
        Ok(())
    }

    pub fn reserve_texture_handle(&mut self, key: String) -> TextureHandle {
        self.store.reserve_texture_handle(key)
    }

    pub fn insert_texture(
        &mut self,
        key: String,
        texture: RendererTexture,
        width: u32,
        height: u32,
    ) -> Option<RendererTexture> {
        self.store.insert_texture(key, texture, width, height)
    }

    pub fn remove_texture(&mut self, key: &str) -> Option<(TextureHandle, RendererTexture)> {
        self.store.remove_texture(key)
    }

    pub fn retire_texture(
        &mut self,
        backend: &mut Backend,
        handle: TextureHandle,
        texture: RendererTexture,
    ) {
        let mut textures = TextureHandleMap::default();
        textures.insert(handle, texture);
        backend.retire_textures(&mut textures);
    }

    pub fn set_texture_for_key(
        &mut self,
        backend: &mut Backend,
        key: String,
        texture: RendererTexture,
        width: u32,
        height: u32,
    ) -> TextureHandle {
        let (handle, old) = self.store.set_texture_for_key(key, texture, width, height);
        if let Some(old) = old {
            self.retire_texture(backend, handle, old);
        }
        handle
    }

    pub fn update_texture_for_key(
        &mut self,
        backend: &mut Backend,
        key: &str,
        rgba: &RgbaImage,
    ) -> Result<(), deadlib_assets::AssetError> {
        if let Some(texture) = self
            .store
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

    pub fn update_texture_for_key_with_sampler(
        &mut self,
        backend: &mut Backend,
        key: &str,
        rgba: &RgbaImage,
        sampler: SamplerDesc,
    ) -> Result<(), deadlib_assets::AssetError> {
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

    pub fn queue_texture_upload(&mut self, key: String, image: RgbaImage) {
        self.store.queue_texture_upload(key, image);
    }

    pub fn queue_video_frame_upload(&mut self, key: String, frame: deadlib_video::VideoFrame) {
        let (image, recycle_tx) = frame.into_upload_parts();
        self.store
            .queue_recyclable_texture_upload(key, image, recycle_tx);
    }

    pub fn queue_pending_generated_textures(&mut self) {
        self.store.queue_pending_generated_textures();
    }

    pub fn drain_texture_uploads(
        &mut self,
        backend: &mut Backend,
        budget: deadlib_assets::upload::TextureUploadBudget,
    ) {
        let (retired, errors) = self.store.drain_texture_uploads_with(
            budget,
            |action| -> Result<Option<RendererTexture>, Box<dyn std::error::Error>> {
                match action {
                    TextureUploadAction::Update { texture, image } => {
                        backend.update_texture(texture, image)?;
                        Ok(None)
                    }
                    TextureUploadAction::Create { image, sampler } => {
                        backend.create_texture(image, sampler).map(Some)
                    }
                }
            },
        );
        for (handle, texture) in retired {
            self.retire_texture(backend, handle, texture);
        }
        for error in errors {
            match error {
                TextureUploadDrainError::Update { key, error } => {
                    warn!("Failed to update queued GPU texture for key '{key}': {error}");
                }
                TextureUploadDrainError::Create { key, error } => {
                    warn!("Failed to create queued GPU texture for key '{key}': {error}");
                }
            }
        }
    }

    pub fn load_initial_fonts(
        &mut self,
        backend: &mut Backend,
        fonts: &'static [deadlib_assets::FontAssetSpec],
    ) -> Result<(), deadlib_assets::AssetError> {
        let dirs = dirs::app_dirs();
        let asset_roots = font_texture_asset_roots(&dirs.data_dir, &dirs.exe_dir);
        for asset in parse_font_asset_specs(fonts.iter().copied(), &asset_roots, |path| {
            dirs.resolve_asset_path(path)
        })? {
            if let Some(fallback) = asset.font.fallback_font_name {
                debug!(
                    "Font '{}' configured to use '{}' as fallback.",
                    asset.name, fallback
                );
            }
            self.register_parsed_font(backend, asset.name, asset.font, &asset.required_textures)?;
            debug!("Loaded font '{}' from '{}'", asset.name, asset.ini_path);
        }
        Ok(())
    }

    pub fn load_initial_assets<T>(
        &mut self,
        backend: &mut Backend,
        manifest: ThemeAssetManifest<T>,
    ) -> Result<(), deadlib_assets::AssetError>
    where
        T: IntoIterator<Item = deadlib_assets::TextureAssetSpec>,
    {
        let ThemeAssetManifest {
            fonts,
            textures,
            texture_needs_repeat_sampler,
        } = manifest;
        self.texture_needs_repeat_sampler = texture_needs_repeat_sampler;
        self.load_initial_textures(backend, textures)?;
        self.load_initial_fonts(backend, fonts)?;
        Ok(())
    }
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

    #[test]
    fn new_manager_has_neutral_repeat_sampler_policy() {
        let assets = AssetManager::new();

        assert!(!(assets.texture_needs_repeat_sampler)("any-texture.png"));
    }
}
