use deadlib_render::SamplerDesc;
use image::RgbaImage;
use std::{collections::HashMap, collections::VecDeque, sync::Arc};

#[derive(Clone, Copy)]
pub struct TextureUploadBudget {
    pub max_uploads: usize,
    pub max_bytes: usize,
}

pub struct PendingTextureUpload {
    pub image: Arc<RgbaImage>,
    pub sampler: SamplerDesc,
    pub bytes: usize,
}

#[derive(Default)]
pub struct TextureUploadQueue {
    order: VecDeque<String>,
    entries: HashMap<String, PendingTextureUpload>,
    queued_bytes: usize,
}

impl TextureUploadQueue {
    pub fn contains(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    pub fn push(&mut self, key: String, image: Arc<RgbaImage>, sampler: SamplerDesc) {
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
        assert_eq!(queue.queued_bytes(), (2 * 2 * 4 + 1 * 1 * 4) as usize);

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
}
