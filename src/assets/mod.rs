pub(crate) mod dynamic;
pub mod i18n;
mod textures;

use crate::config::dirs;
use crate::engine::gfx::{
    Backend, SamplerDesc, Texture as GfxTexture, TextureHandle, TextureHandleMap,
};
use crate::engine::present::font::{self, Font, FontLoadData, FontParseError};
use image::RgbaImage;
use log::{debug, warn};
use std::collections::{HashMap, VecDeque};
use std::{error::Error as StdError, fmt, path::Path, sync::Arc};

#[cfg(test)]
pub(crate) use self::dynamic::{
    BannerCacheOptions, dedupe_dynamic_keys, dynamic_image_cache_path_for,
    load_or_build_cached_dynamic_image, save_cached_banner_image, save_raw_cached_banner_image,
};
#[cfg(test)]
pub(crate) use self::textures::parse_texture_resolution_hint;
pub use self::textures::{
    TexMeta, TextureChoice, TextureHints, canonical_texture_key, hold_judgment_texture_choices,
    judgment_texture_choices, open_image_fallback, parse_sprite_sheet_dims, parse_texture_hints,
    register_generated_texture, register_texture_dims, resolve_texture_choice, sprite_sheet_dims,
    strip_sprite_hints, texture_dims, texture_handle, texture_registry_generation,
    texture_source_dims_from_real, texture_source_frame_dims_from_real,
};
use self::textures::{
    apply_texture_hints, clear_texture_handles, generated_texture, register_texture_handle,
    remove_texture_handle, take_pending_generated_texture_keys,
};

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

#[derive(Clone, Copy)]
pub(crate) struct TextureUploadBudget {
    pub max_uploads: usize,
    pub max_bytes: usize,
}

struct PendingTextureUpload {
    image: Arc<RgbaImage>,
    sampler: SamplerDesc,
    bytes: usize,
}

#[derive(Default)]
struct TextureUploadQueue {
    order: VecDeque<String>,
    entries: HashMap<String, PendingTextureUpload>,
    queued_bytes: usize,
}

impl TextureUploadQueue {
    fn push(&mut self, key: String, image: Arc<RgbaImage>, sampler: SamplerDesc) {
        let bytes = image.as_raw().len();
        if let Some(old) = self.entries.insert(
            key.clone(),
            PendingTextureUpload {
                image,
                sampler,
                bytes,
            },
        ) {
            self.queued_bytes = self.queued_bytes.saturating_sub(old.bytes);
        } else {
            self.order.push_back(key);
        }
        self.queued_bytes = self.queued_bytes.saturating_add(bytes);
    }

    fn remove(&mut self, key: &str) {
        if let Some(old) = self.entries.remove(key) {
            self.queued_bytes = self.queued_bytes.saturating_sub(old.bytes);
        }
    }

    fn pop_next(
        &mut self,
        budget: TextureUploadBudget,
        drained_uploads: usize,
        drained_bytes: usize,
    ) -> Option<(String, PendingTextureUpload)> {
        while let Some(key) = self.order.pop_front() {
            let Some(upload) = self.entries.remove(&key) else {
                continue;
            };
            let next_bytes = drained_bytes.saturating_add(upload.bytes);
            let fits_budget =
                drained_uploads < budget.max_uploads && next_bytes <= budget.max_bytes;
            let allow_first =
                drained_uploads == 0 && budget.max_uploads > 0 && budget.max_bytes > 0;
            if fits_budget || allow_first {
                self.queued_bytes = self.queued_bytes.saturating_sub(upload.bytes);
                return Some((key, upload));
            }
            self.entries.insert(key.clone(), upload);
            self.order.push_front(key);
            return None;
        }
        None
    }
}

