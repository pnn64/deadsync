pub(super) fn snapshot(queue: &TextureUploadQueue) -> State {
    let mut items = Vec::new();
    for (&key, upload) in &queue.entries {
        let (kind, width, height, raw, capacity, shared, strong, conversion) =
            match upload.image.as_ref() {
                Some(UploadImage::Shared(image)) => (
                    1,
                    image.width(),
                    image.height(),
                    image.as_raw().clone(),
                    Some(image.as_raw().capacity()),
                    Some(Arc::as_ptr(image) as usize),
                    Arc::strong_count(image),
                    None,
                ),
                Some(UploadImage::Owned(image)) => (
                    2,
                    image.width(),
                    image.height(),
                    image.as_raw().clone(),
                    Some(image.as_raw().capacity()),
                    None,
                    0,
                    None,
                ),
                Some(UploadImage::RecyclableYuv420(image)) => {
                    let conversion = image.conversion();
                    (
                        3,
                        image.width(),
                        image.height(),
                        image.as_raw().to_vec(),
                        None,
                        None,
                        0,
                        Some([
                            conversion.levels.map(f32::to_bits),
                            conversion.coeffs.map(f32::to_bits),
                        ]),
                    )
                }
                None => (0, 0, 0, Vec::new(), None, None, 0, None),
            };
        items.push(Item {
            key,
            kind,
            width,
            height,
            raw,
            capacity,
            shared,
            strong,
            conversion,
            recycle: upload.recycle_tx.is_some(),
            source_size: upload.source_size,
            sampler: upload.sampler,
            bytes: upload.bytes,
        });
    }
    items.sort_unstable_by_key(|item| item.key);
    State {
        order: queue.order.iter().copied().collect(),
        order_capacity: queue.order.capacity(),
        entries_capacity: queue.entries.capacity(),
        bytes: queue.queued_bytes(),
        count: queue.len(),
        items,
    }
}

pub(super) fn layout() -> [usize; 14] {
    use std::mem::{align_of, offset_of, size_of};
    [
        size_of::<TextureUploadQueue>(),
        align_of::<TextureUploadQueue>(),
        offset_of!(TextureUploadQueue, order),
        offset_of!(TextureUploadQueue, entries),
        offset_of!(TextureUploadQueue, queued_bytes),
        size_of::<PendingTextureUpload>(),
        align_of::<PendingTextureUpload>(),
        offset_of!(PendingTextureUpload, image),
        offset_of!(PendingTextureUpload, recycle_tx),
        offset_of!(PendingTextureUpload, sampler),
        offset_of!(PendingTextureUpload, source_size),
        offset_of!(PendingTextureUpload, bytes),
        size_of::<UploadImage>(),
        align_of::<UploadImage>(),
    ]
}

pub(super) fn set_bytes(queue: &mut TextureUploadQueue, bytes: usize) {
    queue.queued_bytes = bytes;
}
