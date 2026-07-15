pub mod draw_prep;

use glam::Mat4 as Matrix4;
use std::ops::Deref;
use std::{collections::HashMap, sync::Arc};

// --- Public Data Contract ---
pub type TextureHandle = u64;
pub const INVALID_TEXTURE_HANDLE: TextureHandle = 0;
pub type FastU64Map<V> = HashMap<u64, V, rustc_hash::FxBuildHasher>;
pub type TMeshCacheKey = u64;
pub const INVALID_TMESH_CACHE_KEY: TMeshCacheKey = 0;

pub struct TextureHandleMap<V> {
    slots: Vec<Option<V>>,
}

impl<V> Default for TextureHandleMap<V> {
    #[inline(always)]
    fn default() -> Self {
        Self { slots: Vec::new() }
    }
}

impl<V> TextureHandleMap<V> {
    #[inline(always)]
    fn slot(handle: TextureHandle) -> Option<usize> {
        handle
            .checked_sub(1)
            .and_then(|index| usize::try_from(index).ok())
    }

    #[inline(always)]
    pub fn contains_key(&self, handle: &TextureHandle) -> bool {
        self.get(handle).is_some()
    }

    #[inline(always)]
    pub fn get(&self, handle: &TextureHandle) -> Option<&V> {
        self.slots.get(Self::slot(*handle)?)?.as_ref()
    }

    #[inline(always)]
    pub fn get_mut(&mut self, handle: &TextureHandle) -> Option<&mut V> {
        self.slots.get_mut(Self::slot(*handle)?)?.as_mut()
    }

    pub fn insert(&mut self, handle: TextureHandle, value: V) -> Option<V> {
        let slot = Self::slot(handle)?;
        if slot >= self.slots.len() {
            self.slots.resize_with(slot + 1, || None);
        }
        self.slots[slot].replace(value)
    }

    #[inline(always)]
    pub fn remove(&mut self, handle: &TextureHandle) -> Option<V> {
        self.slots.get_mut(Self::slot(*handle)?)?.take()
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.slots.clear();
    }

    #[inline(always)]
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.slots.iter().filter_map(Option::as_ref)
    }

    #[inline(always)]
    pub fn into_values(self) -> impl Iterator<Item = V> {
        self.slots.into_iter().flatten()
    }
}

#[derive(Clone)]
pub struct RenderList {
    pub clear_color: [f32; 4],
    pub cameras: Vec<Matrix4>,
    pub sprite_instances: Vec<SpriteInstanceRaw>,
    pub objects: Vec<RenderObject>,
}
#[derive(Clone)]
pub struct RenderObject {
    pub object_type: ObjectType,
    pub texture_handle: TextureHandle,
    pub blend: BlendMode,
    pub z: i16,
    pub order: u32,
    pub camera: u8,
}

#[repr(C)]
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    bytemuck::Pod,
    bytemuck::Zeroable,
)]
pub struct MeshVertex {
    pub pos: [f32; 2],
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(
    Clone, Copy, Debug, serde::Serialize, serde::Deserialize, bytemuck::Pod, bytemuck::Zeroable,
)]
pub struct TexturedMeshVertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    pub tex_matrix_scale: [f32; 2],
}

impl Default for TexturedMeshVertex {
    #[inline(always)]
    fn default() -> Self {
        Self {
            pos: [0.0, 0.0, 0.0],
            uv: [0.0, 0.0],
            color: [0.0, 0.0, 0.0, 0.0],
            tex_matrix_scale: [1.0, 1.0],
        }
    }
}

#[derive(Clone)]
pub enum TexturedMeshVertices {
    Shared(Arc<[TexturedMeshVertex]>),
    Transient(Vec<TexturedMeshVertex>),
}

impl AsRef<[TexturedMeshVertex]> for TexturedMeshVertices {
    #[inline(always)]
    fn as_ref(&self) -> &[TexturedMeshVertex] {
        match self {
            Self::Shared(vertices) => vertices.as_ref(),
            Self::Transient(vertices) => vertices.as_slice(),
        }
    }
}

