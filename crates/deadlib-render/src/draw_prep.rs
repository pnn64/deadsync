use crate::{
    BlendMode, FastTMeshMap, INVALID_TEXTURE_HANDLE, MeshVertex, ObjectType, RenderList,
    RetainedTMeshGeometry, SpriteInstanceRaw, TMeshGeometryId, TextureHandle,
    TexturedMeshInstanceRaw, TexturedMeshVertex, TexturedMeshVertices,
};
use glam::{Mat4 as Matrix4, Vec4 as Vector4};
use std::{
    collections::HashMap,
    hash::BuildHasherDefault,
    ops::{Deref, DerefMut},
};
use twox_hash::XxHash64;

type TMeshHasher = BuildHasherDefault<XxHash64>;
type TMeshGeomMap = HashMap<TMeshGeomKey, FrameTMeshGeom, TMeshHasher>;
type CachedTMeshMap = FastTMeshMap<bool>;

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
        geometry_id: TMeshGeometryId,
        vertex_count: u32,
    },
}

/// Result of asking a backend to retain immutable textured-mesh geometry.
///
/// `Resident` and `Uploaded` both allow the prepared run to reference the
/// backend cache. Only `Uploaded` contributes to `cached_upload_vertices`;
/// `CapacityExceeded` and `IdentityMismatch` distinguish bounded saturation
/// from a violated cache contract. `UploadFailed` remains unavailable but
/// retains attempted-upload timing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TMeshCacheResult {
    Resident,
    Uploaded,
    CapacityExceeded,
    IdentityMismatch,
    UploadFailed,
}

/// Outcome counters for one bounded retained-geometry prewarm pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TMeshPrewarmStats {
    pub requested: u32,
    pub resident: u32,
    pub uploaded: u32,
    pub unavailable: u32,
    pub capacity_exceeded: u32,
    pub identity_mismatch: u32,
    pub upload_failed: u32,
    pub uploaded_vertices: u64,
}

impl TMeshPrewarmStats {
    #[inline(always)]
    pub const fn ready(self) -> bool {
        self.requested == self.resident.saturating_add(self.uploaded)
    }

    #[inline]
    pub fn record(&mut self, result: TMeshCacheResult, vertex_count: usize) {
        self.requested = self.requested.saturating_add(1);
        match result {
            TMeshCacheResult::CapacityExceeded => {
                self.unavailable = self.unavailable.saturating_add(1);
                self.capacity_exceeded = self.capacity_exceeded.saturating_add(1);
            }
            TMeshCacheResult::IdentityMismatch => {
                self.unavailable = self.unavailable.saturating_add(1);
                self.identity_mismatch = self.identity_mismatch.saturating_add(1);
            }
            TMeshCacheResult::Resident => {
                self.resident = self.resident.saturating_add(1);
            }
            TMeshCacheResult::Uploaded => {
                self.uploaded = self.uploaded.saturating_add(1);
                self.uploaded_vertices = self.uploaded_vertices.saturating_add(vertex_count as u64);
            }
            TMeshCacheResult::UploadFailed => {
                self.upload_failed = self.upload_failed.saturating_add(1);
            }
        }
    }
}

impl TMeshCacheResult {
    #[inline(always)]
    pub const fn is_available(self) -> bool {
        matches!(self, Self::Resident | Self::Uploaded)
    }

    #[inline(always)]
    pub const fn was_uploaded(self) -> bool {
        matches!(self, Self::Uploaded)
    }

