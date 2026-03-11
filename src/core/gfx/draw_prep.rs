use crate::core::gfx::{BlendMode, MeshMode, ObjectType, RenderList, TexturedMeshVertex};
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
pub struct CachedTMeshGeom<Buffer> {
    pub cache_id: u64,
    pub vertex_count: u32,
    pub buffer: Buffer,
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
pub struct TexturedMeshRun<Tex, Buffer> {
    pub vertex_start: u32,
    pub vertex_count: u32,
    pub dynamic_geom: bool,
    pub geom_key: u64,
    pub cached_vertex_buffer: Option<Buffer>,
    pub instance_start: u32,
    pub instance_count: u32,
    pub mode: MeshMode,
    pub blend: BlendMode,
    pub texture: Tex,
    pub camera: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawOp<Tex, Buffer> {
    Sprite(SpriteRun<Tex>),
    Mesh(usize),
    TexturedMesh(TexturedMeshRun<Tex, Buffer>),
}

#[derive(Debug, Default)]
pub struct GlScratch<Tex, Buffer> {
    pub sprite_instances: Vec<SpriteInstanceRaw>,
    pub tmesh_vertices: Vec<TexturedMeshVertexRaw>,
    pub tmesh_instances: Vec<TexturedMeshInstanceRaw>,
    pub ops: Vec<DrawOp<Tex, Buffer>>,
    tmesh_geom: HashMap<TMeshGeomKey, FrameTMeshGeom<Buffer>>,
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
enum FrameTMeshGeom<Buffer> {
    Dynamic {
        vertex_start: u32,
        vertex_count: u32,
    },
    Cached(CachedTMeshGeom<Buffer>),
}

impl<Tex, Buffer> GlScratch<Tex, Buffer> {
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
pub fn decompose_2d(m: [[f32; 4]; 4]) -> ([f32; 2], [f32; 2], [f32; 2]) {
    let center = [m[3][0], m[3][1]];
    let c0 = [m[0][0], m[0][1]];
    let c1 = [m[1][0], m[1][1]];
    let sx = c0[0].hypot(c0[1]).max(1e-12);
    let sy = c1[0].hypot(c1[1]).max(1e-12);
    let cos_t = c0[0] / sx;
    let sin_t = c0[1] / sx;
    (center, [sx, sy], [sin_t, cos_t])
}

pub fn prepare_gl<Tex, Buffer, ResolveTexture, ResolveCachedGeom>(
    render_list: &RenderList<'_>,
    scratch: &mut GlScratch<Tex, Buffer>,
    mut resolve_texture: ResolveTexture,
    mut resolve_cached_geom: ResolveCachedGeom,
) -> PrepareStats
where
    Tex: Copy + Eq,
    Buffer: Copy,
    ResolveTexture: FnMut(&str) -> Option<Tex>,
    ResolveCachedGeom: FnMut(&[TexturedMeshVertex]) -> Option<CachedTMeshGeom<Buffer>>,
{
    let objects_len = render_list.objects.len();

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
                texture_id,
                tint,
                uv_scale,
                uv_offset,
                local_offset,
                local_offset_rot_sin_cos,
                edge_fade,
            } => {
                let Some(texture) = resolve_texture(texture_id.as_ref()) else {
                    continue;
                };

                let model: [[f32; 4]; 4] = obj.transform.into();
                let (center, size, rot_sin_cos) = decompose_2d(model);
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

                if let Some(DrawOp::Sprite(last)) = scratch.ops.last_mut()
                    && last.texture == texture
                    && last.blend == obj.blend
                    && last.camera == obj.camera
                    && last.instance_start + last.instance_count == instance_start
                {
                    last.instance_count += 1;
                    continue;
                }

                scratch.ops.push(DrawOp::Sprite(SpriteRun {
                    instance_start,
                    instance_count: 1,
                    blend: obj.blend,
                    texture,
                    camera: obj.camera,
                }));
            }
            ObjectType::Mesh { vertices, .. } => {
                if !vertices.is_empty() {
                    scratch.ops.push(DrawOp::Mesh(idx));
                }
            }
            ObjectType::TexturedMesh {
                texture_id,
                vertices,
                mode,
                uv_scale,
                uv_offset,
                uv_tex_shift,
            } => {
                if *mode != MeshMode::Triangles || vertices.is_empty() {
                    continue;
                }

                let Some(texture) = resolve_texture(texture_id.as_ref()) else {
                    continue;
                };

                let geom_key = TMeshGeomKey {
                    ptr: vertices.as_ptr() as usize,
                    len: vertices.len(),
                };
                let geom = if let Some(geom) = scratch.tmesh_geom.get(&geom_key).copied() {
                    geom
                } else {
                    let geom = if let Some(cached) = resolve_cached_geom(vertices.as_ref()) {
                        FrameTMeshGeom::Cached(cached)
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
                        FrameTMeshGeom::Dynamic {
                            vertex_start,
                            vertex_count: vertices.len() as u32,
                        }
                    };
                    scratch.tmesh_geom.insert(geom_key, geom);
                    geom
                };

                let (vertex_start, vertex_count, dynamic_geom, geom_run_key, cached_vertex_buffer) =
                    match geom {
                        FrameTMeshGeom::Dynamic {
                            vertex_start,
                            vertex_count,
                        } => (
                            vertex_start,
                            vertex_count,
                            true,
                            (1u64 << 63) | u64::from(vertex_start),
                            None,
                        ),
                        FrameTMeshGeom::Cached(cached) => (
                            0,
                            cached.vertex_count,
                            false,
                            cached.cache_id,
                            Some(cached.buffer),
                        ),
                    };

                let instance_start = scratch.tmesh_instances.len() as u32;
                let model: [[f32; 4]; 4] = obj.transform.into();
                scratch.tmesh_instances.push(TexturedMeshInstanceRaw {
                    model_col0: model[0],
                    model_col1: model[1],
                    model_col2: model[2],
                    model_col3: model[3],
                    uv_scale: *uv_scale,
                    uv_offset: *uv_offset,
                    uv_tex_shift: *uv_tex_shift,
                });

                if let Some(DrawOp::TexturedMesh(last)) = scratch.ops.last_mut()
                    && last.texture == texture
                    && last.blend == obj.blend
                    && last.camera == obj.camera
                    && last.mode == *mode
                    && last.dynamic_geom == dynamic_geom
                    && last.geom_key == geom_run_key
                    && last.instance_start + last.instance_count == instance_start
                {
                    last.instance_count += 1;
                    continue;
                }

                scratch.ops.push(DrawOp::TexturedMesh(TexturedMeshRun {
                    vertex_start,
                    vertex_count,
                    dynamic_geom,
                    geom_key: geom_run_key,
                    cached_vertex_buffer,
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

    stats
}
