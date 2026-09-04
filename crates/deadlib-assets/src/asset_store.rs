use crate::{
    FontStore, TextureDecodeJob, TextureDecodeResult, TextureKeyLoad, TextureStore,
    black_texture_image,
    decode::decode_texture_jobs_with,
    fallback_texture_image, initial_texture_sampler, prepare_texture_key_load,
    register_texture_dims,
    upload::{PendingTextureUpload, TextureUploadBudget, TextureUploadImage},
    white_texture_image,
};
use deadlib_present::font::{Font, FontMap};
use deadlib_render_core::{SamplerDesc, TextureHandle, TextureHandleMap};
use deadlib_video::Yuv420Image;
use image::RgbaImage;
use log::warn;
use std::{path::PathBuf, sync::mpsc::SyncSender};

pub enum TextureUploadAction<'a, T> {
    Update {
        texture: &'a mut T,
        image: TextureUploadImage<'a>,
        sampler: SamplerDesc,
    },
    Create {
        image: TextureUploadImage<'a>,
        sampler: SamplerDesc,
    },
}

pub enum TextureUploadDrainError<E> {
    Update { key: String, error: E },
    Create { key: String, error: E },
}

pub enum TextureKeyStoreLoad<E, T> {
    Skip,
    Missing { key: String },
    DecodeFailed { key: String, message: String },
    CreateFailed { key: String, error: E },
    Loaded { retired: Option<T> },
}

pub struct InitialTextureLoad<T> {
    pub key: String,
    pub built_in: bool,
    pub retired: Option<T>,
}

pub struct AssetStore<T> {
    texture_store: TextureStore<T>,
    font_store: FontStore,
}

