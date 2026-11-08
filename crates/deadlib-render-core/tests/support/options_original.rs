//! Frozen renderer option/slot code from 64f4ace17.
use deadlib_render_core::{BackendType, FastU64Map, PresentModePolicy};

#[derive(Debug)]
pub struct DenseSlotMap<V> {
    slots: FastU64Map<u32>,
    values: Vec<V>,
}

impl<V> Default for DenseSlotMap<V> {
    fn default() -> Self {
        Self {
            slots: FastU64Map::default(),
            values: Vec::new(),
        }
    }
}

impl<V> DenseSlotMap<V> {
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            slots: FastU64Map::with_capacity_and_hasher(
                capacity,
                rustc_hash::FxBuildHasher::default(),
            ),
            values: Vec::with_capacity(capacity),
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn get(&self, key: u64) -> Option<(u64, &V)> {
        let slot = *self.slots.get(&key)?;
        Some((u64::from(slot) + 1, self.values.get(slot as usize)?))
    }

    #[inline(always)]
    #[must_use]
    pub fn get_slot(&self, buffer_key: u64) -> Option<&V> {
        self.values
            .get(usize::try_from(buffer_key.checked_sub(1)?).ok()?)
    }

    /// # Panics
    ///
    /// Panics if the function's input or state invariants are violated.
    pub fn insert(&mut self, key: u64, value: V) -> u64 {
        assert!(key != 0, "dense slot identity zero is reserved");
        let slot = match self.slots.entry(key) {
            std::collections::hash_map::Entry::Occupied(entry) => {
                let slot = *entry.get();
                self.values[slot as usize] = value;
                slot
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                let slot = u32::try_from(self.values.len()).expect("dense slot count exceeds u32");
                self.values.push(value);
                entry.insert(slot);
                slot
            }
        };
        u64::from(slot) + 1
    }

    #[inline(always)]
    #[must_use]
    pub const fn len(&self) -> usize {
        self.values.len()
    }

    #[inline(always)]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn drain(&mut self) -> impl Iterator<Item = V> + '_ {
        self.slots.clear();
        self.values.drain(..)
    }

    pub fn clear(&mut self) {
        self.slots.clear();
        self.values.clear();
    }
}

pub fn backend(s: &str) -> Result<BackendType, String> {
    match s.trim().to_ascii_lowercase().as_str() {
        #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
        "vulkan" => Ok(BackendType::Vulkan),
        #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
        "vulkan-wgpu" | "vulkan_wgpu" | "wgpu-vulkan" | "vulkan (wgpu)" => {
            Ok(BackendType::VulkanWgpu)
        }
        #[cfg(target_os = "macos")]
        "metal" | "metal-native" | "metal_native" | "native-metal" => Ok(BackendType::Metal),
        #[cfg(target_os = "macos")]
        "metal-wgpu" | "metal_wgpu" | "wgpu-metal" | "metal (wgpu)" => Ok(BackendType::MetalWgpu),
        "opengl" => Ok(BackendType::OpenGL),
        "opengl-wgpu" | "opengl_wgpu" | "wgpu-opengl" | "opengl (wgpu)" => {
            Ok(BackendType::OpenGLWgpu)
        }
        "software" | "cpu" => Ok(BackendType::Software),
        #[cfg(target_os = "windows")]
        "directx" | "dx12" | "directx (wgpu)" => Ok(BackendType::DirectX),
        _ => Err(format!("'{s}' is not a valid video renderer")),
    }
}
pub fn policy(s: &str) -> Result<PresentModePolicy, String> {
    match s.trim().to_ascii_lowercase().as_str() {
        "mailbox" | "balanced" => Ok(PresentModePolicy::Mailbox),
        "immediate" | "unhinged" => Ok(PresentModePolicy::Immediate),
        other => Err(format!("'{other}' is not a valid present mode policy")),
    }
}
