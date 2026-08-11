use deadlib_render::SamplerDesc;
use deadlib_render::TextureHandle;
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

pub struct PendingTextureUpload {
    image: Option<UploadImage>,
    recycle_tx: Option<SyncSender<Vec<u8>>>,
    pub sampler: SamplerDesc,
    pub bytes: usize,
}

enum UploadImage {
    Shared(Arc<RgbaImage>),
    Recyclable(RgbaImage),
}

impl UploadImage {
    fn as_image(&self) -> &RgbaImage {
        match self {
            Self::Shared(image) => image,
            Self::Recyclable(image) => image,
        }
    }
}

impl PendingTextureUpload {
    #[inline(always)]
    pub fn image(&self) -> &RgbaImage {
        self.image
            .as_ref()
            .map(UploadImage::as_image)
            .expect("pending texture upload image must be present")
    }
}

impl Drop for PendingTextureUpload {
    fn drop(&mut self) {
        let (Some(UploadImage::Recyclable(image)), Some(recycle_tx)) =
            (self.image.take(), self.recycle_tx.take())
        else {
            return;
        };
        let _ = recycle_tx.try_send(image.into_raw());
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
    pub fn contains(&self, handle: TextureHandle) -> bool {
        self.entries.contains_key(&handle)
    }

    pub fn push(&mut self, handle: TextureHandle, image: Arc<RgbaImage>, sampler: SamplerDesc) {
        self.push_inner(handle, UploadImage::Shared(image), sampler, None);
    }

    pub fn push_recyclable(
        &mut self,
        handle: TextureHandle,
        image: RgbaImage,
        sampler: SamplerDesc,
        recycle_tx: SyncSender<Vec<u8>>,
    ) {
        self.push_inner(
            handle,
            UploadImage::Recyclable(image),
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
        let bytes = image.as_image().as_raw().len();
        let upload = PendingTextureUpload {
            image: Some(image),
            recycle_tx,
            sampler,
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

    pub fn pop_next(
        &mut self,
        budget: TextureUploadBudget,
        drained_uploads: usize,
        drained_bytes: usize,
    ) -> Option<(TextureHandle, PendingTextureUpload)> {
        while let Some(handle) = self.order.pop_front() {
            let Some(upload) = self.entries.remove(&handle) else {
                continue;
            };
            let next_bytes = drained_bytes.saturating_add(upload.bytes);
            let fits_budget =
                drained_uploads < budget.max_uploads && next_bytes <= budget.max_bytes;
            let allow_first =
                drained_uploads == 0 && budget.max_uploads > 0 && budget.max_bytes > 0;
            if fits_budget || allow_first {
                self.queued_bytes = self.queued_bytes.saturating_sub(upload.bytes);
                return Some((handle, upload));
            }
            self.entries.insert(handle, upload);
            self.order.push_front(handle);
            return None;
        }
        None
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    fn queued_bytes(&self) -> usize {
        self.queued_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::sync_channel;

    fn blank_rgba(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_pixel(width, height, image::Rgba([0, 0, 0, 0]))
    }

    #[test]
    fn replaces_existing_key_without_dup_order() {
        let mut queue = TextureUploadQueue::default();
        queue.push(7, Arc::new(blank_rgba(1, 1)), SamplerDesc::default());
        queue.push(7, Arc::new(blank_rgba(2, 2)), SamplerDesc::default());
        queue.push(11, Arc::new(blank_rgba(1, 1)), SamplerDesc::default());

        assert_eq!(queue.len(), 2);
        assert!(queue.contains(7));
        assert!(queue.contains(11));
        assert!(!queue.contains(99));
        assert_eq!(queue.queued_bytes(), (2 * 2 * 4 + 4) as usize);

        let budget = TextureUploadBudget {
            max_uploads: 4,
            max_bytes: 64,
        };
        let (first_handle, first) = queue.pop_next(budget, 0, 0).unwrap();
        assert_eq!(first_handle, 7);
        assert_eq!(first.bytes, (2 * 2 * 4) as usize);

        let (second_handle, second) = queue.pop_next(budget, 1, first.bytes).unwrap();
        assert_eq!(second_handle, 11);
        assert_eq!(second.bytes, 4);
        assert!(!queue.contains(7));
        assert!(!queue.contains(11));
        assert!(
            queue
                .pop_next(budget, 2, first.bytes + second.bytes)
                .is_none()
        );
    }

    #[test]
    fn replaced_recyclable_upload_returns_its_pixel_buffer() {
        let mut queue = TextureUploadQueue::default();
        let (recycle_tx, recycle_rx) = sync_channel(1);
        let image = RgbaImage::from_raw(1, 1, vec![1, 2, 3, 4]).unwrap();
        queue.push_recyclable(7, image, SamplerDesc::default(), recycle_tx);

        queue.push(7, Arc::new(blank_rgba(1, 1)), SamplerDesc::default());

        assert_eq!(recycle_rx.try_recv().unwrap(), vec![1, 2, 3, 4]);
    }

    #[test]
    fn allows_one_oversize_upload_then_stops_at_budget() {
        let mut queue = TextureUploadQueue::default();
        queue.push(7, Arc::new(blank_rgba(3, 1)), SamplerDesc::default());
        queue.push(11, Arc::new(blank_rgba(1, 1)), SamplerDesc::default());

        let budget = TextureUploadBudget {
            max_uploads: 1,
            max_bytes: 8,
        };
        let (first_handle, first) = queue.pop_next(budget, 0, 0).unwrap();
        assert_eq!(first_handle, 7);
        assert_eq!(first.bytes, 12);
        assert!(queue.pop_next(budget, 1, first.bytes).is_none());
        assert!(queue.contains(11));
    }

    #[test]
    fn video_upload_retains_numeric_handle() {
        let mut queue = TextureUploadQueue::default();
        queue.push(42, Arc::new(blank_rgba(1, 1)), SamplerDesc::default());
        let (queued, _) = queue
            .pop_next(
                TextureUploadBudget {
                    max_uploads: 1,
                    max_bytes: 4,
                },
                0,
                0,
            )
            .unwrap();
        assert_eq!(queued, 42);
    }
}