impl<T> AssetStore<T> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            texture_store: TextureStore::new(),
            font_store: FontStore::new(),
        }
    }

    pub fn register_font(&mut self, name: &'static str, font: Font) {
        self.font_store.register_font(name, font);
    }

    pub fn register_fonts(&mut self, fonts: impl IntoIterator<Item = (&'static str, Font)>) {
        self.font_store.register_fonts(fonts);
    }

    #[must_use]
    pub const fn fonts(&self) -> &FontMap {
        self.font_store.fonts()
    }

    #[inline(always)]
    #[must_use]
    pub fn has_font(&self, name: &str) -> bool {
        self.font_store.has_font(name)
    }

    pub fn with_fonts<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&FontMap) -> R,
    {
        self.font_store.with_fonts(f)
    }

    pub fn with_font<F, R>(&self, name: &str, f: F) -> Option<R>
    where
        F: FnOnce(&Font) -> R,
    {
        self.font_store.with_font(name, f)
    }

    #[inline(always)]
    #[must_use]
    pub const fn textures(&self) -> &TextureHandleMap<T> {
        self.texture_store.textures()
    }

    #[inline(always)]
    #[must_use]
    pub fn has_texture_key(&self, key: &str) -> bool {
        self.texture_store.has_texture_key(key)
    }

    #[inline(always)]
    #[must_use]
    pub fn has_uploaded_texture_key(&self, key: &str) -> bool {
        self.texture_store.has_uploaded_texture_key(key)
    }

    #[inline(always)]
    #[must_use]
    pub fn has_pending_texture_upload(&self, key: &str) -> bool {
        self.texture_store.has_pending_texture_upload(key)
    }

    #[inline(always)]
    #[must_use]
    pub fn has_pending_texture_upload_handle(&self, handle: TextureHandle) -> bool {
        self.texture_store.has_pending_texture_upload_handle(handle)
    }

    pub fn take_textures(&mut self) -> TextureHandleMap<T> {
        self.texture_store.take_textures()
    }

    pub fn reserve_texture_handle(&mut self, key: String) -> TextureHandle {
        self.texture_store.reserve_texture_handle(key)
    }

    pub fn insert_texture(
        &mut self,
        key: String,
        texture: T,
        width: u32,
        height: u32,
    ) -> Option<T> {
        self.texture_store
            .insert_texture(key, texture, width, height)
    }

    pub fn remove_texture(&mut self, key: &str) -> Option<(TextureHandle, T)> {
        self.texture_store.remove_texture(key)
    }

    pub fn set_texture_for_key(
        &mut self,
        key: String,
        texture: T,
        width: u32,
        height: u32,
    ) -> (TextureHandle, Option<T>) {
        self.texture_store
            .set_texture_for_key(key, texture, width, height)
    }

    pub fn uploaded_texture_mut(&mut self, key: &str, width: u32, height: u32) -> Option<&mut T> {
        self.texture_store.uploaded_texture_mut(key, width, height)
    }

    pub fn queue_texture_upload(&mut self, key: String, image: RgbaImage) {
        self.texture_store.queue_texture_upload(key, image);
    }

    pub fn queue_recyclable_texture_upload(
        &mut self,
        handle: TextureHandle,
        image: RgbaImage,
        recycle_tx: SyncSender<Vec<u8>>,
    ) {
        self.texture_store
            .queue_recyclable_texture_upload(handle, image, recycle_tx);
    }

    pub fn queue_recyclable_yuv420_upload(
        &mut self,
        handle: TextureHandle,
        image: Yuv420Image,
        recycle_tx: SyncSender<Vec<u8>>,
    ) {
        self.texture_store
            .queue_recyclable_yuv420_upload(handle, image, recycle_tx);
    }

    pub fn queue_pending_generated_textures(&mut self) {
        self.texture_store.queue_pending_generated_textures();
    }

    pub fn pop_next_upload(
        &mut self,
        budget: TextureUploadBudget,
        drained_uploads: usize,
        drained_bytes: usize,
    ) -> Option<(TextureHandle, PendingTextureUpload)> {
        self.texture_store
            .pop_next_upload(budget, drained_uploads, drained_bytes)
    }

    pub fn drain_texture_uploads_with<E>(
        &mut self,
        budget: TextureUploadBudget,
        mut apply: impl for<'a> FnMut(TextureUploadAction<'a, T>) -> Result<Option<T>, E>,
    ) -> (Vec<T>, Vec<TextureUploadDrainError<E>>) {
        let mut retired = Vec::new();
        let mut errors = Vec::new();
        let mut drained_uploads = 0usize;
        let mut drained_bytes = 0usize;
        while let Some((handle, upload)) =
            self.pop_next_upload(budget, drained_uploads, drained_bytes)
        {
            drained_uploads = drained_uploads.saturating_add(1);
            drained_bytes = drained_bytes.saturating_add(upload.bytes);

            let image = upload.image();
            let mut updated = false;
            if let Some(texture) =
                self.texture_store
                    .apply_upload_update(handle, image.width(), image.height())
            {
                match apply(TextureUploadAction::Update {
                    texture,
                    image,
                    sampler: upload.sampler,
                }) {
                    Ok(replacement) => {
                        updated = true;
                        if let Some(replacement) = replacement {
                            let old = self.texture_store.set_texture_for_handle(
                                handle,
                                replacement,
                                image.width(),
                                image.height(),
                            );
                            if let Some(old) = old {
                                retired.push(old);
                            }
                        }
                    }
                    Err(error) => errors.push(TextureUploadDrainError::Update {
                        key: self.texture_store.texture_key(handle).to_owned(),
                        error,
                    }),
                }
            }
            if updated {
                continue;
            }

            match apply(TextureUploadAction::Create {
                image,
                sampler: upload.sampler,
            }) {
                Ok(Some(texture)) => {
                    let old = self.texture_store.set_texture_for_handle(
                        handle,
                        texture,
                        image.width(),
                        image.height(),
                    );
                    if let Some(old) = old {
                        retired.push(old);
                    }
                }
                Ok(None) => {}
                Err(error) => errors.push(TextureUploadDrainError::Create {
                    key: self.texture_store.texture_key(handle).to_owned(),
                    error,
                }),
            }
        }
        (retired, errors)
    }

    pub fn load_initial_textures_with<E>(
        &mut self,
        jobs: Vec<TextureDecodeJob>,
        needs_repeat_sampler: impl Fn(&str) -> bool,
        mut create: impl FnMut(&RgbaImage, SamplerDesc) -> Result<T, E>,
    ) -> Result<Vec<InitialTextureLoad<T>>, E> {
        let load_count = jobs.len().saturating_add(2);
        self.texture_store.reserve_initial_textures(load_count);
        let mut loaded = Vec::with_capacity(load_count);
        let mut load = |key: String, image: &RgbaImage, sampler: SamplerDesc, built_in: bool| {
            let texture = create(image, sampler)?;
            register_texture_dims(&key, image.width(), image.height());
            let old = self.insert_texture(key.clone(), texture, image.width(), image.height());
            loaded.push(InitialTextureLoad {
                key,
                built_in,
                retired: old,
            });
            Ok(())
        };

        for built_in in [white_texture_image(), black_texture_image()] {
            load(
                built_in.key.to_string(),
                &built_in.image,
                SamplerDesc::default(),
                true,
            )?;
        }

        let fallback = fallback_texture_image();
        decode_texture_jobs_with(jobs, |result| {
            let (key, image) = match result {
                TextureDecodeResult::Decoded { key, image } => (key, image),
                TextureDecodeResult::Failed { key, message } => {
                    warn!("Failed to load texture for key '{key}': {message}. Using fallback.");
                    let sampler = initial_texture_sampler(&key, needs_repeat_sampler(&key));
                    return load(key, &fallback, sampler, false);
                }
            };
            let sampler = initial_texture_sampler(&key, needs_repeat_sampler(&key));
            load(key, &image, sampler, false)
        })?;

        Ok(loaded)
    }

    pub fn load_texture_key_with<E>(
        &mut self,
        texture_key: &str,
        sampler_override: Option<SamplerDesc>,
        force_reload: bool,
        canonical_texture_key: impl Fn(&str) -> String,
        resolve_asset_path: impl Fn(&str) -> PathBuf,
        needs_repeat_sampler: impl Fn(&str) -> bool,
        mut create: impl FnMut(&RgbaImage, SamplerDesc) -> Result<T, E>,
    ) -> TextureKeyStoreLoad<E, T> {
        match prepare_texture_key_load(
            texture_key,
            sampler_override,
            force_reload,
            |key| self.has_texture_key(key),
            canonical_texture_key,
            resolve_asset_path,
            needs_repeat_sampler,
        ) {
            TextureKeyLoad::Skip => TextureKeyStoreLoad::Skip,
            TextureKeyLoad::Missing { key } => TextureKeyStoreLoad::Missing { key },
            TextureKeyLoad::DecodeFailed { key, message } => {
                TextureKeyStoreLoad::DecodeFailed { key, message }
            }
            TextureKeyLoad::Image {
                key,
                image,
                sampler,
                register_dims,
            } => match create(image.as_ref(), sampler) {
                Ok(texture) => {
                    let (_handle, old) = self.set_texture_for_key(
                        key.clone(),
                        texture,
                        image.width(),
                        image.height(),
                    );
                    if register_dims {
                        register_texture_dims(&key, image.width(), image.height());
                    }
                    TextureKeyStoreLoad::Loaded { retired: old }
                }
                Err(error) => TextureKeyStoreLoad::CreateFailed { key, error },
            },
        }
    }
}

