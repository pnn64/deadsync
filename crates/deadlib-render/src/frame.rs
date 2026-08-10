use crate::{
    BlendMode, INVALID_TMESH_CACHE_KEY, MeshVertex, SpriteInstanceRaw, TMeshCacheKey,
    TextureHandle, TexturedMeshInstanceRaw, TexturedMeshVertex, TexturedMeshVertices,
};
use glam::Mat4;

/// Complete backend-neutral output of one presentation pass.
///
/// Geometry, instances, and commands are already in final painter order. GPU
/// backends only resolve retained textured geometry to cached or frame-local
/// storage before executing `ops`; they do not reconstruct draw commands.
#[derive(Clone)]
pub struct RenderFrame {
    pub clear_color: [f32; 4],
    pub cameras: Vec<Mat4>,
    pub sprite_instances: Vec<SpriteInstanceRaw>,
    pub mesh_vertices: Vec<MeshVertex>,
    pub tmesh_instances: Vec<TexturedMeshInstanceRaw>,
    pub tmesh_geometries: Vec<TexturedMeshGeometry>,
    pub ops: Vec<DrawOp>,
}

#[derive(Clone)]
pub struct TexturedMeshGeometry {
    pub vertices: TexturedMeshVertices,
    pub cache_key: TMeshCacheKey,
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
    pub blend: BlendMode,
    pub camera: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TexturedMeshRun {
    pub geometry: u32,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TexturedMeshSource {
    buffer_key: u64,
    vertex_start: u32,
    vertex_count: u32,
}

impl TexturedMeshSource {
    #[inline(always)]
    pub const fn transient(vertex_start: u32, vertex_count: u32) -> Self {
        Self {
            buffer_key: INVALID_TMESH_CACHE_KEY,
            vertex_start,
            vertex_count,
        }
    }

    #[inline(always)]
    pub const fn cached(buffer_key: u64, vertex_count: u32) -> Self {
        debug_assert!(buffer_key != INVALID_TMESH_CACHE_KEY);
        Self {
            buffer_key,
            vertex_start: 0,
            vertex_count,
        }
    }

    #[inline(always)]
    pub const fn vertex_start(self) -> u32 {
        self.vertex_start
    }

    #[inline(always)]
    pub const fn vertex_count(self) -> u32 {
        self.vertex_count
    }

    #[inline(always)]
    /// Returns the backend-local identity of retained GPU storage.
    ///
    /// This is not the presentation geometry's cache key. Backends may return
    /// a dense slot or another non-zero identity that makes recording cheap.
    pub const fn buffer_key(self) -> Option<u64> {
        if self.buffer_key == INVALID_TMESH_CACHE_KEY {
            None
        } else {
            Some(self.buffer_key)
        }
    }

    #[inline(always)]
    pub const fn shares_vertex_buffer(self, other: Self) -> bool {
        self.buffer_key == other.buffer_key
    }
}

/// Tracks the textured-mesh vertex buffer currently bound by a backend.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TexturedMeshBufferCache {
    last_source: Option<TexturedMeshSource>,
}

impl TexturedMeshBufferCache {
    #[inline(always)]
    pub fn update_required(&mut self, source: TexturedMeshSource) -> bool {
        if self
            .last_source
            .is_some_and(|last| last.shares_vertex_buffer(source))
        {
            false
        } else {
            self.last_source = Some(source);
            true
        }
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        self.last_source = None;
    }
}

/// Backend-owned resolution storage for retained textured geometry.
///
/// Owner: one GPU backend on the render thread. Lifetime: session. Capacity is
/// warmed by ordinary frames and retained between frames. Each frame clears the
/// lengths, resolves every distinct geometry once, and uploads only entries not
/// already present in the backend's bounded retained cache. There is no
/// eviction or destruction here; those policies remain backend-owned.
#[derive(Debug, Default)]
pub struct TexturedMeshUploads {
    pub vertices: Vec<TexturedMeshVertex>,
    pub sources: Vec<TexturedMeshSource>,
}

impl TexturedMeshUploads {
    pub fn with_capacity(vertices: usize, geometries: usize) -> Self {
        Self {
            vertices: Vec::with_capacity(vertices),
            sources: Vec::with_capacity(geometries),
        }
    }

