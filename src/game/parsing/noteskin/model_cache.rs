use super::{ModelDrawState, SpriteSlot};
use crate::engine::gfx::{TMeshCacheKey, TexturedMeshVertex};
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hash, Hasher};
use std::sync::Arc;
use twox_hash::XxHash64;

const MODEL_MESH_CACHE_LIMIT: usize = 512;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct ModelMeshCacheKey {
    slot: *const SpriteSlot,
    size: [u32; 2],
    rotation: u32,
    pos: [u32; 3],
    rot: [u32; 3],
    zoom: [u32; 3],
    vert_align: u32,
    tint: [u32; 4],
}

#[derive(Default)]
pub(crate) struct ModelMeshCache {
    entries: HashMap<ModelMeshCacheKey, Arc<[TexturedMeshVertex]>, BuildHasherDefault<XxHash64>>,
}

impl ModelMeshCache {
    #[inline(always)]
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: HashMap::with_capacity_and_hasher(capacity, BuildHasherDefault::default()),
        }
    }

    #[inline(always)]
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    #[inline(always)]
    pub(crate) fn get_or_insert_with<F>(
        &mut self,
        slot: &SpriteSlot,
        size: [f32; 2],
        rotation_deg: f32,
        draw: ModelDrawState,
        tint: [f32; 4],
        build: F,
    ) -> (TMeshCacheKey, Arc<[TexturedMeshVertex]>)
    where
        F: FnOnce() -> Arc<[TexturedMeshVertex]>,
    {
        let key = model_cache_key(slot, size, rotation_deg, draw, tint);
        let geom_cache_key = hashed_model_cache_key(&key);
        if let Some(vertices) = self.entries.get(&key) {
            return (geom_cache_key, vertices.clone());
        }
        let vertices = build();
        if self.entries.len() < MODEL_MESH_CACHE_LIMIT {
            self.entries.insert(key, vertices.clone());
        }
        (geom_cache_key, vertices)
    }
}

#[inline(always)]
const fn norm_bits(v: f32) -> u32 {
    if v == 0.0 {
        0.0f32.to_bits()
    } else {
        v.to_bits()
    }
}

#[inline(always)]
fn model_cache_key(
    slot: &SpriteSlot,
    size: [f32; 2],
    rotation_deg: f32,
    draw: ModelDrawState,
    tint: [f32; 4],
) -> ModelMeshCacheKey {
    ModelMeshCacheKey {
        slot: slot as *const SpriteSlot,
        size: [norm_bits(size[0]), norm_bits(size[1])],
        rotation: norm_bits(rotation_deg),
        pos: [
            norm_bits(draw.pos[0]),
            norm_bits(draw.pos[1]),
            norm_bits(draw.pos[2]),
        ],
        rot: [
            norm_bits(draw.rot[0]),
            norm_bits(draw.rot[1]),
            norm_bits(draw.rot[2]),
        ],
        zoom: [
            norm_bits(draw.zoom[0]),
            norm_bits(draw.zoom[1]),
            norm_bits(draw.zoom[2]),
        ],
        vert_align: norm_bits(draw.vert_align),
        tint: [
            norm_bits(tint[0]),
            norm_bits(tint[1]),
            norm_bits(tint[2]),
            norm_bits(tint[3]),
        ],
    }
}

#[inline(always)]
fn hashed_model_cache_key(key: &ModelMeshCacheKey) -> TMeshCacheKey {
    let mut hasher = XxHash64::default();
    key.hash(&mut hasher);
    hasher.finish().max(1)
}
