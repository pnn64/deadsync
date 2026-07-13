//! Immutable actor snapshots lowered once into flat renderer primitives.
//!
//! Compilation is cold screen/song-boundary work outside gameplay. Emission is
//! render-thread work: after [`CompiledSceneScratch::reserve`] it only copies
//! preordered POD values and clones shared geometry owners. It performs no actor
//! traversal, layout, texture lookup, text lookup, or sorting.

use crate::actors::{Actor, Background, SpriteSource, TextContent};
use crate::anim::EffectMode;
use crate::compose::{
    ComposeScratch, TextLayoutCache, build_screen_cached_with_scratch_and_texture_context,
};
use crate::font::{self, Font};
use crate::space::Metrics;
use crate::texture::TextureContext;
use deadlib_render::{
    CachedTMeshGeometry, DrawFrame as RenderDrawFrame, FastU64Map,
    FrameCapacity as DrawFrameCapacity, FramePrepareStats, INVALID_TEXTURE_HANDLE,
    INVALID_TMESH_CACHE_KEY, ObjectType, RenderList, RenderObject, SpriteInstanceRaw,
    TMeshCacheKey, TexturedMeshVertices,
    draw_prep::{DrawScratch, prepare_render_list},
    tmesh_fingerprint,
};
use glam::Mat4;
use std::collections::HashMap;
use std::fmt;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

static NEXT_SCENE_ID: AtomicU64 = AtomicU64::new(1);

/// Identifies one lowered render primitive for one compiled-scene generation.
///
/// IDs are dense indices in final draw order. A typed slot additionally carries
/// the scene generation so it cannot silently target a recompiled scene.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

/// Typed handle for patching one compiled sprite's UV origin.
///
/// Slots are scene-local and become invalid when that scene is recompiled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SpriteUvSlot {
    scene_id: u64,
    node: NodeId,
    instance: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteUvPatch {
    pub slot: SpriteUvSlot,
    pub offset: [f32; 2],
}

/// Patches the complete affine UV rectangle state for a compiled sprite.
///
/// This is distinct from [`SpriteUvPatch`] because composing a rectangle stores
/// both its origin and the floating-point result of `end - origin`; even a
/// nominal unit span is not guaranteed to subtract to exactly `1.0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteUvRectPatch {
    pub slot: SpriteUvSlot,
    pub scale: [f32; 2],
    pub offset: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatchError {
    WrongScene(NodeId),
    UnknownNode(NodeId),
    NotSprite(NodeId),
    MissingSpriteInstance(NodeId),
}

impl fmt::Display for PatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongScene(id) => {
                write!(f, "compiled node {} belongs to another scene", id.0)
            }
            Self::UnknownNode(id) => write!(f, "compiled node {} does not exist", id.0),
            Self::NotSprite(id) => write!(f, "compiled node {} is not a sprite", id.0),
            Self::MissingSpriteInstance(id) => {
                write!(f, "compiled sprite node {} has no instance", id.0)
            }
        }
    }
}

impl std::error::Error for PatchError {}

/// Why an immutable scene cannot be used as the first root-level sprite
/// fragment of a mixed compiled/legacy frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RootPrefixError {
    Patch(PatchError),
    CameraCount {
        count: usize,
    },
    NonRootCamera {
        node: NodeId,
        camera: u8,
    },
    NonSpritePrimitive {
        node: NodeId,
    },
    InvalidSpriteInstance {
        node: NodeId,
        instance: u32,
    },
    NonDenseOrder {
        node: NodeId,
        order: u32,
        primitive_count: usize,
    },
}

impl fmt::Display for RootPrefixError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Patch(error) => error.fmt(f),
            Self::CameraCount { count } => write!(
                f,
                "compiled root prefix has {count} cameras; exactly one root camera is required"
            ),
            Self::NonRootCamera { node, camera } => write!(
                f,
                "compiled root-prefix node {} uses camera {camera}; only camera 0 is supported",
                node.0
            ),
            Self::NonSpritePrimitive { node } => write!(
                f,
                "compiled root-prefix node {} is not a sprite primitive",
                node.0
            ),
            Self::InvalidSpriteInstance { node, instance } => write!(
                f,
                "compiled root-prefix node {} references missing sprite instance {instance}",
                node.0
            ),
            Self::NonDenseOrder {
                node,
                order,
                primitive_count,
            } => write!(
                f,
                "compiled root-prefix node {} has order {order}; expected one unique order below {primitive_count}",
                node.0
            ),
        }
    }
}

impl std::error::Error for RootPrefixError {}

impl From<PatchError> for RootPrefixError {
    #[inline(always)]
    fn from(error: PatchError) -> Self {
        Self::Patch(error)
    }
}

/// External revisions whose changes require recompilation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompileStamp {
    pub metrics_revision: u64,
    pub font_revision: u64,
    pub texture_revision: u64,
}

impl CompileStamp {
    /// Checks the cold-path revisions that must still match before submission.
    #[inline(always)]
    pub const fn matches(
        self,
        metrics_revision: u64,
        font_revision: u64,
        texture_revision: u64,
    ) -> bool {
        self.metrics_revision == metrics_revision
            && self.font_revision == font_revision
            && self.texture_revision == texture_revision
    }
}

/// Provenance that cannot be recovered from a materialized [`Actor`] snapshot.
///
/// DSL tweens become ordinary actor fields during building, while SongLua
/// capture/proxy trees use otherwise-supported `SharedFrame` and camera actors.
/// Callers must mark either source so compilation fails instead of freezing a
/// value that is expected to retain runtime behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompileOptions {
    pub has_dsl_tweens: bool,
    pub has_song_lua_capture_proxy: bool,
}

