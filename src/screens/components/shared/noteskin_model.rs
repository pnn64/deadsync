use crate::core::gfx::{BlendMode, MeshMode, TexturedMeshVertex};
use crate::game::parsing::noteskin::{ModelDrawState, ModelMesh, SpriteSlot};
use crate::ui::actors::{Actor, SizeSpec};
use std::collections::HashMap;
use std::hash::BuildHasherDefault;
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
    fn get_or_insert_with<F>(
        &mut self,
        key: ModelMeshCacheKey,
        build: F,
    ) -> Arc<[TexturedMeshVertex]>
    where
        F: FnOnce() -> Arc<[TexturedMeshVertex]>,
    {
        if let Some(vertices) = self.entries.get(&key) {
            return vertices.clone();
        }
        let vertices = build();
        if self.entries.len() < MODEL_MESH_CACHE_LIMIT {
            self.entries.insert(key, vertices.clone());
        }
        vertices
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
fn model_uv_params(slot: &SpriteSlot, uv_rect: [f32; 4]) -> ([f32; 2], [f32; 2], [f32; 2]) {
    let uv_scale = [uv_rect[2] - uv_rect[0], uv_rect[3] - uv_rect[1]];
    let uv_offset = [uv_rect[0], uv_rect[1]];
    let uv_tex_shift = match slot.source.as_ref() {
        crate::game::parsing::noteskin::SpriteSource::Atlas { tex_dims, .. } => {
            let tw = tex_dims.0.max(1) as f32;
            let th = tex_dims.1.max(1) as f32;
            let base_u0 = slot.def.src[0] as f32 / tw;
            let base_v0 = slot.def.src[1] as f32 / th;
            [uv_offset[0] - base_u0, uv_offset[1] - base_v0]
        }
        crate::game::parsing::noteskin::SpriteSource::Animated { .. } => [0.0, 0.0],
    };
    (uv_scale, uv_offset, uv_tex_shift)
}

#[inline(always)]
fn model_tint(color: [f32; 4], draw: ModelDrawState) -> [f32; 4] {
    [
        color[0] * draw.tint[0],
        color[1] * draw.tint[1],
        color[2] * draw.tint[2],
        color[3] * draw.tint[3],
    ]
}

#[inline(always)]
const fn model_blend(draw: ModelDrawState, blend: BlendMode) -> BlendMode {
    if draw.blend_add {
        BlendMode::Add
    } else {
        blend
    }
}

#[inline(always)]
fn build_model_vertices(
    slot: &SpriteSlot,
    model: &ModelMesh,
    size: [f32; 2],
    rotation_deg: f32,
    draw: ModelDrawState,
    tint: [f32; 4],
) -> Arc<[TexturedMeshVertex]> {
    let model_size = model.size();
    let model_h = model_size[1];
    let scale = if model_h > f32::EPSILON && size[1] > f32::EPSILON {
        size[1] / model_h
    } else {
        1.0
    };
    let zoom = [
        draw.zoom[0].max(0.0),
        draw.zoom[1].max(0.0),
        draw.zoom[2].max(0.0),
    ];
    let local_scale = [scale * zoom[0], scale * zoom[1], scale * zoom[2]];
    let rx = draw.rot[0].to_radians();
    let ry = draw.rot[1].to_radians();
    let rz = (draw.rot[2] + rotation_deg).to_radians();
    let (sin_x, cos_x) = rx.sin_cos();
    let (sin_y, cos_y) = ry.sin_cos();
    let (sin_z, cos_z) = rz.sin_cos();
    let tx = draw.pos[0] * scale;
    let ty = draw.pos[1] * scale;
    let tz = draw.pos[2] * scale;
    let focal = model_size[0]
        .max(model_size[1])
        .mul_add(6.0, 0.0)
        .max(180.0);
    let align_y = (0.5 - draw.vert_align) * size[1];

    let mut vertices = Vec::with_capacity(model.vertices.len());
    for v in model.vertices.iter() {
        let mut lx = v.pos[0] * local_scale[0];
        let mut ly = v.pos[1] * local_scale[1] + align_y;
        let lz = v.pos[2] * local_scale[2];
        if slot.def.mirror_h {
            lx = -lx;
        }
        if slot.def.mirror_v {
            ly = -ly;
        }

        let x1 = lx;
        let y1 = ly.mul_add(cos_x, -lz * sin_x);
        let z1 = ly.mul_add(sin_x, lz * cos_x);

        let x2 = x1.mul_add(cos_y, z1 * sin_y);
        let y2 = y1;
        let z2 = z1.mul_add(cos_y, -x1 * sin_y);

        let x3 = x2.mul_add(cos_z, -y2 * sin_z) + tx;
        let y3 = x2.mul_add(sin_z, y2 * cos_z) + ty;
        let y_screen = -y3;
        let z3 = z2 + tz;
        let perspective = focal / (focal - z3).max(1.0);
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
            pos: [x3 * perspective, y_screen * perspective],
            uv: [u, v_tex],
            tex_matrix_scale: v.tex_matrix_scale,
            color: tint,
        });
    }
    Arc::from(vertices)
}