    #[inline(always)]
    pub const fn upload_attempted(self) -> bool {
        matches!(self, Self::Uploaded | Self::UploadFailed)
    }
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameCapacity {
    pub cameras: usize,
    pub sprite_instances: usize,
    pub mesh_vertices: usize,
    pub tmesh_vertices: usize,
    pub tmesh_instances: usize,
    pub ops: usize,
}

impl FrameCapacity {
    #[inline]
    pub fn growth_events(self, after: Self) -> u32 {
        [
            after.cameras > self.cameras,
            after.sprite_instances > self.sprite_instances,
            after.mesh_vertices > self.mesh_vertices,
            after.tmesh_vertices > self.tmesh_vertices,
            after.tmesh_instances > self.tmesh_instances,
            after.ops > self.ops,
        ]
        .into_iter()
        .map(u32::from)
        .sum()
    }
}

/// Persistent, renderer-owned storage for one flat frame.
///
/// The render/logic thread owns this value for a screen or song and calls
/// [`DrawFrame::begin`] before each emission. Buffers retain capacity across
/// frames; callers reserve their measured worst case during transition warmup.
/// Emission itself is single-threaded and performs linear appends. This type
/// does no lookup, eviction, pruning, or resource destruction.
#[derive(Debug, Default)]
pub struct DrawFrame {
    pub clear_color: [f32; 4],
    /// Semantic primitives emitted before adjacent runs were batched.
    pub render_objects: u32,
    pub cameras: Vec<Matrix4>,
    pub sprite_instances: Vec<SpriteInstanceRaw>,
    pub mesh_vertices: Vec<MeshVertex>,
    pub tmesh_vertices: Vec<TexturedMeshVertex>,
    pub tmesh_instances: Vec<TexturedMeshInstanceRaw>,
    pub ops: Vec<DrawOp>,
    retained_tmeshes: Vec<RetainedTMeshGeometry>,
}

impl DrawFrame {
    pub fn with_capacity(capacity: FrameCapacity) -> Self {
        Self {
            clear_color: [0.0; 4],
            render_objects: 0,
            cameras: Vec::with_capacity(capacity.cameras),
            sprite_instances: Vec::with_capacity(capacity.sprite_instances),
            mesh_vertices: Vec::with_capacity(capacity.mesh_vertices),
            tmesh_vertices: Vec::with_capacity(capacity.tmesh_vertices),
            tmesh_instances: Vec::with_capacity(capacity.tmesh_instances),
            ops: Vec::with_capacity(capacity.ops),
            retained_tmeshes: Vec::new(),
        }
    }

    #[inline]
    pub fn begin(&mut self, clear_color: [f32; 4]) {
        self.clear_color = clear_color;
        self.render_objects = 0;
        self.cameras.clear();
        self.sprite_instances.clear();
        self.mesh_vertices.clear();
        self.tmesh_vertices.clear();
        self.tmesh_instances.clear();
        self.ops.clear();
        self.retained_tmeshes.clear();
    }

    pub fn reserve(&mut self, capacity: FrameCapacity) {
        reserve_to(&mut self.cameras, capacity.cameras);
        reserve_to(&mut self.sprite_instances, capacity.sprite_instances);
        reserve_to(&mut self.mesh_vertices, capacity.mesh_vertices);
        reserve_to(&mut self.tmesh_vertices, capacity.tmesh_vertices);
        reserve_to(&mut self.tmesh_instances, capacity.tmesh_instances);
        reserve_to(&mut self.ops, capacity.ops);
    }

    #[inline]
    pub fn capacity(&self) -> FrameCapacity {
        FrameCapacity {
            cameras: self.cameras.capacity(),
            sprite_instances: self.sprite_instances.capacity(),
            mesh_vertices: self.mesh_vertices.capacity(),
            tmesh_vertices: self.tmesh_vertices.capacity(),
            tmesh_instances: self.tmesh_instances.capacity(),
            ops: self.ops.capacity(),
        }
    }

    /// Binds the immutable geometry owners referenced by this frame's cached
    /// textured-mesh ops. Compiled presentation calls this once on its cold
    /// path; backend preparation consumes the same frame and sidecar set.
    #[inline]
    pub fn set_retained_tmeshes(&mut self, geometries: Vec<RetainedTMeshGeometry>) {
        self.retained_tmeshes = geometries;
    }

    #[inline(always)]
    pub fn retained_tmeshes(&self) -> &[RetainedTMeshGeometry] {
        self.retained_tmeshes.as_slice()
    }