impl Deref for TexturedMeshVertices {
    type Target = [TexturedMeshVertex];

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

#[repr(C)]
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    bytemuck::Pod,
    bytemuck::Zeroable,
)]
pub struct SpriteInstanceRaw {
    pub center: [f32; 4],
    pub size: [f32; 2],
    pub rot_sin_cos: [f32; 2],
    pub tint: [f32; 4],
    pub uv_scale: [f32; 2],
    pub uv_offset: [f32; 2],
    pub local_offset: [f32; 2],
    pub local_offset_rot_sin_cos: [f32; 2],
    pub edge_fade: [f32; 4],
    pub texture_mask: f32,
}

#[repr(C)]
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    bytemuck::Pod,
    bytemuck::Zeroable,
)]
pub struct TexturedMeshInstanceRaw {
    pub model_col0: [f32; 4],
    pub model_col1: [f32; 4],
    pub model_col2: [f32; 4],
    pub model_col3: [f32; 4],
    pub tint: [f32; 4],
    pub uv_scale: [f32; 2],
    pub uv_offset: [f32; 2],
    pub uv_tex_shift: [f32; 2],
    pub texture_mask: f32,
}

impl TexturedMeshInstanceRaw {
    #[inline(always)]
    pub fn new(
        transform: Matrix4,
        tint: [f32; 4],
        uv_scale: [f32; 2],
        uv_offset: [f32; 2],
        uv_tex_shift: [f32; 2],
        texture_mask: bool,
    ) -> Self {
        Self {
            model_col0: [
                transform.x_axis.x,
                transform.x_axis.y,
                transform.x_axis.z,
                transform.x_axis.w,
            ],
            model_col1: [
                transform.y_axis.x,
                transform.y_axis.y,
                transform.y_axis.z,
                transform.y_axis.w,
            ],
            model_col2: [
                transform.z_axis.x,
                transform.z_axis.y,
                transform.z_axis.z,
                transform.z_axis.w,
            ],
            model_col3: [
                transform.w_axis.x,
                transform.w_axis.y,
                transform.w_axis.z,
                transform.w_axis.w,
            ],
            tint,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            texture_mask: texture_mask as u8 as f32,
        }
    }

    #[inline(always)]
    pub fn transform(&self) -> Matrix4 {
        Matrix4::from_cols_array(&[
            self.model_col0[0],
            self.model_col0[1],
            self.model_col0[2],
            self.model_col0[3],
            self.model_col1[0],
            self.model_col1[1],
            self.model_col1[2],
            self.model_col1[3],
            self.model_col2[0],
            self.model_col2[1],
            self.model_col2[2],
            self.model_col2[3],
            self.model_col3[0],
            self.model_col3[1],
            self.model_col3[2],
            self.model_col3[3],
        ])
    }

    #[inline(always)]
    pub fn set_transform(&mut self, transform: Matrix4) {
        self.model_col0 = [
            transform.x_axis.x,
            transform.x_axis.y,
            transform.x_axis.z,
            transform.x_axis.w,
        ];
        self.model_col1 = [
            transform.y_axis.x,
            transform.y_axis.y,
            transform.y_axis.z,
            transform.y_axis.w,
        ];
        self.model_col2 = [
            transform.z_axis.x,
            transform.z_axis.y,
            transform.z_axis.z,
            transform.z_axis.w,
        ];
        self.model_col3 = [
            transform.w_axis.x,
            transform.w_axis.y,
            transform.w_axis.z,
            transform.w_axis.w,
        ];
    }
}

#[derive(Clone)]
pub enum ObjectType {
    Sprite(u32),
    Mesh {
        transform: Matrix4,
        tint: [f32; 4],
        vertices: Arc<[MeshVertex]>,
    },
    TexturedMesh {
        instance: TexturedMeshInstanceRaw,
        vertices: TexturedMeshVertices,
        geom_cache_key: TMeshCacheKey,
        depth_test: bool,
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

impl core::fmt::Display for BackendType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            Self::Vulkan => f.write_str("Vulkan"),
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            Self::VulkanWgpu => f.write_str("Vulkan (wgpu)"),
            #[cfg(target_os = "macos")]
            Self::Metal => f.write_str("Metal (wgpu)"),
            Self::OpenGL => f.write_str("OpenGL"),
            Self::OpenGLWgpu => f.write_str("OpenGL (wgpu)"),
            Self::Software => f.write_str("Software"),
            #[cfg(target_os = "windows")]
            Self::DirectX => f.write_str("DirectX"),
        }
    }
}

