pub(crate) mod dynamic;
mod error;
mod fonts;
mod textures;

use crate::core::gfx::{
    Backend, INVALID_TEXTURE_HANDLE, ObjectType, RenderList, SamplerDesc, Texture as GfxTexture,
    TextureHandle,
};
use crate::core::ui::font::Font;
use image::RgbaImage;
use std::collections::HashMap;

#[cfg(test)]
pub(crate) use self::dynamic::{
    BannerCacheOptions, collect_stale_dynamic_keys, dedupe_dynamic_keys,
    dynamic_image_cache_path_for, load_or_build_cached_dynamic_image, save_cached_banner_image,
    save_raw_cached_banner_image,
};
pub use self::error::AssetError;
use self::textures::ascii_ci_hash;
#[cfg(test)]
pub(crate) use self::textures::parse_texture_resolution_hint;
pub use self::textures::{
    TexMeta, TextureHints, canonical_texture_key, open_image_fallback, parse_sprite_sheet_dims,
    parse_texture_hints, register_generated_texture, register_texture_dims, sprite_sheet_dims,
    texture_dims, texture_source_dims_from_real, texture_source_frame_dims_from_real,
};

pub struct AssetManager {
    textures: HashMap<TextureHandle, GfxTexture>,
    texture_handles: HashMap<String, TextureHandle>,
    texture_handles_ascii_ci: HashMap<u64, TextureHandle>,
    next_texture_handle: TextureHandle,
    fonts: HashMap<&'static str, Font>,
}

impl AssetManager {
    pub fn new() -> Self {
        Self {
            textures: HashMap::new(),
            texture_handles: HashMap::new(),
            texture_handles_ascii_ci: HashMap::new(),
            next_texture_handle: 1,
            fonts: HashMap::new(),
        }
    }

    pub fn register_font(&mut self, name: &'static str, font: Font) {
        self.fonts.insert(name, font);
    }