impl CompileOptions {
    /// Explicitly declares that the snapshot has no hidden runtime provenance.
    pub const IMMUTABLE: Self = Self {
        has_dsl_tweens: false,
        has_song_lua_capture_proxy: false,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedFeature {
    SpriteMaskSource,
    SpriteMaskDestination,
    TextMaskDestination,
    TextClip,
    Shadow,
    Effect,
    SpriteAnimation,
    TextureCoordinateVelocity,
    TransientText,
    AttributedText,
    JitteredText,
    DistortedText,
    UnbalancedCameraScope,
    DslTween,
    SongLuaCaptureProxy,
}

impl UnsupportedFeature {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SpriteMaskSource => "sprite mask source",
            Self::SpriteMaskDestination => "sprite mask destination",
            Self::TextMaskDestination => "text mask destination",
            Self::TextClip => "text clipping",
            Self::Shadow => "shadow expansion",
            Self::Effect => "time-driven effect",
            Self::SpriteAnimation => "animated sprite state",
            Self::TextureCoordinateVelocity => "texture-coordinate velocity",
            Self::TransientText => "transient owned text",
            Self::AttributedText => "attributed text",
            Self::JitteredText => "jittered text",
            Self::DistortedText => "distorted text",
            Self::UnbalancedCameraScope => "unbalanced camera scope",
            Self::DslTween => "DSL tween state",
            Self::SongLuaCaptureProxy => "SongLua capture/proxy state",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompileError {
    Unsupported {
        /// Root actor index followed by child indices. An empty path describes
        /// scene-level provenance rather than one materialized actor.
        path: Vec<u32>,
        feature: UnsupportedFeature,
    },
    MissingFont {
        path: Vec<u32>,
        font: &'static str,
    },
    MissingTexture {
        path: Vec<u32>,
        key: Arc<str>,
    },
    TooManyNodes {
        count: usize,
    },
    TooManyCameras {
        count: usize,
        max: usize,
    },
    ConflictingGeometryKey {
        cache_key: TMeshCacheKey,
    },
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported { path, feature } => {
                write!(f, "unsupported {} at actor path {path:?}", feature.as_str())
            }
            Self::MissingFont { path, font } => {
                write!(f, "missing font {font:?} at actor path {path:?}")
            }
            Self::MissingTexture { path, key } => {
                write!(f, "missing texture {key:?} at actor path {path:?}")
            }
            Self::TooManyNodes { count } => {
                write!(
                    f,
                    "compiled scene has {count} primitives; maximum is {}",
                    u32::MAX
                )
            }
            Self::TooManyCameras { count, max } => {
                write!(f, "compiled scene has {count} cameras; maximum is {max}")
            }
            Self::ConflictingGeometryKey { cache_key } => write!(
                f,
                "textured-mesh cache key {cache_key:#x} refers to conflicting geometry"
            ),
        }
    }
}

impl std::error::Error for CompileError {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompileStats {
    pub compiled_primitives: u32,
    pub unique_textures: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EmitStats {
    pub emitted_primitives: u32,
    pub patched_slots: u32,
    pub scratch_growth_events: u32,
    pub texture_refreshes: u32,
    pub sort_fallbacks: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameCapacity {
    pub cameras: usize,
    pub sprite_instances: usize,
    pub primitives: usize,
}

/// Flat, immutable output of one successful scene compilation.
pub struct CompiledScene {
    scene_id: u64,
    stamp: CompileStamp,
    clear_color: [f32; 4],
    cameras: Box<[Mat4]>,
    sprite_instances: Box<[SpriteInstanceRaw]>,
    objects: Box<[RenderObject]>,
    stats: CompileStats,
}

/// A cold-validated sprite-only scene that represents the first actors in a
/// root actor list.
///
/// Its compiled camera and clear color are deliberately not emitted. Mixed
/// composition uses the live root camera and clear color, so resize and
/// overscan changes retain the same behavior as the legacy path. The wrapper
/// owns its scene so structural validation happens once, outside live frames.
pub struct CompiledRootPrefix {
    scene: CompiledScene,
    order_span: u32,
}

impl CompiledScene {
    #[inline(always)]
    pub const fn stamp(&self) -> CompileStamp {
        self.stamp
    }

    #[inline(always)]
    pub const fn stats(&self) -> CompileStats {
        self.stats
    }

    #[inline(always)]
    pub fn capacity(&self) -> FrameCapacity {
        FrameCapacity {
            cameras: self.cameras.len(),
            sprite_instances: self.sprite_instances.len(),
            primitives: self.objects.len(),
        }
    }

    #[inline(always)]
    pub fn primitive(&self, id: NodeId) -> Option<&RenderObject> {
        self.objects.get(id.0 as usize)
    }

    /// Creates a typed UV slot for a compiled sprite primitive.
    pub fn sprite_uv_slot(&self, id: NodeId) -> Result<SpriteUvSlot, PatchError> {
        let Some(object) = self.objects.get(id.0 as usize) else {
            return Err(PatchError::UnknownNode(id));
        };
        let ObjectType::Sprite(instance) = &object.object_type else {
            return Err(PatchError::NotSprite(id));
        };
        if *instance as usize >= self.sprite_instances.len() {
            return Err(PatchError::MissingSpriteInstance(id));
        }
        Ok(SpriteUvSlot {
            scene_id: self.scene_id,
            node: id,
            instance: *instance,
        })
    }

    pub fn primitives(
        &self,
    ) -> impl ExactSizeIterator<Item = (NodeId, &RenderObject)> + DoubleEndedIterator {
        self.objects
            .iter()
            .enumerate()
            .map(|(index, object)| (NodeId(index as u32), object))
    }

    /// Converts this scene into a sprite-only prefix for mixed root
    /// composition.
    ///
    /// The accepted shape is intentionally narrow: one root camera, no custom
    /// camera references, sprite primitives only, and the dense order domain
    /// produced by the supported immutable compiler subset.
    pub fn into_root_sprite_prefix(self) -> Result<CompiledRootPrefix, RootPrefixError> {
        if self.cameras.len() != 1 {
            return Err(RootPrefixError::CameraCount {
                count: self.cameras.len(),
            });
        }

        let primitive_count = self.objects.len();
        let mut seen_orders = vec![false; primitive_count];
        for (index, object) in self.objects.iter().enumerate() {
            let node = NodeId(index as u32);
            if object.camera != 0 {
                return Err(RootPrefixError::NonRootCamera {
                    node,
                    camera: object.camera,
                });
            }
            let ObjectType::Sprite(instance) = &object.object_type else {
                return Err(RootPrefixError::NonSpritePrimitive { node });
            };
            if *instance as usize >= self.sprite_instances.len() {
                return Err(RootPrefixError::InvalidSpriteInstance {
                    node,
                    instance: *instance,
                });
            }
            let Ok(order) = usize::try_from(object.order) else {
                return Err(RootPrefixError::NonDenseOrder {
                    node,
                    order: object.order,
                    primitive_count,
                });
            };
            if order >= primitive_count || seen_orders[order] {
                return Err(RootPrefixError::NonDenseOrder {
                    node,
                    order: object.order,
                    primitive_count,
                });
            }
            seen_orders[order] = true;
        }

        Ok(CompiledRootPrefix {
            scene: self,
            order_span: primitive_count as u32,
        })
    }

    /// Emits the frozen scene in compile-time `(z, order)` order.
    ///
    /// Recycle the returned list through
    /// [`CompiledSceneScratch::recycle_render_list`] before the next emission.
    pub fn emit(&self, scratch: &mut CompiledSceneScratch) -> RenderList {
        self.emit_validated(scratch, &[])
    }

    /// Emits the scene and patches declared sprite UV origins by direct index.
    ///
    /// Validation happens before output buffers are taken from `scratch`, so a
    /// stale slot cannot discard warmed storage. The warmed success path does
    /// no lookup, sorting, traversal, or allocation.
    pub fn emit_with_uv_patches(
        &self,
        scratch: &mut CompiledSceneScratch,
        patches: &[SpriteUvPatch],
    ) -> Result<RenderList, PatchError> {
        for patch in patches {
            self.validate_sprite_uv_slot(patch.slot)?;
        }
        Ok(self.emit_validated(scratch, patches))
    }

    fn validate_sprite_uv_slot(&self, slot: SpriteUvSlot) -> Result<usize, PatchError> {
        if slot.scene_id != self.scene_id {
            return Err(PatchError::WrongScene(slot.node));
        }
        let instance = slot.instance as usize;
        if instance >= self.sprite_instances.len() {
            return Err(PatchError::MissingSpriteInstance(slot.node));
        }
        Ok(instance)
    }

    fn emit_validated(
        &self,
        scratch: &mut CompiledSceneScratch,
        patches: &[SpriteUvPatch],
    ) -> RenderList {
        let mut cameras = std::mem::take(&mut scratch.cameras);
        let mut sprite_instances = std::mem::take(&mut scratch.sprite_instances);
        let mut objects = std::mem::take(&mut scratch.objects);
        cameras.clear();
        sprite_instances.clear();
        objects.clear();

        let growth = u32::from(ensure_capacity(&mut cameras, self.cameras.len()))
            + u32::from(ensure_capacity(
                &mut sprite_instances,
                self.sprite_instances.len(),
            ))
            + u32::from(ensure_capacity(&mut objects, self.objects.len()));
        cameras.extend_from_slice(&self.cameras);
        sprite_instances.extend_from_slice(&self.sprite_instances);
        objects.extend_from_slice(&self.objects);
        for patch in patches {
            sprite_instances[patch.slot.instance as usize].uv_offset = patch.offset;
        }

        scratch.frame_stats = EmitStats {
            emitted_primitives: self.stats.compiled_primitives,
            patched_slots: u32::try_from(patches.len()).unwrap_or(u32::MAX),
            scratch_growth_events: growth,
            texture_refreshes: 0,
            sort_fallbacks: 0,
        };
        RenderList {
            clear_color: self.clear_color,
            cameras,
            sprite_instances,
            objects,
        }
    }

    /// Flattens this immutable scene into a retained renderer frame once.
    ///
    /// Mesh transforms and draw-run construction happen here on the cold path.
    /// The returned frame can be submitted directly by hardware backends every
    /// frame without actor composition, RenderList emission, or draw prep.
    /// Conflicting nonzero geometry keys are rejected before draw prep so no
    /// draw op can address a differently sized or shaped retained buffer.
    pub fn compile_draw_frame(&self) -> Result<CompiledDrawFrame, CompileError> {
        let mut emit_scratch = CompiledSceneScratch::default();
        emit_scratch.reserve(self.capacity());
        let render = self.emit(&mut emit_scratch);
        let geometries = collect_cached_tmesh_geometries(&render)?;
        let mut draw_scratch = DrawScratch::default();
        let (view, _) = prepare_render_list(&render, &mut draw_scratch, |_, _| true);
        let mut frame = RenderDrawFrame::with_capacity(DrawFrameCapacity {
            cameras: view.cameras.len(),
            sprite_instances: view.sprite_instances.len(),
            mesh_vertices: view.mesh_vertices.len(),
            tmesh_vertices: view.tmesh_vertices.len(),
            tmesh_instances: view.tmesh_instances.len(),
            ops: view.ops.len(),
        });
        frame.begin(view.clear_color);
        frame.render_objects = self.stats.compiled_primitives;
        frame.cameras.extend_from_slice(view.cameras);
        frame
            .sprite_instances
            .extend_from_slice(view.sprite_instances);
        frame.mesh_vertices.extend_from_slice(view.mesh_vertices);
        frame.tmesh_vertices.extend_from_slice(view.tmesh_vertices);
        frame
            .tmesh_instances
            .extend_from_slice(view.tmesh_instances);
        frame.ops.extend_from_slice(view.ops);

        Ok(CompiledDrawFrame {
            frame,
            geometries,
            stamp: self.stamp,
            scene_id: self.scene_id,
        })
    }
}

fn collect_cached_tmesh_geometries(
    render: &RenderList,
) -> Result<Vec<CachedTMeshGeometry>, CompileError> {
    let mut indices = FastU64Map::<usize>::default();
    let mut geometries = Vec::<CachedTMeshGeometry>::new();
    for object in &render.objects {
        let ObjectType::TexturedMesh {
            vertices,
            geom_cache_key,
            ..
        } = &object.object_type
        else {
            continue;
        };
        if *geom_cache_key == INVALID_TMESH_CACHE_KEY {
            continue;
        }

        let fingerprint = tmesh_fingerprint(vertices.as_ref());
        if let Some(index) = indices.get(geom_cache_key).copied() {
            let retained = &geometries[index];
            if retained.vertices.len() != vertices.len()
                || retained.fingerprint != fingerprint
                || bytemuck::cast_slice::<_, u8>(retained.vertices.as_ref())
                    != bytemuck::cast_slice::<_, u8>(vertices.as_ref())
            {
                return Err(CompileError::ConflictingGeometryKey {
                    cache_key: *geom_cache_key,
                });
            }
            continue;
        }

        let retained = match vertices {
            TexturedMeshVertices::Shared(vertices) => Arc::clone(vertices),
            TexturedMeshVertices::Transient(vertices) => Arc::from(vertices.clone()),
        };
        indices.insert(*geom_cache_key, geometries.len());
        geometries.push(CachedTMeshGeometry {
            cache_key: *geom_cache_key,
            fingerprint,
            vertices: retained,
        });
    }
    Ok(geometries)
}

impl CompiledRootPrefix {
    #[inline(always)]
    pub const fn stamp(&self) -> CompileStamp {
        self.scene.stamp
    }

    #[inline(always)]
    pub const fn stats(&self) -> CompileStats {
        self.scene.stats
    }

    #[inline(always)]
    pub fn primitive_count(&self) -> usize {
        self.scene.objects.len()
    }

    #[inline(always)]
    pub fn sprite_count(&self) -> usize {
        self.scene.sprite_instances.len()
    }

    #[inline(always)]
    pub const fn order_span(&self) -> u32 {
        self.order_span
    }

    #[inline(always)]
    pub fn primitive(&self, id: NodeId) -> Option<&RenderObject> {
        self.scene.primitive(id)
    }

    #[inline(always)]
    pub fn sprite_uv_slot(&self, id: NodeId) -> Result<SpriteUvSlot, PatchError> {
        self.scene.sprite_uv_slot(id)
    }

    pub(crate) fn validate_uv_rect_patches(
        &self,
        patches: &[SpriteUvRectPatch],
    ) -> Result<(), RootPrefixError> {
        for patch in patches {
            self.scene.validate_sprite_uv_slot(patch.slot)?;
        }
        Ok(())
    }

    pub(crate) fn append_to(
        &self,
        sprite_instances: &mut Vec<SpriteInstanceRaw>,
        objects: &mut Vec<RenderObject>,
        patches: &[SpriteUvRectPatch],
    ) {
        // A root prefix is inserted before any legacy actor is visited. Keeping
        // the destination empty avoids per-frame rebasing of either stream.
        debug_assert!(sprite_instances.is_empty());
        debug_assert!(objects.is_empty());
        sprite_instances.extend_from_slice(&self.scene.sprite_instances);
        objects.extend(self.scene.objects.iter().cloned());
        for patch in patches {
            let sprite = &mut sprite_instances[patch.slot.instance as usize];
            sprite.uv_scale = patch.scale;
            sprite.uv_offset = patch.offset;
        }
    }
}

/// Retained, renderer-ready output for one immutable compiled scene.
///
/// Owned by the render thread for a screen/song lifetime. It is built and
/// capacity-sized at transition time, never grows or evicts on submission,
/// performs no lookup or resource destruction during a frame, and is freed at
/// the owning transition. Its compile stamp must be checked after external
/// metric, font, or texture-registry changes. Keyed immutable textured meshes
/// are retained once in `geometries` and must be prewarmed before submitting
/// their cached draw ops. Invalid-key geometry remains in the frame's transient
/// vertex buffer and is uploaded on each submission.
pub struct CompiledDrawFrame {
    frame: RenderDrawFrame,
    geometries: Vec<CachedTMeshGeometry>,
    stamp: CompileStamp,
    scene_id: u64,
}

impl CompiledDrawFrame {
    #[inline(always)]
    pub const fn frame(&self) -> &RenderDrawFrame {
        &self.frame
    }

    #[inline(always)]
    pub fn geometries(&self) -> &[CachedTMeshGeometry] {
        self.geometries.as_slice()
    }

    #[inline(always)]
    pub const fn stamp(&self) -> CompileStamp {
        self.stamp
    }

    /// Returns false when transition-owned geometry must be recompiled.
    #[inline(always)]
    pub fn is_current<T: TextureContext + ?Sized>(
        &self,
        metrics_revision: u64,
        font_revision: u64,
        textures: &T,
    ) -> bool {
        self.stamp.matches(
            metrics_revision,
            font_revision,
            textures.texture_registry_generation(),
        )
    }

    #[inline(always)]
    pub fn prepare_stats(&self) -> FramePrepareStats {
        self.frame.prepare_stats()
    }

    /// Applies retained UV state directly to the renderer instance buffer.
    pub fn apply_uv_patches(&mut self, patches: &[SpriteUvPatch]) -> Result<(), PatchError> {
        for patch in patches {
            self.sprite_index(patch.slot)?;
        }
        for patch in patches {
            self.frame.sprite_instances[patch.slot.instance as usize].uv_offset = patch.offset;
        }
        Ok(())
    }

    fn sprite_index(&self, slot: SpriteUvSlot) -> Result<usize, PatchError> {
        let id = slot.node;
        if slot.scene_id != self.scene_id {
            return Err(PatchError::WrongScene(id));
        }
        let instance = slot.instance as usize;
        if instance >= self.frame.sprite_instances.len() {
            return Err(PatchError::MissingSpriteInstance(id));
        }
        Ok(instance)
    }
}

/// Recycled output storage for one render-thread compiled-scene stream.
///
/// This is single-thread-owned, lives for the screen or session, is sized from
/// a scene's exact [`FrameCapacity`] during transition warmup, never evicts,
/// performs no lookup on emission, and frees its buffers where it is dropped.
#[derive(Default)]
pub struct CompiledSceneScratch {
    cameras: Vec<Mat4>,
    sprite_instances: Vec<SpriteInstanceRaw>,
    objects: Vec<RenderObject>,
    frame_stats: EmitStats,
}

impl CompiledSceneScratch {
    /// Reserves the exact known high-water mark before the first live frame.
    pub fn reserve(&mut self, capacity: FrameCapacity) {
        ensure_capacity(&mut self.cameras, capacity.cameras);
        ensure_capacity(&mut self.sprite_instances, capacity.sprite_instances);
        ensure_capacity(&mut self.objects, capacity.primitives);
    }

    #[inline(always)]
    pub const fn frame_stats(&self) -> EmitStats {
        self.frame_stats
    }

    #[inline(always)]
    pub fn capacity(&self) -> FrameCapacity {
        FrameCapacity {
            cameras: self.cameras.capacity(),
            sprite_instances: self.sprite_instances.capacity(),
            primitives: self.objects.capacity(),
        }
    }

    pub fn recycle_render_list(&mut self, render: &mut RenderList) {
        let mut cameras = std::mem::take(&mut render.cameras);
        let mut sprite_instances = std::mem::take(&mut render.sprite_instances);
        let mut objects = std::mem::take(&mut render.objects);
        cameras.clear();
        sprite_instances.clear();
        objects.clear();
        self.cameras = cameras;
        self.sprite_instances = sprite_instances;
        self.objects = objects;
    }
}

#[inline(always)]
fn ensure_capacity<T>(values: &mut Vec<T>, needed: usize) -> bool {
    if values.capacity() >= needed {
        return false;
    }
    values.reserve_exact(needed.saturating_sub(values.len()));
    true
}

/// Cold-path compiler for immutable actor snapshots.
pub struct SceneCompiler<'a, T: TextureContext + ?Sized> {
    metrics: &'a Metrics,
    fonts: &'a HashMap<&'static str, Font>,
    textures: &'a T,
    text_cache: &'a mut TextLayoutCache,
    metrics_revision: u64,
    font_revision: u64,
}

impl<'a, T: TextureContext + ?Sized> SceneCompiler<'a, T> {
    pub fn new(
        metrics: &'a Metrics,
        fonts: &'a HashMap<&'static str, Font>,
        textures: &'a T,
        text_cache: &'a mut TextLayoutCache,
        metrics_revision: u64,
        font_revision: u64,
    ) -> Self {
        Self {
            metrics,
            fonts,
            textures,
            text_cache,
            metrics_revision,
            font_revision,
        }
    }

