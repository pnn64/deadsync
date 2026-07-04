pub mod audio_folder;
pub mod i18n;
#[doc(hidden)]
pub mod present_dsl;
mod textures;
pub mod visual_styles;

use deadlib_platform::dirs;
use deadlib_present::font::{self, Font, FontLoadData, FontParseError};
use deadlib_present::texture as present_texture;
use deadlib_render::{SamplerDesc, TextureHandle, TextureHandleMap};
use deadlib_renderer::{Backend, Texture as RendererTexture};
use image::RgbaImage;
use log::{debug, warn};
use std::collections::HashMap;
use std::{error::Error as StdError, fmt, path::Path, sync::Arc};

pub use self::textures::{
    TexMeta, TextureChoice, TextureHints, canonical_texture_key, held_miss_texture_choices,
    hold_judgment_texture_choices, judgment_texture_choices, open_image_fallback,
    parse_sprite_sheet_dims, parse_texture_hints, register_generated_texture,
    register_texture_dims, resolve_texture_choice, resolve_texture_choice_entry, sprite_sheet_dims,
    strip_sprite_hints, texture_dims, texture_handle, texture_registry_generation,
    texture_source_dims_from_real, texture_source_frame_dims_from_real,
};
use self::textures::{
    apply_texture_hints, clear_texture_handles, fix_hidden_alpha, generated_texture,
    register_texture_handle, remove_texture_handle, take_pending_generated_texture_keys,
};
pub use deadlib_assets::upload::TextureUploadBudget;
use deadlib_assets::upload::TextureUploadQueue;
pub use deadsync_theme::{FontRole, machine_font_key, machine_font_key_for_text};

pub fn media_path_key(path: &Path) -> Arc<str> {
    match path.to_string_lossy() {
        std::borrow::Cow::Borrowed(key) => Arc::from(key),
        std::borrow::Cow::Owned(key) => Arc::from(key),
    }
}

#[derive(Debug)]
pub enum AssetError {
    FontParse(FontParseError),
    Image(image::ImageError),
    Backend(String),
    UnknownFont(&'static str),
}

impl fmt::Display for AssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FontParse(err) => write!(f, "{err}"),
            Self::Image(err) => write!(f, "{err}"),
            Self::Backend(err) => write!(f, "GPU texture operation failed: {err}"),
            Self::UnknownFont(name) => write!(f, "Unknown font name: {name}"),
        }
    }
}

impl StdError for AssetError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::FontParse(err) => Some(err),
            Self::Image(err) => Some(err),
            Self::Backend(_) | Self::UnknownFont(_) => None,
        }
    }
}

impl From<FontParseError> for AssetError {
    fn from(value: FontParseError) -> Self {
        Self::FontParse(value)
    }
}

impl From<image::ImageError> for AssetError {
    fn from(value: image::ImageError) -> Self {
        Self::Image(value)
    }
}

impl From<Box<dyn StdError>> for AssetError {
    fn from(value: Box<dyn StdError>) -> Self {
        Self::Backend(value.to_string())
    }
}

struct AssetFontTextureContext;

impl font::FontTextureContext for AssetFontTextureContext {
    fn canonical_texture_key(&self, path: &Path) -> String {
        canonical_texture_key(path)
    }

    fn sprite_sheet_dims(&self, key: &str) -> (u32, u32) {
        sprite_sheet_dims(key)
    }

    fn texture_hint_is_default(&self, raw: &str) -> bool {
        parse_texture_hints(raw).is_default()
    }

    fn texture_hint_doubleres(&self, raw: &str) -> bool {
        parse_texture_hints(raw).doubleres
    }
}

const FONT_TEXTURE_CONTEXT: AssetFontTextureContext = AssetFontTextureContext;

pub struct PresentTextureContext;

impl present_texture::TextureContext for PresentTextureContext {
    #[inline(always)]
    fn texture_registry_generation(&self) -> u64 {
        texture_registry_generation()
    }

    #[inline(always)]
    fn texture_dims(&self, key: &str) -> Option<present_texture::TextureMeta> {
        texture_dims(key).map(|meta| present_texture::TextureMeta {
            w: meta.w,
            h: meta.h,
        })
    }

