mod backends;

#[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
use crate::engine::gfx::backends::vulkan;
use crate::engine::gfx::backends::{opengl, wgpu_core};
use deadsync_render::{
    DrawStats, PresentModePolicy, RenderList, SamplerDesc, TextureHandle, TextureHandleMap,
};
use deadsync_render_backend_software as software;
use glow::HasContext;
use image::RgbaImage;
use std::{error::Error, str::FromStr, sync::Arc};
use winit::window::Window;

// --- Public API Facade ---

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendType {
    #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
    Vulkan,
    #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
    VulkanWgpu,
    #[cfg(target_os = "macos")]
    Metal,
    OpenGL,
    OpenGLWgpu,
    Software,
    #[cfg(target_os = "windows")]
    DirectX,
}

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
            BackendImpl::Vulkan(state) => {
                vulkan::draw(state, render_list, textures, apply_present_back_pressure)
            }
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            BackendImpl::VulkanWgpu(state) => {
                wgpu_core::draw(state, render_list, textures, apply_present_back_pressure)
            }
            #[cfg(target_os = "macos")]
            BackendImpl::Metal(state) => {
                wgpu_core::draw(state, render_list, textures, apply_present_back_pressure)
            }
            BackendImpl::OpenGL(state) => {
                opengl::draw(state, render_list, textures, apply_present_back_pressure)
            }
            BackendImpl::OpenGLWgpu(state) => {
                wgpu_core::draw(state, render_list, textures, apply_present_back_pressure)
            }
            BackendImpl::Software(state) => software::draw(
                state,
                render_list,
                &SoftwareTextureLookup(textures),
                apply_present_back_pressure,
            ),
            #[cfg(target_os = "windows")]
            BackendImpl::DirectX(state) => {
                wgpu_core::draw(state, render_list, textures, apply_present_back_pressure)
            }
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
            BackendImpl::Software(state) => {
                crate::engine::space::set_current_window_px(width, height);
                software::resize(state, width, height);
            }
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
                let tex = opengl::create_texture(&state.gl, image, sampler)?;
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
                opengl::update_texture(&state.gl, texture, image)?;
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
                // SAFETY: Each texture handle came from this backend and runtime
                // retirement only drops the GL object name; the driver defers
                // actual destruction until it is no longer in use.
                unsafe {
                    for tex in old_textures.values() {
                        if let Texture::OpenGL(opengl::Texture(handle)) = tex {
                            state.gl.delete_texture(*handle);
                        }
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
                // SAFETY: `wait_for_idle()` above guarantees no in-flight GPU work
                // still references these texture handles, and each handle came from
                // this OpenGL backend.
                unsafe {
                    for tex in old_textures.values() {
                        if let Texture::OpenGL(opengl::Texture(handle)) = tex {
                            state.gl.delete_texture(*handle);
                        }
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
            BackendImpl::Vulkan(state) => {
                let _ = vulkan::flush_pending_uploads(state);
                if let Some(device) = &state.device {
                    // SAFETY: `device` is the live Vulkan logical device for this
                    // backend, and we only wait for idle before tearing down or
                    // reclaiming resources.
                    unsafe {
                        let _ = device.device_wait_idle();
                    }
                }
                vulkan::retire_submitted_uploads(state);
                vulkan::retire_all_textures(state);
            }
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            BackendImpl::VulkanWgpu(state) => {
                let _ = state.device.poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: None,
                });
            }
            #[cfg(target_os = "macos")]
            BackendImpl::Metal(state) => {
                let _ = state.device.poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: None,
                });
            }
            BackendImpl::OpenGL(_) => {
                // This is a no-op for OpenGL.
            }
            BackendImpl::OpenGLWgpu(state) => {
                let _ = state.device.poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: None,
                });
            }
            BackendImpl::Software(_) => {
                // CPU renderer is synchronous; nothing to wait for.
            }
            #[cfg(target_os = "windows")]
            BackendImpl::DirectX(state) => {
                let _ = state.device.poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: None,
                });
            }
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
        BackendType::Software => {
            let size = window.inner_size();
            crate::engine::space::set_current_window_px(size.width, size.height);
            BackendImpl::Software(software::init(window, vsync_enabled)?)
        }
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

// -- Boilerplate impls --
impl core::fmt::Display for BackendType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            Self::Vulkan => write!(f, "Vulkan"),
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            Self::VulkanWgpu => write!(f, "Vulkan (wgpu)"),
            #[cfg(target_os = "macos")]
            Self::Metal => write!(f, "Metal (wgpu)"),
            Self::OpenGL => write!(f, "OpenGL"),
            Self::OpenGLWgpu => write!(f, "OpenGL (wgpu)"),
            Self::Software => write!(f, "Software"),
            #[cfg(target_os = "windows")]
            Self::DirectX => write!(f, "DirectX"),
        }
    }
}
impl FromStr for BackendType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            "vulkan" => Ok(Self::Vulkan),
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            "vulkan-wgpu" | "vulkan_wgpu" | "wgpu-vulkan" | "vulkan (wgpu)" => Ok(Self::VulkanWgpu),
            #[cfg(target_os = "macos")]
            "metal" | "metal-wgpu" | "metal_wgpu" | "wgpu-metal" | "metal (wgpu)" => {
                Ok(Self::Metal)
            }
            "opengl" => Ok(Self::OpenGL),
            "opengl-wgpu" | "opengl_wgpu" | "wgpu-opengl" | "opengl (wgpu)" => Ok(Self::OpenGLWgpu),
            "software" | "cpu" => Ok(Self::Software),
            #[cfg(target_os = "windows")]
            "directx" | "dx12" | "directx (wgpu)" => Ok(Self::DirectX),
            _ => Err(format!("'{s}' is not a valid video renderer")),
        }
    }
}
