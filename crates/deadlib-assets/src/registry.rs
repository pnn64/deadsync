use crate::{ascii_ci_hash, parse_sprite_sheet_dims};
use deadlib_render::{FastU64Map, INVALID_TEXTURE_HANDLE, SamplerDesc, TextureHandle};
use image::RgbaImage;
use rustc_hash::FxHashMap;
use std::sync::{
    Arc, LazyLock, RwLock,
    atomic::{AtomicU64, Ordering},
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

static TEXTURE_HANDLE_ALIASES: LazyLock<RwLock<FastU64Map<TextureHandle>>> =
    LazyLock::new(|| RwLock::new(FastU64Map::default()));

static GENERATED_TEXTURES: LazyLock<RwLock<GeneratedTextureRegistry<GeneratedTexture>>> =
    LazyLock::new(|| RwLock::new(GeneratedTextureRegistry::default()));
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
    aliases: &mut FastU64Map<TextureHandle>,
    key: &str,
    handle: TextureHandle,
) {
    let folded = ascii_ci_hash(key);
    match aliases.get_mut(&folded) {
        Some(existing) if *existing != handle => *existing = INVALID_TEXTURE_HANDLE,
        Some(_) => {}
        None => {
            aliases.insert(folded, handle);
        }
    }
}

fn rebuild_texture_handle_aliases(
    handles: &FxHashMap<String, TextureHandle>,
    aliases: &mut FastU64Map<TextureHandle>,
) {
    aliases.clear();
    aliases.reserve(handles.len());
    for (key, &handle) in handles {
        note_texture_handle_alias(aliases, key, handle);
    }
}