    #[inline(always)]
    fn sprite_sheet_dims(&self, key: &str) -> (u32, u32) {
        sprite_sheet_dims(key)
    }

    #[inline(always)]
    fn texture_handle(&self, key: &str) -> TextureHandle {
        texture_handle(key)
    }
}

pub const PRESENT_TEXTURE_CONTEXT: PresentTextureContext = PresentTextureContext;

pub struct AssetManager {
    textures: TextureHandleMap<RendererTexture>,
    uploaded_texture_dims: TextureHandleMap<TexMeta>,
    texture_handles: HashMap<String, TextureHandle>,
    next_texture_handle: TextureHandle,
    fonts: HashMap<&'static str, Font>,
    pending_texture_uploads: TextureUploadQueue,
}

impl AssetManager {
    pub fn new() -> Self {
        Self {
            textures: TextureHandleMap::default(),
            uploaded_texture_dims: TextureHandleMap::default(),
            texture_handles: HashMap::new(),
            next_texture_handle: 1,
            fonts: HashMap::new(),
            pending_texture_uploads: TextureUploadQueue::default(),
        }
    }

    pub fn register_font(&mut self, name: &'static str, mut font: Font) {
        font.cache_tag = 0;
        font.chain_key = 0;
        self.fonts.insert(name, font);
        font::refresh_chain_keys(&mut self.fonts);
    }

