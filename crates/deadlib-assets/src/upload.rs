use deadlib_render::SamplerDesc;
use image::RgbaImage;
use rustc_hash::FxHashMap;
use std::{
    borrow::Borrow,
    collections::{VecDeque, hash_map::Entry},
    hash::{Hash, Hasher},
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

/// Texture identity retained by the upload queue.
///
/// Ordinary one-shot uploads keep their owned `String`, while live video
/// frames clone a song/screen-lifetime `Arc<str>`. Equality and hashing are
/// deliberately based only on the string contents so both forms coalesce.
#[derive(Clone, Debug)]
pub enum TextureUploadKey {
    Owned(String),
    Shared(Arc<str>),
}

impl TextureUploadKey {
    #[inline(always)]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Owned(key) => key,
            Self::Shared(key) => key,
        }
    }

    pub fn into_string(self) -> String {
        match self {
            Self::Owned(key) => key,
            Self::Shared(key) => key.to_string(),
        }
    }
}

impl AsRef<str> for TextureUploadKey {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for TextureUploadKey {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl PartialEq for TextureUploadKey {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for TextureUploadKey {}

impl Hash for TextureUploadKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

#[derive(Default)]
pub struct TextureUploadQueue {
    order: VecDeque<TextureUploadKey>,
    entries: FxHashMap<TextureUploadKey, PendingTextureUpload>,
    queued_bytes: usize,
}

impl TextureUploadQueue {
    pub fn contains(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    pub fn push(&mut self, key: String, image: Arc<RgbaImage>, sampler: SamplerDesc) {
        self.push_inner(
            TextureUploadKey::Owned(key),
            UploadImage::Shared(image),
            sampler,
            None,
        );
    }

    pub fn push_shared(&mut self, key: Arc<str>, image: Arc<RgbaImage>, sampler: SamplerDesc) {
        self.push_inner(
            TextureUploadKey::Shared(key),
            UploadImage::Shared(image),
            sampler,
            None,
        );
    }

    pub fn push_recyclable_shared(
        &mut self,
        key: Arc<str>,
        image: RgbaImage,
        sampler: SamplerDesc,
        recycle_tx: SyncSender<Vec<u8>>,
    ) {
        self.push_inner(
            TextureUploadKey::Shared(key),
            UploadImage::Recyclable(image),
            sampler,
            Some(recycle_tx),
        );
    }

    fn push_inner(
        &mut self,
        key: TextureUploadKey,
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
    ) -> Option<(TextureUploadKey, PendingTextureUpload)> {
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
        assert_eq!(first_key.as_str(), "shared");
        assert_eq!(first.bytes, (2 * 2 * 4) as usize);

        let (second_key, second) = queue.pop_next(budget, 1, first.bytes).unwrap();
        assert_eq!(second_key.as_str(), "other");
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
        queue.push_recyclable_shared(
            Arc::from("video"),
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
        assert_eq!(first_key.as_str(), "big");
        assert_eq!(first.bytes, 12);
        assert!(queue.pop_next(budget, 1, first.bytes).is_none());
        assert!(queue.contains("small"));
    }

    #[test]
    fn shared_video_upload_key_preserves_arc_identity() {
        let key: Arc<str> = Arc::from("video/shared");
        let mut queue = TextureUploadQueue::default();
        queue.push_shared(
            Arc::clone(&key),
            Arc::new(blank_rgba(1, 1)),
            SamplerDesc::default(),
        );
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
        let TextureUploadKey::Shared(queued) = queued else {
            panic!("shared video key changed ownership form");
        };
        assert!(Arc::ptr_eq(&key, &queued));
    }
}
