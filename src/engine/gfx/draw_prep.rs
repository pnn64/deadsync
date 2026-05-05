use crate::engine::gfx::{
    BlendMode, FastU64Map, INVALID_TEXTURE_HANDLE, INVALID_TMESH_CACHE_KEY, MeshMode, MeshVertex,
    ObjectType, RenderList, TMeshCacheKey, TextureHandle, TexturedMeshVertex,
};
use glam::{Mat4 as Matrix4, Vec4 as Vector4};
use std::{collections::HashMap, hash::BuildHasherDefault};
use twox_hash::XxHash64;

type TMeshHasher = BuildHasherDefault<XxHash64>;
type TMeshGeomMap = HashMap<TMeshGeomKey, FrameTMeshGeom, TMeshHasher>;

#[repr(C)]
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    bytemuck::Pod,
    bytemuck::Zeroable,
)]
pub struct SpriteInstanceRaw {
    pub center: [f32; 4],
    pub size: [f32; 2],
    pub rot_sin_cos: [f32; 2],
    pub tint: [f32; 4],
    pub uv_scale: [f32; 2],
    pub uv_offset: [f32; 2],
    pub local_offset: [f32; 2],
    pub local_offset_rot_sin_cos: [f32; 2],
    pub edge_fade: [f32; 4],
    pub texture_mask: f32,
}

