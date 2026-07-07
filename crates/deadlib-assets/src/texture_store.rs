use crate::{
    GeneratedTexture, TexMeta, clear_texture_handles, generated_texture, register_texture_dims,
    register_texture_handle, remove_texture_handle, take_pending_generated_texture_keys,
    upload::{PendingTextureUpload, TextureUploadBudget, TextureUploadQueue},
};
use deadlib_render::{SamplerDesc, TextureHandle, TextureHandleMap};
use image::RgbaImage;
use std::{collections::HashMap, sync::Arc};

pub struct TextureStore<T> {
    textures: TextureHandleMap<T>,
    uploaded_texture_dims: TextureHandleMap<TexMeta>,
    texture_handles: HashMap<String, TextureHandle>,
    next_texture_handle: TextureHandle,
    pending_texture_uploads: TextureUploadQueue,
}

impl<T> TextureStore<T> {
    pub fn new() -> Self {
        Self {
            textures: TextureHandleMap::default(),
            uploaded_texture_dims: TextureHandleMap::default(),
            texture_handles: HashMap::new(),
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

    #[inline(always)]
    fn alloc_texture_handle(&mut self) -> TextureHandle {
        let handle = self.next_texture_handle;
        self.next_texture_handle = self.next_texture_handle.wrapping_add(1).max(1);
        handle
    }

    pub fn reserve_texture_handle(&mut self, key: String) -> TextureHandle {
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
    ) -> Option<(String, PendingTextureUpload)> {
        self.pending_texture_uploads
            .pop_next(budget, drained_uploads, drained_bytes)
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
}
