use deadlib_render::{
    BackendType, DrawFrame, DrawStats, PresentModePolicy, RenderList, RetainedTMeshGeometry,
    SamplerDesc, TMeshCacheEpoch, TMeshPrewarmStats, TMeshRetireStats, TextureHandle,
    TextureHandleMap,
};
use deadlib_render_backend_gl as opengl;
use deadlib_render_backend_software as software;
#[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
use deadlib_render_backend_vulkan as vulkan;
use deadlib_render_backend_wgpu as wgpu_core;
use image::RgbaImage;
use std::{error::Error, sync::Arc};
use winit::window::Window;

mod window_size;
pub use window_size::{
    render_size_for_physical, render_size_for_window, request_window_size,
    with_requested_window_size,
};

// --- Public API Facade ---

// A handle to a backend-specific texture resource.
pub enum Texture {
    #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
    Vulkan(vulkan::Texture),
    #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
    VulkanWgpu(wgpu_core::Texture),
    #[cfg(target_os = "macos")]
    Metal(wgpu_core::Texture),
    OpenGL(opengl::Texture),
    OpenGLWgpu(wgpu_core::Texture),
    Software(software::Texture),
    #[cfg(target_os = "windows")]
    DirectX(wgpu_core::Texture),
}

struct SoftwareTextureLookup<'a>(&'a TextureHandleMap<Texture>);

struct OpenGlTextureLookup<'a>(&'a TextureHandleMap<Texture>);

#[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
struct VulkanTextureLookup<'a>(&'a TextureHandleMap<Texture>);

#[derive(Clone, Copy)]
enum WgpuTextureKind {
    #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
    Vulkan,
    #[cfg(target_os = "macos")]
    Metal,
    OpenGL,
    #[cfg(target_os = "windows")]
    DirectX,
}

struct WgpuTextureLookup<'a> {
    textures: &'a TextureHandleMap<Texture>,
    kind: WgpuTextureKind,
}

#[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
impl vulkan::TextureLookup for VulkanTextureLookup<'_> {
    fn vulkan_texture(&self, handle: TextureHandle) -> Option<&vulkan::Texture> {
        match self.0.get(&handle)? {
            Texture::Vulkan(texture) => Some(texture),
            _ => None,
        }
    }
}

impl opengl::TextureLookup for OpenGlTextureLookup<'_> {
    fn opengl_texture(&self, handle: TextureHandle) -> Option<&opengl::Texture> {
        match self.0.get(&handle)? {
            Texture::OpenGL(texture) => Some(texture),
            _ => None,
        }
    }
}

impl wgpu_core::TextureLookup for WgpuTextureLookup<'_> {
    fn wgpu_texture(&self, handle: TextureHandle) -> Option<&wgpu_core::Texture> {
        match (self.kind, self.textures.get(&handle)?) {
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            (WgpuTextureKind::Vulkan, Texture::VulkanWgpu(texture)) => Some(texture),
            #[cfg(target_os = "macos")]
            (WgpuTextureKind::Metal, Texture::Metal(texture)) => Some(texture),
            (WgpuTextureKind::OpenGL, Texture::OpenGLWgpu(texture)) => Some(texture),
            #[cfg(target_os = "windows")]
            (WgpuTextureKind::DirectX, Texture::DirectX(texture)) => Some(texture),
            _ => None,
        }
    }
}

impl software::TextureLookup for SoftwareTextureLookup<'_> {
    fn software_texture(&self, handle: TextureHandle) -> Option<&software::Texture> {
        match self.0.get(&handle)? {
            Texture::Software(texture) => Some(texture),
            _ => None,
        }
    }
}

// An internal enum to hold the state for the active rendering backend.
enum BackendImpl {
    #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
    Vulkan(vulkan::State),
    #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
    VulkanWgpu(wgpu_core::State),
    #[cfg(target_os = "macos")]
    Metal(wgpu_core::State),
    OpenGL(opengl::State),
    OpenGLWgpu(wgpu_core::State),
    Software(software::State),
    #[cfg(target_os = "windows")]
    DirectX(wgpu_core::State),
}

/// A public, opaque wrapper around the active rendering backend.
/// This hides platform-specific variants from the rest of the application.
pub struct Backend {
    inner: BackendImpl,
    tmesh_epoch: Option<TMeshCacheEpoch>,
}