pub fn register_texture_handle(key: &str, handle: TextureHandle) {
    let mut handles = TEXTURE_HANDLES.write().unwrap();
    let mut aliases = TEXTURE_HANDLE_ALIASES.write().unwrap();
    let replaced = handles.insert(key.to_string(), handle);
    if replaced.is_some_and(|old| old != handle) {
        rebuild_texture_handle_aliases(&handles, &mut aliases);
        touch_texture_registry();
    } else if replaced.is_none() {
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
    rebuild_texture_handle_aliases(&handles, &mut aliases);
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
        .copied()
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
    register_texture_dims(key, w, h);
}

pub fn generated_texture(key: &str) -> Option<GeneratedTexture> {
    GENERATED_TEXTURES.read().unwrap().get(key).cloned()
}

pub fn take_pending_generated_texture_keys() -> Vec<String> {
    GENERATED_TEXTURES.write().unwrap().take_pending_keys()
}

#[cfg(any(test, feature = "bench-support"))]
pub fn texture_metadata_workload_for_bench(
    keys: &[&str],
    replacements: usize,
    lookup_rounds: usize,
) -> u64 {
    let mut registry = TextureMetadataRegistry::default();
    for &key in keys.iter().step_by(4) {
        let sheet = parse_sprite_sheet_dims(key);
        registry.insert_sheet_dims(key, sheet);
    }
    for replacement in 0..=replacements {
        for (index, &key) in keys.iter().enumerate() {
            let (w, h) = benchmark_texture_dims(index, replacement);
            registry.register(key, TexMeta { w, h }, parse_sprite_sheet_dims(key));
        }
    }

    let mut checksum = registry.entries.len() as u64;
    for round in 0..lookup_rounds {
        for (index, &key) in keys.iter().enumerate() {
            let meta = registry.texture_dims(key).unwrap_or(TexMeta { w: 0, h: 0 });
            let sheet = registry.sheet_dims(key).unwrap_or_default();
            checksum = checksum.wrapping_add(
                u64::from(meta.w)
                    ^ u64::from(meta.h).rotate_left(11)
                    ^ u64::from(sheet.0).rotate_left(23)
                    ^ u64::from(sheet.1).rotate_left(37)
                    ^ (index as u64).rotate_left((round & 31) as u32),
            );
        }
    }
    checksum
}

#[cfg(any(test, feature = "bench-support"))]
pub fn texture_metadata_workload_legacy_for_bench(
    keys: &[&str],
    replacements: usize,
    lookup_rounds: usize,
) -> u64 {
    use std::collections::HashMap;

    let mut textures = HashMap::<String, TexMeta>::new();
    let mut sheets = HashMap::<String, (u32, u32)>::new();
    for &key in keys.iter().step_by(4) {
        sheets.insert(key.to_string(), parse_sprite_sheet_dims(key));
    }
    for replacement in 0..=replacements {
        for (index, &key) in keys.iter().enumerate() {
            let (w, h) = benchmark_texture_dims(index, replacement);
            let sheet = parse_sprite_sheet_dims(key);
            let same_meta = textures
                .get(key)
                .is_some_and(|meta| meta.w == w && meta.h == h);
            if same_meta && sheets.get(key).copied() == Some(sheet) {
                continue;
            }
            let key = key.to_string();
            textures.insert(key.clone(), TexMeta { w, h });
            sheets.insert(key, sheet);
        }
    }

    let mut checksum = textures.len() as u64;
    for round in 0..lookup_rounds {
        for (index, &key) in keys.iter().enumerate() {
            let meta = textures.get(key).copied().unwrap_or(TexMeta { w: 0, h: 0 });
            let sheet = sheets.get(key).copied().unwrap_or_default();
            checksum = checksum.wrapping_add(
                u64::from(meta.w)
                    ^ u64::from(meta.h).rotate_left(11)
                    ^ u64::from(sheet.0).rotate_left(23)
                    ^ u64::from(sheet.1).rotate_left(37)
                    ^ (index as u64).rotate_left((round & 31) as u32),
            );
        }
    }
    checksum
}

#[cfg(any(test, feature = "bench-support"))]
fn benchmark_texture_dims(index: usize, replacement: usize) -> (u32, u32) {
    (
        64 + ((index + replacement) % 17) as u32,
        32 + ((index * 3 + replacement) % 13) as u32,
    )
}

#[cfg(any(test, feature = "bench-support"))]
pub fn generated_texture_workload_for_bench(
    keys: &[&str],
    replacements: usize,
    lookup_rounds: usize,
) -> u64 {
    let mut registry = GeneratedTextureRegistry::<u64>::default();
    for replacement in 0..=replacements {
        for (index, &key) in keys.iter().enumerate() {
            registry.register(key, benchmark_generated_value(index, replacement));
        }
    }

    let mut checksum = generated_texture_lookup_checksum(&registry, keys, lookup_rounds);
    checksum = checksum.wrapping_add(pending_key_checksum(registry.take_pending_keys()));
    for (index, &key) in keys.iter().enumerate().step_by(2) {
        registry.register(key, benchmark_generated_value(index, replacements + 1));
    }
    checksum.wrapping_add(pending_key_checksum(registry.take_pending_keys()))
}

#[cfg(any(test, feature = "bench-support"))]
pub fn generated_texture_workload_legacy_for_bench(
    keys: &[&str],
    replacements: usize,
    lookup_rounds: usize,
) -> u64 {
    use std::collections::{HashMap, HashSet};

    let mut entries = HashMap::<String, u64>::new();
    let mut pending = HashSet::<String>::new();
    for replacement in 0..=replacements {
        for (index, &key) in keys.iter().enumerate() {
            entries.insert(
                key.to_string(),
                benchmark_generated_value(index, replacement),
            );
            pending.insert(key.to_string());
        }
    }

    let mut checksum = entries.len() as u64;
    for round in 0..lookup_rounds {
        for (index, &key) in keys.iter().enumerate() {
            checksum = checksum.wrapping_add(
                entries.get(key).copied().unwrap_or_default()
                    ^ (index as u64).rotate_left((round & 31) as u32),
            );
        }
    }
    checksum = checksum.wrapping_add(pending_key_checksum(pending.drain()));
    for (index, &key) in keys.iter().enumerate().step_by(2) {
        entries.insert(
            key.to_string(),
            benchmark_generated_value(index, replacements + 1),
        );
        pending.insert(key.to_string());
    }
    checksum.wrapping_add(pending_key_checksum(pending.drain()))
}

#[cfg(any(test, feature = "bench-support"))]
fn generated_texture_lookup_checksum(
    registry: &GeneratedTextureRegistry<u64>,
    keys: &[&str],
    lookup_rounds: usize,
) -> u64 {
    let mut checksum = registry.entries.len() as u64;
    for round in 0..lookup_rounds {
        for (index, &key) in keys.iter().enumerate() {
            checksum = checksum.wrapping_add(
                registry.get(key).copied().unwrap_or_default()
                    ^ (index as u64).rotate_left((round & 31) as u32),
            );
        }
    }
    checksum
}

#[cfg(any(test, feature = "bench-support"))]
fn benchmark_generated_value(index: usize, replacement: usize) -> u64 {
    ((index as u64) << 32) | replacement as u64
}

#[cfg(any(test, feature = "bench-support"))]
fn pending_key_checksum(keys: impl IntoIterator<Item = String>) -> u64 {
    keys.into_iter().fold(0, |checksum, key| {
        checksum.wrapping_add(ascii_ci_hash(&key))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn optimized_registry_workloads_match_legacy_behavior() {
        let keys = [
            "graphics/banner.png",
            "grades/grades 1x19.png",
            "noteskins/dance/tap note 4x1 (doubleres).png",
            "generated/player-1/lifebar",
            "mixed/CaSe/Texture.PNG",
        ];

        assert_eq!(
            texture_metadata_workload_for_bench(&keys, 4, 7),
            texture_metadata_workload_legacy_for_bench(&keys, 4, 7)
        );
        assert_eq!(
            generated_texture_workload_for_bench(&keys, 4, 7),
            generated_texture_workload_legacy_for_bench(&keys, 4, 7)
        );
    }
}
