//! Frozen upload queue from 294b5139a0b28c46973abcbd1a617431ea4c1fac.
use deadlib_render_core::SamplerDesc;
use deadlib_render_core::TextureHandle;
use deadlib_video::Yuv420Image;
use image::RgbaImage;
use rustc_hash::FxHashMap;
use std::{
    collections::{VecDeque, hash_map::Entry},
    sync::{Arc, mpsc::SyncSender},
};

#[derive(Clone, Copy)]
pub struct TextureUploadBudget {
    pub max_uploads: usize,
    pub max_bytes: usize,
}

// Shared test-only inspection preserves visibility of the frozen private fields.
use super::{Item, State};
include!("upload_snapshot.rs");

pub struct PendingTextureUpload {
    image: Option<UploadImage>,
    recycle_tx: Option<SyncSender<Vec<u8>>>,
    pub sampler: SamplerDesc,
    pub source_size: Option<[u32; 2]>,
    pub bytes: usize,
}

enum UploadImage {
    Shared(Arc<RgbaImage>),
    Owned(RgbaImage),
    RecyclableYuv420(Yuv420Image),
}

impl UploadImage {
    fn as_image(&self) -> TextureUploadImage<'_> {
        match self {
            Self::Shared(image) => TextureUploadImage::Rgba(image),
            Self::Owned(image) => TextureUploadImage::Rgba(image),
            Self::RecyclableYuv420(image) => TextureUploadImage::Yuv420(image),
        }
    }
}

#[derive(Clone, Copy)]
pub enum TextureUploadImage<'a> {
    Rgba(&'a RgbaImage),
    Yuv420(&'a Yuv420Image),
}

impl TextureUploadImage<'_> {
    #[inline(always)]
    #[must_use]
    pub fn width(self) -> u32 {
        match self {
            Self::Rgba(image) => image.width(),
            Self::Yuv420(image) => image.width(),
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn height(self) -> u32 {
        match self {
            Self::Rgba(image) => image.height(),
            Self::Yuv420(image) => image.height(),
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn byte_len(self) -> usize {
        match self {
            Self::Rgba(image) => image.as_raw().len(),
            Self::Yuv420(image) => image.as_raw().len(),
        }
    }
}

impl PendingTextureUpload {
    #[inline(always)]
    /// # Panics
    ///
    /// Panics if an internal state invariant is violated.
    pub fn image(&self) -> TextureUploadImage<'_> {
        self.image
            .as_ref()
            .map(UploadImage::as_image)
            .expect("pending texture upload image must be present")
    }
}

impl Drop for PendingTextureUpload {
    fn drop(&mut self) {
        let (Some(image), Some(recycle_tx)) = (self.image.take(), self.recycle_tx.take()) else {
            return;
        };
        let raw = match image {
            UploadImage::Owned(image) => image.into_raw(),
            UploadImage::RecyclableYuv420(image) => image.into_raw(),
            UploadImage::Shared(_) => return,
        };
        let _ = recycle_tx.try_send(raw);
    }
}

#[derive(Default)]
pub struct TextureUploadQueue {
    order: VecDeque<TextureHandle>,
    entries: FxHashMap<TextureHandle, PendingTextureUpload>,
    queued_bytes: usize,
}

impl TextureUploadQueue {
    #[inline(always)]
    #[must_use]
    pub fn contains(&self, handle: TextureHandle) -> bool {
        self.entries.contains_key(&handle)
    }

    #[inline(always)]
    pub(crate) fn dimensions(&self, handle: TextureHandle) -> Option<(u32, u32)> {
        let image = self.entries.get(&handle)?.image();
        Some((image.width(), image.height()))
    }

    pub fn push(&mut self, handle: TextureHandle, image: Arc<RgbaImage>, sampler: SamplerDesc) {
        self.push_inner(handle, UploadImage::Shared(image), sampler, None);
    }

    /// Queue prepared pixels and the original dimensions from the same revision.
    pub fn push_prepared(
        &mut self, handle: TextureHandle, image: Arc<RgbaImage>,
        sampler: SamplerDesc, source_size: [u32; 2],
    ) {
        self.push(handle, image, sampler);
        self.entries.get_mut(&handle).expect("queued upload").source_size = Some(source_size);
    }

    /// Queues uniquely owned pixels without allocating a shared ownership handle.
    pub fn push_owned(&mut self, handle: TextureHandle, image: RgbaImage, sampler: SamplerDesc) {
        self.push_inner(handle, UploadImage::Owned(image), sampler, None);
    }

    pub fn push_recyclable(
        &mut self,
        handle: TextureHandle,
        image: RgbaImage,
        sampler: SamplerDesc,
        recycle_tx: SyncSender<Vec<u8>>,
    ) {
        self.push_inner(handle, UploadImage::Owned(image), sampler, Some(recycle_tx));
    }

    pub fn push_recyclable_yuv420(
        &mut self,
        handle: TextureHandle,
        image: Yuv420Image,
        sampler: SamplerDesc,
        recycle_tx: SyncSender<Vec<u8>>,
    ) {
        self.push_inner(
            handle,
            UploadImage::RecyclableYuv420(image),
            sampler,
            Some(recycle_tx),
        );
    }

    fn push_inner(
        &mut self,
        handle: TextureHandle,
        image: UploadImage,
        sampler: SamplerDesc,
        recycle_tx: Option<SyncSender<Vec<u8>>>,
    ) {
        let bytes = image.as_image().byte_len();
        let upload = PendingTextureUpload {
            image: Some(image),
            recycle_tx,
            sampler,
            source_size: None,
            bytes,
        };
        match self.entries.entry(handle) {
            Entry::Occupied(mut entry) => {
                let old = entry.insert(upload);
                self.queued_bytes = self.queued_bytes.saturating_sub(old.bytes);
            }
            Entry::Vacant(entry) => {
                self.order.push_back(*entry.key());
                entry.insert(upload);
            }
        }
        self.queued_bytes = self.queued_bytes.saturating_add(bytes);
    }

    pub fn remove(&mut self, handle: TextureHandle) {
        if let Some(old) = self.entries.remove(&handle) {
            self.queued_bytes = self.queued_bytes.saturating_sub(old.bytes);
        }
    }

    #[inline]
    pub fn pop_next(
        &mut self,
        budget: TextureUploadBudget,
        drained_uploads: usize,
        drained_bytes: usize,
    ) -> Option<(TextureHandle, PendingTextureUpload)> {
        while let Some(&handle) = self.order.front() {
            let Entry::Occupied(entry) = self.entries.entry(handle) else {
                self.order.pop_front();
                continue;
            };
            let upload_bytes = entry.get().bytes;
            let next_bytes = drained_bytes.saturating_add(upload_bytes);
            let fits_budget =
                drained_uploads < budget.max_uploads && next_bytes <= budget.max_bytes;
            let allow_first =
                drained_uploads == 0 && budget.max_uploads > 0 && budget.max_bytes > 0;
            if !fits_budget && !allow_first {
                return None;
            }
            let (_, upload) = entry.remove_entry();
            self.order.pop_front();
            self.queued_bytes = self.queued_bytes.saturating_sub(upload_bytes);
            return Some((handle, upload));
        }
        None
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    const fn queued_bytes(&self) -> usize {
        self.queued_bytes
    }
}
