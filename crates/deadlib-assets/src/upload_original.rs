// Frozen upload insertion methods from f2dd892e7.
#[derive(Default)]
struct OriginalQueue {
    order: VecDeque<TextureHandle>,
    entries: FxHashMap<TextureHandle, PendingTextureUpload>,
    queued_bytes: usize,
}
impl OriginalQueue {
    pub fn push(&mut self, handle: TextureHandle, image: Arc<RgbaImage>, sampler: SamplerDesc) {
        self.push_inner(handle, UploadImage::Shared(image), sampler, None);
    }

    pub fn push_prepared(
        &mut self,
        handle: TextureHandle,
        image: Arc<RgbaImage>,
        sampler: SamplerDesc,
        source_size: [u32; 2],
    ) {
        self.push(handle, image, sampler);
        self.entries
            .get_mut(&handle)
            .expect("queued upload")
            .source_size = Some(source_size);
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
}
