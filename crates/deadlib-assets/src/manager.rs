use crate::{
    AssetError, FontStore, TextureDecodeJob, TextureStore, black_texture_image,
    decode::{TextureDecodeResult, decode_texture_jobs_with},
    decode_texture_image, fallback_texture_image, generated_texture, register_texture_dims,
    upload::{TextureUploadBudget, TextureUploadImage},
    white_texture_image,
};
use deadlib_present::font::{Font, FontMap};
use deadlib_render::{Backend, Texture as RendererTexture};
use deadlib_render_core::{SamplerDesc, TextureHandle, TextureHandleMap, Yuv420Upload};
use image::RgbaImage;
use log::{debug, warn};

/// Render-thread asset owner. Workers only supply decoded images; renderer calls
/// and retirement stay on the owner thread. Identities and upload queues retain
/// their storage across frames, and replacement delegates destruction to the backend.
pub struct AssetManager {
    texture_store: TextureStore<RendererTexture>,
    font_store: FontStore,
}

impl AssetManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            texture_store: TextureStore::new(),
            font_store: FontStore::new(),
        }
    }

    /// Borrows this store's texture identities and dimensions for presentation.
    pub fn texture_context(&self) -> &TextureStore<RendererTexture> {
        &self.texture_store
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
    pub const fn textures(&self) -> &TextureHandleMap<RendererTexture> {
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

    pub fn take_textures(&mut self) -> TextureHandleMap<RendererTexture> {
        self.texture_store.take_textures()
    }

    pub fn reserve_texture_handle(&mut self, key: String) -> TextureHandle {
        self.texture_store.reserve_texture_handle(key)
    }

    pub fn insert_texture(
        &mut self,
        key: String,
        texture: RendererTexture,
        width: u32,
        height: u32,
    ) -> Option<RendererTexture> {
        self.texture_store
            .insert_texture(key, texture, width, height)
    }

    pub fn remove_texture(&mut self, key: &str) -> Option<(TextureHandle, RendererTexture)> {
        self.texture_store.remove_texture(key)
    }

    pub fn set_texture_for_key(
        &mut self,
        backend: &mut Backend,
        key: String,
        texture: RendererTexture,
        width: u32,
        height: u32,
    ) -> TextureHandle {
        let (handle, old) = self
            .texture_store
            .set_texture_for_key(key, texture, width, height);
        if let Some(old) = old {
            backend.retire_texture(old);
        }
        handle
    }

    pub fn update_texture_for_key(
        &mut self,
        backend: &mut Backend,
        key: &str,
        rgba: &RgbaImage,
    ) -> Result<(), AssetError> {
        if let Some(texture) =
            self.texture_store
                .uploaded_texture_mut(key, rgba.width(), rgba.height())
            && !Backend::texture_is_yuv420(texture)
        {
            backend.update_texture(texture, rgba)?;
            return Ok(());
        }

        self.update_texture_for_key_with_sampler(backend, key, rgba, SamplerDesc::default())
    }

    pub fn update_texture_for_key_with_sampler(
        &mut self,
        backend: &mut Backend,
        key: &str,
        rgba: &RgbaImage,
        sampler: SamplerDesc,
    ) -> Result<(), AssetError> {
        let texture = backend.create_texture(rgba, sampler)?;
        self.set_texture_for_key(
            backend,
            key.to_string(),
            texture,
            rgba.width(),
            rgba.height(),
        );
        register_texture_dims(key, rgba.width(), rgba.height());
        Ok(())
    }

    pub fn queue_texture_upload(&mut self, key: String, image: RgbaImage) {
        self.texture_store.queue_texture_upload(key, image);
    }

    pub fn queue_video_frame_upload(
        &mut self,
        handle: TextureHandle,
        frame: deadlib_video::VideoFrame,
    ) {
        let (image, recycle_tx) = frame.into_upload_parts();
        self.texture_store
            .queue_recyclable_yuv420_upload(handle, image, recycle_tx);
    }

    pub fn queue_pending_generated_textures(&mut self) {
        self.texture_store.queue_pending_generated_textures();
    }

    /// Drains the render-thread queue with count/byte budgets. Pending uploads own
    /// their recycle sender, so every exit (including failure) returns frame storage.
    pub fn drain_texture_uploads(&mut self, backend: &mut Backend, budget: TextureUploadBudget) {
        let mut drained_uploads = 0usize;
        let mut drained_bytes = 0usize;
        while let Some((handle, upload)) =
            self.texture_store
                .pop_next_upload(budget, drained_uploads, drained_bytes)
        {
            drained_uploads = drained_uploads.saturating_add(1);
            drained_bytes = drained_bytes.saturating_add(upload.bytes);
            let image = upload.image();
            if let Some(texture) =
                self.texture_store
                    .apply_upload_update(handle, image.width(), image.height())
                && Backend::texture_is_yuv420(texture)
                    == matches!(image, TextureUploadImage::Yuv420(_))
            {
                match update_upload_texture(backend, texture, image) {
                    Ok(()) => continue,
                    Err(error) => warn!(
                        "Failed to update queued GPU texture for key '{}': {error}",
                        self.texture_store.texture_key(handle)
                    ),
                }
            }
            match create_upload_texture(backend, image, upload.sampler) {
                Ok(texture) => {
                    if let Some(old) = self.texture_store.set_texture_for_handle(
                        handle,
                        texture,
                        image.width(),
                        image.height(),
                    ) {
                        backend.retire_texture(old);
                    }
                }
                Err(error) => warn!(
                    "Failed to create queued GPU texture for key '{}': {error}",
                    self.texture_store.texture_key(handle)
                ),
            }
        }
    }

    /// Startup boundary: reserve identities, install primitives, and stream decoded
    /// images from bounded workers to the renderer. A decode failure uses a checker.
    pub fn load_textures(
        &mut self,
        backend: &mut Backend,
        jobs: Vec<TextureDecodeJob>,
    ) -> Result<(), AssetError> {
        self.texture_store
            .reserve_initial_textures(jobs.len().saturating_add(2));
        let mut load = |key: String, image: &RgbaImage, sampler: SamplerDesc| {
            let texture = backend.create_texture(image, sampler)?;
            register_texture_dims(&key, image.width(), image.height());
            debug!("Loaded texture: {key}");
            if let Some(old) = self.insert_texture(key, texture, image.width(), image.height()) {
                backend.retire_texture(old);
            }
            Ok::<_, AssetError>(())
        };
        for built_in in [white_texture_image(), black_texture_image()] {
            load(
                built_in.key.to_owned(),
                &built_in.image,
                SamplerDesc::default(),
            )?;
        }
        let fallback = fallback_texture_image();
        decode_texture_jobs_with(
            jobs,
            |TextureDecodeResult {
                 key,
                 sampler,
                 image,
             }| {
                let image = match &image {
                    Ok(image) => image,
                    Err(error) => {
                        warn!("Failed to load texture for key '{key}': {error}. Using fallback.");
                        &fallback
                    }
                };
                load(key, image, sampler)
            },
        )
    }

    /// Decode and upload one resolved asset at a loading boundary.
    pub fn load_texture(
        &mut self,
        backend: &mut Backend,
        job: &TextureDecodeJob,
    ) -> Result<(), AssetError> {
        let image = decode_texture_image(&job.path, &job.hints)?;
        self.update_texture_for_key_with_sampler(backend, &job.key, &image, job.sampler)
    }

    pub fn load_generated_texture(
        &mut self,
        backend: &mut Backend,
        key: &str,
        sampler: Option<SamplerDesc>,
    ) -> Result<bool, AssetError> {
        let Some(generated) = generated_texture(key) else {
            return Ok(false);
        };
        let image = generated.image;
        let texture = backend.create_texture(&image, sampler.unwrap_or(generated.sampler))?;
        // The generated registry already owns the source dimensions.
        self.set_texture_for_key(
            backend,
            key.to_owned(),
            texture,
            image.width(),
            image.height(),
        );
        Ok(true)
    }
}

fn create_upload_texture(
    backend: &mut Backend,
    image: TextureUploadImage<'_>,
    sampler: SamplerDesc,
) -> Result<RendererTexture, AssetError> {
    match image {
        TextureUploadImage::Rgba(image) => Ok(backend.create_texture(image, sampler)?),
        TextureUploadImage::Yuv420(image) => {
            let (y, u, v) = image.planes();
            let conversion = image.conversion();
            Ok(backend.create_yuv420_texture(
                Yuv420Upload {
                    width: image.width(),
                    height: image.height(),
                    y,
                    u,
                    v,
                    levels: conversion.levels,
                    coeffs: conversion.coeffs,
                },
                sampler,
            )?)
        }
    }
}

fn update_upload_texture(
    backend: &mut Backend,
    texture: &mut RendererTexture,
    image: TextureUploadImage<'_>,
) -> Result<(), AssetError> {
    match image {
        TextureUploadImage::Rgba(image) => Ok(backend.update_texture(texture, image)?),
        TextureUploadImage::Yuv420(image) => {
            let (y, u, v) = image.planes();
            let conversion = image.conversion();
            Ok(backend.update_yuv420_texture(
                texture,
                Yuv420Upload {
                    width: image.width(),
                    height: image.height(),
                    y,
                    u,
                    v,
                    levels: conversion.levels,
                    coeffs: conversion.coeffs,
                },
            )?)
        }
    }
}

impl Default for AssetManager {
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
        let mut assets = AssetManager::new();
        assets.queue_texture_upload("queued".to_string(), blank_rgba(2, 2));

        assert!(assets.has_texture_key("queued"));
        assert!(assets.has_pending_texture_upload("queued"));

        assert!(assets.remove_texture("queued").is_none());
        assert!(!assets.has_texture_key("queued"));
        assert!(!assets.has_pending_texture_upload("queued"));
    }

    // One hidden software window exercises the concrete owner without GPU timing
    // or a mock renderer. Keep the sequence together to share the OS event loop.
    #[cfg(target_os = "windows")]
    #[test]
    #[allow(deprecated)] // EventLoop::create_window avoids a running UI loop in this test.
    fn renderer_uploads_preserve_budgets_recycling_and_formats() {
        use deadlib_render_core::{BackendType, PresentModePolicy};
        use deadlib_video::Yuv420Image;
        use std::sync::{Arc, mpsc::sync_channel};
        use winit::platform::windows::EventLoopBuilderExtWindows;

        let event_loop = winit::event_loop::EventLoop::builder()
            .with_any_thread(true)
            .build()
            .expect("create test event loop");
        let window = event_loop
            .create_window(
                winit::window::Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(64, 64)),
            )
            .expect("create hidden test window");
        let mut backend = deadlib_render::create_backend(
            BackendType::Software,
            Arc::new(window),
            false,
            PresentModePolicy::Immediate,
            false,
            false,
        )
        .expect("software backend");
        let mut assets = AssetManager::new();
        assets
            .load_textures(
                &mut backend,
                vec![TextureDecodeJob {
                    key: "missing.png".into(),
                    path: std::path::PathBuf::from("__missing_initial_texture__.png"),
                    sampler: SamplerDesc::default(),
                    hints: crate::TextureHints::default(),
                }],
            )
            .expect("fallback and builtins load");
        assert!(assets.has_uploaded_texture_key(crate::WHITE_TEXTURE_KEY));
        assert!(assets.has_uploaded_texture_key(crate::BLACK_TEXTURE_KEY));
        assert_eq!(
            software_image(&assets, "missing.png"),
            &fallback_texture_image()
        );

        let budget = TextureUploadBudget {
            max_uploads: 1,
            max_bytes: 16,
        };
        let handle = assets.reserve_texture_handle("video".into());
        let (tx, rx) = sync_channel(4);
        let frame = RgbaImage::from_pixel(2, 2, image::Rgba([1, 2, 3, 255]));
        let raw_ptr = frame.as_ptr();
        assets
            .texture_store
            .queue_recyclable_texture_upload(handle, frame, tx.clone());
        assets.queue_texture_upload("later".into(), RgbaImage::new(1, 1));
        assets.drain_texture_uploads(
            &mut backend,
            TextureUploadBudget {
                max_uploads: 0,
                ..budget
            },
        );
        assert!(!assets.has_uploaded_texture_key("video"));
        assert!(rx.try_recv().is_err());
        assets.drain_texture_uploads(&mut backend, budget);
        let recycled_rgba = rx.try_recv().expect("recycle uploaded RGBA");
        assert_eq!(recycled_rgba.as_ptr(), raw_ptr);
        assert!(assets.has_pending_texture_upload("later"));
        let first_pixels = software_image(&assets, "video").as_ptr();
        assets.queue_texture_upload(
            "video".into(),
            RgbaImage::from_pixel(2, 2, image::Rgba([9, 8, 7, 255])),
        );
        assets.drain_texture_uploads(&mut backend, budget); // Earlier queued key owns this budget.
        assert!(assets.has_pending_texture_upload("video"));
        assets.drain_texture_uploads(&mut backend, budget);
        assert_eq!(software_image(&assets, "video").as_ptr(), first_pixels);
        assert_eq!(
            software_image(&assets, "video").get_pixel(0, 0).0,
            [9, 8, 7, 255]
        );

        let yuv = Yuv420Image::from_raw(2, 2, vec![16, 16, 16, 16, 128, 128]).expect("YUV frame");
        let yuv_ptr = yuv.planes().0.as_ptr();
        assets
            .texture_store
            .queue_recyclable_yuv420_upload(handle, yuv, tx.clone());
        assets.drain_texture_uploads(&mut backend, budget);
        assert!(Backend::texture_is_yuv420(
            assets.textures().get(&handle).expect("YUV texture")
        ));
        let recycled_yuv = rx.try_recv().expect("recycle uploaded YUV");
        assert_eq!(recycled_yuv.as_ptr(), yuv_ptr);
        assert_eq!(
            software_image(&assets, "video").get_pixel(0, 0).0,
            [0, 0, 0, 255]
        );
        assets.queue_texture_upload("video".into(), RgbaImage::new(2, 2));
        assets.drain_texture_uploads(&mut backend, budget);
        assert!(!Backend::texture_is_yuv420(
            assets.textures().get(&handle).expect("RGBA texture")
        ));

        assets.texture_store.queue_recyclable_texture_upload(
            handle,
            RgbaImage::new(4, 4),
            tx.clone(),
        );
        assets.queue_texture_upload("after-large".into(), RgbaImage::new(1, 1));
        assets.drain_texture_uploads(
            &mut backend,
            TextureUploadBudget {
                max_uploads: 2,
                ..budget
            },
        );
        assert_eq!(software_image(&assets, "video").dimensions(), (4, 4));
        assert_eq!(rx.try_recv().expect("recycle resized frame").len(), 64);
        assert!(assets.has_pending_texture_upload("after-large")); // First oversize upload is permitted.
        assert_eq!(assets.reserve_texture_handle("video".into()), handle);

        assets
            .texture_store
            .queue_recyclable_texture_upload(handle, RgbaImage::new(2, 2), tx);
        let (_, removed) = assets.remove_texture("video").expect("remove live texture");
        backend.retire_texture(removed);
        assert_eq!(rx.try_recv().expect("recycle cancelled upload").len(), 16);
        assert!(!assets.has_pending_texture_upload_handle(handle));
        backend.dispose_textures(&mut assets.take_textures());
        backend.cleanup();
    }

    #[cfg(target_os = "windows")]
    fn software_image<'a>(assets: &'a AssetManager, key: &str) -> &'a RgbaImage {
        let handle = assets
            .texture_store
            .bind_texture(key)
            .expect("bound texture")
            .handle;
        let RendererTexture::Software(texture) =
            assets.textures().get(&handle).expect("uploaded texture")
        else {
            panic!("software renderer expected");
        };
        &texture.image
    }
}
