use crate::{
    BlendMode, INVALID_TMESH_CACHE_KEY, MeshVertex, ObjectType, RenderBatchKind, RenderList,
    RenderObject, TMeshCacheKey, TextureHandle, TexturedMeshInstanceRaw, TexturedMeshVertex,
    TexturedMeshVertices,
};
use glam::Vec4 as Vector4;

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

    #[inline(always)]
    pub const fn shares_vertex_buffer(self, other: Self) -> bool {
        match (self, other) {
            (Self::Transient { .. }, Self::Transient { .. }) => true,
            (
                Self::Cached {
                    cache_key: left, ..
                },
                Self::Cached {
                    cache_key: right, ..
                },
            ) => left == right,
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CameraUploadCache {
    last_camera: Option<u8>,
}

impl CameraUploadCache {
    #[inline(always)]
    pub fn update_required(&mut self, camera: u8) -> bool {
        if self.last_camera == Some(camera) {
            false
        } else {
            self.last_camera = Some(camera);
            true
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum TMeshGeomKey {
    Cached(TMeshCacheKey),
    Shared { ptr: usize, len: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// One-entry geometry reuse owned by a single draw-prep call.
///
/// Thread model: stack-local and single-threaded. Lifetime/capacity: one frame,
/// one geometry. Warmup: none. A miss replaces the entry in O(1) and copies the
/// source into the already retained staging vector; there is no scan, allocation,
/// or deferred destruction. The draw-prep benchmark reports staged vertex count,
/// which exposes misses. Worst case is one bounded source copy per logical batch.
struct FrameTMeshGeom {
    key: TMeshGeomKey,
    source: TexturedMeshSource,
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
        }
    }
}

#[inline(always)]
fn transient_tmesh_source(
    scratch: &mut DrawScratch,
    vertices: &[TexturedMeshVertex],
) -> TexturedMeshSource {
    let vertex_start = scratch.tmesh_vertices.len() as u32;
    scratch.tmesh_vertices.extend_from_slice(vertices);
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
    last_geom: &mut Option<FrameTMeshGeom>,
) -> TexturedMeshSource {
    let key = TMeshGeomKey::Shared {
        ptr: vertices.as_ptr() as usize,
        len: vertices.len(),
    };
    if let Some(geom) = *last_geom
        && geom.key == key
    {
        return geom.source;
    }
    let source = transient_tmesh_source(scratch, vertices);
    *last_geom = Some(FrameTMeshGeom { key, source });
    source
}

pub fn prepare<EnsureCached>(
    render_list: &RenderList,
    scratch: &mut DrawScratch,
    mut ensure_cached_tmesh: EnsureCached,
) where
    EnsureCached: FnMut(TMeshCacheKey, &[TexturedMeshVertex]) -> bool,
{
    scratch.mesh_vertices.clear();
    scratch.tmesh_vertices.clear();
    scratch.tmesh_instances.clear();

    scratch.ops.clear();
    let batch_count = render_list.batches.len();
    if scratch.ops.capacity() < batch_count {
        scratch.ops.reserve(batch_count - scratch.ops.len());
    }
    debug_assert!(scratch.ops.capacity() >= batch_count);

    let mut last_tmesh_geom = None;
    for batch in &render_list.batches {
        match batch.kind {
            RenderBatchKind::Sprite {
                instance_start,
                instance_count,
                blend,
                texture_handle,
                camera,
            } => scratch.ops.push(DrawOp::Sprite(SpriteRun {
                instance_start,
                instance_count,
                blend,
                texture_handle,
                camera,
            })),
            RenderBatchKind::Mesh {
                object_start,
                object_count,
                blend,
                camera,
            } => prepare_mesh_batch(
                &render_list.objects,
                scratch,
                object_start,
                object_count,
                blend,
                camera,
            ),
            RenderBatchKind::TexturedMesh {
                object_start,
                object_count,
            } => prepare_tmesh_batch(
                &render_list.objects,
                scratch,
                object_start,
                object_count,
                &mut ensure_cached_tmesh,
                &mut last_tmesh_geom,
            ),
        }
    }
}

fn prepare_mesh_batch(
    objects: &[RenderObject],
    scratch: &mut DrawScratch,
    object_start: u32,
    object_count: u32,
    blend: BlendMode,
    camera: u8,
) {
    let vertex_start = scratch.mesh_vertices.len() as u32;
    for object in &objects[object_start as usize..(object_start + object_count) as usize] {
        let ObjectType::Mesh {
            transform,
            tint,
            vertices,
        } = &object.object_type
        else {
            debug_assert!(false, "mesh batch contains a non-mesh object");
            continue;
        };
        append_mesh_vertices(&mut scratch.mesh_vertices, transform, *tint, vertices);
    }
    let vertex_count = scratch.mesh_vertices.len() as u32 - vertex_start;
    if vertex_count != 0 {
        scratch.ops.push(DrawOp::Mesh(MeshRun {
            vertex_start,
            vertex_count,
            blend,
            camera,
        }));
    }
}

#[inline(always)]
fn append_mesh_vertices(
    out: &mut Vec<MeshVertex>,
    transform: &glam::Mat4,
    tint: [f32; 4],
    vertices: &[MeshVertex],
) {
    out.reserve(vertices.len());

    // Presentation composes ordinary 2D meshes with this exact transform.
    // Avoid a full Mat4 × Vec4 and four color multiplies per vertex for the
    // common gameplay density-graph path.
    if transform.x_axis == Vector4::X
        && transform.y_axis == -Vector4::Y
        && transform.z_axis == Vector4::Z
        && transform.w_axis.z == 0.0
        && transform.w_axis.w == 1.0
        && tint == [1.0; 4]
    {
        let translate_x = transform.w_axis.x;
        let translate_y = transform.w_axis.y;
        out.extend(vertices.iter().map(|vertex| MeshVertex {
            pos: [vertex.pos[0] + translate_x, translate_y - vertex.pos[1]],
            color: vertex.color,
        }));
        return;
    }

    for vertex in vertices {
        let pos = *transform * Vector4::new(vertex.pos[0], vertex.pos[1], 0.0, 1.0);
        out.push(MeshVertex {
            pos: [pos.x, pos.y],
            color: [
                vertex.color[0] * tint[0],
                vertex.color[1] * tint[1],
                vertex.color[2] * tint[2],
                vertex.color[3] * tint[3],
            ],
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn prepare_tmesh_batch<EnsureCached>(
    objects: &[RenderObject],
    scratch: &mut DrawScratch,
    object_start: u32,
    object_count: u32,
    ensure_cached_tmesh: &mut EnsureCached,
    last_tmesh_geom: &mut Option<FrameTMeshGeom>,
) where
    EnsureCached: FnMut(TMeshCacheKey, &[TexturedMeshVertex]) -> bool,
{
    let object_end = (object_start + object_count) as usize;
    let object = &objects[object_start as usize];
    let ObjectType::TexturedMesh {
        vertices,
        geom_cache_key,
        depth_test,
        ..
    } = &object.object_type
    else {
        debug_assert!(false, "textured-mesh batch starts with another object type");
        return;
    };
    let source = tmesh_source(
        scratch,
        vertices,
        *geom_cache_key,
        ensure_cached_tmesh,
        last_tmesh_geom,
    );
    let instance_start = scratch.tmesh_instances.len() as u32;
    for object in &objects[object_start as usize..object_end] {
        let ObjectType::TexturedMesh { instance, .. } = &object.object_type else {
            debug_assert!(false, "textured-mesh batch contains another object type");
            continue;
        };
        scratch.tmesh_instances.push(*instance);
    }
    scratch.ops.push(DrawOp::TexturedMesh(TexturedMeshRun {
        source,
        instance_start,
        instance_count: object_count,
        blend: object.blend,
        texture_handle: object.texture_handle,
        camera: object.camera,
        depth_test: *depth_test,
    }));
}

fn tmesh_source<EnsureCached>(
    scratch: &mut DrawScratch,
    vertices: &TexturedMeshVertices,
    geom_cache_key: TMeshCacheKey,
    ensure_cached_tmesh: &mut EnsureCached,
    last_tmesh_geom: &mut Option<FrameTMeshGeom>,
) -> TexturedMeshSource
where
    EnsureCached: FnMut(TMeshCacheKey, &[TexturedMeshVertex]) -> bool,
{
    if geom_cache_key != INVALID_TMESH_CACHE_KEY {
        let key = TMeshGeomKey::Cached(geom_cache_key);
        if let Some(geom) = *last_tmesh_geom
            && geom.key == key
        {
            return geom.source;
        }
        if ensure_cached_tmesh(geom_cache_key, vertices.as_ref()) {
            let source = TexturedMeshSource::Cached {
                cache_key: geom_cache_key,
                vertex_count: vertices.len() as u32,
            };
            *last_tmesh_geom = Some(FrameTMeshGeom { key, source });
            return source;
        }
    }
    match vertices {
        TexturedMeshVertices::Transient(vertices) => {
            *last_tmesh_geom = None;
            transient_tmesh_source(scratch, vertices.as_slice())
        }
        TexturedMeshVertices::Shared(vertices) => {
            shared_tmesh_source(scratch, vertices.as_ref(), last_tmesh_geom)
        }
        TexturedMeshVertices::Reusable(vertices) => {
            shared_tmesh_source(scratch, vertices.as_slice(), last_tmesh_geom)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CameraUploadCache, DrawOp, DrawScratch, TexturedMeshSource, append_mesh_vertices, prepare,
    };
    use crate::{
        BlendMode, INVALID_TMESH_CACHE_KEY, MeshVertex, MeshVertices, ObjectType, RenderList,
        RenderObject, SpriteInstanceRaw, TexturedMeshInstanceRaw, TexturedMeshVertex,
        TexturedMeshVertices,
    };
    use glam::{Mat4 as Matrix4, Vec3, Vec4 as Vector4};
    use std::sync::Arc;

    fn sprite_object(order: u32) -> RenderObject {
        RenderObject {
            object_type: ObjectType::Sprite(order),
            texture_handle: u64::from(order) + 1,
            blend: BlendMode::Alpha,
            z: 0,
            order,
            camera: 0,
        }
    }

    #[test]
    fn prepare_reserves_ops_for_batch_count() {
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
            batches: Vec::new(),
        };
        let mut render_list = render_list;
        crate::build_render_batches(&render_list.objects, &mut render_list.batches);
        let mut scratch = DrawScratch::with_capacity(0, 0, 0, 100);

        prepare(&render_list, &mut scratch, |_, _| false);

        assert!(scratch.ops.capacity() >= render_list.batches.len());
    }

    #[test]
    fn textured_mesh_sources_compare_gpu_buffer_identity() {
        let transient_a = TexturedMeshSource::Transient {
            vertex_start: 0,
            vertex_count: 48,
            geom_key: 48,
        };
        let transient_b = TexturedMeshSource::Transient {
            vertex_start: 96,
            vertex_count: 72,
            geom_key: (96_u64 << 32) | 72,
        };
        let cached_a = TexturedMeshSource::Cached {
            cache_key: 41,
            vertex_count: 48,
        };
        let cached_a_other_range = TexturedMeshSource::Cached {
            cache_key: 41,
            vertex_count: 96,
        };
        let cached_b = TexturedMeshSource::Cached {
            cache_key: 42,
            vertex_count: 48,
        };

        assert!(transient_a.shares_vertex_buffer(transient_b));
        assert!(cached_a.shares_vertex_buffer(cached_a_other_range));
        assert!(!transient_a.shares_vertex_buffer(cached_a));
        assert!(!cached_a.shares_vertex_buffer(cached_b));
    }

    #[test]
    fn camera_upload_cache_only_updates_when_camera_changes() {
        let mut cache = CameraUploadCache::default();

        assert!(cache.update_required(0));
        assert!(!cache.update_required(0));
        assert!(cache.update_required(1));
        assert!(!cache.update_required(1));
        assert!(cache.update_required(0));
    }

    #[test]
    fn camera_upload_caches_preserve_each_programs_state() {
        let mut programs = [CameraUploadCache::default(); 3];

        assert!(programs[0].update_required(0));
        assert!(programs[1].update_required(0));
        assert!(programs[2].update_required(0));
        assert!(!programs[0].update_required(0));
        assert!(!programs[1].update_required(0));
        assert!(!programs[2].update_required(0));
    }

    #[test]
    fn prepare_copies_reusable_geometry_once_for_multiple_passes() {
        let vertices = Arc::new(vec![TexturedMeshVertex::default(); 6]);
        let objects = (0..2)
            .map(|order| RenderObject {
                object_type: ObjectType::TexturedMesh {
                    instance: TexturedMeshInstanceRaw::new(
                        Matrix4::IDENTITY,
                        [1.0; 4],
                        [1.0; 2],
                        [0.0; 2],
                        [0.0; 2],
                        order != 0,
                    ),
                    vertices: TexturedMeshVertices::Reusable(Arc::clone(&vertices)),
                    geom_cache_key: INVALID_TMESH_CACHE_KEY,
                    depth_test: true,
                },
                texture_handle: 1,
                blend: BlendMode::Alpha,
                z: 0,
                order,
                camera: 0,
            })
            .collect();
        let render_list = RenderList {
            clear_color: [0.0, 0.0, 0.0, 1.0],
            cameras: vec![Matrix4::IDENTITY],
            sprite_instances: Vec::new(),
            objects,
            batches: Vec::new(),
        };
        let mut render_list = render_list;
        crate::build_render_batches(&render_list.objects, &mut render_list.batches);
        let mut scratch = DrawScratch::with_capacity(0, 0, 0, 2);

        prepare(&render_list, &mut scratch, |_, _| false);

        assert_eq!(scratch.tmesh_vertices.len(), vertices.len());
        assert_eq!(scratch.tmesh_instances.len(), 2);
        assert_eq!(scratch.ops.len(), 1);
    }

    #[test]
    fn prepare_gameplay_mesh_preserves_flipped_positions_and_colors() {
        let vertices = Arc::new(vec![
            MeshVertex {
                pos: [3.0, 7.0],
                color: [0.25, 0.5, 0.75, 1.0],
            },
            MeshVertex {
                pos: [-2.0, 11.0],
                color: [1.0, 0.75, 0.5, 0.25],
            },
        ]);
        let transform = Matrix4::from_translation(Vec3::new(40.0, 90.0, 0.0))
            * Matrix4::from_scale(Vec3::new(1.0, -1.0, 1.0));
        let mut render_list = RenderList {
            clear_color: [0.0, 0.0, 0.0, 1.0],
            cameras: vec![Matrix4::IDENTITY],
            sprite_instances: Vec::new(),
            objects: vec![RenderObject {
                object_type: ObjectType::Mesh {
                    transform,
                    tint: [1.0; 4],
                    vertices: MeshVertices::Reusable(Arc::clone(&vertices)),
                },
                texture_handle: 0,
                blend: BlendMode::Alpha,
                z: 5,
                order: 0,
                camera: 0,
            }],
            batches: Vec::new(),
        };
        crate::build_render_batches(&render_list.objects, &mut render_list.batches);
        let mut scratch = DrawScratch::default();

        prepare(&render_list, &mut scratch, |_, _| false);

        assert_eq!(scratch.mesh_vertices.len(), 2);
        assert_eq!(scratch.mesh_vertices[0].pos, [43.0, 83.0]);
        assert_eq!(scratch.mesh_vertices[0].color, vertices[0].color);
        assert_eq!(scratch.mesh_vertices[1].pos, [38.0, 79.0]);
        assert_eq!(scratch.mesh_vertices[1].color, vertices[1].color);
        assert!(matches!(
            scratch.ops.as_slice(),
            [DrawOp::Mesh(run)] if run.vertex_start == 0 && run.vertex_count == 2
        ));
    }

    #[test]
    fn mesh_fast_path_falls_back_for_general_transform_and_tint() {
        let vertices = [
            MeshVertex {
                pos: [3.0, 7.0],
                color: [0.25, 0.5, 0.75, 1.0],
            },
            MeshVertex {
                pos: [-2.0, 11.0],
                color: [1.0, 0.75, 0.5, 0.25],
            },
        ];
        let transform =
            Matrix4::from_rotation_z(0.35) * Matrix4::from_translation(Vec3::new(40.0, 90.0, 0.0));
        let tint = [0.5, 0.25, 0.75, 0.8];
        let mut actual = Vec::new();

        append_mesh_vertices(&mut actual, &transform, tint, &vertices);

        for (actual, source) in actual.iter().zip(vertices) {
            let expected_pos = transform * Vector4::new(source.pos[0], source.pos[1], 0.0, 1.0);
            assert_eq!(actual.pos, [expected_pos.x, expected_pos.y]);
            assert_eq!(
                actual.color,
                [
                    source.color[0] * tint[0],
                    source.color[1] * tint[1],
                    source.color[2] * tint[2],
                    source.color[3] * tint[3],
                ]
            );
        }
    }

    #[test]
    fn prepare_consumes_batches_in_z_then_order_sequence() {
        let specs = [(5, 2, 30), (4, 0, 10), (5, 1, 20)];
        let objects = specs
            .iter()
            .enumerate()
            .map(|(index, &(z, order, texture_handle))| RenderObject {
                object_type: ObjectType::Sprite(index as u32),
                texture_handle,
                blend: BlendMode::Alpha,
                z,
                order,
                camera: 0,
            })
            .collect::<Vec<_>>();
        let mut render_list = RenderList {
            clear_color: [0.0, 0.0, 0.0, 1.0],
            cameras: vec![Matrix4::IDENTITY],
            sprite_instances: Vec::new(),
            objects,
            batches: Vec::new(),
        };
        crate::build_render_batches(&render_list.objects, &mut render_list.batches);
        let mut scratch = DrawScratch::default();

        prepare(&render_list, &mut scratch, |_, _| false);

        let textures = scratch
            .ops
            .iter()
            .map(|op| match op {
                super::DrawOp::Sprite(run) => run.texture_handle,
                _ => panic!("expected sprite draw run"),
            })
            .collect::<Vec<_>>();
        assert_eq!(textures, vec![10, 20, 30]);
    }
}
