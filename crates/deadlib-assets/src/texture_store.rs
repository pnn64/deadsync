use crate::registry::{
    drain_pending_generated_textures, next_texture_revision, register_texture_dims_shared,
    reserve_texture_metadata,
};
use crate::{
    GeneratedTexture, TexMeta, ascii_ci_hash, parse_sprite_sheet_dims, register_texture_dims,
    texture_dims, texture_registry_generation,
    upload::{PendingTextureUpload, TextureUploadBudget, TextureUploadQueue},
};
use deadlib_present::texture::{TextureContext, TextureMeta};
use deadlib_render_core::{
    FastU64Map, INVALID_TEXTURE_HANDLE, SamplerDesc, TextureHandle, TextureHandleMap,
};
use deadlib_video::Yuv420Image;
use image::RgbaImage;
use rustc_hash::FxHashMap;
use std::cell::Cell;
use std::sync::{Arc, mpsc::SyncSender};

/// Resolved sprite identity and sizing, retained until the store revision changes.
#[derive(Clone, Copy, Debug)]
pub struct BoundTexture {
    pub handle: TextureHandle,
    /// None while a reserved name has neither decoded metadata nor an upload.
    pub dimensions: Option<TexMeta>,
    pub sheet: (u32, u32),
}

/// Application-owned GPU identities and pending uploads, borrowed by presentation.
///
/// The render/asset owner mutates this store at load, upload, and release boundaries;
/// borrowed views do no I/O. Names, sheet grids, and aliases have one entry per live
/// asset (at most one alias per folded name), reserved during initial loading.
/// They live with the store and are removed explicitly with their textures; no
/// draw-time insertion or eviction occurs. Exact and unique alias lookups are O(1),
/// missing names return an invalid handle. Ambiguous aliases can scan the live set
/// and rebuild on removal. Metadata-only sizing remains available before upload.
/// Revision checks use an atomic metadata stamp and local cells, never a registry
/// lock. Same-size uploads preserve bindings; identity and sizing changes invalidate
/// them. There are no live counters; binding/lifecycle tests cover invalidation.
pub struct TextureStore<T> {
    textures: TextureHandleMap<T>,
    uploaded_texture_dims: TextureHandleMap<TexMeta>,
    texture_handles: FxHashMap<Arc<str>, TextureHandle>,
    texture_keys: TextureHandleMap<Arc<str>>,
    next_texture_handle: TextureHandle,
    texture_aliases: FastU64Map<TextureHandle>,
    sheets: TextureHandleMap<(u32, u32)>,
    revision: Cell<u64>,
    metadata_revision: Cell<u64>,
    pending_texture_uploads: TextureUploadQueue,
}

