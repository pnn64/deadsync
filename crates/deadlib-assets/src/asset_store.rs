use crate::{
    FontStore, TextureDecodeJob, TextureKeyLoad, TextureStore, prepare_initial_texture_images,
    prepare_texture_key_load, register_texture_dims,
    upload::{PendingTextureUpload, TextureUploadBudget},
};
use deadlib_present::font::Font;
use deadlib_render::{SamplerDesc, TextureHandle, TextureHandleMap};
use image::RgbaImage;
use std::{collections::HashMap, path::PathBuf};

pub enum TextureUploadAction<'a, T> {
    Update {
        texture: &'a mut T,
        image: &'a RgbaImage,
    },
    Create {
        image: &'a RgbaImage,
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
    Loaded { retired: Option<(TextureHandle, T)> },
}

pub struct InitialTextureLoad<T> {
    pub key: String,
    pub built_in: bool,
    pub retired: Option<(TextureHandle, T)>,
}

pub struct AssetStore<T> {
    texture_store: TextureStore<T>,
    font_store: FontStore,
}

impl<T> AssetStore<T> {
    pub fn new() -> Self {
        Self {
            texture_store: TextureStore::new(),
            font_store: FontStore::new(),
        }
    }

    pub fn register_font(&mut self, name: &'static str, font: Font) {
        self.font_store.register_font(name, font);
    }

    pub const fn fonts(&self) -> &HashMap<&'static str, Font> {
        self.font_store.fonts()
    }

    #[inline(always)]
    pub fn has_font(&self, name: &str) -> bool {
        self.font_store.has_font(name)
    }

    pub fn with_fonts<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&HashMap<&'static str, Font>) -> R,
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
    pub fn textures(&self) -> &TextureHandleMap<T> {
        self.texture_store.textures()
    }

    #[inline(always)]
    pub fn has_texture_key(&self, key: &str) -> bool {
        self.texture_store.has_texture_key(key)
    }

    #[inline(always)]
    pub fn has_uploaded_texture_key(&self, key: &str) -> bool {
        self.texture_store.has_uploaded_texture_key(key)
    }

    #[inline(always)]
    pub fn has_pending_texture_upload(&self, key: &str) -> bool {
        self.texture_store.has_pending_texture_upload(key)
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

    pub fn queue_pending_generated_textures(&mut self) {
        self.texture_store.queue_pending_generated_textures();
    }

    pub fn pop_next_upload(
        &mut self,
        budget: TextureUploadBudget,
        drained_uploads: usize,
        drained_bytes: usize,
    ) -> Option<(String, PendingTextureUpload)> {
        self.texture_store
            .pop_next_upload(budget, drained_uploads, drained_bytes)
    }

    pub fn drain_texture_uploads_with<E>(
        &mut self,
        budget: TextureUploadBudget,
        mut apply: impl for<'a> FnMut(TextureUploadAction<'a, T>) -> Result<Option<T>, E>,
    ) -> (Vec<(TextureHandle, T)>, Vec<TextureUploadDrainError<E>>) {
        let mut retired = Vec::new();
        let mut errors = Vec::new();
        let mut drained_uploads = 0usize;
        let mut drained_bytes = 0usize;
        while let Some((key, upload)) = self.pop_next_upload(budget, drained_uploads, drained_bytes)
        {
            drained_uploads = drained_uploads.saturating_add(1);
            drained_bytes = drained_bytes.saturating_add(upload.bytes);

            let mut updated = false;
            if let Some(texture) =
                self.uploaded_texture_mut(&key, upload.image.width(), upload.image.height())
            {
                match apply(TextureUploadAction::Update {
                    texture,
                    image: upload.image.as_ref(),
                }) {
                    Ok(_) => updated = true,
                    Err(error) => errors.push(TextureUploadDrainError::Update {
                        key: key.clone(),
                        error,
                    }),
                }
            }
            if updated {
                continue;
            }

            match apply(TextureUploadAction::Create {
                image: upload.image.as_ref(),
                sampler: upload.sampler,
            }) {
                Ok(Some(texture)) => {
                    let (handle, old) = self.set_texture_for_key(
                        key,
                        texture,
                        upload.image.width(),
                        upload.image.height(),
                    );
                    if let Some(old) = old {
                        retired.push((handle, old));
                    }
                }
                Ok(None) => {}
                Err(error) => errors.push(TextureUploadDrainError::Create { key, error }),
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
        let mut loaded = Vec::new();
        for prepared in prepare_initial_texture_images(jobs, needs_repeat_sampler) {
            let texture = create(prepared.image.as_ref(), prepared.sampler)?;
            register_texture_dims(
                &prepared.key,
                prepared.image.width(),
                prepared.image.height(),
            );
            let old = self.insert_texture(
                prepared.key.clone(),
                texture,
                prepared.image.width(),
                prepared.image.height(),
            );
            let handle = self
                .texture_store
                .texture_handle(&prepared.key)
                .expect("inserted texture must have a registered handle");
            loaded.push(InitialTextureLoad {
                key: prepared.key,
                built_in: prepared.built_in,
                retired: old.map(|texture| (handle, texture)),
            });
        }
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
                    let (handle, old) = self.set_texture_for_key(
                        key.clone(),
                        texture,
                        image.width(),
                        image.height(),
                    );
                    if register_dims {
                        register_texture_dims(&key, image.width(), image.height());
                    }
                    TextureKeyStoreLoad::Loaded {
                        retired: old.map(|texture| (handle, texture)),
                    }
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
}