    /// Describes an already-flat direct frame without traversing its draw ops.
    /// Backends add per-kind run counts while recording the existing op loop.
    #[inline]
    pub fn prepare_stats(&self) -> FramePrepareStats {
        FramePrepareStats {
            dynamic_upload_vertices: self.tmesh_vertices.len() as u64,
            render_objects: self.render_objects,
            sprite_instances: saturating_u32(self.sprite_instances.len()),
            mesh_vertices: saturating_u32(self.mesh_vertices.len()),
            tmesh_vertices: saturating_u32(self.tmesh_vertices.len()),
            tmesh_instances: saturating_u32(self.tmesh_instances.len()),
            draw_ops: saturating_u32(self.ops.len()),
            mesh_vertex_capacity: self.mesh_vertices.capacity(),
            tmesh_vertex_capacity: self.tmesh_vertices.capacity(),
            tmesh_instance_capacity: self.tmesh_instances.capacity(),
            op_capacity: self.ops.capacity(),
            ..FramePrepareStats::default()
        }
    }

    #[inline]
    pub fn view(&self) -> DrawFrameView<'_> {
        DrawFrameView {
            clear_color: self.clear_color,
            cameras: self.cameras.as_slice(),
            sprite_instances: self.sprite_instances.as_slice(),
            mesh_vertices: self.mesh_vertices.as_slice(),
            tmesh_vertices: self.tmesh_vertices.as_slice(),
            tmesh_instances: self.tmesh_instances.as_slice(),
            ops: self.ops.as_slice(),
        }
    }
}

impl AsRef<DrawFrame> for DrawFrame {
    #[inline(always)]
    fn as_ref(&self) -> &DrawFrame {
        self
    }
}

#[inline]
fn reserve_to<T>(values: &mut Vec<T>, capacity: usize) {
    if values.capacity() < capacity {
        values.reserve(capacity - values.len());
    }
}

/// Borrowed backend input shared by direct frames and legacy `RenderList`
/// preparation. Every run in `ops` indexes the corresponding slices here.
#[derive(Clone, Copy, Debug)]
pub struct DrawFrameView<'a> {
    pub clear_color: [f32; 4],
    pub cameras: &'a [Matrix4],
    pub sprite_instances: &'a [SpriteInstanceRaw],
    pub mesh_vertices: &'a [MeshVertex],
    pub tmesh_vertices: &'a [TexturedMeshVertex],
    pub tmesh_instances: &'a [TexturedMeshInstanceRaw],
    pub ops: &'a [DrawOp],
}

/// Persistent adapter scratch for the legacy `RenderList` path.
///
/// A backend owns one value on its render thread for the backend lifetime.
/// `with_capacity` is its warmup point; vectors and hash-table buckets are
/// retained thereafter. A miss transforms/copies dynamic geometry or invokes
/// the caller's bounded static-geometry upload hook. Frame-local map entries
/// are cleared without releasing buckets, and all storage is destroyed with
/// the backend. [`FramePrepareStats`] reports exact output and growth counts;
/// worst-case preparation is linear in render objects plus emitted vertices.
#[derive(Debug, Default)]
pub struct DrawScratch {
    frame: DrawFrame,
    shared_tmesh_geom: TMeshGeomMap,
    cached_tmesh: CachedTMeshMap,
}

impl Deref for DrawScratch {
    type Target = DrawFrame;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.frame
    }
}

impl DerefMut for DrawScratch {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.frame
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FramePrepareStats {
    pub dynamic_upload_vertices: u64,
    pub cached_upload_vertices: u64,
    pub render_objects: u32,
    pub sprite_instances: u32,
    pub mesh_vertices: u32,
    pub tmesh_vertices: u32,
    pub tmesh_instances: u32,
    pub draw_ops: u32,
    pub sprite_runs: u32,
    pub mesh_runs: u32,
    pub tmesh_runs: u32,
    pub scratch_growth_events: u32,
    pub mesh_vertex_capacity: usize,
    pub tmesh_vertex_capacity: usize,
    pub tmesh_instance_capacity: usize,
    pub op_capacity: usize,
    pub shared_tmesh_capacity: usize,
    pub cached_tmesh_capacity: usize,
}

/// Backwards-compatible name retained for existing backend callers.
pub type PrepareStats = FramePrepareStats;

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

#[derive(Clone, Copy, Debug, Default)]
struct DrawScratchCapacity {
    frame: FrameCapacity,
    shared_tmesh: usize,
    cached_tmesh: usize,
}

impl DrawScratchCapacity {
    #[inline]
    fn capture(scratch: &DrawScratch) -> Self {
        Self {
            frame: scratch.frame.capacity(),
            shared_tmesh: scratch.shared_tmesh_geom.capacity(),
            cached_tmesh: scratch.cached_tmesh.capacity(),
        }
    }

