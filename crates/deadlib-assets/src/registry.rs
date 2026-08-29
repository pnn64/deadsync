use crate::{ascii_ci_hash, parse_sprite_sheet_dims};
use deadlib_render_core::{FastU64Map, INVALID_TEXTURE_HANDLE, SamplerDesc, TextureHandle};
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

    #[cfg(any(test, feature = "bench-support"))]
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

static TEXTURE_HANDLES: LazyLock<RwLock<FxHashMap<Arc<str>, TextureHandle>>> =
    LazyLock::new(|| RwLock::new(FxHashMap::default()));

#[derive(Clone, Copy)]
struct TextureHandleAlias {
    handle: TextureHandle,
    refs: usize,
}

/// Process-lifetime case-insensitive texture alias index.
///
/// Ownership/threading: all asset/render users share it behind this `RwLock`.
/// Capacity/lifetime: at most one entry per folded registered key, grown during
/// load/prewarm and cleared with the texture registry. Lookup misses fall back
/// to the exact registry scan and never perform I/O. Unique removals are O(1);
/// the deliberately rare ambiguous-hash removal rebuilds the bounded registry
/// to recover the surviving exact handle. Destruction remains on the caller's
/// render/transition path. `texture_identity_hot_paths` reports removal cost;
/// no live counter exists yet. Worst-case work is one full registry scan only
/// after removing an alias already marked ambiguous.
static TEXTURE_HANDLE_ALIASES: LazyLock<RwLock<FastU64Map<TextureHandleAlias>>> =
    LazyLock::new(|| RwLock::new(FastU64Map::default()));

static GENERATED_TEXTURES: LazyLock<RwLock<GeneratedTextureRegistry<GeneratedTexture>>> =
    LazyLock::new(|| RwLock::new(GeneratedTextureRegistry::default()));
// Producers publish after inserting under the registry write lock. The render
// thread uses this as an idle fast gate, then takes that same lock to drain;
// a concurrent late publish can only cause one harmless extra poll.
static GENERATED_TEXTURES_PENDING: AtomicBool = AtomicBool::new(false);
static TEXTURE_REGISTRY_GENERATION: AtomicU64 = AtomicU64::new(1);

#[inline(always)]
fn touch_texture_registry() {
    TEXTURE_REGISTRY_GENERATION.fetch_add(1, Ordering::Relaxed);
}

#[inline(always)]
pub fn texture_registry_generation() -> u64 {
    TEXTURE_REGISTRY_GENERATION.load(Ordering::Relaxed)
}

fn note_texture_handle_alias(
    aliases: &mut FastU64Map<TextureHandleAlias>,
    key: &str,
    handle: TextureHandle,
) {
    let folded = ascii_ci_hash(key);
    match aliases.get_mut(&folded) {
        Some(existing) => {
            if existing.handle != handle {
                existing.handle = INVALID_TEXTURE_HANDLE;
            }
            existing.refs = existing.refs.saturating_add(1);
        }
        None => {
            aliases.insert(folded, TextureHandleAlias { handle, refs: 1 });
        }
    }
}

fn rebuild_texture_handle_aliases(
    handles: &FxHashMap<Arc<str>, TextureHandle>,
    aliases: &mut FastU64Map<TextureHandleAlias>,
) {
    aliases.clear();
    aliases.reserve(handles.len());
    for (key, &handle) in handles {
        note_texture_handle_alias(aliases, key, handle);
    }
}

/// Remove a common unique alias in O(1). An already-colliding alias takes the
/// rare rebuild path so deleting one collision restores exact fallback lookup.
fn remove_texture_handle_alias(
    handles: &FxHashMap<Arc<str>, TextureHandle>,
    aliases: &mut FastU64Map<TextureHandleAlias>,
    key: &str,
) {
    let folded = ascii_ci_hash(key);
    let Some(alias) = aliases.get_mut(&folded) else {
        return;
    };
    if alias.handle == INVALID_TEXTURE_HANDLE {
        rebuild_texture_handle_aliases(handles, aliases);
    } else if alias.refs > 1 {
        alias.refs -= 1;
    } else {
        aliases.remove(&folded);
    }
}

