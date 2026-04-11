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
        build: F,
    ) -> (TMeshCacheKey, Arc<[TexturedMeshVertex]>)
    where
        F: FnOnce() -> Arc<[TexturedMeshVertex]>,
    {
        let key = model_cache_key(slot, size, rotation_deg, draw);
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
    }
}

#[inline(always)]
fn hashed_model_cache_key(key: &ModelMeshCacheKey) -> TMeshCacheKey {
    let mut hasher = XxHash64::default();
    key.hash(&mut hasher);
    hasher.finish().max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::parsing::noteskin::{
        ModelAutoRotKey, ModelMesh, ModelTweenSegment, SpriteDefinition, SpriteSource,
    };

    fn test_slot() -> SpriteSlot {
        SpriteSlot {
            def: SpriteDefinition::default(),
            base_rot_sin_cos: [0.0, 1.0],
            source_size: [64, 64],
            source: Arc::new(SpriteSource::Atlas {
                texture_key: Arc::from("test"),
                tex_dims: (64, 64),
            }),
            uv_velocity: [0.0, 0.0],
            uv_offset: [0.0, 0.0],
            note_color_translate: false,
            model: Some(Arc::new(ModelMesh {
                vertices: Arc::from([]),
                bounds: [0.0; 6],
            })),
            model_draw: ModelDrawState::default(),
            model_timeline: Arc::<[ModelTweenSegment]>::from([]),
            model_effect: crate::engine::present::anim::EffectState::default(),
            model_auto_rot_total_frames: 0.0,
            model_auto_rot_z_keys: Arc::<[ModelAutoRotKey]>::from([]),
        }
    }

    #[test]
    fn cached_geometry_reuses_key_across_tints() {
        let slot = test_slot();
        let size = [48.0, 64.0];
        let draw = ModelDrawState::default();
        let mut cache = ModelMeshCache::default();
        let mut builds = 0usize;

        let (key_a, verts_a) = cache.get_or_insert_with(&slot, size, 15.0, draw, || {
            builds += 1;
            Arc::from(vec![TexturedMeshVertex::default()])
        });
        let (key_b, verts_b) = cache.get_or_insert_with(&slot, size, 15.0, draw, || {
            builds += 1;
            Arc::from(vec![TexturedMeshVertex::default()])
        });

        assert_eq!(builds, 1);
        assert_eq!(key_a, key_b);
        assert!(Arc::ptr_eq(&verts_a, &verts_b));
    }
}
