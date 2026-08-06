use crate::{
    GeneratedTexture, TexMeta, clear_texture_handles, generated_texture, register_texture_dims,
    register_texture_handle, remove_texture_handle, take_pending_generated_texture_keys,
    upload::{PendingTextureUpload, TextureUploadBudget, TextureUploadKey, TextureUploadQueue},
};
use deadlib_render::{SamplerDesc, TextureHandle, TextureHandleMap};
use image::RgbaImage;
use rustc_hash::FxHashMap;
use std::sync::{Arc, mpsc::SyncSender};

pub struct TextureStore<T> {
    textures: TextureHandleMap<T>,
    uploaded_texture_dims: TextureHandleMap<TexMeta>,
    texture_handles: FxHashMap<String, TextureHandle>,
    next_texture_handle: TextureHandle,
    pending_texture_uploads: TextureUploadQueue,
}

impl<T> TextureStore<T> {
    pub fn new() -> Self {
        Self {
            textures: TextureHandleMap::default(),
            uploaded_texture_dims: TextureHandleMap::default(),
            texture_handles: FxHashMap::default(),
            next_texture_handle: 1,
            pending_texture_uploads: TextureUploadQueue::default(),
        }
    }

    #[inline(always)]
    pub fn textures(&self) -> &TextureHandleMap<T> {
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
    pub fn has_pending_texture_upload(&self, key: &str) -> bool {
        self.pending_texture_uploads.contains(key)
    }

    #[inline(always)]
    pub fn texture_handle(&self, key: &str) -> Option<TextureHandle> {
        self.texture_handles.get(key).copied()
    }

    pub fn take_textures(&mut self) -> TextureHandleMap<T> {
        self.texture_handles.clear();
        clear_texture_handles();
        self.uploaded_texture_dims.clear();
        std::mem::take(&mut self.textures)
    }

    pub fn reserve_texture_handle(&mut self, key: String) -> TextureHandle {
        match self.texture_handles.entry(key) {
            std::collections::hash_map::Entry::Occupied(entry) => *entry.get(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                let handle = self.next_texture_handle;
                self.next_texture_handle = self.next_texture_handle.wrapping_add(1).max(1);
                register_texture_handle(entry.key(), handle);
                entry.insert(handle);
                handle
            }
        }
    }

    pub fn insert_texture(
        &mut self,
        key: String,
        texture: T,
        width: u32,
        height: u32,
    ) -> Option<T> {
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

    pub fn remove_texture(&mut self, key: &str) -> Option<(TextureHandle, T)> {
        self.pending_texture_uploads.remove(key);
        let handle = self.texture_handles.remove(key)?;
        remove_texture_handle(key);
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
        self.pending_texture_uploads.remove(&key);
        let handle = self.reserve_texture_handle(key);
        self.uploaded_texture_dims.insert(
            handle,
            TexMeta {
                w: width,
                h: height,
            },
        );
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

    #[inline(always)]
    fn uploaded_texture_dims_match(&self, key: &str, width: u32, height: u32) -> bool {
        self.texture_handles
            .get(key)
            .and_then(|handle| self.uploaded_texture_dims.get(handle))
            .is_some_and(|meta| meta.w == width && meta.h == height)
    }

    pub fn queue_texture_upload_shared(
        &mut self,
        key: String,
        image: Arc<RgbaImage>,
        sampler: SamplerDesc,
    ) {
        self.reserve_texture_handle(key.clone());
        register_texture_dims(&key, image.width(), image.height());
        self.pending_texture_uploads.push(key, image, sampler);
    }

    pub fn queue_texture_upload(&mut self, key: String, image: RgbaImage) {
        self.queue_texture_upload_with_sampler(key, image, SamplerDesc::default());
    }

    pub fn queue_recyclable_texture_upload_shared(
        &mut self,
        key: Arc<str>,
        image: RgbaImage,
        recycle_tx: SyncSender<Vec<u8>>,
    ) {
        if !self.uploaded_texture_dims_match(&key, image.width(), image.height()) {
            register_texture_dims(&key, image.width(), image.height());
        }
        self.pending_texture_uploads.push_recyclable_shared(
            key,
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
        for key in take_pending_generated_texture_keys() {
            let Some(GeneratedTexture { image, sampler }) = generated_texture(&key) else {
                continue;
            };
            self.queue_texture_upload_shared(key, image, sampler);
        }
    }

    pub fn pop_next_upload(
        &mut self,
        budget: TextureUploadBudget,
        drained_uploads: usize,
        drained_bytes: usize,
    ) -> Option<(TextureUploadKey, PendingTextureUpload)> {
        self.pending_texture_uploads
            .pop_next(budget, drained_uploads, drained_bytes)
    }
}

impl<T> Default for TextureStore<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct VideoTextureMetadataBenchmark {
    textures: TextureStore<()>,
    key: Arc<str>,
    width: u32,
    height: u32,
}

#[cfg(feature = "bench-support")]
impl VideoTextureMetadataBenchmark {
    pub fn new(width: u32, height: u32) -> Self {
        let key: Arc<str> = Arc::from("__gameplay_video_metadata_benchmark__");
        let mut textures = TextureStore::new();
        textures.insert_texture(key.to_string(), (), width, height);
        register_texture_dims(&key, width, height);
        Self {
            textures,
            key,
            width,
            height,
        }
    }

    pub fn global_frame(&self) -> u64 {
        register_texture_dims(&self.key, self.width, self.height);
        self.checksum()
    }

    pub fn local_frame(&self) -> u64 {
        if !self
            .textures
            .uploaded_texture_dims_match(&self.key, self.width, self.height)
        {
            register_texture_dims(&self.key, self.width, self.height);
        }
        self.checksum()
    }

    fn checksum(&self) -> u64 {
        self.textures.texture_handle(&self.key).unwrap_or_default()
            ^ u64::from(self.width).rotate_left(17)
            ^ u64::from(self.height).rotate_left(37)
    }
}

#[cfg(any(test, feature = "bench-support"))]
pub fn texture_handle_reservation_workload_for_bench(
    keys: &[&str],
    replacements: usize,
    lookup_rounds: usize,
) -> u64 {
    let mut handles = FxHashMap::<String, TextureHandle>::default();
    let mut next_handle: TextureHandle = 1;

    for _ in 0..=replacements {
        for &key in keys {
            match handles.entry(key.to_string()) {
                std::collections::hash_map::Entry::Occupied(_) => {}
                std::collections::hash_map::Entry::Vacant(entry) => {
                    let handle = next_handle;
                    next_handle = next_handle.wrapping_add(1).max(1);
                    entry.insert(handle);
                }
            }
        }
    }

    texture_handle_workload_checksum(&handles, keys, lookup_rounds, next_handle)
}

#[cfg(any(test, feature = "bench-support"))]
pub fn texture_handle_reservation_workload_legacy_for_bench(
    keys: &[&str],
    replacements: usize,
    lookup_rounds: usize,
) -> u64 {
    let mut handles = std::collections::HashMap::<String, TextureHandle>::new();
    let mut next_handle: TextureHandle = 1;

    for _ in 0..=replacements {
        for &key in keys {
            let key = key.to_string();
            if !handles.contains_key(&key) {
                let handle = next_handle;
                next_handle = next_handle.wrapping_add(1).max(1);
                handles.insert(key.clone(), handle);
            }
        }
    }

    texture_handle_workload_checksum(&handles, keys, lookup_rounds, next_handle)
}

#[cfg(any(test, feature = "bench-support"))]
fn texture_handle_workload_checksum<S>(
    handles: &std::collections::HashMap<String, TextureHandle, S>,
    keys: &[&str],
    lookup_rounds: usize,
    next_handle: TextureHandle,
) -> u64
where
    S: std::hash::BuildHasher,
{
    let mut checksum = handles.len() as u64 ^ next_handle;
    for round in 0..lookup_rounds {
        for (index, &key) in keys.iter().enumerate() {
            checksum = checksum.wrapping_add(
                handles.get(key).copied().unwrap_or_default()
                    ^ (index as u64).rotate_left((round & 31) as u32),
            );
        }
        checksum ^= u64::from(handles.contains_key("__missing_texture_key__"));
    }
    checksum
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

    #[test]
    fn handle_reservation_preserves_handles_and_matches_legacy_workload() {
        let mut textures = TextureStore::<()>::new();
        let first = textures.reserve_texture_handle("__texture_store_parity_first__".to_string());
        let second = textures.reserve_texture_handle("__texture_store_parity_second__".to_string());

        assert_eq!(
            textures.reserve_texture_handle("__texture_store_parity_first__".to_string()),
            first
        );
        assert_ne!(first, second);

        let keys = [
            "graphics/banner 1x1.png",
            "noteskins/dance/tap note 4x1.png",
            "generated/player-1/lifebar",
        ];
        assert_eq!(
            texture_handle_reservation_workload_for_bench(&keys, 4, 7),
            texture_handle_reservation_workload_legacy_for_bench(&keys, 4, 7)
        );

        assert!(
            textures
                .remove_texture("__texture_store_parity_first__")
                .is_none()
        );
        assert!(
            textures
                .remove_texture("__texture_store_parity_second__")
                .is_none()
        );
    }
}