pub struct AssetManager {
    textures: TextureHandleMap<GfxTexture>,
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
    pub fn textures(&self) -> &TextureHandleMap<GfxTexture> {
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

    pub fn take_textures(&mut self) -> TextureHandleMap<GfxTexture> {
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
        } = font::parse(&ini_path.to_string_lossy())?;
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
        texture: GfxTexture,
        width: u32,
        height: u32,
    ) -> Option<GfxTexture> {
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

    pub(crate) fn remove_texture(&mut self, key: &str) -> Option<(TextureHandle, GfxTexture)> {
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
        texture: GfxTexture,
    ) {
        let mut textures = TextureHandleMap::default();
        textures.insert(handle, texture);
        backend.retire_textures(&mut textures);
    }

    pub(crate) fn set_texture_for_key(
        &mut self,
        backend: &mut Backend,
        key: String,
        texture: GfxTexture,
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
            } = font::parse(&resolved_str)?;

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

/// Logical font role in the theme, mirroring Simply Love's per-role .redir
/// table (`Themes/Simply Love/Fonts/<MachineFont> <Role>.redir`).
///
/// Use [`machine_font_key`] to resolve a role to a registered font key under
/// the active [`crate::config::MachineFont`].
///
/// **Do not** use this for gameplay-side text (notefield combo, judgment
/// label, hold judgment) — those follow each player's per-profile
/// `ComboFont`, not the machine machine font.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontRole {
    /// Body text (option labels, descriptions). Always Miso, regardless
    /// of `MachineFont` — matches SL's `Mega Normal.redir -> Miso/_miso light`.
    Normal,
    /// Emphasised UI labels (in-screen highlights, prompt answers).
    Bold,
    /// Large screen titles (top-of-screen header bars).
    Header,
    /// Bottom-of-screen action prompts (e.g., submit footer).
    Footer,
    /// Numeric stats text (BPM, percentages, score counters).
    Numbers,
    /// Evaluation panel numerics (large grade/percentage on results).
    ScreenEval,
}

/// Resolve a logical [`FontRole`] under the given [`crate::config::MachineFont`]
/// to the registered font key in [`AssetManager::load_initial_fonts`].
///
/// Mirrors the Simply Love `<MachineFont> <Role>.redir` table:
///
/// | Role         | Common (default)            | Mega                          |
/// | ------------ | --------------------------- | ----------------------------- |
/// | `Normal`     | `miso`                      | `miso` (unchanged in SL)      |
/// | `Bold`       | `wendy`                     | `mega_alpha`                  |
/// | `Header`     | `wendy`                     | `mega_alpha`                  |
/// | `Footer`     | `wendy`                     | `mega_alpha`                  |
/// | `Numbers`    | `wendy_monospace_numbers`   | `mega_monospace_numbers`      |
/// | `ScreenEval` | `wendy_screenevaluation`    | `mega_screenevaluation`       |
pub fn machine_font_key(machine_font: crate::config::MachineFont, role: FontRole) -> &'static str {
    use crate::config::MachineFont::{Common, Mega};
    match (machine_font, role) {
        (_, FontRole::Normal) => "miso",
        (Common, FontRole::Bold | FontRole::Header | FontRole::Footer) => "wendy",
        (Mega, FontRole::Bold | FontRole::Header | FontRole::Footer) => "mega_alpha",
        (Common, FontRole::Numbers) => "wendy_monospace_numbers",
        (Mega, FontRole::Numbers) => "mega_monospace_numbers",
        (Common, FontRole::ScreenEval) => "wendy_screenevaluation",
        (Mega, FontRole::ScreenEval) => "mega_screenevaluation",
    }
}

/// Convenience wrapper that reads the active [`crate::config::MachineFont`]
/// from the global config and resolves the role.
#[inline]
pub fn current_machine_font_key(role: FontRole) -> &'static str {
    machine_font_key(crate::config::get().machine_font, role)
}

/// Codepoints supported by `assets/fonts/Mega/_mega font.ini`. Mega ships
/// upper + lower Latin + digits + a small punctuation set; anything outside
/// this range is missing and would per-glyph fall back through Miso, which
/// produces ugly mixed-font strings (e.g. uppercase Mega + lowercase Miso).
fn mega_alpha_supports_char(c: char) -> bool {
    matches!(c,
        'A'..='Z' | 'a'..='z' | '0'..='9' |
        ' ' | '?' | '!' | '.' | ',' | ';' | ':' | '\'' | '"' |
        '+' | '=' | '-' | '_' | '<' | '>' | '[' | ']' |
        '@' | '#' | '$' | '%' | '^' | '&' | '(' | ')' | '{' | '}' |
        '/' | '\\'
    )
}