impl<T> Default for AssetStore<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::sync_channel;

    #[test]
    fn asset_store_tracks_pending_texture_uploads() {
        let mut store = AssetStore::<()>::new();
        store.queue_texture_upload("queued".to_string(), RgbaImage::new(2, 2));

        assert!(store.has_texture_key("queued"));
        assert!(store.has_pending_texture_upload("queued"));
        assert!(store.remove_texture("queued").is_none());
        assert!(!store.has_texture_key("queued"));
        assert!(!store.has_pending_texture_upload("queued"));
    }

    #[test]
    fn drain_texture_uploads_with_creates_missing_texture() {
        let mut store = AssetStore::<u32>::new();
        store.queue_texture_upload("queued".to_string(), RgbaImage::new(2, 2));

        let (retired, errors): (_, Vec<TextureUploadDrainError<()>>) = store
            .drain_texture_uploads_with(
                TextureUploadBudget {
                    max_uploads: 1,
                    max_bytes: 64,
                },
                |action| match action {
                    TextureUploadAction::Update { .. } => Ok(None),
                    TextureUploadAction::Create { .. } => Ok(Some(7)),
                },
            );

        assert!(retired.is_empty());
        assert!(errors.is_empty());
        assert!(store.has_uploaded_texture_key("queued"));
    }

    #[test]
    fn drained_recyclable_upload_returns_its_pixel_buffer() {
        let mut store = AssetStore::<u32>::new();
        let (recycle_tx, recycle_rx) = sync_channel(1);
        let image = RgbaImage::from_raw(2, 1, vec![1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
        let handle = store.reserve_texture_handle("video".to_string());
        store.queue_recyclable_texture_upload(handle, image, recycle_tx);

        let (_, errors): (_, Vec<TextureUploadDrainError<()>>) = store.drain_texture_uploads_with(
            TextureUploadBudget {
                max_uploads: 1,
                max_bytes: 8,
            },
            |action| match action {
                TextureUploadAction::Update { .. } => Ok(None),
                TextureUploadAction::Create { .. } => Ok(Some(7)),
            },
        );

        assert!(errors.is_empty());
        assert_eq!(recycle_rx.try_recv().unwrap(), vec![1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn handle_keyed_video_upload_updates_then_replaces_on_resize() {
        let mut store = AssetStore::<u32>::new();
        store.insert_texture("video".to_string(), 7, 2, 1);
        let handle = store.reserve_texture_handle("video".to_string());
        let (recycle_tx, recycle_rx) = sync_channel(2);

        store.queue_recyclable_texture_upload(handle, RgbaImage::new(2, 1), recycle_tx.clone());
        let mut updates = 0;
        let (retired, errors): (_, Vec<TextureUploadDrainError<()>>) = store
            .drain_texture_uploads_with(
                TextureUploadBudget {
                    max_uploads: 1,
                    max_bytes: 64,
                },
                |action| match action {
                    TextureUploadAction::Update { texture, .. } => {
                        assert_eq!(*texture, 7);
                        updates += 1;
                        Ok(None)
                    }
                    TextureUploadAction::Create { .. } => panic!("same-size frame recreated"),
                },
            );
        assert_eq!(updates, 1);
        assert!(retired.is_empty());
        assert!(errors.is_empty());

        store.queue_recyclable_texture_upload(handle, RgbaImage::new(4, 2), recycle_tx);
        let (retired, errors): (_, Vec<TextureUploadDrainError<()>>) = store
            .drain_texture_uploads_with(
                TextureUploadBudget {
                    max_uploads: 1,
                    max_bytes: 64,
                },
                |action| match action {
                    TextureUploadAction::Update { .. } => panic!("resized frame updated in place"),
                    TextureUploadAction::Create { .. } => Ok(Some(11)),
                },
            );
        assert_eq!(retired, vec![7]);
        assert!(errors.is_empty());
        assert_eq!(recycle_rx.try_iter().count(), 2);
    }

    #[test]
    fn same_size_upload_can_replace_a_texture_kind() {
        let mut store = AssetStore::<u32>::new();
        store.insert_texture("video".to_string(), 7, 2, 2);
        let handle = store.reserve_texture_handle("video".to_string());
        let (recycle_tx, recycle_rx) = sync_channel(1);
        let image = Yuv420Image::from_raw(2, 2, vec![16, 16, 16, 16, 128, 128]).unwrap();
        store.queue_recyclable_yuv420_upload(handle, image, recycle_tx);

        let (retired, errors): (_, Vec<TextureUploadDrainError<()>>) = store
            .drain_texture_uploads_with(
                TextureUploadBudget {
                    max_uploads: 1,
                    max_bytes: 6,
                },
                |action| match action {
                    TextureUploadAction::Update { texture, image, .. } => {
                        assert_eq!(*texture, 7);
                        assert!(matches!(image, TextureUploadImage::Yuv420(_)));
                        Ok(Some(11))
                    }
                    TextureUploadAction::Create { .. } => panic!("same-size upload is an update"),
                },
            );

        assert_eq!(retired, vec![7]);
        assert!(errors.is_empty());
        assert_eq!(recycle_rx.try_recv().unwrap().len(), 6);
    }

    #[test]
    fn load_texture_key_with_skips_cached_key() {
        let mut store = AssetStore::<u32>::new();
        store.insert_texture("cached.png".to_string(), 1, 2, 2);

        let result = store.load_texture_key_with(
            "cached.png",
            None,
            false,
            str::to_string,
            |path| PathBuf::from(path),
            |_| false,
            |_, _| Ok::<u32, ()>(2),
        );

        assert!(matches!(result, TextureKeyStoreLoad::Skip));
    }

    #[test]
    fn load_initial_textures_with_loads_builtins() {
        let mut store = AssetStore::<u32>::new();

        let loaded = store
            .load_initial_textures_with(
                Vec::new(),
                |_| false,
                |image, _| Ok::<u32, ()>(image.width() * image.height()),
            )
            .unwrap();

        assert_eq!(loaded.len(), 2);
        assert!(loaded.iter().all(|load| load.built_in));
        assert!(loaded.iter().all(|load| load.retired.is_none()));
        assert!(store.has_uploaded_texture_key(crate::WHITE_TEXTURE_KEY));
        assert!(store.has_uploaded_texture_key(crate::BLACK_TEXTURE_KEY));
    }

    #[test]
    fn load_initial_textures_with_preserves_failed_decode_fallback() {
        let mut store = AssetStore::<u32>::new();
        let mut uploads = Vec::new();

        let loaded = store
            .load_initial_textures_with(
                vec![TextureDecodeJob {
                    key: "grades/goldstar (stretch).png".to_string(),
                    path: PathBuf::from("__missing_initial_texture__.png"),
                }],
                |_| true,
                |image, sampler| {
                    uploads.push((image.width(), image.height(), sampler.wrap));
                    Ok::<u32, ()>(image.width() * image.height())
                },
            )
            .unwrap();

        assert_eq!(loaded.len(), 3);
        assert!(loaded[..2].iter().all(|load| load.built_in));
        assert!(!loaded[2].built_in);
        assert_eq!(loaded[2].key, "grades/goldstar (stretch).png");
        assert_eq!(uploads[2].0, 2);
        assert_eq!(uploads[2].1, 2);
        assert_eq!(uploads[2].2, deadlib_render_core::SamplerWrap::Repeat);
    }

    #[test]
    fn load_initial_textures_with_stops_workers_after_create_error() {
        let mut store = AssetStore::<u32>::new();
        let jobs = (0..32)
            .map(|index| TextureDecodeJob {
                key: format!("missing-{index}.png"),
                path: PathBuf::from(format!("__missing_initial_texture_{index}__.png")),
            })
            .collect();
        let mut creates = 0;

        let result = store.load_initial_textures_with(
            jobs,
            |_| false,
            |image, _| {
                creates += 1;
                (creates < 3)
                    .then_some(image.width())
                    .ok_or("create failed")
            },
        );

        assert!(matches!(result, Err("create failed")));
        assert_eq!(creates, 3);
    }
}