#[inline(always)]
fn actor_from_vertices(
    slot: &SpriteSlot,
    xy: [f32; 2],
    vertices: Arc<[TexturedMeshVertex]>,
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    uv_tex_shift: [f32; 2],
    blend: BlendMode,
    z: i16,
) -> Actor {
    Actor::TexturedMesh {
        align: [0.0, 0.0],
        offset: xy,
        world_z: 0.0,
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        texture: slot.texture_key_shared(),
        vertices,
        mode: MeshMode::Triangles,
        uv_scale,
        uv_offset,
        uv_tex_shift,
        visible: true,
        blend,
        z,
    }
}

#[inline(always)]
fn actor_from_draw(
    slot: &SpriteSlot,
    draw: ModelDrawState,
    xy: [f32; 2],
    size: [f32; 2],
    uv_rect: [f32; 4],
    rotation_deg: f32,
    color: [f32; 4],
    blend: BlendMode,
    z: i16,
) -> Option<Actor> {
    let model = slot.model.as_ref()?;
    if !draw.visible || model.vertices.is_empty() {
        return None;
    }

    let tint = model_tint(color, draw);
    let blend = model_blend(draw, blend);
    let vertices = build_model_vertices(slot, model, size, rotation_deg, draw, tint);
    let (uv_scale, uv_offset, uv_tex_shift) = model_uv_params(slot, uv_rect);
    Some(actor_from_vertices(
        slot,
        xy,
        vertices,
        uv_scale,
        uv_offset,
        uv_tex_shift,
        blend,
        z,
    ))
}

#[inline(always)]
pub(crate) fn noteskin_model_actor_from_draw_cached(
    slot: &SpriteSlot,
    draw: ModelDrawState,
    xy: [f32; 2],
    size: [f32; 2],
    uv_rect: [f32; 4],
    rotation_deg: f32,
    color: [f32; 4],
    blend: BlendMode,
    z: i16,
    cache: &mut ModelMeshCache,
) -> Option<Actor> {
    let model = slot.model.as_ref()?;
    if !draw.visible || model.vertices.is_empty() {
        return None;
    }

    let tint = model_tint(color, draw);
    let key = model_cache_key(slot, size, rotation_deg, draw, tint);
    let vertices = cache.get_or_insert_with(key, || {
        build_model_vertices(slot, model, size, rotation_deg, draw, tint)
    });
    let (uv_scale, uv_offset, uv_tex_shift) = model_uv_params(slot, uv_rect);
    Some(actor_from_vertices(
        slot,
        xy,
        vertices,
        uv_scale,
        uv_offset,
        uv_tex_shift,
        model_blend(draw, blend),
        z,
    ))
}

#[inline(always)]
pub(crate) fn noteskin_model_actor(
    slot: &SpriteSlot,
    xy: [f32; 2],
    size: [f32; 2],
    uv_rect: [f32; 4],
    rotation_deg: f32,
    elapsed: f32,
    beat: f32,
    color: [f32; 4],
    blend: BlendMode,
    z: i16,
) -> Option<Actor> {
    let draw = slot.model_draw_at(elapsed, beat);
    actor_from_draw(slot, draw, xy, size, uv_rect, rotation_deg, color, blend, z)
}
