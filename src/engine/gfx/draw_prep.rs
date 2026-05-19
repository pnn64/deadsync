use crate::engine::gfx::{
    BlendMode, FastU64Map, INVALID_TEXTURE_HANDLE, INVALID_TMESH_CACHE_KEY, MeshVertex, ObjectType,
    RenderList, TMeshCacheKey, TextureHandle, TexturedMeshInstanceRaw, TexturedMeshVertex,
    TexturedMeshVertices,
};
use glam::Vec4 as Vector4;
use std::{collections::HashMap, hash::BuildHasherDefault};
use twox_hash::XxHash64;

type TMeshHasher = BuildHasherDefault<XxHash64>;
type TMeshGeomMap = HashMap<TMeshGeomKey, FrameTMeshGeom, TMeshHasher>;

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
    pub mesh_vertices: Vec<MeshVertex>,
    pub tmesh_vertices: Vec<TexturedMeshVertex>,
    pub tmesh_instances: Vec<TexturedMeshInstanceRaw>,
    pub ops: Vec<DrawOp>,
    shared_tmesh_geom: TMeshGeomMap,
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
        mesh_vertices: usize,
        tmesh_vertices: usize,
        tmesh_instances: usize,
        ops: usize,
    ) -> Self {
        Self {
            mesh_vertices: Vec::with_capacity(mesh_vertices),
            tmesh_vertices: Vec::with_capacity(tmesh_vertices),
            tmesh_instances: Vec::with_capacity(tmesh_instances),
            ops: Vec::with_capacity(ops),
            shared_tmesh_geom: HashMap::with_capacity_and_hasher(
                ops,
                BuildHasherDefault::default(),
            ),
            cached_tmesh: FastU64Map::with_capacity_and_hasher(ops, BuildHasherDefault::default()),
        }
    }
}

#[inline(always)]
fn transient_tmesh_source(
    scratch: &mut DrawScratch,
    vertices: &[TexturedMeshVertex],
    stats: &mut PrepareStats,
) -> TexturedMeshSource {
    let vertex_start = scratch.tmesh_vertices.len() as u32;
    scratch.tmesh_vertices.extend_from_slice(vertices);
    stats.dynamic_upload_vertices = stats
        .dynamic_upload_vertices
        .saturating_add(vertices.len() as u64);
    let vertex_count = vertices.len() as u32;
    TexturedMeshSource::Transient {
        vertex_start,
        vertex_count,
        geom_key: ((vertex_start as u64) << 32) | u64::from(vertex_count),
    }
}