    #[inline]
    fn growth_events(self, after: Self) -> u32 {
        self.frame.growth_events(after.frame)
            + u32::from(after.shared_tmesh > self.shared_tmesh)
            + u32::from(after.cached_tmesh > self.cached_tmesh)
    }
}

#[inline]
fn saturating_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
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
            frame: DrawFrame::with_capacity(FrameCapacity {
                mesh_vertices,
                tmesh_vertices,
                tmesh_instances,
                ops,
                ..FrameCapacity::default()
            }),
            shared_tmesh_geom: HashMap::with_capacity_and_hasher(
                ops,
                BuildHasherDefault::default(),
            ),
            cached_tmesh: CachedTMeshMap::with_capacity_and_hasher(
                ops,
                BuildHasherDefault::default(),
            ),
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
    EnsureCached: FnMut(TMeshGeometryId, &[TexturedMeshVertex]) -> TMeshCacheResult,
{
    let objects_len = render_list.objects.len();
    let capacity_before = DrawScratchCapacity::capture(scratch);

    scratch.mesh_vertices.clear();
    scratch.tmesh_vertices.clear();
    scratch.tmesh_instances.clear();

    scratch.ops.clear();
    if scratch.ops.capacity() < objects_len {
        let additional = objects_len - scratch.ops.len();
        scratch.ops.reserve(additional);
    }
    debug_assert!(scratch.ops.capacity() >= objects_len);

    let mut stats = FramePrepareStats {
        render_objects: saturating_u32(objects_len),
        sprite_instances: saturating_u32(render_list.sprite_instances.len()),
        ..FramePrepareStats::default()
    };
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
                                stats.sprite_runs = stats.sprite_runs.saturating_add(1);
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
                    stats.sprite_runs = stats.sprite_runs.saturating_add(1);
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
                stats.mesh_runs = stats.mesh_runs.saturating_add(1);
                i += 1;
            }
            ObjectType::TexturedMesh {
                instance,
                vertices,
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
                let source = match vertices {
                    TexturedMeshVertices::Retained(geometry) => {
                        let geometry = geometry.as_ref();
                        let geometry_id = geometry.id();
                        if !cached_tmesh_cleared {
                            scratch.cached_tmesh.clear();
                            cached_tmesh_cleared = true;
                        }
                        let cached = if let Some(cached) = scratch.cached_tmesh.get(&geometry_id) {
                            *cached
                        } else {
                            let result = ensure_cached_tmesh(geometry_id, geometry.vertices());
                            let cached = result.is_available();
                            scratch.cached_tmesh.insert(geometry_id, cached);
                            if result.was_uploaded() {
                                stats.cached_upload_vertices = stats
                                    .cached_upload_vertices
                                    .saturating_add(geometry.vertices().len() as u64);
                            }
                            cached
                        };
                        if cached {
                            TexturedMeshSource::Cached {
                                geometry_id,
                                vertex_count: geometry.vertices().len() as u32,
                            }
                        } else {
                            shared_tmesh_source(
                                scratch,
                                geometry.vertices(),
                                &mut stats,
                                &mut shared_tmesh_geom_cleared,
                            )
                        }
                    }
                    TexturedMeshVertices::Transient(vertices) => {
                        transient_tmesh_source(scratch, vertices.as_slice(), &mut stats)
                    }
                    TexturedMeshVertices::Shared(vertices) => shared_tmesh_source(
                        scratch,
                        vertices.as_ref(),
                        &mut stats,
                        &mut shared_tmesh_geom_cleared,
                    ),
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
                stats.tmesh_runs = stats.tmesh_runs.saturating_add(1);
                i += 1;
            }
        }
    }

    let capacity_after = DrawScratchCapacity::capture(scratch);
    stats.mesh_vertices = saturating_u32(scratch.mesh_vertices.len());
    stats.tmesh_vertices = saturating_u32(scratch.tmesh_vertices.len());
    stats.tmesh_instances = saturating_u32(scratch.tmesh_instances.len());
    stats.draw_ops = saturating_u32(scratch.ops.len());
    stats.scratch_growth_events = capacity_before.growth_events(capacity_after);
    stats.mesh_vertex_capacity = capacity_after.frame.mesh_vertices;
    stats.tmesh_vertex_capacity = capacity_after.frame.tmesh_vertices;
    stats.tmesh_instance_capacity = capacity_after.frame.tmesh_instances;
    stats.op_capacity = capacity_after.frame.ops;
    stats.shared_tmesh_capacity = capacity_after.shared_tmesh;
    stats.cached_tmesh_capacity = capacity_after.cached_tmesh;

    stats
}