/// Flat frame bound to the backend cache generation that successfully
/// prewarmed all of its retained textured-mesh geometry.
///
/// Construction is private to [`Backend::prepare_draw_frame`]. The owner and
/// epoch cannot be replaced independently, so direct submission cannot pair a
/// stale generation with an otherwise valid frame by mistake. The render
/// thread owns this value for its screen/song lifetime and drops it at the next
/// transition. Warm submission performs one epoch comparison and borrows the
/// already-flat slices; it does not allocate, traverse geometry, or mutate the
/// retained cache.
pub struct PreparedDrawFrame<F> {
    owner: F,
    epoch: TMeshCacheEpoch,
}

/// Result of prewarming one flat frame against the active cache generation.
pub struct DrawFramePrewarm<F> {
    pub prepared: Option<PreparedDrawFrame<F>>,
    pub stats: TMeshPrewarmStats,
}

impl<F> PreparedDrawFrame<F> {
    #[inline(always)]
    pub const fn owner(&self) -> &F {
        &self.owner
    }

    #[inline(always)]
    pub const fn epoch(&self) -> TMeshCacheEpoch {
        self.epoch
    }

    #[inline]
    pub fn into_owner(self) -> F {
        self.owner
    }
}

impl<F: AsRef<DrawFrame>> PreparedDrawFrame<F> {
    #[inline(always)]
    pub fn frame(&self) -> &DrawFrame {
        self.owner.as_ref()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TMeshEpochError {
    Unavailable,
    Stale,
}

impl core::fmt::Display for TMeshEpochError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Unavailable => f.write_str("retained textured-mesh cache is unavailable"),
            Self::Stale => f.write_str("retained textured-mesh cache epoch is stale"),
        }
    }
}

impl Error for TMeshEpochError {}

#[inline(always)]
fn validate_tmesh_epoch(
    active: Option<TMeshCacheEpoch>,
    supplied: TMeshCacheEpoch,
) -> Result<(), TMeshEpochError> {
    match active {
        Some(active) if active == supplied => Ok(()),
        Some(_) => Err(TMeshEpochError::Stale),
        None => Err(TMeshEpochError::Unavailable),
    }
}

#[inline]
fn initial_tmesh_epoch(backend_type: BackendType) -> Option<TMeshCacheEpoch> {
    (!matches!(backend_type, BackendType::Software)).then(TMeshCacheEpoch::fresh)
}

#[inline]
fn stamp_prepared_frame<F>(
    owner: F,
    epoch: TMeshCacheEpoch,
    stats: TMeshPrewarmStats,
) -> DrawFramePrewarm<F> {
    let prepared = if stats.ready() {
        Some(PreparedDrawFrame { owner, epoch })
    } else {
        None
    };
    DrawFramePrewarm { prepared, stats }
}

fn advance_tmesh_epoch_state(
    active: &mut Option<TMeshCacheEpoch>,
    retire: impl FnOnce(&Option<TMeshCacheEpoch>) -> Result<TMeshRetireStats, Box<dyn Error>>,
) -> Result<(TMeshCacheEpoch, TMeshRetireStats), Box<dyn Error>> {
    active.take().ok_or(TMeshEpochError::Unavailable)?;
    let retired = retire(active)?;
    let epoch = TMeshCacheEpoch::fresh();
    *active = Some(epoch);
    Ok((epoch, retired))
}

fn retire_tmesh_cache(inner: &mut BackendImpl) -> Result<TMeshRetireStats, Box<dyn Error>> {
    match inner {
        #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
        BackendImpl::Vulkan(state) => vulkan::retire_textured_meshes(state)
            .map_err(|error| {
                std::io::Error::other(format!(
                    "failed to retire Vulkan textured-mesh cache: {error:?}"
                ))
            })
            .map_err(Into::into),
        #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
        BackendImpl::VulkanWgpu(state) => Ok(wgpu_core::retire_textured_meshes(state)),
        #[cfg(target_os = "macos")]
        BackendImpl::Metal(state) => Ok(wgpu_core::retire_textured_meshes(state)),
        BackendImpl::OpenGL(state) => Ok(opengl::retire_textured_meshes(state)),
        BackendImpl::OpenGLWgpu(state) => Ok(wgpu_core::retire_textured_meshes(state)),
        BackendImpl::Software(_) => Err(TMeshEpochError::Unavailable.into()),
        #[cfg(target_os = "windows")]
        BackendImpl::DirectX(state) => Ok(wgpu_core::retire_textured_meshes(state)),
    }
}

