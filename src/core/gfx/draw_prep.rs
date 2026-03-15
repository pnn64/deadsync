use crate::core::gfx::{
    BlendMode, INVALID_TEXTURE_HANDLE, MeshMode, ObjectType, RenderList, TextureHandle,
};
use cgmath::Matrix4;
use std::collections::HashMap;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SpriteInstanceRaw {
    pub center: [f32; 2],
    pub size: [f32; 2],
    pub rot_sin_cos: [f32; 2],
    pub tint: [f32; 4],
    pub uv_scale: [f32; 2],
    pub uv_offset: [f32; 2],
    pub local_offset: [f32; 2],
    pub local_offset_rot_sin_cos: [f32; 2],
    pub edge_fade: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TexturedMeshVertexRaw {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    pub tex_matrix_scale: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TexturedMeshInstanceRaw {
    pub model_col0: [f32; 4],
    pub model_col1: [f32; 4],
    pub model_col2: [f32; 4],
    pub model_col3: [f32; 4],
    pub uv_scale: [f32; 2],
    pub uv_offset: [f32; 2],
    pub uv_tex_shift: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpriteRun<Tex> {
    pub instance_start: u32,
    pub instance_count: u32,
    pub blend: BlendMode,
    pub texture: Tex,
    pub camera: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TexturedMeshRun<Tex> {
    pub vertex_start: u32,
    pub vertex_count: u32,
    pub geom_key: u64,
    pub instance_start: u32,
    pub instance_count: u32,
    pub mode: MeshMode,
    pub blend: BlendMode,
    pub texture: Tex,
    pub camera: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawOp<Tex> {
    Sprite(SpriteRun<Tex>),
    Mesh(usize),
    TexturedMesh(TexturedMeshRun<Tex>),
}

#[derive(Debug, Default)]
pub struct GlScratch<Tex> {
    pub sprite_instances: Vec<SpriteInstanceRaw>,
    pub tmesh_vertices: Vec<TexturedMeshVertexRaw>,
    pub tmesh_instances: Vec<TexturedMeshInstanceRaw>,
    pub ops: Vec<DrawOp<Tex>>,
    tmesh_geom: HashMap<TMeshGeomKey, FrameTMeshGeom>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PrepareStats {
    pub dynamic_upload_vertices: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TMeshGeomKey {
    ptr: usize,
    len: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FrameTMeshGeom {
    vertex_start: u32,
    vertex_count: u32,
}

impl<Tex> GlScratch<Tex> {
    #[inline(always)]
    pub fn with_capacity(
        sprite_instances: usize,
        tmesh_vertices: usize,
        tmesh_instances: usize,
        ops: usize,
    ) -> Self {
        Self {
            sprite_instances: Vec::with_capacity(sprite_instances),
            tmesh_vertices: Vec::with_capacity(tmesh_vertices),
            tmesh_instances: Vec::with_capacity(tmesh_instances),
            ops: Vec::with_capacity(ops),
            tmesh_geom: HashMap::with_capacity(ops),
        }
    }
}

#[inline(always)]
pub fn decompose_2d(m: &Matrix4<f32>) -> ([f32; 2], [f32; 2], [f32; 2]) {
    let center = [m.w.x, m.w.y];
    let c0 = [m.x.x, m.x.y];
    let c1 = [m.y.x, m.y.y];
    let sx = c0[0].hypot(c0[1]).max(1e-12);
    let sy = c1[0].hypot(c1[1]).max(1e-12);
    let cos_t = c0[0] / sx;
    let sin_t = c0[1] / sx;
    (center, [sx, sy], [sin_t, cos_t])
}

#[inline(always)]
fn resolve_texture_cached<Tex, ResolveTexture>(
    texture_handle: TextureHandle,
    last_texture_handle: &mut TextureHandle,
    last_texture: &mut Option<Tex>,
    resolve_texture: &mut ResolveTexture,
) -> Option<Tex>
where
    Tex: Copy,
    ResolveTexture: FnMut(TextureHandle) -> Option<Tex>,
{
    if *last_texture_handle == texture_handle {
        return *last_texture;
    }
    let texture = resolve_texture(texture_handle);
    *last_texture_handle = texture_handle;
    *last_texture = texture;
    texture
}

#[inline(always)]
fn textured_instance_raw(
    m: &Matrix4<f32>,
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    uv_tex_shift: [f32; 2],
) -> TexturedMeshInstanceRaw {
    TexturedMeshInstanceRaw {
        model_col0: [m.x.x, m.x.y, m.x.z, m.x.w],
        model_col1: [m.y.x, m.y.y, m.y.z, m.y.w],
        model_col2: [m.z.x, m.z.y, m.z.z, m.z.w],
        model_col3: [m.w.x, m.w.y, m.w.z, m.w.w],
        uv_scale,
        uv_offset,
        uv_tex_shift,
    }
}

#[inline(always)]
fn flush_sprite_run<Tex>(sprite_run: &mut Option<SpriteRun<Tex>>, ops: &mut Vec<DrawOp<Tex>>) {
    if let Some(run) = sprite_run.take() {
        ops.push(DrawOp::Sprite(run));
    }
}

pub fn prepare_gl<Tex, ResolveTexture>(
    render_list: &RenderList<'_>,
    scratch: &mut GlScratch<Tex>,
    mut resolve_texture: ResolveTexture,
) -> PrepareStats
where
    Tex: Copy + Eq,
    ResolveTexture: FnMut(TextureHandle) -> Option<Tex>,
{
    let objects_len = render_list.objects.len();
    let mut last_texture_handle = INVALID_TEXTURE_HANDLE;
    let mut last_texture = None;
    let mut sprite_run: Option<SpriteRun<Tex>> = None;

    scratch.sprite_instances.clear();
    if scratch.sprite_instances.capacity() < objects_len {
        scratch
            .sprite_instances
            .reserve(objects_len - scratch.sprite_instances.capacity());
    }

    scratch.tmesh_vertices.clear();
    let want_tmesh = objects_len.saturating_mul(4);
    if scratch.tmesh_vertices.capacity() < want_tmesh {
        scratch
            .tmesh_vertices
            .reserve(want_tmesh - scratch.tmesh_vertices.capacity());
    }

    scratch.tmesh_instances.clear();
    if scratch.tmesh_instances.capacity() < objects_len {
        scratch
            .tmesh_instances
            .reserve(objects_len - scratch.tmesh_instances.capacity());
    }

    scratch.ops.clear();
    if scratch.ops.capacity() < objects_len {
        scratch.ops.reserve(objects_len - scratch.ops.capacity());
    }

    scratch.tmesh_geom.clear();
    if scratch.tmesh_geom.capacity() < objects_len {
        scratch
            .tmesh_geom
            .reserve(objects_len - scratch.tmesh_geom.capacity());
    }

    let mut stats = PrepareStats::default();

    for (idx, obj) in render_list.objects.iter().enumerate() {
        match &obj.object_type {
            ObjectType::Sprite {
                tint,
                uv_scale,
                uv_offset,
                local_offset,
                local_offset_rot_sin_cos,
                edge_fade,
                ..
            } => {
                let texture_handle = obj.texture_handle;
                if texture_handle == INVALID_TEXTURE_HANDLE {
                    continue;
                }
                let Some(texture) = resolve_texture_cached(
                    texture_handle,
                    &mut last_texture_handle,
                    &mut last_texture,
                    &mut resolve_texture,
                ) else {
                    continue;
                };

                let (center, size, rot_sin_cos) = decompose_2d(&obj.transform);
                let instance_start = scratch.sprite_instances.len() as u32;
                scratch.sprite_instances.push(SpriteInstanceRaw {
                    center,
                    size,
                    rot_sin_cos,
                    tint: *tint,
                    uv_scale: *uv_scale,
                    uv_offset: *uv_offset,
                    local_offset: *local_offset,
                    local_offset_rot_sin_cos: *local_offset_rot_sin_cos,
                    edge_fade: *edge_fade,
                });

                if let Some(last) = sprite_run.as_mut()
                    && last.texture == texture
                    && last.blend == obj.blend
                    && last.camera == obj.camera
                    && last.instance_start + last.instance_count == instance_start
                {
                    last.instance_count += 1;
                    continue;
                }

                flush_sprite_run(&mut sprite_run, &mut scratch.ops);
                sprite_run = Some(SpriteRun {
                    instance_start,
                    instance_count: 1,
                    blend: obj.blend,
                    texture,
                    camera: obj.camera,
                });
            }
            ObjectType::Mesh { vertices, .. } => {
                flush_sprite_run(&mut sprite_run, &mut scratch.ops);
                if !vertices.is_empty() {
                    scratch.ops.push(DrawOp::Mesh(idx));
                }
            }
            ObjectType::TexturedMesh {
                vertices,
                mode,
                uv_scale,
                uv_offset,
                uv_tex_shift,
                ..
            } => {
                flush_sprite_run(&mut sprite_run, &mut scratch.ops);
                if *mode != MeshMode::Triangles || vertices.is_empty() {
                    continue;
                }
                let texture_handle = obj.texture_handle;
                if texture_handle == INVALID_TEXTURE_HANDLE {
                    continue;
                }

                let Some(texture) = resolve_texture_cached(
                    texture_handle,
                    &mut last_texture_handle,
                    &mut last_texture,
                    &mut resolve_texture,
                ) else {
                    continue;
                };

                let geom_key = TMeshGeomKey {
                    ptr: vertices.as_ptr() as usize,
                    len: vertices.len(),
                };
                let geom = if let Some(geom) = scratch.tmesh_geom.get(&geom_key).copied() {
                    geom
                } else {
                    let vertex_start = scratch.tmesh_vertices.len() as u32;
                    scratch.tmesh_vertices.reserve(vertices.len());
                    for v in vertices.iter() {
                        scratch.tmesh_vertices.push(TexturedMeshVertexRaw {
                            pos: v.pos,
                            uv: v.uv,
                            color: v.color,
                            tex_matrix_scale: v.tex_matrix_scale,
                        });
                    }
                    stats.dynamic_upload_vertices = stats
                        .dynamic_upload_vertices
                        .saturating_add(vertices.len() as u64);
                    let geom = FrameTMeshGeom {
                        vertex_start,
                        vertex_count: vertices.len() as u32,
                    };
                    scratch.tmesh_geom.insert(geom_key, geom);
                    geom
                };
                let vertex_start = geom.vertex_start;
                let vertex_count = geom.vertex_count;
                let geom_run_key = ((vertex_start as u64) << 32) | u64::from(vertex_count);

                let instance_start = scratch.tmesh_instances.len() as u32;
                scratch.tmesh_instances.push(textured_instance_raw(
                    &obj.transform,
                    *uv_scale,
                    *uv_offset,
                    *uv_tex_shift,
                ));

                if let Some(DrawOp::TexturedMesh(last)) = scratch.ops.last_mut()
                    && last.texture == texture
                    && last.blend == obj.blend
                    && last.camera == obj.camera
                    && last.mode == *mode
                    && last.geom_key == geom_run_key
                    && last.vertex_count == vertex_count
                    && last.instance_start + last.instance_count == instance_start
                {
                    last.instance_count += 1;
                    continue;
                }

                scratch.ops.push(DrawOp::TexturedMesh(TexturedMeshRun {
                    vertex_start,
                    vertex_count,
                    geom_key: geom_run_key,
                    instance_start,
                    instance_count: 1,
                    mode: *mode,
                    blend: obj.blend,
                    texture,
                    camera: obj.camera,
                }));
            }
        }
    }

    flush_sprite_run(&mut sprite_run, &mut scratch.ops);

    stats
}
