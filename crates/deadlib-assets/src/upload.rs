use deadlib_render::SamplerDesc;
use image::RgbaImage;
use rustc_hash::FxHashMap;
use std::{
    collections::{VecDeque, hash_map::Entry},
    sync::{Arc, mpsc::SyncSender},
};

#[cfg(any(test, feature = "bench-support"))]
use std::collections::HashMap;

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
    order: VecDeque<String>,
    entries: FxHashMap<String, PendingTextureUpload>,
    queued_bytes: usize,
}

impl TextureUploadQueue {
    pub fn contains(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    pub fn push(&mut self, key: String, image: Arc<RgbaImage>, sampler: SamplerDesc) {
        self.push_inner(key, UploadImage::Shared(image), sampler, None);
    }

    pub fn push_recyclable(
        &mut self,
        key: String,
        image: RgbaImage,
        sampler: SamplerDesc,
        recycle_tx: SyncSender<Vec<u8>>,
    ) {
        self.push_inner(
            key,
            UploadImage::Recyclable(image),
            sampler,
            Some(recycle_tx),
        );
    }

    fn push_inner(
        &mut self,
        key: String,
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
        match self.entries.entry(key) {
            Entry::Occupied(mut entry) => {
                let old = entry.insert(upload);
                self.queued_bytes = self.queued_bytes.saturating_sub(old.bytes);
            }
            Entry::Vacant(entry) => {
                self.order.push_back(entry.key().clone());
                entry.insert(upload);
            }
        }
        self.queued_bytes = self.queued_bytes.saturating_add(bytes);
    }

    pub fn remove(&mut self, key: &str) {
        if let Some(old) = self.entries.remove(key) {
            self.queued_bytes = self.queued_bytes.saturating_sub(old.bytes);
        }
    }

    pub fn pop_next(
        &mut self,
        budget: TextureUploadBudget,
        drained_uploads: usize,
        drained_bytes: usize,
    ) -> Option<(String, PendingTextureUpload)> {
        while let Some(key) = self.order.pop_front() {
            let Some((entry_key, upload)) = self.entries.remove_entry(&key) else {
                continue;
            };
            let next_bytes = drained_bytes.saturating_add(upload.bytes);
            let fits_budget =
                drained_uploads < budget.max_uploads && next_bytes <= budget.max_bytes;
            let allow_first =
                drained_uploads == 0 && budget.max_uploads > 0 && budget.max_bytes > 0;
            if fits_budget || allow_first {
                self.queued_bytes = self.queued_bytes.saturating_sub(upload.bytes);
                return Some((entry_key, upload));
            }
            self.entries.insert(entry_key, upload);
            self.order.push_front(key);
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

#[cfg(any(test, feature = "bench-support"))]
#[derive(Default)]
struct LegacyTextureUploadQueue {
    order: VecDeque<String>,
    entries: HashMap<String, PendingTextureUpload>,
    queued_bytes: usize,
}

#[cfg(any(test, feature = "bench-support"))]
impl LegacyTextureUploadQueue {
    fn push(&mut self, key: String, image: Arc<RgbaImage>, sampler: SamplerDesc) {
        let bytes = image.as_raw().len();
        if let Some(old) = self.entries.insert(
            key.clone(),
            PendingTextureUpload {
                image: Some(UploadImage::Shared(image)),
                recycle_tx: None,
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

#[cfg(any(test, feature = "bench-support"))]
fn upload_key_checksum(key: &str) -> u64 {
    key.bytes().fold(0_u64, |checksum, byte| {
        checksum.rotate_left(5) ^ u64::from(byte)
    })
}

#[cfg(any(test, feature = "bench-support"))]
trait UploadQueueBench {
    fn bench_push(&mut self, key: String, image: Arc<RgbaImage>);
    fn bench_pop(
        &mut self,
        budget: TextureUploadBudget,
        drained_uploads: usize,
        drained_bytes: usize,
    ) -> Option<(String, usize)>;
}

#[cfg(any(test, feature = "bench-support"))]
impl UploadQueueBench for TextureUploadQueue {
    fn bench_push(&mut self, key: String, image: Arc<RgbaImage>) {
        self.push(key, image, SamplerDesc::default());
    }

    fn bench_pop(
        &mut self,
        budget: TextureUploadBudget,
        drained_uploads: usize,
        drained_bytes: usize,
    ) -> Option<(String, usize)> {
        self.pop_next(budget, drained_uploads, drained_bytes)
            .map(|(key, upload)| (key, upload.bytes))
    }
}

#[cfg(any(test, feature = "bench-support"))]
impl UploadQueueBench for LegacyTextureUploadQueue {
    fn bench_push(&mut self, key: String, image: Arc<RgbaImage>) {
        self.push(key, image, SamplerDesc::default());
    }

    fn bench_pop(
        &mut self,
        budget: TextureUploadBudget,
        drained_uploads: usize,
        drained_bytes: usize,
    ) -> Option<(String, usize)> {
        self.pop_next(budget, drained_uploads, drained_bytes)
            .map(|(key, upload)| (key, upload.bytes))
    }
}

#[cfg(any(test, feature = "bench-support"))]
fn run_upload_queue_workload(
    queue: &mut impl UploadQueueBench,
    keys: &[&str],
    replacements: usize,
    image: &Arc<RgbaImage>,
) -> u64 {
    for &key in keys {
        queue.bench_push(key.to_owned(), Arc::clone(image));
    }
    for _ in 0..replacements {
        for &key in keys {
            queue.bench_push(key.to_owned(), Arc::clone(image));
        }
    }

    let budget = TextureUploadBudget {
        max_uploads: 1,
        max_bytes: image.as_raw().len(),
    };
    let mut checksum = 0_u64;
    for _ in keys {
        assert!(
            queue.bench_pop(budget, 1, 0).is_none(),
            "exhausted upload budget must defer the next item"
        );
        let (key, bytes) = queue
            .bench_pop(budget, 0, 0)
            .expect("queued upload must remain after deferral");
        checksum = checksum.rotate_left(7) ^ upload_key_checksum(&key) ^ bytes as u64;
    }
    checksum
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
pub fn texture_upload_queue_workload_for_bench(
    keys: &[&str],
    replacements: usize,
    image: &Arc<RgbaImage>,
) -> u64 {
    let mut queue = TextureUploadQueue::default();
    run_upload_queue_workload(&mut queue, keys, replacements, image)
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
pub fn texture_upload_queue_workload_legacy_for_bench(
    keys: &[&str],
    replacements: usize,
    image: &Arc<RgbaImage>,
) -> u64 {
    let mut queue = LegacyTextureUploadQueue::default();
    run_upload_queue_workload(&mut queue, keys, replacements, image)
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

        assert_eq!(queue.len(), 2);
        assert!(queue.contains("shared"));
        assert!(queue.contains("other"));
        assert!(!queue.contains("missing"));
        assert_eq!(queue.queued_bytes(), (2 * 2 * 4 + 4) as usize);

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
        assert!(!queue.contains("shared"));
        assert!(!queue.contains("other"));
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
        queue.push_recyclable(
            "video".to_string(),
            image,
            SamplerDesc::default(),
            recycle_tx,
        );

        queue.push(
            "video".to_string(),
            Arc::new(blank_rgba(1, 1)),
            SamplerDesc::default(),
        );

        assert_eq!(recycle_rx.try_recv().unwrap(), vec![1, 2, 3, 4]);
    }

    #[test]
    fn allows_one_oversize_upload_then_stops_at_budget() {
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
        assert!(queue.contains("small"));
    }

    #[test]
    fn optimized_upload_queue_matches_legacy_replacement_and_deferral() {
        let image = Arc::new(blank_rgba(4, 4));
        let keys = ["video/banner", "video/background", "generated/note"];
        assert_eq!(
            texture_upload_queue_workload_for_bench(&keys, 4, &image),
            texture_upload_queue_workload_legacy_for_bench(&keys, 4, &image)
        );
    }
}
