use crate::engine::gfx::{BlendMode, MeshMode, TexturedMeshVertex};
use crate::engine::present::actors::{Actor, SizeSpec};
use crate::game::parsing::noteskin::{
    ModelDrawState, ModelMesh, ModelMeshCache, SpriteSlot, build_model_geometry,
};
use glam::{Mat4 as Matrix4, Vec3 as Vector3, Vec4};
use std::sync::Arc;

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
fn model_draw_transform(model_size: [f32; 2], affine: Matrix4) -> Matrix4 {
    let focal = model_size[0]
        .max(model_size[1])
        .mul_add(6.0, 0.0)
        .max(180.0);
    let inv_focal = focal.recip();
    Matrix4::from_cols(
        Vec4::new(
            affine.x_axis.x,
            -affine.x_axis.y,
            0.0,
            -affine.x_axis.z * inv_focal,
        ),
        Vec4::new(
            affine.y_axis.x,
            -affine.y_axis.y,
            0.0,
            -affine.y_axis.z * inv_focal,
        ),
        Vec4::new(
            affine.z_axis.x,
            -affine.z_axis.y,
            0.0,
            -affine.z_axis.z * inv_focal,
        ),
        Vec4::new(
            affine.w_axis.x,
            -affine.w_axis.y,
            0.0,
            1.0 - affine.w_axis.z * inv_focal,
        ),
    )
}

#[inline(always)]
fn model_affine_transform(
    model: &ModelMesh,
    size: [f32; 2],
    rotation_deg: f32,
    draw: ModelDrawState,
) -> Matrix4 {
    let model_size = model.size();
    let model_h = model_size[1];
    let scale = if model_h > f32::EPSILON && size[1] > f32::EPSILON {
        size[1] / model_h
    } else {
        1.0
    };
    let local_scale = Vector3::new(
        scale * draw.zoom[0].max(0.0),
        scale * draw.zoom[1].max(0.0),
        scale * draw.zoom[2].max(0.0),
    );
    let align_y = (0.5 - draw.vert_align) * size[1];
    Matrix4::from_translation(Vector3::new(draw.pos[0], draw.pos[1], draw.pos[2]))
        * sm_rotation_xyz(draw.rot[0], draw.rot[1], draw.rot[2] + rotation_deg)
        * Matrix4::from_translation(Vector3::new(0.0, align_y, 0.0))
        * Matrix4::from_scale(local_scale)
}

#[inline(always)]
fn sm_rotation_xyz(rot_x_deg: f32, rot_y_deg: f32, rot_z_deg: f32) -> Matrix4 {
    let (sin_x, cos_x) = rot_x_deg.to_radians().sin_cos();
    let (sin_y, cos_y) = rot_y_deg.to_radians().sin_cos();
    let (sin_z, cos_z) = rot_z_deg.to_radians().sin_cos();
    Matrix4::from_cols(
        Vec4::new(
            cos_z * cos_y,
            cos_z * sin_y * sin_x + sin_z * cos_x,
            cos_z * sin_y * cos_x - sin_z * sin_x,
            0.0,
        ),
        Vec4::new(
            -sin_z * cos_y,
            -sin_z * sin_y * sin_x + cos_z * cos_x,
            -sin_z * sin_y * cos_x - cos_z * sin_x,
            0.0,
        ),
        Vec4::new(-sin_y, cos_y * sin_x, cos_y * cos_x, 0.0),
        Vec4::new(0.0, 0.0, 0.0, 1.0),
    )
}

fn depth_sorted_vertices(
    vertices: &[TexturedMeshVertex],
    local_transform: Matrix4,
) -> Arc<[TexturedMeshVertex]> {
    if vertices.len() <= 3 {
        return Arc::from(vertices.to_vec());
    }

    let mut tris = Vec::with_capacity(vertices.len() / 3);
    for tri in vertices.chunks_exact(3) {
        let p0 = local_transform * Vec4::new(tri[0].pos[0], tri[0].pos[1], tri[0].pos[2], 1.0);
        let p1 = local_transform * Vec4::new(tri[1].pos[0], tri[1].pos[1], tri[1].pos[2], 1.0);
        let p2 = local_transform * Vec4::new(tri[2].pos[0], tri[2].pos[1], tri[2].pos[2], 1.0);
        let e1 = Vector3::new(p1.x - p0.x, p1.y - p0.y, p1.z - p0.z);
        let e2 = Vector3::new(p2.x - p0.x, p2.y - p0.y, p2.z - p0.z);
        if e1.cross(e2).z >= 0.0 {
            continue;
        }
        let z = (p0.z + p1.z + p2.z) / 3.0;
        tris.push((z, tri));
    }
    tris.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut out = Vec::with_capacity(vertices.len());
    for (_, tri) in tris {
        out.extend_from_slice(tri);
    }
    Arc::from(out)
}

#[inline(always)]
fn actor_from_vertices(
    slot: &SpriteSlot,
    xy: [f32; 2],
    tint: [f32; 4],
    vertices: Arc<[TexturedMeshVertex]>,
    geom_cache_key: crate::engine::gfx::TMeshCacheKey,
    local_transform: Matrix4,
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
        local_transform,
        texture: slot.texture_key_shared(),
        tint,
        vertices,
        geom_cache_key,
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
    let vertices = build_model_geometry(slot);
    let affine = model_affine_transform(model, size, rotation_deg, draw);
    let local_transform = model_draw_transform(model.size(), affine);
    let (uv_scale, uv_offset, uv_tex_shift) = model_uv_params(slot, uv_rect);
    Some(actor_from_vertices(
        slot,
        xy,
        tint,
        vertices,
        crate::engine::gfx::INVALID_TMESH_CACHE_KEY,
        local_transform,
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
    let affine = model_affine_transform(model, size, rotation_deg, draw);
    let local_transform = model_draw_transform(model.size(), affine);
    let (geom_cache_key, vertices) = cache.get_or_insert_slot(slot)?;
    let (uv_scale, uv_offset, uv_tex_shift) = model_uv_params(slot, uv_rect);
    Some(actor_from_vertices(
        slot,
        xy,
        tint,
        vertices,
        geom_cache_key,
        local_transform,
        uv_scale,
        uv_offset,
        uv_tex_shift,
        model_blend(draw, blend),
        z,
    ))
}

pub(crate) fn noteskin_model_actor_from_draw_depth_sorted_affine(
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
    let affine = model_affine_transform(model, size, rotation_deg, draw);
    let local_transform = affine * Matrix4::from_scale(Vector3::new(1.0, -1.0, 1.0));
    let vertices = depth_sorted_vertices(build_model_geometry(slot).as_ref(), local_transform);
    if vertices.is_empty() {
        return None;
    }
    let (uv_scale, uv_offset, uv_tex_shift) = model_uv_params(slot, uv_rect);
    Some(actor_from_vertices(
        slot,
        xy,
        tint,
        vertices,
        crate::engine::gfx::INVALID_TMESH_CACHE_KEY,
        local_transform,
        uv_scale,
        uv_offset,
        uv_tex_shift,
        blend,
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