/// Prepares a legacy [`RenderList`] and exposes it through the direct-frame
/// backend contract without copying cameras or sprite instances.
///
/// The returned view borrows stable input slices from `render_list` and the
/// transformed geometry/run buffers from `scratch`. Its structure is therefore
/// identical to [`DrawFrame::view`] while preserving the legacy path's current
/// ownership and allocation behavior.
pub fn prepare_render_list<'a, EnsureCached>(
    render_list: &'a RenderList,
    scratch: &'a mut DrawScratch,
    ensure_cached_tmesh: EnsureCached,
) -> (DrawFrameView<'a>, PrepareStats)
where
    EnsureCached: FnMut(TMeshGeometryId, &[TexturedMeshVertex]) -> TMeshCacheResult,
{
    let stats = prepare(render_list, scratch, ensure_cached_tmesh);
    let view = DrawFrameView {
        clear_color: render_list.clear_color,
        cameras: render_list.cameras.as_slice(),
        sprite_instances: render_list.sprite_instances.as_slice(),
        mesh_vertices: scratch.frame.mesh_vertices.as_slice(),
        tmesh_vertices: scratch.frame.tmesh_vertices.as_slice(),
        tmesh_instances: scratch.frame.tmesh_instances.as_slice(),
        ops: scratch.frame.ops.as_slice(),
    };
    (view, stats)
}

#[cfg(test)]
mod tests {
    use super::{DrawFrame, DrawScratch, TMeshCacheResult, TMeshPrewarmStats, prepare};
    use crate::{
        BlendMode, INVALID_TEXTURE_HANDLE, ObjectType, RenderList, RenderObject,
        RetainedTMeshGeometry, SpriteInstanceRaw, TexturedMeshVertex,
    };
    use glam::Mat4 as Matrix4;
    use std::sync::Arc;

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

        prepare(&render_list, &mut scratch, |_, _| {
            TMeshCacheResult::CapacityExceeded
        });

        assert!(scratch.ops.capacity() >= render_list.objects.len());
    }

    #[test]
    fn tmesh_prewarm_stats_track_ready_and_failed_geometry() {
        let mut stats = TMeshPrewarmStats::default();
        assert!(stats.ready());

        stats.record(TMeshCacheResult::Resident, 3);
        stats.record(TMeshCacheResult::Uploaded, 7);
        assert!(stats.ready());
        assert_eq!(stats.uploaded_vertices, 7);

        stats.record(TMeshCacheResult::CapacityExceeded, 11);
        stats.record(TMeshCacheResult::IdentityMismatch, 11);
        stats.record(TMeshCacheResult::UploadFailed, 13);
        assert!(!stats.ready());
        assert_eq!(
            stats,
            TMeshPrewarmStats {
                requested: 5,
                resident: 1,
                uploaded: 1,
                unavailable: 2,
                capacity_exceeded: 1,
                identity_mismatch: 1,
                upload_failed: 1,
                uploaded_vertices: 7,
            }
        );
    }

    #[test]
    fn draw_frame_binds_and_clears_retained_geometry_sidecars() {
        let vertices = Arc::from([TexturedMeshVertex::default()]);
        let geometry = RetainedTMeshGeometry::new(1, vertices).expect("retained geometry");
        let mut frame = DrawFrame::default();

        frame.set_retained_tmeshes(vec![geometry]);
        assert_eq!(frame.retained_tmeshes().len(), 1);

        frame.begin([0.0, 0.0, 0.0, 1.0]);
        assert!(frame.retained_tmeshes().is_empty());
    }
}