impl core::str::FromStr for BackendType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
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

#[cfg(all(
    target_os = "windows",
    not(target_vendor = "win7"),
    not(target_pointer_width = "32")
))]
pub const BACKEND_TYPE_CHOICES: &[(BackendType, &str)] = &[
    (BackendType::OpenGL, "OpenGL"),
    (BackendType::Vulkan, "Vulkan"),
    (BackendType::DirectX, "DirectX"),
    (BackendType::OpenGLWgpu, "OpenGL (wgpu)"),
    (BackendType::VulkanWgpu, "Vulkan (wgpu)"),
    (BackendType::Software, "Software"),
];
#[cfg(all(
    target_os = "windows",
    any(target_vendor = "win7", target_pointer_width = "32")
))]
pub const BACKEND_TYPE_CHOICES: &[(BackendType, &str)] = &[
    (BackendType::OpenGL, "OpenGL"),
    (BackendType::DirectX, "DirectX"),
    (BackendType::OpenGLWgpu, "OpenGL (wgpu)"),
    (BackendType::Software, "Software"),
];
#[cfg(all(target_os = "macos", not(target_pointer_width = "32")))]
pub const BACKEND_TYPE_CHOICES: &[(BackendType, &str)] = &[
    (BackendType::OpenGL, "OpenGL"),
    (BackendType::Vulkan, "Vulkan"),
    (BackendType::Metal, "Metal (wgpu)"),
    (BackendType::OpenGLWgpu, "OpenGL (wgpu)"),
    (BackendType::VulkanWgpu, "Vulkan (wgpu)"),
    (BackendType::Software, "Software"),
];
#[cfg(all(
    not(any(target_os = "windows", target_os = "macos")),
    not(target_pointer_width = "32")
))]
pub const BACKEND_TYPE_CHOICES: &[(BackendType, &str)] = &[
    (BackendType::OpenGL, "OpenGL"),
    (BackendType::Vulkan, "Vulkan"),
    (BackendType::OpenGLWgpu, "OpenGL (wgpu)"),
    (BackendType::VulkanWgpu, "Vulkan (wgpu)"),
    (BackendType::Software, "Software"),
];
#[cfg(all(not(target_os = "windows"), target_pointer_width = "32"))]
pub const BACKEND_TYPE_CHOICES: &[(BackendType, &str)] = &[
    (BackendType::OpenGL, "OpenGL"),
    (BackendType::OpenGLWgpu, "OpenGL (wgpu)"),
    (BackendType::Software, "Software"),
];

pub fn backend_type_choice_index(backend: BackendType) -> usize {
    BACKEND_TYPE_CHOICES
        .iter()
        .position(|(candidate, _)| *candidate == backend)
        .unwrap_or(0)
}

pub fn backend_type_from_choice(idx: usize) -> BackendType {
    BACKEND_TYPE_CHOICES
        .get(idx)
        .map_or_else(|| BACKEND_TYPE_CHOICES[0].0, |(backend, _)| *backend)
}

pub fn build_software_thread_choices() -> Vec<u8> {
    let max_threads = std::thread::available_parallelism()
        .map(std::num::NonZero::get)
        .unwrap_or(8)
        .clamp(2, 32);
    let mut out = Vec::with_capacity(max_threads + 1);
    out.push(0);
    for n in 1..=max_threads {
        out.push(n as u8);
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentModePolicy {
    Mailbox,
    Immediate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PresentModeTrace {
    #[default]
    Unknown,
    Fifo,
    FifoRelaxed,
    Mailbox,
    Immediate,
}

impl PresentModeTrace {
    #[inline(always)]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Fifo => "fifo",
            Self::FifoRelaxed => "fifo_relaxed",
            Self::Mailbox => "mailbox",
            Self::Immediate => "immediate",
        }
    }
}

impl core::fmt::Display for PresentModeTrace {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ClockDomainTrace {
    #[default]
    Unknown,
    Device,
    Monotonic,
    MonotonicRaw,
    Qpc,
}

impl ClockDomainTrace {
    #[inline(always)]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Device => "device",
            Self::Monotonic => "monotonic",
            Self::MonotonicRaw => "monotonic_raw",
            Self::Qpc => "qpc",
        }
    }
}