    pub const fn fonts(&self) -> &HashMap<&'static str, Font> {
        &self.fonts
    }

    #[inline(always)]
    pub fn textures(&self) -> &TextureHandleMap<RendererTexture> {
        &self.textures
    }

    #[inline(always)]
    pub fn has_texture_key(&self, key: &str) -> bool {
        self.texture_handles.contains_key(key)
    }

    #[inline(always)]
    pub fn has_uploaded_texture_key(&self, key: &str) -> bool {
        self.texture_handles
            .get(key)
            .is_some_and(|handle| self.textures.contains_key(handle))
    }

    #[inline(always)]
    pub(crate) fn has_pending_texture_upload(&self, key: &str) -> bool {
        self.pending_texture_uploads.contains(key)
    }

    pub fn take_textures(&mut self) -> TextureHandleMap<RendererTexture> {
        self.texture_handles.clear();
        clear_texture_handles();
        self.uploaded_texture_dims.clear();
        std::mem::take(&mut self.textures)
    }

    pub fn with_fonts<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&HashMap<&'static str, Font>) -> R,
    {
        f(&self.fonts)
    }

    pub fn with_font<F, R>(&self, name: &str, f: F) -> Option<R>
    where
        F: FnOnce(&Font) -> R,
    {
        self.fonts.get(name).map(f)
    }

    fn register_parsed_font(
        &mut self,
        backend: &mut Backend,
        name: &'static str,
        font: Font,
        required_textures: &[std::path::PathBuf],
    ) -> Result<(), AssetError> {
        for tex_path in required_textures {
            let key = canonical_texture_key(tex_path);
            if !self.has_texture_key(&key) {
                let hints = font
                    .texture_hints_map
                    .get(&key)
                    .map(|s| parse_texture_hints(s))
                    .unwrap_or_default();
                let mut image_data = open_image_fallback(tex_path)?.to_rgba8();
                if !hints.is_default() {
                    apply_texture_hints(&mut image_data, &hints);
                }
                fix_hidden_alpha(&mut image_data);
                let texture = backend.create_texture(&image_data, hints.sampler_desc())?;
                register_texture_dims(&key, image_data.width(), image_data.height());
                self.insert_texture(
                    key.clone(),
                    texture,
                    image_data.width(),
                    image_data.height(),
                );
                debug!("Loaded font texture: {key}");
            }
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
        if self.fonts.contains_key(name) {
            return Ok(());
        }
        let FontLoadData {
            font,
            required_textures,
        } = font::parse_with_texture_context(&ini_path.to_string_lossy(), &FONT_TEXTURE_CONTEXT)?;
        self.register_parsed_font(backend, name, font, &required_textures)?;
        debug!("Loaded font '{name}' from '{}'", ini_path.display());
        Ok(())
    }

    #[inline(always)]
    fn alloc_texture_handle(&mut self) -> TextureHandle {
        let handle = self.next_texture_handle;
        self.next_texture_handle = self.next_texture_handle.wrapping_add(1).max(1);
        handle
    }

    pub(crate) fn reserve_texture_handle(&mut self, key: String) -> TextureHandle {
        match self.texture_handles.get(&key).copied() {
            Some(handle) => handle,
            None => {
                let handle = self.alloc_texture_handle();
                self.texture_handles.insert(key.clone(), handle);
                register_texture_handle(&key, handle);
                handle
            }
        }
    }

    pub(crate) fn insert_texture(
        &mut self,
        key: String,
        texture: RendererTexture,
        width: u32,
        height: u32,
    ) -> Option<RendererTexture> {
        let handle = self.reserve_texture_handle(key);
        self.uploaded_texture_dims.insert(
            handle,
            TexMeta {
                w: width,
                h: height,
            },
        );
        self.textures.insert(handle, texture)
    }

    pub(crate) fn remove_texture(&mut self, key: &str) -> Option<(TextureHandle, RendererTexture)> {
        self.pending_texture_uploads.remove(key);
        let handle = self.texture_handles.remove(key)?;
        remove_texture_handle(key);
        self.uploaded_texture_dims.remove(&handle);
        self.textures
            .remove(&handle)
            .map(|texture| (handle, texture))
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
        self.pending_texture_uploads.remove(&key);
        let handle = self.reserve_texture_handle(key);
        self.uploaded_texture_dims.insert(
            handle,
            TexMeta {
                w: width,
                h: height,
            },
        );
        if let Some(old) = self.textures.insert(handle, texture) {
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
        self.pending_texture_uploads.remove(key);
        let handle = self.texture_handles.get(key).copied();
        if let Some(handle) = handle
            && let Some(meta) = self.uploaded_texture_dims.get(&handle).copied()
            && meta.w == rgba.width()
            && meta.h == rgba.height()
            && let Some(texture) = self.textures.get_mut(&handle)
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
        self.pending_texture_uploads.remove(key);
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

    fn queue_texture_upload_shared(
        &mut self,
        key: String,
        image: Arc<RgbaImage>,
        sampler: SamplerDesc,
    ) {
        self.reserve_texture_handle(key.clone());
        register_texture_dims(&key, image.width(), image.height());
        self.pending_texture_uploads.push(key, image, sampler);
    }

    pub(crate) fn queue_texture_upload(&mut self, key: String, image: RgbaImage) {
        self.queue_texture_upload_with_sampler(key, image, SamplerDesc::default());
    }

    pub(crate) fn queue_texture_upload_with_sampler(
        &mut self,
        key: String,
        image: RgbaImage,
        sampler: SamplerDesc,
    ) {
        self.queue_texture_upload_shared(key, Arc::new(image), sampler);
    }

    pub(crate) fn queue_pending_generated_textures(&mut self) {
        for key in take_pending_generated_texture_keys() {
            let Some(generated) = generated_texture(&key) else {
                continue;
            };
            self.queue_texture_upload_shared(key, generated.image, generated.sampler);
        }
    }

    pub(crate) fn drain_texture_uploads(
        &mut self,
        backend: &mut Backend,
        budget: TextureUploadBudget,
    ) {
        let mut drained_uploads = 0usize;
        let mut drained_bytes = 0usize;
        while let Some((key, upload)) =
            self.pending_texture_uploads
                .pop_next(budget, drained_uploads, drained_bytes)
        {
            drained_uploads = drained_uploads.saturating_add(1);
            drained_bytes = drained_bytes.saturating_add(upload.bytes);

            let handle = self.texture_handles.get(&key).copied();
            let mut updated = false;
            if let Some(handle) = handle
                && let Some(meta) = self.uploaded_texture_dims.get(&handle).copied()
                && meta.w == upload.image.width()
                && meta.h == upload.image.height()
                && let Some(texture) = self.textures.get_mut(&handle)
            {
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
        for &name in &[
            "wendy",
            "miso",
            "cjk",
            "emoji",
            "game",
            "wendy_monospace_numbers",
            "wendy_screenevaluation",
            "wendy_combo",
            "combo_arial_rounded",
            "combo_asap",
            "combo_bebas_neue",
            "combo_source_code",
            "combo_work",
            "combo_wendy_cursed",
            "combo_mega",
            "wendy_white",
            "mega_alpha",
            "mega_monospace_numbers",
            "mega_screenevaluation",
            "mega_game",
        ] {
            let ini_path_str = match name {
                "wendy" => "assets/fonts/wendy/_wendy small.ini",
                "miso" => "assets/fonts/miso/_miso light.ini",
                "cjk" => "assets/fonts/cjk/_jfonts 16px.ini",
                "emoji" => "assets/fonts/emoji/_emoji 16px.ini",
                "game" => "assets/fonts/game/_game chars 16px.ini",
                "wendy_monospace_numbers" => "assets/fonts/wendy/_wendy monospace numbers.ini",
                "wendy_screenevaluation" => "assets/fonts/wendy/_ScreenEvaluation numbers.ini",
                "wendy_combo" => "assets/fonts/_combo/wendy/Wendy.ini",
                "combo_arial_rounded" => "assets/fonts/_combo/Arial Rounded/Arial Rounded.ini",
                "combo_asap" => "assets/fonts/_combo/Asap/Asap.ini",
                "combo_bebas_neue" => "assets/fonts/_combo/Bebas Neue/Bebas Neue.ini",
                "combo_source_code" => "assets/fonts/_combo/Source Code/Source Code.ini",
                "combo_work" => "assets/fonts/_combo/Work/Work.ini",
                "combo_wendy_cursed" => "assets/fonts/_combo/Wendy (Cursed)/Wendy (Cursed).ini",
                "combo_mega" => "assets/fonts/_combo/Mega/Mega.ini",
                "wendy_white" => "assets/fonts/wendy/_wendy white.ini",
                "mega_alpha" => "assets/fonts/Mega/_mega font.ini",
                "mega_monospace_numbers" => "assets/fonts/Mega/_mega monospace numbers.ini",
                "mega_screenevaluation" => "assets/fonts/Mega/_ScreenEvaluation numbers.ini",
                "mega_game" => "assets/fonts/Mega/_game chars 36px 4x1.ini",
                _ => return Err(AssetError::UnknownFont(name)),
            };

            let resolved = dirs::app_dirs().resolve_asset_path(ini_path_str);
            let resolved_str = resolved.to_string_lossy();
            let FontLoadData {
                mut font,
                required_textures,
            } = font::parse_with_texture_context(&resolved_str, &FONT_TEXTURE_CONTEXT)?;

            if name == "miso" {
                font.fallback_font_name = Some("game");
                debug!("Font 'miso' configured to use 'game' as fallback.");
            }

            if name == "game" {
                font.fallback_font_name = Some("cjk");
                debug!("Font 'game' configured to use 'cjk' as fallback.");
            }

            if name == "cjk" {
                font.fallback_font_name = Some("emoji");
                debug!("Font 'cjk' configured to use 'emoji' as fallback.");
            }

            // Mega is uppercase-Latin + digits + a small punctuation set. Fall
            // back through Miso so screens that ever pass lowercase or non-ASCII
            // through a Mega-bound role still render readable glyphs instead of
            // missing ones. Mega's own ini imports `Mega/_game chars 36px` for
            // name-entry glyphs, so we don't need to override that.
            if name == "mega_alpha" {
                font.fallback_font_name = Some("miso");
                debug!("Font 'mega_alpha' configured to use 'miso' as fallback.");
            }
            self.register_parsed_font(backend, name, font, &required_textures)?;
            debug!("Loaded font '{name}' from '{ini_path_str}'");
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
        assert!(assets.pending_texture_uploads.contains("queued"));

        assert!(assets.remove_texture("queued").is_none());
        assert!(!assets.has_texture_key("queued"));
        assert!(!assets.pending_texture_uploads.contains("queued"));
        assert!(!assets.has_pending_texture_upload("queued"));
    }
}