#[derive(Clone, Copy)]
enum SubmissionKind {
    RenderList,
    DirectFrame,
}

fn record_submission(mut stats: DrawStats, kind: SubmissionKind) -> DrawStats {
    match kind {
        SubmissionKind::RenderList => {
            stats.render_list_submissions = stats.render_list_submissions.saturating_add(1);
            stats.draw_prep_bypassed = false;
        }
        SubmissionKind::DirectFrame => {
            stats.direct_frame_submissions = stats.direct_frame_submissions.saturating_add(1);
            stats.draw_prep_bypassed = true;
        }
    }
    stats
}

impl Backend {
    /// Active retained-geometry cache generation for this hardware backend.
    ///
    /// Software rendering and a hardware backend whose retirement failed have
    /// no active epoch and reject retained prewarm/direct submission.
    #[inline(always)]
    pub const fn tmesh_epoch(&self) -> Option<TMeshCacheEpoch> {
        self.tmesh_epoch
    }

    /// Whether this backend can consume a prepared [`DrawFrame`] directly.
    #[inline(always)]
    pub fn supports_direct_frame(&self) -> bool {
        self.tmesh_epoch.is_some()
    }

    /// Makes immutable textured-mesh geometry resident before live rendering.
    ///
    /// The render thread exclusively owns each hardware backend's active 16 MiB
    /// cache generation. Call this at screen/song warmup with the backend's
    /// current epoch; stale or absent epochs fail before backend dispatch. The
    /// pass attempts every supplied entry, records hits/uploads/failures, and
    /// never prunes. A full cache reports capacity exhaustion separately from
    /// identity mismatch or upload failure. Entries are keyed by the complete
    /// logical-key/fingerprint identity, so safe revisions can coexist within
    /// one epoch. [`Backend::advance_tmesh_epoch`] synchronously retires every
    /// entry at a transition boundary and reports retired entry/byte totals.
    /// Cleanup is the final destruction path. A direct frame miss skips that
    /// run instead of uploading during gameplay. Prewarm work is bounded by
    /// this slice and the byte cap, with one lookup and at most one upload per
    /// entry. Live frames perform one cache lookup per retained run and no cache
    /// maintenance, upload, or destruction. Callers should require
    /// [`TMeshPrewarmStats::ready`] before selecting direct submission.
    pub fn prewarm_textured_meshes(
        &mut self,
        epoch: TMeshCacheEpoch,
        geometries: &[RetainedTMeshGeometry],
    ) -> Result<TMeshPrewarmStats, Box<dyn Error>> {
        validate_tmesh_epoch(self.tmesh_epoch, epoch)?;
        match &mut self.inner {
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            BackendImpl::Vulkan(state) => Ok(vulkan::prewarm_textured_meshes(state, geometries)),
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            BackendImpl::VulkanWgpu(state) => {
                Ok(wgpu_core::prewarm_textured_meshes(state, geometries))
            }
            #[cfg(target_os = "macos")]
            BackendImpl::Metal(state) => Ok(wgpu_core::prewarm_textured_meshes(state, geometries)),
            BackendImpl::OpenGL(state) => Ok(opengl::prewarm_textured_meshes(state, geometries)),
            BackendImpl::OpenGLWgpu(state) => {
                Ok(wgpu_core::prewarm_textured_meshes(state, geometries))
            }
            BackendImpl::Software(_) => Err(std::io::Error::other(
                "Textured-mesh prewarming is not supported by the software renderer",
            )
            .into()),
            #[cfg(target_os = "windows")]
            BackendImpl::DirectX(state) => {
                Ok(wgpu_core::prewarm_textured_meshes(state, geometries))
            }
        }
    }

    /// Prewarms and binds one flat frame to the active cache generation.
    ///
    /// The returned frame is `Some` only when every retained geometry is
    /// resident. Incomplete prewarm retains its detailed counters but drops the
    /// unprepared owner, preventing safe code from submitting it directly.
    pub fn prepare_draw_frame<F>(&mut self, owner: F) -> Result<DrawFramePrewarm<F>, Box<dyn Error>>
    where
        F: AsRef<DrawFrame>,
    {
        let epoch = self.tmesh_epoch.ok_or(TMeshEpochError::Unavailable)?;
        let stats = self.prewarm_textured_meshes(epoch, owner.as_ref().retained_tmeshes())?;
        Ok(stamp_prepared_frame(owner, epoch, stats))
    }