#[inline(always)]
fn shared_tmesh_source(
    scratch: &mut DrawScratch,
    vertices: &[TexturedMeshVertex],
    stats: &mut PrepareStats,
    shared_tmesh_geom_cleared: &mut bool,
) -> TexturedMeshSource {
    if !*shared_tmesh_geom_cleared {
        scratch.shared_tmesh_geom.clear();
        *shared_tmesh_geom_cleared = true;
    }

    let geom_key = TMeshGeomKey {
        ptr: vertices.as_ptr() as usize,
        len: vertices.len(),
    };
    if let Some(geom) = scratch.shared_tmesh_geom.get(&geom_key).copied() {
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
    scratch.shared_tmesh_geom.insert(
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

    scratch.mesh_vertices.clear();
    scratch.tmesh_vertices.clear();
    scratch.tmesh_instances.clear();

    scratch.ops.clear();
    if scratch.ops.capacity() < objects_len {
        scratch.ops.reserve(objects_len - scratch.ops.len());
    }
    debug_assert!(scratch.ops.capacity() >= objects_len);

    let mut stats = PrepareStats::default();
    let mut shared_tmesh_geom_cleared = false;
    let mut cached_tmesh_cleared = false;
    let mut i = 0usize;

    while i < objects_len {
        let obj = &render_list.objects[i];
        match &obj.object_type {
            ObjectType::Sprite(instance_start) => {
                let mut sprite_run: Option<SpriteRun> = None;
                let mut obj = obj;
                let mut instance_start = *instance_start;
                loop {
                    let texture_handle = obj.texture_handle;
                    if texture_handle != INVALID_TEXTURE_HANDLE {
                        if let Some(last) = sprite_run.as_mut()
                            && last.texture_handle == texture_handle
                            && last.blend == obj.blend
                            && last.camera == obj.camera
                            && last.instance_start + last.instance_count == instance_start
                        {
                            last.instance_count += 1;
                        } else {
                            if let Some(run) = sprite_run.take() {
                                scratch.ops.push(DrawOp::Sprite(run));
                            }
                            sprite_run = Some(SpriteRun {
                                instance_start,
                                instance_count: 1,
                                blend: obj.blend,
                                texture_handle,
                                camera: obj.camera,
                            });
                        }
                    }

                    i += 1;
                    if i >= objects_len {
                        break;
                    }
                    obj = &render_list.objects[i];
                    let ObjectType::Sprite(next_instance_start) = &obj.object_type else {
                        break;
                    };
                    instance_start = *next_instance_start;
                }

                if let Some(run) = sprite_run {
                    scratch.ops.push(DrawOp::Sprite(run));
                }
            }
            ObjectType::Mesh {
                transform,
                tint,
                vertices,
            } => {
                if vertices.is_empty() {
                    i += 1;
                    continue;
                }

                let vertex_start = scratch.mesh_vertices.len() as u32;
                scratch.mesh_vertices.reserve(vertices.len());
                for v in vertices.iter() {
                    let p = *transform * Vector4::new(v.pos[0], v.pos[1], 0.0, 1.0);
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
                    && last.vertex_start + last.vertex_count == vertex_start
                {
                    last.vertex_count += vertices.len() as u32;
                    i += 1;
                    continue;
                }

                scratch.ops.push(DrawOp::Mesh(MeshRun {
                    vertex_start,
                    vertex_count: vertices.len() as u32,
                    blend: obj.blend,
                    camera: obj.camera,
                }));
                i += 1;
            }
            ObjectType::TexturedMesh {
                instance,
                vertices,
                geom_cache_key,
                depth_test,
                ..
            } => {
                if vertices.is_empty() {
                    i += 1;
                    continue;
                }
                let texture_handle = obj.texture_handle;
                if texture_handle == INVALID_TEXTURE_HANDLE {
                    i += 1;
                    continue;
                }
                let source = if *geom_cache_key != INVALID_TMESH_CACHE_KEY {
                    if !cached_tmesh_cleared {
                        scratch.cached_tmesh.clear();
                        cached_tmesh_cleared = true;
                    }
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
                        match vertices {
                            TexturedMeshVertices::Transient(vertices) => {
                                transient_tmesh_source(scratch, vertices.as_slice(), &mut stats)
                            }
                            TexturedMeshVertices::Shared(vertices) => shared_tmesh_source(
                                scratch,
                                vertices.as_ref(),
                                &mut stats,
                                &mut shared_tmesh_geom_cleared,
                            ),
                        }
                    }
                } else {
                    match vertices {
                        TexturedMeshVertices::Transient(vertices) => {
                            transient_tmesh_source(scratch, vertices.as_slice(), &mut stats)
                        }
                        TexturedMeshVertices::Shared(vertices) => shared_tmesh_source(
                            scratch,
                            vertices.as_ref(),
                            &mut stats,
                            &mut shared_tmesh_geom_cleared,
                        ),
                    }
                };

                let instance_start = scratch.tmesh_instances.len() as u32;
                scratch.tmesh_instances.push(*instance);

                if let Some(DrawOp::TexturedMesh(last)) = scratch.ops.last_mut()
                    && last.texture_handle == texture_handle
                    && last.blend == obj.blend
                    && last.camera == obj.camera
                    && last.depth_test == *depth_test
                    && last.source == source
                    && last.instance_start + last.instance_count == instance_start
                {
                    last.instance_count += 1;
                    i += 1;
                    continue;
                }

                scratch.ops.push(DrawOp::TexturedMesh(TexturedMeshRun {
                    source,
                    instance_start,
                    instance_count: 1,
                    blend: obj.blend,
                    texture_handle,
                    camera: obj.camera,
                    depth_test: *depth_test,
                }));
                i += 1;
            }
        }
    }

    stats
}

#[cfg(test)]
mod tests {
    use super::{DrawScratch, prepare};
    use crate::engine::gfx::{
        BlendMode, INVALID_TEXTURE_HANDLE, ObjectType, RenderList, RenderObject, SpriteInstanceRaw,
    };
    use glam::Mat4 as Matrix4;

    fn sprite_object(order: u32) -> RenderObject {
        RenderObject {
            object_type: ObjectType::Sprite(order),
            texture_handle: INVALID_TEXTURE_HANDLE,
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
            sprite_instances: (0..101)
                .map(|_| SpriteInstanceRaw {
                    center: [0.0, 0.0, 0.0, 1.0],
                    size: [1.0, 1.0],
                    rot_sin_cos: [0.0, 1.0],
                    tint: [1.0, 1.0, 1.0, 1.0],
                    uv_scale: [1.0, 1.0],
                    uv_offset: [0.0, 0.0],
                    local_offset: [0.0, 0.0],
                    local_offset_rot_sin_cos: [0.0, 1.0],
                    edge_fade: [0.0, 0.0, 0.0, 0.0],
                    texture_mask: 0.0,
                })
                .collect(),
            objects: (0..101).map(sprite_object).collect(),
        };
        let mut scratch = DrawScratch::with_capacity(0, 0, 0, 100);

        prepare(&render_list, &mut scratch, |_, _| false);

        assert!(scratch.ops.capacity() >= render_list.objects.len());
    }
}
