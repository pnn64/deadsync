use deadlib_render::{
    BackendType, DrawStats, PresentModePolicy, RenderList, SamplerDesc, TextureHandle,
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
pub struct Backend(BackendImpl);

impl Backend {
    pub fn draw(
        &mut self,
        render_list: &RenderList,
        textures: &TextureHandleMap<Texture>,
        apply_present_back_pressure: bool,
    ) -> Result<DrawStats, Box<dyn Error>> {
        match &mut self.0 {
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
        }
    }

    pub fn request_screenshot(&mut self) {
        match &mut self.0 {
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
        match &mut self.0 {
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
        if let BackendImpl::Software(state) = &mut self.0 {
            software::set_thread_hint(state, threads);
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        match &mut self.0 {
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
        match &mut self.0 {
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
        match &mut self.0 {
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
        match (&mut self.0, texture) {
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
        match &mut self.0 {
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
        match &mut self.0 {
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
        match &mut self.0 {
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
    Ok(Backend(backend_impl))
}

impl Backend {
    pub fn set_present_config(
        &mut self,
        vsync_enabled: bool,
        present_mode_policy: PresentModePolicy,
    ) {
        match &mut self.0 {
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
