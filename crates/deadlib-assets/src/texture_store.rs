use crate::registry::{
    drain_pending_generated_textures, register_texture_dims_shared, register_texture_handle_shared,
};
use crate::{
    GeneratedTexture, TexMeta, clear_texture_handles, register_texture_dims, remove_texture_handle,
    upload::{PendingTextureUpload, TextureUploadBudget, TextureUploadQueue},
};
use deadlib_render_core::{SamplerDesc, TextureHandle, TextureHandleMap};
use deadlib_video::Yuv420Image;
use image::RgbaImage;
use rustc_hash::FxHashMap;
use std::sync::{Arc, mpsc::SyncSender};

pub struct TextureStore<T> {
    textures: TextureHandleMap<T>,
    uploaded_texture_dims: TextureHandleMap<TexMeta>,
    texture_handles: FxHashMap<Arc<str>, TextureHandle>,
    texture_keys: TextureHandleMap<Arc<str>>,
    next_texture_handle: TextureHandle,
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
            pending_texture_uploads: TextureUploadQueue::default(),
        }
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
        clear_texture_handles();
        self.uploaded_texture_dims.clear();
        std::mem::take(&mut self.textures)
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
        register_texture_handle_shared(Arc::clone(&key), handle);
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
        let handle = self.reserve_texture_handle(key);
        self.pending_texture_uploads.remove(handle);
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

    #[cfg(feature = "bench-support")]
    pub fn queue_texture_upload_shared_reference(
        &mut self,
        key: String,
        image: Arc<RgbaImage>,
        sampler: SamplerDesc,
    ) {
        let handle = self.reserve_texture_handle(key.clone());
        register_texture_dims(&key, image.width(), image.height());
        self.pending_texture_uploads.push(handle, image, sampler);
    }

    #[cfg(feature = "bench-support")]
    pub fn queue_texture_upload_shared_metadata_reference(
        &mut self,
        key: String,
        image: Arc<RgbaImage>,
        sampler: SamplerDesc,
    ) {
        let (width, height) = (image.width(), image.height());
        let handle = if let Some(&handle) = self.texture_handles.get(key.as_str()) {
            register_texture_dims(&key, width, height);
            handle
        } else {
            let key: Arc<str> = Arc::from(key);
            register_texture_dims_shared(Arc::clone(&key), width, height);
            self.reserve_new_texture_handle(key)
        };
        self.pending_texture_uploads.push(handle, image, sampler);
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
        let dimensions_match = self
            .uploaded_texture_dims
            .get(&handle)
            .is_some_and(|meta| meta.w == image.width() && meta.h == image.height());
        if !dimensions_match {
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
        let dimensions_match = self
            .uploaded_texture_dims
            .get(&handle)
            .is_some_and(|meta| meta.w == image.width() && meta.h == image.height());
        if !dimensions_match {
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
        self.pending_texture_uploads
            .pop_next(budget, drained_uploads, drained_bytes)
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
        assert_eq!(crate::texture_handle(key), first_handle);
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