/// # Panics
///
/// Panics if an internal synchronization lock is poisoned.
fn register_texture_handle_inner(key: impl AsRef<str> + Into<Arc<str>>, handle: TextureHandle) {
    let mut handles = TEXTURE_HANDLES.write().unwrap();
    let mut aliases = TEXTURE_HANDLE_ALIASES.write().unwrap();
    let lookup = key.as_ref();
    if let Some((owned_key, old)) = handles.remove_entry(lookup) {
        if old == handle {
            handles.insert(owned_key, old);
            return;
        }
        remove_texture_handle_alias(&handles, &mut aliases, lookup);
        handles.insert(owned_key, handle);
        note_texture_handle_alias(&mut aliases, lookup, handle);
        touch_texture_registry();
    } else {
        note_texture_handle_alias(&mut aliases, lookup, handle);
        handles.insert(key.into(), handle);
        touch_texture_registry();
    }
}

/// # Panics
///
/// Panics if an internal synchronization lock is poisoned.
pub fn register_texture_handle(key: &str, handle: TextureHandle) {
    register_texture_handle_inner(key, handle);
}

pub(crate) fn register_texture_handle_shared(key: Arc<str>, handle: TextureHandle) {
    register_texture_handle_inner(key, handle);
}

/// # Panics
///
/// Panics if an internal synchronization lock is poisoned.
pub fn remove_texture_handle(key: &str) {
    let mut handles = TEXTURE_HANDLES.write().unwrap();
    if handles.remove(key).is_none() {
        return;
    }
    let mut aliases = TEXTURE_HANDLE_ALIASES.write().unwrap();
    remove_texture_handle_alias(&handles, &mut aliases, key);
    touch_texture_registry();
}