    /// Retires the complete active cache and publishes a fresh generation.
    ///
    /// The old token is invalidated before backend synchronization or resource
    /// destruction begins. Any failure therefore leaves the backend without an
    /// active epoch, preventing stale direct frames from being submitted.
    pub fn advance_tmesh_epoch(
        &mut self,
    ) -> Result<(TMeshCacheEpoch, TMeshRetireStats), Box<dyn Error>> {
        let inner = &mut self.inner;
        advance_tmesh_epoch_state(&mut self.tmesh_epoch, |_| retire_tmesh_cache(inner))
    }

    pub fn draw(
        &mut self,
        render_list: &RenderList,
        textures: &TextureHandleMap<Texture>,
        apply_present_back_pressure: bool,
    ) -> Result<DrawStats, Box<dyn Error>> {
        let stats = match &mut self.inner {
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            BackendImpl::Vulkan(state) => vulkan::draw(
                state,
                render_list,
                &VulkanTextureLookup(textures),
                apply_present_back_pressure,
            ),
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            BackendImpl::VulkanWgpu(state) => wgpu_core::draw(
                state,
                render_list,
                &WgpuTextureLookup {
                    textures,
                    kind: WgpuTextureKind::Vulkan,
                },
                apply_present_back_pressure,
            ),
            #[cfg(target_os = "macos")]
            BackendImpl::Metal(state) => wgpu_core::draw(
                state,
                render_list,
                &WgpuTextureLookup {
                    textures,
                    kind: WgpuTextureKind::Metal,
                },
                apply_present_back_pressure,
            ),
            BackendImpl::OpenGL(state) => opengl::draw(
                state,
                render_list,
                &OpenGlTextureLookup(textures),
                apply_present_back_pressure,
            ),
            BackendImpl::OpenGLWgpu(state) => wgpu_core::draw(
                state,
                render_list,
                &WgpuTextureLookup {
                    textures,
                    kind: WgpuTextureKind::OpenGL,
                },
                apply_present_back_pressure,
            ),
            BackendImpl::Software(state) => software::draw(
                state,
                render_list,
                &SoftwareTextureLookup(textures),
                apply_present_back_pressure,
            ),
            #[cfg(target_os = "windows")]
            BackendImpl::DirectX(state) => wgpu_core::draw(
                state,
                render_list,
                &WgpuTextureLookup {
                    textures,
                    kind: WgpuTextureKind::DirectX,
                },
                apply_present_back_pressure,
            ),
        }?;
        Ok(record_submission(stats, SubmissionKind::RenderList))
    }

    /// Submits an already prepared flat frame without rebuilding draw runs.
    ///
    /// Hardware backends consume the frame's slices directly. The software
    /// renderer still requires the legacy `RenderList` representation. Cached
    /// textured-mesh sources must already be resident in the selected hardware
    /// backend. The frame's privately bound cache epoch must match the active
    /// generation; a stale frame fails before backend dispatch. A missing entry
    /// within the active generation is skipped without a gameplay-time upload.
    pub fn draw_frame<F>(
        &mut self,
        prepared: &PreparedDrawFrame<F>,
        textures: &TextureHandleMap<Texture>,
        apply_present_back_pressure: bool,
    ) -> Result<DrawStats, Box<dyn Error>>
    where
        F: AsRef<DrawFrame>,
    {
        validate_tmesh_epoch(self.tmesh_epoch, prepared.epoch)?;
        let frame = prepared.frame();
        let stats = match &mut self.inner {
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            BackendImpl::Vulkan(state) => vulkan::draw_frame(
                state,
                frame,
                &VulkanTextureLookup(textures),
                apply_present_back_pressure,
            ),
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            BackendImpl::VulkanWgpu(state) => wgpu_core::draw_frame(
                state,
                frame,
                &WgpuTextureLookup {
                    textures,
                    kind: WgpuTextureKind::Vulkan,
                },
                apply_present_back_pressure,
            ),
            #[cfg(target_os = "macos")]
            BackendImpl::Metal(state) => wgpu_core::draw_frame(
                state,
                frame,
                &WgpuTextureLookup {
                    textures,
                    kind: WgpuTextureKind::Metal,
                },
                apply_present_back_pressure,
            ),
            BackendImpl::OpenGL(state) => opengl::draw_frame(
                state,
                frame,
                &OpenGlTextureLookup(textures),
                apply_present_back_pressure,
            ),
            BackendImpl::OpenGLWgpu(state) => wgpu_core::draw_frame(
                state,
                frame,
                &WgpuTextureLookup {
                    textures,
                    kind: WgpuTextureKind::OpenGL,
                },
                apply_present_back_pressure,
            ),
            BackendImpl::Software(_) => Err(std::io::Error::other(
                "Direct DrawFrame submission is not supported by the software renderer",
            )
            .into()),
            #[cfg(target_os = "windows")]
            BackendImpl::DirectX(state) => wgpu_core::draw_frame(
                state,
                frame,
                &WgpuTextureLookup {
                    textures,
                    kind: WgpuTextureKind::DirectX,
                },
                apply_present_back_pressure,
            ),
        }?;
        Ok(record_submission(stats, SubmissionKind::DirectFrame))
    }