#[repr(C)]
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    bytemuck::Pod,
    bytemuck::Zeroable,
)]
pub struct TexturedMeshInstanceRaw {
    pub model_col0: [f32; 4],
    pub model_col1: [f32; 4],
    pub model_col2: [f32; 4],
    pub model_col3: [f32; 4],
    pub tint: [f32; 4],
    pub uv_scale: [f32; 2],
    pub uv_offset: [f32; 2],
    pub uv_tex_shift: [f32; 2],
    pub texture_mask: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpriteRun {
    pub instance_start: u32,
    pub instance_count: u32,
    pub blend: BlendMode,
    pub texture_handle: TextureHandle,
    pub camera: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeshRun {
    pub vertex_start: u32,
    pub vertex_count: u32,
    pub mode: MeshMode,
    pub blend: BlendMode,
    pub camera: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TexturedMeshSource {
    Transient {
        vertex_start: u32,
        vertex_count: u32,
        geom_key: u64,
    },
    Cached {
        cache_key: TMeshCacheKey,
        vertex_count: u32,
    },
}

impl TexturedMeshSource {
    #[inline(always)]
    pub const fn vertex_start(self) -> u32 {
        match self {
            Self::Transient { vertex_start, .. } => vertex_start,
            Self::Cached { .. } => 0,
        }
    }

    #[inline(always)]
    pub const fn vertex_count(self) -> u32 {
        match self {
            Self::Transient { vertex_count, .. } | Self::Cached { vertex_count, .. } => {
                vertex_count
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TexturedMeshRun {
    pub source: TexturedMeshSource,
    pub instance_start: u32,
    pub instance_count: u32,
    pub mode: MeshMode,
    pub blend: BlendMode,
    pub texture_handle: TextureHandle,
    pub camera: u8,
    pub depth_test: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawOp {
    Sprite(SpriteRun),
    Mesh(MeshRun),
    TexturedMesh(TexturedMeshRun),
}

#[derive(Debug, Default)]
pub struct DrawScratch {
    pub sprite_instances: Vec<SpriteInstanceRaw>,
    pub mesh_vertices: Vec<MeshVertex>,
    pub tmesh_vertices: Vec<TexturedMeshVertex>,
    pub tmesh_instances: Vec<TexturedMeshInstanceRaw>,
    pub ops: Vec<DrawOp>,
    transient_tmesh_geom: TMeshGeomMap,
    cached_tmesh: FastU64Map<bool>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PrepareStats {
    pub dynamic_upload_vertices: u64,
    pub cached_upload_vertices: u64,
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
    geom_key: u64,
}

impl DrawScratch {
    #[inline(always)]
    pub fn with_capacity(
        sprite_instances: usize,
        mesh_vertices: usize,
        tmesh_vertices: usize,
        tmesh_instances: usize,
        ops: usize,
    ) -> Self {
        Self {
            sprite_instances: Vec::with_capacity(sprite_instances),
            mesh_vertices: Vec::with_capacity(mesh_vertices),
            tmesh_vertices: Vec::with_capacity(tmesh_vertices),
            tmesh_instances: Vec::with_capacity(tmesh_instances),
            ops: Vec::with_capacity(ops),
            transient_tmesh_geom: HashMap::with_capacity_and_hasher(
                ops,
                BuildHasherDefault::default(),
            ),
            cached_tmesh: FastU64Map::with_capacity_and_hasher(ops, BuildHasherDefault::default()),
        }
    }
}

#[inline(always)]
fn textured_instance_raw(
    m: &Matrix4,
    tint: [f32; 4],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    uv_tex_shift: [f32; 2],
    texture_mask: bool,
) -> TexturedMeshInstanceRaw {
    TexturedMeshInstanceRaw {
        model_col0: [m.x_axis.x, m.x_axis.y, m.x_axis.z, m.x_axis.w],
        model_col1: [m.y_axis.x, m.y_axis.y, m.y_axis.z, m.y_axis.w],
        model_col2: [m.z_axis.x, m.z_axis.y, m.z_axis.z, m.z_axis.w],
        model_col3: [m.w_axis.x, m.w_axis.y, m.w_axis.z, m.w_axis.w],
        tint,
        uv_scale,
        uv_offset,
        uv_tex_shift,
        texture_mask: texture_mask as u8 as f32,
    }
}

#[inline(always)]
fn flush_sprite_run(sprite_run: &mut Option<SpriteRun>, ops: &mut Vec<DrawOp>) {
    if let Some(run) = sprite_run.take() {
        ops.push(DrawOp::Sprite(run));
    }
}

#[inline(always)]
fn transient_tmesh_source(
    scratch: &mut DrawScratch,
    vertices: &[TexturedMeshVertex],
    stats: &mut PrepareStats,
) -> TexturedMeshSource {
    let geom_key = TMeshGeomKey {
        ptr: vertices.as_ptr() as usize,
        len: vertices.len(),
    };
    if let Some(geom) = scratch.transient_tmesh_geom.get(&geom_key).copied() {
        return TexturedMeshSource::Transient {
            vertex_start: geom.vertex_start,
            vertex_count: geom.vertex_count,
            geom_key: geom.geom_key,
        };
    }

    let vertex_start = scratch.tmesh_vertices.len() as u32;
    scratch.tmesh_vertices.extend_from_slice(vertices);
    stats.dynamic_upload_vertices = stats
        .dynamic_upload_vertices
        .saturating_add(vertices.len() as u64);
    let vertex_count = vertices.len() as u32;
    let geom_run_key = ((vertex_start as u64) << 32) | u64::from(vertex_count);
    scratch.transient_tmesh_geom.insert(
        geom_key,
        FrameTMeshGeom {
            vertex_start,
            vertex_count,
            geom_key: geom_run_key,
        },
    );
    TexturedMeshSource::Transient {
        vertex_start,
        vertex_count,
        geom_key: geom_run_key,
    }
}

pub fn prepare<EnsureCached>(
    render_list: &RenderList,
    scratch: &mut DrawScratch,
    mut ensure_cached_tmesh: EnsureCached,
) -> PrepareStats
where
    EnsureCached: FnMut(TMeshCacheKey, &[TexturedMeshVertex]) -> bool,
{
    let objects_len = render_list.objects.len();
    let mut sprite_run: Option<SpriteRun> = None;

    scratch.sprite_instances.clear();
    if scratch.sprite_instances.capacity() < objects_len {
        scratch
            .sprite_instances
            .reserve(objects_len - scratch.sprite_instances.len());
    }
    debug_assert!(scratch.sprite_instances.capacity() >= objects_len);

    scratch.mesh_vertices.clear();
    scratch.tmesh_vertices.clear();
    scratch.tmesh_instances.clear();

    scratch.ops.clear();
    if scratch.ops.capacity() < objects_len {
        scratch.ops.reserve(objects_len - scratch.ops.len());
    }
    debug_assert!(scratch.ops.capacity() >= objects_len);

    let mut stats = PrepareStats::default();
    let mut tmesh_maps_cleared = false;

    for obj in render_list.objects.iter() {
        match &obj.object_type {
            ObjectType::Sprite {
                center,
                size,
                rot_sin_cos,
                tint,
                uv_scale,
                uv_offset,
                local_offset,
                local_offset_rot_sin_cos,
                edge_fade,
                texture_mask,
                ..
            } => {
                let texture_handle = obj.texture_handle;
                if texture_handle == INVALID_TEXTURE_HANDLE {
                    continue;
                }

                let instance_start = scratch.sprite_instances.len() as u32;
                scratch.sprite_instances.push(SpriteInstanceRaw {
                    center: *center,
                    size: *size,
                    rot_sin_cos: *rot_sin_cos,
                    tint: *tint,
                    uv_scale: *uv_scale,
                    uv_offset: *uv_offset,
                    local_offset: *local_offset,
                    local_offset_rot_sin_cos: *local_offset_rot_sin_cos,
                    edge_fade: *edge_fade,
                    texture_mask: *texture_mask as u8 as f32,
                });

                if let Some(last) = sprite_run.as_mut()
                    && last.texture_handle == texture_handle
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
                    texture_handle,
                    camera: obj.camera,
                });
            }
            ObjectType::Mesh {
                tint,
                vertices,
                mode,
            } => {
                flush_sprite_run(&mut sprite_run, &mut scratch.ops);
                if *mode != MeshMode::Triangles || vertices.is_empty() {
                    continue;
                }

                let vertex_start = scratch.mesh_vertices.len() as u32;
                scratch.mesh_vertices.reserve(vertices.len());
                for v in vertices.iter() {
                    let p = obj.transform * Vector4::new(v.pos[0], v.pos[1], 0.0, 1.0);
                    scratch.mesh_vertices.push(MeshVertex {
                        pos: [p.x, p.y],
                        color: [
                            v.color[0] * tint[0],
                            v.color[1] * tint[1],
                            v.color[2] * tint[2],
                            v.color[3] * tint[3],
                        ],
                    });
                }

                if let Some(DrawOp::Mesh(last)) = scratch.ops.last_mut()
                    && last.blend == obj.blend
                    && last.camera == obj.camera
                    && last.mode == *mode
                    && last.vertex_start + last.vertex_count == vertex_start
                {
                    last.vertex_count += vertices.len() as u32;
                    continue;
                }

                scratch.ops.push(DrawOp::Mesh(MeshRun {
                    vertex_start,
                    vertex_count: vertices.len() as u32,
                    mode: *mode,
                    blend: obj.blend,
                    camera: obj.camera,
                }));
            }
            ObjectType::TexturedMesh {
                tint,
                vertices,
                geom_cache_key,
                mode,
                uv_scale,
                uv_offset,
                uv_tex_shift,
                texture_mask,
                depth_test,
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
                if !tmesh_maps_cleared {
                    scratch.transient_tmesh_geom.clear();
                    scratch.cached_tmesh.clear();
                    tmesh_maps_cleared = true;
                }

                let source = if *geom_cache_key != INVALID_TMESH_CACHE_KEY {
                    let cached = if let Some(cached) = scratch.cached_tmesh.get(geom_cache_key) {
                        *cached
                    } else {
                        let cached = ensure_cached_tmesh(*geom_cache_key, vertices.as_ref());
                        scratch.cached_tmesh.insert(*geom_cache_key, cached);
                        if cached {
                            stats.cached_upload_vertices = stats
                                .cached_upload_vertices
                                .saturating_add(vertices.len() as u64);
                        }
                        cached
                    };
                    if cached {
                        TexturedMeshSource::Cached {
                            cache_key: *geom_cache_key,
                            vertex_count: vertices.len() as u32,
                        }
                    } else {
                        transient_tmesh_source(scratch, vertices.as_ref(), &mut stats)
                    }
                } else {
                    transient_tmesh_source(scratch, vertices.as_ref(), &mut stats)
                };

                let instance_start = scratch.tmesh_instances.len() as u32;
                scratch.tmesh_instances.push(textured_instance_raw(
                    &obj.transform,
                    *tint,
                    *uv_scale,
                    *uv_offset,
                    *uv_tex_shift,
                    *texture_mask,
                ));

                if let Some(DrawOp::TexturedMesh(last)) = scratch.ops.last_mut()
                    && last.texture_handle == texture_handle
                    && last.blend == obj.blend
                    && last.camera == obj.camera
                    && last.depth_test == *depth_test
                    && last.mode == *mode
                    && last.source == source
                    && last.instance_start + last.instance_count == instance_start
                {
                    last.instance_count += 1;
                    continue;
                }

                scratch.ops.push(DrawOp::TexturedMesh(TexturedMeshRun {
                    source,
                    instance_start,
                    instance_count: 1,
                    mode: *mode,
                    blend: obj.blend,
                    texture_handle,
                    camera: obj.camera,
                    depth_test: *depth_test,
                }));
            }
        }
    }

    flush_sprite_run(&mut sprite_run, &mut scratch.ops);
    stats
}

#[cfg(test)]
mod tests {
    use super::{DrawScratch, prepare};
    use crate::engine::gfx::{
        BlendMode, INVALID_TEXTURE_HANDLE, ObjectType, RenderList, RenderObject,
    };
    use glam::Mat4 as Matrix4;

    fn sprite_object(order: u32) -> RenderObject {
        RenderObject {
            object_type: ObjectType::Sprite {
                center: [0.0, 0.0, 0.0, 1.0],
                size: [1.0, 1.0],
                rot_sin_cos: [0.0, 1.0],
                tint: [1.0, 1.0, 1.0, 1.0],
                uv_scale: [1.0, 1.0],
                uv_offset: [0.0, 0.0],
                local_offset: [0.0, 0.0],
                local_offset_rot_sin_cos: [0.0, 1.0],
                edge_fade: [0.0, 0.0, 0.0, 0.0],
                texture_mask: false,
            },
            texture_handle: INVALID_TEXTURE_HANDLE,
            transform: Matrix4::IDENTITY,
            blend: BlendMode::Alpha,
            z: 0,
            order,
            camera: 0,
        }
    }

    #[test]
    fn prepare_reserves_scratch_buffers_from_len() {
        let render_list = RenderList {
            clear_color: [0.0, 0.0, 0.0, 1.0],
            cameras: vec![Matrix4::IDENTITY],
            objects: (0..101).map(sprite_object).collect(),
        };
        let mut scratch = DrawScratch::with_capacity(100, 0, 0, 0, 100);

        prepare(&render_list, &mut scratch, |_, _| false);

        assert!(scratch.sprite_instances.capacity() >= render_list.objects.len());
        assert!(scratch.ops.capacity() >= render_list.objects.len());
    }
}