#[inline]
fn mega_alpha_supports(text: &str) -> bool {
    text.chars().all(mega_alpha_supports_char)
}

/// Variant of [`machine_font_key`] that, for the alphabetic roles
/// (`Bold` / `Header` / `Footer`) under [`crate::config::MachineFont::Mega`],
/// **wholesale** falls the entire string back to Wendy when it contains
/// any glyph Mega can't render. This avoids the mixed-font appearance you
/// get from per-glyph fallback through Miso (e.g., a CJK or symbol-heavy
/// `submit_footer` rendering as half Mega / half Miso).
///
/// Numeric roles (`Numbers` / `ScreenEval`) stay on the direct resolver
/// since their inputs are always digits Mega supports.
pub fn machine_font_key_for_text(
    machine_font: crate::config::MachineFont,
    role: FontRole,
    text: &str,
) -> &'static str {
    use crate::config::MachineFont::Mega;
    match (machine_font, role) {
        (Mega, FontRole::Bold | FontRole::Header | FontRole::Footer)
            if !mega_alpha_supports(text) =>
        {
            "wendy"
        }
        _ => machine_font_key(machine_font, role),
    }
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
    use crate::config::MachineFont;
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicUsize, Ordering},
    };

    #[test]
    fn machine_font_key_normal_is_always_miso() {
        // Mirrors SL's `Mega Normal.redir -> Miso/_miso light`: body text
        // never swaps to Mega even when the machine font is Mega.
        assert_eq!(machine_font_key(MachineFont::Common, FontRole::Normal), "miso");
        assert_eq!(machine_font_key(MachineFont::Mega, FontRole::Normal), "miso");
    }

    #[test]
    fn machine_font_key_common_routes_to_wendy_family() {
        assert_eq!(machine_font_key(MachineFont::Common, FontRole::Bold), "wendy");
        assert_eq!(machine_font_key(MachineFont::Common, FontRole::Header), "wendy");
        assert_eq!(machine_font_key(MachineFont::Common, FontRole::Footer), "wendy");
        assert_eq!(
            machine_font_key(MachineFont::Common, FontRole::Numbers),
            "wendy_monospace_numbers"
        );
        assert_eq!(
            machine_font_key(MachineFont::Common, FontRole::ScreenEval),
            "wendy_screenevaluation"
        );
    }

    #[test]
    fn machine_font_key_mega_routes_to_mega_family() {
        assert_eq!(machine_font_key(MachineFont::Mega, FontRole::Bold), "mega_alpha");
        assert_eq!(machine_font_key(MachineFont::Mega, FontRole::Header), "mega_alpha");
        assert_eq!(machine_font_key(MachineFont::Mega, FontRole::Footer), "mega_alpha");
        assert_eq!(
            machine_font_key(MachineFont::Mega, FontRole::Numbers),
            "mega_monospace_numbers"
        );
        assert_eq!(
            machine_font_key(MachineFont::Mega, FontRole::ScreenEval),
            "mega_screenevaluation"
        );
    }

    #[test]
    fn machine_font_key_for_text_passes_through_when_common() {
        // Common is the default; the for_text policy must never alter it.
        for role in [
            FontRole::Normal,
            FontRole::Bold,
            FontRole::Header,
            FontRole::Footer,
            FontRole::Numbers,
            FontRole::ScreenEval,
        ] {
            assert_eq!(
                machine_font_key_for_text(MachineFont::Common, role, "anything"),
                machine_font_key(MachineFont::Common, role),
                "role={role:?}"
            );
        }
    }

    #[test]
    fn machine_font_key_for_text_uses_mega_alpha_for_ascii() {
        assert_eq!(
            machine_font_key_for_text(MachineFont::Mega, FontRole::Header, "Select Music"),
            "mega_alpha"
        );
        assert_eq!(
            machine_font_key_for_text(MachineFont::Mega, FontRole::Footer, "Press Start"),
            "mega_alpha"
        );
    }

    #[test]
    fn machine_font_key_for_text_falls_back_wholesale_for_unsupported_chars() {
        // CJK title -- entire actor falls back to Wendy, not per-glyph mix.
        assert_eq!(
            machine_font_key_for_text(MachineFont::Mega, FontRole::Header, "リズム"),
            "wendy"
        );
        // Symbol-heavy submit footer (icons in deadsync's strings).
        assert_eq!(
            machine_font_key_for_text(MachineFont::Mega, FontRole::Footer, "◐ ✔ ⊘"),
            "wendy"
        );
        // Even one bad char triggers fallback.
        assert_eq!(
            machine_font_key_for_text(MachineFont::Mega, FontRole::Bold, "Hello\u{2014}World"),
            "wendy"
        );
    }

    #[test]
    fn machine_font_key_for_text_keeps_numeric_roles_on_mega_unconditionally() {
        // Numeric roles are always digits Mega supports; for_text shouldn't
        // ever fall them back even if the caller passes weird input.
        assert_eq!(
            machine_font_key_for_text(MachineFont::Mega, FontRole::Numbers, "リズム"),
            "mega_monospace_numbers"
        );
        assert_eq!(
            machine_font_key_for_text(MachineFont::Mega, FontRole::ScreenEval, "リズム"),
            "mega_screenevaluation"
        );
    }

    static NEXT_TMP_ID: AtomicUsize = AtomicUsize::new(1);

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(name: &str) -> Self {
            let id = NEXT_TMP_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "deadsync-assets-{name}-{}-{id}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn test_rgba(color: [u8; 4]) -> RgbaImage {
        RgbaImage::from_raw(1, 1, color.to_vec()).expect("test pixel should match image size")
    }

    fn write_test_png(path: &Path, color: [u8; 4]) {
        test_rgba(color).save(path).unwrap();
    }

    fn blank_rgba(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_pixel(width, height, image::Rgba([0, 0, 0, 0]))
    }

    #[test]
    fn parses_texture_resolution_hint_from_parenthetical_res_tag() {
        assert_eq!(
            parse_texture_resolution_hint("_miso light 15x15 (res 360x360).png"),
            Some((360, 360))
        );
    }

    #[test]
    fn parses_texture_resolution_hint_case_insensitively() {
        assert_eq!(
            parse_texture_resolution_hint("banner (ReS 512x160).png"),
            Some((512, 160))
        );
    }

    #[test]
    fn ignores_invalid_res_tags_until_a_valid_one() {
        assert_eq!(
            parse_texture_resolution_hint("sheet (res nope) (res 384 x170).png"),
            Some((384, 170))
        );
    }

    #[test]
    fn ignores_zero_sized_res_tags() {
        assert_eq!(parse_texture_resolution_hint("sheet (res 0x170).png"), None);
    }

    #[test]
    fn ignores_non_parenthetical_sheet_dims() {
        assert_eq!(
            parse_texture_resolution_hint("_miso light 16x7 doubleres.png"),
            None
        );
    }

    #[test]
    fn dedupe_dynamic_keys_preserves_first_owner_order() {
        assert_eq!(
            dedupe_dynamic_keys(vec![
                "banner.mp4".to_string(),
                "shared.mp4".to_string(),
                "banner.mp4".to_string(),
                "shared.mp4".to_string(),
                "bg.mp4".to_string(),
            ]),
            vec![
                "banner.mp4".to_string(),
                "shared.mp4".to_string(),
                "bg.mp4".to_string(),
            ]
        );
    }

    #[test]
    fn cache_hit_skips_stale_variant_prune() {
        let dir = TempDir::new("cache-hit-no-prune");
        let src = dir.path().join("banner.png");
        let cache_dir = dir.path().join("cache");
        let opts = BannerCacheOptions { enabled: true };
        let expected = test_rgba([1, 2, 3, 4]);

        write_test_png(&src, [1, 2, 3, 4]);
        let (cache_path, path_hex) = dynamic_image_cache_path_for(&src, opts, &cache_dir).unwrap();
        let stale_path = cache_path
            .parent()
            .unwrap()
            .join(format!("{path_hex}-ffffffffffffffff.rgba"));
        assert!(save_raw_cached_banner_image(&cache_path, &expected));
        assert!(save_raw_cached_banner_image(
            &stale_path,
            &test_rgba([9, 8, 7, 6])
        ));

        let rgba = load_or_build_cached_dynamic_image(&src, opts, &cache_dir)
            .expect("cache hit should load cached image");

        assert_eq!(rgba, expected);
        assert!(stale_path.is_file());
    }

    #[test]
    fn cache_write_prunes_stale_variants() {
        let dir = TempDir::new("cache-write-prune");
        let src = dir.path().join("banner.png");
        let cache_dir = dir.path().join("cache");
        let opts = BannerCacheOptions { enabled: true };
        let current = test_rgba([4, 3, 2, 1]);

        write_test_png(&src, [4, 3, 2, 1]);
        let (cache_path, path_hex) = dynamic_image_cache_path_for(&src, opts, &cache_dir).unwrap();
        let stale_path = cache_path
            .parent()
            .unwrap()
            .join(format!("{path_hex}-eeeeeeeeeeeeeeee.rgba"));
        assert!(save_raw_cached_banner_image(
            &stale_path,
            &test_rgba([7, 7, 7, 7])
        ));

        save_cached_banner_image(&cache_path, &path_hex, &current);

        assert!(cache_path.is_file());
        assert!(!stale_path.exists());
    }

    #[test]
    fn texture_upload_queue_replaces_existing_key_without_dup_order() {
        let mut queue = TextureUploadQueue::default();
        queue.push(
            "shared".to_string(),
            Arc::new(blank_rgba(1, 1)),
            SamplerDesc::default(),
        );
        queue.push(
            "shared".to_string(),
            Arc::new(blank_rgba(2, 2)),
            SamplerDesc::default(),
        );
        queue.push(
            "other".to_string(),
            Arc::new(blank_rgba(1, 1)),
            SamplerDesc::default(),
        );

        assert_eq!(queue.entries.len(), 2);
        assert_eq!(queue.queued_bytes, (2 * 2 * 4 + 1 * 1 * 4) as usize);

        let budget = TextureUploadBudget {
            max_uploads: 4,
            max_bytes: 64,
        };
        let (first_key, first) = queue.pop_next(budget, 0, 0).unwrap();
        assert_eq!(first_key, "shared");
        assert_eq!(first.bytes, (2 * 2 * 4) as usize);

        let (second_key, second) = queue.pop_next(budget, 1, first.bytes).unwrap();
        assert_eq!(second_key, "other");
        assert_eq!(second.bytes, 4);
        assert!(
            queue
                .pop_next(budget, 2, first.bytes + second.bytes)
                .is_none()
        );
    }

    #[test]
    fn texture_upload_queue_allows_one_oversize_upload_then_stops_at_budget() {
        let mut queue = TextureUploadQueue::default();
        queue.push(
            "big".to_string(),
            Arc::new(blank_rgba(3, 1)),
            SamplerDesc::default(),
        );
        queue.push(
            "small".to_string(),
            Arc::new(blank_rgba(1, 1)),
            SamplerDesc::default(),
        );

        let budget = TextureUploadBudget {
            max_uploads: 1,
            max_bytes: 8,
        };
        let (first_key, first) = queue.pop_next(budget, 0, 0).unwrap();
        assert_eq!(first_key, "big");
        assert_eq!(first.bytes, 12);
        assert!(queue.pop_next(budget, 1, first.bytes).is_none());
        assert!(queue.entries.contains_key("small"));
    }

    #[test]
    fn remove_texture_cancels_pending_upload_for_reserved_handle() {
        let mut assets = AssetManager::new();
        assets.queue_texture_upload("queued".to_string(), blank_rgba(2, 2));

        assert!(assets.has_texture_key("queued"));
        assert!(
            assets
                .pending_texture_uploads
                .entries
                .contains_key("queued")
        );

        assert!(assets.remove_texture("queued").is_none());
        assert!(!assets.has_texture_key("queued"));
        assert!(
            !assets
                .pending_texture_uploads
                .entries
                .contains_key("queued")
        );
    }
}
