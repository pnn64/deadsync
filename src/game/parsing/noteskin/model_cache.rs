use super::SpriteSlot;
use crate::engine::gfx::{TMeshCacheKey, TexturedMeshVertex};
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hash, Hasher};
use std::sync::Arc;
use twox_hash::XxHash64;

const MODEL_MESH_CACHE_LIMIT: usize = 512;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct ModelMeshCacheKey {
    slot: *const SpriteSlot,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModelMeshCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub saturated_misses: u64,
}

pub(crate) struct ModelMeshCache {
    entries: HashMap<ModelMeshCacheKey, Arc<[TexturedMeshVertex]>, BuildHasherDefault<XxHash64>>,
    stats: ModelMeshCacheStats,
}

impl Default for ModelMeshCache {
    fn default() -> Self {
        Self::with_capacity(0)
    }
}

impl ModelMeshCache {
    #[inline(always)]
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: HashMap::with_capacity_and_hasher(capacity, BuildHasherDefault::default()),
            stats: ModelMeshCacheStats::default(),
        }
    }

    #[inline(always)]
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    #[inline(always)]
    pub(crate) const fn stats(&self) -> ModelMeshCacheStats {
        self.stats
    }

    #[inline(always)]
    pub(crate) fn reset_stats(&mut self) {
        self.stats = ModelMeshCacheStats::default();
    }

    #[inline(always)]
    pub(crate) fn prewarm_slot(&mut self, slot: &SpriteSlot) {
        if slot.model.is_none() {
            return;
        }
        let _ = self.get_or_insert_with(slot, || build_model_geometry(slot));
    }

    #[inline(always)]
    pub(crate) fn get_or_insert_slot(
        &mut self,
        slot: &SpriteSlot,
    ) -> Option<(TMeshCacheKey, Arc<[TexturedMeshVertex]>)> {
        slot.model
            .as_ref()
            .map(|_| self.get_or_insert_with(slot, || build_model_geometry(slot)))
    }

    #[inline(always)]
    pub(crate) fn get_or_insert_with<F>(
        &mut self,
        slot: &SpriteSlot,
        build: F,
    ) -> (TMeshCacheKey, Arc<[TexturedMeshVertex]>)
    where
        F: FnOnce() -> Arc<[TexturedMeshVertex]>,
    {
        let key = model_cache_key(slot);
        let geom_cache_key = hashed_model_cache_key(&key);
        if let Some(vertices) = self.entries.get(&key) {
            self.stats.hits = self.stats.hits.saturating_add(1);
            return (geom_cache_key, vertices.clone());
        }
        self.stats.misses = self.stats.misses.saturating_add(1);
        let vertices = build();
        if self.entries.len() < MODEL_MESH_CACHE_LIMIT {
            self.entries.insert(key, vertices.clone());
        } else {
            self.stats.saturated_misses = self.stats.saturated_misses.saturating_add(1);
        }
        (geom_cache_key, vertices)
    }
}

#[inline(always)]
pub(crate) fn build_model_geometry(slot: &SpriteSlot) -> Arc<[TexturedMeshVertex]> {
    let model = slot
        .model
        .as_ref()
        .expect("model geometry requested for non-model noteskin slot");
    let mut vertices = Vec::with_capacity(model.vertices.len());
    for v in model.vertices.iter() {
        let mut pos = v.pos;
        if slot.def.mirror_h {
            pos[0] = -pos[0];
        }
        if slot.def.mirror_v {
            pos[1] = -pos[1];
        }
        let u = if slot.def.mirror_h {
            1.0 - v.uv[0]
        } else {
            v.uv[0]
        };
        let v_tex = if slot.def.mirror_v {
            1.0 - v.uv[1]
        } else {
            v.uv[1]
        };
        vertices.push(TexturedMeshVertex {
            pos,
            uv: [u, v_tex],
            tex_matrix_scale: v.tex_matrix_scale,
            color: [1.0; 4],
        });
    }
    Arc::from(vertices)
}

#[inline(always)]
fn model_cache_key(slot: &SpriteSlot) -> ModelMeshCacheKey {
    ModelMeshCacheKey {
        slot: slot as *const SpriteSlot,
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
        ModelAutoRotKey, ModelDrawState, ModelMesh, ModelTweenSegment, ModelVertex,
        SpriteDefinition, SpriteSource,
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
                vertices: Arc::from([ModelVertex {
                    pos: [0.0, 0.0, 0.0],
                    uv: [0.0, 0.0],
                    tex_matrix_scale: [1.0, 1.0],
                }]),
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
        let mut cache = ModelMeshCache::default();
        let mut builds = 0usize;

        let (key_a, verts_a) = cache.get_or_insert_with(&slot, || {
            builds += 1;
            Arc::from(vec![TexturedMeshVertex::default()])
        });
        let (key_b, verts_b) = cache.get_or_insert_with(&slot, || {
            builds += 1;
            Arc::from(vec![TexturedMeshVertex::default()])
        });

        assert_eq!(builds, 1);
        assert_eq!(key_a, key_b);
        assert!(Arc::ptr_eq(&verts_a, &verts_b));
        assert_eq!(
            cache.stats(),
            ModelMeshCacheStats {
                hits: 1,
                misses: 1,
                saturated_misses: 0,
            }
        );
    }

    #[test]
    fn cached_geometry_ignores_draw_state_changes() {
        let slot = test_slot();
        let mut cache = ModelMeshCache::default();
        let mut builds = 0usize;
        let draw = ModelDrawState {
            pos: [12.0, -4.0, 1.0],
            rot: [10.0, 20.0, 30.0],
            zoom: [1.5, 0.75, 2.0],
            vert_align: 0.1,
            ..ModelDrawState::default()
        };

        let (_, verts_a) = cache.get_or_insert_with(&slot, || {
            builds += 1;
            Arc::from(vec![TexturedMeshVertex::default()])
        });
        let (_, verts_b) = cache.get_or_insert_with(&slot, || {
            builds += 1;
            Arc::from(vec![TexturedMeshVertex {
                pos: [draw.pos[0], draw.pos[1], draw.pos[2]],
                ..TexturedMeshVertex::default()
            }])
        });

        assert_eq!(builds, 1);
        assert!(Arc::ptr_eq(&verts_a, &verts_b));
    }
}