impl<T> TextureStore<T> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            textures: TextureHandleMap::default(),
            uploaded_texture_dims: TextureHandleMap::default(),
            texture_handles: FxHashMap::default(),
            texture_keys: TextureHandleMap::default(),
            next_texture_handle: 1,
            texture_aliases: FastU64Map::default(),
            sheets: TextureHandleMap::default(),
            revision: Cell::new(next_texture_revision()),
            metadata_revision: Cell::new(texture_registry_generation()),
            pending_texture_uploads: TextureUploadQueue::default(),
        }
    }

    /// Revision for cached bindings; independent stores always have distinct stamps.
    pub fn revision(&self) -> u64 {
        let metadata = texture_registry_generation();
        if self.metadata_revision.replace(metadata) != metadata {
            self.revision.set(next_texture_revision());
        }
        self.revision.get()
    }

    /// Resolves an exact name, with the existing ASCII-insensitive fallback.
    pub fn texture_handle(&self, key: &str) -> TextureHandle {
        if let Some(&handle) = self.texture_handles.get(key) {
            return handle;
        }
        let Some(&alias) = self.texture_aliases.get(&ascii_ci_hash(key)) else {
            return INVALID_TEXTURE_HANDLE;
        };
        if alias != INVALID_TEXTURE_HANDLE {
            return if self.texture_key(alias).eq_ignore_ascii_case(key) {
                alias
            } else {
                INVALID_TEXTURE_HANDLE
            };
        }
        self.texture_handles
            .iter()
            .find_map(|(candidate, &handle)| candidate.eq_ignore_ascii_case(key).then_some(handle))
            .unwrap_or(INVALID_TEXTURE_HANDLE)
    }

    fn dimensions(&self, handle: TextureHandle) -> Option<TexMeta> {
        self.pending_texture_uploads
            .dimensions(handle)
            .map(|(w, h)| TexMeta { w, h })
            .or_else(|| self.uploaded_texture_dims.get(&handle).copied())
    }

    /// Binds a reserved identity; metadata alone never invents a GPU handle.
    ///
    /// A queued upload can already supply native dimensions. Rendering may use
    /// that handle once the application's upload drain installs the texture.
    pub fn bind_texture(&self, key: &str) -> Option<BoundTexture> {
        let handle = self.texture_handle(key);
        if handle == INVALID_TEXTURE_HANDLE {
            return None;
        }
        Some(BoundTexture {
            handle,
            dimensions: self
                .dimensions(handle)
                .or_else(|| texture_dims(self.texture_key(handle))),
            sheet: *self.sheets.get(&handle)?,
        })
    }

    #[inline(always)]
    #[must_use]
    pub const fn textures(&self) -> &TextureHandleMap<T> {
        &self.textures
    }

    #[inline(always)]
    #[must_use]
    pub fn has_texture_key(&self, key: &str) -> bool {
        self.texture_handles.contains_key(key)
    }

    #[inline(always)]
    #[must_use]
    pub fn has_uploaded_texture_key(&self, key: &str) -> bool {
        self.texture_handles
            .get(key)
            .is_some_and(|handle| self.textures.contains_key(handle))
    }

    #[inline(always)]
    #[must_use]
    pub fn has_pending_texture_upload(&self, key: &str) -> bool {
        self.texture_handles
            .get(key)
            .is_some_and(|&handle| self.pending_texture_uploads.contains(handle))
    }

    #[inline(always)]
    #[must_use]
    pub fn has_pending_texture_upload_handle(&self, handle: TextureHandle) -> bool {
        self.pending_texture_uploads.contains(handle)
    }

    pub fn take_textures(&mut self) -> TextureHandleMap<T> {
        self.texture_handles.clear();
        self.texture_keys.clear();
        self.texture_aliases.clear();
        self.sheets.clear();
        self.pending_texture_uploads = TextureUploadQueue::default();
        self.revision.set(next_texture_revision());
        self.uploaded_texture_dims.clear();
        std::mem::take(&mut self.textures)
    }

    pub(crate) fn reserve_initial_textures(&mut self, additional: usize) {
        let dense_additional = additional.saturating_add(1);
        self.textures.reserve(dense_additional);
        self.uploaded_texture_dims.reserve(dense_additional);
        self.texture_handles.reserve(additional);
        self.texture_keys.reserve(dense_additional);
        self.texture_aliases.reserve(additional);
        self.sheets.reserve(dense_additional);
        reserve_texture_metadata(additional);
    }

    pub fn reserve_texture_handle(&mut self, key: String) -> TextureHandle {
        if let Some(&handle) = self.texture_handles.get(key.as_str()) {
            return handle;
        }
        self.reserve_new_texture_handle(Arc::from(key))
    }

    fn reserve_new_texture_handle(&mut self, key: Arc<str>) -> TextureHandle {
        let handle = self.next_texture_handle;
        self.next_texture_handle = self.next_texture_handle.wrapping_add(1).max(1);
        note_texture_handle_alias(&mut self.texture_aliases, &key, handle);
        self.sheets.insert(handle, parse_sprite_sheet_dims(&key));
        self.revision.set(next_texture_revision());
        self.texture_keys.insert(handle, Arc::clone(&key));
        self.texture_handles.insert(key, handle);
        handle
    }

    pub fn insert_texture(
        &mut self,
        key: String,
        texture: T,
        width: u32,
        height: u32,
    ) -> Option<T> {
        let handle = self.reserve_texture_handle(key);
        if !self
            .uploaded_texture_dims
            .get(&handle)
            .is_some_and(|meta| meta.w == width && meta.h == height)
        {
            self.revision.set(next_texture_revision());
        }
        self.uploaded_texture_dims.insert(
            handle,
            TexMeta {
                w: width,
                h: height,
            },
        );
        self.textures.insert(handle, texture)
    }

    pub fn remove_texture(&mut self, key: &str) -> Option<(TextureHandle, T)> {
        let handle = self.texture_handles.remove(key)?;
        self.pending_texture_uploads.remove(handle);
        self.texture_keys.remove(&handle);
        remove_texture_handle_alias(&self.texture_handles, &mut self.texture_aliases, key);
        self.sheets.remove(&handle);
        self.revision.set(next_texture_revision());
        self.uploaded_texture_dims.remove(&handle);
        self.textures
            .remove(&handle)
            .map(|texture| (handle, texture))
    }

    pub fn set_texture_for_key(
        &mut self,
        key: String,
        texture: T,
        width: u32,
        height: u32,
    ) -> (TextureHandle, Option<T>) {
        let handle = self.reserve_texture_handle(key);
        if !self.upload_dims_match(handle, width, height) {
            self.revision.set(next_texture_revision());
        }
        self.uploaded_texture_dims.insert(
            handle,
            TexMeta {
                w: width,
                h: height,
            },
        );
        self.pending_texture_uploads.remove(handle);
        let old = self.textures.insert(handle, texture);
        (handle, old)
    }

    pub fn uploaded_texture_mut(&mut self, key: &str, width: u32, height: u32) -> Option<&mut T> {
        let handle = self.texture_handles.get(key).copied()?;
        let meta = self.uploaded_texture_dims.get(&handle).copied()?;
        if meta.w == width && meta.h == height {
            self.textures.get_mut(&handle)
        } else {
            None
        }
    }

    #[cfg(test)]
    fn uploaded_texture_dims_match(&self, key: &str, width: u32, height: u32) -> bool {
        self.texture_handles
            .get(key)
            .and_then(|handle| self.uploaded_texture_dims.get(handle))
            .is_some_and(|meta| meta.w == width && meta.h == height)
    }

    #[inline(always)]
    fn uploaded_texture_mut_by_handle(
        &mut self,
        handle: TextureHandle,
        width: u32,
        height: u32,
    ) -> Option<&mut T> {
        let meta = self.uploaded_texture_dims.get(&handle).copied()?;
        if meta.w == width && meta.h == height {
            self.textures.get_mut(&handle)
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn texture_key(&self, handle: TextureHandle) -> &str {
        self.texture_keys
            .get(&handle)
            .map(AsRef::as_ref)
            .unwrap_or("<released texture>")
    }

    pub fn queue_texture_upload_shared(
        &mut self,
        key: String,
        image: Arc<RgbaImage>,
        sampler: SamplerDesc,
    ) {
        let (width, height) = (image.width(), image.height());
        let handle = if let Some(&handle) = self.texture_handles.get(key.as_str()) {
            if !self.upload_dims_match(handle, width, height) {
                self.revision.set(next_texture_revision());
                register_texture_dims(&key, width, height);
            }
            handle
        } else {
            let key: Arc<str> = Arc::from(key);
            register_texture_dims_shared(Arc::clone(&key), width, height);
            self.reserve_new_texture_handle(key)
        };
        self.pending_texture_uploads.push(handle, image, sampler);
    }

    #[inline(always)]
    fn upload_dims_match(&self, handle: TextureHandle, width: u32, height: u32) -> bool {
        self.pending_texture_uploads
            .dimensions(handle)
            .or_else(|| {
                self.uploaded_texture_dims
                    .get(&handle)
                    .map(|meta| (meta.w, meta.h))
            })
            .is_some_and(|dims| dims == (width, height))
    }

    pub fn queue_texture_upload(&mut self, key: String, image: RgbaImage) {
        self.queue_texture_upload_with_sampler(key, image, SamplerDesc::default());
    }

    pub fn queue_recyclable_texture_upload(
        &mut self,
        handle: TextureHandle,
        image: RgbaImage,
        recycle_tx: SyncSender<Vec<u8>>,
    ) {
        let dimensions_match = self.upload_dims_match(handle, image.width(), image.height());
        if !dimensions_match {
            self.revision.set(next_texture_revision());
            register_texture_dims(self.texture_key(handle), image.width(), image.height());
        }
        self.pending_texture_uploads.push_recyclable(
            handle,
            image,
            SamplerDesc::default(),
            recycle_tx,
        );
    }

    pub fn queue_recyclable_yuv420_upload(
        &mut self,
        handle: TextureHandle,
        image: Yuv420Image,
        recycle_tx: SyncSender<Vec<u8>>,
    ) {
        let dimensions_match = self.upload_dims_match(handle, image.width(), image.height());
        if !dimensions_match {
            self.revision.set(next_texture_revision());
            register_texture_dims(self.texture_key(handle), image.width(), image.height());
        }
        self.pending_texture_uploads.push_recyclable_yuv420(
            handle,
            image,
            SamplerDesc::default(),
            recycle_tx,
        );
    }

    pub fn queue_texture_upload_with_sampler(
        &mut self,
        key: String,
        image: RgbaImage,
        sampler: SamplerDesc,
    ) {
        self.queue_texture_upload_shared(key, Arc::new(image), sampler);
    }

    pub fn queue_pending_generated_textures(&mut self) {
        drain_pending_generated_textures(|key, GeneratedTexture { image, sampler }| {
            let (width, height) = (image.width(), image.height());
            let handle = if let Some(&handle) = self.texture_handles.get(key.as_ref()) {
                if !self.upload_dims_match(handle, width, height) {
                    self.revision.set(next_texture_revision());
                    register_texture_dims(&key, width, height);
                }
                handle
            } else {
                register_texture_dims_shared(Arc::clone(&key), width, height);
                self.reserve_new_texture_handle(key)
            };
            self.pending_texture_uploads.push(handle, image, sampler);
        });
    }

    pub fn pop_next_upload(
        &mut self,
        budget: TextureUploadBudget,
        drained_uploads: usize,
        drained_bytes: usize,
    ) -> Option<(TextureHandle, PendingTextureUpload)> {
        let next = self
            .pending_texture_uploads
            .pop_next(budget, drained_uploads, drained_bytes)?;
        let (handle, upload) = &next;
        let (width, height) = (upload.image().width(), upload.image().height());
        if !self.upload_dims_match(*handle, width, height) {
            self.revision.set(next_texture_revision());
        }
        Some(next)
    }

    pub fn apply_upload_update(
        &mut self,
        handle: TextureHandle,
        width: u32,
        height: u32,
    ) -> Option<&mut T> {
        self.uploaded_texture_mut_by_handle(handle, width, height)
    }

    pub fn set_texture_for_handle(
        &mut self,
        handle: TextureHandle,
        texture: T,
        width: u32,
        height: u32,
    ) -> Option<T> {
        if !self
            .uploaded_texture_dims
            .get(&handle)
            .is_some_and(|meta| meta.w == width && meta.h == height)
        {
            self.revision.set(next_texture_revision());
        }
        self.uploaded_texture_dims.insert(
            handle,
            TexMeta {
                w: width,
                h: height,
            },
        );
        self.textures.insert(handle, texture)
    }
}

impl<T> TextureContext for TextureStore<T> {
    fn texture_registry_generation(&self) -> u64 {
        self.revision()
    }

    fn texture_handle(&self, key: &str) -> TextureHandle {
        self.texture_handle(key)
    }

    fn texture_dims(&self, key: &str) -> Option<TextureMeta> {
        let handle = self.texture_handle(key);
        self.dimensions(handle)
            .or_else(|| {
                texture_dims(if handle == INVALID_TEXTURE_HANDLE {
                    key
                } else {
                    self.texture_key(handle)
                })
            })
            .map(|meta| TextureMeta {
                w: meta.w,
                h: meta.h,
            })
    }

    fn sprite_sheet_dims(&self, key: &str) -> (u32, u32) {
        self.sheets
            .get(&self.texture_handle(key))
            .copied()
            .unwrap_or_else(|| crate::sprite_sheet_dims(key))
    }
}

fn note_texture_handle_alias(
    aliases: &mut FastU64Map<TextureHandle>,
    key: &str,
    handle: TextureHandle,
) {
    // Each local key has its own handle, so a second folded key is ambiguous.
    aliases
        .entry(ascii_ci_hash(key))
        .and_modify(|alias| *alias = INVALID_TEXTURE_HANDLE)
        .or_insert(handle);
}

fn rebuild_texture_handle_aliases(
    handles: &FxHashMap<Arc<str>, TextureHandle>,
    aliases: &mut FastU64Map<TextureHandle>,
) {
    aliases.clear();
    aliases.reserve(handles.len());
    for (key, &handle) in handles {
        note_texture_handle_alias(aliases, key, handle);
    }
}

/// Remove a common unique alias in O(1). An already-colliding alias takes the
/// rare rebuild path so deleting one collision restores exact fallback lookup.
fn remove_texture_handle_alias(
    handles: &FxHashMap<Arc<str>, TextureHandle>,
    aliases: &mut FastU64Map<TextureHandle>,
    key: &str,
) {
    let folded = ascii_ci_hash(key);
    if aliases.get(&folded) == Some(&INVALID_TEXTURE_HANDLE) {
        rebuild_texture_handle_aliases(handles, aliases);
    } else {
        aliases.remove(&folded);
    }
}

impl<T> Default for TextureStore<T> {
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
        let mut textures = TextureStore::<()>::new();
        textures.queue_texture_upload("queued".to_string(), blank_rgba(2, 2));

        assert!(textures.has_texture_key("queued"));
        assert!(textures.has_pending_texture_upload("queued"));

        assert!(textures.remove_texture("queued").is_none());
        assert!(!textures.has_texture_key("queued"));
        assert!(!textures.has_pending_texture_upload("queued"));
    }

    #[test]
    fn shared_upload_key_keeps_handle_and_metadata_in_sync() {
        let key = "shared-key-ownership-test";
        let mut textures = TextureStore::<()>::new();
        textures.queue_texture_upload_shared(
            key.to_string(),
            Arc::new(blank_rgba(2, 2)),
            SamplerDesc::default(),
        );
        let first_handle = textures.reserve_texture_handle(key.to_string());
        assert!(textures.upload_dims_match(first_handle, 2, 2));

        textures.queue_texture_upload_shared(
            key.to_string(),
            Arc::new(blank_rgba(4, 2)),
            SamplerDesc::default(),
        );
        let second_handle = textures.reserve_texture_handle(key.to_string());

        assert_eq!(first_handle, second_handle);
        assert!(!textures.upload_dims_match(second_handle, 2, 2));
        assert!(textures.upload_dims_match(second_handle, 4, 2));
        assert_eq!(textures.texture_handle(key), first_handle);
        let dims = crate::texture_dims(key).unwrap();
        assert_eq!((dims.w, dims.h), (4, 2));
    }

    #[test]
    fn uploaded_dimensions_distinguish_steady_video_frames_from_resizes() {
        let mut textures = TextureStore::<()>::new();
        textures.insert_texture("video-frame".to_string(), (), 640, 360);

        assert!(textures.uploaded_texture_dims_match("video-frame", 640, 360));
        assert!(!textures.uploaded_texture_dims_match("video-frame", 1280, 720));
        assert!(!textures.uploaded_texture_dims_match("missing-video", 640, 360));

        textures.set_texture_for_key("video-frame".to_string(), (), 1280, 720);
        assert!(textures.uploaded_texture_dims_match("video-frame", 1280, 720));
        assert!(!textures.uploaded_texture_dims_match("video-frame", 640, 360));
    }
}
