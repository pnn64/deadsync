use crate::{ascii_ci_hash, parse_sprite_sheet_dims};
use deadlib_render::{FastU64Map, INVALID_TEXTURE_HANDLE, SamplerDesc, TextureHandle};
use image::RgbaImage;
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, LazyLock, Mutex, RwLock,
        atomic::{AtomicU64, Ordering},
    },
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

static TEX_META: LazyLock<RwLock<HashMap<String, TexMeta>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

static SHEET_DIMS: LazyLock<RwLock<HashMap<String, (u32, u32)>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

static TEXTURE_HANDLES: LazyLock<RwLock<HashMap<String, TextureHandle>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

static TEXTURE_HANDLE_ALIASES: LazyLock<RwLock<FastU64Map<TextureHandle>>> =
    LazyLock::new(|| RwLock::new(FastU64Map::default()));

static GENERATED_TEXTURES: LazyLock<RwLock<HashMap<String, GeneratedTexture>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));
static GENERATED_TEXTURES_PENDING: LazyLock<Mutex<HashSet<String>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));
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
    handles: &HashMap<String, TextureHandle>,
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
    let same_meta = TEX_META
        .read()
        .unwrap()
        .get(key)
        .is_some_and(|meta| meta.w == w && meta.h == h);
    if same_meta && SHEET_DIMS.read().unwrap().get(key).copied() == Some(sheet) {
        return;
    }

    let key = key.to_string();
    let mut m = TEX_META.write().unwrap();
    m.insert(key.clone(), TexMeta { w, h });
    drop(m);
    SHEET_DIMS.write().unwrap().insert(key, sheet);
    touch_texture_registry();
}

pub fn texture_dims(key: &str) -> Option<TexMeta> {
    TEX_META.read().unwrap().get(key).copied()
}

pub fn sprite_sheet_dims(key: &str) -> (u32, u32) {
    if let Some(dims) = SHEET_DIMS.read().unwrap().get(key).copied() {
        return dims;
    }
    let dims = parse_sprite_sheet_dims(key);
    SHEET_DIMS.write().unwrap().insert(key.to_string(), dims);
    dims
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
    GENERATED_TEXTURES.write().unwrap().insert(
        key.to_string(),
        GeneratedTexture {
            image: Arc::new(image),
            sampler,
        },
    );
    GENERATED_TEXTURES_PENDING
        .lock()
        .unwrap()
        .insert(key.to_string());
    register_texture_dims(key, w, h);
}

pub fn generated_texture(key: &str) -> Option<GeneratedTexture> {
    GENERATED_TEXTURES.read().unwrap().get(key).cloned()
}

pub fn take_pending_generated_texture_keys() -> Vec<String> {
    let mut pending = GENERATED_TEXTURES_PENDING.lock().unwrap();
    if pending.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(pending.len());
    out.extend(pending.drain());
    out
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
}