    pub fn request_screenshot(&mut self) {
        match &mut self.inner {
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            BackendImpl::Vulkan(state) => vulkan::request_screenshot(state),
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            BackendImpl::VulkanWgpu(state) => wgpu_core::request_screenshot(state),
            #[cfg(target_os = "macos")]
            BackendImpl::Metal(state) => wgpu_core::request_screenshot(state),
            BackendImpl::OpenGL(state) => opengl::request_screenshot(state),
            BackendImpl::OpenGLWgpu(state) => wgpu_core::request_screenshot(state),
            BackendImpl::Software(state) => software::request_screenshot(state),
            #[cfg(target_os = "windows")]
            BackendImpl::DirectX(state) => wgpu_core::request_screenshot(state),
        }
    }

    pub fn capture_frame(&mut self) -> Result<RgbaImage, Box<dyn Error>> {
        match &mut self.inner {
            BackendImpl::OpenGL(state) => opengl::capture_frame(state),
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            BackendImpl::Vulkan(state) => vulkan::capture_frame(state),
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            BackendImpl::VulkanWgpu(state) => wgpu_core::capture_frame(state),
            #[cfg(target_os = "macos")]
            BackendImpl::Metal(state) => wgpu_core::capture_frame(state),
            BackendImpl::OpenGLWgpu(state) => wgpu_core::capture_frame(state),
            BackendImpl::Software(_) => Err(std::io::Error::other(
                "Screenshot capture is not implemented for Software renderer yet",
            )
            .into()),
            #[cfg(target_os = "windows")]
            BackendImpl::DirectX(state) => wgpu_core::capture_frame(state),
        }
    }