    #[inline(always)]
    pub fn source(&self, geometry: u32) -> Option<TexturedMeshSource> {
        self.sources.get(geometry as usize).copied()
    }
}

/// Resolves frame geometry to retained or frame-local upload storage.
///
/// `ensure_cached` returns a non-zero backend-local buffer identity when the
/// geometry is retained. The same identity must always refer to the same GPU
/// buffer for the lifetime of `uploads`' consumer.
pub fn resolve_textured_meshes<EnsureCached>(
    frame: &RenderFrame,
    uploads: &mut TexturedMeshUploads,
    mut ensure_cached: EnsureCached,
) where
    EnsureCached: FnMut(TMeshCacheKey, &[TexturedMeshVertex]) -> Option<u64>,
{
    uploads.vertices.clear();
    uploads.sources.clear();
    if uploads.sources.capacity() < frame.tmesh_geometries.len() {
        uploads.sources.reserve(frame.tmesh_geometries.len());
    }

    for geometry in &frame.tmesh_geometries {
        let vertices = geometry.vertices.as_ref();
        if geometry.cache_key != INVALID_TMESH_CACHE_KEY
            && let Some(buffer_key) = ensure_cached(geometry.cache_key, vertices)
        {
            uploads.sources.push(TexturedMeshSource::cached(
                buffer_key,
                saturating_u32(vertices.len()),
            ));
            continue;
        }

        let vertex_start = saturating_u32(uploads.vertices.len());
        uploads.vertices.extend_from_slice(vertices);
        uploads.sources.push(TexturedMeshSource::transient(
            vertex_start,
            saturating_u32(vertices.len()),
        ));
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

pub const DRAW_STORAGE_SLOTS: usize = 8;
pub const SOFTWARE_OBJECTS_STORAGE_SLOT: usize = 5;
pub const SOFTWARE_MESH_STORAGE_SLOT: usize = 6;
pub const SOFTWARE_TMESH_STORAGE_SLOT: usize = 7;
pub const DRAW_STORAGE_NAMES: [&str; DRAW_STORAGE_SLOTS] = [
    "frame_mesh",
    "tmesh_upload",
    "frame_tmesh_inst",
    "frame_ops",
    "tmesh_sources",
    "software_objects",
    "software_mesh",
    "software_tmesh",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawStorageStats {
    pub capacities: [u32; DRAW_STORAGE_SLOTS],
}

pub fn draw_storage_stats(
    frame: &RenderFrame,
    uploads: Option<&TexturedMeshUploads>,
) -> DrawStorageStats {
    let mut capacities = [0; DRAW_STORAGE_SLOTS];
    capacities[0] = saturating_u32(frame.mesh_vertices.capacity());
    capacities[2] = saturating_u32(frame.tmesh_instances.capacity());
    capacities[3] = saturating_u32(frame.ops.capacity());
    capacities[4] = saturating_u32(frame.tmesh_geometries.capacity());
    if let Some(uploads) = uploads {
        capacities[1] = saturating_u32(uploads.vertices.capacity());
        capacities[4] = capacities[4].max(saturating_u32(uploads.sources.capacity()));
    }
    DrawStorageStats { capacities }
}

#[inline(always)]
const fn saturating_u32(value: usize) -> u32 {
    if value > u32::MAX as usize {
        u32::MAX
    } else {
        value as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn vertex(x: f32) -> TexturedMeshVertex {
        TexturedMeshVertex {
            pos: [x, 0.0, 0.0],
            ..TexturedMeshVertex::default()
        }
    }

    fn frame(geometries: Vec<TexturedMeshGeometry>) -> RenderFrame {
        RenderFrame {
            clear_color: [0.0; 4],
            cameras: Vec::new(),
            sprite_instances: Vec::new(),
            mesh_vertices: Vec::new(),
            tmesh_instances: Vec::new(),
            tmesh_geometries: geometries,
            ops: Vec::new(),
        }
    }

    #[test]
    fn textured_mesh_buffer_cache_rebinds_only_when_storage_changes() {
        let transient_a = TexturedMeshSource::transient(0, 6);
        let transient_b = TexturedMeshSource::transient(6, 12);
        let cached_a = TexturedMeshSource::cached(11, 6);
        let cached_a_again = TexturedMeshSource::cached(11, 12);
        let cached_b = TexturedMeshSource::cached(12, 6);
        let mut cache = TexturedMeshBufferCache::default();

        assert!(cache.update_required(transient_a));
        assert!(!cache.update_required(transient_b));
        assert!(cache.update_required(cached_a));
        assert!(!cache.update_required(cached_a_again));
        assert!(cache.update_required(cached_b));
        cache.reset();
        assert!(cache.update_required(cached_b));
    }

    #[test]
    fn textured_mesh_resolution_uploads_only_cache_misses() {
        let frame = frame(vec![
            TexturedMeshGeometry {
                vertices: TexturedMeshVertices::Shared(Arc::from([vertex(1.0), vertex(2.0)])),
                cache_key: 7,
            },
            TexturedMeshGeometry {
                vertices: TexturedMeshVertices::Transient(vec![vertex(3.0)]),
                cache_key: INVALID_TMESH_CACHE_KEY,
            },
            TexturedMeshGeometry {
                vertices: TexturedMeshVertices::Shared(Arc::from([vertex(4.0), vertex(5.0)])),
                cache_key: 9,
            },
        ]);
        let mut uploads = TexturedMeshUploads::default();

        resolve_textured_meshes(&frame, &mut uploads, |key, _| (key == 7).then_some(70));

        assert_eq!(uploads.sources.len(), 3);
        assert_eq!(uploads.sources[0].buffer_key(), Some(70));
        assert_eq!(uploads.sources[0].vertex_count(), 2);
        assert_eq!(uploads.sources[1].buffer_key(), None);
        assert_eq!(uploads.sources[1].vertex_start(), 0);
        assert_eq!(uploads.sources[1].vertex_count(), 1);
        assert_eq!(uploads.sources[2].buffer_key(), None);
        assert_eq!(uploads.sources[2].vertex_start(), 1);
        assert_eq!(uploads.sources[2].vertex_count(), 2);
        assert_eq!(
            uploads
                .vertices
                .iter()
                .map(|vertex| vertex.pos[0])
                .collect::<Vec<_>>(),
            vec![3.0, 4.0, 5.0]
        );
    }

    #[test]
    fn textured_mesh_resolution_reuses_and_clears_storage() {
        let mut uploads = TexturedMeshUploads::with_capacity(8, 4);
        let populated = frame(vec![TexturedMeshGeometry {
            vertices: TexturedMeshVertices::Transient(vec![vertex(1.0), vertex(2.0)]),
            cache_key: INVALID_TMESH_CACHE_KEY,
        }]);
        resolve_textured_meshes(&populated, &mut uploads, |_, _| None);
        let vertex_capacity = uploads.vertices.capacity();
        let source_capacity = uploads.sources.capacity();

        resolve_textured_meshes(&frame(Vec::new()), &mut uploads, |_, _| None);

        assert!(uploads.vertices.is_empty());
        assert!(uploads.sources.is_empty());
        assert_eq!(uploads.vertices.capacity(), vertex_capacity);
        assert_eq!(uploads.sources.capacity(), source_capacity);
    }
}