    pub const fn fonts(&self) -> &HashMap<&'static str, Font> {
        &self.fonts
    }

    #[inline(always)]
    pub fn textures(&self) -> &HashMap<TextureHandle, GfxTexture> {
        &self.textures
    }

    #[inline(always)]
    pub fn has_texture_key(&self, key: &str) -> bool {
        self.texture_handles.contains_key(key)
    }

    pub fn take_textures(&mut self) -> HashMap<TextureHandle, GfxTexture> {
        self.texture_handles.clear();
        self.texture_handles_ascii_ci.clear();
        std::mem::take(&mut self.textures)
    }

    #[inline(always)]
    pub fn texture_handle_for_key(&self, key: &str) -> TextureHandle {
        if let Some(handle) = self.texture_handles.get(key) {
            return *handle;
        }
        if let Some(handle) = self.texture_handles_ascii_ci.get(&ascii_ci_hash(key))
            && *handle != INVALID_TEXTURE_HANDLE
        {
            return *handle;
        }
        self.texture_handles
            .iter()
            .find_map(|(candidate, handle)| candidate.eq_ignore_ascii_case(key).then_some(*handle))
            .unwrap_or(INVALID_TEXTURE_HANDLE)
    }

    pub fn resolve_render_textures(&self, render: &mut RenderList<'_>) {
        #[inline(always)]
        fn texture_key<'a>(obj: &'a crate::core::gfx::RenderObject<'a>) -> Option<&'a str> {
            match &obj.object_type {
                ObjectType::Sprite { texture_id, .. }
                | ObjectType::TexturedMesh { texture_id, .. } => Some(texture_id.as_ref()),
                ObjectType::Mesh { .. } => None,
            }
        }

        let objects = &mut render.objects;
        let mut last_handle = INVALID_TEXTURE_HANDLE;
        for idx in 0..objects.len() {
            let handle = match texture_key(&objects[idx]) {
                Some(key) if idx > 0 && texture_key(&objects[idx - 1]) == Some(key) => last_handle,
                Some(key) => self.texture_handle_for_key(key),
                None => INVALID_TEXTURE_HANDLE,
            };
            objects[idx].texture_handle = handle;
            last_handle = handle;
        }
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
                self.note_texture_handle_alias(&key, handle);
                handle
            }
        }
    }

    pub(crate) fn insert_texture(
        &mut self,
        key: String,
        texture: GfxTexture,
    ) -> Option<GfxTexture> {
        let handle = self.reserve_texture_handle(key);
        self.textures.insert(handle, texture)
    }

    pub(crate) fn remove_texture(&mut self, key: &str) -> Option<(TextureHandle, GfxTexture)> {
        let handle = self.texture_handles.remove(key)?;
        self.rebuild_texture_handle_aliases();
        self.textures
            .remove(&handle)
            .map(|texture| (handle, texture))
    }

    pub(crate) fn dispose_texture(
        &mut self,
        backend: &mut Backend,
        handle: TextureHandle,
        texture: GfxTexture,
    ) {
        let mut textures = HashMap::with_capacity(1);
        textures.insert(handle, texture);
        backend.dispose_textures(&mut textures);
    }

    pub(crate) fn set_texture_for_key(
        &mut self,
        backend: &mut Backend,
        key: String,
        texture: GfxTexture,
    ) -> TextureHandle {
        let handle = self.reserve_texture_handle(key);
        if let Some(old) = self.textures.insert(handle, texture) {
            self.dispose_texture(backend, handle, old);
        }
        handle
    }

    pub(crate) fn update_texture_for_key(
        &mut self,
        backend: &mut Backend,
        key: &str,
        rgba: &RgbaImage,
    ) -> Result<(), AssetError> {
        let dims = texture_dims(key);
        let handle = self.texture_handles.get(key).copied();
        if let (Some(meta), Some(handle)) = (dims, handle)
            && meta.w == rgba.width()
            && meta.h == rgba.height()
            && let Some(texture) = self.textures.get_mut(&handle)
        {
            backend.update_texture(texture, rgba)?;
            return Ok(());
        }

        let texture = backend.create_texture(rgba, SamplerDesc::default())?;
        self.set_texture_for_key(backend, key.to_string(), texture);
        register_texture_dims(key, rgba.width(), rgba.height());
        Ok(())
    }

    fn note_texture_handle_alias(&mut self, key: &str, handle: TextureHandle) {
        let folded = ascii_ci_hash(key);
        match self.texture_handles_ascii_ci.get_mut(&folded) {
            Some(existing) if *existing != handle => *existing = INVALID_TEXTURE_HANDLE,
            Some(_) => {}
            None => {
                self.texture_handles_ascii_ci.insert(folded, handle);
            }
        }
    }

    fn rebuild_texture_handle_aliases(&mut self) {
        self.texture_handles_ascii_ci.clear();
        self.texture_handles_ascii_ci
            .reserve(self.texture_handles.len());
        for (key, &handle) in &self.texture_handles {
            let folded = ascii_ci_hash(key);
            match self.texture_handles_ascii_ci.get_mut(&folded) {
                Some(existing) if *existing != handle => *existing = INVALID_TEXTURE_HANDLE,
                Some(_) => {}
                None => {
                    self.texture_handles_ascii_ci.insert(folded, handle);
                }
            }
        }
    }

    pub fn load_initial_assets(&mut self, backend: &mut Backend) -> Result<(), AssetError> {
        self.load_initial_textures(backend)?;
        self.load_initial_fonts(backend)?;
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
    use std::{
        collections::HashSet,
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicUsize, Ordering},
    };

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
    fn collect_stale_dynamic_keys_skips_desired_entries() {
        let current = vec![
            "keep.mp4".to_string(),
            "drop-a.mp4".to_string(),
            "drop-b.mp4".to_string(),
        ];
        let desired = HashSet::from(["keep.mp4".to_string()]);
        assert_eq!(
            collect_stale_dynamic_keys(current.iter(), &desired),
            vec!["drop-a.mp4".to_string(), "drop-b.mp4".to_string()]
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
        let (cache_path, path_hex) =
            dynamic_image_cache_path_for(&src, opts, cache_dir.to_str().unwrap()).unwrap();
        let stale_path = cache_path
            .parent()
            .unwrap()
            .join(format!("{path_hex}-ffffffffffffffff.rgba"));
        assert!(save_raw_cached_banner_image(&cache_path, &expected));
        assert!(save_raw_cached_banner_image(
            &stale_path,
            &test_rgba([9, 8, 7, 6])
        ));

        let rgba = load_or_build_cached_dynamic_image(&src, opts, cache_dir.to_str().unwrap())
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
        let (cache_path, path_hex) =
            dynamic_image_cache_path_for(&src, opts, cache_dir.to_str().unwrap()).unwrap();
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
}
