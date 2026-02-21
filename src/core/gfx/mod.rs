mod backends;

use crate::core::gfx::backends::{opengl, software, vulkan, wgpu_core};
use cgmath::Matrix4;
use glow::HasContext;
use image::RgbaImage;
use std::{borrow::Cow, collections::HashMap, error::Error, str::FromStr, sync::Arc};
use winit::window::Window;

// --- Public Data Contract ---
#[derive(Clone)]
pub struct RenderList<'a> {
    pub clear_color: [f32; 4],
    pub cameras: Vec<Matrix4<f32>>,
    pub objects: Vec<RenderObject<'a>>,
}
#[derive(Clone)]
pub struct RenderObject<'a> {
    pub object_type: ObjectType<'a>,
    pub transform: Matrix4<f32>,
    pub blend: BlendMode,
    pub z: i16,
    pub order: u32,
    pub camera: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct MeshVertex {
    pub pos: [f32; 2],
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TexturedMeshVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub tex_matrix_scale: [f32; 2],
    pub color: [f32; 4],
}

impl Default for TexturedMeshVertex {
    #[inline(always)]
    fn default() -> Self {
        Self {
            pos: [0.0, 0.0],
            uv: [0.0, 0.0],
            tex_matrix_scale: [1.0, 1.0],
            color: [0.0, 0.0, 0.0, 0.0],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshMode {
    Triangles,
}

#[derive(Clone)]
pub enum ObjectType<'a> {
    Sprite {
        texture_id: Cow<'a, str>,
        tint: [f32; 4],
        uv_scale: [f32; 2],
        uv_offset: [f32; 2],
        local_offset: [f32; 2],
        local_offset_rot_sin_cos: [f32; 2],
        edge_fade: [f32; 4],
    },
    Mesh {
        vertices: Cow<'a, [MeshVertex]>,
        mode: MeshMode,
    },
    #[allow(dead_code)]
    TexturedMesh {
        texture_id: Cow<'a, str>,
        vertices: Cow<'a, [TexturedMeshVertex]>,
        mode: MeshMode,
        uv_scale: [f32; 2],
        uv_offset: [f32; 2],
        uv_tex_shift: [f32; 2],
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SamplerFilter {
    Linear,
    Nearest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SamplerWrap {
    Clamp,
    Repeat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SamplerDesc {
    pub filter: SamplerFilter,
    pub wrap: SamplerWrap,
    pub mipmaps: bool,
}

impl Default for SamplerDesc {
    #[inline(always)]
    fn default() -> Self {
        Self {
            filter: SamplerFilter::Linear,
            wrap: SamplerWrap::Clamp,
            mipmaps: false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendMode {
    Alpha,
    Add,
    #[allow(dead_code)]
    Multiply,
    #[allow(dead_code)]
    Subtract,
}

// --- Public API Facade ---

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendType {
    Vulkan,
    VulkanWgpu,
    OpenGL,
    OpenGLWgpu,
    Software,
    #[cfg(target_os = "windows")]
    DirectX,
}

// A handle to a backend-specific texture resource.
pub enum Texture {
    Vulkan(vulkan::Texture),
    VulkanWgpu(wgpu_core::Texture),
    OpenGL(opengl::Texture),
    OpenGLWgpu(wgpu_core::Texture),
    Software(software::Texture),
    #[cfg(target_os = "windows")]
    DirectX(wgpu_core::Texture),
}

// An internal enum to hold the state for the active rendering backend.
enum BackendImpl {
    Vulkan(vulkan::State),
    VulkanWgpu(wgpu_core::State),
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
        render_list: &RenderList<'_>,
        textures: &HashMap<String, Texture>,
    ) -> Result<u32, Box<dyn Error>> {
        match &mut self.0 {
            BackendImpl::Vulkan(state) => vulkan::draw(state, render_list, textures),
            BackendImpl::VulkanWgpu(state) => wgpu_core::draw(state, render_list, textures),
            BackendImpl::OpenGL(state) => opengl::draw(state, render_list, textures),
            BackendImpl::OpenGLWgpu(state) => wgpu_core::draw(state, render_list, textures),
            BackendImpl::Software(state) => software::draw(state, render_list, textures),
            #[cfg(target_os = "windows")]
            BackendImpl::DirectX(state) => wgpu_core::draw(state, render_list, textures),
        }
    }

    pub fn configure_software_threads(&mut self, threads: Option<usize>) {
        if let BackendImpl::Software(state) = &mut self.0 {
            software::set_thread_hint(state, threads);
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        match &mut self.0 {
            BackendImpl::Vulkan(state) => vulkan::resize(state, width, height),
            BackendImpl::VulkanWgpu(state) => wgpu_core::resize(state, width, height),
            BackendImpl::OpenGL(state) => opengl::resize(state, width, height),
            BackendImpl::OpenGLWgpu(state) => wgpu_core::resize(state, width, height),
            BackendImpl::Software(state) => software::resize(state, width, height),
            #[cfg(target_os = "windows")]
            BackendImpl::DirectX(state) => wgpu_core::resize(state, width, height),
        }
    }

    pub fn cleanup(&mut self) {
        match &mut self.0 {
            BackendImpl::Vulkan(state) => vulkan::cleanup(state),
            BackendImpl::VulkanWgpu(state) => wgpu_core::cleanup(state),
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
            BackendImpl::Vulkan(state) => {
                let tex = vulkan::create_texture(state, image, sampler)?;
                Ok(Texture::Vulkan(tex))
            }
            BackendImpl::VulkanWgpu(state) => {
                let tex = wgpu_core::create_texture(state, image, sampler)?;
                Ok(Texture::VulkanWgpu(tex))
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

    pub fn dispose_textures(&mut self, textures: &mut HashMap<String, Texture>) {
        self.wait_for_idle();

        let old_textures = std::mem::take(textures);
        match &mut self.0 {
            BackendImpl::Vulkan(_) => {
                // Vulkan textures are cleaned up by their Drop implementation.
                drop(old_textures);
            }
            BackendImpl::VulkanWgpu(_) => {
                drop(old_textures);
            }
            BackendImpl::OpenGL(state) => unsafe {
                for tex in old_textures.values() {
                    if let Texture::OpenGL(opengl::Texture(handle)) = tex {
                        state.gl.delete_texture(*handle);
                    }
                }
            },
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
            BackendImpl::Vulkan(state) => {
                let _ = vulkan::flush_pending_uploads(state);
                if let Some(device) = &state.device {
                    unsafe {
                        let _ = device.device_wait_idle();
                    }
                }
            }
            BackendImpl::VulkanWgpu(state) => {
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
    gfx_debug_enabled: bool,
) -> Result<Backend, Box<dyn Error>> {
    let backend_impl = match backend_type {
        BackendType::Vulkan => {
            BackendImpl::Vulkan(vulkan::init(&window, vsync_enabled, gfx_debug_enabled)?)
        }
        BackendType::VulkanWgpu => BackendImpl::VulkanWgpu(wgpu_core::init_vulkan(
            window,
            vsync_enabled,
            gfx_debug_enabled,
        )?),
        BackendType::OpenGL => {
            BackendImpl::OpenGL(opengl::init(window, vsync_enabled, gfx_debug_enabled)?)
        }
        BackendType::OpenGLWgpu => BackendImpl::OpenGLWgpu(wgpu_core::init_opengl(
            window,
            vsync_enabled,
            gfx_debug_enabled,
        )?),
        BackendType::Software => BackendImpl::Software(software::init(window, vsync_enabled)?),
        #[cfg(target_os = "windows")]
        BackendType::DirectX => BackendImpl::DirectX(wgpu_core::init_dx12(
            window,
            vsync_enabled,
            gfx_debug_enabled,
        )?),
    };
    Ok(Backend(backend_impl))
}

// -- Boilerplate impls --
impl core::fmt::Display for BackendType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Vulkan => write!(f, "Vulkan"),
            Self::VulkanWgpu => write!(f, "Vulkan (wgpu)"),
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
            "vulkan" => Ok(Self::Vulkan),
            "vulkan-wgpu" | "vulkan_wgpu" | "wgpu-vulkan" | "vulkan (wgpu)" => Ok(Self::VulkanWgpu),
            "opengl" => Ok(Self::OpenGL),
            "opengl-wgpu" | "opengl_wgpu" | "wgpu-opengl" | "opengl (wgpu)" => Ok(Self::OpenGLWgpu),
            "software" | "cpu" => Ok(Self::Software),
            #[cfg(target_os = "windows")]
            "directx" | "dx12" | "directx (wgpu)" => Ok(Self::DirectX),
            _ => Err(format!("'{s}' is not a valid video renderer")),
        }
    }
}