    pub fn configure_software_threads(&mut self, threads: Option<usize>) {
        if let BackendImpl::Software(state) = &mut self.inner {
            software::set_thread_hint(state, threads);
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        match &mut self.inner {
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            BackendImpl::Vulkan(state) => vulkan::resize(state, width, height),
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            BackendImpl::VulkanWgpu(state) => wgpu_core::resize(state, width, height),
            #[cfg(target_os = "macos")]
            BackendImpl::Metal(state) => wgpu_core::resize(state, width, height),
            BackendImpl::OpenGL(state) => opengl::resize(state, width, height),
            BackendImpl::OpenGLWgpu(state) => wgpu_core::resize(state, width, height),
            BackendImpl::Software(state) => software::resize(state, width, height),
            #[cfg(target_os = "windows")]
            BackendImpl::DirectX(state) => wgpu_core::resize(state, width, height),
        }
    }

    pub fn cleanup(&mut self) {
        self.tmesh_epoch = None;
        match &mut self.inner {
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            BackendImpl::Vulkan(state) => vulkan::cleanup(state),
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            BackendImpl::VulkanWgpu(state) => wgpu_core::cleanup(state),
            #[cfg(target_os = "macos")]
            BackendImpl::Metal(state) => wgpu_core::cleanup(state),
            BackendImpl::OpenGL(state) => opengl::cleanup(state),
            BackendImpl::OpenGLWgpu(state) => wgpu_core::cleanup(state),
            BackendImpl::Software(state) => software::cleanup(state),
            #[cfg(target_os = "windows")]
            BackendImpl::DirectX(state) => wgpu_core::cleanup(state),
        }
    }

    pub fn create_texture(
        &mut self,
        image: &RgbaImage,
        sampler: SamplerDesc,
    ) -> Result<Texture, Box<dyn Error>> {
        match &mut self.inner {
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            BackendImpl::Vulkan(state) => {
                let tex = vulkan::create_texture(state, image, sampler)?;
                Ok(Texture::Vulkan(tex))
            }
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            BackendImpl::VulkanWgpu(state) => {
                let tex = wgpu_core::create_texture(state, image, sampler)?;
                Ok(Texture::VulkanWgpu(tex))
            }
            #[cfg(target_os = "macos")]
            BackendImpl::Metal(state) => {
                let tex = wgpu_core::create_texture(state, image, sampler)?;
                Ok(Texture::Metal(tex))
            }
            BackendImpl::OpenGL(state) => {
                let tex = opengl::create_texture(state, image, sampler)?;
                Ok(Texture::OpenGL(tex))
            }
            BackendImpl::OpenGLWgpu(state) => {
                let tex = wgpu_core::create_texture(state, image, sampler)?;
                Ok(Texture::OpenGLWgpu(tex))
            }
            BackendImpl::Software(_state) => {
                let tex = software::create_texture(image, sampler)?;
                Ok(Texture::Software(tex))
            }
            #[cfg(target_os = "windows")]
            BackendImpl::DirectX(state) => {
                let tex = wgpu_core::create_texture(state, image, sampler)?;
                Ok(Texture::DirectX(tex))
            }
        }
    }

    pub fn update_texture(
        &mut self,
        texture: &mut Texture,
        image: &RgbaImage,
    ) -> Result<(), Box<dyn Error>> {
        match (&mut self.inner, texture) {
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            (BackendImpl::Vulkan(state), Texture::Vulkan(texture)) => {
                vulkan::update_texture(state, texture, image)
            }
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            (BackendImpl::VulkanWgpu(state), Texture::VulkanWgpu(texture)) => {
                wgpu_core::update_texture(state, texture, image)
            }
            #[cfg(target_os = "macos")]
            (BackendImpl::Metal(state), Texture::Metal(texture)) => {
                wgpu_core::update_texture(state, texture, image)
            }
            (BackendImpl::OpenGL(state), Texture::OpenGL(texture)) => {
                opengl::update_texture(state, texture, image)?;
                Ok(())
            }
            (BackendImpl::OpenGLWgpu(state), Texture::OpenGLWgpu(texture)) => {
                wgpu_core::update_texture(state, texture, image)
            }
            (BackendImpl::Software(_state), Texture::Software(texture)) => {
                software::update_texture(texture, image)
            }
            #[cfg(target_os = "windows")]
            (BackendImpl::DirectX(state), Texture::DirectX(texture)) => {
                wgpu_core::update_texture(state, texture, image)
            }
            _ => Err(std::io::Error::other("texture/backend mismatch").into()),
        }
    }

    pub fn retire_textures(&mut self, textures: &mut TextureHandleMap<Texture>) {
        let old_textures = std::mem::take(textures);
        match &mut self.inner {
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            BackendImpl::Vulkan(state) => {
                let retired = old_textures
                    .into_values()
                    .filter_map(|texture| match texture {
                        Texture::Vulkan(texture) => Some(texture),
                        _ => None,
                    })
                    .collect();
                vulkan::retire_textures(state, retired);
            }
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            BackendImpl::VulkanWgpu(_) => {
                drop(old_textures);
            }
            #[cfg(target_os = "macos")]
            BackendImpl::Metal(_) => {
                drop(old_textures);
            }
            BackendImpl::OpenGL(state) => {
                for tex in old_textures.values() {
                    if let Texture::OpenGL(texture) = tex {
                        opengl::delete_texture(state, texture);
                    }
                }
            }
            BackendImpl::OpenGLWgpu(_) => {
                drop(old_textures);
            }
            BackendImpl::Software(_) => {
                drop(old_textures);
            }
            #[cfg(target_os = "windows")]
            BackendImpl::DirectX(_) => {
                drop(old_textures);
            }
        }
    }

    pub fn dispose_textures(&mut self, textures: &mut TextureHandleMap<Texture>) {
        self.wait_for_idle();

        let old_textures = std::mem::take(textures);
        match &mut self.inner {
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            BackendImpl::Vulkan(_) => {
                // Vulkan textures are cleaned up by their Drop implementation.
                drop(old_textures);
            }
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            BackendImpl::VulkanWgpu(_) => {
                drop(old_textures);
            }
            #[cfg(target_os = "macos")]
            BackendImpl::Metal(_) => {
                drop(old_textures);
            }
            BackendImpl::OpenGL(state) => {
                for tex in old_textures.values() {
                    if let Texture::OpenGL(texture) = tex {
                        opengl::delete_texture(state, texture);
                    }
                }
            }
            BackendImpl::OpenGLWgpu(_) => {
                drop(old_textures);
            }
            BackendImpl::Software(_) => {
                drop(old_textures);
            }
            #[cfg(target_os = "windows")]
            BackendImpl::DirectX(_) => {
                drop(old_textures);
            }
        }
    }

    pub fn wait_for_idle(&mut self) {
        match &mut self.inner {
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            BackendImpl::Vulkan(state) => vulkan::wait_for_idle(state),
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            BackendImpl::VulkanWgpu(state) => wgpu_core::wait_for_idle(state),
            #[cfg(target_os = "macos")]
            BackendImpl::Metal(state) => wgpu_core::wait_for_idle(state),
            BackendImpl::OpenGL(_) => {
                // This is a no-op for OpenGL.
            }
            BackendImpl::OpenGLWgpu(state) => wgpu_core::wait_for_idle(state),
            BackendImpl::Software(_) => {
                // CPU renderer is synchronous; nothing to wait for.
            }
            #[cfg(target_os = "windows")]
            BackendImpl::DirectX(state) => wgpu_core::wait_for_idle(state),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_list_submission_records_draw_prep_path() {
        let stats = record_submission(
            DrawStats {
                vertices: 42,
                draw_prep_us: 7,
                ..DrawStats::default()
            },
            SubmissionKind::RenderList,
        );

        assert_eq!(stats.vertices, 42);
        assert_eq!(stats.render_list_submissions, 1);
        assert_eq!(stats.direct_frame_submissions, 0);
        assert!(!stats.draw_prep_bypassed);
        assert_eq!(stats.draw_prep_us, 7);
    }

    #[test]
    fn direct_submission_records_draw_prep_bypass() {
        let stats = record_submission(
            DrawStats {
                vertices: 42,
                ..DrawStats::default()
            },
            SubmissionKind::DirectFrame,
        );

        assert_eq!(stats.vertices, 42);
        assert_eq!(stats.render_list_submissions, 0);
        assert_eq!(stats.direct_frame_submissions, 1);
        assert!(stats.draw_prep_bypassed);
        assert_eq!(stats.draw_prep_us, 0);
    }

    #[test]
    fn tmesh_epoch_validation_rejects_stale_and_missing_generations() {
        let active = TMeshCacheEpoch::fresh();
        let stale = TMeshCacheEpoch::fresh();

        assert_eq!(validate_tmesh_epoch(Some(active), active), Ok(()));
        assert_eq!(
            validate_tmesh_epoch(Some(active), stale),
            Err(TMeshEpochError::Stale)
        );
        assert_eq!(
            validate_tmesh_epoch(None, active),
            Err(TMeshEpochError::Unavailable)
        );
    }

    #[test]
    fn backend_creation_epochs_are_unique_and_software_has_none() {
        let first = initial_tmesh_epoch(BackendType::OpenGL).expect("hardware epoch");
        let second = initial_tmesh_epoch(BackendType::OpenGL).expect("recreated hardware epoch");

        assert_ne!(first, second);
        assert_eq!(initial_tmesh_epoch(BackendType::Software), None);
    }

    #[test]
    fn prepared_frame_is_stamped_only_after_complete_prewarm() {
        let epoch = TMeshCacheEpoch::fresh();
        let mut ready = TMeshPrewarmStats::default();
        ready.requested = 1;
        ready.resident = 1;
        let result = stamp_prepared_frame(DrawFrame::default(), epoch, ready);
        let prepared = result
            .prepared
            .expect("complete prewarm must stamp the frame");

        assert_eq!(result.stats, ready);
        assert_eq!(prepared.epoch(), epoch);
        assert_eq!(prepared.frame().ops.len(), 0);

        let mut incomplete = ready;
        incomplete.requested = 2;
        incomplete.unavailable = 1;
        let result = stamp_prepared_frame(DrawFrame::default(), epoch, incomplete);
        assert!(result.prepared.is_none());
        assert_eq!(result.stats, incomplete);
    }

    #[test]
    fn epoch_advance_invalidates_before_retirement_then_publishes_fresh() {
        let old = TMeshCacheEpoch::fresh();
        let prepared =
            stamp_prepared_frame(DrawFrame::default(), old, TMeshPrewarmStats::default())
                .prepared
                .expect("an empty frame needs no geometry uploads");
        let retired = TMeshRetireStats {
            entries: 3,
            bytes: 192,
        };
        let mut active = Some(old);

        let (fresh, actual) = advance_tmesh_epoch_state(&mut active, |during_retirement| {
            assert_eq!(*during_retirement, None);
            Ok(retired)
        })
        .expect("retirement succeeds");

        assert_ne!(fresh, old);
        assert_eq!(active, Some(fresh));
        assert_eq!(actual, retired);
        assert_eq!(
            validate_tmesh_epoch(active, prepared.epoch()),
            Err(TMeshEpochError::Stale)
        );
    }

    #[test]
    fn epoch_advance_failure_leaves_cache_unavailable() {
        let mut active = Some(TMeshCacheEpoch::fresh());

        let result = advance_tmesh_epoch_state(&mut active, |during_retirement| {
            assert_eq!(*during_retirement, None);
            Err(std::io::Error::other("retirement failed").into())
        });

        assert!(result.is_err());
        assert_eq!(active, None);
    }

    #[test]
    fn missing_epoch_does_not_attempt_retirement() {
        let mut active = None;
        let mut called = false;

        let result = advance_tmesh_epoch_state(&mut active, |_| {
            called = true;
            Ok(TMeshRetireStats::default())
        });

        assert!(result.is_err());
        assert!(!called);
        assert_eq!(active, None);
    }
}

/// Creates and initializes a new graphics backend.
pub fn create_backend(
    backend_type: BackendType,
    window: Arc<Window>,
    vsync_enabled: bool,
    present_mode_policy: PresentModePolicy,
    gfx_debug_enabled: bool,
    high_dpi_enabled: bool,
) -> Result<Backend, Box<dyn Error>> {
    let backend_impl = match backend_type {
        #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
        BackendType::Vulkan => BackendImpl::Vulkan(vulkan::init(
            &window,
            vsync_enabled,
            present_mode_policy,
            gfx_debug_enabled,
        )?),
        #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
        BackendType::VulkanWgpu => BackendImpl::VulkanWgpu(wgpu_core::init_vulkan(
            window,
            vsync_enabled,
            present_mode_policy,
            gfx_debug_enabled,
        )?),
        #[cfg(target_os = "macos")]
        BackendType::Metal => BackendImpl::Metal(wgpu_core::init_metal(
            window,
            vsync_enabled,
            present_mode_policy,
            gfx_debug_enabled,
        )?),
        BackendType::OpenGL => BackendImpl::OpenGL(opengl::init(
            window,
            vsync_enabled,
            gfx_debug_enabled,
            high_dpi_enabled,
        )?),
        BackendType::OpenGLWgpu => BackendImpl::OpenGLWgpu(wgpu_core::init_opengl(
            window,
            vsync_enabled,
            present_mode_policy,
            gfx_debug_enabled,
        )?),
        BackendType::Software => BackendImpl::Software(software::init(window, vsync_enabled)?),
        #[cfg(target_os = "windows")]
        BackendType::DirectX => BackendImpl::DirectX(wgpu_core::init_dx12(
            window,
            vsync_enabled,
            present_mode_policy,
            gfx_debug_enabled,
        )?),
    };
    let tmesh_epoch = initial_tmesh_epoch(backend_type);
    Ok(Backend {
        inner: backend_impl,
        tmesh_epoch,
    })
}

impl Backend {
    pub fn set_present_config(
        &mut self,
        vsync_enabled: bool,
        present_mode_policy: PresentModePolicy,
    ) {
        match &mut self.inner {
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            BackendImpl::Vulkan(state) => {
                vulkan::set_present_config(state, vsync_enabled, present_mode_policy)
            }
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            BackendImpl::VulkanWgpu(state) => {
                wgpu_core::set_present_config(state, vsync_enabled, present_mode_policy)
            }
            #[cfg(target_os = "macos")]
            BackendImpl::Metal(state) => {
                wgpu_core::set_present_config(state, vsync_enabled, present_mode_policy)
            }
            BackendImpl::OpenGL(state) => opengl::set_vsync_enabled(state, vsync_enabled),
            BackendImpl::OpenGLWgpu(state) => {
                wgpu_core::set_present_config(state, vsync_enabled, present_mode_policy)
            }
            BackendImpl::Software(_) => {}
            #[cfg(target_os = "windows")]
            BackendImpl::DirectX(state) => {
                wgpu_core::set_present_config(state, vsync_enabled, present_mode_policy)
            }
        }
    }
}
