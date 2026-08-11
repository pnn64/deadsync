use crate::{ascii_ci_hash, parse_sprite_sheet_dims};
use deadlib_render::{FastU64Map, INVALID_TEXTURE_HANDLE, SamplerDesc, TextureHandle};
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
    entries: FxHashMap<String, RegisteredTextureMeta>,
}

impl TextureMetadataRegistry {
    fn register(&mut self, key: &str, texture: TexMeta, sheet: (u32, u32)) -> bool {
        if let Some(entry) = self.entries.get_mut(key) {
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
                key.to_string(),
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

    fn insert_sheet_dims(&mut self, key: &str, sheet: (u32, u32)) -> (u32, u32) {
        self.entries
            .entry(key.to_string())
            .or_insert(RegisteredTextureMeta {
                texture: None,
                sheet,
            })
            .sheet
    }
}

struct GeneratedTextureEntry<T> {
    value: T,
    pending: bool,
}

struct GeneratedTextureRegistry<T> {
    entries: FxHashMap<String, GeneratedTextureEntry<T>>,
    pending: usize,
}

impl<T> Default for GeneratedTextureRegistry<T> {
    fn default() -> Self {
        Self {
            entries: FxHashMap::default(),
            pending: 0,
        }
    }
}

impl<T> GeneratedTextureRegistry<T> {
    fn register(&mut self, key: &str, value: T) {
        if let Some(entry) = self.entries.get_mut(key) {
            entry.value = value;
            if !entry.pending {
                entry.pending = true;
                self.pending += 1;
            }
            return;
        }
        self.entries.insert(
            key.to_string(),
            GeneratedTextureEntry {
                value,
                pending: true,
            },
        );
        self.pending += 1;
    }

    fn get(&self, key: &str) -> Option<&T> {
        self.entries.get(key).map(|entry| &entry.value)
    }

    fn take_pending_keys(&mut self) -> Vec<String> {
        if self.pending == 0 {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(self.pending);
        for (key, entry) in &mut self.entries {
            if entry.pending {
                entry.pending = false;
                out.push(key.clone());
            }
        }
        self.pending = 0;
        out
    }
}

static TEXTURE_METADATA: LazyLock<RwLock<TextureMetadataRegistry>> =
    LazyLock::new(|| RwLock::new(TextureMetadataRegistry::default()));

static TEXTURE_HANDLES: LazyLock<RwLock<FxHashMap<String, TextureHandle>>> =
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
    handles: &FxHashMap<String, TextureHandle>,
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
    handles: &FxHashMap<String, TextureHandle>,
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

pub fn register_texture_handle(key: &str, handle: TextureHandle) {
    let mut handles = TEXTURE_HANDLES.write().unwrap();
    let mut aliases = TEXTURE_HANDLE_ALIASES.write().unwrap();
    if let Some((owned_key, old)) = handles.remove_entry(key) {
        if old == handle {
            handles.insert(owned_key, old);
            return;
        }
        remove_texture_handle_alias(&handles, &mut aliases, key);
        handles.insert(owned_key, handle);
        note_texture_handle_alias(&mut aliases, key, handle);
        touch_texture_registry();
    } else {
        handles.insert(key.to_string(), handle);
        note_texture_handle_alias(&mut aliases, key, handle);
        touch_texture_registry();
    }
}

pub fn remove_texture_handle(key: &str) {
    let mut handles = TEXTURE_HANDLES.write().unwrap();
    if handles.remove(key).is_none() {
        return;
    }
    let mut aliases = TEXTURE_HANDLE_ALIASES.write().unwrap();
    remove_texture_handle_alias(&handles, &mut aliases, key);
    touch_texture_registry();
}

pub fn clear_texture_handles() {
    TEXTURE_HANDLES.write().unwrap().clear();
    TEXTURE_HANDLE_ALIASES.write().unwrap().clear();
    touch_texture_registry();
}

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

pub fn texture_dims(key: &str) -> Option<TexMeta> {
    TEXTURE_METADATA.read().unwrap().texture_dims(key)
}

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

pub fn generated_texture(key: &str) -> Option<GeneratedTexture> {
    GENERATED_TEXTURES.read().unwrap().get(key).cloned()
}

pub fn take_pending_generated_texture_keys() -> Vec<String> {
    if !GENERATED_TEXTURES_PENDING.swap(false, Ordering::AcqRel) {
        return Vec::new();
    }
    GENERATED_TEXTURES.write().unwrap().take_pending_keys()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_alias_removal_updates_reference_count_without_rebuild() {
        let mut handles = FxHashMap::default();
        handles.insert("Banner.png".to_string(), 17);
        handles.insert("banner.PNG".to_string(), 17);
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
        handles.insert("Banner.png".to_string(), 17);
        handles.insert("banner.PNG".to_string(), 23);
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
            deadlib_render::INVALID_TEXTURE_HANDLE
        );

        register_texture_handle("Other.png", 23);
        clear_texture_handles();
        assert_eq!(
            texture_handle("other.png"),
            deadlib_render::INVALID_TEXTURE_HANDLE
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
        assert_eq!(registry.take_pending_keys(), ["generated/lifebar"]);
        assert!(registry.take_pending_keys().is_empty());

        registry.register("generated/lifebar", 3);
        assert_eq!(registry.take_pending_keys(), ["generated/lifebar"]);
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

        let pending = take_pending_generated_texture_keys();
        assert!(
            pending
                .iter()
                .any(|key| key == "generated/pending-gate-test")
        );
        assert!(take_pending_generated_texture_keys().is_empty());
    }
}
