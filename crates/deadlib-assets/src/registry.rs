use crate::parse_sprite_sheet_dims;
use deadlib_render_core::SamplerDesc;
use image::RgbaImage;
use rustc_hash::FxHashMap;
use std::sync::{
    Arc, LazyLock, RwLock,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

#[derive(Clone, Copy, Debug)]
pub struct TexMeta {
    pub w: u32,
    pub h: u32,
}

#[derive(Clone)]
pub struct GeneratedTexture {
    pub image: Arc<RgbaImage>,
    pub sampler: SamplerDesc,
}

#[derive(Clone, Copy)]
struct RegisteredTextureMeta {
    texture: Option<TexMeta>,
    sheet: (u32, u32),
}

#[derive(Default)]
struct TextureMetadataRegistry {
    entries: FxHashMap<Arc<str>, RegisteredTextureMeta>,
}

impl TextureMetadataRegistry {
    fn reserve(&mut self, additional: usize) {
        self.entries.reserve(additional);
    }

    fn register(
        &mut self,
        key: impl AsRef<str> + Into<Arc<str>>,
        texture: TexMeta,
        sheet: (u32, u32),
    ) -> bool {
        if let Some(entry) = self.entries.get_mut(key.as_ref()) {
            if entry
                .texture
                .is_some_and(|meta| meta.w == texture.w && meta.h == texture.h)
                && entry.sheet == sheet
            {
                return false;
            }
            *entry = RegisteredTextureMeta {
                texture: Some(texture),
                sheet,
            };
        } else {
            self.entries.insert(
                key.into(),
                RegisteredTextureMeta {
                    texture: Some(texture),
                    sheet,
                },
            );
        }
        true
    }

    fn texture_dims(&self, key: &str) -> Option<TexMeta> {
        self.entries.get(key).and_then(|entry| entry.texture)
    }

    fn sheet_dims(&self, key: &str) -> Option<(u32, u32)> {
        self.entries.get(key).map(|entry| entry.sheet)
    }

    fn insert_sheet_dims(
        &mut self,
        key: impl AsRef<str> + Into<Arc<str>>,
        sheet: (u32, u32),
    ) -> (u32, u32) {
        if let Some(entry) = self.entries.get(key.as_ref()) {
            return entry.sheet;
        }
        self.entries
            .entry(key.into())
            .or_insert(RegisteredTextureMeta {
                texture: None,
                sheet,
            })
            .sheet
    }
}

struct GeneratedTextureEntry<T> {
    key: Arc<str>,
    value: T,
    pending: bool,
}

/// Session-lifetime generated-texture registry drained by the render thread.
///
/// Producers append only the first update for a key until the next drain. The
/// pending vector shares the registered keys, so draining touches only
/// changed entries and then retains that vector's allocation for the next
/// batch. Its `Arc<str>` values share the session-owned map keys, so an update
/// performs no key allocation. Delivery holds the registry write lock once for
/// the batch; a miss is impossible because entries are never evicted.
/// Destruction occurs when the process registry is dropped.
/// `asset_cache_hot_paths` measures sparse-drain and delivery cost. Worst-case
/// drain work is linear in changed textures, never in the full registry size.
struct GeneratedTextureRegistry<T> {
    entries: FxHashMap<Arc<str>, GeneratedTextureEntry<T>>,
    pending_keys: Vec<Arc<str>>,
}

impl<T> Default for GeneratedTextureRegistry<T> {
    fn default() -> Self {
        Self {
            entries: FxHashMap::default(),
            pending_keys: Vec::new(),
        }
    }
}

impl<T> GeneratedTextureRegistry<T> {
    fn register(&mut self, key: &str, value: T) {
        if let Some(entry) = self.entries.get_mut(key) {
            entry.value = value;
            if !entry.pending {
                entry.pending = true;
                self.pending_keys.push(Arc::clone(&entry.key));
            }
            return;
        }
        let key: Arc<str> = Arc::from(key);
        self.pending_keys.push(Arc::clone(&key));
        self.entries.insert(
            Arc::clone(&key),
            GeneratedTextureEntry {
                key,
                value,
                pending: true,
            },
        );
    }

    fn get(&self, key: &str) -> Option<&T> {
        self.entries.get(key).map(|entry| &entry.value)
    }

    fn shared_key(&self, key: &str) -> Option<Arc<str>> {
        self.entries.get(key).map(|entry| Arc::clone(&entry.key))
    }

    #[cfg(test)]
    fn take_pending_keys(&mut self) -> Vec<Arc<str>> {
        let pending = std::mem::take(&mut self.pending_keys);
        for key in &pending {
            self.entries
                .get_mut(key)
                .expect("queued generated texture must remain registered")
                .pending = false;
        }
        pending
    }

    fn drain_pending(&mut self, mut visit: impl FnMut(Arc<str>, T))
    where
        T: Clone,
    {
        let mut pending = std::mem::take(&mut self.pending_keys);
        for key in pending.drain(..) {
            let entry = self
                .entries
                .get_mut(&key)
                .expect("queued generated texture must remain registered");
            entry.pending = false;
            let value = entry.value.clone();
            visit(key, value);
        }
        self.pending_keys = pending;
    }
}

static TEXTURE_METADATA: LazyLock<RwLock<TextureMetadataRegistry>> =
    LazyLock::new(|| RwLock::new(TextureMetadataRegistry::default()));

static GENERATED_TEXTURES: LazyLock<RwLock<GeneratedTextureRegistry<GeneratedTexture>>> =
    LazyLock::new(|| RwLock::new(GeneratedTextureRegistry::default()));
// Producers publish after inserting under the registry write lock. The render
// thread uses this as an idle fast gate, then takes that same lock to drain;
// a concurrent late publish can only cause one harmless extra poll.
static GENERATED_TEXTURES_PENDING: AtomicBool = AtomicBool::new(false);
// Unique stamps let presentation distinguish independent stores as well as
// metadata revisions. This counter contains no GPU identities or mappings.
static NEXT_TEXTURE_REVISION: AtomicU64 = AtomicU64::new(2);
static TEXTURE_REGISTRY_GENERATION: AtomicU64 = AtomicU64::new(1);

pub(crate) fn next_texture_revision() -> u64 {
    NEXT_TEXTURE_REVISION.fetch_add(1, Ordering::Relaxed)
}

#[inline(always)]
fn touch_texture_registry() {
    TEXTURE_REGISTRY_GENERATION.store(next_texture_revision(), Ordering::Relaxed);
}

#[inline(always)]
pub fn texture_registry_generation() -> u64 {
    TEXTURE_REGISTRY_GENERATION.load(Ordering::Relaxed)
}

pub(crate) fn reserve_texture_metadata(additional: usize) {
    TEXTURE_METADATA.write().unwrap().reserve(additional);
}

/// # Panics
///
/// Panics if an internal synchronization lock is poisoned.
pub fn register_texture_dims(key: &str, w: u32, h: u32) {
    let sheet = parse_sprite_sheet_dims(key);
    if TEXTURE_METADATA
        .write()
        .unwrap()
        .register(key, TexMeta { w, h }, sheet)
    {
        touch_texture_registry();
    }
}

pub(crate) fn register_texture_dims_shared(key: Arc<str>, w: u32, h: u32) {
    let sheet = parse_sprite_sheet_dims(&key);
    if TEXTURE_METADATA
        .write()
        .unwrap()
        .register(key, TexMeta { w, h }, sheet)
    {
        touch_texture_registry();
    }
}

/// # Panics
///
/// Panics if an internal synchronization lock is poisoned.
pub fn texture_dims(key: &str) -> Option<TexMeta> {
    TEXTURE_METADATA.read().unwrap().texture_dims(key)
}

/// # Panics
///
/// Panics if an internal synchronization lock is poisoned.
pub fn sprite_sheet_dims(key: &str) -> (u32, u32) {
    if let Some(dims) = TEXTURE_METADATA.read().unwrap().sheet_dims(key) {
        return dims;
    }
    let dims = parse_sprite_sheet_dims(key);
    TEXTURE_METADATA
        .write()
        .unwrap()
        .insert_sheet_dims(key, dims)
}

/// # Panics
///
/// Panics if an internal synchronization lock is poisoned.
pub fn register_generated_texture(key: &str, image: RgbaImage, sampler: SamplerDesc) {
    let (w, h) = (image.width(), image.height());
    GENERATED_TEXTURES.write().unwrap().register(
        key,
        GeneratedTexture {
            image: Arc::new(image),
            sampler,
        },
    );
    GENERATED_TEXTURES_PENDING.store(true, Ordering::Release);
    register_texture_dims(key, w, h);
}

/// # Panics
///
/// Panics if an internal synchronization lock is poisoned.
pub fn generated_texture(key: &str) -> Option<GeneratedTexture> {
    GENERATED_TEXTURES.read().unwrap().get(key).cloned()
}

/// Return the session-owned key for a generated texture without allocating a
/// second string.
///
/// # Panics
///
/// Panics if the generated-texture registry lock is poisoned.
#[must_use]
pub fn generated_texture_shared_key(key: &str) -> Option<Arc<str>> {
    GENERATED_TEXTURES.read().unwrap().shared_key(key)
}

pub(crate) fn drain_pending_generated_textures(mut visit: impl FnMut(Arc<str>, GeneratedTexture)) {
    if !GENERATED_TEXTURES_PENDING.swap(false, Ordering::AcqRel) {
        return;
    }
    GENERATED_TEXTURES
        .write()
        .unwrap()
        .drain_pending(&mut visit);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combined_metadata_registry_preserves_cached_sheet_and_texture_dimensions() {
        let mut registry = TextureMetadataRegistry::default();
        let key = "noteskins/dance/tap note 4x1.png";

        assert_eq!(registry.insert_sheet_dims(key, (4, 1)), (4, 1));
        assert!(registry.texture_dims(key).is_none());
        assert!(registry.register(key, TexMeta { w: 256, h: 64 }, (4, 1)));
        assert_eq!(
            registry.texture_dims(key).map(|meta| (meta.w, meta.h)),
            Some((256, 64))
        );
        assert_eq!(registry.sheet_dims(key), Some((4, 1)));
        assert!(!registry.register(key, TexMeta { w: 256, h: 64 }, (4, 1)));
        assert!(registry.register(key, TexMeta { w: 512, h: 128 }, (4, 1)));
        assert_eq!(
            registry.texture_dims(key).map(|meta| (meta.w, meta.h)),
            Some((512, 128))
        );
    }

    #[test]
    fn generated_registry_coalesces_replacements_and_pending_keys() {
        let mut registry = GeneratedTextureRegistry::default();
        registry.register("generated/lifebar", 1_u64);
        registry.register("generated/lifebar", 2_u64);

        assert_eq!(registry.get("generated/lifebar"), Some(&2));
        let first_key = registry.shared_key("generated/lifebar").unwrap();
        let second_key = registry.shared_key("generated/lifebar").unwrap();
        assert!(Arc::ptr_eq(&first_key, &second_key));
        assert_eq!(
            registry
                .take_pending_keys()
                .iter()
                .map(AsRef::as_ref)
                .collect::<Vec<_>>(),
            ["generated/lifebar"]
        );
        assert!(registry.take_pending_keys().is_empty());

        registry.register("generated/lifebar", 3);
        assert!(Arc::ptr_eq(
            &first_key,
            &registry.shared_key("generated/lifebar").unwrap()
        ));
        assert_eq!(
            registry
                .take_pending_keys()
                .iter()
                .map(AsRef::as_ref)
                .collect::<Vec<_>>(),
            ["generated/lifebar"]
        );
    }

    #[test]
    fn generated_registry_drains_only_changed_keys_in_publish_order() {
        let mut registry = GeneratedTextureRegistry::default();
        for index in 0..512 {
            registry.register(&format!("generated/{index:04}"), index);
        }
        assert_eq!(registry.take_pending_keys().len(), 512);

        registry.register("generated/0400", 900);
        registry.register("generated/0007", 901);
        registry.register("generated/0400", 902);

        assert_eq!(
            registry
                .take_pending_keys()
                .iter()
                .map(AsRef::as_ref)
                .collect::<Vec<_>>(),
            ["generated/0400", "generated/0007"]
        );
        assert_eq!(registry.get("generated/0400"), Some(&902));
        assert!(registry.take_pending_keys().is_empty());
    }

    #[test]
    fn generated_registry_delivers_latest_values_and_reuses_owned_keys() {
        let mut registry = GeneratedTextureRegistry::default();
        registry.register("generated/a", 1_u64);
        registry.register("generated/b", 2);
        registry.register("generated/a", 3);

        let stored_key = Arc::clone(
            &registry
                .entries
                .get("generated/a")
                .expect("registered key exists")
                .key,
        );
        let mut delivered = Vec::new();
        registry.drain_pending(|key, value| delivered.push((key, value)));

        assert!(Arc::ptr_eq(&stored_key, &delivered[0].0));
        assert_eq!(
            delivered
                .iter()
                .map(|(key, value)| (key.as_ref(), *value))
                .collect::<Vec<_>>(),
            [("generated/a", 3), ("generated/b", 2)]
        );
        assert!(registry.pending_keys.is_empty());
    }

    #[test]
    fn generated_texture_pending_gate_coalesces_idle_polls() {
        GENERATED_TEXTURES_PENDING.store(false, Ordering::Release);
        let _ = GENERATED_TEXTURES.write().unwrap().take_pending_keys();
        register_generated_texture(
            "generated/pending-gate-test",
            RgbaImage::new(1, 1),
            SamplerDesc::default(),
        );

        let mut pending = Vec::new();
        drain_pending_generated_textures(|key, _| pending.push(key));
        assert!(
            pending
                .iter()
                .any(|key| key.as_ref() == "generated/pending-gate-test")
        );
        drain_pending_generated_textures(|key, _| pending.push(key));
        assert_eq!(pending.len(), 1);
    }
}