impl core::fmt::Display for ClockDomainTrace {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PresentStats {
    pub mode: PresentModeTrace,
    pub display_clock: ClockDomainTrace,
    pub host_clock: ClockDomainTrace,
    pub in_flight_images: u8,
    pub waited_for_image: bool,
    pub applied_back_pressure: bool,
    pub queue_idle_waited: bool,
    pub suboptimal: bool,
    pub submitted_present_id: u32,
    pub completed_present_id: u32,
    pub refresh_ns: u64,
    pub actual_interval_ns: u64,
    pub present_margin_ns: u64,
    pub host_present_ns: u64,
    pub calibration_error_ns: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DrawStats {
    pub vertices: u32,
    pub acquire_us: u32,
    pub submit_us: u32,
    pub present_us: u32,
    pub present_stats: PresentStats,
    pub gpu_wait_us: u32,
    pub backend_setup_us: u32,
    pub backend_prepare_us: u32,
    pub backend_record_us: u32,
}
impl PresentModePolicy {
    #[inline(always)]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mailbox => "mailbox",
            Self::Immediate => "immediate",
        }
    }
}

pub const fn present_mode_policy_choice_index(policy: PresentModePolicy) -> usize {
    match policy {
        PresentModePolicy::Mailbox => 0,
        PresentModePolicy::Immediate => 1,
    }
}

pub const fn present_mode_policy_from_choice(idx: usize) -> PresentModePolicy {
    match idx {
        1 => PresentModePolicy::Immediate,
        _ => PresentModePolicy::Mailbox,
    }
}

impl core::fmt::Display for PresentModePolicy {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl core::str::FromStr for PresentModePolicy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "mailbox" | "balanced" => Ok(Self::Mailbox),
            "immediate" | "unhinged" => Ok(Self::Immediate),
            other => Err(format!("'{other}' is not a valid present mode policy")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn present_mode_policy_choices_match_options_order() {
        assert_eq!(
            present_mode_policy_choice_index(PresentModePolicy::Mailbox),
            0
        );
        assert_eq!(
            present_mode_policy_choice_index(PresentModePolicy::Immediate),
            1
        );
        assert_eq!(
            present_mode_policy_from_choice(0),
            PresentModePolicy::Mailbox
        );
        assert_eq!(
            present_mode_policy_from_choice(1),
            PresentModePolicy::Immediate
        );
        assert_eq!(
            present_mode_policy_from_choice(99),
            PresentModePolicy::Mailbox
        );
    }

    #[test]
    fn backend_type_choices_match_options_order() {
        assert_eq!(BACKEND_TYPE_CHOICES[0], (BackendType::OpenGL, "OpenGL"));
        assert_eq!(backend_type_choice_index(BackendType::OpenGL), 0);
        assert_eq!(backend_type_from_choice(0), BackendType::OpenGL);
        assert_eq!(backend_type_from_choice(usize::MAX), BackendType::OpenGL);
        assert_eq!(
            backend_type_choice_index(BackendType::Software),
            BACKEND_TYPE_CHOICES.len() - 1
        );

        #[cfg(target_os = "windows")]
        assert!(
            BACKEND_TYPE_CHOICES
                .iter()
                .any(|(backend, _)| *backend == BackendType::DirectX)
        );
        #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
        assert!(
            BACKEND_TYPE_CHOICES
                .iter()
                .any(|(backend, _)| *backend == BackendType::VulkanWgpu)
        );
        #[cfg(target_os = "macos")]
        assert!(
            BACKEND_TYPE_CHOICES
                .iter()
                .any(|(backend, _)| *backend == BackendType::Metal)
        );
    }

    #[test]
    fn software_thread_choices_include_auto_and_available_range() {
        let choices = build_software_thread_choices();
        assert_eq!(choices.first().copied(), Some(0));
        assert!(choices.len() >= 3);
        assert!(choices.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(choices.len() <= 33);
    }
}