/// # Panics
///
/// Panics if an internal synchronization lock is poisoned.
pub fn clear_texture_handles() {
    TEXTURE_HANDLES.write().unwrap().clear();
    TEXTURE_HANDLE_ALIASES.write().unwrap().clear();
    touch_texture_registry();
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
pub fn texture_handle(key: &str) -> TextureHandle {
    if let Some(handle) = TEXTURE_HANDLES.read().unwrap().get(key).copied() {
        return handle;
    }
    if let Some(handle) = TEXTURE_HANDLE_ALIASES
        .read()
        .unwrap()
        .get(&ascii_ci_hash(key))
        .map(|alias| alias.handle)
        && handle != INVALID_TEXTURE_HANDLE
    {
        return handle;
    }
    TEXTURE_HANDLES
        .read()
        .unwrap()
        .iter()
        .find_map(|(candidate, handle)| candidate.eq_ignore_ascii_case(key).then_some(*handle))
        .unwrap_or(INVALID_TEXTURE_HANDLE)
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

pub(crate) fn drain_pending_generated_textures(mut visit: impl FnMut(Arc<str>, GeneratedTexture)) {
    if !GENERATED_TEXTURES_PENDING.swap(false, Ordering::AcqRel) {
        return;
    }
    GENERATED_TEXTURES
        .write()
        .unwrap()
        .drain_pending(&mut visit);
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct GeneratedTexturePendingBench {
    registry: GeneratedTextureRegistry<u64>,
}

#[cfg(feature = "bench-support")]
impl GeneratedTexturePendingBench {
    #[must_use]
    pub fn new(keys: &[String]) -> Self {
        let mut registry = GeneratedTextureRegistry::default();
        for (index, key) in keys.iter().enumerate() {
            registry.register(key, index as u64);
        }
        drop(registry.take_pending_keys());
        Self { registry }
    }

    #[must_use]
    pub fn update_and_drain(&mut self, keys: &[String], indices: &[usize]) -> Vec<Arc<str>> {
        for &index in indices {
            self.registry.register(&keys[index], index as u64);
        }
        self.registry.take_pending_keys()
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct GeneratedTextureDeliveryBench {
    registry: RwLock<GeneratedTextureRegistry<u64>>,
}

#[cfg(feature = "bench-support")]
impl GeneratedTextureDeliveryBench {
    #[must_use]
    pub fn new(keys: &[String]) -> Self {
        let mut registry = GeneratedTextureRegistry::default();
        for (index, key) in keys.iter().enumerate() {
            registry.register(key, index as u64);
        }
        drop(registry.take_pending_keys());
        Self {
            registry: RwLock::new(registry),
        }
    }

    #[must_use]
    pub fn update_and_fetch_reference(&self, keys: &[String], indices: &[usize]) -> u64 {
        {
            let mut registry = self.registry.write().unwrap();
            for &index in indices {
                registry.register(&keys[index], index as u64);
            }
        }
        let pending = self.registry.write().unwrap().take_pending_keys();
        pending.into_iter().fold(0, |sum, key| {
            let value = self
                .registry
                .read()
                .unwrap()
                .get(&key)
                .copied()
                .expect("pending generated texture remains registered");
            sum ^ value.rotate_left((key.len() % 64) as u32)
        })
    }

    #[must_use]
    pub fn update_and_deliver(&self, keys: &[String], indices: &[usize]) -> u64 {
        let mut registry = self.registry.write().unwrap();
        for &index in indices {
            registry.register(&keys[index], index as u64);
        }
        let mut checksum = 0;
        registry.drain_pending(|key, value| {
            checksum ^= value.rotate_left((key.len() % 64) as u32);
        });
        checksum
    }
}

#[cfg(feature = "bench-support")]
struct OwnedGeneratedTextureEntry {
    value: u64,
    pending: bool,
}

#[cfg(feature = "bench-support")]
#[derive(Default)]
struct OwnedGeneratedTextureRegistry {
    entries: FxHashMap<String, OwnedGeneratedTextureEntry>,
    pending_keys: Vec<String>,
}

#[cfg(feature = "bench-support")]
impl OwnedGeneratedTextureRegistry {
    fn register(&mut self, key: &str, value: u64) {
        if let Some(entry) = self.entries.get_mut(key) {
            entry.value = value;
            if !entry.pending {
                entry.pending = true;
                self.pending_keys.push(key.to_owned());
            }
            return;
        }
        let key = key.to_owned();
        self.pending_keys.push(key.clone());
        self.entries.insert(
            key,
            OwnedGeneratedTextureEntry {
                value,
                pending: true,
            },
        );
    }

    fn drain_pending(&mut self) -> u64 {
        let mut pending = std::mem::take(&mut self.pending_keys);
        let mut checksum = 0;
        for key in pending.drain(..) {
            let entry = self
                .entries
                .get_mut(&key)
                .expect("queued benchmark texture remains registered");
            entry.pending = false;
            checksum ^= entry.value.rotate_left((key.len() % 64) as u32);
        }
        self.pending_keys = pending;
        checksum
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct GeneratedTextureOwnedKeyBench {
    registry: RwLock<OwnedGeneratedTextureRegistry>,
}

#[cfg(feature = "bench-support")]
impl GeneratedTextureOwnedKeyBench {
    #[must_use]
    pub fn new(keys: &[String]) -> Self {
        let mut registry = OwnedGeneratedTextureRegistry::default();
        for (index, key) in keys.iter().enumerate() {
            registry.register(key, index as u64);
        }
        registry.drain_pending();
        Self {
            registry: RwLock::new(registry),
        }
    }

    #[must_use]
    pub fn update_and_deliver(&self, keys: &[String], indices: &[usize]) -> u64 {
        let mut registry = self.registry.write().unwrap();
        for &index in indices {
            registry.register(&keys[index], index as u64);
        }
        registry.drain_pending()
    }
}

#[cfg(feature = "bench-support")]
#[must_use]
pub fn texture_key_ownership_reference(keys: &[String]) -> u64 {
    let mut registry = FxHashMap::<String, TextureHandle>::default();
    let mut metadata = FxHashMap::<String, TextureHandle>::default();
    let mut store = FxHashMap::<Arc<str>, TextureHandle>::default();
    for (index, input) in keys.iter().enumerate() {
        let key = input.clone();
        let handle = index as TextureHandle + 1;
        registry.insert(key.clone(), handle);
        metadata.insert(key.clone(), handle);
        store.insert(Arc::from(key), handle);
    }
    registry
        .iter()
        .fold(registry.len() as u64, |sum, (key, handle)| {
            sum.wrapping_add(key.len() as u64).wrapping_add(*handle)
        })
        ^ (metadata.len() as u64).rotate_left(7)
        ^ store.len() as u64
}

#[cfg(feature = "bench-support")]
#[must_use]
pub fn texture_key_ownership_shared(keys: &[String]) -> u64 {
    let mut registry = FxHashMap::<Arc<str>, TextureHandle>::default();
    let mut metadata = FxHashMap::<Arc<str>, TextureHandle>::default();
    let mut store = FxHashMap::<Arc<str>, TextureHandle>::default();
    for (index, input) in keys.iter().enumerate() {
        let key: Arc<str> = Arc::from(input.clone());
        let handle = index as TextureHandle + 1;
        registry.insert(Arc::clone(&key), handle);
        metadata.insert(Arc::clone(&key), handle);
        store.insert(key, handle);
    }
    registry
        .iter()
        .fold(registry.len() as u64, |sum, (key, handle)| {
            sum.wrapping_add(key.len() as u64).wrapping_add(*handle)
        })
        ^ (metadata.len() as u64).rotate_left(7)
        ^ store.len() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_alias_removal_updates_reference_count_without_rebuild() {
        let mut handles = FxHashMap::default();
        handles.insert(Arc::from("Banner.png"), 17);
        handles.insert(Arc::from("banner.PNG"), 17);
        let mut aliases = FastU64Map::default();
        note_texture_handle_alias(&mut aliases, "Banner.png", 17);
        note_texture_handle_alias(&mut aliases, "banner.PNG", 17);

        handles.remove("Banner.png");
        remove_texture_handle_alias(&handles, &mut aliases, "Banner.png");

        let alias = aliases.get(&ascii_ci_hash("banner.png")).unwrap();
        assert_eq!(alias.handle, 17);
        assert_eq!(alias.refs, 1);
    }

    #[test]
    fn colliding_alias_removal_rebuilds_the_surviving_handle() {
        let mut handles = FxHashMap::default();
        handles.insert(Arc::from("Banner.png"), 17);
        handles.insert(Arc::from("banner.PNG"), 23);
        let mut aliases = FastU64Map::default();
        rebuild_texture_handle_aliases(&handles, &mut aliases);
        assert_eq!(
            aliases.get(&ascii_ci_hash("banner.png")).unwrap().handle,
            INVALID_TEXTURE_HANDLE
        );

        handles.remove("Banner.png");
        remove_texture_handle_alias(&handles, &mut aliases, "Banner.png");

        let alias = aliases.get(&ascii_ci_hash("banner.png")).unwrap();
        assert_eq!(alias.handle, 23);
        assert_eq!(alias.refs, 1);
    }

    #[test]
    fn texture_handle_lookup_tracks_registry_lifecycle() {
        clear_texture_handles();

        register_texture_handle("Graphics/Banner.png", 17);
        assert_eq!(texture_handle("Graphics/Banner.png"), 17);
        assert_eq!(texture_handle("graphics/banner.png"), 17);

        remove_texture_handle("Graphics/Banner.png");
        assert_eq!(
            texture_handle("graphics/banner.png"),
            deadlib_render_core::INVALID_TEXTURE_HANDLE
        );

        register_texture_handle("Other.png", 23);
        clear_texture_handles();
        assert_eq!(
            texture_handle("other.png"),
            deadlib_render_core::INVALID_TEXTURE_HANDLE
        );
    }

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