    /// Validates and lowers one immutable snapshot through the legacy composer.
    ///
    /// `options` is mandatory because DSL-tween and SongLua capture/proxy
    /// provenance is erased before an `Actor` value reaches this crate.
    pub fn compile(
        &mut self,
        actors: &[Actor],
        clear_color: [f32; 4],
        options: CompileOptions,
    ) -> Result<CompiledScene, CompileError> {
        validate_provenance(options)?;
        let validation = validate_scene(actors, self.fonts, self.textures)?;
        let camera_count = validation.custom_cameras.saturating_add(1);
        if camera_count > 256 {
            return Err(CompileError::TooManyCameras {
                count: camera_count,
                max: 256,
            });
        }

        let mut compose_scratch = ComposeScratch::default();
        let render = build_screen_cached_with_scratch_and_texture_context(
            actors,
            clear_color,
            self.metrics,
            self.fonts,
            0.0,
            self.text_cache,
            &mut compose_scratch,
            self.textures,
        );
        freeze_render_list(
            render,
            CompileStamp {
                metrics_revision: self.metrics_revision,
                font_revision: self.font_revision,
                texture_revision: self.textures.texture_registry_generation(),
            },
        )
    }
}

fn validate_provenance(options: CompileOptions) -> Result<(), CompileError> {
    let feature = if options.has_song_lua_capture_proxy {
        Some(UnsupportedFeature::SongLuaCaptureProxy)
    } else if options.has_dsl_tweens {
        Some(UnsupportedFeature::DslTween)
    } else {
        None
    };
    match feature {
        Some(feature) => Err(CompileError::Unsupported {
            path: Vec::new(),
            feature,
        }),
        None => Ok(()),
    }
}

#[derive(Default)]
struct Validation {
    custom_cameras: usize,
}

fn validate_scene<T: TextureContext + ?Sized>(
    actors: &[Actor],
    fonts: &HashMap<&'static str, Font>,
    textures: &T,
) -> Result<Validation, CompileError> {
    let mut validation = Validation::default();
    validate_list(actors, fonts, textures, &mut validation, &mut Vec::new())?;
    Ok(validation)
}

fn validate_list<T: TextureContext + ?Sized>(
    actors: &[Actor],
    fonts: &HashMap<&'static str, Font>,
    textures: &T,
    validation: &mut Validation,
    path: &mut Vec<u32>,
) -> Result<(), CompileError> {
    let mut camera_pushes = Vec::<Vec<u32>>::new();
    for (index, actor) in actors.iter().enumerate() {
        let index = u32::try_from(index).map_err(|_| CompileError::TooManyNodes {
            count: actors.len(),
        })?;
        path.push(index);
        match actor {
            Actor::CameraPush { .. } => {
                validation.custom_cameras = validation.custom_cameras.saturating_add(1);
                camera_pushes.push(path.clone());
            }
            Actor::CameraPop => {
                if camera_pushes.pop().is_none() {
                    return unsupported(path, UnsupportedFeature::UnbalancedCameraScope);
                }
            }
            _ => validate_actor(actor, fonts, textures, validation, path)?,
        }
        path.pop();
    }
    if let Some(push_path) = camera_pushes.first() {
        return Err(CompileError::Unsupported {
            path: push_path.clone(),
            feature: UnsupportedFeature::UnbalancedCameraScope,
        });
    }
    Ok(())
}

fn validate_actor<T: TextureContext + ?Sized>(
    actor: &Actor,
    fonts: &HashMap<&'static str, Font>,
    textures: &T,
    validation: &mut Validation,
    path: &mut Vec<u32>,
) -> Result<(), CompileError> {
    match actor {
        Actor::Sprite {
            source,
            visible,
            mask_source,
            mask_dest,
            texcoordvelocity,
            animate,
            shadow_len,
            effect,
            ..
        } => {
            if *mask_source {
                return unsupported(path, UnsupportedFeature::SpriteMaskSource);
            }
            if *mask_dest {
                return unsupported(path, UnsupportedFeature::SpriteMaskDestination);
            }
            if *animate {
                return unsupported(path, UnsupportedFeature::SpriteAnimation);
            }
            if texcoordvelocity.is_some() {
                return unsupported(path, UnsupportedFeature::TextureCoordinateVelocity);
            }
            if effect.mode != EffectMode::None {
                return unsupported(path, UnsupportedFeature::Effect);
            }
            if shadow_len[0] != 0.0 || shadow_len[1] != 0.0 {
                return unsupported(path, UnsupportedFeature::Shadow);
            }
            if *visible {
                validate_sprite_texture(source, textures, path)?;
            }
        }
        Actor::Text {
            font,
            content,
            attributes,
            jitter,
            distortion,
            clip,
            mask_dest,
            shadow_len,
            effect,
            stroke_color,
            ..
        } => {
            if matches!(content, TextContent::Owned(_)) {
                return unsupported(path, UnsupportedFeature::TransientText);
            }
            if !attributes.is_empty() {
                return unsupported(path, UnsupportedFeature::AttributedText);
            }
            if *jitter {
                return unsupported(path, UnsupportedFeature::JitteredText);
            }
            if distortion.max(0.0) > 1e-6 {
                return unsupported(path, UnsupportedFeature::DistortedText);
            }
            if clip.is_some() {
                return unsupported(path, UnsupportedFeature::TextClip);
            }
            if *mask_dest {
                return unsupported(path, UnsupportedFeature::TextMaskDestination);
            }
            if effect.mode != EffectMode::None {
                return unsupported(path, UnsupportedFeature::Effect);
            }
            if shadow_len[0] != 0.0 || shadow_len[1] != 0.0 {
                return unsupported(path, UnsupportedFeature::Shadow);
            }
            let Some(font_data) = fonts.get(font) else {
                return Err(CompileError::MissingFont {
                    path: path.clone(),
                    font,
                });
            };
            validate_textures(
                font_data,
                fonts,
                content.as_str(),
                *stroke_color,
                textures,
                path,
            )?;
        }
        Actor::Mesh { .. } => {}
        Actor::TexturedMesh {
            texture, visible, ..
        } => {
            if *visible {
                require_texture(texture, textures, path)?;
            }
        }
        Actor::Frame {
            children,
            background,
            ..
        } => {
            validate_background(background.as_ref(), textures, path)?;
            validate_list(children, fonts, textures, validation, path)?;
        }
        Actor::SharedFrame {
            children,
            background,
            ..
        } => {
            validate_background(background.as_ref(), textures, path)?;
            validate_list(children, fonts, textures, validation, path)?;
        }
        Actor::Camera { children, .. } => {
            validation.custom_cameras = validation.custom_cameras.saturating_add(1);
            validate_list(children, fonts, textures, validation, path)?;
        }
        Actor::CameraPush { .. } | Actor::CameraPop => {}
        Actor::Shadow { .. } => return unsupported(path, UnsupportedFeature::Shadow),
    }
    Ok(())
}

fn unsupported<T>(path: &[u32], feature: UnsupportedFeature) -> Result<T, CompileError> {
    Err(CompileError::Unsupported {
        path: path.to_vec(),
        feature,
    })
}

fn validate_background<T: TextureContext + ?Sized>(
    background: Option<&Background>,
    textures: &T,
    path: &[u32],
) -> Result<(), CompileError> {
    match background {
        Some(Background::Color(color)) if color[3] > 0.0 => {
            require_texture("__white", textures, path)
        }
        Some(Background::Texture(key)) => require_texture(key, textures, path),
        _ => Ok(()),
    }
}

fn validate_sprite_texture<T: TextureContext + ?Sized>(
    source: &SpriteSource,
    textures: &T,
    path: &[u32],
) -> Result<(), CompileError> {
    match source {
        SpriteSource::Solid => require_texture("__white", textures, path),
        SpriteSource::TextureStaticHandle {
            key,
            handle,
            generation,
        } if *handle != INVALID_TEXTURE_HANDLE
            && *generation == textures.texture_registry_generation() =>
        {
            Ok(())
        }
        SpriteSource::TextureHandle {
            key: _,
            handle,
            generation,
        } if *handle != INVALID_TEXTURE_HANDLE
            && *generation == textures.texture_registry_generation() =>
        {
            Ok(())
        }
        _ => require_texture(
            source
                .texture_key()
                .expect("non-solid sprite source has a texture key"),
            textures,
            path,
        ),
    }
}

fn validate_textures<T: TextureContext + ?Sized>(
    start_font: &Font,
    fonts: &HashMap<&'static str, Font>,
    text: &str,
    stroke_color: Option<[f32; 4]>,
    textures: &T,
    path: &[u32],
) -> Result<(), CompileError> {
    let draws_space = start_font.glyph_map.contains_key(&' ');
    let stroke_alpha = stroke_color.unwrap_or(start_font.default_stroke_color)[3];
    let needs_stroke = stroke_alpha > 0.0 && !start_font.stroke_texture_map.is_empty();
    for ch in text.chars().filter(|ch| *ch != '\n') {
        let Some(glyph) = font::find_glyph(start_font, ch, fonts) else {
            continue;
        };
        if ch != ' ' || draws_space {
            require_texture(glyph.texture_key.as_ref(), textures, path)?;
        }
        if needs_stroke && let Some(key) = glyph.stroke_texture_key.as_ref() {
            require_texture(key.as_ref(), textures, path)?;
        }
    }
    Ok(())
}

fn require_texture<T: TextureContext + ?Sized>(
    key: &str,
    textures: &T,
    path: &[u32],
) -> Result<(), CompileError> {
    if textures.texture_handle(key) != INVALID_TEXTURE_HANDLE {
        return Ok(());
    }
    Err(CompileError::MissingTexture {
        path: path.to_vec(),
        key: Arc::from(key),
    })
}

fn freeze_render_list(
    render: RenderList,
    stamp: CompileStamp,
) -> Result<CompiledScene, CompileError> {
    let RenderList {
        clear_color,
        cameras,
        sprite_instances,
        mut objects,
    } = render;
    if objects.len() > u32::MAX as usize {
        return Err(CompileError::TooManyNodes {
            count: objects.len(),
        });
    }
    for object in &mut objects {
        let ObjectType::TexturedMesh { vertices, .. } = &mut object.object_type else {
            continue;
        };
        let previous = std::mem::replace(vertices, TexturedMeshVertices::Transient(Vec::new()));
        *vertices = match previous {
            TexturedMeshVertices::Shared(vertices) => TexturedMeshVertices::Shared(vertices),
            TexturedMeshVertices::Transient(vertices) => {
                TexturedMeshVertices::Shared(Arc::from(vertices))
            }
        };
    }
    let stats = CompileStats {
        compiled_primitives: objects.len() as u32,
        unique_textures: unique_textures(&objects),
    };
    Ok(CompiledScene {
        scene_id: NEXT_SCENE_ID.fetch_add(1, Ordering::Relaxed),
        stamp,
        clear_color,
        cameras: cameras.into_boxed_slice(),
        sprite_instances: sprite_instances.into_boxed_slice(),
        objects: objects.into_boxed_slice(),
        stats,
    })
}

fn unique_textures(objects: &[RenderObject]) -> u32 {
    let mut handles = objects
        .iter()
        .map(|object| object.texture_handle)
        .filter(|handle| *handle != INVALID_TEXTURE_HANDLE)
        .collect::<Vec<_>>();
    handles.sort_unstable();
    handles.dedup();
    u32::try_from(handles.len()).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::{SizeSpec, TextAlign, TextAttribute, TextContent};
    use crate::anim::{EffectMode, EffectState};
    use crate::compose::{
        build_screen_cached_with_scratch_and_texture_context,
        build_screen_cached_with_scratch_and_texture_context_and_root_prefix,
    };
    use crate::font::Glyph;
    use crate::texture::TextureMeta;
    use deadlib_render::{
        BlendMode, MeshVertex, TexturedMeshInstanceRaw, TexturedMeshVertex,
        draw_prep::{DrawOp, TMeshCacheResult, TexturedMeshSource},
    };
    use glam::{Mat4, Vec3};
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    thread_local! {
        static COUNT_ALLOCS: Cell<bool> = const { Cell::new(false) };
        static ALLOC_COUNT: Cell<usize> = const { Cell::new(0) };
    }

    struct CountingAllocator;

    #[global_allocator]
    static ALLOCATOR: CountingAllocator = CountingAllocator;

    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            if COUNT_ALLOCS.with(Cell::get) {
                ALLOC_COUNT.with(|count| count.set(count.get().saturating_add(1)));
            }
            // SAFETY: this allocator only observes the call, then forwards the
            // unchanged layout to the process system allocator.
            unsafe { System.alloc(layout) }
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            if COUNT_ALLOCS.with(Cell::get) {
                ALLOC_COUNT.with(|count| count.set(count.get().saturating_add(1)));
            }
            // SAFETY: this allocator only observes the call, then forwards the
            // unchanged layout to the process system allocator.
            unsafe { System.alloc_zeroed(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            // SAFETY: `ptr` and `layout` came from the forwarded system
            // allocation and are returned to that same allocator.
            unsafe { System.dealloc(ptr, layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            if COUNT_ALLOCS.with(Cell::get) {
                ALLOC_COUNT.with(|count| count.set(count.get().saturating_add(1)));
            }
            // SAFETY: `ptr` and `layout` came from the forwarded system
            // allocator; `new_size` is passed through unchanged.
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    fn count_allocs(run: impl FnOnce()) -> usize {
        ALLOC_COUNT.with(|count| count.set(0));
        COUNT_ALLOCS.with(|enabled| enabled.set(true));
        run();
        COUNT_ALLOCS.with(|enabled| enabled.set(false));
        ALLOC_COUNT.with(Cell::get)
    }

    struct TestTextures {
        generation: u64,
        dims: HashMap<String, TextureMeta>,
        handles: HashMap<String, u64>,
        handle_calls: Cell<u32>,
    }

    impl TextureContext for TestTextures {
        fn texture_registry_generation(&self) -> u64 {
            self.generation
        }

        fn texture_dims(&self, key: &str) -> Option<TextureMeta> {
            self.dims.get(key).copied()
        }

        fn sprite_sheet_dims(&self, key: &str) -> (u32, u32) {
            crate::font::parse_sprite_sheet_dims_from_key(key)
        }

        fn texture_handle(&self, key: &str) -> u64 {
            self.handle_calls.set(self.handle_calls.get() + 1);
            self.handles
                .get(key)
                .copied()
                .unwrap_or(INVALID_TEXTURE_HANDLE)
        }
    }

    fn textures() -> TestTextures {
        TestTextures {
            generation: 9,
            dims: HashMap::from([
                ("sheet 2x2".to_owned(), TextureMeta { w: 64, h: 32 }),
                ("plain".to_owned(), TextureMeta { w: 24, h: 12 }),
            ]),
            handles: HashMap::from([
                ("__white".to_owned(), 1),
                ("sheet 2x2".to_owned(), 2),
                ("plain".to_owned(), 3),
                ("mesh_tex".to_owned(), 4),
                ("font_a".to_owned(), 5),
                ("font_b".to_owned(), 6),
            ]),
            handle_calls: Cell::new(0),
        }
    }

    fn glyph(key: &str) -> Glyph {
        Glyph {
            texture_key: Arc::from(key),
            stroke_texture_key: None,
            tex_rect: [0.0, 0.0, 8.0, 8.0],
            uv_scale: [0.5, 0.5],
            uv_offset: [0.0, 0.0],
            size: [8.0, 8.0],
            offset: [0.0, 0.0],
            advance: 8.0,
            advance_i32: 8,
        }
    }

    fn fonts() -> HashMap<&'static str, Font> {
        let glyph_a = glyph("font_a");
        let glyph_b = glyph("font_b");
        let mut glyph_map = HashMap::new();
        glyph_map.insert('A', glyph_a.clone());
        glyph_map.insert('B', glyph_b.clone());
        let mut ascii = std::array::from_fn(|_| None);
        ascii['A' as usize] = Some(glyph_a);
        ascii['B' as usize] = Some(glyph_b);
        HashMap::from([(
            "test",
            Font {
                glyph_map,
                ascii_glyphs: Box::new(ascii),
                default_glyph: None,
                line_spacing: 10,
                height: 10,
                fallback_font_name: None,
                cache_tag: 4,
                chain_key: 4,
                default_stroke_color: [0.0; 4],
                stroke_texture_map: HashMap::new(),
                texture_hints_map: HashMap::new(),
            },
        )])
    }

    fn metrics() -> Metrics {
        Metrics {
            left: -320.0,
            right: 320.0,
            top: 240.0,
            bottom: -240.0,
        }
    }

    fn sprite(source: SpriteSource) -> Actor {
        Actor::Sprite {
            align: [0.5, 0.5],
            offset: [20.0, 30.0],
            world_z: 0.25,
            size: [SizeSpec::Px(16.0), SizeSpec::Px(12.0)],
            source,
            tint: [0.8, 0.7, 0.6, 0.9],
            glow: [0.1, 0.2, 0.3, 0.4],
            z: 2,
            cell: None,
            grid: None,
            uv_rect: None,
            visible: true,
            flip_x: false,
            flip_y: false,
            cropleft: 0.0,
            cropright: 0.0,
            croptop: 0.0,
            cropbottom: 0.0,
            fadeleft: 0.0,
            faderight: 0.0,
            fadetop: 0.0,
            fadebottom: 0.0,
            blend: BlendMode::Alpha,
            mask_source: false,
            mask_dest: false,
            rot_x_deg: 0.0,
            rot_y_deg: 0.0,
            rot_z_deg: 0.0,
            local_offset: [0.0, 0.0],
            local_offset_rot_sin_cos: [0.0, 1.0],
            texcoordvelocity: None,
            animate: false,
            state_delay: 0.0,
            scale: [1.0, 1.0],
            shadow_len: [0.0, 0.0],
            shadow_color: [0.0, 0.0, 0.0, 0.5],
            effect: EffectState::default(),
        }
    }

    fn flat_sprite(z: i16, uv_offset: [f32; 2]) -> Actor {
        let mut actor = sprite(SpriteSource::TextureStatic("plain"));
        let Actor::Sprite {
            glow,
            z: actor_z,
            uv_rect,
            ..
        } = &mut actor
        else {
            unreachable!()
        };
        *glow = [0.0; 4];
        *actor_z = z;
        *uv_rect = Some([
            uv_offset[0],
            uv_offset[1],
            uv_offset[0] + 1.0,
            uv_offset[1] + 1.0,
        ]);
        actor
    }

    fn text() -> Actor {
        Actor::Text {
            align: [0.5, 0.5],
            offset: [40.0, 20.0],
            local_transform: Mat4::from_rotation_z(0.1),
            color: [0.9, 0.8, 0.7, 1.0],
            stroke_color: None,
            glow: [0.0; 4],
            font: "test",
            content: TextContent::Static("AB"),
            attributes: Vec::new(),
            align_text: TextAlign::Center,
            z: 1,
            scale: [1.25, 0.75],
            fit_width: None,
            fit_height: None,
            line_spacing: None,
            wrap_width_pixels: None,
            max_width: None,
            max_height: None,
            max_w_pre_zoom: false,
            max_h_pre_zoom: false,
            jitter: false,
            distortion: 0.0,
            clip: None,
            mask_dest: false,
            blend: BlendMode::Alpha,
            shadow_len: [0.0, 0.0],
            shadow_color: [0.0, 0.0, 0.0, 0.5],
            effect: EffectState::default(),
        }
    }

    fn mesh() -> Actor {
        Actor::Mesh {
            align: [0.0, 0.0],
            offset: [4.0, 6.0],
            size: [SizeSpec::Fill, SizeSpec::Px(20.0)],
            vertices: Arc::from([
                MeshVertex {
                    pos: [0.0, 0.0],
                    color: [1.0, 0.0, 0.0, 1.0],
                },
                MeshVertex {
                    pos: [1.0, 0.0],
                    color: [0.0, 1.0, 0.0, 1.0],
                },
                MeshVertex {
                    pos: [0.0, 1.0],
                    color: [0.0, 0.0, 1.0, 1.0],
                },
            ]),
            visible: true,
            blend: BlendMode::Add,
            z: 2,
        }
    }

    fn textured_mesh() -> Actor {
        Actor::TexturedMesh {
            align: [0.0, 0.0],
            offset: [8.0, 9.0],
            world_z: 0.5,
            size: [SizeSpec::Px(10.0), SizeSpec::Px(10.0)],
            local_transform: Mat4::from_scale(Vec3::new(2.0, 3.0, 1.0)),
            texture: Arc::from("mesh_tex"),
            tint: [0.4, 0.5, 0.6, 0.7],
            glow: [0.0; 4],
            vertices: Arc::from([
                TexturedMeshVertex::default(),
                TexturedMeshVertex {
                    pos: [1.0, 0.0, 0.0],
                    ..TexturedMeshVertex::default()
                },
                TexturedMeshVertex {
                    pos: [0.0, 1.0, 0.0],
                    ..TexturedMeshVertex::default()
                },
            ]),
            geom_cache_key: 44,
            uv_scale: [0.5, 0.75],
            uv_offset: [0.1, 0.2],
            uv_tex_shift: [0.3, 0.4],
            depth_test: true,
            visible: true,
            blend: BlendMode::Alpha,
            z: -1,
        }
    }

    fn fixture() -> Vec<Actor> {
        let mut native = sprite(SpriteSource::TextureStatic("sheet 2x2"));
        let Actor::Sprite {
            size,
            cell,
            grid,
            flip_x,
            flip_y,
            rot_x_deg,
            rot_y_deg,
            rot_z_deg,
            ..
        } = &mut native
        else {
            unreachable!()
        };
        *size = [SizeSpec::Px(0.0), SizeSpec::Px(0.0)];
        *cell = Some((3, u32::MAX));
        *grid = Some((2, 2));
        *flip_x = true;
        *flip_y = true;
        *rot_x_deg = 20.0;
        *rot_y_deg = 180.0;
        *rot_z_deg = 35.0;

        let mut uv = sprite(SpriteSource::Texture(Arc::from("plain")));
        let Actor::Sprite {
            uv_rect,
            cropleft,
            faderight,
            blend,
            ..
        } = &mut uv
        else {
            unreachable!()
        };
        *uv_rect = Some([0.1, 0.2, 0.8, 0.9]);
        *cropleft = 0.1;
        *faderight = 0.2;
        *blend = BlendMode::Add;

        vec![
            Actor::Frame {
                align: [0.0, 0.0],
                offset: [10.0, 12.0],
                size: [SizeSpec::Fill, SizeSpec::Px(120.0)],
                children: vec![native, uv],
                background: Some(Background::Color([0.1, 0.2, 0.3, 0.4])),
                z: 3,
            },
            Actor::SharedFrame {
                align: [0.5, 0.5],
                offset: [100.0, 100.0],
                size: [SizeSpec::Px(80.0), SizeSpec::Px(60.0)],
                children: Arc::from([mesh(), textured_mesh()]),
                background: None,
                z: -2,
                tint: [0.8, 0.7, 0.6, 0.5],
                blend: Some(BlendMode::Add),
            },
            text(),
            Actor::Camera {
                view_proj: Mat4::from_translation(Vec3::new(1.0, 2.0, 0.0)),
                children: vec![sprite(SpriteSource::TextureStatic("plain"))],
            },
            Actor::CameraPush {
                view_proj: Mat4::from_scale(Vec3::new(0.5, 0.5, 1.0)),
            },
            mesh(),
            Actor::CameraPop,
        ]
    }

    fn assert_tmesh_vertices_eq(actual: &[TexturedMeshVertex], expected: &[TexturedMeshVertex]) {
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(expected) {
            assert_eq!(actual.pos, expected.pos);
            assert_eq!(actual.uv, expected.uv);
            assert_eq!(actual.color, expected.color);
            assert_eq!(actual.tex_matrix_scale, expected.tex_matrix_scale);
        }
    }

    fn assert_render_eq(expected: &RenderList, actual: &RenderList) {
        assert_eq!(expected.clear_color, actual.clear_color);
        assert_eq!(expected.cameras.len(), actual.cameras.len());
        for (expected, actual) in expected.cameras.iter().zip(&actual.cameras) {
            assert_eq!(expected.to_cols_array(), actual.to_cols_array());
        }
        assert_eq!(expected.sprite_instances, actual.sprite_instances);
        assert_eq!(expected.objects.len(), actual.objects.len());
        for (expected, actual) in expected.objects.iter().zip(&actual.objects) {
            assert_eq!(expected.texture_handle, actual.texture_handle);
            assert_eq!(expected.blend, actual.blend);
            assert_eq!(expected.z, actual.z);
            assert_eq!(expected.order, actual.order);
            assert_eq!(expected.camera, actual.camera);
            match (&expected.object_type, &actual.object_type) {
                (ObjectType::Sprite(expected), ObjectType::Sprite(actual)) => {
                    assert_eq!(expected, actual);
                }
                (
                    ObjectType::Mesh {
                        transform: expected_transform,
                        tint: expected_tint,
                        vertices: expected_vertices,
                    },
                    ObjectType::Mesh {
                        transform: actual_transform,
                        tint: actual_tint,
                        vertices: actual_vertices,
                    },
                ) => {
                    assert_eq!(
                        expected_transform.to_cols_array(),
                        actual_transform.to_cols_array()
                    );
                    assert_eq!(expected_tint, actual_tint);
                    assert_eq!(expected_vertices.len(), actual_vertices.len());
                    for (expected, actual) in expected_vertices.iter().zip(actual_vertices.iter()) {
                        assert_eq!(expected.pos, actual.pos);
                        assert_eq!(expected.color, actual.color);
                    }
                }
                (
                    ObjectType::TexturedMesh {
                        instance: expected_instance,
                        vertices: expected_vertices,
                        geom_cache_key: expected_key,
                        depth_test: expected_depth,
                    },
                    ObjectType::TexturedMesh {
                        instance: actual_instance,
                        vertices: actual_vertices,
                        geom_cache_key: actual_key,
                        depth_test: actual_depth,
                    },
                ) => {
                    assert_eq!(expected_instance, actual_instance);
                    assert_eq!(expected_key, actual_key);
                    assert_eq!(expected_depth, actual_depth);
                    assert_eq!(expected_vertices.len(), actual_vertices.len());
                    for (expected, actual) in expected_vertices.iter().zip(actual_vertices.iter()) {
                        assert_eq!(expected.pos, actual.pos);
                        assert_eq!(expected.uv, actual.uv);
                        assert_eq!(expected.color, actual.color);
                        assert_eq!(expected.tex_matrix_scale, actual.tex_matrix_scale);
                    }
                }
                _ => panic!("render object kinds differ"),
            }
        }
    }

    fn compile(
        actors: &[Actor],
        fonts: &HashMap<&'static str, Font>,
        textures: &TestTextures,
    ) -> Result<CompiledScene, CompileError> {
        let metrics = metrics();
        let mut text_cache = TextLayoutCache::default();
        SceneCompiler::new(&metrics, fonts, textures, &mut text_cache, 7, 8).compile(
            actors,
            [0.01, 0.02, 0.03, 1.0],
            CompileOptions::IMMUTABLE,
        )
    }

    fn assert_compile_error(result: Result<CompiledScene, CompileError>, expected: CompileError) {
        match result {
            Err(actual) => assert_eq!(actual, expected),
            Ok(_) => panic!("scene unexpectedly compiled"),
        }
    }

    #[test]
    fn emitted_scene_matches_legacy_structure() {
        let actors = fixture();
        let fonts = fonts();
        let textures = textures();
        let metrics = metrics();
        let mut legacy_cache = TextLayoutCache::default();
        let mut legacy_scratch = ComposeScratch::default();
        let legacy = build_screen_cached_with_scratch_and_texture_context(
            &actors,
            [0.01, 0.02, 0.03, 1.0],
            &metrics,
            &fonts,
            0.0,
            &mut legacy_cache,
            &mut legacy_scratch,
            &textures,
        );
        let scene = compile(&actors, &fonts, &textures).expect("supported fixture compiles");
        let emitted = scene.emit(&mut CompiledSceneScratch::default());

        assert_render_eq(&legacy, &emitted);
        assert_eq!(
            scene.stats().compiled_primitives as usize,
            legacy.objects.len()
        );
        assert_eq!(scene.stamp().texture_revision, 9);
        assert_eq!(scene.primitives().len(), legacy.objects.len());
        assert!(scene.primitive(NodeId(0)).is_some());
        assert!(scene.primitive(NodeId(u32::MAX)).is_none());
    }

    #[test]
    fn warmed_emit_reuses_exact_capacity_without_lookups_or_sorting() {
        let actors = fixture();
        let fonts = fonts();
        let textures = textures();
        let scene = compile(&actors, &fonts, &textures).expect("supported fixture compiles");
        let calls_after_compile = textures.handle_calls.get();
        let mut scratch = CompiledSceneScratch::default();
        scratch.reserve(scene.capacity());
        let warmed_capacity = scratch.capacity();

        let mut first = scene.emit(&mut scratch);
        assert_eq!(scratch.frame_stats().scratch_growth_events, 0);
        assert_eq!(scratch.frame_stats().texture_refreshes, 0);
        assert_eq!(scratch.frame_stats().sort_fallbacks, 0);
        assert_eq!(textures.handle_calls.get(), calls_after_compile);
        scratch.recycle_render_list(&mut first);
        assert_eq!(scratch.capacity(), warmed_capacity);

        let mut second = scene.emit(&mut scratch);
        assert_eq!(scratch.frame_stats().scratch_growth_events, 0);
        assert_eq!(textures.handle_calls.get(), calls_after_compile);
        scratch.recycle_render_list(&mut second);
        assert_eq!(scratch.capacity(), warmed_capacity);

        let allocations = count_allocs(|| {
            for _ in 0..64 {
                let mut render = scene.emit(&mut scratch);
                scratch.recycle_render_list(&mut render);
            }
        });
        assert_eq!(allocations, 0);
        assert_eq!(scratch.frame_stats().scratch_growth_events, 0);
    }

    #[test]
    fn sprite_uv_slots_match_recomposed_offsets() {
        let fonts = fonts();
        let textures = textures();
        let mut base = sprite(SpriteSource::TextureStatic("plain"));
        let Actor::Sprite { uv_rect, glow, .. } = &mut base else {
            unreachable!()
        };
        *uv_rect = Some([0.0, 0.0, 1.0, 1.0]);
        *glow = [0.0; 4];
        let scene = compile(&[base.clone()], &fonts, &textures).expect("sprite compiles");
        let slot = scene
            .sprite_uv_slot(NodeId(0))
            .expect("first primitive is a sprite");
        let metrics = metrics();
        let mut legacy_cache = TextLayoutCache::default();
        let mut legacy_scratch = ComposeScratch::default();
        let mut compiled_scratch = CompiledSceneScratch::default();
        compiled_scratch.reserve(scene.capacity());

        for offset in [[0.0, 0.0], [0.25, -0.5], [12_345.5, -9_876.25]] {
            let mut current = base.clone();
            let Actor::Sprite { uv_rect, .. } = &mut current else {
                unreachable!()
            };
            *uv_rect = Some([offset[0], offset[1], offset[0] + 1.0, offset[1] + 1.0]);
            let legacy = build_screen_cached_with_scratch_and_texture_context(
                &[current],
                [0.01, 0.02, 0.03, 1.0],
                &metrics,
                &fonts,
                0.0,
                &mut legacy_cache,
                &mut legacy_scratch,
                &textures,
            );
            let mut emitted = scene
                .emit_with_uv_patches(&mut compiled_scratch, &[SpriteUvPatch { slot, offset }])
                .expect("slot remains valid for its scene");

            assert_render_eq(&legacy, &emitted);
            assert_eq!(compiled_scratch.frame_stats().patched_slots, 1);
            compiled_scratch.recycle_render_list(&mut emitted);
        }
    }

    #[test]
    fn warmed_uv_slot_updates_allocate_nothing() {
        let fonts = fonts();
        let textures = textures();
        let mut first = sprite(SpriteSource::TextureStatic("plain"));
        let mut second = sprite(SpriteSource::TextureStatic("plain"));
        for actor in [&mut first, &mut second] {
            let Actor::Sprite { glow, .. } = actor else {
                unreachable!()
            };
            *glow = [0.0; 4];
        }
        let scene = compile(&[first, second], &fonts, &textures).expect("sprites compile");
        let slots = [
            scene.sprite_uv_slot(NodeId(0)).expect("sprite slot 0"),
            scene.sprite_uv_slot(NodeId(1)).expect("sprite slot 1"),
        ];
        let mut scratch = CompiledSceneScratch::default();
        scratch.reserve(scene.capacity());
        let mut warm = scene.emit(&mut scratch);
        scratch.recycle_render_list(&mut warm);

        let allocations = count_allocs(|| {
            for frame in 0..64 {
                let patches = [
                    SpriteUvPatch {
                        slot: slots[0],
                        offset: [frame as f32 * 0.01, 0.0],
                    },
                    SpriteUvPatch {
                        slot: slots[1],
                        offset: [0.0, frame as f32 * -0.02],
                    },
                ];
                let mut render = scene
                    .emit_with_uv_patches(&mut scratch, &patches)
                    .expect("slots remain valid");
                scratch.recycle_render_list(&mut render);
            }
        });

        assert_eq!(allocations, 0);
        assert_eq!(scratch.frame_stats().patched_slots, 2);
        assert_eq!(scratch.frame_stats().scratch_growth_events, 0);
    }

    #[test]
    fn root_sprite_prefix_matches_whole_legacy_compose() {
        let fonts = fonts();
        let textures = textures();
        let metrics = metrics();
        let prefix_actors = vec![flat_sprite(-100, [0.0, 0.0]), flat_sprite(-99, [0.0, 0.0])];
        let rest = vec![
            // Equal-z legacy output must remain after the compiled actor that
            // preceded it in the original root list.
            flat_sprite(-99, [0.5, 0.25]),
            flat_sprite(-98, [0.75, 0.5]),
        ];
        let mut all = prefix_actors.clone();
        all.extend(rest.iter().cloned());

        let mut legacy_cache = TextLayoutCache::default();
        let mut legacy_scratch = ComposeScratch::default();
        let legacy = build_screen_cached_with_scratch_and_texture_context(
            &all,
            [0.01, 0.02, 0.03, 1.0],
            &metrics,
            &fonts,
            0.0,
            &mut legacy_cache,
            &mut legacy_scratch,
            &textures,
        );

        let mut scene = compile(&prefix_actors, &fonts, &textures).expect("prefix compiles");
        // The mixed path deliberately ignores the frozen camera and uses the
        // live composer camera (including its live overscan adjustment).
        scene.cameras[0] = Mat4::from_scale(Vec3::new(9.0, 7.0, 1.0));
        let prefix = scene
            .into_root_sprite_prefix()
            .expect("sprite-only root prefix validates");
        let slot = prefix
            .sprite_uv_slot(NodeId(1))
            .expect("second prefix sprite");
        let patches = [SpriteUvRectPatch {
            slot,
            scale: [1.0, 1.0],
            offset: [0.0, 0.0],
        }];
        let mut mixed_cache = TextLayoutCache::default();
        let mut mixed_scratch = ComposeScratch::default();
        let mixed = build_screen_cached_with_scratch_and_texture_context_and_root_prefix(
            &rest,
            [0.01, 0.02, 0.03, 1.0],
            &metrics,
            &fonts,
            0.0,
            &mut mixed_cache,
            &mut mixed_scratch,
            &textures,
            &prefix,
            &patches,
        )
        .expect("validated prefix composes");

        assert_render_eq(&legacy, &mixed);
        assert_eq!(mixed_scratch.frame_stats().compiled_prefix_primitives, 2);
        assert_eq!(mixed_scratch.frame_stats().compiled_prefix_sprites, 2);
        assert_eq!(mixed_scratch.frame_stats().compiled_prefix_patches, 1);
        assert_eq!(mixed.objects[1].z, -99);
        assert_eq!(mixed.objects[1].order, 1);
        assert_eq!(mixed.objects[2].z, -99);
        assert_eq!(mixed.objects[2].order, 2);
    }

    #[test]
    fn stale_root_prefix_patch_leaves_compose_scratch_untouched() {
        let fonts = fonts();
        let textures = textures();
        let metrics = metrics();
        let actors = [flat_sprite(-99, [0.0, 0.0])];
        let first = compile(&actors, &fonts, &textures)
            .expect("first scene compiles")
            .into_root_sprite_prefix()
            .expect("first prefix validates");
        let second = compile(&actors, &fonts, &textures)
            .expect("second scene compiles")
            .into_root_sprite_prefix()
            .expect("second prefix validates");
        let stale = second.sprite_uv_slot(NodeId(0)).expect("second scene slot");
        let mut text_cache = TextLayoutCache::default();
        let mut scratch = ComposeScratch::default();
        let mut warm = build_screen_cached_with_scratch_and_texture_context_and_root_prefix(
            &[],
            [0.0, 0.0, 0.0, 1.0],
            &metrics,
            &fonts,
            0.0,
            &mut text_cache,
            &mut scratch,
            &textures,
            &first,
            &[],
        )
        .expect("warm compose succeeds");
        scratch.recycle_render_list(&mut warm);
        let stats_before = scratch.frame_stats();

        let error = match build_screen_cached_with_scratch_and_texture_context_and_root_prefix(
            &[],
            [0.0, 0.0, 0.0, 1.0],
            &metrics,
            &fonts,
            0.0,
            &mut text_cache,
            &mut scratch,
            &textures,
            &first,
            &[SpriteUvRectPatch {
                slot: stale,
                scale: [1.0, 1.0],
                offset: [0.25, 0.5],
            }],
        ) {
            Err(error) => error,
            Ok(_) => panic!("cross-scene slot unexpectedly composed"),
        };

        assert!(matches!(
            error,
            RootPrefixError::Patch(PatchError::WrongScene(NodeId(0)))
        ));
        assert_eq!(scratch.frame_stats(), stats_before);
    }

    #[test]
    fn warmed_root_prefix_compose_allocates_nothing() {
        let fonts = fonts();
        let textures = textures();
        let metrics = metrics();
        let prefix = compile(
            &[flat_sprite(-100, [0.0, 0.0]), flat_sprite(-99, [0.0, 0.0])],
            &fonts,
            &textures,
        )
        .expect("prefix compiles")
        .into_root_sprite_prefix()
        .expect("prefix validates");
        let slot = prefix.sprite_uv_slot(NodeId(1)).expect("patchable sprite");
        let rest = [flat_sprite(-98, [0.0, 0.0])];
        let mut text_cache = TextLayoutCache::default();
        let mut scratch = ComposeScratch::default();
        let mut warm = build_screen_cached_with_scratch_and_texture_context_and_root_prefix(
            &rest,
            [0.0, 0.0, 0.0, 1.0],
            &metrics,
            &fonts,
            0.0,
            &mut text_cache,
            &mut scratch,
            &textures,
            &prefix,
            &[SpriteUvRectPatch {
                slot,
                scale: [1.0, 1.0],
                offset: [0.0, 0.0],
            }],
        )
        .expect("warm compose succeeds");
        scratch.recycle_render_list(&mut warm);

        let allocations = count_allocs(|| {
            for frame in 0..64 {
                let patches = [SpriteUvRectPatch {
                    slot,
                    scale: [1.0, 1.0],
                    offset: [frame as f32 * 0.01, frame as f32 * -0.02],
                }];
                let mut render =
                    build_screen_cached_with_scratch_and_texture_context_and_root_prefix(
                        &rest,
                        [0.0, 0.0, 0.0, 1.0],
                        &metrics,
                        &fonts,
                        0.0,
                        &mut text_cache,
                        &mut scratch,
                        &textures,
                        &prefix,
                        &patches,
                    )
                    .expect("validated patch remains current");
                scratch.recycle_render_list(&mut render);
            }
        });

        assert_eq!(allocations, 0);
        assert_eq!(scratch.frame_stats().scratch_growth_events, 0);
        assert_eq!(scratch.frame_stats().compiled_prefix_primitives, 2);
    }

    #[test]
    fn retained_draw_frame_matches_legacy_draw_preparation() {
        let mut invalid_key = textured_mesh();
        let Actor::TexturedMesh { geom_cache_key, .. } = &mut invalid_key else {
            unreachable!()
        };
        *geom_cache_key = INVALID_TMESH_CACHE_KEY;
        let mut actors = fixture();
        actors.push(textured_mesh());
        actors.push(invalid_key);
        let fonts = fonts();
        let textures = textures();
        let scene = compile(&actors, &fonts, &textures).expect("fixture compiles");
        let mut emit_scratch = CompiledSceneScratch::default();
        let render = scene.emit(&mut emit_scratch);
        let direct = scene
            .compile_draw_frame()
            .expect("fixture geometry keys are consistent");
        let mut expected_scratch = DrawScratch::default();
        let (expected, _) = prepare_render_list(&render, &mut expected_scratch, |key, vertices| {
            let geometry = direct
                .geometries()
                .iter()
                .find(|geometry| geometry.cache_key == key)
                .expect("every cached run has retained geometry");
            assert_tmesh_vertices_eq(geometry.vertices.as_ref(), vertices);
            TMeshCacheResult::Resident
        });
        let actual = direct.frame().view();

        assert_eq!(actual.clear_color, expected.clear_color);
        assert_eq!(actual.cameras, expected.cameras);
        assert_eq!(actual.sprite_instances, expected.sprite_instances);
        assert_eq!(actual.mesh_vertices.len(), expected.mesh_vertices.len());
        for (actual, expected) in actual.mesh_vertices.iter().zip(expected.mesh_vertices) {
            assert_eq!(actual.pos, expected.pos);
            assert_eq!(actual.color, expected.color);
        }
        assert_eq!(actual.tmesh_vertices.len(), expected.tmesh_vertices.len());
        for (actual, expected) in actual.tmesh_vertices.iter().zip(expected.tmesh_vertices) {
            assert_eq!(actual.pos, expected.pos);
            assert_eq!(actual.uv, expected.uv);
            assert_eq!(actual.color, expected.color);
            assert_eq!(actual.tex_matrix_scale, expected.tex_matrix_scale);
        }
        assert_eq!(actual.tmesh_instances, expected.tmesh_instances);
        assert_eq!(actual.ops, expected.ops);
        assert_eq!(direct.prepare_stats().draw_ops as usize, actual.ops.len());
        assert_eq!(direct.stamp(), scene.stamp());
        assert!(direct.is_current(7, 8, &textures));
        assert!(!direct.is_current(6, 8, &textures));
        assert!(!direct.is_current(7, 9, &textures));
        let invalid_vertex_count = render
            .objects
            .iter()
            .filter_map(|object| match &object.object_type {
                ObjectType::TexturedMesh {
                    vertices,
                    geom_cache_key: INVALID_TMESH_CACHE_KEY,
                    ..
                } => Some(vertices.len()),
                ObjectType::Sprite(_)
                | ObjectType::Mesh { .. }
                | ObjectType::TexturedMesh { .. } => None,
            })
            .sum::<usize>();
        assert_eq!(actual.tmesh_vertices.len(), invalid_vertex_count);
        assert_eq!(
            direct.prepare_stats().dynamic_upload_vertices,
            invalid_vertex_count as u64
        );
        assert_eq!(
            direct
                .geometries()
                .iter()
                .filter(|geometry| geometry.cache_key == 44)
                .count(),
            1,
            "duplicate keyed actors retain one upload payload"
        );
        assert!(
            direct
                .geometries()
                .iter()
                .any(|geometry| geometry.cache_key != 44),
            "static text geometry is retained"
        );
        assert!(actual.ops.iter().any(|op| matches!(
            op,
            DrawOp::TexturedMesh(run) if matches!(run.source, TexturedMeshSource::Cached { .. })
        )));
        assert!(actual.ops.iter().any(|op| matches!(
            op,
            DrawOp::TexturedMesh(run) if matches!(run.source, TexturedMeshSource::Transient { .. })
        )));
    }

    #[test]
    fn retained_geometry_deduplicates_identical_keys() {
        let fonts = fonts();
        let textures = textures();
        let scene = compile(&[textured_mesh(), textured_mesh()], &fonts, &textures)
            .expect("identical keyed meshes compile");
        let direct = scene
            .compile_draw_frame()
            .expect("identical keyed meshes freeze");

        assert_eq!(direct.geometries().len(), 1);
        assert_eq!(direct.geometries()[0].cache_key, 44);
        assert_eq!(
            direct.geometries()[0].fingerprint,
            tmesh_fingerprint(direct.geometries()[0].vertices.as_ref())
        );
    }

    #[test]
    fn retained_geometry_rejects_key_with_different_count() {
        let mut conflicting = textured_mesh();
        let Actor::TexturedMesh { vertices, .. } = &mut conflicting else {
            unreachable!()
        };
        *vertices = Arc::from([TexturedMeshVertex::default(); 2]);
        let fonts = fonts();
        let textures = textures();
        let scene = compile(&[textured_mesh(), conflicting], &fonts, &textures)
            .expect("actor snapshot compiles before retained geometry validation");

        assert_eq!(
            scene.compile_draw_frame().err(),
            Some(CompileError::ConflictingGeometryKey { cache_key: 44 })
        );
    }

    #[test]
    fn retained_geometry_rejects_key_with_different_data() {
        let mut conflicting = textured_mesh();
        let Actor::TexturedMesh { vertices, .. } = &mut conflicting else {
            unreachable!()
        };
        let mut changed = vertices.to_vec();
        changed[0].pos[0] = 99.0;
        *vertices = Arc::from(changed);
        let fonts = fonts();
        let textures = textures();
        let scene = compile(&[textured_mesh(), conflicting], &fonts, &textures)
            .expect("actor snapshot compiles before retained geometry validation");

        assert_eq!(
            scene.compile_draw_frame().err(),
            Some(CompileError::ConflictingGeometryKey { cache_key: 44 })
        );
    }

    #[test]
    fn retained_draw_frame_uv_updates_allocate_nothing() {
        let fonts = fonts();
        let textures = textures();
        let mut actor = sprite(SpriteSource::TextureStatic("plain"));
        let Actor::Sprite { glow, z, .. } = &mut actor else {
            unreachable!()
        };
        *glow = [0.0; 4];
        *z = -2;
        let scene = compile(&[actor, textured_mesh()], &fonts, &textures).expect("scene compiles");
        let slot = scene.sprite_uv_slot(NodeId(0)).expect("sprite slot");
        let mut direct = scene
            .compile_draw_frame()
            .expect("fixture geometry keys are consistent");
        assert_eq!(direct.geometries().len(), 1);
        assert!(direct.frame().tmesh_vertices.is_empty());

        let allocations = count_allocs(|| {
            for frame in 0..64 {
                direct
                    .apply_uv_patches(&[SpriteUvPatch {
                        slot,
                        offset: [frame as f32 * 0.01, frame as f32 * -0.02],
                    }])
                    .expect("slot remains valid");
            }
        });

        assert_eq!(allocations, 0);
        assert_eq!(direct.frame().sprite_instances[0].uv_offset, [0.63, -1.26]);
    }

    #[test]
    fn sprite_uv_slots_reject_wrong_or_missing_nodes() {
        let fonts = fonts();
        let textures = textures();
        let scene = compile(&[text()], &fonts, &textures).expect("text compiles");

        assert_eq!(
            scene.sprite_uv_slot(NodeId(0)),
            Err(PatchError::NotSprite(NodeId(0)))
        );
        assert_eq!(
            scene.sprite_uv_slot(NodeId(10)),
            Err(PatchError::UnknownNode(NodeId(10)))
        );
    }

    #[test]
    fn sprite_uv_slots_reject_other_scene_generations() {
        let fonts = fonts();
        let textures = textures();
        let mut actor = sprite(SpriteSource::TextureStatic("plain"));
        let Actor::Sprite { glow, .. } = &mut actor else {
            unreachable!()
        };
        *glow = [0.0; 4];
        let first = compile(&[actor.clone()], &fonts, &textures).expect("first scene compiles");
        let second = compile(&[actor], &fonts, &textures).expect("second scene compiles");
        let stale = first.sprite_uv_slot(NodeId(0)).expect("first sprite slot");
        let patch = SpriteUvPatch {
            slot: stale,
            offset: [1.0, 2.0],
        };

        let mut scratch = CompiledSceneScratch::default();
        assert_eq!(
            second.emit_with_uv_patches(&mut scratch, &[patch]).err(),
            Some(PatchError::WrongScene(NodeId(0)))
        );
        let mut direct = second
            .compile_draw_frame()
            .expect("fixture geometry keys are consistent");
        assert_eq!(
            direct.apply_uv_patches(&[patch]),
            Err(PatchError::WrongScene(NodeId(0)))
        );
    }

    #[test]
    fn rejects_actor_features_with_nested_paths() {
        let fonts = fonts();
        let textures = textures();
        let mut cases = Vec::new();

        let mut mask = sprite(SpriteSource::Solid);
        let Actor::Sprite { mask_source, .. } = &mut mask else {
            unreachable!()
        };
        *mask_source = true;
        cases.push((mask, UnsupportedFeature::SpriteMaskSource));

        let mut mask_dest_actor = sprite(SpriteSource::Solid);
        let Actor::Sprite { mask_dest, .. } = &mut mask_dest_actor else {
            unreachable!()
        };
        *mask_dest = true;
        cases.push((mask_dest_actor, UnsupportedFeature::SpriteMaskDestination));

        let mut animation = sprite(SpriteSource::Solid);
        let Actor::Sprite { animate, .. } = &mut animation else {
            unreachable!()
        };
        *animate = true;
        cases.push((animation, UnsupportedFeature::SpriteAnimation));

        let mut velocity = sprite(SpriteSource::Solid);
        let Actor::Sprite {
            texcoordvelocity, ..
        } = &mut velocity
        else {
            unreachable!()
        };
        *texcoordvelocity = Some([1.0, 0.0]);
        cases.push((velocity, UnsupportedFeature::TextureCoordinateVelocity));

        let mut effect = sprite(SpriteSource::Solid);
        let Actor::Sprite {
            effect: effect_state,
            ..
        } = &mut effect
        else {
            unreachable!()
        };
        effect_state.mode = EffectMode::Pulse;
        cases.push((effect, UnsupportedFeature::Effect));

        let mut sprite_shadow = sprite(SpriteSource::Solid);
        let Actor::Sprite { shadow_len, .. } = &mut sprite_shadow else {
            unreachable!()
        };
        *shadow_len = [1.0, 0.0];
        cases.push((sprite_shadow, UnsupportedFeature::Shadow));

        let mut attributed = text();
        let Actor::Text { attributes, .. } = &mut attributed else {
            unreachable!()
        };
        attributes.push(TextAttribute {
            start: 0,
            length: 1,
            color: [1.0; 4],
            vertex_colors: None,
            glow: None,
        });
        cases.push((attributed, UnsupportedFeature::AttributedText));

        for (actor, feature) in cases {
            let nested = [Actor::Frame {
                align: [0.0, 0.0],
                offset: [0.0, 0.0],
                size: [SizeSpec::Fill, SizeSpec::Fill],
                children: vec![actor],
                background: None,
                z: 0,
            }];
            assert_compile_error(
                compile(&nested, &fonts, &textures),
                CompileError::Unsupported {
                    path: vec![0, 0],
                    feature,
                },
            );
        }
    }

    #[test]
    fn rejects_transient_text_shadow_and_clipping() {
        let fonts = fonts();
        let textures = textures();

        let mut cases = Vec::new();
        let mut transient = text();
        let Actor::Text { content, .. } = &mut transient else {
            unreachable!()
        };
        *content = TextContent::Owned("AB".to_owned());
        cases.push((transient, UnsupportedFeature::TransientText));

        let mut clipped = text();
        let Actor::Text { clip, .. } = &mut clipped else {
            unreachable!()
        };
        *clip = Some([0.0, 0.0, 1.0, 1.0]);
        cases.push((clipped, UnsupportedFeature::TextClip));

        let mut masked = text();
        let Actor::Text { mask_dest, .. } = &mut masked else {
            unreachable!()
        };
        *mask_dest = true;
        cases.push((masked, UnsupportedFeature::TextMaskDestination));

        let mut jittered = text();
        let Actor::Text { jitter, .. } = &mut jittered else {
            unreachable!()
        };
        *jitter = true;
        cases.push((jittered, UnsupportedFeature::JitteredText));

        let mut distorted = text();
        let Actor::Text { distortion, .. } = &mut distorted else {
            unreachable!()
        };
        *distortion = 0.5;
        cases.push((distorted, UnsupportedFeature::DistortedText));

        let mut effected = text();
        let Actor::Text { effect, .. } = &mut effected else {
            unreachable!()
        };
        effect.mode = EffectMode::DiffuseShift;
        cases.push((effected, UnsupportedFeature::Effect));

        let shadow = Actor::Shadow {
            len: [1.0, 1.0],
            color: [0.0, 0.0, 0.0, 0.5],
            child: Box::new(text()),
        };
        cases.push((shadow, UnsupportedFeature::Shadow));

        for (actor, feature) in cases {
            assert_compile_error(
                compile(&[actor], &fonts, &textures),
                CompileError::Unsupported {
                    path: vec![0],
                    feature,
                },
            );
        }
    }

    #[test]
    fn rejects_hidden_provenance_explicitly() {
        let fonts = fonts();
        let textures = textures();
        let metrics = metrics();
        let mut cache = TextLayoutCache::default();
        let mut compiler = SceneCompiler::new(&metrics, &fonts, &textures, &mut cache, 0, 0);

        assert_compile_error(
            compiler.compile(
                &[],
                [0.0; 4],
                CompileOptions {
                    has_dsl_tweens: true,
                    has_song_lua_capture_proxy: false,
                },
            ),
            CompileError::Unsupported {
                path: Vec::new(),
                feature: UnsupportedFeature::DslTween,
            },
        );
        assert_compile_error(
            compiler.compile(
                &[],
                [0.0; 4],
                CompileOptions {
                    has_dsl_tweens: false,
                    has_song_lua_capture_proxy: true,
                },
            ),
            CompileError::Unsupported {
                path: Vec::new(),
                feature: UnsupportedFeature::SongLuaCaptureProxy,
            },
        );
    }

    #[test]
    fn rejects_unbalanced_camera_scopes_and_missing_resources() {
        let fonts = fonts();
        let textures = textures();
        assert_compile_error(
            compile(&[Actor::CameraPop], &fonts, &textures),
            CompileError::Unsupported {
                path: vec![0],
                feature: UnsupportedFeature::UnbalancedCameraScope,
            },
        );
        assert_compile_error(
            compile(
                &[Actor::CameraPush {
                    view_proj: Mat4::IDENTITY,
                }],
                &fonts,
                &textures,
            ),
            CompileError::Unsupported {
                path: vec![0],
                feature: UnsupportedFeature::UnbalancedCameraScope,
            },
        );

        let missing = sprite(SpriteSource::TextureStatic("missing"));
        assert_compile_error(
            compile(&[missing], &fonts, &textures),
            CompileError::MissingTexture {
                path: vec![0],
                key: Arc::from("missing"),
            },
        );

        let mut missing_font = text();
        let Actor::Text { font, .. } = &mut missing_font else {
            unreachable!()
        };
        *font = "missing";
        assert_compile_error(
            compile(&[missing_font], &fonts, &textures),
            CompileError::MissingFont {
                path: vec![0],
                font: "missing",
            },
        );
    }

    #[test]
    fn frozen_textured_mesh_geometry_is_shared() {
        let fonts = fonts();
        let textures = textures();
        let scene = compile(&[textured_mesh()], &fonts, &textures).expect("mesh compiles");
        let object = scene.primitive(NodeId(0)).expect("one primitive");
        let ObjectType::TexturedMesh { vertices, .. } = &object.object_type else {
            panic!("expected textured mesh");
        };
        assert!(matches!(vertices, TexturedMeshVertices::Shared(_)));
        let instance = match &object.object_type {
            ObjectType::TexturedMesh { instance, .. } => *instance,
            _ => TexturedMeshInstanceRaw::new(
                Mat4::IDENTITY,
                [1.0; 4],
                [1.0; 2],
                [0.0; 2],
                [0.0; 2],
                false,
            ),
        };
        assert_eq!(instance.texture_mask, 0.0);
    }
}
