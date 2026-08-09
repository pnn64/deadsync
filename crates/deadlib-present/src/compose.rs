use crate::actors::{self, SizeSpec};
use crate::{anim, font, space};
use deadlib_render as renderer;
use deadlib_render::{BlendMode, RenderFrame};
use glam::{Mat4 as Matrix4, Vec2 as Vector2, Vec3 as Vector3, Vec4 as Vector4};
use smallvec::SmallVec;
use space::Metrics;
use std::cell::{Cell, OnceCell};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::num::NonZeroU32;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Instant;

/* ======================= RENDERER SCREEN BUILDER ======================= */

const MAX_RECYCLED_TEXT_MESH_VERTEX_BUFFERS: usize = 512;
const MAX_RETAINED_FRAME_ENTRIES: usize = 64;

/// Detached draw used only while clipping or copying a retained fragment.
/// Live frame composition stays in `FrameBuilder`'s typed arrays.
#[repr(C)]
#[derive(Clone)]
struct EditableDraw {
    texture_handle: renderer::TextureHandle,
    order: u32,
    z: i16,
    blend: BlendMode,
    camera: u8,
    object_type: EditablePayload,
}

/// Compact sortable header pointing at a typed composition payload.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DrawItem {
    texture_handle: renderer::TextureHandle,
    order: u32,
    payload_index: u32,
    z: i16,
    blend: BlendMode,
    camera: u8,
    kind: DrawKind,
}

impl DrawItem {
    #[inline(always)]
    const fn sort_key(self) -> (i16, u32) {
        (self.z, self.order)
    }

    #[cfg(any(test, feature = "bench-support"))]
    const fn synthetic(z: i16, order: u32, payload_index: u32) -> Self {
        Self {
            texture_handle: payload_index as renderer::TextureHandle,
            order,
            payload_index,
            z,
            blend: BlendMode::Alpha,
            camera: 0,
            kind: DrawKind::Sprite,
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DrawKind {
    Sprite,
    Mesh,
    TexturedMesh,
}

#[derive(Clone)]
struct MeshPayload {
    transform: Matrix4,
    tint: [f32; 4],
    vertices: MeshVertices,
}

#[derive(Clone)]
struct TexturedMeshPayload {
    instance: renderer::TexturedMeshInstanceRaw,
    vertices: renderer::TexturedMeshVertices,
    geom_cache_key: renderer::TMeshCacheKey,
    depth_test: bool,
}

#[derive(Default)]
struct FrameBuilder {
    items: Vec<DrawItem>,
    meshes: Vec<Option<MeshPayload>>,
    textured_meshes: Vec<Option<TexturedMeshPayload>>,
}

impl FrameBuilder {
    #[inline(always)]
    fn len(&self) -> usize {
        self.items.len()
    }

    fn clear(&mut self) {
        self.items.clear();
        self.meshes.clear();
        self.textured_meshes.clear();
    }

    fn reserve(&mut self, additional: usize) {
        self.items.reserve(additional);
    }

    fn append_retained(&mut self, cached: &Self, sprite_start: u32, order_counter: &mut u32) {
        let mesh_start = saturating_u32(self.meshes.len());
        let textured_mesh_start = saturating_u32(self.textured_meshes.len());
        self.items.reserve(cached.items.len());
        self.meshes.reserve(cached.meshes.len());
        self.textured_meshes.reserve(cached.textured_meshes.len());
        self.meshes.extend(cached.meshes.iter().cloned());
        self.textured_meshes
            .extend(cached.textured_meshes.iter().cloned());

        self.items.extend(cached.items.iter().map(|item| {
            let mut item = *item;
            item.payload_index = match item.kind {
                DrawKind::Sprite => item.payload_index.saturating_add(sprite_start),
                DrawKind::Mesh => item.payload_index.saturating_add(mesh_start),
                DrawKind::TexturedMesh => item.payload_index.saturating_add(textured_mesh_start),
            };
            item.order = *order_counter;
            *order_counter = order_counter.saturating_add(1);
            item
        }));
    }

    fn truncate(&mut self, len: usize) {
        self.items.truncate(len);
    }

    fn swap(&mut self, left: usize, right: usize) {
        self.items.swap(left, right);
    }

    #[inline(always)]
    fn push_sprite(
        &mut self,
        texture_handle: renderer::TextureHandle,
        order: u32,
        z: i16,
        blend: BlendMode,
        camera: u8,
        instance_index: u32,
    ) {
        self.items.push(DrawItem {
            texture_handle,
            order,
            payload_index: instance_index,
            z,
            blend,
            camera,
            kind: DrawKind::Sprite,
        });
    }

    #[inline(always)]
    fn push_mesh(
        &mut self,
        texture_handle: renderer::TextureHandle,
        order: u32,
        z: i16,
        blend: BlendMode,
        camera: u8,
        payload: MeshPayload,
    ) {
        let payload_index = saturating_u32(self.meshes.len());
        self.meshes.push(Some(payload));
        self.items.push(DrawItem {
            texture_handle,
            order,
            payload_index,
            z,
            blend,
            camera,
            kind: DrawKind::Mesh,
        });
    }

    #[inline(always)]
    fn push_textured_mesh(
        &mut self,
        texture_handle: renderer::TextureHandle,
        order: u32,
        z: i16,
        blend: BlendMode,
        camera: u8,
        payload: TexturedMeshPayload,
    ) {
        let payload_index = saturating_u32(self.textured_meshes.len());
        self.textured_meshes.push(Some(payload));
        self.items.push(DrawItem {
            texture_handle,
            order,
            payload_index,
            z,
            blend,
            camera,
            kind: DrawKind::TexturedMesh,
        });
    }

    #[inline(always)]
    fn push(&mut self, object: EditableDraw) {
        let EditableDraw {
            texture_handle,
            order,
            z,
            blend,
            camera,
            object_type,
        } = object;
        match object_type {
            EditablePayload::Sprite(instance_index) => {
                self.push_sprite(texture_handle, order, z, blend, camera, instance_index)
            }
            EditablePayload::Mesh {
                transform,
                tint,
                vertices,
            } => self.push_mesh(
                texture_handle,
                order,
                z,
                blend,
                camera,
                MeshPayload {
                    transform,
                    tint,
                    vertices,
                },
            ),
            EditablePayload::TexturedMesh {
                instance,
                vertices,
                geom_cache_key,
                depth_test,
            } => self.push_textured_mesh(
                texture_handle,
                order,
                z,
                blend,
                camera,
                TexturedMeshPayload {
                    instance,
                    vertices,
                    geom_cache_key,
                    depth_test,
                },
            ),
        }
    }

    fn take_object(&mut self, index: usize) -> EditableDraw {
        let item = self.items[index];
        EditableDraw {
            texture_handle: item.texture_handle,
            order: item.order,
            z: item.z,
            blend: item.blend,
            camera: item.camera,
            object_type: self.take_payload(item),
        }
    }

    fn replace_object(&mut self, index: usize, object: EditableDraw) {
        let EditableDraw {
            texture_handle,
            order,
            z,
            blend,
            camera,
            object_type,
        } = object;
        let item_index = self.items.len();
        self.push(EditableDraw {
            texture_handle,
            order,
            z,
            blend,
            camera,
            object_type,
        });
        self.items.swap(index, item_index);
        self.items.pop();
    }

    fn clone_retained_object(&self, index: usize) -> Option<EditableDraw> {
        let item = self.items[index];
        let object_type = match item.kind {
            DrawKind::Sprite => EditablePayload::Sprite(item.payload_index),
            DrawKind::Mesh => {
                let payload = self.meshes.get(item.payload_index as usize)?.as_ref()?;
                EditablePayload::Mesh {
                    transform: payload.transform,
                    tint: payload.tint,
                    vertices: payload.vertices.clone(),
                }
            }
            DrawKind::TexturedMesh => {
                let payload = self
                    .textured_meshes
                    .get(item.payload_index as usize)?
                    .as_ref()?;
                if matches!(
                    payload.vertices,
                    renderer::TexturedMeshVertices::Transient(_)
                ) {
                    return None;
                }
                EditablePayload::TexturedMesh {
                    instance: payload.instance,
                    vertices: payload.vertices.clone(),
                    geom_cache_key: payload.geom_cache_key,
                    depth_test: payload.depth_test,
                }
            }
        };
        Some(EditableDraw {
            texture_handle: item.texture_handle,
            order: item.order,
            z: item.z,
            blend: item.blend,
            camera: item.camera,
            object_type,
        })
    }

    fn take_payload(&mut self, item: DrawItem) -> EditablePayload {
        match item.kind {
            DrawKind::Sprite => EditablePayload::Sprite(item.payload_index),
            DrawKind::Mesh => {
                let payload = self.meshes[item.payload_index as usize]
                    .take()
                    .expect("draw item references live mesh payload");
                EditablePayload::Mesh {
                    transform: payload.transform,
                    tint: payload.tint,
                    vertices: payload.vertices,
                }
            }
            DrawKind::TexturedMesh => {
                let payload = self.textured_meshes[item.payload_index as usize]
                    .take()
                    .expect("draw item references live textured-mesh payload");
                EditablePayload::TexturedMesh {
                    instance: payload.instance,
                    vertices: payload.vertices,
                    geom_cache_key: payload.geom_cache_key,
                    depth_test: payload.depth_test,
                }
            }
        }
    }
}

#[derive(Clone)]
enum EditablePayload {
    Sprite(u32),
    Mesh {
        transform: Matrix4,
        tint: [f32; 4],
        vertices: MeshVertices,
    },
    TexturedMesh {
        instance: renderer::TexturedMeshInstanceRaw,
        vertices: renderer::TexturedMeshVertices,
        geom_cache_key: renderer::TMeshCacheKey,
        depth_test: bool,
    },
}

#[derive(Clone)]
enum MeshVertices {
    Shared(Arc<[renderer::MeshVertex]>),
    Reusable(Arc<Vec<renderer::MeshVertex>>),
}

impl AsRef<[renderer::MeshVertex]> for MeshVertices {
    #[inline(always)]
    fn as_ref(&self) -> &[renderer::MeshVertex] {
        match self {
            Self::Shared(vertices) => vertices.as_ref(),
            Self::Reusable(vertices) => vertices.as_slice(),
        }
    }
}

impl Deref for MeshVertices {
    type Target = [renderer::MeshVertex];

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

pub use crate::texture::{NullTextureContext, TextureContext, TextureMeta};

const NULL_TEXTURE_CONTEXT: NullTextureContext = NullTextureContext;

#[inline(always)]
pub fn build_screen(
    actors: &[actors::Actor],
    clear_color: [f32; 4],
    m: &Metrics,
    fonts: &font::FontMap,
    total_elapsed: f32,
) -> RenderFrame {
    build_screen_with_texture_context(
        actors,
        clear_color,
        m,
        fonts,
        total_elapsed,
        &NULL_TEXTURE_CONTEXT,
    )
}

#[inline(always)]
pub fn build_screen_with_texture_context<T: TextureContext + ?Sized>(
    actors: &[actors::Actor],
    clear_color: [f32; 4],
    m: &Metrics,
    fonts: &font::FontMap,
    total_elapsed: f32,
    texture_ctx: &T,
) -> RenderFrame {
    let mut text_cache = TextLayoutCache::default();
    build_screen_cached_with_texture_context(
        actors,
        clear_color,
        m,
        fonts,
        total_elapsed,
        &mut text_cache,
        texture_ctx,
    )
}

#[inline(always)]
pub fn build_screen_cached(
    actors: &[actors::Actor],
    clear_color: [f32; 4],
    m: &Metrics,
    fonts: &font::FontMap,
    total_elapsed: f32,
    text_cache: &mut TextLayoutCache,
) -> RenderFrame {
    build_screen_cached_with_texture_context(
        actors,
        clear_color,
        m,
        fonts,
        total_elapsed,
        text_cache,
        &NULL_TEXTURE_CONTEXT,
    )
}

#[inline(always)]
pub fn build_screen_cached_with_texture_context<T: TextureContext + ?Sized>(
    actors: &[actors::Actor],
    clear_color: [f32; 4],
    m: &Metrics,
    fonts: &font::FontMap,
    total_elapsed: f32,
    text_cache: &mut TextLayoutCache,
    texture_ctx: &T,
) -> RenderFrame {
    let mut scratch = ComposeScratch::default();
    build_screen_cached_with_scratch_and_texture_context(
        actors,
        clear_color,
        m,
        fonts,
        total_elapsed,
        text_cache,
        &mut scratch,
        texture_ctx,
    )
}

#[inline(always)]
pub fn build_screen_cached_with_scratch(
    actors: &[actors::Actor],
    clear_color: [f32; 4],
    m: &Metrics,
    fonts: &font::FontMap,
    total_elapsed: f32,
    text_cache: &mut TextLayoutCache,
    scratch: &mut ComposeScratch,
) -> RenderFrame {
    build_screen_cached_with_scratch_and_texture_context(
        actors,
        clear_color,
        m,
        fonts,
        total_elapsed,
        text_cache,
        scratch,
        &NULL_TEXTURE_CONTEXT,
    )
}

#[inline(always)]
pub fn build_screen_cached_with_scratch_and_texture_context<T: TextureContext + ?Sized>(
    actors: &[actors::Actor],
    clear_color: [f32; 4],
    m: &Metrics,
    fonts: &font::FontMap,
    total_elapsed: f32,
    text_cache: &mut TextLayoutCache,
    scratch: &mut ComposeScratch,
    texture_ctx: &T,
) -> RenderFrame {
    build_screen_cached_with_scratch_and_texture_context_impl(
        actors,
        clear_color,
        m,
        fonts,
        total_elapsed,
        text_cache,
        scratch,
        texture_ctx,
        None,
    )
}

#[inline(always)]
#[allow(clippy::too_many_arguments)]
pub fn build_screen_cached_with_scratch_and_texture_context_and_actor_resources<
    T: TextureContext + ?Sized,
>(
    actors: &[actors::Actor],
    clear_color: [f32; 4],
    m: &Metrics,
    fonts: &font::FontMap,
    total_elapsed: f32,
    text_cache: &mut TextLayoutCache,
    scratch: &mut ComposeScratch,
    texture_ctx: &T,
    actor_resources: &actors::ActorResourceArena,
) -> RenderFrame {
    build_screen_cached_with_scratch_and_texture_context_impl(
        actors,
        clear_color,
        m,
        fonts,
        total_elapsed,
        text_cache,
        scratch,
        texture_ctx,
        Some(actor_resources),
    )
}

#[inline(always)]
#[allow(clippy::too_many_arguments)]
/// Composes ordered borrowed actor slices as one continuous actor stream.
/// Camera scopes and painter order may cross slice boundaries.
pub fn build_screen_segments_cached_with_scratch_and_texture_context_and_actor_resources<
    T: TextureContext + ?Sized,
>(
    actor_segments: &[ActorSegment<'_>],
    clear_color: [f32; 4],
    m: &Metrics,
    fonts: &font::FontMap,
    total_elapsed: f32,
    text_cache: &mut TextLayoutCache,
    scratch: &mut ComposeScratch,
    texture_ctx: &T,
    actor_resources: &actors::ActorResourceArena,
) -> RenderFrame {
    build_screen_segments_cached_with_scratch_and_texture_context_impl(
        actor_segments,
        clear_color,
        m,
        fonts,
        total_elapsed,
        text_cache,
        scratch,
        texture_ctx,
        Some(actor_resources),
    )
}

/// A borrowed actor slice with a layer shift applied during composition.
#[derive(Clone, Copy, Debug)]
pub struct ActorSegment<'a> {
    actors: &'a [actors::Actor],
    z_shift: i16,
}

impl<'a> ActorSegment<'a> {
    pub const fn new(actors: &'a [actors::Actor]) -> Self {
        Self { actors, z_shift: 0 }
    }

    pub const fn shifted(actors: &'a [actors::Actor], z_shift: i16) -> Self {
        Self { actors, z_shift }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_screen_cached_with_scratch_and_texture_context_impl<T: TextureContext + ?Sized>(
    actors: &[actors::Actor],
    clear_color: [f32; 4],
    m: &Metrics,
    fonts: &font::FontMap,
    total_elapsed: f32,
    text_cache: &mut TextLayoutCache,
    scratch: &mut ComposeScratch,
    texture_ctx: &T,
    actor_resources: Option<&actors::ActorResourceArena>,
) -> RenderFrame {
    build_screen_segments_cached_with_scratch_and_texture_context_impl(
        &[ActorSegment::new(actors)],
        clear_color,
        m,
        fonts,
        total_elapsed,
        text_cache,
        scratch,
        texture_ctx,
        actor_resources,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_screen_segments_cached_with_scratch_and_texture_context_impl<
    T: TextureContext + ?Sized,
>(
    actor_segments: &[ActorSegment<'_>],
    clear_color: [f32; 4],
    m: &Metrics,
    fonts: &font::FontMap,
    total_elapsed: f32,
    text_cache: &mut TextLayoutCache,
    scratch: &mut ComposeScratch,
    texture_ctx: &T,
    actor_resources: Option<&actors::ActorResourceArena>,
) -> RenderFrame {
    // Hold one immutable arena borrow for the whole composition pass. Resolving
    // an actor ID is then a bounds-checked slice access, not a RefCell borrow.
    let actor_textures = actor_resources.map(actors::ActorResourceArena::texture_keys);
    let mut builder = std::mem::take(&mut scratch.frame_builder);
    builder.clear();
    let actor_count = actor_segments.iter().fold(0usize, |count, segment| {
        count.saturating_add(segment.actors.len())
    });
    let object_capacity = actor_count.saturating_mul(4).max(64);
    if builder.items.capacity() < object_capacity {
        builder.reserve(object_capacity);
    }
    debug_assert!(builder.items.capacity() >= object_capacity);
    let mut sprite_instances = std::mem::take(&mut scratch.sprite_instances);
    sprite_instances.clear();
    if sprite_instances.capacity() < object_capacity {
        sprite_instances.reserve(object_capacity - sprite_instances.len());
    }
    debug_assert!(sprite_instances.capacity() >= object_capacity);
    let mut cameras = std::mem::take(&mut scratch.cameras);
    cameras.clear();
    if cameras.capacity() < 4 {
        cameras.reserve(4 - cameras.len());
    }
    debug_assert!(cameras.capacity() >= 4);
    let mut texture_cache = std::mem::take(&mut scratch.texture_cache);
    texture_cache.begin_frame(texture_ctx);
    cameras.push(glam::camera::rh::proj::opengl::orthographic(
        m.left, m.right, m.bottom, m.top, -1.0, 1.0,
    ));
    let mut order_counter: u32 = 0;
    let mut masks = std::mem::take(&mut scratch.masks);
    masks.clear();
    if masks.capacity() < 8 {
        masks.reserve(8 - masks.len());
    }
    debug_assert!(masks.capacity() >= 8);

    let root_rect = SmRect {
        x: 0.0,
        y: 0.0,
        w: m.right - m.left,
        h: m.top - m.bottom,
    };
    let camera: u8 = 0;

    build_actor_sequence(
        actor_segments.iter().flat_map(|segment| {
            let base_z = segment.z_shift;
            segment
                .actors
                .iter()
                .map(move |actor| ActorBuild { actor, base_z })
        }),
        root_rect,
        m,
        fonts,
        scratch,
        camera,
        ComposeStyle::IDENTITY,
        &mut cameras,
        &mut masks,
        &mut order_counter,
        &mut builder,
        &mut sprite_instances,
        text_cache,
        &mut texture_cache,
        texture_ctx,
        actor_textures.as_deref(),
        total_elapsed,
    );

    let sort_fallback = !builder
        .items
        .windows(2)
        .all(|pair| pair[0].sort_key() <= pair[1].sort_key());
    if sort_fallback {
        let sort_started = scratch.collect_frame_stats.then(Instant::now);
        sort_composed_draw_items(&mut builder.items, scratch);
        if let Some(started) = sort_started {
            scratch.frame_stats.sort_us = elapsed_us_since(started);
        }
        scratch.frame_stats.sort_fallback = scratch.collect_frame_stats;
    }
    scratch.masks = masks;
    scratch.texture_cache = texture_cache;

    // Overscan adjustment (CenterImage): post-multiply a centering matrix onto
    // every camera in clip space. This single global point
    // covers the base camera, any custom pushed cameras, and screenshots, and is
    // applied live without rebuilding projections.
    if let Some(centering) = space::current_centering_matrix() {
        for cam in &mut cameras {
            *cam = centering * *cam;
        }
    }

    let mut mesh_vertices = std::mem::take(&mut scratch.mesh_vertices);
    let mut tmesh_instances = std::mem::take(&mut scratch.tmesh_instances);
    let mut tmesh_geometries = std::mem::take(&mut scratch.tmesh_geometries);
    let mut ops = std::mem::take(&mut scratch.ops);
    let mut tmesh_geom_map = std::mem::take(&mut scratch.tmesh_geom_map);
    mesh_vertices.clear();
    tmesh_instances.clear();
    tmesh_geometries.clear();
    ops.clear();
    tmesh_geom_map.clear();
    // Fallback sorting can fragment sprite runs. Count the gather plan while
    // finalizing so the hot path never rescans the completed operation list.
    let gather_plan = if sort_fallback {
        finish_frame::<true>(
            &mut builder,
            &mut mesh_vertices,
            &mut tmesh_instances,
            &mut tmesh_geometries,
            &mut ops,
            &mut tmesh_geom_map,
        )
    } else {
        finish_frame::<false>(
            &mut builder,
            &mut mesh_vertices,
            &mut tmesh_instances,
            &mut tmesh_geometries,
            &mut ops,
            &mut tmesh_geom_map,
        )
    };
    if sort_fallback {
        let gather = if sprite_gather_is_profitable(gather_plan) {
            let mut sorted_sprite_instances = std::mem::take(&mut scratch.sorted_sprite_instances);
            gather_finalized_sprites(
                &mut ops,
                &mut sprite_instances,
                &mut sorted_sprite_instances,
                gather_plan,
            );
            scratch.sorted_sprite_instances = sorted_sprite_instances;
            gather_plan
        } else {
            SpriteGatherStats {
                sprites: 0,
                runs_before: gather_plan.runs_before,
                runs_after: gather_plan.runs_before,
            }
        };
        if scratch.collect_frame_stats {
            scratch.frame_stats.sprite_gathered = gather.sprites;
            scratch.frame_stats.sprite_runs_before = gather.runs_before;
            scratch.frame_stats.sprite_runs_after = gather.runs_after;
        }
    }
    scratch.frame_builder = builder;
    scratch.tmesh_geom_map = tmesh_geom_map;

    RenderFrame {
        clear_color,
        cameras,
        sprite_instances,
        mesh_vertices,
        tmesh_instances,
        tmesh_geometries,
        ops,
    }
}

#[derive(Default)]
pub struct ComposeScratch {
    frame_builder: FrameBuilder,
    sprite_instances: Vec<renderer::SpriteInstanceRaw>,
    sorted_sprite_instances: Vec<renderer::SpriteInstanceRaw>,
    mesh_vertices: Vec<renderer::MeshVertex>,
    tmesh_instances: Vec<renderer::TexturedMeshInstanceRaw>,
    tmesh_geometries: Vec<renderer::TexturedMeshGeometry>,
    ops: Vec<renderer::DrawOp>,
    tmesh_geom_map: HashMap<TMeshGeomKey, u32, rustc_hash::FxBuildHasher>,
    cameras: Vec<Matrix4>,
    masks: Vec<WorldRect>,
    z_counts: Vec<usize>,
    z_perm: Vec<usize>,
    sparse_z_keys: Vec<usize>,
    sparse_z_bucket_by_key: Vec<u8>,
    texture_cache: TextureLookupCache,
    transient_text_mesh_builders: Vec<TextMeshBatchBuilder>,
    recycled_text_mesh_vertices: Vec<Vec<renderer::TexturedMeshVertex>>,
    retained_frames: RetainedFrameCache,
    collect_frame_stats: bool,
    frame_stats: ComposeFrameStats,
}

pub const COMPOSE_STORAGE_SLOTS: usize = 23;
pub const COMPOSE_STORAGE_NAMES: [&str; COMPOSE_STORAGE_SLOTS] = [
    "draw_items",
    "mesh_payloads",
    "tmesh_payloads",
    "sprite_inst",
    "sprite_sort",
    "mesh_vertices",
    "tmesh_inst",
    "tmesh_geoms",
    "draw_ops",
    "tmesh_geom_map",
    "cameras",
    "masks",
    "z_counts",
    "z_perm",
    "sparse_z_keys",
    "sparse_z_buckets",
    "texture_dims",
    "texture_sheets",
    "texture_handles",
    "text_builders",
    "text_recycle",
    "text_vertices",
    "retained_frames",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ComposeStorageStats {
    pub capacities: [u32; COMPOSE_STORAGE_SLOTS],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ComposeFrameStats {
    pub sort_us: u32,
    pub sort_fallback: bool,
    pub sprite_gathered: u32,
    pub sprite_runs_before: u32,
    pub sprite_runs_after: u32,
}

impl ComposeScratch {
    /// Reserves the permutation/count storage needed when a known group can
    /// expand into distinct draw items after the representative warmup frame.
    pub fn prewarm_draw_sort(&mut self, draw_count: usize) {
        if self.z_counts.capacity() < draw_count {
            self.z_counts
                .reserve(draw_count.saturating_sub(self.z_counts.len()));
        }
        if self.z_perm.capacity() < draw_count {
            self.z_perm
                .reserve(draw_count.saturating_sub(self.z_perm.len()));
        }
    }

    /// Enables or disables low-overhead timing for the next composition pass.
    #[inline(always)]
    pub fn begin_frame_stats(&mut self, enabled: bool) {
        self.collect_frame_stats = enabled;
        self.frame_stats = ComposeFrameStats::default();
    }

    #[inline(always)]
    pub const fn frame_stats(&self) -> ComposeFrameStats {
        self.frame_stats
    }

    pub fn recycle_frame(&mut self, render: &mut RenderFrame) {
        let mut geometries = std::mem::take(&mut render.tmesh_geometries);
        for geometry in geometries.drain(..) {
            let renderer::TexturedMeshVertices::Transient(mut vertices) = geometry.vertices else {
                continue;
            };
            if self.recycled_text_mesh_vertices.len() >= MAX_RECYCLED_TEXT_MESH_VERTEX_BUFFERS {
                continue;
            }
            vertices.clear();
            self.recycled_text_mesh_vertices.push(vertices);
        }
        self.tmesh_geometries = geometries;
        let mut sprite_instances = std::mem::take(&mut render.sprite_instances);
        sprite_instances.clear();
        self.sprite_instances = sprite_instances;
        let mut mesh_vertices = std::mem::take(&mut render.mesh_vertices);
        mesh_vertices.clear();
        self.mesh_vertices = mesh_vertices;
        let mut tmesh_instances = std::mem::take(&mut render.tmesh_instances);
        tmesh_instances.clear();
        self.tmesh_instances = tmesh_instances;
        let mut ops = std::mem::take(&mut render.ops);
        ops.clear();
        self.ops = ops;
        let mut cameras = std::mem::take(&mut render.cameras);
        cameras.clear();
        self.cameras = cameras;
    }

    /// Starts a new song-lifetime retained presentation working set.
    pub fn clear_retained_frames(&mut self) {
        self.retained_frames.entries.clear();
        self.retained_frames.stats = RetainedFrameCacheStats::default();
    }

    pub fn retained_frame_stats(&self) -> RetainedFrameCacheStats {
        let mut stats = self.retained_frames.stats;
        stats.entries = saturating_u32(self.retained_frames.entries.len());
        stats
    }

    pub fn reset_retained_frame_stats(&mut self) {
        self.retained_frames.stats = RetainedFrameCacheStats::default();
    }

    /// Returns retained CPU buffer capacities for opt-in frame diagnostics.
    ///
    /// The nested text-vertex total is linear in a hard-capped set of 512
    /// buffers, so callers should sample this only when diagnostics are active.
    pub fn storage_stats(&self) -> ComposeStorageStats {
        let text_vertex_capacity = self
            .recycled_text_mesh_vertices
            .iter()
            .fold(0usize, |sum, vertices| {
                sum.saturating_add(vertices.capacity())
            });
        ComposeStorageStats {
            capacities: [
                saturating_u32(self.frame_builder.items.capacity()),
                saturating_u32(self.frame_builder.meshes.capacity()),
                saturating_u32(self.frame_builder.textured_meshes.capacity()),
                saturating_u32(self.sprite_instances.capacity()),
                saturating_u32(self.sorted_sprite_instances.capacity()),
                saturating_u32(self.mesh_vertices.capacity()),
                saturating_u32(self.tmesh_instances.capacity()),
                saturating_u32(self.tmesh_geometries.capacity()),
                saturating_u32(self.ops.capacity()),
                saturating_u32(self.tmesh_geom_map.capacity()),
                saturating_u32(self.cameras.capacity()),
                saturating_u32(self.masks.capacity()),
                saturating_u32(self.z_counts.capacity()),
                saturating_u32(self.z_perm.capacity()),
                saturating_u32(self.sparse_z_keys.capacity()),
                saturating_u32(self.sparse_z_bucket_by_key.capacity()),
                saturating_u32(self.texture_cache.dims.capacity()),
                saturating_u32(self.texture_cache.sheets.capacity()),
                saturating_u32(self.texture_cache.handles.capacity()),
                saturating_u32(self.transient_text_mesh_builders.capacity()),
                saturating_u32(self.recycled_text_mesh_vertices.capacity()),
                saturating_u32(text_vertex_capacity),
                saturating_u32(self.retained_frames.entries.capacity()),
            ],
        }
    }

    #[inline(always)]
    fn transient_text_mesh_scratch(
        &mut self,
    ) -> (
        &mut Vec<TextMeshBatchBuilder>,
        &mut Vec<Vec<renderer::TexturedMeshVertex>>,
    ) {
        (
            &mut self.transient_text_mesh_builders,
            &mut self.recycled_text_mesh_vertices,
        )
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RetainedFrameCacheStats {
    pub hits: u32,
    pub misses: u32,
    pub saturated: u32,
    pub entries: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct RetainedFrameKey {
    frame_id: u64,
    parent: [u32; 4],
    metrics: [u32; 4],
    tint: [u32; 4],
    base_z: i16,
    blend: u8,
}

struct CachedRetainedFrame {
    builder: FrameBuilder,
    sprite_instances: Vec<renderer::SpriteInstanceRaw>,
}

/// Precomposed immutable actor fragments owned by the gameplay compose scratch.
///
/// Owner/thread model: the game/render frame loop is the sole owner and caller.
/// Lifetime: one song, cleared explicitly before gameplay prewarm. Capacity: 64
/// placement variants. Warmup: gameplay's transition prewarm composes every
/// retained fragment once. A hit copies typed payloads and compact items and
/// does not traverse actors or query text layout. A miss composes once and
/// inserts if capacity remains. Saturation bypasses insertion; there is no scan,
/// eviction, or destruction on live frames. Entries are destroyed on the next
/// song transition. Counters are exposed through `retained_frame_stats`. Hit
/// cost is linear only in the fragment's final draw count; misses are
/// bounded by the immutable child count supplied by the screen.
#[derive(Default)]
struct RetainedFrameCache {
    entries: HashMap<RetainedFrameKey, CachedRetainedFrame, rustc_hash::FxBuildHasher>,
    stats: RetainedFrameCacheStats,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum TMeshGeomKey {
    Cached(renderer::TMeshCacheKey),
    Shared(usize),
}

const FRAME_TMESH_GEOMS_MAX: usize = 128;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SpriteGatherStats {
    sprites: u32,
    runs_before: u32,
    runs_after: u32,
}

#[cfg(any(test, feature = "bench-support"))]
fn analyze_final_sprite_gather(ops: &[renderer::DrawOp]) -> SpriteGatherStats {
    let mut stats = SpriteGatherStats::default();
    let mut previous = None;
    for op in ops {
        let renderer::DrawOp::Sprite(run) = *op else {
            previous = None;
            continue;
        };
        stats.sprites = stats.sprites.saturating_add(run.instance_count);
        stats.runs_before = stats.runs_before.saturating_add(1);
        stats.runs_after = stats.runs_after.saturating_add(u32::from(
            previous.is_none_or(|previous| !sprite_runs_are_compatible(previous, run)),
        ));
        previous = Some(run);
    }
    stats
}

fn gather_finalized_sprites(
    ops: &mut Vec<renderer::DrawOp>,
    sprite_instances: &mut Vec<renderer::SpriteInstanceRaw>,
    sorted: &mut Vec<renderer::SpriteInstanceRaw>,
    plan: SpriteGatherStats,
) {
    sorted.clear();
    let sprite_count = plan.sprites as usize;
    if sorted.capacity() < sprite_count {
        sorted.reserve(sprite_count);
    }

    let mut write = 0usize;
    for read in 0..ops.len() {
        let mut op = ops[read];
        if let renderer::DrawOp::Sprite(ref mut run) = op {
            let start = run.instance_start as usize;
            let end = start.saturating_add(run.instance_count as usize);
            let new_start = saturating_u32(sorted.len());
            sorted.extend_from_slice(
                sprite_instances
                    .get(start..end)
                    .expect("sprite draw range references live instances"),
            );
            run.instance_start = new_start;

            if write > 0
                && let renderer::DrawOp::Sprite(previous) = &mut ops[write - 1]
                && sprite_runs_are_compatible(*previous, *run)
            {
                previous.instance_count =
                    previous.instance_count.saturating_add(run.instance_count);
                continue;
            }
        }
        ops[write] = op;
        write += 1;
    }
    ops.truncate(write);
    debug_assert_eq!(sorted.len(), sprite_count);
    std::mem::swap(sprite_instances, sorted);
}

const MIN_GATHER_SAVED_RUNS: u32 = 8;
const GATHER_SAVED_RUN_RATIO: u32 = 4;

#[inline(always)]
const fn sprite_gather_is_profitable(stats: SpriteGatherStats) -> bool {
    let saved = stats.runs_before.saturating_sub(stats.runs_after);
    saved >= MIN_GATHER_SAVED_RUNS && saved.saturating_mul(GATHER_SAVED_RUN_RATIO) >= stats.sprites
}

#[inline(always)]
fn sprite_runs_are_compatible(previous: renderer::SpriteRun, current: renderer::SpriteRun) -> bool {
    previous.blend == current.blend
        && previous.texture_handle == current.texture_handle
        && previous.camera == current.camera
}

#[allow(clippy::too_many_arguments)]
fn finish_frame<const TRACK_SPRITE_RUNS: bool>(
    builder: &mut FrameBuilder,
    mesh_vertices: &mut Vec<renderer::MeshVertex>,
    tmesh_instances: &mut Vec<renderer::TexturedMeshInstanceRaw>,
    tmesh_geometries: &mut Vec<renderer::TexturedMeshGeometry>,
    ops: &mut Vec<renderer::DrawOp>,
    geom_map: &mut HashMap<TMeshGeomKey, u32, rustc_hash::FxBuildHasher>,
) -> SpriteGatherStats {
    if ops.capacity() < builder.items.len() {
        ops.reserve(builder.items.len());
    }
    let mut sprite_stats = SpriteGatherStats::default();
    let mut previous_sprite = None;
    let mut cursor = 0usize;
    while cursor < builder.items.len() {
        let item = builder.items[cursor];
        let texture_handle = item.texture_handle;
        let blend = item.blend;
        let camera = item.camera;
        match item.kind {
            DrawKind::Sprite => {
                let instance_start = item.payload_index;
                if texture_handle == renderer::INVALID_TEXTURE_HANDLE {
                    cursor += 1;
                    continue;
                }
                let mut instance_count = 1u32;
                while let Some(next) = builder.items.get(cursor + instance_count as usize) {
                    if next.z != item.z
                        || next.texture_handle != texture_handle
                        || next.blend != blend
                        || next.camera != camera
                        || next.kind != DrawKind::Sprite
                        || next.payload_index != instance_start.saturating_add(instance_count)
                    {
                        break;
                    }
                    instance_count = instance_count.saturating_add(1);
                }
                let run = renderer::SpriteRun {
                    instance_start,
                    instance_count,
                    blend,
                    texture_handle,
                    camera,
                };
                if TRACK_SPRITE_RUNS {
                    sprite_stats.sprites = sprite_stats.sprites.saturating_add(instance_count);
                    sprite_stats.runs_before = sprite_stats.runs_before.saturating_add(1);
                    sprite_stats.runs_after =
                        sprite_stats
                            .runs_after
                            .saturating_add(u32::from(previous_sprite.is_none_or(|previous| {
                                !sprite_runs_are_compatible(previous, run)
                            })));
                    previous_sprite = Some(run);
                }
                ops.push(renderer::DrawOp::Sprite(run));
                cursor = cursor.saturating_add(instance_count as usize);
            }
            DrawKind::Mesh => {
                let MeshPayload {
                    transform,
                    tint,
                    vertices,
                } = builder.meshes[item.payload_index as usize]
                    .take()
                    .expect("draw item references live mesh payload");
                if vertices.is_empty() {
                    cursor += 1;
                    continue;
                }
                let vertex_start = saturating_u32(mesh_vertices.len());
                append_mesh_vertices(mesh_vertices, &transform, tint, vertices.as_ref());
                let mut object_count = 1usize;
                while let Some(next) = builder.items.get(cursor + object_count).copied() {
                    let compatible = next.z == item.z
                        && next.blend == blend
                        && next.camera == camera
                        && next.kind == DrawKind::Mesh
                        && builder.meshes[next.payload_index as usize]
                            .as_ref()
                            .is_some_and(|payload| !payload.vertices.is_empty());
                    if !compatible {
                        break;
                    }
                    let MeshPayload {
                        transform,
                        tint,
                        vertices,
                    } = builder.meshes[next.payload_index as usize]
                        .take()
                        .expect("draw item references live mesh payload");
                    append_mesh_vertices(mesh_vertices, &transform, tint, vertices.as_ref());
                    object_count += 1;
                }
                ops.push(renderer::DrawOp::Mesh(renderer::MeshRun {
                    vertex_start,
                    vertex_count: saturating_u32(mesh_vertices.len()).saturating_sub(vertex_start),
                    blend,
                    camera,
                }));
                if TRACK_SPRITE_RUNS {
                    previous_sprite = None;
                }
                cursor += object_count;
            }
            DrawKind::TexturedMesh => {
                let TexturedMeshPayload {
                    instance,
                    vertices,
                    geom_cache_key,
                    depth_test,
                } = builder.textured_meshes[item.payload_index as usize]
                    .take()
                    .expect("draw item references live textured-mesh payload");
                if vertices.is_empty() || texture_handle == renderer::INVALID_TEXTURE_HANDLE {
                    cursor += 1;
                    continue;
                }
                let identity = tmesh_identity(&vertices, geom_cache_key);
                let geometry = push_tmesh_geometry(
                    vertices,
                    geom_cache_key,
                    identity,
                    tmesh_geometries,
                    geom_map,
                );
                let instance_start = saturating_u32(tmesh_instances.len());
                tmesh_instances.push(instance);
                let mut object_count = 1usize;
                while identity.is_some()
                    && let Some(next) = builder.items.get(cursor + object_count).copied()
                {
                    let compatible = next.z == item.z
                        && next.texture_handle == texture_handle
                        && next.blend == blend
                        && next.camera == camera
                        && next.kind == DrawKind::TexturedMesh
                        && builder.textured_meshes[next.payload_index as usize]
                            .as_ref()
                            .is_some_and(|payload| {
                                payload.depth_test == depth_test
                                    && tmesh_identity(&payload.vertices, payload.geom_cache_key)
                                        == identity
                            });
                    if !compatible {
                        break;
                    }
                    let payload = builder.textured_meshes[next.payload_index as usize]
                        .take()
                        .expect("draw item references live textured-mesh payload");
                    tmesh_instances.push(payload.instance);
                    object_count += 1;
                }
                ops.push(renderer::DrawOp::TexturedMesh(renderer::TexturedMeshRun {
                    geometry,
                    instance_start,
                    instance_count: saturating_u32(tmesh_instances.len())
                        .saturating_sub(instance_start),
                    blend,
                    texture_handle,
                    camera,
                    depth_test,
                }));
                if TRACK_SPRITE_RUNS {
                    previous_sprite = None;
                }
                cursor += object_count;
            }
        }
    }
    builder.clear();
    sprite_stats
}

fn push_tmesh_geometry(
    vertices: renderer::TexturedMeshVertices,
    cache_key: renderer::TMeshCacheKey,
    identity: Option<TMeshGeomKey>,
    geometries: &mut Vec<renderer::TexturedMeshGeometry>,
    geom_map: &mut HashMap<TMeshGeomKey, u32, rustc_hash::FxBuildHasher>,
) -> u32 {
    if let Some(identity) = identity
        && let Some(&geometry) = geom_map.get(&identity)
    {
        return geometry;
    }
    let geometry = saturating_u32(geometries.len());
    geometries.push(renderer::TexturedMeshGeometry {
        vertices,
        cache_key,
    });
    if let Some(identity) = identity
        && geom_map.len() < FRAME_TMESH_GEOMS_MAX
    {
        geom_map.insert(identity, geometry);
    }
    geometry
}

#[inline(always)]
fn tmesh_identity(
    vertices: &renderer::TexturedMeshVertices,
    cache_key: renderer::TMeshCacheKey,
) -> Option<TMeshGeomKey> {
    if cache_key != renderer::INVALID_TMESH_CACHE_KEY {
        return Some(TMeshGeomKey::Cached(cache_key));
    }
    match vertices {
        renderer::TexturedMeshVertices::Shared(vertices) => {
            Some(TMeshGeomKey::Shared(vertices.as_ptr() as usize))
        }
        renderer::TexturedMeshVertices::Reusable(vertices) => {
            Some(TMeshGeomKey::Shared(vertices.as_ptr() as usize))
        }
        renderer::TexturedMeshVertices::Transient(_) => None,
    }
}

#[inline(always)]
fn append_mesh_vertices(
    out: &mut Vec<renderer::MeshVertex>,
    transform: &Matrix4,
    tint: [f32; 4],
    vertices: &[renderer::MeshVertex],
) {
    out.reserve(vertices.len());
    if transform.x_axis == Vector4::X
        && transform.y_axis == -Vector4::Y
        && transform.z_axis == Vector4::Z
        && transform.w_axis.z == 0.0
        && transform.w_axis.w == 1.0
        && tint == [1.0; 4]
    {
        let translate_x = transform.w_axis.x;
        let translate_y = transform.w_axis.y;
        out.extend(vertices.iter().map(|vertex| renderer::MeshVertex {
            pos: [vertex.pos[0] + translate_x, translate_y - vertex.pos[1]],
            color: vertex.color,
        }));
        return;
    }

    for vertex in vertices {
        let pos = *transform * Vector4::new(vertex.pos[0], vertex.pos[1], 0.0, 1.0);
        out.push(renderer::MeshVertex {
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

fn sort_draw_items(objects: &mut [DrawItem], scratch: &mut ComposeScratch) {
    sort_draw_items_impl(objects, scratch, true);
}

// Actor composition preserves draw order within each z layer. Discover, count,
// and validate the usual small layer set in one pass.
fn sort_composed_draw_items(objects: &mut [DrawItem], scratch: &mut ComposeScratch) {
    if objects.len() < 2 {
        return;
    }
    if collect_ordered_sparse_z_buckets(objects, scratch) {
        sort_draw_items_from_sparse_counts(objects, scratch);
    } else {
        sort_draw_items(objects, scratch);
    }
}

#[cfg(any(test, feature = "bench-support"))]
fn sort_draw_items_legacy(objects: &mut [DrawItem], scratch: &mut ComposeScratch) {
    sort_draw_items_impl(objects, scratch, false);
}

fn sort_draw_items_impl(
    objects: &mut [DrawItem],
    scratch: &mut ComposeScratch,
    optimized_sparse_sort: bool,
) {
    if objects.len() < 2 {
        return;
    }

    let mut min_z = objects[0].z;
    let mut max_z = min_z;
    let mut sorted_by_z = true;
    let mut sorted_by_key = true;
    let mut prev_key = (min_z, objects[0].order);
    for object in &objects[1..] {
        let key = (object.z, object.order);
        sorted_by_z &= prev_key.0 <= object.z;
        sorted_by_key &= prev_key <= key;
        min_z = min_z.min(object.z);
        max_z = max_z.max(object.z);
        prev_key = key;
    }
    if sorted_by_key {
        return;
    }
    if sorted_by_z {
        objects.sort_unstable_by_key(|object| (object.z, object.order));
        return;
    }

    let range = (i32::from(max_z) - i32::from(min_z) + 1) as usize;
    let dense_range_limit = objects.len().saturating_mul(8).max(256);
    if range > dense_range_limit {
        if optimized_sparse_sort {
            sort_draw_items_sparse_buckets(objects, scratch);
        } else {
            objects.sort_unstable_by_key(|object| (object.z, object.order));
        }
        return;
    }

    scratch.z_counts.clear();
    scratch.z_counts.resize(range, 0);
    scratch.z_perm.clear();
    scratch.z_perm.resize(range, 0);

    let min_z_i = i32::from(min_z);
    let mut buckets_ordered = true;
    for object in objects.iter() {
        let bucket = (i32::from(object.z) - min_z_i) as usize;
        let bucket_seen = scratch.z_counts[bucket] != 0;
        let previous_order = &mut scratch.z_perm[bucket];
        let order = object.order as usize;
        buckets_ordered &= !bucket_seen || *previous_order <= order;
        *previous_order = order;
        scratch.z_counts[bucket] += 1;
    }
    if !buckets_ordered {
        objects.sort_unstable_by_key(|object| (object.z, object.order));
        return;
    }

    let mut next = 0usize;
    for count in &mut scratch.z_counts {
        let bucket_len = *count;
        *count = next;
        next += bucket_len;
    }

    scratch.z_perm.clear();
    scratch.z_perm.resize(objects.len(), 0);
    for (old_index, object) in objects.iter().enumerate() {
        let bucket = (i32::from(object.z) - min_z_i) as usize;
        let new_index = scratch.z_counts[bucket];
        scratch.z_counts[bucket] = new_index + 1;
        scratch.z_perm[old_index] = new_index;
    }

    for start in 0..objects.len() {
        while scratch.z_perm[start] != start {
            let next = scratch.z_perm[start];
            objects.swap(start, next);
            scratch.z_perm.swap(start, next);
        }
    }

    debug_assert!(
        objects
            .windows(2)
            .all(|pair| (pair[0].z, pair[0].order) <= (pair[1].z, pair[1].order))
    );
}

/// Sparse render lists usually span a wide numeric z range but use only a
/// handful of distinct layers. Sort those compact layer keys, then perform a
/// stable bucket permutation so the comparatively large render objects move
/// only during the final ordering pass.
fn sort_draw_items_sparse_buckets(objects: &mut [DrawItem], scratch: &mut ComposeScratch) {
    if collect_sparse_z_buckets(objects, scratch) {
        sort_draw_items_in_sparse_buckets(objects, scratch);
    } else {
        objects.sort_unstable_by_key(|object| (object.z, object.order));
    }
}

const MAX_SPARSE_Z_BUCKETS: usize = 64;
const Z_KEY_COUNT: usize = u16::MAX as usize + 1;
const MISSING_Z_BUCKET: u8 = u8::MAX;

fn begin_sparse_z_collection(scratch: &mut ComposeScratch) {
    if scratch.sparse_z_bucket_by_key.len() != Z_KEY_COUNT {
        scratch
            .sparse_z_bucket_by_key
            .resize(Z_KEY_COUNT, MISSING_Z_BUCKET);
    }
    for &encoded_z in &scratch.sparse_z_keys {
        scratch.sparse_z_bucket_by_key[encoded_z] = MISSING_Z_BUCKET;
    }
    scratch.sparse_z_keys.clear();
}

fn collect_sparse_z_buckets(objects: &[DrawItem], scratch: &mut ComposeScratch) -> bool {
    begin_sparse_z_collection(scratch);
    for object in objects.iter() {
        let encoded_z = (i32::from(object.z) - i32::from(i16::MIN)) as usize;
        if scratch.sparse_z_bucket_by_key[encoded_z] == MISSING_Z_BUCKET {
            if scratch.sparse_z_keys.len() == MAX_SPARSE_Z_BUCKETS {
                return false;
            }
            scratch.sparse_z_bucket_by_key[encoded_z] = 0;
            scratch.sparse_z_keys.push(encoded_z);
        }
    }
    scratch.sparse_z_keys.sort_unstable();
    for (bucket, &encoded_z) in scratch.sparse_z_keys.iter().enumerate() {
        scratch.sparse_z_bucket_by_key[encoded_z] = bucket as u8;
    }
    true
}

fn collect_ordered_sparse_z_buckets(objects: &[DrawItem], scratch: &mut ComposeScratch) -> bool {
    begin_sparse_z_collection(scratch);
    scratch.z_counts.clear();
    scratch.z_perm.clear();
    for object in objects {
        let encoded_z = (i32::from(object.z) - i32::from(i16::MIN)) as usize;
        let bucket = scratch.sparse_z_bucket_by_key[encoded_z];
        if bucket == MISSING_Z_BUCKET {
            if scratch.sparse_z_keys.len() == MAX_SPARSE_Z_BUCKETS {
                return false;
            }
            let bucket = scratch.sparse_z_keys.len();
            scratch.sparse_z_bucket_by_key[encoded_z] = bucket as u8;
            scratch.sparse_z_keys.push(encoded_z);
            scratch.z_counts.push(1);
            scratch.z_perm.push(object.order as usize);
        } else {
            let bucket = bucket as usize;
            if scratch.z_perm[bucket] > object.order as usize {
                return false;
            }
            scratch.z_perm[bucket] = object.order as usize;
            scratch.z_counts[bucket] += 1;
        }
    }

    scratch.sparse_z_keys.sort_unstable();
    for (sorted_bucket, &encoded_z) in scratch.sparse_z_keys.iter().enumerate() {
        let insertion_bucket = scratch.sparse_z_bucket_by_key[encoded_z] as usize;
        scratch.z_perm[sorted_bucket] = scratch.z_counts[insertion_bucket];
        scratch.sparse_z_bucket_by_key[encoded_z] = sorted_bucket as u8;
    }
    std::mem::swap(&mut scratch.z_counts, &mut scratch.z_perm);
    true
}

fn sort_draw_items_in_sparse_buckets(objects: &mut [DrawItem], scratch: &mut ComposeScratch) {
    let bucket_count = scratch.sparse_z_keys.len();
    scratch.z_counts.clear();
    scratch.z_counts.resize(bucket_count, 0);
    scratch.z_perm.clear();
    scratch.z_perm.resize(bucket_count, 0);
    let mut buckets_ordered = true;
    for object in objects.iter() {
        let encoded_z = (i32::from(object.z) - i32::from(i16::MIN)) as usize;
        let bucket = scratch.sparse_z_bucket_by_key[encoded_z] as usize;
        let bucket_seen = scratch.z_counts[bucket] != 0;
        let previous_order = &mut scratch.z_perm[bucket];
        let order = object.order as usize;
        buckets_ordered &= !bucket_seen || *previous_order <= order;
        *previous_order = order;
        scratch.z_counts[bucket] += 1;
    }
    if !buckets_ordered {
        objects.sort_unstable_by_key(|object| (object.z, object.order));
        return;
    }

    sort_draw_items_from_sparse_counts(objects, scratch);
}

fn sort_draw_items_from_sparse_counts(objects: &mut [DrawItem], scratch: &mut ComposeScratch) {
    let mut next = 0usize;
    for count in &mut scratch.z_counts {
        let bucket_len = *count;
        *count = next;
        next += bucket_len;
    }

    scratch.z_perm.clear();
    scratch.z_perm.resize(objects.len(), 0);
    for (old_index, object) in objects.iter().enumerate() {
        let encoded_z = (i32::from(object.z) - i32::from(i16::MIN)) as usize;
        let bucket = scratch.sparse_z_bucket_by_key[encoded_z] as usize;
        let new_index = scratch.z_counts[bucket];
        scratch.z_counts[bucket] = new_index + 1;
        scratch.z_perm[old_index] = new_index;
    }

    let permutation = &mut scratch.z_perm;
    for start in 0..objects.len() {
        while permutation[start] != start {
            let next = permutation[start];
            objects.swap(start, next);
            permutation.swap(start, next);
        }
    }

    debug_assert!(
        objects
            .windows(2)
            .all(|pair| (pair[0].z, pair[0].order) <= (pair[1].z, pair[1].order))
    );
}

/// Sparse-z gameplay-shaped compact draw-item sorting benchmark support.
#[cfg(feature = "bench-support")]
pub struct RenderSortBenchmark {
    objects: Vec<DrawItem>,
    scratch: ComposeScratch,
}

#[cfg(feature = "bench-support")]
impl RenderSortBenchmark {
    pub fn new(object_count: usize) -> Self {
        let objects = (0..object_count)
            .map(|index| DrawItem::synthetic(0, index as u32, index as u32))
            .collect();
        Self {
            objects,
            scratch: ComposeScratch::default(),
        }
    }

    pub fn sort_frame(&mut self, frame: usize) -> u64 {
        self.sort_frame_with(frame, false)
    }

    pub fn sort_legacy_frame(&mut self, frame: usize) -> u64 {
        self.sort_frame_with(frame, true)
    }

    pub fn sort_composed_frame(&mut self, frame: usize) -> u64 {
        self.prepare_sparse_frame(frame);
        sort_composed_draw_items(&mut self.objects, &mut self.scratch);
        self.frame_checksum()
    }

    pub fn sort_dense_frame(&mut self, frame: usize) -> u64 {
        self.prepare_dense_frame(frame);
        sort_draw_items(&mut self.objects, &mut self.scratch);
        self.frame_checksum()
    }

    pub fn sort_composed_dense_frame(&mut self, frame: usize) -> u64 {
        self.prepare_dense_frame(frame);
        sort_composed_draw_items(&mut self.objects, &mut self.scratch);
        self.frame_checksum()
    }

    fn sort_frame_with(&mut self, frame: usize, legacy: bool) -> u64 {
        self.prepare_sparse_frame(frame);
        if legacy {
            sort_draw_items_legacy(&mut self.objects, &mut self.scratch);
        } else {
            sort_draw_items(&mut self.objects, &mut self.scratch);
        }
        self.frame_checksum()
    }

    fn prepare_sparse_frame(&mut self, frame: usize) {
        const Z_VALUES: [i16; 12] = [
            -30_000, -99, 90, 2_101, 0, 1_500, -12_000, 50, 32_000, 91, -1, 8_000,
        ];
        for (position, object) in self.objects.iter_mut().enumerate() {
            let mixed = position
                .wrapping_mul(17)
                .wrapping_add(frame.wrapping_mul(5))
                % Z_VALUES.len();
            object.z = Z_VALUES[mixed];
            object.order = position as u32;
        }
    }

    fn prepare_dense_frame(&mut self, frame: usize) {
        for (position, object) in self.objects.iter_mut().enumerate() {
            let mixed = position
                .wrapping_mul(17)
                .wrapping_add(frame.wrapping_mul(5))
                % 32;
            object.z = mixed as i16 - 16;
            object.order = position as u32;
        }
    }

    fn frame_checksum(&self) -> u64 {
        let len = self.objects.len();
        debug_assert!(
            self.objects
                .windows(2)
                .all(|pair| (pair[0].z, pair[0].order) <= (pair[1].z, pair[1].order))
        );
        self.objects.iter().fold(len as u64, |checksum, object| {
            checksum
                .wrapping_mul(0x9e37_79b1_85eb_ca87)
                .wrapping_add(u64::from(object.z as u16) << 32)
                .wrapping_add(u64::from(object.order))
                .wrapping_add(u64::from(object.payload_index))
        })
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TextPageId(NonZeroU32);

impl TextPageId {
    #[inline(always)]
    fn from_index(index: usize) -> Self {
        let one_based =
            u32::try_from(index.saturating_add(1)).expect("text layout exceeded the page ID range");
        Self(NonZeroU32::new(one_based).expect("text page IDs are one-based"))
    }

    #[inline(always)]
    const fn index(self) -> usize {
        self.0.get() as usize - 1
    }
}

/// One layout-owned texture page and its single-threaded registry lookup cache.
///
/// The entry lives and is destroyed with the text layout. Layout construction
/// bounds capacity to its unique pages, and normal prewarm populates handles.
/// A miss or generation change performs one registry lookup and overwrites the
/// old value in constant time; there is no eviction or frame-dependent scan.
/// Registry lookup instrumentation remains owned by `TextureContext`. Worst-case
/// compose work is one lookup per visible page after a registry generation bump.
struct CachedTextPage {
    key: Arc<str>,
    handle: Cell<renderer::TextureHandle>,
    generation_stamp: Cell<u64>,
}

impl CachedTextPage {
    fn new(key: Arc<str>) -> Self {
        Self {
            key,
            handle: Cell::new(renderer::INVALID_TEXTURE_HANDLE),
            generation_stamp: Cell::new(0),
        }
    }

    #[inline(always)]
    fn texture_handle<T: TextureContext + ?Sized>(
        &self,
        generation: u64,
        texture_ctx: &T,
    ) -> renderer::TextureHandle {
        // Zero is the uncached stamp. At the single wrapping generation value,
        // deliberately query every time instead of risking a false initial hit.
        let stamp = generation.wrapping_add(1);
        if stamp != 0 && self.generation_stamp.get() == stamp {
            return self.handle.get();
        }
        let handle = texture_ctx.texture_handle(self.key.as_ref());
        self.handle.set(handle);
        self.generation_stamp.set(stamp);
        handle
    }
}

type TextPageBuilder = Vec<CachedTextPage>;

#[inline(always)]
fn intern_text_page(pages: &mut TextPageBuilder, key: &Arc<str>) -> TextPageId {
    if let Some(index) = pages
        .iter()
        .position(|page| Arc::ptr_eq(&page.key, key) || page.key.as_ref() == key.as_ref())
    {
        return TextPageId::from_index(index);
    }
    let id = TextPageId::from_index(pages.len());
    pages.push(CachedTextPage::new(Arc::clone(key)));
    id
}

#[derive(Clone, Copy)]
struct CachedGlyph {
    texture_page: TextPageId,
    stroke_page: Option<TextPageId>,
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    size: [f32; 2],
    offset: [f32; 2],
    advance_i32: i32,
    char_index: usize,
    draw_quad: bool,
}

#[derive(Clone)]
struct CachedLine {
    width_i32: i32,
    glyph_start: usize,
    glyph_len: usize,
}

#[derive(Clone)]
struct CachedTextMeshBatch {
    texture_page: TextPageId,
    geom_cache_key: renderer::TMeshCacheKey,
    vertices: Arc<[renderer::TexturedMeshVertex]>,
}

#[derive(Default)]
struct CachedTextMeshVariants {
    by_align: [OnceCell<Box<[CachedTextMeshBatch]>>; 3],
}

impl CachedTextMeshVariants {
    #[inline(always)]
    const fn index(align: actors::TextAlign) -> usize {
        match align {
            actors::TextAlign::Left => 0,
            actors::TextAlign::Center => 1,
            actors::TextAlign::Right => 2,
        }
    }

    #[inline(always)]
    fn get_or_init<F>(&self, align: actors::TextAlign, init: F) -> &[CachedTextMeshBatch]
    where
        F: FnOnce(actors::TextAlign) -> Vec<CachedTextMeshBatch>,
    {
        self.by_align[Self::index(align)]
            .get_or_init(|| init(align).into_boxed_slice())
            .as_ref()
    }

    #[cfg(test)]
    #[inline(always)]
    fn is_built(&self, align: actors::TextAlign) -> bool {
        self.by_align[Self::index(align)].get().is_some()
    }
}

struct CachedTextLayout {
    layout_seed: u64,
    font_height: i32,
    line_spacing: i32,
    max_logical_width_i: i32,
    glyph_count: usize,
    texture_pages: Vec<CachedTextPage>,
    lines: Vec<CachedLine>,
    glyphs: Vec<CachedGlyph>,
    fill_batches: CachedTextMeshVariants,
    stroke_batches: CachedTextMeshVariants,
}

/// Sizes of the cached text records whose representation is exercised by the
/// text-page benchmark. Kept behind bench support so private layout details do
/// not become part of the presentation API.
#[cfg(feature = "bench-support")]
pub fn benchmark_text_layout_type_sizes() -> (usize, usize, usize) {
    (
        std::mem::size_of::<CachedGlyph>(),
        std::mem::size_of::<CachedTextMeshBatch>(),
        std::mem::size_of::<CachedTextLayout>(),
    )
}

impl CachedTextLayout {
    fn empty() -> Self {
        Self {
            layout_seed: 0,
            font_height: 0,
            line_spacing: 0,
            max_logical_width_i: 0,
            glyph_count: 0,
            texture_pages: Vec::new(),
            lines: Vec::new(),
            glyphs: Vec::new(),
            fill_batches: CachedTextMeshVariants::default(),
            stroke_batches: CachedTextMeshVariants::default(),
        }
    }

    fn frame_inline_scratch() -> Self {
        Self {
            layout_seed: 0,
            font_height: 0,
            line_spacing: 0,
            max_logical_width_i: 0,
            glyph_count: 0,
            texture_pages: Vec::with_capacity(2),
            lines: Vec::with_capacity(1),
            glyphs: Vec::with_capacity(actors::InlineText::CAPACITY),
            fill_batches: CachedTextMeshVariants::default(),
            stroke_batches: CachedTextMeshVariants::default(),
        }
    }

    fn clear_frame_inline_scratch(&mut self) {
        self.layout_seed = 0;
        self.font_height = 0;
        self.line_spacing = 0;
        self.max_logical_width_i = 0;
        self.glyph_count = 0;
        self.texture_pages.clear();
        self.lines.clear();
        self.glyphs.clear();
        self.fill_batches = CachedTextMeshVariants::default();
        self.stroke_batches = CachedTextMeshVariants::default();
    }

    #[inline(always)]
    fn texture_page(&self, id: TextPageId) -> &CachedTextPage {
        self.texture_pages
            .get(id.index())
            .expect("cached text page ID belongs to its layout")
    }

    #[inline(always)]
    fn fill_batches(&self, align: actors::TextAlign) -> &[CachedTextMeshBatch] {
        self.fill_batches.get_or_init(align, |align| {
            build_text_mesh_batches_for_align(
                self.layout_seed,
                self.font_height,
                self.line_spacing,
                self.max_logical_width_i,
                &self.lines,
                &self.glyphs,
                align,
                false,
            )
        })
    }

    #[inline(always)]
    fn stroke_batches(&self, align: actors::TextAlign) -> &[CachedTextMeshBatch] {
        self.stroke_batches.get_or_init(align, |align| {
            build_text_mesh_batches_for_align(
                self.layout_seed,
                self.font_height,
                self.line_spacing,
                self.max_logical_width_i,
                &self.lines,
                &self.glyphs,
                align,
                true,
            )
        })
    }
}

type WordGlyphs = SmallVec<[CachedGlyph; 16]>;
type AttrIndices = SmallVec<[usize; 8]>;
type ClipPolygon = SmallVec<[ClipVertex; 8]>;
type ClippedMesh = SmallVec<[renderer::TexturedMeshVertex; 18]>;

struct TextLayoutPlacement {
    sx: f32,
    sy: f32,
    block_center_x: f32,
    block_center_y: f32,
}

struct OwnedLayoutEntry {
    layout_index: usize,
}

struct SharedLayoutEntry {
    layout: TextLayoutKey,
    _owner: Arc<str>,
    layout_index: usize,
}

type TextLayoutHasher = rustc_hash::FxBuildHasher;
type OwnedLayoutMap = HashMap<Box<str>, OwnedLayoutEntry, TextLayoutHasher>;
type SharedAliasEntries = SmallVec<[SharedLayoutEntry; 1]>;
type SharedAliasMap = HashMap<usize, SharedAliasEntries, TextLayoutHasher>;

#[derive(Clone, Copy)]
struct TextureCacheEntry<T> {
    fingerprint: u64,
    validated_frame: u64,
    value: T,
}

type TextureLookupMap<T> = HashMap<usize, TextureCacheEntry<T>, TextLayoutHasher>;
type TextureMetaMap = TextureLookupMap<TextureMeta>;
type TextureSheetMap = TextureLookupMap<(u32, u32)>;
type TextureHandleLookupMap = TextureLookupMap<renderer::TextureHandle>;

#[derive(Default)]
struct TextureLookupCache {
    generation: u64,
    frame: u64,
    dims: TextureMetaMap,
    sheets: TextureSheetMap,
    handles: TextureHandleLookupMap,
}

impl TextureLookupCache {
    fn begin_frame<T: TextureContext + ?Sized>(&mut self, texture_ctx: &T) {
        self.frame = self.frame.wrapping_add(1);
        if self.frame == 0 {
            self.clear_entries();
            self.frame = 1;
        }
        let generation = texture_ctx.texture_registry_generation();
        if self.generation == generation {
            return;
        }
        self.generation = generation;
        self.clear_entries();
    }

    fn clear_entries(&mut self) {
        self.dims.clear();
        self.sheets.clear();
        self.handles.clear();
    }

    #[inline(always)]
    fn ptr_cache_key(key_ptr: *const str) -> usize {
        key_ptr as *const () as usize
    }

    #[inline(always)]
    fn key_fingerprint(key: &str) -> u64 {
        let mut hasher = rustc_hash::FxHasher::default();
        key.hash(&mut hasher);
        hasher.finish()
    }

    #[inline(always)]
    fn validated_value<T: Copy>(
        frame: u64,
        entries: &mut TextureLookupMap<T>,
        key_ptr: usize,
        key: &str,
    ) -> Result<T, u64> {
        // The actor tree and cached font storage stay alive for the whole compose
        // call, so a key address validated in this frame cannot change contents.
        // Revalidate on the next frame to protect against allocator address reuse.
        if let Some(entry) = entries.get_mut(&key_ptr) {
            if entry.validated_frame == frame {
                return Ok(entry.value);
            }
            let fingerprint = Self::key_fingerprint(key);
            if entry.fingerprint == fingerprint {
                entry.validated_frame = frame;
                return Ok(entry.value);
            }
            return Err(fingerprint);
        }
        Err(Self::key_fingerprint(key))
    }

    #[inline(always)]
    fn texture_dims<T: TextureContext + ?Sized>(
        &mut self,
        texture_ctx: &T,
        key_ptr: *const str,
        key: &str,
    ) -> Option<TextureMeta> {
        let key_ptr = Self::ptr_cache_key(key_ptr);
        let fingerprint = match Self::validated_value(self.frame, &mut self.dims, key_ptr, key) {
            Ok(meta) => return Some(meta),
            Err(fingerprint) => fingerprint,
        };
        let meta = texture_ctx.texture_dims(key)?;
        self.dims.insert(
            key_ptr,
            TextureCacheEntry {
                fingerprint,
                validated_frame: self.frame,
                value: meta,
            },
        );
        Some(meta)
    }

    #[inline(always)]
    fn sprite_sheet_dims<T: TextureContext + ?Sized>(
        &mut self,
        texture_ctx: &T,
        key_ptr: *const str,
        key: &str,
    ) -> (u32, u32) {
        let key_ptr = Self::ptr_cache_key(key_ptr);
        let fingerprint = match Self::validated_value(self.frame, &mut self.sheets, key_ptr, key) {
            Ok(dims) => return dims,
            Err(fingerprint) => fingerprint,
        };
        let dims = texture_ctx.sprite_sheet_dims(key);
        self.sheets.insert(
            key_ptr,
            TextureCacheEntry {
                fingerprint,
                validated_frame: self.frame,
                value: dims,
            },
        );
        dims
    }

    #[inline(always)]
    fn texture_handle<T: TextureContext + ?Sized>(
        &mut self,
        texture_ctx: &T,
        key_ptr: *const str,
        key: &str,
    ) -> renderer::TextureHandle {
        let key_ptr = Self::ptr_cache_key(key_ptr);
        let fingerprint = match Self::validated_value(self.frame, &mut self.handles, key_ptr, key) {
            Ok(handle) => return handle,
            Err(fingerprint) => fingerprint,
        };
        let handle = texture_ctx.texture_handle(key);
        self.handles.insert(
            key_ptr,
            TextureCacheEntry {
                fingerprint,
                validated_frame: self.frame,
                value: handle,
            },
        );
        handle
    }
}

#[inline(always)]
fn str_ptr(key: &str) -> *const str {
    key as *const str
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct TextLayoutKey {
    font_key: u64,
    line_spacing: i32,
    wrap_width_pixels: i32,
}

struct FrameInlineLayoutSlot {
    layout: CachedTextLayout,
    key: Option<(TextLayoutKey, actors::InlineText)>,
}

impl FrameInlineLayoutSlot {
    fn new() -> Self {
        Self {
            layout: CachedTextLayout::frame_inline_scratch(),
            key: None,
        }
    }

    fn clear(&mut self) {
        self.layout.clear_frame_inline_scratch();
        self.key = None;
    }
}

struct PrewarmedU16Domain {
    key: Option<TextLayoutKey>,
    layouts: Vec<Box<CachedTextLayout>>,
}

impl PrewarmedU16Domain {
    fn new() -> Self {
        Self {
            key: None,
            layouts: Vec::new(),
        }
    }

    fn clear(&mut self) {
        self.key = None;
        self.layouts.clear();
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TextLayoutFrameStats {
    pub owned_hits: u32,
    pub shared_hits: u32,
    pub misses: u32,
    pub built_lines: u32,
    pub built_glyphs: u32,
    pub owned_entries: u32,
    pub shared_aliases: u32,
}

/// Persistent bitmap-text layout storage owned by one presentation caller.
///
/// The cache is single-threaded (`&mut self`) and lives for whatever scope its
/// owner chooses; DeadSync uses separate UI and song-lifetime gameplay instances.
/// Callers prewarm before a live interval, then either freeze exactly or allow a
/// bounded late reserve. A retained miss builds once, while overflow builds an
/// uncached result without eviction, scans, or destructor cascades. `clear()` or
/// dropping the owner destroys entries outside the live interval. Optional
/// per-frame hit/miss and built-glyph counters are exposed through
/// `begin_frame_stats`/`frame_stats`. Worst-case work is one full layout build per
/// uncached text actor in a frame; cache maintenance itself is constant-time.
/// Frame-inline slots are single-threaded, screen/song-lifetime storage whose
/// capacity is fixed by boundary prewarm. Each slot retains one short layout;
/// misses rebuild only that slot, unprepared slot IDs saturate to one fallback,
/// and clear/drop destruction happens at a transition. Existing hit/miss and
/// built-glyph counters include slot activity. Worst-case work is one bounded
/// 14-glyph rebuild per changed frame-inline actor. Prewarmed-u16 domains are
/// bounded dense lookup tables populated at a transition; live hits are one
/// bounds check and never hash, allocate, evict, or rebuild.
pub struct TextLayoutCache {
    // Keep arena growth moving pointers instead of large layouts with initialized OnceCells.
    #[allow(clippy::vec_box)]
    layouts: Vec<Box<CachedTextLayout>>,
    owned_entries: HashMap<TextLayoutKey, OwnedLayoutMap, TextLayoutHasher>,
    shared_aliases: SharedAliasMap,
    entry_count: usize,
    alias_count: usize,
    max_entries: usize,
    max_aliases: usize,
    frame_stats: Option<TextLayoutFrameStats>,
    uncached_layout: Option<Box<CachedTextLayout>>,
    frame_inline_slots: Vec<FrameInlineLayoutSlot>,
    frame_inline_fallback: FrameInlineLayoutSlot,
    prewarmed_u16_domains: Vec<PrewarmedU16Domain>,
}

impl Default for TextLayoutCache {
    fn default() -> Self {
        Self::new(4096)
    }
}

impl TextLayoutCache {
    pub fn new(max_entries: usize) -> Self {
        let max_entries = max_entries.max(1);
        Self {
            layouts: Vec::new(),
            owned_entries: HashMap::default(),
            shared_aliases: HashMap::default(),
            entry_count: 0,
            alias_count: 0,
            max_entries,
            max_aliases: max_entries,
            frame_stats: None,
            uncached_layout: None,
            frame_inline_slots: vec![FrameInlineLayoutSlot::new()],
            frame_inline_fallback: FrameInlineLayoutSlot::new(),
            prewarmed_u16_domains: Vec::new(),
        }
    }

    pub fn configure(&mut self, max_entries: usize) {
        self.max_entries = max_entries.max(1);
        self.max_aliases = self.max_entries;
    }

    /// Freeze the cache at its current size so future misses saturate instead of
    /// growing during a live frame.
    pub fn lock_growth(&mut self) {
        self.max_entries = self.entry_count.max(1);
        self.max_aliases = self.alias_count;
    }

    /// Freeze the prewarmed working set while retaining a bounded number of
    /// later owned texts and shared-text aliases. Layout and top-level alias
    /// storage are reserved now so the ordinary one-layout-per-pointer miss does
    /// not grow either collection. Once either late allowance is full, that
    /// class saturates without scanning or pruning.
    pub fn lock_growth_with_reserve(&mut self, additional_entries: usize) {
        self.max_entries = self
            .entry_count
            .saturating_add(additional_entries)
            .min(self.max_entries)
            .max(1);
        self.max_aliases = self
            .alias_count
            .saturating_add(additional_entries)
            .min(self.max_aliases);
        let additional_layouts = self
            .max_entries
            .saturating_sub(self.entry_count)
            .saturating_add(self.max_aliases.saturating_sub(self.alias_count));
        self.layouts.reserve(additional_layouts);
        self.shared_aliases
            .reserve(self.max_aliases.saturating_sub(self.alias_count));
    }

    pub fn clear(&mut self) {
        self.layouts.clear();
        self.owned_entries.clear();
        self.shared_aliases.clear();
        self.entry_count = 0;
        self.alias_count = 0;
        self.frame_stats = None;
        self.uncached_layout = None;
        for slot in &mut self.frame_inline_slots {
            slot.clear();
        }
        self.frame_inline_fallback.clear();
        for domain in &mut self.prewarmed_u16_domains {
            domain.clear();
        }
    }

    /// Reset optional per-frame instrumentation. Disable it on ordinary frames
    /// so cache hits do not pay for diagnostic counter updates.
    #[inline(always)]
    pub fn begin_frame_stats(&mut self, enabled: bool) {
        self.frame_stats = enabled.then(TextLayoutFrameStats::default);
    }

    #[inline(always)]
    pub fn frame_stats(&self) -> TextLayoutFrameStats {
        let Some(frame_stats) = self.frame_stats else {
            return TextLayoutFrameStats::default();
        };
        TextLayoutFrameStats {
            owned_entries: saturating_u32(
                self.entry_count.saturating_add(
                    self.prewarmed_u16_domains
                        .iter()
                        .map(|domain| domain.layouts.len())
                        .sum(),
                ),
            ),
            shared_aliases: saturating_u32(self.alias_count),
            ..frame_stats
        }
    }

    #[cfg(test)]
    fn owned_layout(&self, key: TextLayoutKey, text: &str) -> Option<&CachedTextLayout> {
        let layout_index = self.owned_entries.get(&key)?.get(text)?.layout_index;
        Some(self.layouts.get(layout_index)?.as_ref())
    }

    #[inline(always)]
    fn uncached_layout_ref(&self) -> &CachedTextLayout {
        self.uncached_layout
            .as_deref()
            .expect("uncached text layout inserted")
    }

    #[inline(always)]
    fn record_layout_build(&mut self, layout: &CachedTextLayout) {
        let Some(frame_stats) = self.frame_stats.as_mut() else {
            return;
        };
        frame_stats.misses = frame_stats.misses.saturating_add(1);
        frame_stats.built_lines = frame_stats
            .built_lines
            .saturating_add(saturating_u32(layout.lines.len()));
        frame_stats.built_glyphs = frame_stats
            .built_glyphs
            .saturating_add(saturating_u32(layout.glyph_count));
    }

    fn insert_owned_layout(
        &mut self,
        key: TextLayoutKey,
        text: &str,
        layout: Box<CachedTextLayout>,
    ) -> Option<usize> {
        if self.entry_count >= self.max_entries {
            self.uncached_layout = Some(layout);
            return None;
        }
        let layout_index = self.layouts.len();
        let replaced = self
            .owned_entries
            .entry(key)
            .or_default()
            .insert(text.into(), OwnedLayoutEntry { layout_index });
        debug_assert!(replaced.is_none());
        self.entry_count += usize::from(replaced.is_none());
        self.layouts.push(layout);
        Some(layout_index)
    }

    fn insert_shared_layout(
        &mut self,
        key: TextLayoutKey,
        text_key: usize,
        text: Arc<str>,
        layout: Box<CachedTextLayout>,
    ) -> Option<usize> {
        if self.alias_count >= self.max_aliases {
            self.uncached_layout = Some(layout);
            return None;
        }
        let layout_index = self.layouts.len();
        let replaced = self.insert_shared_alias(key, text_key, text, layout_index);
        debug_assert!(replaced.is_none());
        self.alias_count += usize::from(replaced.is_none());
        self.layouts.push(layout);
        Some(layout_index)
    }

    fn insert_shared_alias(
        &mut self,
        key: TextLayoutKey,
        text_key: usize,
        text: Arc<str>,
        layout_index: usize,
    ) -> Option<SharedLayoutEntry> {
        let entries = self.shared_aliases.entry(text_key).or_default();
        let new_entry = SharedLayoutEntry {
            layout: key,
            _owner: text,
            layout_index,
        };
        if let Some(entry) = entries.iter_mut().find(|entry| entry.layout == key) {
            return Some(std::mem::replace(entry, new_entry));
        }
        entries.push(new_entry);
        None
    }

    pub fn prewarm_text(
        &mut self,
        fonts: &font::FontMap,
        font_name: &'static str,
        text: &str,
        wrap_width_pixels: Option<i32>,
    ) {
        let Some(font) = fonts.get(font_name) else {
            return;
        };
        let key = TextLayoutKey {
            font_key: font_chain_key(font, fonts),
            line_spacing: font.line_spacing,
            wrap_width_pixels: wrap_width_pixels.unwrap_or(-1),
        };
        let _ = self.get_or_build_owned(key, font, fonts, text);
    }

    /// Prepares a dense decimal layout domain and its fill geometry. Live actors
    /// carrying the same domain ID index this table directly by `u16` value.
    pub fn prewarm_u16_domain(
        &mut self,
        fonts: &font::FontMap,
        font_name: &'static str,
        domain: u8,
        max_value: u16,
        wrap_width_pixels: Option<i32>,
        align: actors::TextAlign,
    ) {
        let Some(font) = fonts.get(font_name) else {
            return;
        };
        let key = TextLayoutKey {
            font_key: font_chain_key(font, fonts),
            line_spacing: font.line_spacing,
            wrap_width_pixels: wrap_width_pixels.unwrap_or(-1),
        };
        let domain_index = usize::from(domain);
        if self.prewarmed_u16_domains.len() <= domain_index {
            self.prewarmed_u16_domains
                .resize_with(domain_index + 1, PrewarmedU16Domain::new);
        }
        let mut layouts = std::mem::take(&mut self.prewarmed_u16_domains[domain_index].layouts);
        layouts.clear();
        let layout_count = usize::from(max_value) + 1;
        if layouts.capacity() < layout_count {
            layouts.reserve(layout_count.saturating_sub(layouts.len()));
        }
        let mut built_lines = 0usize;
        let mut built_glyphs = 0usize;
        for value in 0..=max_value {
            let text = actors::InlineU16Text::new(value);
            let layout = Box::new(build_cached_text_layout(
                font,
                fonts,
                text.as_str(),
                key.line_spacing,
                key.wrap_width_pixels,
                text_layout_mesh_seed(key, text.as_str()),
            ));
            let _ = layout.fill_batches(align);
            built_lines = built_lines.saturating_add(layout.lines.len());
            built_glyphs = built_glyphs.saturating_add(layout.glyph_count);
            layouts.push(layout);
        }
        let slot = &mut self.prewarmed_u16_domains[domain_index];
        slot.key = Some(key);
        slot.layouts = layouts;
        if let Some(stats) = self.frame_stats.as_mut() {
            stats.misses = stats.misses.saturating_add(saturating_u32(layout_count));
            stats.built_lines = stats
                .built_lines
                .saturating_add(saturating_u32(built_lines));
            stats.built_glyphs = stats
                .built_glyphs
                .saturating_add(saturating_u32(built_glyphs));
        }
    }

    fn get_or_build(
        &mut self,
        font: &font::Font,
        fonts: &font::FontMap,
        content: &actors::TextContent,
        wrap_width_pixels: Option<i32>,
        line_spacing: Option<i32>,
    ) -> &CachedTextLayout {
        let key = TextLayoutKey {
            font_key: font_chain_key(font, fonts),
            line_spacing: line_spacing.unwrap_or(font.line_spacing),
            wrap_width_pixels: wrap_width_pixels.unwrap_or(-1),
        };
        match content {
            actors::TextContent::Static(text) => self.get_or_build_owned(key, font, fonts, text),
            actors::TextContent::Owned(text) => self.get_or_build_owned(key, font, fonts, text),
            actors::TextContent::Shared(text) => self.get_or_build_shared(key, font, fonts, text),
            actors::TextContent::Inline(text) => {
                self.get_or_build_owned(key, font, fonts, text.as_str())
            }
            actors::TextContent::FrameInline { text, slot } => {
                self.get_or_build_frame_inline_slot(key, font, fonts, *text, *slot)
            }
            actors::TextContent::InlineU16(text) => {
                self.get_or_build_owned(key, font, fonts, text.as_str())
            }
            actors::TextContent::PrewarmedU16 { text, domain } => {
                self.get_or_build_prewarmed_u16(key, font, fonts, *text, *domain)
            }
            actors::TextContent::InlineU32(text) => {
                self.get_or_build_owned(key, font, fonts, text.as_str())
            }
        }
    }

    fn get_or_build_frame_inline_slot(
        &mut self,
        key: TextLayoutKey,
        font: &font::Font,
        fonts: &font::FontMap,
        text: actors::InlineText,
        slot: u8,
    ) -> &CachedTextLayout {
        let Self {
            frame_inline_slots,
            frame_inline_fallback,
            frame_stats,
            ..
        } = self;
        let slot = frame_inline_slots
            .get_mut(usize::from(slot))
            .unwrap_or(frame_inline_fallback);
        if slot.key == Some((key, text)) {
            if let Some(frame_stats) = frame_stats.as_mut() {
                frame_stats.owned_hits = frame_stats.owned_hits.saturating_add(1);
            }
            return &slot.layout;
        }
        rebuild_cached_text_layout(
            &mut slot.layout,
            font,
            fonts,
            text.as_str(),
            key.line_spacing,
            key.wrap_width_pixels,
            text_layout_mesh_seed(key, text.as_str()),
        );
        slot.key = Some((key, text));
        if let Some(frame_stats) = frame_stats.as_mut() {
            frame_stats.misses = frame_stats.misses.saturating_add(1);
            frame_stats.built_lines = frame_stats
                .built_lines
                .saturating_add(saturating_u32(slot.layout.lines.len()));
            frame_stats.built_glyphs = frame_stats
                .built_glyphs
                .saturating_add(saturating_u32(slot.layout.glyph_count));
        }
        &slot.layout
    }

    fn get_or_build_prewarmed_u16(
        &mut self,
        key: TextLayoutKey,
        font: &font::Font,
        fonts: &font::FontMap,
        text: actors::InlineU16Text,
        domain: u8,
    ) -> &CachedTextLayout {
        let domain_index = usize::from(domain);
        let value_index = usize::from(text.value());
        let is_hit = self
            .prewarmed_u16_domains
            .get(usize::from(domain))
            .filter(|slot| slot.key == Some(key))
            .is_some_and(|slot| value_index < slot.layouts.len());
        if is_hit {
            if let Some(stats) = self.frame_stats.as_mut() {
                stats.owned_hits = stats.owned_hits.saturating_add(1);
            }
            return self.prewarmed_u16_domains[domain_index].layouts[value_index].as_ref();
        }
        self.get_or_build_owned(key, font, fonts, text.as_str())
    }

    fn get_or_build_owned(
        &mut self,
        key: TextLayoutKey,
        font: &font::Font,
        fonts: &font::FontMap,
        text: &str,
    ) -> &CachedTextLayout {
        if let Some(entry) = self
            .owned_entries
            .get_mut(&key)
            .and_then(|font_entries| font_entries.get_mut(text))
        {
            let layout_index = entry.layout_index;
            if let Some(frame_stats) = self.frame_stats.as_mut() {
                frame_stats.owned_hits = frame_stats.owned_hits.saturating_add(1);
            }
            return self.layouts[layout_index].as_ref();
        }
        let layout = Box::new(build_cached_text_layout(
            font,
            fonts,
            text,
            key.line_spacing,
            key.wrap_width_pixels,
            text_layout_mesh_seed(key, text),
        ));
        self.record_layout_build(layout.as_ref());
        if let Some(layout_index) = self.insert_owned_layout(key, text, layout) {
            self.layouts[layout_index].as_ref()
        } else {
            self.uncached_layout_ref()
        }
    }

    fn get_or_build_shared(
        &mut self,
        key: TextLayoutKey,
        font: &font::Font,
        fonts: &font::FontMap,
        text: &Arc<str>,
    ) -> &CachedTextLayout {
        let text_key = Arc::as_ptr(text) as *const () as usize;
        let text_ref = text.as_ref();
        if let Some(entry) = self
            .shared_aliases
            .get_mut(&text_key)
            .and_then(|entries| entries.iter_mut().find(|entry| entry.layout == key))
        {
            let layout_index = entry.layout_index;
            if let Some(frame_stats) = self.frame_stats.as_mut() {
                frame_stats.shared_hits = frame_stats.shared_hits.saturating_add(1);
            }
            return self.layouts[layout_index].as_ref();
        }

        if let Some(entry) = self
            .owned_entries
            .get_mut(&key)
            .and_then(|font_entries| font_entries.get_mut(text_ref))
        {
            let layout_index = entry.layout_index;
            if let Some(frame_stats) = self.frame_stats.as_mut() {
                frame_stats.owned_hits = frame_stats.owned_hits.saturating_add(1);
            }
            if self.alias_count < self.max_aliases {
                let replaced =
                    self.insert_shared_alias(key, text_key, Arc::clone(text), layout_index);
                debug_assert!(replaced.is_none());
                self.alias_count += usize::from(replaced.is_none());
            }
            return self.layouts[layout_index].as_ref();
        }

        let layout = Box::new(build_cached_text_layout(
            font,
            fonts,
            text_ref,
            key.line_spacing,
            key.wrap_width_pixels,
            text_layout_mesh_seed(key, text_ref),
        ));
        self.record_layout_build(layout.as_ref());
        if let Some(layout_index) =
            self.insert_shared_layout(key, text_key, Arc::clone(text), layout)
        {
            self.layouts[layout_index].as_ref()
        } else {
            self.uncached_layout_ref()
        }
    }
}

/// Prepares one independently retained frame-layout slot plus the shared
/// transient vertex and draw-sort pools. Slot storage grows only at this
/// boundary; an unprepared live slot saturates to the single reusable fallback.
pub fn prewarm_frame_inline_text_slot(
    cache: &mut TextLayoutCache,
    scratch: &mut ComposeScratch,
    fonts: &font::FontMap,
    font_name: &'static str,
    text: actors::InlineText,
    slot: u8,
    vertex_buffers: usize,
) {
    let Some(font) = fonts.get(font_name) else {
        return;
    };
    let slot_index = usize::from(slot);
    if cache.frame_inline_slots.len() <= slot_index {
        cache
            .frame_inline_slots
            .resize_with(slot_index + 1, FrameInlineLayoutSlot::new);
    }
    let content = actors::TextContent::frame_inline_slot(text, slot);
    let layout = cache.get_or_build(font, fonts, &content, None, None);
    let vertices_per_buffer = layout.glyph_count.saturating_mul(6);
    let texture_pages = layout.texture_pages.len().max(1);
    let vertex_buffers = vertex_buffers.saturating_mul(texture_pages);
    if scratch.transient_text_mesh_builders.capacity() < texture_pages {
        scratch
            .transient_text_mesh_builders
            .reserve(texture_pages.saturating_sub(scratch.transient_text_mesh_builders.len()));
    }
    if scratch.z_counts.capacity() < vertex_buffers {
        scratch
            .z_counts
            .reserve(vertex_buffers.saturating_sub(scratch.z_counts.len()));
    }
    if scratch.z_perm.capacity() < vertex_buffers {
        scratch
            .z_perm
            .reserve(vertex_buffers.saturating_sub(scratch.z_perm.len()));
    }
    scratch
        .recycled_text_mesh_vertices
        .reserve(vertex_buffers.saturating_sub(scratch.recycled_text_mesh_vertices.len()));
    while scratch.recycled_text_mesh_vertices.len() < vertex_buffers {
        scratch.recycled_text_mesh_vertices.push(Vec::new());
    }
    for vertices in scratch
        .recycled_text_mesh_vertices
        .iter_mut()
        .take(vertex_buffers)
    {
        if vertices.capacity() < vertices_per_buffer {
            vertices.reserve(vertices_per_buffer);
        }
    }
}

#[inline(always)]
const fn saturating_u32(value: usize) -> u32 {
    if value > u32::MAX as usize {
        u32::MAX
    } else {
        value as u32
    }
}

#[inline(always)]
fn elapsed_us_since(started: Instant) -> u32 {
    started.elapsed().as_micros().min(u128::from(u32::MAX)) as u32
}

fn font_chain_key(font: &font::Font, fonts: &font::FontMap) -> u64 {
    if font.chain_key != 0 {
        return font.chain_key;
    }
    let mut hasher = DefaultHasher::new();
    let mut current = Some(font);
    while let Some(font) = current {
        (font as *const font::Font as usize).hash(&mut hasher);
        current = font.fallback_font_name.and_then(|name| fonts.get(name));
    }
    hasher.finish()
}

#[inline(always)]
fn text_layout_mesh_seed(key: TextLayoutKey, text: &str) -> u64 {
    let mut hasher = rustc_hash::FxHasher::default();
    key.hash(&mut hasher);
    text.hash(&mut hasher);
    let seed = hasher.finish();
    if seed == renderer::INVALID_TMESH_CACHE_KEY {
        1
    } else {
        seed
    }
}

#[inline(always)]
fn text_batch_cache_key(
    layout_seed: u64,
    texture_page: TextPageId,
    stroke: bool,
    align: actors::TextAlign,
) -> u64 {
    let mut hasher = rustc_hash::FxHasher::default();
    layout_seed.hash(&mut hasher);
    texture_page.hash(&mut hasher);
    stroke.hash(&mut hasher);
    match align {
        actors::TextAlign::Left => 0u8,
        actors::TextAlign::Center => 1u8,
        actors::TextAlign::Right => 2u8,
    }
    .hash(&mut hasher);
    let key = hasher.finish();
    if key == renderer::INVALID_TMESH_CACHE_KEY {
        layout_seed.wrapping_add(1).max(1)
    } else {
        key
    }
}

#[inline(always)]
fn cached_glyph(
    texture_pages: &mut TextPageBuilder,
    glyph: &font::Glyph,
    char_index: usize,
    draw_quad: bool,
) -> CachedGlyph {
    CachedGlyph {
        texture_page: intern_text_page(texture_pages, &glyph.texture_key),
        stroke_page: glyph
            .stroke_texture_key
            .as_ref()
            .map(|stroke_key| intern_text_page(texture_pages, stroke_key)),
        uv_scale: glyph.uv_scale,
        uv_offset: glyph.uv_offset,
        size: glyph.size,
        offset: glyph.offset,
        advance_i32: glyph.advance_i32,
        char_index,
        draw_quad,
    }
}

#[inline(always)]
fn glyph_has_fill_quad(glyph: &CachedGlyph) -> bool {
    glyph.draw_quad && glyph.size[0].abs() >= 1e-6 && glyph.size[1].abs() >= 1e-6
}

#[inline(always)]
fn start_x_logical(align: actors::TextAlign, block_w_logical: f32, line_w_logical: f32) -> i32 {
    let align_value = match align {
        actors::TextAlign::Left => 0.0,
        actors::TextAlign::Center => 0.5,
        actors::TextAlign::Right => 1.0,
    };
    let start = (-0.5f32).mul_add(
        block_w_logical,
        align_value * (block_w_logical - line_w_logical),
    );
    lrint_ties_even(start) as i32
}

#[inline(always)]
fn text_block_height_i(font_height: i32, line_spacing: i32, num_lines: usize) -> i32 {
    if num_lines > 1 {
        font_height + ((num_lines - 1) as i32 * line_spacing)
    } else {
        font_height
    }
}

fn resolve_text_layout_placement(
    layout: &CachedTextLayout,
    scale: [f32; 2],
    fit_width: Option<f32>,
    fit_height: Option<f32>,
    max_width: Option<f32>,
    max_height: Option<f32>,
    max_w_pre_zoom: bool,
    max_h_pre_zoom: bool,
    parent: SmRect,
    align: [f32; 2],
    offset: [f32; 2],
) -> Option<TextLayoutPlacement> {
    let num_lines = layout.lines.len();
    if num_lines == 0 {
        return None;
    }

    let block_w_logical_even = quantize_up_even_i32(layout.max_logical_width_i) as f32;
    let cap_height = if layout.font_height > 0 {
        layout.font_height as f32
    } else {
        layout.line_spacing as f32
    };
    let block_h_logical_i = text_block_height_i(layout.font_height, layout.line_spacing, num_lines);
    let block_h_logical = if block_h_logical_i > 0 {
        block_h_logical_i as f32
    } else {
        cap_height
    };

    let s_w_fit = fit_width.map_or(f32::INFINITY, |w| {
        if block_w_logical_even > 0.0 {
            w / block_w_logical_even
        } else {
            1.0
        }
    });
    let s_h_fit = fit_height.map_or(f32::INFINITY, |h| {
        if block_h_logical > 0.0 {
            h / block_h_logical
        } else {
            1.0
        }
    });
    let fit_s = if s_w_fit.is_infinite() && s_h_fit.is_infinite() {
        1.0
    } else {
        s_w_fit.min(s_h_fit).max(0.0)
    };

    let width_before_zoom = block_w_logical_even * fit_s;
    let height_before_zoom = block_h_logical * fit_s;
    let width_after_zoom = width_before_zoom * scale[0];
    let height_after_zoom = height_before_zoom * scale[1];

    let denom_w_for_max = if max_w_pre_zoom {
        width_before_zoom
    } else {
        width_after_zoom
    };
    let denom_h_for_max = if max_h_pre_zoom {
        height_before_zoom
    } else {
        height_after_zoom
    };

    let max_s_w = max_width.map_or(1.0, |mw| {
        if denom_w_for_max > mw {
            (mw / denom_w_for_max).max(0.0)
        } else {
            1.0
        }
    });
    let max_s_h = max_height.map_or(1.0, |mh| {
        if denom_h_for_max > mh {
            (mh / denom_h_for_max).max(0.0)
        } else {
            1.0
        }
    });

    let sx = scale[0] * fit_s * max_s_w;
    let sy = scale[1] * fit_s * max_s_h;
    if sx.abs() < 1e-6 || sy.abs() < 1e-6 {
        return None;
    }

    let block_w_px = block_w_logical_even * sx;
    let block_h_px = block_h_logical * sy;
    let block_left_sm = align[0].mul_add(-block_w_px, parent.x + offset[0]);
    let block_top_sm = align[1].mul_add(-block_h_px, parent.y + offset[1]);

    Some(TextLayoutPlacement {
        sx,
        sy,
        block_center_x: 0.5f32.mul_add(block_w_px, block_left_sm),
        block_center_y: 0.5f32.mul_add(block_h_px, block_top_sm),
    })
}

#[inline(always)]
fn push_cached_line(
    lines: &mut Vec<CachedLine>,
    max_logical_width_i: &mut i32,
    width_i32: i32,
    glyph_start: usize,
    glyph_end: usize,
) {
    *max_logical_width_i = (*max_logical_width_i).max(width_i32);
    lines.push(CachedLine {
        width_i32,
        glyph_start,
        glyph_len: glyph_end.saturating_sub(glyph_start),
    });
}

#[inline(always)]
fn attr_end(attr: &actors::TextAttribute) -> usize {
    attr.start.saturating_add(attr.length)
}

struct TextAttrCursor<'a> {
    attributes: &'a [actors::TextAttribute],
    start_order: AttrIndices,
    end_order: AttrIndices,
    active: AttrIndices,
    active_max: Option<usize>,
    next_start: usize,
    next_end: usize,
}

impl<'a> TextAttrCursor<'a> {
    fn new(attributes: &'a [actors::TextAttribute]) -> Option<Self> {
        if attributes.is_empty() {
            return None;
        }

        let mut start_order = AttrIndices::with_capacity(attributes.len());
        let mut end_order = AttrIndices::with_capacity(attributes.len());
        for index in 0..attributes.len() {
            start_order.push(index);
            end_order.push(index);
        }

        start_order.sort_unstable_by_key(|&index| (attributes[index].start, index));
        end_order.sort_unstable_by_key(|&index| (attr_end(&attributes[index]), index));

        Some(Self {
            attributes,
            start_order,
            end_order,
            active: AttrIndices::new(),
            active_max: None,
            next_start: 0,
            next_end: 0,
        })
    }

    #[inline(always)]
    fn push_active(&mut self, attr_index: usize) {
        self.active.push(attr_index);
        self.active_max = Some(
            self.active_max
                .map_or(attr_index, |max| max.max(attr_index)),
        );
    }

    #[inline(always)]
    fn remove_active(&mut self, attr_index: usize) {
        let Some(index) = self.active.iter().position(|&index| index == attr_index) else {
            return;
        };
        self.active.swap_remove(index);
        if self.active_max == Some(attr_index) {
            self.active_max = self.active.iter().copied().max();
        }
    }

    #[inline(always)]
    fn colors_for(&mut self, char_index: usize) -> [[f32; 4]; 4] {
        while self.next_end < self.end_order.len()
            && attr_end(&self.attributes[self.end_order[self.next_end]]) <= char_index
        {
            let attr_index = self.end_order[self.next_end];
            self.remove_active(attr_index);
            self.next_end += 1;
        }

        while self.next_start < self.start_order.len()
            && self.attributes[self.start_order[self.next_start]].start <= char_index
        {
            let attr_index = self.start_order[self.next_start];
            let attr = &self.attributes[attr_index];
            if char_index < attr_end(attr) {
                self.push_active(attr_index);
            }
            self.next_start += 1;
        }

        self.active_max
            .map(|index| self.attributes[index].colors())
            .unwrap_or([[1.0; 4]; 4])
    }

    #[cfg(test)]
    fn tint_for(&mut self, char_index: usize) -> [f32; 4] {
        self.colors_for(char_index)[0]
    }
}

fn flush_wrapped_word(
    texture_pages: &mut TextPageBuilder,
    lines: &mut Vec<CachedLine>,
    max_logical_width_i: &mut i32,
    line_width: &mut i32,
    line_glyph_start: &mut usize,
    glyphs: &mut Vec<CachedGlyph>,
    line_has_word: &mut bool,
    word_active: &mut bool,
    word_width: &mut i32,
    word_first_char: usize,
    word_space_before: &mut Option<usize>,
    word_glyphs: &mut WordGlyphs,
    wrap_width_pixels: i32,
    space_glyph: Option<&font::Glyph>,
    space_width: i32,
    draws_space: bool,
) {
    if !*word_active {
        return;
    }

    if !*line_has_word {
        *line_width = *word_width;
        glyphs.extend(word_glyphs.drain(..));
        *line_has_word = true;
    } else if line_width.saturating_add(space_width + *word_width) <= wrap_width_pixels {
        *line_width += space_width + *word_width;
        if let Some(glyph) = space_glyph {
            glyphs.push(cached_glyph(
                texture_pages,
                glyph,
                word_space_before.unwrap_or(word_first_char.saturating_sub(1)),
                draws_space,
            ));
        }
        glyphs.extend(word_glyphs.drain(..));
    } else {
        push_cached_line(
            lines,
            max_logical_width_i,
            *line_width,
            *line_glyph_start,
            glyphs.len(),
        );
        *line_glyph_start = glyphs.len();
        *line_width = *word_width;
        glyphs.extend(word_glyphs.drain(..));
        *line_has_word = true;
    }

    *word_active = false;
    *word_width = 0;
    *word_space_before = None;
}

struct TextMeshBatchBuilder {
    texture_page: TextPageId,
    vertices: Vec<renderer::TexturedMeshVertex>,
}

#[inline(always)]
fn take_recycled_text_mesh_vertices(
    recycled_vertices: &mut Vec<Vec<renderer::TexturedMeshVertex>>,
) -> Vec<renderer::TexturedMeshVertex> {
    recycled_vertices.pop().unwrap_or_default()
}

#[inline(always)]
fn text_mesh_batch_builder<'a>(
    builders: &'a mut Vec<TextMeshBatchBuilder>,
    recycled_vertices: &mut Vec<Vec<renderer::TexturedMeshVertex>>,
    texture_page: TextPageId,
) -> &'a mut TextMeshBatchBuilder {
    if let Some(index) = builders
        .iter()
        .position(|builder| builder.texture_page == texture_page)
    {
        return &mut builders[index];
    }
    builders.push(TextMeshBatchBuilder {
        texture_page,
        vertices: take_recycled_text_mesh_vertices(recycled_vertices),
    });
    builders
        .last_mut()
        .expect("text batch builder inserted for texture page")
}

#[inline(always)]
fn push_text_mesh_quad_with_color(
    builders: &mut Vec<TextMeshBatchBuilder>,
    recycled_vertices: &mut Vec<Vec<renderer::TexturedMeshVertex>>,
    texture_page: TextPageId,
    quad_x: f32,
    quad_y: f32,
    size: [f32; 2],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    color: [f32; 4],
) {
    let out = &mut text_mesh_batch_builder(builders, recycled_vertices, texture_page).vertices;
    let x0 = quad_x;
    let y0 = quad_y;
    let x1 = quad_x + size[0];
    let y1 = quad_y + size[1];
    let u0 = uv_offset[0];
    let v0 = uv_offset[1];
    let u1 = uv_offset[0] + uv_scale[0];
    let v1 = uv_offset[1] + uv_scale[1];
    let tex_matrix_scale = [1.0, 1.0];

    out.reserve(6);
    out.push(renderer::TexturedMeshVertex {
        pos: [x0, y0, 0.0],
        uv: [u0, v0],
        tex_matrix_scale,
        color,
    });
    out.push(renderer::TexturedMeshVertex {
        pos: [x0, y1, 0.0],
        uv: [u0, v1],
        tex_matrix_scale,
        color,
    });
    out.push(renderer::TexturedMeshVertex {
        pos: [x1, y1, 0.0],
        uv: [u1, v1],
        tex_matrix_scale,
        color,
    });
    out.push(renderer::TexturedMeshVertex {
        pos: [x0, y0, 0.0],
        uv: [u0, v0],
        tex_matrix_scale,
        color,
    });
    out.push(renderer::TexturedMeshVertex {
        pos: [x1, y1, 0.0],
        uv: [u1, v1],
        tex_matrix_scale,
        color,
    });
    out.push(renderer::TexturedMeshVertex {
        pos: [x1, y0, 0.0],
        uv: [u1, v0],
        tex_matrix_scale,
        color,
    });
}

#[inline(always)]
fn push_text_mesh_quad(
    builders: &mut Vec<TextMeshBatchBuilder>,
    recycled_vertices: &mut Vec<Vec<renderer::TexturedMeshVertex>>,
    texture_page: TextPageId,
    quad_x: f32,
    quad_y: f32,
    size: [f32; 2],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
) {
    push_text_mesh_quad_with_color(
        builders,
        recycled_vertices,
        texture_page,
        quad_x,
        quad_y,
        size,
        uv_scale,
        uv_offset,
        [1.0; 4],
    );
}

fn finish_text_mesh_batches(
    builders: Vec<TextMeshBatchBuilder>,
    layout_seed: u64,
    stroke: bool,
    align: actors::TextAlign,
) -> Vec<CachedTextMeshBatch> {
    let mut out = Vec::with_capacity(builders.len());
    for builder in builders {
        if builder.vertices.is_empty() {
            continue;
        }
        out.push(CachedTextMeshBatch {
            texture_page: builder.texture_page,
            geom_cache_key: text_batch_cache_key(layout_seed, builder.texture_page, stroke, align),
            vertices: Arc::from(builder.vertices),
        });
    }
    out
}

fn build_text_mesh_batches_for_align(
    layout_seed: u64,
    font_height: i32,
    line_spacing: i32,
    max_logical_width_i: i32,
    lines: &[CachedLine],
    glyphs: &[CachedGlyph],
    align: actors::TextAlign,
    stroke: bool,
) -> Vec<CachedTextMeshBatch> {
    if lines.is_empty() || glyphs.is_empty() {
        return Vec::new();
    }

    let block_w_logical_even = quantize_up_even_i32(max_logical_width_i) as f32;
    let block_h_logical_i = if lines.len() > 1 {
        font_height + ((lines.len() - 1) as i32 * line_spacing)
    } else {
        font_height
    };
    let mut pen_y_logical = lrint_ties_even(-(block_h_logical_i as f32) * 0.5) as i32;
    let line_padding = line_spacing - font_height;
    let mut builders = Vec::new();
    let mut recycled_vertices = Vec::new();

    for line in lines {
        pen_y_logical += font_height;
        let baseline_local_logical = pen_y_logical as f32;
        let mut pen_x_logical = start_x_logical(align, block_w_logical_even, line.width_i32 as f32);

        let line_glyphs =
            &glyphs[line.glyph_start..line.glyph_start.saturating_add(line.glyph_len)];
        for glyph in line_glyphs {
            let texture_page = if stroke {
                glyph.stroke_page
            } else if glyph_has_fill_quad(glyph) {
                Some(glyph.texture_page)
            } else {
                None
            };
            let Some(texture_page) = texture_page else {
                pen_x_logical += glyph.advance_i32;
                continue;
            };

            let quad_x_logical = pen_x_logical as f32 + glyph.offset[0];
            let quad_y_logical = baseline_local_logical + glyph.offset[1];
            push_text_mesh_quad(
                &mut builders,
                &mut recycled_vertices,
                texture_page,
                quad_x_logical,
                quad_y_logical,
                glyph.size,
                glyph.uv_scale,
                glyph.uv_offset,
            );
            pen_x_logical += glyph.advance_i32;
        }
        pen_y_logical += line_padding;
    }

    finish_text_mesh_batches(builders, layout_seed, stroke, align)
}

fn push_transient_text_mesh_quad(
    builders: &mut Vec<TextMeshBatchBuilder>,
    recycled_vertices: &mut Vec<Vec<renderer::TexturedMeshVertex>>,
    texture_page: TextPageId,
    quad_x: f32,
    quad_y: f32,
    size: [f32; 2],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    corner_colors: [[f32; 4]; 4],
    jitter_offset: Option<[f32; 2]>,
    distortion: f32,
    char_index: usize,
) {
    const CORNERS: [usize; 6] = [0, 2, 3, 0, 3, 1];
    let out = &mut text_mesh_batch_builder(builders, recycled_vertices, texture_page).vertices;
    let x0 = quad_x;
    let y0 = quad_y;
    let x1 = quad_x + size[0];
    let y1 = quad_y + size[1];
    let u0 = uv_offset[0];
    let v0 = uv_offset[1];
    let u1 = uv_offset[0] + uv_scale[0];
    let v1 = uv_offset[1] + uv_scale[1];
    let positions = [[x0, y0, 0.0], [x1, y0, 0.0], [x0, y1, 0.0], [x1, y1, 0.0]];
    let uvs = [[u0, v0], [u1, v0], [u0, v1], [u1, v1]];
    let tex_matrix_scale = [1.0, 1.0];
    out.reserve(6);
    for corner in CORNERS {
        let mut pos = positions[corner];
        if distortion.abs() > 1e-6 {
            let [dx, dy] = text_distortion_offset(distortion, char_index, corner, size[0], size[1]);
            pos[0] += dx;
            pos[1] += dy;
        }
        if let Some([dx, dy]) = jitter_offset {
            pos[0] += dx;
            pos[1] += dy;
        }
        out.push(renderer::TexturedMeshVertex {
            pos,
            uv: uvs[corner],
            tex_matrix_scale,
            color: corner_colors[corner],
        });
    }
}

fn build_transient_text_mesh_builders(
    layout: &CachedTextLayout,
    text_align: actors::TextAlign,
    attributes: &[actors::TextAttribute],
    jitter_seed: Option<u32>,
    distortion: f32,
    stroke: bool,
    builders: &mut Vec<TextMeshBatchBuilder>,
    recycled_vertices: &mut Vec<Vec<renderer::TexturedMeshVertex>>,
) {
    builders.clear();
    if layout.lines.is_empty() || layout.glyphs.is_empty() {
        return;
    }

    let block_w_logical_even = quantize_up_even_i32(layout.max_logical_width_i) as f32;
    let block_h_logical_i = if layout.lines.len() > 1 {
        layout.font_height + ((layout.lines.len() - 1) as i32 * layout.line_spacing)
    } else {
        layout.font_height
    };
    let mut pen_y_logical = lrint_ties_even(-(block_h_logical_i as f32) * 0.5) as i32;
    let line_padding = layout.line_spacing - layout.font_height;
    let mut attr_cursor = (!stroke).then(|| TextAttrCursor::new(attributes)).flatten();

    for line in &layout.lines {
        pen_y_logical += layout.font_height;
        let baseline_local_logical = pen_y_logical as f32;
        let mut pen_x_logical =
            start_x_logical(text_align, block_w_logical_even, line.width_i32 as f32);
        let line_glyphs =
            &layout.glyphs[line.glyph_start..line.glyph_start.saturating_add(line.glyph_len)];
        for glyph in line_glyphs {
            let texture_page = if stroke {
                glyph.stroke_page
            } else if glyph_has_fill_quad(glyph) {
                Some(glyph.texture_page)
            } else {
                None
            };
            let Some(texture_page) = texture_page else {
                pen_x_logical += glyph.advance_i32;
                continue;
            };
            let colors = attr_cursor
                .as_mut()
                .map_or([[1.0; 4]; 4], |cursor| cursor.colors_for(glyph.char_index));
            push_transient_text_mesh_quad(
                builders,
                recycled_vertices,
                texture_page,
                pen_x_logical as f32 + glyph.offset[0],
                baseline_local_logical + glyph.offset[1],
                glyph.size,
                glyph.uv_scale,
                glyph.uv_offset,
                colors,
                jitter_seed.map(|seed| text_jitter_offset(seed, glyph.char_index)),
                distortion,
                glyph.char_index,
            );
            pen_x_logical += glyph.advance_i32;
        }
        pen_y_logical += line_padding;
    }
}

#[inline(always)]
fn text_jitter_offset(seed: u32, char_index: usize) -> [f32; 2] {
    let mut value = seed.wrapping_mul(0x9e37_79b9);
    value ^= (char_index as u32).wrapping_mul(0x85eb_ca6b);
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    [(value & 1) as f32, ((value >> 1) % 3) as f32]
}

#[inline(always)]
fn text_distortion_offset(
    amount: f32,
    char_index: usize,
    corner: usize,
    width: f32,
    height: f32,
) -> [f32; 2] {
    let mut value = 0xa24b_aed4_u32;
    value ^= (char_index as u32).wrapping_mul(0x9e37_79b9);
    value ^= (corner as u32).wrapping_mul(0x85eb_ca6b);
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    let x = ((value % 9) as f32 / 8.0 - 0.5) * amount * width;
    value = value.rotate_left(13).wrapping_mul(0x846c_a68b);
    let y = ((value % 9) as f32 / 8.0 - 0.5) * amount * height;
    [x, y]
}

fn build_cached_text_layout(
    font: &font::Font,
    fonts: &font::FontMap,
    text: &str,
    line_spacing: i32,
    wrap_width_pixels: i32,
    layout_seed: u64,
) -> CachedTextLayout {
    build_cached_text_layout_reusing(
        font,
        fonts,
        text,
        line_spacing,
        wrap_width_pixels,
        layout_seed,
        CachedTextLayout::empty(),
    )
}

fn rebuild_cached_text_layout(
    layout: &mut CachedTextLayout,
    font: &font::Font,
    fonts: &font::FontMap,
    text: &str,
    line_spacing: i32,
    wrap_width_pixels: i32,
    layout_seed: u64,
) {
    let reusable = std::mem::replace(layout, CachedTextLayout::empty());
    *layout = build_cached_text_layout_reusing(
        font,
        fonts,
        text,
        line_spacing,
        wrap_width_pixels,
        layout_seed,
        reusable,
    );
}

fn build_cached_text_layout_reusing(
    font: &font::Font,
    fonts: &font::FontMap,
    text: &str,
    line_spacing: i32,
    wrap_width_pixels: i32,
    layout_seed: u64,
    reusable: CachedTextLayout,
) -> CachedTextLayout {
    let draws_space = font.glyph_map.contains_key(&' ');
    let space_glyph = font::find_glyph(font, ' ', fonts);
    let space_width = space_glyph.map_or(0, |glyph| glyph.advance_i32);
    let mut max_logical_width_i = 0i32;
    let line_count = text
        .as_bytes()
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
        .saturating_add(1);
    let mut lines = reusable.lines;
    lines.clear();
    if lines.capacity() < line_count {
        lines.reserve(line_count);
    }
    let mut glyphs = reusable.glyphs;
    glyphs.clear();
    if glyphs.capacity() < text.len() {
        glyphs.reserve(text.len());
    }
    let mut texture_pages = reusable.texture_pages;
    texture_pages.clear();
    let mut start_char = 0usize;

    for src in text.split('\n') {
        let mut char_index = start_char;
        if wrap_width_pixels < 0 {
            let mut width_i32 = 0i32;
            let line_glyph_start = glyphs.len();
            for ch in src.chars() {
                if let Some(glyph) = font::find_glyph(font, ch, fonts) {
                    width_i32 += glyph.advance_i32;
                    glyphs.push(cached_glyph(
                        &mut texture_pages,
                        glyph,
                        char_index,
                        ch != ' ' || draws_space,
                    ));
                }
                char_index += 1;
            }
            push_cached_line(
                &mut lines,
                &mut max_logical_width_i,
                width_i32,
                line_glyph_start,
                glyphs.len(),
            );
            start_char = char_index.saturating_add(1);
            continue;
        }

        let mut line_width = 0i32;
        let mut line_glyph_start = glyphs.len();
        let mut line_has_word = false;
        let mut pending_space = None;
        let mut word_active = false;
        let mut word_width = 0i32;
        let mut word_first_char = start_char;
        let mut word_space_before = None;
        let mut word_glyphs = WordGlyphs::new();

        for ch in src.chars() {
            if ch == ' ' {
                flush_wrapped_word(
                    &mut texture_pages,
                    &mut lines,
                    &mut max_logical_width_i,
                    &mut line_width,
                    &mut line_glyph_start,
                    &mut glyphs,
                    &mut line_has_word,
                    &mut word_active,
                    &mut word_width,
                    word_first_char,
                    &mut word_space_before,
                    &mut word_glyphs,
                    wrap_width_pixels,
                    space_glyph,
                    space_width,
                    draws_space,
                );
                pending_space.get_or_insert(char_index);
            } else {
                if !word_active {
                    word_active = true;
                    word_first_char = char_index;
                    word_space_before = pending_space.take();
                }
                if let Some(glyph) = font::find_glyph(font, ch, fonts) {
                    word_width += glyph.advance_i32;
                    word_glyphs.push(cached_glyph(&mut texture_pages, glyph, char_index, true));
                }
            }
            char_index += 1;
        }

        flush_wrapped_word(
            &mut texture_pages,
            &mut lines,
            &mut max_logical_width_i,
            &mut line_width,
            &mut line_glyph_start,
            &mut glyphs,
            &mut line_has_word,
            &mut word_active,
            &mut word_width,
            word_first_char,
            &mut word_space_before,
            &mut word_glyphs,
            wrap_width_pixels,
            space_glyph,
            space_width,
            draws_space,
        );

        if line_has_word {
            push_cached_line(
                &mut lines,
                &mut max_logical_width_i,
                line_width,
                line_glyph_start,
                glyphs.len(),
            );
        } else {
            push_cached_line(
                &mut lines,
                &mut max_logical_width_i,
                0,
                glyphs.len(),
                glyphs.len(),
            );
        }
        start_char = char_index.saturating_add(1);
    }

    CachedTextLayout {
        layout_seed,
        font_height: font.height,
        line_spacing,
        max_logical_width_i,
        glyph_count: glyphs.len(),
        texture_pages,
        lines,
        glyphs,
        fill_batches: CachedTextMeshVariants::default(),
        stroke_batches: CachedTextMeshVariants::default(),
    }
}

#[cfg(test)]
fn wrap_text_lines_by_words<F>(
    text: &str,
    wrap_width_pixels: i32,
    space_width: i32,
    mut word_width: F,
) -> Vec<Box<str>>
where
    F: FnMut(&str) -> i32,
{
    let mut out = Vec::new();
    for src in text.split('\n') {
        if wrap_width_pixels < 0 {
            out.push(src.into());
            continue;
        }
        let mut words = src.split(' ').filter(|word| !word.is_empty());
        let Some(first) = words.next() else {
            out.push("".into());
            continue;
        };
        let mut line = String::from(first);
        let mut line_width = word_width(first);
        for word in words {
            let width_to_add = space_width + word_width(word);
            if line_width + width_to_add <= wrap_width_pixels {
                line.push(' ');
                line.push_str(word);
                line_width += width_to_add;
            } else {
                out.push(line.into_boxed_str());
                line = word.to_owned();
                line_width = word_width(word);
            }
        }
        out.push(line.into_boxed_str());
    }
    out
}

/* ======================= ACTOR -> OBJECT CONVERSION ======================= */

#[derive(Clone, Copy)]
struct SmRect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

#[inline(always)]
fn retained_frame_key(
    frame_id: u64,
    parent: SmRect,
    metrics: &Metrics,
    base_z: i16,
    style: ComposeStyle,
) -> RetainedFrameKey {
    RetainedFrameKey {
        frame_id,
        parent: [
            parent.x.to_bits(),
            parent.y.to_bits(),
            parent.w.to_bits(),
            parent.h.to_bits(),
        ],
        metrics: [
            metrics.left.to_bits(),
            metrics.right.to_bits(),
            metrics.top.to_bits(),
            metrics.bottom.to_bits(),
        ],
        tint: style.tint.map(f32::to_bits),
        base_z,
        blend: match style.blend {
            None => 0,
            Some(BlendMode::Alpha) => 1,
            Some(BlendMode::Add) => 2,
            Some(BlendMode::Multiply) => 3,
            Some(BlendMode::Subtract) => 4,
        },
    }
}

fn capture_retained_frame(
    objects: &FrameBuilder,
    sprite_instances: &[renderer::SpriteInstanceRaw],
    object_start: usize,
    sprite_start: usize,
) -> Option<CachedRetainedFrame> {
    let sprite_start_u32 = u32::try_from(sprite_start).ok()?;
    if object_start > objects.len() {
        return None;
    }
    let mut cached_builder = FrameBuilder::default();
    cached_builder.reserve(objects.len().saturating_sub(object_start));
    for index in object_start..objects.len() {
        let mut object = objects.clone_retained_object(index)?;
        match &mut object.object_type {
            EditablePayload::Sprite(index) => {
                *index = index.checked_sub(sprite_start_u32)?;
            }
            EditablePayload::TexturedMesh {
                vertices: renderer::TexturedMeshVertices::Transient(_),
                ..
            } => return None,
            _ => {}
        }
        object.order = 0;
        cached_builder.push(object);
    }
    Some(CachedRetainedFrame {
        builder: cached_builder,
        sprite_instances: sprite_instances.get(sprite_start..)?.to_vec(),
    })
}

fn append_retained_frame(
    cached: &CachedRetainedFrame,
    order_counter: &mut u32,
    out: &mut FrameBuilder,
    sprite_instances: &mut Vec<renderer::SpriteInstanceRaw>,
) {
    let sprite_start = sprite_instances.len().min(u32::MAX as usize) as u32;
    sprite_instances.extend_from_slice(&cached.sprite_instances);
    out.append_retained(&cached.builder, sprite_start, order_counter);
}

#[cfg(any(test, feature = "bench-support"))]
fn append_retained_frame_legacy(
    cached: &CachedRetainedFrame,
    order_counter: &mut u32,
    out: &mut FrameBuilder,
    sprite_instances: &mut Vec<renderer::SpriteInstanceRaw>,
) {
    let sprite_start = sprite_instances.len().min(u32::MAX as usize) as u32;
    sprite_instances.extend_from_slice(&cached.sprite_instances);
    out.reserve(cached.builder.len());
    for index in 0..cached.builder.len() {
        let mut object = cached
            .builder
            .clone_retained_object(index)
            .expect("retained frame contains only clonable payloads");
        if let EditablePayload::Sprite(index) = &mut object.object_type {
            *index = index.saturating_add(sprite_start);
        }
        object.order = *order_counter;
        *order_counter = order_counter.saturating_add(1);
        out.push(object);
    }
}

#[inline(always)]
fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    (b - a).mul_add(t, a)
}

#[inline(always)]
fn apply_effect_to_sprite(
    effect: anim::EffectState,
    elapsed: f32,
    tint: &mut [f32; 4],
    scale: &mut [f32; 2],
    rot_deg: &mut [f32; 3],
) {
    // We currently don't have song beat/time split plumbed here, so use elapsed for both.
    let beat = elapsed;
    if matches!(effect.mode, anim::EffectMode::Spin) {
        // ITGmania spin uses effect delta from clock and does not use effectoffset.
        let units = anim::effect_clock_units(effect, elapsed, beat);
        rot_deg[0] = (rot_deg[0] + effect.magnitude[0] * units).rem_euclid(360.0);
        rot_deg[1] = (rot_deg[1] + effect.magnitude[1] * units).rem_euclid(360.0);
        rot_deg[2] = (rot_deg[2] + effect.magnitude[2] * units).rem_euclid(360.0);
    }

    if let Some(percent) = anim::effect_mix(effect, elapsed, beat) {
        match effect.mode {
            anim::EffectMode::DiffuseRamp => {
                for (i, out) in tint.iter_mut().enumerate() {
                    let c = lerp_f32(effect.color2[i], effect.color1[i], percent).clamp(0.0, 1.0);
                    *out *= c;
                }
            }
            anim::EffectMode::DiffuseShift => {
                let between = (((percent + 0.25) * 2.0 * std::f32::consts::PI).sin() * 0.5 + 0.5)
                    .clamp(0.0, 1.0);
                for (i, out) in tint.iter_mut().enumerate() {
                    let c = lerp_f32(effect.color2[i], effect.color1[i], between).clamp(0.0, 1.0);
                    *out *= c;
                }
            }
            anim::EffectMode::Pulse => {
                let offset = (percent * std::f32::consts::PI).sin().clamp(0.0, 1.0);
                let zoom = lerp_f32(effect.magnitude[0], effect.magnitude[1], offset).max(0.0);
                let sx = lerp_f32(effect.color2[0], effect.color1[0], offset).max(0.0);
                let sy = lerp_f32(effect.color2[1], effect.color1[1], offset).max(0.0);
                scale[0] *= zoom * sx;
                scale[1] *= zoom * sy;
            }
            anim::EffectMode::GlowShift
            | anim::EffectMode::Bob
            | anim::EffectMode::Bounce
            | anim::EffectMode::Wag
            | anim::EffectMode::Spin
            | anim::EffectMode::None => {}
        }
    }

    tint[0] = tint[0].clamp(0.0, 1.0);
    tint[1] = tint[1].clamp(0.0, 1.0);
    tint[2] = tint[2].clamp(0.0, 1.0);
    tint[3] = tint[3].clamp(0.0, 1.0);
    scale[0] = scale[0].max(0.0);
    scale[1] = scale[1].max(0.0);
}

#[inline(always)]
fn apply_effect_to_text(
    effect: anim::EffectState,
    elapsed: f32,
    color: &mut [f32; 4],
    scale: &mut [f32; 2],
) {
    // We currently don't have song beat/time split plumbed here, so use elapsed for both.
    let beat = elapsed;
    if let Some(percent) = anim::effect_mix(effect, elapsed, beat) {
        match effect.mode {
            anim::EffectMode::DiffuseRamp => {
                for (i, out) in color.iter_mut().enumerate() {
                    let c = lerp_f32(effect.color2[i], effect.color1[i], percent).clamp(0.0, 1.0);
                    *out *= c;
                }
            }
            anim::EffectMode::DiffuseShift => {
                let between = (((percent + 0.25) * 2.0 * std::f32::consts::PI).sin() * 0.5 + 0.5)
                    .clamp(0.0, 1.0);
                for (i, out) in color.iter_mut().enumerate() {
                    let c = lerp_f32(effect.color2[i], effect.color1[i], between).clamp(0.0, 1.0);
                    *out *= c;
                }
            }
            anim::EffectMode::Pulse => {
                let offset = (percent * std::f32::consts::PI).sin().clamp(0.0, 1.0);
                let zoom = lerp_f32(effect.magnitude[0], effect.magnitude[1], offset).max(0.0);
                let sx = lerp_f32(effect.color2[0], effect.color1[0], offset).max(0.0);
                let sy = lerp_f32(effect.color2[1], effect.color1[1], offset).max(0.0);
                scale[0] *= zoom * sx;
                scale[1] *= zoom * sy;
            }
            anim::EffectMode::GlowShift
            | anim::EffectMode::Bob
            | anim::EffectMode::Bounce
            | anim::EffectMode::Wag
            | anim::EffectMode::Spin
            | anim::EffectMode::None => {}
        }
    }

    color[0] = color[0].clamp(0.0, 1.0);
    color[1] = color[1].clamp(0.0, 1.0);
    color[2] = color[2].clamp(0.0, 1.0);
    color[3] = color[3].clamp(0.0, 1.0);
    scale[0] = scale[0].max(0.0);
    scale[1] = scale[1].max(0.0);
}

#[inline(always)]
fn has_shadow(len: [f32; 2]) -> bool {
    len[0] != 0.0 || len[1] != 0.0
}

#[inline(always)]
fn sprite_source_handle(
    source: &actors::SpriteSource,
    generation: u64,
) -> Option<renderer::TextureHandle> {
    match source {
        actors::SpriteSource::TextureStaticHandle {
            handle,
            generation: handle_generation,
            ..
        }
        | actors::SpriteSource::TextureHandle {
            handle,
            generation: handle_generation,
            ..
        }
        | actors::SpriteSource::ArenaTextureHandle {
            handle,
            generation: handle_generation,
            ..
        } if *handle != renderer::INVALID_TEXTURE_HANDLE && *handle_generation == generation => {
            Some(*handle)
        }
        _ => None,
    }
}

fn push_shadow_objects_for_range(
    out: &mut FrameBuilder,
    sprite_instances: &mut Vec<renderer::SpriteInstanceRaw>,
    recycled_vertices: &mut Vec<Vec<renderer::TexturedMeshVertex>>,
    start: usize,
    end: usize,
    len: [f32; 2],
    color: [f32; 4],
) {
    let t_world = Matrix4::from_translation(Vector3::new(len[0], len[1], 0.0));
    for i in start..end {
        let item = out.items[i];
        let z = item.z.saturating_sub(1);
        match item.kind {
            DrawKind::Sprite => {
                let mut sprite = sprite_instances[item.payload_index as usize];
                let mut shadow_tint = color;
                shadow_tint[3] *= sprite.tint[3];
                sprite.tint = shadow_tint;
                sprite.center[0] += len[0];
                sprite.center[1] += len[1];
                let new_sprite_index = sprite_instances.len() as u32;
                sprite_instances.push(sprite);
                out.push_sprite(
                    item.texture_handle,
                    item.order,
                    z,
                    item.blend,
                    item.camera,
                    new_sprite_index,
                );
            }
            DrawKind::Mesh => {
                let source = out.meshes[item.payload_index as usize]
                    .as_ref()
                    .expect("draw item references live mesh payload");
                let mut transform = source.transform;
                let mut tint = source.tint;
                tint[0] *= color[0];
                tint[1] *= color[1];
                tint[2] *= color[2];
                tint[3] *= color[3];
                transform = t_world * transform;
                out.push_mesh(
                    item.texture_handle,
                    item.order,
                    z,
                    item.blend,
                    item.camera,
                    MeshPayload {
                        transform,
                        tint,
                        vertices: source.vertices.clone(),
                    },
                );
            }
            DrawKind::TexturedMesh => {
                let source = out.textured_meshes[item.payload_index as usize]
                    .as_ref()
                    .expect("draw item references live textured-mesh payload");
                let mut instance = source.instance;
                let mut shadow_tint = color;
                shadow_tint[0] *= instance.tint[0];
                shadow_tint[1] *= instance.tint[1];
                shadow_tint[2] *= instance.tint[2];
                shadow_tint[3] *= instance.tint[3];
                instance.tint = shadow_tint;
                instance.set_transform(t_world * instance.transform());
                let vertices = match &source.vertices {
                    renderer::TexturedMeshVertices::Shared(vertices) => {
                        renderer::TexturedMeshVertices::Shared(Arc::clone(vertices))
                    }
                    renderer::TexturedMeshVertices::Reusable(vertices) => {
                        renderer::TexturedMeshVertices::Reusable(Arc::clone(vertices))
                    }
                    renderer::TexturedMeshVertices::Transient(vertices) => {
                        let mut copy = take_recycled_text_mesh_vertices(recycled_vertices);
                        copy.extend_from_slice(vertices);
                        renderer::TexturedMeshVertices::Transient(copy)
                    }
                };
                out.push_textured_mesh(
                    item.texture_handle,
                    item.order,
                    z,
                    item.blend,
                    item.camera,
                    TexturedMeshPayload {
                        instance,
                        vertices,
                        geom_cache_key: source.geom_cache_key,
                        depth_test: source.depth_test,
                    },
                );
            }
        }
    }
}

#[derive(Clone, Copy)]
struct ComposeStyle {
    tint: [f32; 4],
    blend: Option<BlendMode>,
}

#[derive(Clone, Copy)]
struct ActorBuild<'a> {
    actor: &'a actors::Actor,
    base_z: i16,
}

impl ComposeStyle {
    const IDENTITY: Self = Self {
        tint: [1.0; 4],
        blend: None,
    };

    #[inline(always)]
    fn child(self, tint: [f32; 4], blend: Option<BlendMode>) -> Self {
        Self {
            tint: mul_rgba(self.tint, tint),
            blend: self.blend.or(blend),
        }
    }
}

#[inline(always)]
fn mul_rgba(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [a[0] * b[0], a[1] * b[1], a[2] * b[2], a[3] * b[3]]
}

#[inline(always)]
fn build_actor_list<'a, T: TextureContext + ?Sized>(
    actors: &'a [actors::Actor],
    parent: SmRect,
    m: &Metrics,
    fonts: &'a font::FontMap,
    scratch: &mut ComposeScratch,
    base_z: i16,
    camera: u8,
    style: ComposeStyle,
    cameras: &mut Vec<Matrix4>,
    masks: &mut Vec<WorldRect>,
    order_counter: &mut u32,
    out: &mut FrameBuilder,
    sprite_instances: &mut Vec<renderer::SpriteInstanceRaw>,
    text_cache: &mut TextLayoutCache,
    texture_cache: &mut TextureLookupCache,
    texture_ctx: &T,
    actor_textures: Option<&[Arc<str>]>,
    total_elapsed: f32,
) {
    build_actor_sequence(
        actors.iter().map(|actor| ActorBuild { actor, base_z }),
        parent,
        m,
        fonts,
        scratch,
        camera,
        style,
        cameras,
        masks,
        order_counter,
        out,
        sprite_instances,
        text_cache,
        texture_cache,
        texture_ctx,
        actor_textures,
        total_elapsed,
    );
}

#[allow(clippy::too_many_arguments)]
fn build_actor_sequence<'a, T, I>(
    actor_builds: I,
    parent: SmRect,
    m: &Metrics,
    fonts: &'a font::FontMap,
    scratch: &mut ComposeScratch,
    camera: u8,
    style: ComposeStyle,
    cameras: &mut Vec<Matrix4>,
    masks: &mut Vec<WorldRect>,
    order_counter: &mut u32,
    out: &mut FrameBuilder,
    sprite_instances: &mut Vec<renderer::SpriteInstanceRaw>,
    text_cache: &mut TextLayoutCache,
    texture_cache: &mut TextureLookupCache,
    texture_ctx: &T,
    actor_textures: Option<&[Arc<str>]>,
    total_elapsed: f32,
) where
    T: TextureContext + ?Sized,
    I: IntoIterator<Item = ActorBuild<'a>>,
{
    let mut active_camera = camera;
    let mut camera_stack: SmallVec<[u8; 4]> = SmallVec::new();
    for ActorBuild { actor, base_z } in actor_builds {
        match actor {
            actors::Actor::CameraPush { view_proj } => {
                cameras.push(*view_proj);
                camera_stack.push(active_camera);
                active_camera = cameras.len().saturating_sub(1).try_into().unwrap_or(0u8);
            }
            actors::Actor::CameraPop => {
                active_camera = camera_stack.pop().unwrap_or(camera);
            }
            _ => build_actor_recursive(
                actor,
                parent,
                m,
                fonts,
                scratch,
                base_z,
                active_camera,
                style,
                cameras,
                masks,
                order_counter,
                out,
                sprite_instances,
                text_cache,
                texture_cache,
                texture_ctx,
                actor_textures,
                total_elapsed,
            ),
        }
    }
}

struct TexturedMeshActorView<'a> {
    align: [f32; 2],
    offset: [f32; 2],
    world_z: f32,
    size: [actors::SizeSpec; 2],
    local_transform: Matrix4,
    texture: &'a Arc<str>,
    tint: [f32; 4],
    glow: [f32; 4],
    vertices: TexturedMeshActorVertices<'a>,
    geom_cache_key: renderer::TMeshCacheKey,
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    uv_tex_shift: [f32; 2],
    depth_test: bool,
    visible: bool,
    blend: BlendMode,
    z: i16,
}

#[derive(Clone, Copy)]
enum TexturedMeshActorVertices<'a> {
    Shared(&'a Arc<[renderer::TexturedMeshVertex]>),
    Reusable(&'a Arc<Vec<renderer::TexturedMeshVertex>>),
}

impl TexturedMeshActorVertices<'_> {
    #[inline(always)]
    fn is_empty(self) -> bool {
        match self {
            Self::Shared(vertices) => vertices.is_empty(),
            Self::Reusable(vertices) => vertices.is_empty(),
        }
    }

    #[inline(always)]
    fn clone_for_render(self) -> renderer::TexturedMeshVertices {
        match self {
            Self::Shared(vertices) => renderer::TexturedMeshVertices::Shared(Arc::clone(vertices)),
            Self::Reusable(vertices) => {
                renderer::TexturedMeshVertices::Reusable(Arc::clone(vertices))
            }
        }
    }
}

fn textured_mesh_actor_view(actor: &actors::Actor) -> Option<TexturedMeshActorView<'_>> {
    let (
        align,
        offset,
        world_z,
        size,
        local_transform,
        texture,
        tint,
        glow,
        geom_cache_key,
        uv_scale,
        uv_offset,
        uv_tex_shift,
        depth_test,
        visible,
        blend,
        z,
    ) = match actor {
        actors::Actor::TexturedMesh {
            align,
            offset,
            world_z,
            size,
            local_transform,
            texture,
            tint,
            glow,
            geom_cache_key,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            depth_test,
            visible,
            blend,
            z,
            ..
        }
        | actors::Actor::ReusableTexturedMesh {
            align,
            offset,
            world_z,
            size,
            local_transform,
            texture,
            tint,
            glow,
            geom_cache_key,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            depth_test,
            visible,
            blend,
            z,
            ..
        } => (
            *align,
            *offset,
            *world_z,
            *size,
            *local_transform,
            texture,
            *tint,
            *glow,
            *geom_cache_key,
            *uv_scale,
            *uv_offset,
            *uv_tex_shift,
            *depth_test,
            *visible,
            *blend,
            *z,
        ),
        _ => return None,
    };
    let vertices = match actor {
        actors::Actor::TexturedMesh { vertices, .. } => TexturedMeshActorVertices::Shared(vertices),
        actors::Actor::ReusableTexturedMesh { vertices, .. } => {
            TexturedMeshActorVertices::Reusable(vertices)
        }
        _ => unreachable!("textured mesh fields were matched above"),
    };
    Some(TexturedMeshActorView {
        align,
        offset,
        world_z,
        size,
        local_transform,
        texture,
        tint,
        glow,
        vertices,
        geom_cache_key,
        uv_scale,
        uv_offset,
        uv_tex_shift,
        depth_test,
        visible,
        blend,
        z,
    })
}

#[allow(clippy::too_many_arguments)]
fn build_textured_mesh_actor<T: TextureContext + ?Sized>(
    mesh: TexturedMeshActorView<'_>,
    parent: SmRect,
    m: &Metrics,
    base_z: i16,
    camera: u8,
    style: ComposeStyle,
    order_counter: &mut u32,
    out: &mut FrameBuilder,
    texture_cache: &mut TextureLookupCache,
    texture_ctx: &T,
) {
    if !mesh.visible || mesh.vertices.is_empty() {
        return;
    }

    let rect = place_rect(parent, mesh.align, mesh.offset, mesh.size);
    let base_x = m.left + rect.x;
    let base_y = m.top - rect.y;
    let transform = Matrix4::from_translation(Vector3::new(base_x, base_y, mesh.world_z))
        * Matrix4::from_scale(Vector3::new(1.0, -1.0, 1.0))
        * mesh.local_transform;
    let texture_key = mesh.texture.as_ref();
    let texture_key_ptr = str_ptr(texture_key);
    let texture_handle = texture_cache.texture_handle(texture_ctx, texture_key_ptr, texture_key);
    let actor_blend = style.blend.unwrap_or(mesh.blend);
    let layer = base_z.saturating_add(mesh.z);
    let base_order = *order_counter;
    *order_counter = base_order.saturating_add(1);
    out.push_textured_mesh(
        texture_handle,
        base_order,
        layer,
        actor_blend,
        camera,
        TexturedMeshPayload {
            instance: renderer::TexturedMeshInstanceRaw::new(
                transform,
                mul_rgba(mesh.tint, style.tint),
                mesh.uv_scale,
                mesh.uv_offset,
                mesh.uv_tex_shift,
                false,
            ),
            vertices: mesh.vertices.clone_for_render(),
            geom_cache_key: mesh.geom_cache_key,
            depth_test: mesh.depth_test,
        },
    );
    if mesh.glow[3] > 0.0001 {
        let glow_order = *order_counter;
        *order_counter = glow_order.saturating_add(1);
        out.push_textured_mesh(
            texture_handle,
            glow_order,
            layer,
            actor_blend,
            camera,
            TexturedMeshPayload {
                instance: renderer::TexturedMeshInstanceRaw::new(
                    transform,
                    mul_rgba(mesh.glow, style.tint),
                    mesh.uv_scale,
                    mesh.uv_offset,
                    mesh.uv_tex_shift,
                    true,
                ),
                vertices: mesh.vertices.clone_for_render(),
                geom_cache_key: mesh.geom_cache_key,
                depth_test: mesh.depth_test,
            },
        );
    }
}

#[inline(always)]
fn build_actor_recursive<'a, T: TextureContext + ?Sized>(
    actor: &'a actors::Actor,
    parent: SmRect,
    m: &Metrics,
    fonts: &'a font::FontMap,
    scratch: &mut ComposeScratch,
    base_z: i16,
    camera: u8,
    style: ComposeStyle,
    cameras: &mut Vec<Matrix4>,
    masks: &mut Vec<WorldRect>,
    order_counter: &mut u32,
    out: &mut FrameBuilder,
    sprite_instances: &mut Vec<renderer::SpriteInstanceRaw>,
    text_cache: &mut TextLayoutCache,
    texture_cache: &mut TextureLookupCache,
    texture_ctx: &T,
    actor_textures: Option<&[Arc<str>]>,
    total_elapsed: f32,
) {
    if let Some(mesh) = textured_mesh_actor_view(actor) {
        build_textured_mesh_actor(
            mesh,
            parent,
            m,
            base_z,
            camera,
            style,
            order_counter,
            out,
            texture_cache,
            texture_ctx,
        );
        return;
    }
    match actor {
        actors::Actor::Sprite {
            align,
            offset,
            world_z,
            size,
            source,
            tint,
            z,
            cell,
            grid,
            uv_rect,
            visible,
            flip_x,
            flip_y,
            cropleft,
            cropright,
            croptop,
            cropbottom,
            blend,
            mask_source,
            mask_dest,
            glow,
            fadeleft,
            faderight,
            fadetop,
            fadebottom,
            rot_z_deg,
            rot_x_deg,
            rot_y_deg,
            local_offset,
            local_offset_rot_sin_cos,
            texcoordvelocity,
            animate,
            state_delay,
            scale,
            shadow_len,
            shadow_color,
            effect,
        } => {
            if !*visible {
                return;
            }

            let arena_texture_key =
                if let actors::SpriteSource::ArenaTextureHandle { id, .. } = source {
                    let Some(textures) = actor_textures else {
                        debug_assert!(false, "arena texture actor composed without its arena");
                        return;
                    };
                    let Some(key) = textures.get(id.0 as usize) else {
                        debug_assert!(false, "arena texture actor has an invalid texture ID");
                        return;
                    };
                    Some(key.as_ref())
                } else {
                    None
                };
            let (is_solid, texture_name, texture_key_ptr) = match source {
                actors::SpriteSource::TextureStatic(name) => (false, *name, str_ptr(name)),
                actors::SpriteSource::TextureStaticHandle { key, .. } => {
                    (false, *key, str_ptr(key))
                }
                actors::SpriteSource::Texture(name) => {
                    let name = name.as_ref();
                    (false, name, str_ptr(name))
                }
                actors::SpriteSource::TextureHandle { key, .. } => {
                    let name = key.as_ref();
                    (false, name, str_ptr(name))
                }
                actors::SpriteSource::ArenaTextureHandle { .. } => {
                    let name = arena_texture_key.expect("arena texture key resolved above");
                    (false, name, str_ptr(name))
                }
                actors::SpriteSource::Solid => (true, "__white", str_ptr("__white")),
            };

            let mut chosen_cell = *cell;
            let mut chosen_grid = *grid;

            if !is_solid && uv_rect.is_none() {
                let (cols, rows) = grid.unwrap_or_else(|| {
                    texture_cache.sprite_sheet_dims(texture_ctx, texture_key_ptr, texture_name)
                });
                let total = cols.saturating_mul(rows).max(1);

                let start_linear: u32 = match *cell {
                    Some((cx, cy)) if cy != u32::MAX => {
                        let cx = cx.min(cols.saturating_sub(1));
                        let cy = cy.min(rows.saturating_sub(1));
                        cy.saturating_mul(cols).saturating_add(cx)
                    }
                    Some((i, _)) => i,
                    None => 0,
                };

                if *animate && *state_delay > 0.0 && total > 1 {
                    let steps = (total_elapsed / *state_delay).floor().max(0.0) as u32;
                    let idx = (start_linear + (steps % total)) % total;
                    chosen_cell = Some((idx, u32::MAX));
                    chosen_grid = Some((cols, rows));
                } else if chosen_cell.is_none() && total > 1 {
                    chosen_cell = Some((0, u32::MAX));
                    chosen_grid = Some((cols, rows));
                }
            }

            let mut effect_tint = *tint;
            let mut effect_scale = *scale;
            let mut effect_rot = [*rot_x_deg, *rot_y_deg, *rot_z_deg];
            apply_effect_to_sprite(
                *effect,
                total_elapsed,
                &mut effect_tint,
                &mut effect_scale,
                &mut effect_rot,
            );
            effect_tint = mul_rgba(effect_tint, style.tint);
            let actor_blend = style.blend.unwrap_or(*blend);

            let resolved_size = resolve_sprite_size_like_sm(
                *size,
                is_solid,
                texture_name,
                texture_key_ptr,
                *uv_rect,
                chosen_cell,
                chosen_grid,
                effect_scale,
                texture_cache,
                texture_ctx,
            );

            let rect = place_rect(parent, *align, *offset, resolved_size);
            let mask_rect = sm_rect_to_world_edges(rect, m);
            if *mask_source {
                masks.push(mask_rect);
            }
            if *mask_source && !*mask_dest {
                return;
            }
            if *mask_dest && masks.is_empty() {
                return;
            }

            let before = out.len();
            let before_sprite = sprite_instances.len();
            push_sprite(
                out,
                sprite_instances,
                camera,
                rect,
                m,
                is_solid,
                texture_name,
                texture_key_ptr,
                effect_tint,
                *uv_rect,
                chosen_cell,
                chosen_grid,
                *flip_x,
                *flip_y,
                *cropleft,
                *cropright,
                *croptop,
                *cropbottom,
                *fadeleft,
                *faderight,
                *fadetop,
                *fadebottom,
                actor_blend,
                effect_rot[0],
                effect_rot[1],
                effect_rot[2],
                *world_z,
                *local_offset,
                *local_offset_rot_sin_cos,
                *texcoordvelocity,
                sprite_source_handle(source, texture_cache.generation),
                texture_cache,
                texture_ctx,
                total_elapsed,
                false,
            );
            if *mask_dest {
                clip_objects_range_to_world_masks(
                    out,
                    sprite_instances,
                    before,
                    before_sprite,
                    masks,
                    &mut scratch.recycled_text_mesh_vertices,
                );
            }

            let end = out.len();
            let layer = base_z.saturating_add(*z);
            for obj in out.items.iter_mut().take(end).skip(before) {
                obj.z = layer;
                obj.order = {
                    let o = *order_counter;
                    *order_counter += 1;
                    o
                };
            }
            if has_shadow(*shadow_len) {
                push_shadow_objects_for_range(
                    out,
                    sprite_instances,
                    &mut scratch.recycled_text_mesh_vertices,
                    before,
                    end,
                    *shadow_len,
                    mul_rgba(*shadow_color, style.tint),
                );
            }
            if glow[3] > 0.0001 {
                let before = out.len();
                let before_sprite = sprite_instances.len();
                push_sprite(
                    out,
                    sprite_instances,
                    camera,
                    rect,
                    m,
                    is_solid,
                    texture_name,
                    texture_key_ptr,
                    mul_rgba(*glow, style.tint),
                    *uv_rect,
                    chosen_cell,
                    chosen_grid,
                    *flip_x,
                    *flip_y,
                    *cropleft,
                    *cropright,
                    *croptop,
                    *cropbottom,
                    *fadeleft,
                    *faderight,
                    *fadetop,
                    *fadebottom,
                    actor_blend,
                    effect_rot[0],
                    effect_rot[1],
                    effect_rot[2],
                    *world_z,
                    *local_offset,
                    *local_offset_rot_sin_cos,
                    *texcoordvelocity,
                    sprite_source_handle(source, texture_cache.generation),
                    texture_cache,
                    texture_ctx,
                    total_elapsed,
                    true,
                );
                if *mask_dest {
                    clip_objects_range_to_world_masks(
                        out,
                        sprite_instances,
                        before,
                        before_sprite,
                        masks,
                        &mut scratch.recycled_text_mesh_vertices,
                    );
                }
                let end = out.len();
                for index in before..end.min(out.len()) {
                    let obj = &mut out.items[index];
                    obj.z = layer;
                    obj.order = {
                        let o = *order_counter;
                        *order_counter += 1;
                        o
                    };
                }
            }
        }

        actors::Actor::Mesh {
            align,
            offset,
            size,
            tint,
            vertices,
            visible,
            blend,
            z,
        } => {
            if !*visible || vertices.is_empty() {
                return;
            }

            let rect = place_rect(parent, *align, *offset, *size);
            let base_x = m.left + rect.x;
            let base_y = m.top - rect.y;
            let transform = Matrix4::from_translation(Vector3::new(base_x, base_y, 0.0))
                * Matrix4::from_scale(Vector3::new(1.0, -1.0, 1.0));

            let order = *order_counter;
            *order_counter = order.saturating_add(1);
            out.push_mesh(
                renderer::INVALID_TEXTURE_HANDLE,
                order,
                base_z.saturating_add(*z),
                style.blend.unwrap_or(*blend),
                camera,
                MeshPayload {
                    transform,
                    tint: mul_rgba(style.tint, *tint),
                    vertices: MeshVertices::Shared(Arc::clone(vertices)),
                },
            );
        }

        actors::Actor::ReusableMesh {
            align,
            offset,
            size,
            tint,
            vertices,
            visible,
            blend,
            z,
        } => {
            if !*visible || vertices.is_empty() {
                return;
            }

            let rect = place_rect(parent, *align, *offset, *size);
            let base_x = m.left + rect.x;
            let base_y = m.top - rect.y;
            let transform = Matrix4::from_translation(Vector3::new(base_x, base_y, 0.0))
                * Matrix4::from_scale(Vector3::new(1.0, -1.0, 1.0));

            let order = *order_counter;
            *order_counter = order.saturating_add(1);
            out.push_mesh(
                renderer::INVALID_TEXTURE_HANDLE,
                order,
                base_z.saturating_add(*z),
                style.blend.unwrap_or(*blend),
                camera,
                MeshPayload {
                    transform,
                    tint: mul_rgba(style.tint, *tint),
                    vertices: MeshVertices::Reusable(Arc::clone(vertices)),
                },
            );
        }

        actors::Actor::TexturedMesh { .. } | actors::Actor::ReusableTexturedMesh { .. } => {
            unreachable!("textured meshes are composed before the general actor match")
        }

        actors::Actor::Shadow { len, color, child } => {
            let start = out.len();
            build_actor_recursive(
                child,
                parent,
                m,
                fonts,
                scratch,
                base_z,
                camera,
                style,
                cameras,
                masks,
                order_counter,
                out,
                sprite_instances,
                text_cache,
                texture_cache,
                texture_ctx,
                actor_textures,
                total_elapsed,
            );
            let end = out.len();
            if has_shadow(*len) {
                push_shadow_objects_for_range(
                    out,
                    sprite_instances,
                    &mut scratch.recycled_text_mesh_vertices,
                    start,
                    end,
                    *len,
                    mul_rgba(*color, style.tint),
                );
            }
        }

        actors::Actor::Camera {
            view_proj,
            children,
        } => {
            cameras.push(*view_proj);
            let id = cameras.len().saturating_sub(1).try_into().unwrap_or(0u8);
            build_actor_list(
                children,
                parent,
                m,
                fonts,
                scratch,
                base_z,
                id,
                style,
                cameras,
                masks,
                order_counter,
                out,
                sprite_instances,
                text_cache,
                texture_cache,
                texture_ctx,
                actor_textures,
                total_elapsed,
            );
        }

        actors::Actor::CameraPush { .. } | actors::Actor::CameraPop => {}

        actors::Actor::Text {
            align,
            offset,
            local_transform,
            color,
            stroke_color,
            font,
            content,
            attributes,
            align_text,
            z,
            scale,
            fit_width,
            fit_height,
            line_spacing,
            wrap_width_pixels,
            max_width,
            max_height,
            max_w_pre_zoom,
            max_h_pre_zoom,
            jitter,
            distortion,
            clip,
            mask_dest,
            blend,
            shadow_len,
            shadow_color,
            glow: _,
            effect,
        } => {
            if *mask_dest && masks.is_empty() {
                return;
            }
            if let Some(fm) = fonts.get(font) {
                let frame_inline = matches!(content, actors::TextContent::FrameInline { .. });
                let layout =
                    text_cache.get_or_build(fm, fonts, content, *wrap_width_pixels, *line_spacing);
                if layout.lines.is_empty() {
                    return;
                }
                let mut effect_color = *color;
                let mut effect_scale = *scale;
                apply_effect_to_text(*effect, total_elapsed, &mut effect_color, &mut effect_scale);
                effect_color = mul_rgba(effect_color, style.tint);
                let mut stroke_rgba = stroke_color
                    .map(|color| mul_rgba(color, style.tint))
                    .unwrap_or(fm.default_stroke_color);
                stroke_rgba[3] *= effect_color[3];
                let actor_blend = style.blend.unwrap_or(*blend);
                let needs_stroke = stroke_rgba[3] > 0.0 && !fm.stroke_texture_map.is_empty();
                let clip_world = (*clip).map(|[x, y, w, h]| {
                    sm_rect_to_world_edges(
                        SmRect {
                            x: parent.x + x,
                            y: parent.y + y,
                            w,
                            h,
                        },
                        m,
                    )
                });
                let before = out.len();
                let before_sprite = sprite_instances.len();
                let layer = base_z.saturating_add(*z);
                let end = if let Some(placement) = resolve_text_layout_placement(
                    layout,
                    effect_scale,
                    *fit_width,
                    *fit_height,
                    *max_width,
                    *max_height,
                    *max_w_pre_zoom,
                    *max_h_pre_zoom,
                    parent,
                    *align,
                    *offset,
                ) {
                    let text_distortion = distortion.max(0.0);
                    if !frame_inline && attributes.is_empty() && !*jitter && text_distortion <= 1e-6
                    {
                        push_text_mesh_batches(
                            out,
                            layout,
                            layout.fill_batches(*align_text),
                            &placement,
                            [1.0; 4],
                            *local_transform,
                            m,
                            texture_cache.generation,
                            texture_ctx,
                        );
                        if let Some(clip_world) = clip_world {
                            clip_objects_range_to_world_rect(
                                out,
                                sprite_instances,
                                before,
                                before_sprite,
                                clip_world,
                                &mut scratch.recycled_text_mesh_vertices,
                            );
                        }
                        if needs_stroke {
                            let stroke_start = out.len();
                            let stroke_start_sprite = sprite_instances.len();
                            push_text_mesh_batches(
                                out,
                                layout,
                                layout.stroke_batches(*align_text),
                                &placement,
                                stroke_rgba,
                                *local_transform,
                                m,
                                texture_cache.generation,
                                texture_ctx,
                            );
                            if let Some(clip_world) = clip_world {
                                clip_objects_range_to_world_rect(
                                    out,
                                    sprite_instances,
                                    stroke_start,
                                    stroke_start_sprite,
                                    clip_world,
                                    &mut scratch.recycled_text_mesh_vertices,
                                );
                            }
                            for obj in out.items.iter_mut().skip(stroke_start) {
                                obj.z = layer;
                                obj.order = {
                                    let o = *order_counter;
                                    *order_counter += 1;
                                    o
                                };
                                obj.blend = actor_blend;
                                obj.camera = camera;
                            }
                        }
                    } else {
                        let (builders, recycled_vertices) = scratch.transient_text_mesh_scratch();
                        build_transient_text_mesh_builders(
                            layout,
                            *align_text,
                            attributes.as_slice(),
                            jitter.then(|| (total_elapsed * 8.0).floor() as u32),
                            text_distortion,
                            false,
                            builders,
                            recycled_vertices,
                        );
                        push_transient_text_mesh_builders(
                            out,
                            layout,
                            builders,
                            &placement,
                            [1.0; 4],
                            *local_transform,
                            m,
                            texture_cache.generation,
                            texture_ctx,
                        );
                        if let Some(clip_world) = clip_world {
                            clip_objects_range_to_world_rect(
                                out,
                                sprite_instances,
                                before,
                                before_sprite,
                                clip_world,
                                recycled_vertices,
                            );
                        }
                        if needs_stroke {
                            let stroke_start = out.len();
                            let stroke_start_sprite = sprite_instances.len();
                            if frame_inline || text_distortion > 1e-6 {
                                build_transient_text_mesh_builders(
                                    layout,
                                    *align_text,
                                    &[],
                                    None,
                                    text_distortion,
                                    true,
                                    builders,
                                    recycled_vertices,
                                );
                                push_transient_text_mesh_builders(
                                    out,
                                    layout,
                                    builders,
                                    &placement,
                                    stroke_rgba,
                                    *local_transform,
                                    m,
                                    texture_cache.generation,
                                    texture_ctx,
                                );
                            } else {
                                push_text_mesh_batches(
                                    out,
                                    layout,
                                    layout.stroke_batches(*align_text),
                                    &placement,
                                    stroke_rgba,
                                    *local_transform,
                                    m,
                                    texture_cache.generation,
                                    texture_ctx,
                                );
                            }
                            if let Some(clip_world) = clip_world {
                                clip_objects_range_to_world_rect(
                                    out,
                                    sprite_instances,
                                    stroke_start,
                                    stroke_start_sprite,
                                    clip_world,
                                    recycled_vertices,
                                );
                            }
                            for obj in out.items.iter_mut().skip(stroke_start) {
                                obj.z = layer;
                                obj.order = {
                                    let o = *order_counter;
                                    *order_counter += 1;
                                    o
                                };
                                obj.blend = actor_blend;
                                obj.camera = camera;
                            }
                        }
                    }
                    out.len()
                } else {
                    before
                };
                if *mask_dest {
                    clip_objects_range_to_world_masks(
                        out,
                        sprite_instances,
                        before,
                        before_sprite,
                        masks,
                        &mut scratch.recycled_text_mesh_vertices,
                    );
                }
                for obj in out.items.iter_mut().take(end).skip(before) {
                    obj.z = layer;
                    obj.order = {
                        let o = *order_counter;
                        *order_counter += 1;
                        o
                    };
                    obj.blend = actor_blend;
                    obj.camera = camera;
                    if obj.kind == DrawKind::TexturedMesh {
                        let payload = out.textured_meshes[obj.payload_index as usize]
                            .as_mut()
                            .expect("draw item references live textured-mesh payload");
                        let instance = &mut payload.instance;
                        instance.tint[0] *= effect_color[0];
                        instance.tint[1] *= effect_color[1];
                        instance.tint[2] *= effect_color[2];
                        instance.tint[3] *= effect_color[3];
                    }
                }
                if has_shadow(*shadow_len) {
                    push_shadow_objects_for_range(
                        out,
                        sprite_instances,
                        &mut scratch.recycled_text_mesh_vertices,
                        before,
                        end,
                        *shadow_len,
                        mul_rgba(*shadow_color, style.tint),
                    );
                }
            }
        }

        actors::Actor::Frame {
            align,
            offset,
            size,
            children,
            background,
            z,
        } => {
            let rect = place_rect(parent, *align, *offset, *size);
            let layer = base_z.saturating_add(*z);

            if let Some(bg) = background {
                match bg {
                    actors::Background::Color(c) => {
                        let before = out.len();
                        push_sprite(
                            out,
                            sprite_instances,
                            camera,
                            rect,
                            m,
                            true,
                            "__white",
                            str_ptr("__white"),
                            *c,
                            None,
                            None,
                            None,
                            false,
                            false,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            BlendMode::Alpha,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            [0.0, 0.0],
                            [0.0, 1.0],
                            None,
                            None,
                            texture_cache,
                            texture_ctx,
                            total_elapsed,
                            false,
                        );
                        for obj in out.items.iter_mut().skip(before) {
                            obj.z = layer;
                            obj.order = {
                                let o = *order_counter;
                                *order_counter += 1;
                                o
                            };
                        }
                    }
                    actors::Background::Texture(tex) => {
                        let before = out.len();
                        push_sprite(
                            out,
                            sprite_instances,
                            camera,
                            rect,
                            m,
                            false,
                            tex,
                            str_ptr(tex),
                            [1.0; 4],
                            None,
                            None,
                            None,
                            false,
                            false,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            BlendMode::Alpha,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            [0.0, 0.0],
                            [0.0, 1.0],
                            None,
                            None,
                            texture_cache,
                            texture_ctx,
                            total_elapsed,
                            false,
                        );
                        for obj in out.items.iter_mut().skip(before) {
                            obj.z = layer;
                            obj.order = {
                                let o = *order_counter;
                                *order_counter += 1;
                                o
                            };
                        }
                    }
                }
            }

            build_actor_list(
                children,
                rect,
                m,
                fonts,
                scratch,
                layer,
                camera,
                style,
                cameras,
                masks,
                order_counter,
                out,
                sprite_instances,
                text_cache,
                texture_cache,
                texture_ctx,
                actor_textures,
                total_elapsed,
            );
        }

        actors::Actor::SharedFrame {
            align,
            offset,
            size,
            children,
            background,
            z,
            tint,
            blend,
        } => {
            let rect = place_rect(parent, *align, *offset, *size);
            let layer = base_z.saturating_add(*z);

            if let Some(bg) = background {
                match bg {
                    actors::Background::Color(c) => {
                        let before = out.len();
                        push_sprite(
                            out,
                            sprite_instances,
                            camera,
                            rect,
                            m,
                            true,
                            "__white",
                            str_ptr("__white"),
                            *c,
                            None,
                            None,
                            None,
                            false,
                            false,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            BlendMode::Alpha,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            [0.0, 0.0],
                            [0.0, 1.0],
                            None,
                            None,
                            texture_cache,
                            texture_ctx,
                            total_elapsed,
                            false,
                        );
                        for obj in out.items.iter_mut().skip(before) {
                            obj.z = layer;
                            obj.order = {
                                let o = *order_counter;
                                *order_counter += 1;
                                o
                            };
                        }
                    }
                    actors::Background::Texture(tex) => {
                        let before = out.len();
                        push_sprite(
                            out,
                            sprite_instances,
                            camera,
                            rect,
                            m,
                            false,
                            tex,
                            str_ptr(tex),
                            [1.0; 4],
                            None,
                            None,
                            None,
                            false,
                            false,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            BlendMode::Alpha,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            [0.0, 0.0],
                            [0.0, 1.0],
                            None,
                            None,
                            texture_cache,
                            texture_ctx,
                            total_elapsed,
                            false,
                        );
                        for obj in out.items.iter_mut().skip(before) {
                            obj.z = layer;
                            obj.order = {
                                let o = *order_counter;
                                *order_counter += 1;
                                o
                            };
                        }
                    }
                }
            }

            let child_style = style.child(*tint, *blend);
            build_actor_list(
                children,
                rect,
                m,
                fonts,
                scratch,
                layer,
                camera,
                child_style,
                cameras,
                masks,
                order_counter,
                out,
                sprite_instances,
                text_cache,
                texture_cache,
                texture_ctx,
                actor_textures,
                total_elapsed,
            );
        }

        actors::Actor::RetainedFrame {
            align,
            offset,
            size,
            frame,
            z,
            tint,
            blend,
            visible,
        } => {
            if !*visible {
                return;
            }
            let rect = place_rect(parent, *align, *offset, *size);
            let layer = base_z.saturating_add(*z);
            let child_style = style.child(*tint, *blend);
            let cacheable = camera == 0 && masks.is_empty();
            let key = retained_frame_key(frame.id(), rect, m, layer, child_style);

            if cacheable {
                let RetainedFrameCache { entries, stats } = &mut scratch.retained_frames;
                if let Some(cached) = entries.get(&key) {
                    stats.hits = stats.hits.saturating_add(1);
                    append_retained_frame(cached, order_counter, out, sprite_instances);
                    return;
                }
                stats.misses = stats.misses.saturating_add(1);
            }

            let object_start = out.len();
            let sprite_start = sprite_instances.len();
            let camera_start = cameras.len();
            let mask_start = masks.len();
            build_actor_list(
                frame.children(),
                rect,
                m,
                fonts,
                scratch,
                layer,
                camera,
                child_style,
                cameras,
                masks,
                order_counter,
                out,
                sprite_instances,
                text_cache,
                texture_cache,
                texture_ctx,
                actor_textures,
                total_elapsed,
            );

            if !cacheable || cameras.len() != camera_start || masks.len() != mask_start {
                return;
            }
            let Some(cached) =
                capture_retained_frame(out, sprite_instances, object_start, sprite_start)
            else {
                return;
            };
            let cache = &mut scratch.retained_frames;
            if cache.entries.len() < MAX_RETAINED_FRAME_ENTRIES {
                cache.entries.insert(key, cached);
            } else {
                cache.stats.saturated = cache.stats.saturated.saturating_add(1);
            }
        }
    }
}

/* ======================= LAYOUT HELPERS ======================= */

#[inline(always)]
fn resolve_sprite_size_like_sm<T: TextureContext + ?Sized>(
    size: [SizeSpec; 2],
    is_solid: bool,
    texture_name: &str,
    texture_key_ptr: *const str,
    _uv_rect: Option<[f32; 4]>,
    cell: Option<(u32, u32)>,
    grid: Option<(u32, u32)>,
    scale: [f32; 2],
    texture_cache: &mut TextureLookupCache,
    texture_ctx: &T,
) -> [SizeSpec; 2] {
    use SizeSpec::Px;

    #[inline(always)]
    fn native_dims<T: TextureContext + ?Sized>(
        is_solid: bool,
        texture_name: &str,
        texture_key_ptr: *const str,
        cell: Option<(u32, u32)>,
        grid: Option<(u32, u32)>,
        texture_cache: &mut TextureLookupCache,
        texture_ctx: &T,
    ) -> (f32, f32) {
        if is_solid {
            return (1.0, 1.0);
        }
        let Some(meta) = texture_cache.texture_dims(texture_ctx, texture_key_ptr, texture_name)
        else {
            return (0.0, 0.0);
        };
        let (mut tw, mut th) = (meta.w as f32, meta.h as f32);
        if cell.is_some() {
            let (gc, gr) = grid.unwrap_or_else(|| {
                texture_cache.sprite_sheet_dims(texture_ctx, texture_key_ptr, texture_name)
            });
            let cols = gc.max(1);
            let rows = gr.max(1);
            tw /= cols as f32;
            th /= rows as f32;
        }
        (tw, th)
    }

    let (w, h) = match (size[0], size[1]) {
        (Px(w), Px(h)) if w == 0.0 && h == 0.0 => (w, h),
        (Px(w), Px(h)) if w > 0.0 && h == 0.0 => (w, h),
        (Px(w), Px(h)) if w == 0.0 && h > 0.0 => (w, h),
        _ => return size,
    };

    let (nw, nh) = native_dims(
        is_solid,
        texture_name,
        texture_key_ptr,
        cell,
        grid,
        texture_cache,
        texture_ctx,
    );
    let aspect = if nw > 0.0 && nh > 0.0 { nh / nw } else { 1.0 };

    if w == 0.0 && h == 0.0 {
        [Px(nw * scale[0]), Px(nh * scale[1])]
    } else if h == 0.0 {
        [Px(w), Px(w * aspect)]
    } else {
        let inv_aspect = if aspect > 0.0 { 1.0 / aspect } else { 1.0 };
        [Px(h * inv_aspect), Px(h)]
    }
}

#[inline(always)]
fn place_rect(parent: SmRect, align: [f32; 2], offset: [f32; 2], size: [SizeSpec; 2]) -> SmRect {
    let w = match size[0] {
        SizeSpec::Px(w) => w,
        SizeSpec::Fill => parent.w,
    };
    let h = match size[1] {
        SizeSpec::Px(h) => h,
        SizeSpec::Fill => parent.h,
    };
    let rx = parent.x;
    let ry = parent.y;
    let ax = align[0];
    let ay = align[1];
    SmRect {
        x: ax.mul_add(-w, rx + offset[0]),
        y: ay.mul_add(-h, ry + offset[1]),
        w,
        h,
    }
}

#[inline(always)]
fn calculate_uvs<T: TextureContext + ?Sized>(
    texture: &str,
    texture_key_ptr: *const str,
    uv_rect: Option<[f32; 4]>,
    cell: Option<(u32, u32)>,
    grid: Option<(u32, u32)>,
    flip_x: bool,
    flip_y: bool,
    cl: f32,
    cr: f32,
    ct: f32,
    cb: f32,
    texcoordvelocity: Option<[f32; 2]>,
    texture_cache: &mut TextureLookupCache,
    texture_ctx: &T,
    total_elapsed: f32,
) -> ([f32; 2], [f32; 2]) {
    let (mut uv_scale, mut uv_offset) = if let Some([u0, v0, u1, v1]) = uv_rect {
        let du = (u1 - u0).abs().max(1e-6);
        let dv = (v1 - v0).abs().max(1e-6);
        ([du, dv], [u0.min(u1), v0.min(v1)])
    } else if let Some((cx, cy)) = cell {
        let (gc, gr) = grid.unwrap_or_else(|| {
            texture_cache.sprite_sheet_dims(texture_ctx, texture_key_ptr, texture)
        });
        let cols = gc.max(1);
        let rows = gr.max(1);
        let (col, row) = if cy == u32::MAX {
            let idx = cx;
            (idx % cols, (idx / cols).min(rows.saturating_sub(1)))
        } else {
            (
                cx.min(cols.saturating_sub(1)),
                cy.min(rows.saturating_sub(1)),
            )
        };
        let s = [1.0 / cols as f32, 1.0 / rows as f32];
        let o = [col as f32 * s[0], row as f32 * s[1]];
        (s, o)
    } else {
        ([1.0, 1.0], [0.0, 0.0])
    };

    uv_offset[0] += uv_scale[0] * cl;
    uv_offset[1] += uv_scale[1] * ct;
    uv_scale[0] *= (1.0 - cl - cr).max(0.0);
    uv_scale[1] *= (1.0 - ct - cb).max(0.0);

    if flip_x {
        uv_offset[0] += uv_scale[0];
        uv_scale[0] = -uv_scale[0];
    }
    if flip_y {
        uv_offset[1] += uv_scale[1];
        uv_scale[1] = -uv_scale[1];
    }

    if let Some(vel) = texcoordvelocity {
        uv_offset[0] += vel[0] * total_elapsed;
        uv_offset[1] += vel[1] * total_elapsed;
    }

    (uv_scale, uv_offset)
}

#[inline(always)]
fn fold_sprite_xy_rot(
    mut flip_x: bool,
    mut flip_y: bool,
    mut size_x: f32,
    mut size_y: f32,
    rot_x_deg: f32,
    rot_y_deg: f32,
) -> (bool, bool, f32, f32) {
    // Sprite instances only preserve 2D rotation in the fast path. Fold SM's
    // X/Y rotations into foreshortening plus texture flips so Y=180 mirrors
    // horizontally instead of becoming an accidental in-plane 180-degree turn.
    if rot_x_deg == 0.0 && rot_y_deg == 0.0 {
        return (flip_x, flip_y, size_x, size_y);
    }

    let cos_y = rot_y_deg.to_radians().cos();
    size_x *= cos_y.abs();
    if cos_y.is_sign_negative() {
        flip_x = !flip_x;
    }

    let cos_x = rot_x_deg.to_radians().cos();
    size_y *= cos_x.abs();
    if cos_x.is_sign_negative() {
        flip_y = !flip_y;
    }

    (flip_x, flip_y, size_x, size_y)
}

#[inline(always)]
fn push_sprite<T: TextureContext + ?Sized>(
    out: &mut FrameBuilder,
    sprite_instances: &mut Vec<renderer::SpriteInstanceRaw>,
    camera: u8,
    rect: SmRect,
    m: &Metrics,
    is_solid: bool,
    texture_id: &str,
    texture_key_ptr: *const str,
    tint: [f32; 4],
    uv_rect: Option<[f32; 4]>,
    cell: Option<(u32, u32)>,
    grid: Option<(u32, u32)>,
    flip_x: bool,
    flip_y: bool,
    cropleft: f32,
    cropright: f32,
    croptop: f32,
    cropbottom: f32,
    fadeleft: f32,
    faderight: f32,
    fadetop: f32,
    fadebottom: f32,
    blend: BlendMode,
    rot_x_deg: f32,
    rot_y_deg: f32,
    rot_z_deg: f32,
    world_z: f32,
    local_offset: [f32; 2],
    local_offset_rot_sin_cos: [f32; 2],
    texcoordvelocity: Option<[f32; 2]>,
    texture_handle: Option<renderer::TextureHandle>,
    texture_cache: &mut TextureLookupCache,
    texture_ctx: &T,
    total_elapsed: f32,
    texture_mask: bool,
) {
    if tint[3] <= 0.0 {
        return;
    }

    let (cl, cr, ct, cb) = clamp_crop_fractions(cropleft, cropright, croptop, cropbottom);

    let (base_center, base_size) = sm_rect_to_world_center_size(rect, m);
    if base_size.x <= 0.0 || base_size.y <= 0.0 {
        return;
    }

    let sx_crop = (1.0 - cl - cr).max(0.0);
    let sy_crop = (1.0 - ct - cb).max(0.0);
    if sx_crop <= 0.0 || sy_crop <= 0.0 {
        return;
    }

    // StepMania parity: crop shifts geometry toward the un-cropped side(s).
    // (This matches Sprite::DrawTexture(), which moves quad vertices instead of the actor.)
    let center_x = ((cl - cr) * base_size.x).mul_add(0.5, base_center.x);
    let center_y = ((cb - ct) * base_size.y).mul_add(0.5, base_center.y);
    let size_x = base_size.x * sx_crop;
    let size_y = base_size.y * sy_crop;
    let (uv_scale, uv_offset) = if is_solid {
        ([1.0, 1.0], [0.0, 0.0])
    } else {
        calculate_uvs(
            texture_id,
            texture_key_ptr,
            uv_rect,
            cell,
            grid,
            flip_x,
            flip_y,
            cl,
            cr,
            ct,
            cb,
            texcoordvelocity,
            texture_cache,
            texture_ctx,
            total_elapsed,
        )
    };

    let (flip_x, flip_y, size_x, size_y) =
        fold_sprite_xy_rot(flip_x, flip_y, size_x, size_y, rot_x_deg, rot_y_deg);
    let (sin_z, cos_z) = if rot_z_deg == 0.0 {
        (0.0, 1.0)
    } else {
        rot_z_deg.to_radians().sin_cos()
    };

    let fl = fadeleft.clamp(0.0, 1.0);
    let fr = faderight.clamp(0.0, 1.0);
    let ft = fadetop.clamp(0.0, 1.0);
    let fb = fadebottom.clamp(0.0, 1.0);

    // StepMania parity (Sprite::DrawPrimitives edge-fade behavior):
    // - Fade distances are specified in the *pre-crop* [0..1] space.
    // - Visible (post-crop) fraction is `(1 - crop_a - crop_b)`.
    // - Negative crop values can "cancel" fade (used by Simply Love transitions).
    let mut fl_size = (fl + cropleft.min(0.0)).max(0.0);
    let mut fr_size = (fr + cropright.min(0.0)).max(0.0);
    let mut ft_size = (ft + croptop.min(0.0)).max(0.0);
    let mut fb_size = (fb + cropbottom.min(0.0)).max(0.0);

    let sum_x = fl_size + fr_size;
    if sum_x > 0.0 && sx_crop < sum_x {
        let s = sx_crop / sum_x;
        fl_size *= s;
        fr_size *= s;
    }

    let sum_y = ft_size + fb_size;
    if sum_y > 0.0 && sy_crop < sum_y {
        let s = sy_crop / sum_y;
        ft_size *= s;
        fb_size *= s;
    }

    let mut fl_eff = (fl_size / sx_crop).clamp(0.0, 1.0);
    let mut fr_eff = (fr_size / sx_crop).clamp(0.0, 1.0);
    let mut ft_eff = (ft_size / sy_crop).clamp(0.0, 1.0);
    let mut fb_eff = (fb_size / sy_crop).clamp(0.0, 1.0);

    if flip_x {
        std::mem::swap(&mut fl_eff, &mut fr_eff);
    }
    if flip_y {
        std::mem::swap(&mut ft_eff, &mut fb_eff);
    }

    let texture_handle = match texture_handle {
        Some(handle) => handle,
        None => {
            let texture_key = if is_solid { "__white" } else { texture_id };
            texture_cache.texture_handle(texture_ctx, texture_key_ptr, texture_key)
        }
    };

    let sprite_index = sprite_instances.len() as u32;
    sprite_instances.push(renderer::SpriteInstanceRaw {
        center: [center_x, center_y, world_z, 0.0],
        size: [size_x, size_y],
        rot_sin_cos: [sin_z, cos_z],
        tint,
        uv_scale,
        uv_offset,
        local_offset,
        local_offset_rot_sin_cos,
        edge_fade: [fl_eff, fr_eff, ft_eff, fb_eff],
        texture_mask: texture_mask as u8 as f32,
    });

    out.push_sprite(texture_handle, 0, 0, blend, camera, sprite_index);
}

#[inline(always)]
#[must_use]
const fn clamp_crop_fractions(l: f32, r: f32, t: f32, b: f32) -> (f32, f32, f32, f32) {
    (
        l.clamp(0.0, 1.0),
        r.clamp(0.0, 1.0),
        t.clamp(0.0, 1.0),
        b.clamp(0.0, 1.0),
    )
}

#[inline(always)]
#[must_use]
fn lrint_ties_even(v: f32) -> f32 {
    if !v.is_finite() {
        return 0.0;
    }
    // Fast path: already an integer (including -0.0)
    if v.fract() == 0.0 {
        return v;
    }

    let floor = v.floor();
    let frac = v - floor;

    if frac < 0.5 {
        floor
    } else if frac > 0.5 {
        floor + 1.0
    } else {
        // frac == 0.5 exactly: ties-to-even
        // Use i64 for parity check to avoid edge overflow on extreme values.
        let f_even = ((floor as i64) & 1) == 0;
        if f_even { floor } else { floor + 1.0 }
    }
}

#[inline(always)]
#[must_use]
const fn quantize_up_even_i32(v: i32) -> i32 {
    if v <= 0 {
        0
    } else if (v & 1) != 0 {
        v + 1
    } else {
        v
    }
}

fn push_text_mesh_batches<T: TextureContext + ?Sized>(
    out: &mut FrameBuilder,
    layout: &CachedTextLayout,
    batches: &[CachedTextMeshBatch],
    placement: &TextLayoutPlacement,
    tint: [f32; 4],
    local_transform: Matrix4,
    m: &Metrics,
    texture_generation: u64,
    texture_ctx: &T,
) {
    if batches.is_empty() || tint[3] <= 0.0 {
        return;
    }

    let transform = Matrix4::from_translation(Vector3::new(
        m.left + placement.block_center_x,
        m.top - placement.block_center_y,
        0.0,
    )) * Matrix4::from_scale(Vector3::new(placement.sx, -placement.sy, 1.0))
        * local_transform;

    out.reserve(batches.len());
    for batch in batches {
        out.push_textured_mesh(
            layout
                .texture_page(batch.texture_page)
                .texture_handle(texture_generation, texture_ctx),
            0,
            0,
            BlendMode::Alpha,
            0,
            TexturedMeshPayload {
                instance: renderer::TexturedMeshInstanceRaw::new(
                    transform,
                    tint,
                    [1.0, 1.0],
                    [0.0, 0.0],
                    [0.0, 0.0],
                    false,
                ),
                vertices: renderer::TexturedMeshVertices::Shared(Arc::clone(&batch.vertices)),
                geom_cache_key: batch.geom_cache_key,
                depth_test: false,
            },
        );
    }
}

fn push_transient_text_mesh_builders<T: TextureContext + ?Sized>(
    out: &mut FrameBuilder,
    layout: &CachedTextLayout,
    builders: &mut Vec<TextMeshBatchBuilder>,
    placement: &TextLayoutPlacement,
    tint: [f32; 4],
    local_transform: Matrix4,
    m: &Metrics,
    texture_generation: u64,
    texture_ctx: &T,
) {
    if builders.is_empty() || tint[3] <= 0.0 {
        return;
    }

    let transform = Matrix4::from_translation(Vector3::new(
        m.left + placement.block_center_x,
        m.top - placement.block_center_y,
        0.0,
    )) * Matrix4::from_scale(Vector3::new(placement.sx, -placement.sy, 1.0))
        * local_transform;

    out.reserve(builders.len());
    for builder in builders.drain(..) {
        if builder.vertices.is_empty() {
            continue;
        }
        out.push_textured_mesh(
            layout
                .texture_page(builder.texture_page)
                .texture_handle(texture_generation, texture_ctx),
            0,
            0,
            BlendMode::Alpha,
            0,
            TexturedMeshPayload {
                instance: renderer::TexturedMeshInstanceRaw::new(
                    transform,
                    tint,
                    [1.0, 1.0],
                    [0.0, 0.0],
                    [0.0, 0.0],
                    false,
                ),
                vertices: renderer::TexturedMeshVertices::Transient(builder.vertices),
                geom_cache_key: renderer::INVALID_TMESH_CACHE_KEY,
                depth_test: false,
            },
        );
    }
}

#[inline(always)]
fn sm_rect_to_world_center_size(rect: SmRect, m: &Metrics) -> (Vector2, Vector2) {
    (
        Vector2::new(
            0.5f32.mul_add(rect.w, m.left + rect.x),
            m.top - 0.5f32.mul_add(rect.h, rect.y),
        ),
        Vector2::new(rect.w, rect.h),
    )
}

#[derive(Clone, Copy, Debug)]
struct WorldRect {
    left: f32,
    right: f32,
    bottom: f32,
    top: f32,
}

#[inline(always)]
fn sm_rect_to_world_edges(rect: SmRect, m: &Metrics) -> WorldRect {
    let left = m.left + rect.x;
    let right = rect.w.mul_add(1.0, left);

    let top = m.top - rect.y;
    let bottom = top - rect.h;

    WorldRect {
        left,
        right,
        bottom,
        top,
    }
}

fn clip_objects_range_to_world_masks(
    objects: &mut FrameBuilder,
    sprite_instances: &mut Vec<renderer::SpriteInstanceRaw>,
    start: usize,
    sprite_start: usize,
    masks: &[WorldRect],
    recycled_vertices: &mut Vec<Vec<renderer::TexturedMeshVertex>>,
) {
    if start >= objects.len() {
        return;
    }
    if masks.is_empty() {
        for index in start..objects.len() {
            let object = objects.take_object(index);
            recycle_transient_object_vertices(object.object_type, recycled_vertices);
        }
        objects.truncate(start);
        sprite_instances.truncate(sprite_start);
        return;
    }
    if let [mask] = masks {
        clip_objects_range_to_world_rect(
            objects,
            sprite_instances,
            start,
            sprite_start,
            *mask,
            recycled_vertices,
        );
        return;
    }
    let len = objects.len();
    let mut write = start;
    for read in start..len {
        let mut object = objects.take_object(read);
        let keep =
            clip_object_to_world_masks(&mut object, sprite_instances, masks, recycled_vertices);
        if keep {
            objects.replace_object(read, object);
            if write != read {
                objects.swap(write, read);
            }
            write += 1;
        } else {
            recycle_transient_object_vertices(object.object_type, recycled_vertices);
        }
    }
    objects.truncate(write);
    compact_sprite_instances_for_range(objects, start, sprite_instances, sprite_start);
}

struct ClippedSpriteObject {
    object_type: EditablePayload,
    sprite: Option<renderer::SpriteInstanceRaw>,
}

#[inline(always)]
fn object_world_area(
    clipped: &ClippedSpriteObject,
    sprite_instances: &[renderer::SpriteInstanceRaw],
) -> f32 {
    if let Some(sprite) = clipped.sprite {
        return (sprite.size[0] * sprite.size[1]).abs();
    }
    match &clipped.object_type {
        EditablePayload::Sprite(index) => {
            let sprite = sprite_instances[*index as usize];
            (sprite.size[0] * sprite.size[1]).abs()
        }
        EditablePayload::TexturedMesh {
            instance, vertices, ..
        } => {
            if vertices.len() < 3 {
                return 0.0;
            }
            let transform = instance.transform();
            let mut area = 0.0_f32;
            let mut i = 0usize;
            while i + 2 < vertices.len() {
                let p0 = world_xy_3d(&transform, vertices[i].pos);
                let p1 = world_xy_3d(&transform, vertices[i + 1].pos);
                let p2 = world_xy_3d(&transform, vertices[i + 2].pos);
                let a = (p1[0] - p0[0]) * (p2[1] - p0[1]) - (p1[1] - p0[1]) * (p2[0] - p0[0]);
                area += 0.5 * a.abs();
                i += 3;
            }
            area
        }
        EditablePayload::Mesh { .. } => 0.0,
    }
}

fn clip_object_to_world_masks(
    obj: &mut EditableDraw,
    sprite_instances: &mut [renderer::SpriteInstanceRaw],
    masks: &[WorldRect],
    recycled_vertices: &mut Vec<Vec<renderer::TexturedMeshVertex>>,
) -> bool {
    let mut best_obj: Option<ClippedSpriteObject> = None;
    let mut best_area = -1.0_f32;
    for &mask in masks {
        let Some(candidate) = clipped_sprite_object_to_world_rect(
            obj,
            sprite_instances,
            mask,
            Some(&mut *recycled_vertices),
            None,
        ) else {
            continue;
        };
        let area = object_world_area(&candidate, sprite_instances);
        if area > best_area {
            best_area = area;
            if let Some(previous) = best_obj.replace(candidate) {
                recycle_transient_object_vertices(previous.object_type, recycled_vertices);
            }
        } else {
            recycle_transient_object_vertices(candidate.object_type, recycled_vertices);
        }
    }
    if let Some(chosen) = best_obj {
        if let Some(sprite) = chosen.sprite
            && let EditablePayload::Sprite(index) = &chosen.object_type
        {
            sprite_instances[*index as usize] = sprite;
        }
        let source = std::mem::replace(&mut obj.object_type, chosen.object_type);
        recycle_transient_object_vertices(source, recycled_vertices);
        true
    } else {
        false
    }
}

fn recycle_transient_object_vertices(
    object_type: EditablePayload,
    recycled_vertices: &mut Vec<Vec<renderer::TexturedMeshVertex>>,
) {
    let EditablePayload::TexturedMesh {
        vertices: renderer::TexturedMeshVertices::Transient(mut vertices),
        ..
    } = object_type
    else {
        return;
    };
    if recycled_vertices.len() >= MAX_RECYCLED_TEXT_MESH_VERTEX_BUFFERS {
        return;
    }
    vertices.clear();
    recycled_vertices.push(vertices);
}

fn clip_objects_range_to_world_rect(
    objects: &mut FrameBuilder,
    sprite_instances: &mut Vec<renderer::SpriteInstanceRaw>,
    start: usize,
    sprite_start: usize,
    clip: WorldRect,
    recycled_vertices: &mut Vec<Vec<renderer::TexturedMeshVertex>>,
) {
    if start >= objects.len() {
        return;
    }
    if clip.left >= clip.right || clip.bottom >= clip.top {
        for index in start..objects.len() {
            let object = objects.take_object(index);
            recycle_transient_object_vertices(object.object_type, recycled_vertices);
        }
        objects.truncate(start);
        sprite_instances.truncate(sprite_start);
        return;
    }

    let len = objects.len();
    let mut write = start;
    for read in start..len {
        let mut object = objects.take_object(read);
        let keep = clip_sprite_object_to_world_rect_with_recycled(
            &mut object,
            sprite_instances,
            clip,
            Some(&mut *recycled_vertices),
        );
        if keep {
            objects.replace_object(read, object);
            if write != read {
                objects.swap(write, read);
            }
            write += 1;
        } else {
            recycle_transient_object_vertices(object.object_type, recycled_vertices);
        }
    }
    objects.truncate(write);
    compact_sprite_instances_for_range(objects, start, sprite_instances, sprite_start);
}

fn compact_sprite_instances_for_range(
    objects: &mut FrameBuilder,
    start: usize,
    sprite_instances: &mut Vec<renderer::SpriteInstanceRaw>,
    sprite_start: usize,
) {
    let mut write = sprite_start;
    for item in &mut objects.items[start..] {
        if item.kind != DrawKind::Sprite {
            continue;
        }
        let sprite = sprite_instances[item.payload_index as usize];
        item.payload_index = write as u32;
        if write < sprite_instances.len() {
            sprite_instances[write] = sprite;
        } else {
            sprite_instances.push(sprite);
        }
        write += 1;
    }
    sprite_instances.truncate(write);
}

#[cfg(test)]
fn clip_sprite_object_to_world_rect(
    obj: &mut EditableDraw,
    sprite_instances: &mut Vec<renderer::SpriteInstanceRaw>,
    clip: WorldRect,
) -> bool {
    clip_sprite_object_to_world_rect_with_recycled(obj, sprite_instances, clip, None)
}

fn clip_sprite_object_to_world_rect_with_recycled(
    obj: &mut EditableDraw,
    sprite_instances: &mut Vec<renderer::SpriteInstanceRaw>,
    clip: WorldRect,
    recycled_vertices: Option<&mut Vec<Vec<renderer::TexturedMeshVertex>>>,
) -> bool {
    if clip.left >= clip.right || clip.bottom >= clip.top {
        return false;
    }
    let mut textured_mesh_bounds = None;
    match &obj.object_type {
        EditablePayload::Mesh { .. } => return true,
        EditablePayload::TexturedMesh {
            instance, vertices, ..
        } => {
            let transform = instance.transform();
            let Some(bounds) = textured_mesh_world_bounds(vertices.as_ref(), transform) else {
                return false;
            };
            if bounds.right < clip.left
                || bounds.left > clip.right
                || bounds.top < clip.bottom
                || bounds.bottom > clip.top
            {
                return false;
            }
            if bounds.left >= clip.left
                && bounds.right <= clip.right
                && bounds.bottom >= clip.bottom
                && bounds.top <= clip.top
            {
                return true;
            }
            textured_mesh_bounds = Some(bounds);
        }
        EditablePayload::Sprite(_) => {}
    }

    let Some(clipped) = clipped_sprite_object_to_world_rect(
        obj,
        sprite_instances,
        clip,
        recycled_vertices,
        textured_mesh_bounds,
    ) else {
        return false;
    };
    if let Some(sprite) = clipped.sprite
        && let EditablePayload::Sprite(index) = &clipped.object_type
    {
        sprite_instances[*index as usize] = sprite;
    }
    obj.object_type = clipped.object_type;
    true
}

fn clipped_sprite_object_to_world_rect(
    obj: &EditableDraw,
    sprite_instances: &[renderer::SpriteInstanceRaw],
    clip: WorldRect,
    recycled_vertices: Option<&mut Vec<Vec<renderer::TexturedMeshVertex>>>,
    textured_mesh_bounds: Option<WorldRect>,
) -> Option<ClippedSpriteObject> {
    clipped_sprite_object_to_world_rect_impl::<true>(
        obj,
        sprite_instances,
        clip,
        recycled_vertices,
        textured_mesh_bounds,
    )
}

#[cfg(any(test, feature = "bench-support"))]
fn clipped_sprite_object_to_world_rect_legacy(
    obj: &EditableDraw,
    sprite_instances: &[renderer::SpriteInstanceRaw],
    clip: WorldRect,
    recycled_vertices: Option<&mut Vec<Vec<renderer::TexturedMeshVertex>>>,
    textured_mesh_bounds: Option<WorldRect>,
) -> Option<ClippedSpriteObject> {
    clipped_sprite_object_to_world_rect_impl::<false>(
        obj,
        sprite_instances,
        clip,
        recycled_vertices,
        textured_mesh_bounds,
    )
}

#[inline(always)]
fn clipped_sprite_object_to_world_rect_impl<const ACCEPT_CONTAINED_SPRITE: bool>(
    obj: &EditableDraw,
    sprite_instances: &[renderer::SpriteInstanceRaw],
    clip: WorldRect,
    mut recycled_vertices: Option<&mut Vec<Vec<renderer::TexturedMeshVertex>>>,
    textured_mesh_bounds: Option<WorldRect>,
) -> Option<ClippedSpriteObject> {
    if clip.left >= clip.right || clip.bottom >= clip.top {
        return None;
    }
    match &obj.object_type {
        EditablePayload::Sprite(index) => {
            let sprite = sprite_instances[*index as usize];
            let eps = 1e-6;
            let offset_world = [
                sprite.local_offset_rot_sin_cos[1].mul_add(
                    sprite.local_offset[0],
                    -(sprite.local_offset_rot_sin_cos[0] * sprite.local_offset[1]),
                ),
                sprite.local_offset_rot_sin_cos[0].mul_add(
                    sprite.local_offset[0],
                    sprite.local_offset_rot_sin_cos[1] * sprite.local_offset[1],
                ),
            ];
            let world_center = [
                sprite.center[0] + offset_world[0],
                sprite.center[1] + offset_world[1],
            ];
            if sprite.rot_sin_cos[0].abs() > eps || sprite.rot_sin_cos[1] < 1.0 - eps {
                return clip_rotated_sprite_to_world_rect(
                    sprite.tint,
                    sprite.center,
                    sprite.size,
                    sprite.rot_sin_cos,
                    sprite.uv_scale,
                    sprite.uv_offset,
                    offset_world,
                    clip,
                    sprite.texture_mask != 0.0,
                );
            }

            let w = sprite.size[0];
            let h = sprite.size[1];
            if w <= eps || h <= eps {
                return None;
            }

            let half_w = w * 0.5;
            let half_h = h * 0.5;

            let left = world_center[0] - half_w;
            let right = world_center[0] + half_w;
            let bottom = world_center[1] - half_h;
            let top = world_center[1] + half_h;

            if ACCEPT_CONTAINED_SPRITE
                && left >= clip.left
                && right <= clip.right
                && bottom >= clip.bottom
                && top <= clip.top
            {
                return Some(ClippedSpriteObject {
                    object_type: EditablePayload::Sprite(*index),
                    sprite: None,
                });
            }

            let inter_left = left.max(clip.left);
            let inter_right = right.min(clip.right);
            let inter_bottom = bottom.max(clip.bottom);
            let inter_top = top.min(clip.top);
            if inter_left >= inter_right || inter_bottom >= inter_top {
                return None;
            }

            let inv_w = 1.0 / w;
            let inv_h = 1.0 / h;

            let cl = ((inter_left - left) * inv_w).clamp(0.0, 1.0);
            let cr = ((right - inter_right) * inv_w).clamp(0.0, 1.0);
            let cb = ((inter_bottom - bottom) * inv_h).clamp(0.0, 1.0);
            let ct = ((top - inter_top) * inv_h).clamp(0.0, 1.0);

            let sx_crop = (1.0 - cl - cr).max(0.0);
            let sy_crop = (1.0 - ct - cb).max(0.0);
            if sx_crop <= eps || sy_crop <= eps {
                return None;
            }

            let uv_offset = [
                sprite.uv_offset[0] + sprite.uv_scale[0] * cl,
                sprite.uv_offset[1] + sprite.uv_scale[1] * ct,
            ];
            let uv_scale = [sprite.uv_scale[0] * sx_crop, sprite.uv_scale[1] * sy_crop];

            let center_x = ((cl - cr) * w).mul_add(0.5, world_center[0]) - offset_world[0];
            let center_y = ((cb - ct) * h).mul_add(0.5, world_center[1]) - offset_world[1];
            let new_w = w * sx_crop;
            let new_h = h * sy_crop;

            Some(ClippedSpriteObject {
                object_type: EditablePayload::Sprite(*index),
                sprite: Some(renderer::SpriteInstanceRaw {
                    center: [center_x, center_y, sprite.center[2], sprite.center[3]],
                    size: [new_w, new_h],
                    rot_sin_cos: sprite.rot_sin_cos,
                    tint: sprite.tint,
                    uv_scale,
                    uv_offset,
                    local_offset: sprite.local_offset,
                    local_offset_rot_sin_cos: sprite.local_offset_rot_sin_cos,
                    edge_fade: sprite.edge_fade,
                    texture_mask: sprite.texture_mask,
                }),
            })
        }
        EditablePayload::TexturedMesh {
            instance,
            vertices: mesh_vertices,
            geom_cache_key,
            depth_test,
        } => {
            let vertices = mesh_vertices.as_ref();
            let transform = instance.transform();
            let bounds = match textured_mesh_bounds {
                Some(bounds) => bounds,
                None => textured_mesh_world_bounds(vertices, transform)?,
            };
            if bounds.right < clip.left
                || bounds.left > clip.right
                || bounds.top < clip.bottom
                || bounds.bottom > clip.top
            {
                return None;
            }
            if bounds.left >= clip.left
                && bounds.right <= clip.right
                && bounds.bottom >= clip.bottom
                && bounds.top <= clip.top
            {
                let vertices = match mesh_vertices {
                    renderer::TexturedMeshVertices::Shared(vertices) => {
                        renderer::TexturedMeshVertices::Shared(Arc::clone(vertices))
                    }
                    renderer::TexturedMeshVertices::Reusable(vertices) => {
                        renderer::TexturedMeshVertices::Reusable(Arc::clone(vertices))
                    }
                    renderer::TexturedMeshVertices::Transient(vertices) => {
                        let mut cloned = recycled_vertices
                            .as_mut()
                            .map(|pool| take_recycled_text_mesh_vertices(pool))
                            .unwrap_or_default();
                        cloned.clear();
                        cloned.extend_from_slice(vertices);
                        renderer::TexturedMeshVertices::Transient(cloned)
                    }
                };
                return Some(ClippedSpriteObject {
                    object_type: EditablePayload::TexturedMesh {
                        instance: *instance,
                        vertices,
                        geom_cache_key: *geom_cache_key,
                        depth_test: *depth_test,
                    },
                    sprite: None,
                });
            }
            clip_textured_mesh_to_world_rect(
                instance.tint,
                vertices,
                transform,
                instance.uv_scale,
                instance.uv_offset,
                instance.uv_tex_shift,
                clip,
                instance.texture_mask != 0.0,
                recycled_vertices,
            )
        }
        EditablePayload::Mesh { .. } => Some(ClippedSpriteObject {
            object_type: obj.object_type.clone(),
            sprite: None,
        }),
    }
}

/// Mixed-payload retained-frame append benchmark support.
#[cfg(feature = "bench-support")]
pub struct RetainedAppendBenchmark {
    cached: CachedRetainedFrame,
    out: FrameBuilder,
    sprites: Vec<renderer::SpriteInstanceRaw>,
}

#[cfg(feature = "bench-support")]
impl RetainedAppendBenchmark {
    pub fn new(draw_count: usize) -> Self {
        let mesh_vertices: Arc<[renderer::MeshVertex]> =
            Arc::from([renderer::MeshVertex::default(); 6]);
        let tmesh_vertices: Arc<[renderer::TexturedMeshVertex]> =
            Arc::from([renderer::TexturedMeshVertex::default(); 6]);
        let mut builder = FrameBuilder::default();
        builder.reserve(draw_count);
        let mut sprites = Vec::with_capacity(draw_count.div_ceil(3));
        for index in 0..draw_count {
            match index % 3 {
                0 => {
                    let sprite = renderer::SpriteInstanceRaw {
                        center: [index as f32, 0.0, 0.0, 1.0],
                        size: [16.0; 2],
                        rot_sin_cos: [0.0, 1.0],
                        tint: [1.0; 4],
                        uv_scale: [1.0; 2],
                        uv_offset: [0.0; 2],
                        local_offset: [0.0; 2],
                        local_offset_rot_sin_cos: [0.0, 1.0],
                        edge_fade: [0.0; 4],
                        texture_mask: 0.0,
                    };
                    let sprite_index = saturating_u32(sprites.len());
                    sprites.push(sprite);
                    builder.push_sprite(
                        1,
                        0,
                        (index % 8) as i16,
                        BlendMode::Alpha,
                        0,
                        sprite_index,
                    );
                }
                1 => builder.push_mesh(
                    0,
                    0,
                    (index % 8) as i16,
                    BlendMode::Add,
                    0,
                    MeshPayload {
                        transform: Matrix4::IDENTITY,
                        tint: [1.0; 4],
                        vertices: MeshVertices::Shared(Arc::clone(&mesh_vertices)),
                    },
                ),
                _ => builder.push_textured_mesh(
                    2,
                    0,
                    (index % 8) as i16,
                    BlendMode::Alpha,
                    0,
                    TexturedMeshPayload {
                        instance: renderer::TexturedMeshInstanceRaw::new(
                            Matrix4::IDENTITY,
                            [1.0; 4],
                            [1.0; 2],
                            [0.0; 2],
                            [0.0; 2],
                            false,
                        ),
                        vertices: renderer::TexturedMeshVertices::Shared(Arc::clone(
                            &tmesh_vertices,
                        )),
                        geom_cache_key: index as u64 + 1,
                        depth_test: false,
                    },
                ),
            }
        }
        Self {
            cached: CachedRetainedFrame {
                builder,
                sprite_instances: sprites,
            },
            out: FrameBuilder::default(),
            sprites: Vec::new(),
        }
    }

    pub fn legacy_frame(&mut self) -> u64 {
        self.append(true)
    }

    pub fn bulk_frame(&mut self) -> u64 {
        self.append(false)
    }

    fn append(&mut self, legacy: bool) -> u64 {
        self.out.clear();
        self.sprites.clear();
        let mut order = 17;
        if legacy {
            append_retained_frame_legacy(
                &self.cached,
                &mut order,
                &mut self.out,
                &mut self.sprites,
            );
        } else {
            append_retained_frame(&self.cached, &mut order, &mut self.out, &mut self.sprites);
        }
        self.out.items.iter().fold(order as u64, |checksum, item| {
            checksum.rotate_left(7)
                ^ u64::from(item.order)
                ^ u64::from(item.payload_index).rotate_left(17)
                ^ item.texture_handle
        })
    }
}

/// Fragmented sprite-run finalization benchmark support.
#[cfg(feature = "bench-support")]
pub struct SpriteGatherBenchmark {
    items: Vec<DrawItem>,
    source_sprites: Vec<renderer::SpriteInstanceRaw>,
    builder: FrameBuilder,
    sprites: Vec<renderer::SpriteInstanceRaw>,
    gathered: Vec<renderer::SpriteInstanceRaw>,
    mesh_vertices: Vec<renderer::MeshVertex>,
    tmesh_instances: Vec<renderer::TexturedMeshInstanceRaw>,
    tmesh_geometries: Vec<renderer::TexturedMeshGeometry>,
    ops: Vec<renderer::DrawOp>,
    geom_map: HashMap<TMeshGeomKey, u32, rustc_hash::FxBuildHasher>,
    gather_stats: SpriteGatherStats,
}

#[cfg(feature = "bench-support")]
impl SpriteGatherBenchmark {
    pub fn new(sprite_count: usize, layers: usize) -> Self {
        let layers = layers.clamp(1, i16::MAX as usize);
        let mut items = Vec::with_capacity(sprite_count);
        let mut source_sprites = Vec::with_capacity(sprite_count);
        for index in 0..sprite_count {
            items.push(DrawItem {
                texture_handle: 1,
                order: index as u32,
                payload_index: index as u32,
                z: (index % layers) as i16,
                blend: BlendMode::Alpha,
                camera: 0,
                kind: DrawKind::Sprite,
            });
            source_sprites.push(renderer::SpriteInstanceRaw {
                center: [index as f32, 0.0, 0.0, 1.0],
                size: [16.0; 2],
                rot_sin_cos: [0.0, 1.0],
                tint: [1.0; 4],
                uv_scale: [1.0; 2],
                uv_offset: [0.0; 2],
                local_offset: [0.0; 2],
                local_offset_rot_sin_cos: [0.0, 1.0],
                edge_fade: [0.0; 4],
                texture_mask: 0.0,
            });
        }
        items.sort_unstable_by_key(|item| item.sort_key());
        Self {
            items,
            source_sprites,
            builder: FrameBuilder::default(),
            sprites: Vec::new(),
            gathered: Vec::new(),
            mesh_vertices: Vec::new(),
            tmesh_instances: Vec::new(),
            tmesh_geometries: Vec::new(),
            ops: Vec::new(),
            geom_map: HashMap::default(),
            gather_stats: SpriteGatherStats::default(),
        }
    }

    pub fn legacy_frame(&mut self) -> u64 {
        self.prepare();
        self.gather_stats = self.finalize::<false>();
        self.frame_checksum()
    }

    pub fn scanned_analysis_frame(&mut self) -> u64 {
        self.prepare();
        self.finalize::<false>();
        self.gather_stats = analyze_final_sprite_gather(&self.ops);
        self.analysis_checksum()
    }

    pub fn inline_analysis_frame(&mut self) -> u64 {
        self.prepare();
        self.gather_stats = self.finalize::<true>();
        self.analysis_checksum()
    }

    pub fn gathered_frame(&mut self) -> u64 {
        self.prepare();
        self.gather_stats = self.finalize::<true>();
        gather_finalized_sprites(
            &mut self.ops,
            &mut self.sprites,
            &mut self.gathered,
            self.gather_stats,
        );
        self.frame_checksum()
    }

    pub fn op_count(&self) -> usize {
        self.ops.len()
    }

    fn prepare(&mut self) {
        self.builder.clear();
        self.builder.items.extend_from_slice(&self.items);
        self.sprites.clear();
        self.sprites.extend_from_slice(&self.source_sprites);
        self.mesh_vertices.clear();
        self.tmesh_instances.clear();
        self.tmesh_geometries.clear();
        self.ops.clear();
        self.geom_map.clear();
    }

    fn finalize<const TRACK_SPRITE_RUNS: bool>(&mut self) -> SpriteGatherStats {
        finish_frame::<TRACK_SPRITE_RUNS>(
            &mut self.builder,
            &mut self.mesh_vertices,
            &mut self.tmesh_instances,
            &mut self.tmesh_geometries,
            &mut self.ops,
            &mut self.geom_map,
        )
    }

    fn analysis_checksum(&self) -> u64 {
        self.frame_checksum()
            ^ u64::from(self.gather_stats.sprites)
            ^ u64::from(self.gather_stats.runs_before).rotate_left(21)
            ^ u64::from(self.gather_stats.runs_after).rotate_left(42)
    }

    fn frame_checksum(&self) -> u64 {
        self.ops.iter().fold(0u64, |checksum, op| {
            let renderer::DrawOp::Sprite(run) = *op else {
                return checksum;
            };
            let instances = &self.sprites[run.instance_start as usize
                ..run.instance_start.saturating_add(run.instance_count) as usize];
            instances.iter().fold(checksum, |checksum, instance| {
                checksum.rotate_left(5)
                    ^ u64::from(instance.center[0].to_bits())
                    ^ run.texture_handle
            })
        })
    }
}

/// Gameplay-shaped fully contained sprite-clipping benchmark support.
#[cfg(feature = "bench-support")]
pub struct SpriteClipBenchmark {
    objects: Vec<EditableDraw>,
    sprite_instances: Vec<renderer::SpriteInstanceRaw>,
    clip: WorldRect,
}

#[cfg(feature = "bench-support")]
impl SpriteClipBenchmark {
    pub fn new(sprite_count: usize) -> Self {
        let mut objects = Vec::with_capacity(sprite_count);
        let mut sprite_instances = Vec::with_capacity(sprite_count);
        for index in 0..sprite_count {
            let column = index % 32;
            let row = index / 32;
            sprite_instances.push(renderer::SpriteInstanceRaw {
                center: [
                    32.0 + column as f32 * 18.0,
                    32.0 + row as f32 * 24.0,
                    0.0,
                    1.0,
                ],
                size: [12.0, 16.0],
                rot_sin_cos: [0.0, 1.0],
                tint: [1.0; 4],
                uv_scale: [1.0; 2],
                uv_offset: [0.0; 2],
                local_offset: [(index % 3) as f32 - 1.0, (index % 5) as f32 - 2.0],
                local_offset_rot_sin_cos: [0.0, 1.0],
                edge_fade: [0.0; 4],
                texture_mask: 0.0,
            });
            objects.push(EditableDraw {
                object_type: EditablePayload::Sprite(index as u32),
                texture_handle: 1,
                blend: BlendMode::Alpha,
                z: 0,
                order: index as u32,
                camera: 0,
            });
        }
        Self {
            objects,
            sprite_instances,
            clip: WorldRect {
                left: 0.0,
                right: 640.0,
                bottom: 0.0,
                top: 480.0,
            },
        }
    }

    pub fn clip_legacy_frame(&mut self) -> u64 {
        self.clip_frame::<false>()
    }

    pub fn clip_contained_frame(&mut self) -> u64 {
        self.clip_frame::<true>()
    }

    fn clip_frame<const ACCEPT_CONTAINED: bool>(&mut self) -> u64 {
        let mut checksum = self.objects.len() as u64;
        for object in &self.objects {
            let clipped = if ACCEPT_CONTAINED {
                clipped_sprite_object_to_world_rect(
                    object,
                    &self.sprite_instances,
                    self.clip,
                    None,
                    None,
                )
            } else {
                clipped_sprite_object_to_world_rect_legacy(
                    object,
                    &self.sprite_instances,
                    self.clip,
                    None,
                    None,
                )
            }
            .expect("benchmark sprites are contained by the clip");
            let EditablePayload::Sprite(index) = clipped.object_type else {
                unreachable!("axis-aligned sprite clipping keeps sprite geometry");
            };
            if let Some(sprite) = clipped.sprite {
                self.sprite_instances[index as usize] = sprite;
            }
            let sprite = self.sprite_instances[index as usize];
            checksum = checksum.rotate_left(5)
                ^ u64::from(sprite.center[0].to_bits())
                ^ (u64::from(sprite.center[1].to_bits()) << 32)
                ^ u64::from(sprite.size[0].to_bits())
                ^ u64::from(sprite.uv_scale[1].to_bits());
        }
        checksum
    }
}

#[derive(Clone, Copy)]
struct ClipVertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

#[inline(always)]
fn sprite_world_xy(
    center: [f32; 4],
    size: [f32; 2],
    rot_sin_cos: [f32; 2],
    offset_world: [f32; 2],
    p: [f32; 2],
) -> [f32; 2] {
    let local_x = p[0] * size[0];
    let local_y = p[1] * size[1];
    [
        rot_sin_cos[1].mul_add(
            local_x,
            (-rot_sin_cos[0] * local_y) + center[0] + offset_world[0],
        ),
        rot_sin_cos[0].mul_add(
            local_x,
            (rot_sin_cos[1] * local_y) + center[1] + offset_world[1],
        ),
    ]
}

#[inline(always)]
fn world_xy_3d(t: &Matrix4, p: [f32; 3]) -> [f32; 2] {
    let clip = *t * Vector4::new(p[0], p[1], p[2], 1.0);
    let inv_w = if clip.w.abs() > f32::EPSILON {
        clip.w.recip()
    } else {
        1.0
    };
    [clip.x * inv_w, clip.y * inv_w]
}

#[inline(always)]
fn is_affine_world_transform(t: &Matrix4) -> bool {
    t.x_axis.w == 0.0 && t.y_axis.w == 0.0 && t.z_axis.w == 0.0 && t.w_axis.w == 1.0
}

#[inline(always)]
fn affine_world_xy_3d(t: &Matrix4, p: [f32; 3]) -> [f32; 2] {
    [
        t.x_axis.x * p[0] + t.y_axis.x * p[1] + t.z_axis.x * p[2] + t.w_axis.x,
        t.x_axis.y * p[0] + t.y_axis.y * p[1] + t.z_axis.y * p[2] + t.w_axis.y,
    ]
}

fn textured_mesh_world_bounds(
    vertices: &[renderer::TexturedMeshVertex],
    transform: Matrix4,
) -> Option<WorldRect> {
    if is_affine_world_transform(&transform) {
        textured_mesh_world_bounds_with(vertices, |pos| affine_world_xy_3d(&transform, pos))
    } else {
        textured_mesh_world_bounds_with(vertices, |pos| world_xy_3d(&transform, pos))
    }
}

#[cfg(any(test, feature = "bench-support"))]
fn textured_mesh_world_bounds_legacy(
    vertices: &[renderer::TexturedMeshVertex],
    transform: Matrix4,
) -> Option<WorldRect> {
    textured_mesh_world_bounds_with(vertices, |pos| world_xy_3d(&transform, pos))
}

#[inline(always)]
fn textured_mesh_world_bounds_with(
    vertices: &[renderer::TexturedMeshVertex],
    mut world_xy: impl FnMut([f32; 3]) -> [f32; 2],
) -> Option<WorldRect> {
    let first = vertices.first()?;
    let first = world_xy(first.pos);
    let mut bounds = WorldRect {
        left: first[0],
        right: first[0],
        bottom: first[1],
        top: first[1],
    };
    for vertex in &vertices[1..] {
        let p = world_xy(vertex.pos);
        bounds.left = bounds.left.min(p[0]);
        bounds.right = bounds.right.max(p[0]);
        bounds.bottom = bounds.bottom.min(p[1]);
        bounds.top = bounds.top.max(p[1]);
    }
    Some(bounds)
}

#[inline(always)]
fn lerp_clip(a: ClipVertex, b: ClipVertex, t: f32) -> ClipVertex {
    let t = t.clamp(0.0, 1.0);
    ClipVertex {
        pos: [
            (b.pos[0] - a.pos[0]).mul_add(t, a.pos[0]),
            (b.pos[1] - a.pos[1]).mul_add(t, a.pos[1]),
        ],
        uv: [
            (b.uv[0] - a.uv[0]).mul_add(t, a.uv[0]),
            (b.uv[1] - a.uv[1]).mul_add(t, a.uv[1]),
        ],
        color: [
            (b.color[0] - a.color[0]).mul_add(t, a.color[0]),
            (b.color[1] - a.color[1]).mul_add(t, a.color[1]),
            (b.color[2] - a.color[2]).mul_add(t, a.color[2]),
            (b.color[3] - a.color[3]).mul_add(t, a.color[3]),
        ],
    }
}

fn clip_poly_edge_into(
    poly: &[ClipVertex],
    axis: usize,
    bound: f32,
    keep_greater: bool,
) -> ClipPolygon {
    let mut out = ClipPolygon::new();
    if poly.is_empty() {
        return out;
    }
    let mut prev = poly[poly.len() - 1];
    let mut prev_in = if keep_greater {
        prev.pos[axis] >= bound
    } else {
        prev.pos[axis] <= bound
    };

    for &curr in poly {
        let curr_in = if keep_greater {
            curr.pos[axis] >= bound
        } else {
            curr.pos[axis] <= bound
        };
        if prev_in && curr_in {
            out.push(curr);
        } else if prev_in && !curr_in {
            let denom = curr.pos[axis] - prev.pos[axis];
            if denom.abs() > 1e-6 {
                let t = (bound - prev.pos[axis]) / denom;
                out.push(lerp_clip(prev, curr, t));
            }
        } else if !prev_in && curr_in {
            let denom = curr.pos[axis] - prev.pos[axis];
            if denom.abs() > 1e-6 {
                let t = (bound - prev.pos[axis]) / denom;
                out.push(lerp_clip(prev, curr, t));
            }
            out.push(curr);
        }
        prev = curr;
        prev_in = curr_in;
    }
    out
}

fn clip_polygon_to_world_rect(poly: &[ClipVertex], clip: WorldRect) -> ClipPolygon {
    let mut p = clip_poly_edge_into(poly, 0, clip.left, true);
    p = clip_poly_edge_into(&p, 0, clip.right, false);
    p = clip_poly_edge_into(&p, 1, clip.bottom, true);
    clip_poly_edge_into(&p, 1, clip.top, false)
}

#[inline(always)]
fn baked_tmesh_uv(
    vertex: &renderer::TexturedMeshVertex,
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    uv_tex_shift: [f32; 2],
) -> [f32; 2] {
    [
        vertex.uv[0].mul_add(uv_scale[0], uv_offset[0])
            + uv_tex_shift[0] * (vertex.tex_matrix_scale[0] - 1.0),
        vertex.uv[1].mul_add(uv_scale[1], uv_offset[1])
            + uv_tex_shift[1] * (vertex.tex_matrix_scale[1] - 1.0),
    ]
}

#[inline(always)]
fn clipped_text_mesh_out<'a>(
    out: &'a mut Option<Vec<renderer::TexturedMeshVertex>>,
    recycled_vertices: &mut Option<&mut Vec<Vec<renderer::TexturedMeshVertex>>>,
    source_len: usize,
) -> &'a mut Vec<renderer::TexturedMeshVertex> {
    out.get_or_insert_with(|| {
        let mut vertices = recycled_vertices
            .take()
            .map(take_recycled_text_mesh_vertices)
            .unwrap_or_default();
        vertices.reserve(source_len.min(48));
        vertices
    })
}

fn clip_textured_mesh_to_world_rect(
    tint: [f32; 4],
    vertices: &[renderer::TexturedMeshVertex],
    transform: Matrix4,
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    uv_tex_shift: [f32; 2],
    clip: WorldRect,
    texture_mask: bool,
    recycled_vertices: Option<&mut Vec<Vec<renderer::TexturedMeshVertex>>>,
) -> Option<ClippedSpriteObject> {
    if is_affine_world_transform(&transform) {
        clip_textured_mesh_to_world_rect_with(
            tint,
            vertices,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            clip,
            texture_mask,
            recycled_vertices,
            |pos| affine_world_xy_3d(&transform, pos),
        )
    } else {
        clip_textured_mesh_to_world_rect_with(
            tint,
            vertices,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            clip,
            texture_mask,
            recycled_vertices,
            |pos| world_xy_3d(&transform, pos),
        )
    }
}

#[cfg(any(test, feature = "bench-support"))]
#[allow(clippy::too_many_arguments)]
fn clip_textured_mesh_to_world_rect_legacy(
    tint: [f32; 4],
    vertices: &[renderer::TexturedMeshVertex],
    transform: Matrix4,
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    uv_tex_shift: [f32; 2],
    clip: WorldRect,
    texture_mask: bool,
    recycled_vertices: Option<&mut Vec<Vec<renderer::TexturedMeshVertex>>>,
) -> Option<ClippedSpriteObject> {
    clip_textured_mesh_to_world_rect_with(
        tint,
        vertices,
        uv_scale,
        uv_offset,
        uv_tex_shift,
        clip,
        texture_mask,
        recycled_vertices,
        |pos| world_xy_3d(&transform, pos),
    )
}

#[inline(always)]
#[allow(clippy::too_many_arguments)]
fn clip_textured_mesh_to_world_rect_with(
    tint: [f32; 4],
    vertices: &[renderer::TexturedMeshVertex],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    uv_tex_shift: [f32; 2],
    clip: WorldRect,
    texture_mask: bool,
    mut recycled_vertices: Option<&mut Vec<Vec<renderer::TexturedMeshVertex>>>,
    mut world_xy: impl FnMut([f32; 3]) -> [f32; 2],
) -> Option<ClippedSpriteObject> {
    if vertices.len() < 3 {
        return None;
    }

    let mut out: Option<Vec<renderer::TexturedMeshVertex>> = None;
    for tri in vertices.chunks_exact(3) {
        let p0 = world_xy(tri[0].pos);
        let p1 = world_xy(tri[1].pos);
        let p2 = world_xy(tri[2].pos);
        let left = p0[0].min(p1[0]).min(p2[0]);
        let right = p0[0].max(p1[0]).max(p2[0]);
        let bottom = p0[1].min(p1[1]).min(p2[1]);
        let top = p0[1].max(p1[1]).max(p2[1]);
        if right < clip.left || left > clip.right || top < clip.bottom || bottom > clip.top {
            continue;
        }

        let uv0 = baked_tmesh_uv(&tri[0], uv_scale, uv_offset, uv_tex_shift);
        let uv1 = baked_tmesh_uv(&tri[1], uv_scale, uv_offset, uv_tex_shift);
        let uv2 = baked_tmesh_uv(&tri[2], uv_scale, uv_offset, uv_tex_shift);
        if left >= clip.left && right <= clip.right && bottom >= clip.bottom && top <= clip.top {
            let out = clipped_text_mesh_out(&mut out, &mut recycled_vertices, vertices.len());
            out.push(renderer::TexturedMeshVertex {
                pos: [p0[0], p0[1], 0.0],
                uv: uv0,
                tex_matrix_scale: [1.0, 1.0],
                color: tri[0].color,
            });
            out.push(renderer::TexturedMeshVertex {
                pos: [p1[0], p1[1], 0.0],
                uv: uv1,
                tex_matrix_scale: [1.0, 1.0],
                color: tri[1].color,
            });
            out.push(renderer::TexturedMeshVertex {
                pos: [p2[0], p2[1], 0.0],
                uv: uv2,
                tex_matrix_scale: [1.0, 1.0],
                color: tri[2].color,
            });
            continue;
        }

        let poly = [
            ClipVertex {
                pos: p0,
                uv: uv0,
                color: tri[0].color,
            },
            ClipVertex {
                pos: p1,
                uv: uv1,
                color: tri[1].color,
            },
            ClipVertex {
                pos: p2,
                uv: uv2,
                color: tri[2].color,
            },
        ];
        let clipped = clip_polygon_to_world_rect(&poly, clip);
        if clipped.len() < 3 {
            continue;
        }
        let out = clipped_text_mesh_out(&mut out, &mut recycled_vertices, vertices.len());

        let base = clipped[0];
        let mut i = 1usize;
        while i + 1 < clipped.len() {
            for vertex in [base, clipped[i], clipped[i + 1]] {
                out.push(renderer::TexturedMeshVertex {
                    pos: [vertex.pos[0], vertex.pos[1], 0.0],
                    uv: vertex.uv,
                    tex_matrix_scale: [1.0, 1.0],
                    color: vertex.color,
                });
            }
            i += 1;
        }
    }

    let out = out?;
    if out.is_empty() {
        return None;
    }

    Some(ClippedSpriteObject {
        object_type: EditablePayload::TexturedMesh {
            instance: renderer::TexturedMeshInstanceRaw::new(
                Matrix4::IDENTITY,
                tint,
                [1.0, 1.0],
                [0.0, 0.0],
                [0.0, 0.0],
                texture_mask,
            ),
            vertices: renderer::TexturedMeshVertices::Transient(out),
            geom_cache_key: renderer::INVALID_TMESH_CACHE_KEY,
            depth_test: false,
        },
        sprite: None,
    })
}

/// Gameplay-shaped partially clipped text-mesh transform benchmark support.
#[cfg(feature = "bench-support")]
pub struct TexturedMeshClipBenchmark {
    vertices: Vec<renderer::TexturedMeshVertex>,
    transform: Matrix4,
    clip: WorldRect,
    recycled_vertices: Vec<Vec<renderer::TexturedMeshVertex>>,
}

#[cfg(feature = "bench-support")]
impl TexturedMeshClipBenchmark {
    pub fn new(glyphs: usize) -> Self {
        let mut vertices = Vec::with_capacity(glyphs.saturating_mul(6));
        for glyph in 0..glyphs {
            let left = glyph as f32 * 12.0;
            let right = left + 12.0;
            let bottom = 0.0;
            let top = 32.0;
            for (pos, uv) in [
                ([left, bottom, 0.0], [0.0, 1.0]),
                ([right, bottom, 0.0], [1.0, 1.0]),
                ([right, top, 0.0], [1.0, 0.0]),
                ([left, bottom, 0.0], [0.0, 1.0]),
                ([right, top, 0.0], [1.0, 0.0]),
                ([left, top, 0.0], [0.0, 0.0]),
            ] {
                vertices.push(renderer::TexturedMeshVertex {
                    pos,
                    uv,
                    color: [1.0; 4],
                    tex_matrix_scale: [1.0; 2],
                });
            }
        }
        Self {
            vertices,
            transform: Matrix4::from_translation(Vector3::new(8.0, 36.0, 0.0))
                * Matrix4::from_scale(Vector3::new(1.0, -1.0, 1.0)),
            clip: WorldRect {
                left: 8.0,
                right: 328.0,
                bottom: 12.0,
                top: 36.0,
            },
            recycled_vertices: Vec::with_capacity(1),
        }
    }

    pub fn clip_legacy_frame(&mut self) -> u64 {
        self.clip_frame(true)
    }

    pub fn clip_affine_frame(&mut self) -> u64 {
        self.clip_frame(false)
    }

    fn clip_frame(&mut self, legacy: bool) -> u64 {
        let bounds = if legacy {
            textured_mesh_world_bounds_legacy(&self.vertices, self.transform)
        } else {
            textured_mesh_world_bounds(&self.vertices, self.transform)
        }
        .expect("benchmark mesh is not empty");
        let clipped = if legacy {
            clip_textured_mesh_to_world_rect_legacy(
                [1.0; 4],
                &self.vertices,
                self.transform,
                [1.0; 2],
                [0.0; 2],
                [0.0; 2],
                self.clip,
                false,
                Some(&mut self.recycled_vertices),
            )
        } else {
            clip_textured_mesh_to_world_rect(
                [1.0; 4],
                &self.vertices,
                self.transform,
                [1.0; 2],
                [0.0; 2],
                [0.0; 2],
                self.clip,
                false,
                Some(&mut self.recycled_vertices),
            )
        }
        .expect("benchmark mesh intersects the clip");
        let EditablePayload::TexturedMesh {
            vertices: renderer::TexturedMeshVertices::Transient(mut vertices),
            ..
        } = clipped.object_type
        else {
            unreachable!("textured mesh clipping returns transient mesh geometry");
        };
        let checksum = (vertices.len() as u64)
            ^ (u64::from(bounds.left.to_bits()) << 32)
            ^ u64::from(bounds.right.to_bits());
        vertices.clear();
        self.recycled_vertices.push(vertices);
        checksum
    }
}

fn clip_rotated_sprite_to_world_rect(
    tint: [f32; 4],
    center: [f32; 4],
    size: [f32; 2],
    rot_sin_cos: [f32; 2],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    offset_world: [f32; 2],
    clip: WorldRect,
    texture_mask: bool,
) -> Option<ClippedSpriteObject> {
    let poly = [
        ClipVertex {
            pos: sprite_world_xy(
                center,
                size,
                rot_sin_cos,
                offset_world,
                [-0.5_f32, -0.5_f32],
            ),
            uv: [uv_offset[0], uv_offset[1] + uv_scale[1]],
            color: [1.0; 4],
        },
        ClipVertex {
            pos: sprite_world_xy(center, size, rot_sin_cos, offset_world, [0.5_f32, -0.5_f32]),
            uv: [uv_offset[0] + uv_scale[0], uv_offset[1] + uv_scale[1]],
            color: [1.0; 4],
        },
        ClipVertex {
            pos: sprite_world_xy(center, size, rot_sin_cos, offset_world, [0.5_f32, 0.5_f32]),
            uv: [uv_offset[0] + uv_scale[0], uv_offset[1]],
            color: [1.0; 4],
        },
        ClipVertex {
            pos: sprite_world_xy(center, size, rot_sin_cos, offset_world, [-0.5_f32, 0.5_f32]),
            uv: [uv_offset[0], uv_offset[1]],
            color: [1.0; 4],
        },
    ];
    let clipped = clip_polygon_to_world_rect(&poly, clip);
    if clipped.len() < 3 {
        return None;
    }

    let mut out = ClippedMesh::new();
    let base = clipped[0];
    let mut i = 1usize;
    while i + 1 < clipped.len() {
        for v in [base, clipped[i], clipped[i + 1]] {
            out.push(renderer::TexturedMeshVertex {
                pos: [v.pos[0], v.pos[1], 0.0],
                uv: v.uv,
                tex_matrix_scale: [1.0, 1.0],
                color: v.color,
            });
        }
        i += 1;
    }

    Some(ClippedSpriteObject {
        object_type: EditablePayload::TexturedMesh {
            instance: renderer::TexturedMeshInstanceRaw::new(
                Matrix4::IDENTITY,
                tint,
                [1.0, 1.0],
                [0.0, 0.0],
                [0.0, 0.0],
                texture_mask,
            ),
            vertices: renderer::TexturedMeshVertices::Transient(out.into_vec()),
            geom_cache_key: renderer::INVALID_TMESH_CACHE_KEY,
            depth_test: false,
        },
        sprite: None,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        ActorSegment, CachedGlyph, CachedRetainedFrame, CachedTextLayout, CachedTextMeshBatch,
        CachedTextMeshVariants, CachedTextPage, ComposeScratch, DrawItem, EditableDraw,
        EditablePayload, FrameBuilder, MeshPayload, MeshVertices, TextAttrCursor, TextLayoutCache,
        TextLayoutKey, TextMeshBatchBuilder, TextPageId, TextureCacheEntry, TextureContext,
        TextureLookupCache, TextureMeta, TexturedMeshPayload, WorldRect,
        analyze_final_sprite_gather, append_retained_frame, append_retained_frame_legacy,
        build_cached_text_layout, build_screen_cached_with_scratch_and_texture_context,
        build_screen_cached_with_scratch_and_texture_context_and_actor_resources,
        build_screen_segments_cached_with_scratch_and_texture_context_and_actor_resources,
        build_transient_text_mesh_builders, clip_object_to_world_masks,
        clip_sprite_object_to_world_rect, clip_textured_mesh_to_world_rect,
        clip_textured_mesh_to_world_rect_legacy, clipped_sprite_object_to_world_rect,
        clipped_sprite_object_to_world_rect_legacy, finish_frame, fold_sprite_xy_rot,
        font_chain_key, gather_finalized_sprites, is_affine_world_transform,
        prewarm_frame_inline_text_slot, push_shadow_objects_for_range, resolve_sprite_size_like_sm,
        sort_composed_draw_items, sort_draw_items, sort_draw_items_legacy, str_ptr,
        textured_mesh_world_bounds, textured_mesh_world_bounds_legacy, wrap_text_lines_by_words,
    };
    use crate::actors::{
        Actor, ActorResourceArena, InlineText, RetainedActorFrame, SizeSpec, SpriteSource,
        TextAlign, TextAttribute, TextAttributes, TextContent,
    };
    use crate::font;
    use crate::font::{Font, Glyph};
    use crate::space::Metrics;
    use deadlib_render::{
        BlendMode, DrawOp, INVALID_TEXTURE_HANDLE, INVALID_TMESH_CACHE_KEY, MeshRun, MeshVertex,
        RenderFrame, SpriteInstanceRaw, SpriteRun, TMeshCacheKey, TexturedMeshGeometry,
        TexturedMeshInstanceRaw, TexturedMeshRun, TexturedMeshVertex, TexturedMeshVertices,
    };
    use glam::{Mat4 as Matrix4, Vec3 as Vector3};
    use std::cell::Cell;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;

    #[derive(Clone, Copy)]
    struct TestDrawTextureContext;

    impl TextureContext for TestDrawTextureContext {
        fn texture_registry_generation(&self) -> u64 {
            1
        }

        fn texture_dims(&self, _key: &str) -> Option<TextureMeta> {
            None
        }

        fn sprite_sheet_dims(&self, key: &str) -> (u32, u32) {
            crate::font::parse_sprite_sheet_dims_from_key(key)
        }

        fn texture_handle(&self, key: &str) -> deadlib_render::TextureHandle {
            key.bytes()
                .fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
                    (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3)
                })
                .max(1)
        }
    }

    fn build_screen(
        actors: &[Actor],
        clear_color: [f32; 4],
        metrics: &Metrics,
        fonts: &font::FontMap,
        total_elapsed: f32,
    ) -> RenderFrame {
        super::build_screen_with_texture_context(
            actors,
            clear_color,
            metrics,
            fonts,
            total_elapsed,
            &TestDrawTextureContext,
        )
    }

    fn build_screen_cached_with_scratch(
        actors: &[Actor],
        clear_color: [f32; 4],
        metrics: &Metrics,
        fonts: &font::FontMap,
        total_elapsed: f32,
        text_cache: &mut TextLayoutCache,
        scratch: &mut ComposeScratch,
    ) -> RenderFrame {
        build_screen_cached_with_scratch_and_texture_context(
            actors,
            clear_color,
            metrics,
            fonts,
            total_elapsed,
            text_cache,
            scratch,
            &TestDrawTextureContext,
        )
    }

    fn test_sprite_instance(x: f32) -> SpriteInstanceRaw {
        SpriteInstanceRaw {
            center: [x, 0.0, 0.0, 1.0],
            size: [1.0; 2],
            rot_sin_cos: [0.0, 1.0],
            tint: [1.0; 4],
            uv_scale: [1.0; 2],
            uv_offset: [0.0; 2],
            local_offset: [0.0; 2],
            local_offset_rot_sin_cos: [0.0, 1.0],
            edge_fade: [0.0; 4],
            texture_mask: 0.0,
        }
    }

    fn finish_test_builder(
        builder: FrameBuilder,
        sprite_instances: Vec<SpriteInstanceRaw>,
    ) -> RenderFrame {
        finish_test_builder_with_stats::<false>(builder, sprite_instances).0
    }

    fn finish_test_builder_with_stats<const TRACK_SPRITE_RUNS: bool>(
        mut builder: FrameBuilder,
        sprite_instances: Vec<SpriteInstanceRaw>,
    ) -> (RenderFrame, super::SpriteGatherStats) {
        let mut mesh_vertices = Vec::new();
        let mut tmesh_instances = Vec::new();
        let mut tmesh_geometries = Vec::new();
        let mut ops = Vec::new();
        let mut geom_map = HashMap::default();
        let stats = finish_frame::<TRACK_SPRITE_RUNS>(
            &mut builder,
            &mut mesh_vertices,
            &mut tmesh_instances,
            &mut tmesh_geometries,
            &mut ops,
            &mut geom_map,
        );
        (
            RenderFrame {
                clear_color: [0.0; 4],
                cameras: vec![Matrix4::IDENTITY],
                sprite_instances,
                mesh_vertices,
                tmesh_instances,
                tmesh_geometries,
                ops,
            },
            stats,
        )
    }

    fn assert_test_frames_equal(expected: &RenderFrame, actual: &RenderFrame) {
        assert_eq!(expected.clear_color, actual.clear_color);
        assert_eq!(expected.cameras, actual.cameras);
        assert_eq!(expected.sprite_instances, actual.sprite_instances);
        assert_eq!(expected.mesh_vertices, actual.mesh_vertices);
        assert_eq!(expected.tmesh_instances, actual.tmesh_instances);
        assert_eq!(
            expected.tmesh_geometries.len(),
            actual.tmesh_geometries.len()
        );
        for (expected, actual) in expected
            .tmesh_geometries
            .iter()
            .zip(&actual.tmesh_geometries)
        {
            assert_eq!(expected.cache_key, actual.cache_key);
            assert_eq!(expected.vertices.as_ref(), actual.vertices.as_ref());
        }
        assert_eq!(expected.ops, actual.ops);
    }

    fn sprite_painter_stream(
        frame: &RenderFrame,
    ) -> Vec<(
        BlendMode,
        deadlib_render::TextureHandle,
        u8,
        SpriteInstanceRaw,
    )> {
        let mut stream = Vec::new();
        for op in &frame.ops {
            let DrawOp::Sprite(run) = *op else {
                panic!("sprite-only test frame contains a non-sprite operation");
            };
            for index in run.instance_start..run.instance_start + run.instance_count {
                stream.push((
                    run.blend,
                    run.texture_handle,
                    run.camera,
                    frame.sprite_instances[index as usize],
                ));
            }
        }
        stream
    }

    #[test]
    fn retained_bulk_splice_matches_per_object_append() {
        let mesh_vertices: Arc<[MeshVertex]> = Arc::from([MeshVertex::default(); 3]);
        let tmesh_vertices: Arc<[TexturedMeshVertex]> =
            Arc::from([TexturedMeshVertex::default(); 3]);
        let mut cached_builder = FrameBuilder::default();
        cached_builder.push_sprite(7, 0, 2, BlendMode::Alpha, 0, 0);
        cached_builder.push_mesh(
            0,
            0,
            3,
            BlendMode::Add,
            0,
            MeshPayload {
                transform: Matrix4::IDENTITY,
                tint: [1.0; 4],
                vertices: MeshVertices::Shared(mesh_vertices),
            },
        );
        cached_builder.push_textured_mesh(
            8,
            0,
            4,
            BlendMode::Alpha,
            0,
            TexturedMeshPayload {
                instance: TexturedMeshInstanceRaw::new(
                    Matrix4::IDENTITY,
                    [1.0; 4],
                    [1.0; 2],
                    [0.0; 2],
                    [0.0; 2],
                    false,
                ),
                vertices: TexturedMeshVertices::Shared(tmesh_vertices),
                geom_cache_key: 41,
                depth_test: false,
            },
        );
        let cached = CachedRetainedFrame {
            builder: cached_builder,
            sprite_instances: vec![test_sprite_instance(4.0)],
        };

        let mut legacy_builder = FrameBuilder::default();
        let mut bulk_builder = FrameBuilder::default();
        let mut legacy_sprites = vec![test_sprite_instance(-1.0)];
        let mut bulk_sprites = legacy_sprites.clone();
        let mut legacy_order = 7;
        let mut bulk_order = 7;
        append_retained_frame_legacy(
            &cached,
            &mut legacy_order,
            &mut legacy_builder,
            &mut legacy_sprites,
        );
        append_retained_frame(
            &cached,
            &mut bulk_order,
            &mut bulk_builder,
            &mut bulk_sprites,
        );

        assert_eq!(legacy_order, bulk_order);
        let legacy = finish_test_builder(legacy_builder, legacy_sprites);
        let bulk = finish_test_builder(bulk_builder, bulk_sprites);
        assert_test_frames_equal(&legacy, &bulk);
    }

    #[test]
    fn fallback_sprite_gather_coalesces_without_changing_painter_output() {
        let sprites = vec![
            test_sprite_instance(0.0),
            test_sprite_instance(1.0),
            test_sprite_instance(2.0),
            test_sprite_instance(3.0),
        ];
        let make_builder = || {
            let mut builder = FrameBuilder::default();
            for (order, (index, z)) in [(0, 0), (2, 0), (1, 1), (3, 1)].into_iter().enumerate() {
                builder.push_sprite(7, order as u32, z, BlendMode::Alpha, 0, index);
            }
            builder
        };

        let legacy = finish_test_builder(make_builder(), sprites.clone());
        let (mut gathered, inline_gather) =
            finish_test_builder_with_stats::<true>(make_builder(), sprites);
        let mut gather_scratch = Vec::new();
        let scanned_gather = analyze_final_sprite_gather(&gathered.ops);
        assert_eq!(scanned_gather, inline_gather);
        gather_finalized_sprites(
            &mut gathered.ops,
            &mut gathered.sprite_instances,
            &mut gather_scratch,
            inline_gather,
        );

        assert_eq!(legacy.ops.len(), 4);
        assert_eq!(gathered.ops.len(), 1);
        assert_eq!(inline_gather.sprites, 4);
        assert_eq!(inline_gather.runs_before, 4);
        assert_eq!(inline_gather.runs_after, 1);
        assert_ne!(legacy.sprite_instances, gathered.sprite_instances);
        assert_eq!(
            sprite_painter_stream(&legacy),
            sprite_painter_stream(&gathered)
        );
    }

    #[test]
    fn inline_sprite_analysis_matches_second_pass_across_mixed_draws() {
        let mut builder = FrameBuilder::default();
        builder.push_sprite(7, 0, 0, BlendMode::Alpha, 0, 0);
        builder.push_sprite(INVALID_TEXTURE_HANDLE, 1, 0, BlendMode::Alpha, 0, 1);
        builder.push_sprite(7, 2, 0, BlendMode::Alpha, 0, 2);
        builder.push_mesh(
            0,
            3,
            0,
            BlendMode::Alpha,
            0,
            MeshPayload {
                transform: Matrix4::IDENTITY,
                tint: [1.0; 4],
                vertices: MeshVertices::Shared(Arc::from([MeshVertex::default(); 3])),
            },
        );
        builder.push_sprite(7, 4, 0, BlendMode::Alpha, 0, 3);

        let (frame, inline) = finish_test_builder_with_stats::<true>(
            builder,
            (0..4)
                .map(|index| test_sprite_instance(index as f32))
                .collect(),
        );
        assert_eq!(inline, analyze_final_sprite_gather(&frame.ops));
        assert_eq!(inline.sprites, 3);
        assert_eq!(inline.runs_before, 3);
        assert_eq!(inline.runs_after, 2);
    }

    fn sprite_run(frame: &RenderFrame, index: usize) -> SpriteRun {
        let DrawOp::Sprite(run) = frame.ops[index] else {
            panic!("expected sprite draw operation");
        };
        run
    }

    fn mesh_run(frame: &RenderFrame, index: usize) -> MeshRun {
        let DrawOp::Mesh(run) = frame.ops[index] else {
            panic!("expected mesh draw operation");
        };
        run
    }

    fn tmesh_draw(
        frame: &RenderFrame,
        index: usize,
    ) -> (
        TexturedMeshRun,
        &TexturedMeshInstanceRaw,
        &TexturedMeshGeometry,
    ) {
        let DrawOp::TexturedMesh(run) = frame.ops[index] else {
            panic!("expected textured-mesh draw operation");
        };
        let instance = frame
            .tmesh_instances
            .get(run.instance_start as usize)
            .expect("textured-mesh run references an instance");
        let geometry = frame
            .tmesh_geometries
            .get(run.geometry as usize)
            .expect("textured-mesh run references geometry");
        (run, instance, geometry)
    }

    #[derive(Default)]
    struct TestTextureContext {
        generation: u64,
        dims: HashMap<String, TextureMeta>,
        handles: HashMap<String, deadlib_render::TextureHandle>,
    }

    impl TextureContext for TestTextureContext {
        fn texture_registry_generation(&self) -> u64 {
            self.generation
        }

        fn texture_dims(&self, key: &str) -> Option<TextureMeta> {
            self.dims.get(key).copied()
        }

        fn sprite_sheet_dims(&self, key: &str) -> (u32, u32) {
            crate::font::parse_sprite_sheet_dims_from_key(key)
        }

        fn texture_handle(&self, key: &str) -> deadlib_render::TextureHandle {
            self.handles
                .get(key)
                .copied()
                .unwrap_or(deadlib_render::INVALID_TEXTURE_HANDLE)
        }
    }

    fn boxed_lines(lines: &[&str]) -> Vec<Box<str>> {
        lines.iter().map(|line| Box::<str>::from(*line)).collect()
    }

    fn test_draw_item(z: i16, order: u32) -> DrawItem {
        DrawItem::synthetic(z, order, order)
    }

    fn test_layout() -> CachedTextLayout {
        CachedTextLayout {
            layout_seed: 1,
            font_height: 10,
            line_spacing: 10,
            max_logical_width_i: 0,
            glyph_count: 0,
            texture_pages: Vec::new(),
            lines: Vec::new(),
            glyphs: Vec::new(),
            fill_batches: CachedTextMeshVariants::default(),
            stroke_batches: CachedTextMeshVariants::default(),
        }
    }

    fn test_glyph(texture_key: &Arc<str>) -> Glyph {
        Glyph {
            texture_key: Arc::clone(texture_key),
            stroke_texture_key: None,
            tex_rect: [0.0, 0.0, 8.0, 8.0],
            uv_scale: [0.5, 0.5],
            uv_offset: [0.0, 0.0],
            size: [8.0, 10.0],
            offset: [0.0, -10.0],
            advance: 8.0,
            advance_i32: 8,
        }
    }

    fn test_stroked_glyph(texture_key: &Arc<str>, stroke_key: &Arc<str>) -> Glyph {
        let mut glyph = test_glyph(texture_key);
        glyph.stroke_texture_key = Some(Arc::clone(stroke_key));
        glyph
    }

    fn test_sprite(source: SpriteSource) -> Actor {
        Actor::Sprite {
            align: [0.0, 0.0],
            offset: [4.0, 8.0],
            world_z: 0.0,
            size: [SizeSpec::Px(16.0), SizeSpec::Px(32.0)],
            source,
            tint: [0.8, 0.6, 0.4, 1.0],
            glow: [0.0; 4],
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
            shadow_color: [0.0; 4],
            effect: Default::default(),
        }
    }

    #[test]
    fn build_screen_reserves_recycled_buffers_from_len() {
        let actor = Actor::Mesh {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            tint: [1.0; 4],
            vertices: Arc::from(vec![MeshVertex::default(); 3]),
            visible: true,
            blend: BlendMode::Alpha,
            z: 0,
        };
        let actors = vec![actor; 101];
        let metrics = Metrics {
            left: 0.0,
            right: 100.0,
            top: 100.0,
            bottom: 0.0,
        };
        let fonts = font::FontMap::default();
        let mut text_cache = TextLayoutCache::default();
        let mut scratch = ComposeScratch::default();
        scratch.frame_builder.items = Vec::with_capacity(100);
        scratch.cameras = Vec::with_capacity(2);
        scratch.masks = Vec::with_capacity(4);

        let render = build_screen_cached_with_scratch(
            &actors,
            [0.0, 0.0, 0.0, 1.0],
            &metrics,
            &fonts,
            0.0,
            &mut text_cache,
            &mut scratch,
        );

        assert!(scratch.frame_builder.items.capacity() >= actors.len().saturating_mul(4));
        assert_eq!(render.mesh_vertices.len(), actors.len().saturating_mul(3));
        assert!(render.cameras.capacity() >= 4);
    }

    #[test]
    fn reusable_mesh_composes_shared_vertices_and_tint() {
        let vertices = Arc::new(vec![MeshVertex {
            pos: [3.0, 7.0],
            color: [0.8, 0.6, 0.4, 0.5],
        }]);
        let actors = [Actor::ReusableMesh {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Px(1.0), SizeSpec::Px(1.0)],
            tint: [0.5, 0.25, 1.0, 0.75],
            vertices: Arc::clone(&vertices),
            visible: true,
            blend: BlendMode::Alpha,
            z: 4,
        }];
        let metrics = Metrics {
            left: 0.0,
            right: 100.0,
            top: 100.0,
            bottom: 0.0,
        };

        let render = build_screen(&actors, [0.0; 4], &metrics, &font::FontMap::default(), 0.0);
        let run = mesh_run(&render, 0);
        let composed = &render.mesh_vertices
            [run.vertex_start as usize..(run.vertex_start + run.vertex_count) as usize];
        assert_eq!(composed.len(), vertices.len());
        assert_eq!(composed[0].color, [0.4, 0.15, 0.4, 0.375]);
    }

    #[test]
    fn transient_textured_mesh_buffers_do_not_coalesce() {
        let instance = TexturedMeshInstanceRaw::new(
            Matrix4::IDENTITY,
            [1.0; 4],
            [1.0; 2],
            [0.0; 2],
            [0.0; 2],
            false,
        );
        let object = |x| EditableDraw {
            texture_handle: 1,
            order: x as u32,
            z: 0,
            blend: BlendMode::Alpha,
            camera: 0,
            object_type: EditablePayload::TexturedMesh {
                instance,
                vertices: deadlib_render::TexturedMeshVertices::Transient(vec![
                    TexturedMeshVertex {
                        pos: [x, 0.0, 0.0],
                        ..TexturedMeshVertex::default()
                    },
                ]),
                geom_cache_key: INVALID_TMESH_CACHE_KEY,
                depth_test: false,
            },
        };
        let mut builder = FrameBuilder::default();
        builder.push(object(1.0));
        builder.push(object(2.0));
        let mut mesh_vertices = Vec::new();
        let mut instances = Vec::new();
        let mut geometries = Vec::new();
        let mut ops = Vec::new();
        let mut geometry_map =
            HashMap::<super::TMeshGeomKey, u32, rustc_hash::FxBuildHasher>::default();

        super::finish_frame::<false>(
            &mut builder,
            &mut mesh_vertices,
            &mut instances,
            &mut geometries,
            &mut ops,
            &mut geometry_map,
        );

        assert_eq!(ops.len(), 2);
        assert_eq!(instances.len(), 2);
        assert_eq!(geometries.len(), 2);
        assert!(matches!(ops[0], DrawOp::TexturedMesh(run) if run.geometry == 0));
        assert!(matches!(ops[1], DrawOp::TexturedMesh(run) if run.geometry == 1));
    }

    #[test]
    fn flat_camera_scope_matches_nested_camera() {
        let vertices: Arc<[MeshVertex]> = Arc::from([
            MeshVertex {
                pos: [0.0, 0.0],
                color: [1.0, 0.0, 0.0, 1.0],
            },
            MeshVertex {
                pos: [12.0, 0.0],
                color: [0.0, 1.0, 0.0, 1.0],
            },
            MeshVertex {
                pos: [0.0, 9.0],
                color: [0.0, 0.0, 1.0, 1.0],
            },
        ]);
        let mesh = Actor::Mesh {
            align: [0.0, 0.0],
            offset: [3.0, 4.0],
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            tint: [1.0; 4],
            vertices,
            visible: true,
            blend: BlendMode::Alpha,
            z: 7,
        };
        let view_proj = Matrix4::from_cols_array(&[
            1.0, 0.0, 0.0, 0.0, //
            0.0, 2.0, 0.0, 0.0, //
            0.0, 0.0, 3.0, 0.0, //
            4.0, 5.0, 6.0, 1.0,
        ]);
        let metrics = Metrics {
            left: 0.0,
            right: 100.0,
            top: 100.0,
            bottom: 0.0,
        };
        let fonts = font::FontMap::default();
        let nested = [Actor::Camera {
            view_proj,
            children: vec![mesh.clone()],
        }];
        let flat = [Actor::CameraPush { view_proj }, mesh, Actor::CameraPop];

        let nested_render = build_screen(&nested, [0.0, 0.0, 0.0, 1.0], &metrics, &fonts, 0.0);
        let flat_render = build_screen(&flat, [0.0, 0.0, 0.0, 1.0], &metrics, &fonts, 0.0);

        assert_eq!(nested_render.clear_color, flat_render.clear_color);
        assert_eq!(nested_render.cameras.len(), flat_render.cameras.len());
        for (nested_camera, flat_camera) in nested_render.cameras.iter().zip(&flat_render.cameras) {
            assert_eq!(nested_camera.to_cols_array(), flat_camera.to_cols_array());
        }
        assert_eq!(nested_render.ops, flat_render.ops);
        assert_eq!(nested_render.mesh_vertices, flat_render.mesh_vertices);

        let resources = ActorResourceArena::new(0);
        let mut contiguous_text = TextLayoutCache::default();
        let mut contiguous_scratch = ComposeScratch::default();
        let contiguous = build_screen_cached_with_scratch_and_texture_context_and_actor_resources(
            &flat,
            [0.0, 0.0, 0.0, 1.0],
            &metrics,
            &fonts,
            0.0,
            &mut contiguous_text,
            &mut contiguous_scratch,
            &TestDrawTextureContext,
            &resources,
        );
        let segments = [
            ActorSegment::new(&flat[..1]),
            ActorSegment::new(&flat[1..2]),
            ActorSegment::new(&flat[2..]),
        ];
        let mut segmented_text = TextLayoutCache::default();
        let mut segmented_scratch = ComposeScratch::default();
        let segmented =
            build_screen_segments_cached_with_scratch_and_texture_context_and_actor_resources(
                &segments,
                [0.0, 0.0, 0.0, 1.0],
                &metrics,
                &fonts,
                0.0,
                &mut segmented_text,
                &mut segmented_scratch,
                &TestDrawTextureContext,
                &resources,
            );
        assert_test_frames_equal(&contiguous, &segmented);
    }

    #[test]
    fn shifted_actor_segment_matches_materialized_actor_z() {
        let vertices: Arc<[MeshVertex]> = Arc::from([
            MeshVertex {
                pos: [0.0, 0.0],
                color: [1.0; 4],
            },
            MeshVertex {
                pos: [12.0, 0.0],
                color: [1.0; 4],
            },
            MeshVertex {
                pos: [0.0, 9.0],
                color: [1.0; 4],
            },
        ]);
        let source = [Actor::Mesh {
            align: [0.0, 0.0],
            offset: [3.0, 4.0],
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            tint: [0.5, 0.25, 0.75, 0.8],
            vertices: Arc::clone(&vertices),
            visible: true,
            blend: BlendMode::Alpha,
            z: 7,
        }];
        let materialized = [Actor::Mesh {
            align: [0.0, 0.0],
            offset: [3.0, 4.0],
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            tint: [0.5, 0.25, 0.75, 0.8],
            vertices,
            visible: true,
            blend: BlendMode::Alpha,
            z: 107,
        }];
        let metrics = Metrics {
            left: 0.0,
            right: 100.0,
            top: 100.0,
            bottom: 0.0,
        };
        let fonts = font::FontMap::default();
        let expected = build_screen(&materialized, [0.0, 0.0, 0.0, 1.0], &metrics, &fonts, 0.0);
        let resources = ActorResourceArena::new(0);
        let mut text = TextLayoutCache::default();
        let mut scratch = ComposeScratch::default();
        let actual =
            build_screen_segments_cached_with_scratch_and_texture_context_and_actor_resources(
                &[ActorSegment::shifted(&source, 100)],
                [0.0, 0.0, 0.0, 1.0],
                &metrics,
                &fonts,
                0.0,
                &mut text,
                &mut scratch,
                &TestDrawTextureContext,
                &resources,
            );
        assert_test_frames_equal(&expected, &actual);
    }

    fn test_font() -> Font {
        let texture_key = Arc::<str>::from("test_font_page");
        let glyph_a = test_glyph(&texture_key);
        let glyph_b = test_glyph(&texture_key);
        let mut glyph_map = HashMap::new();
        glyph_map.insert('A', glyph_a.clone());
        glyph_map.insert('B', glyph_b.clone());
        let mut ascii = std::array::from_fn(|_| None);
        ascii['A' as usize] = Some(glyph_a);
        ascii['B' as usize] = Some(glyph_b);
        Font {
            glyph_map,
            ascii_glyphs: Box::new(ascii),
            default_glyph: None,
            line_spacing: 10,
            height: 10,
            fallback_font_name: None,
            cache_tag: 1,
            chain_key: 1,
            default_stroke_color: [0.0; 4],
            stroke_texture_map: HashMap::new(),
            texture_hints_map: HashMap::new(),
        }
    }

    fn test_font_split_pages() -> Font {
        let texture_key_a = Arc::<str>::from("test_font_page_a");
        let texture_key_b = Arc::<str>::from("test_font_page_b");
        let glyph_a = test_glyph(&texture_key_a);
        let glyph_b = test_glyph(&texture_key_b);
        let mut glyph_map = HashMap::new();
        glyph_map.insert('A', glyph_a.clone());
        glyph_map.insert('B', glyph_b.clone());
        let mut ascii = std::array::from_fn(|_| None);
        ascii['A' as usize] = Some(glyph_a);
        ascii['B' as usize] = Some(glyph_b);
        Font {
            glyph_map,
            ascii_glyphs: Box::new(ascii),
            default_glyph: None,
            line_spacing: 10,
            height: 10,
            fallback_font_name: None,
            cache_tag: 2,
            chain_key: 2,
            default_stroke_color: [0.0; 4],
            stroke_texture_map: HashMap::new(),
            texture_hints_map: HashMap::new(),
        }
    }

    fn test_font_with_stroke() -> Font {
        let texture_key = Arc::<str>::from("test_font_page");
        let stroke_key = Arc::<str>::from("test_font_stroke_page");
        let glyph_a = test_stroked_glyph(&texture_key, &stroke_key);
        let glyph_b = test_stroked_glyph(&texture_key, &stroke_key);
        let mut glyph_map = HashMap::new();
        glyph_map.insert('A', glyph_a.clone());
        glyph_map.insert('B', glyph_b.clone());
        let mut ascii = std::array::from_fn(|_| None);
        ascii['A' as usize] = Some(glyph_a);
        ascii['B' as usize] = Some(glyph_b);
        let mut stroke_texture_map = HashMap::new();
        stroke_texture_map.insert(
            "test_font_page".to_owned(),
            "test_font_stroke_page".to_owned(),
        );
        Font {
            glyph_map,
            ascii_glyphs: Box::new(ascii),
            default_glyph: None,
            line_spacing: 10,
            height: 10,
            fallback_font_name: None,
            cache_tag: 2,
            chain_key: 2,
            default_stroke_color: [1.0; 4],
            stroke_texture_map,
            texture_hints_map: HashMap::new(),
        }
    }

    fn test_split_stroke_font(
        fill_a: &Arc<str>,
        fill_b: &Arc<str>,
        stroke_a: &Arc<str>,
        stroke_b: &Arc<str>,
    ) -> Font {
        let glyph_a = test_stroked_glyph(fill_a, stroke_a);
        let glyph_b = test_stroked_glyph(fill_b, stroke_b);
        let glyph_map = HashMap::from([('A', glyph_a.clone()), ('B', glyph_b.clone())]);
        let mut ascii = std::array::from_fn(|_| None);
        ascii['A' as usize] = Some(glyph_a);
        ascii['B' as usize] = Some(glyph_b);
        let stroke_texture_map = HashMap::from([
            (fill_a.to_string(), stroke_a.to_string()),
            (fill_b.to_string(), stroke_b.to_string()),
        ]);
        Font {
            glyph_map,
            ascii_glyphs: Box::new(ascii),
            default_glyph: None,
            line_spacing: 10,
            height: 10,
            fallback_font_name: None,
            cache_tag: 4,
            chain_key: 4,
            default_stroke_color: [1.0; 4],
            stroke_texture_map,
            texture_hints_map: HashMap::new(),
        }
    }

    fn test_stroked_text_actor() -> Actor {
        Actor::Text {
            align: [0.0, 0.0],
            offset: [10.0, 20.0],
            local_transform: Matrix4::IDENTITY,
            color: [1.0; 4],
            stroke_color: Some([1.0; 4]),
            glow: [0.0; 4],
            font: "test",
            content: TextContent::static_str("AB"),
            attributes: TextAttributes::default(),
            align_text: TextAlign::Left,
            z: 0,
            scale: [1.0, 1.0],
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
            shadow_color: [0.0; 4],
            effect: Default::default(),
        }
    }

    #[derive(Default)]
    struct CountingPageContext {
        generation: Cell<u64>,
        handle_calls: Cell<u32>,
    }

    impl TextureContext for CountingPageContext {
        fn texture_registry_generation(&self) -> u64 {
            self.generation.get()
        }

        fn texture_dims(&self, _key: &str) -> Option<TextureMeta> {
            None
        }

        fn sprite_sheet_dims(&self, _key: &str) -> (u32, u32) {
            (1, 1)
        }

        fn texture_handle(&self, key: &str) -> deadlib_render::TextureHandle {
            self.handle_calls.set(self.handle_calls.get() + 1);
            let page = match key {
                "fill_a" => 1,
                "fill_b" => 2,
                "stroke_a" => 3,
                "stroke_b" => 4,
                _ => panic!("unexpected text page {key}"),
            };
            self.generation.get().wrapping_mul(100).wrapping_add(page)
        }
    }

    #[test]
    fn text_layout_builds_only_requested_fill_align() {
        let fonts = font::FontMap::from_iter([("test", test_font())]);
        let font = fonts.get("test").expect("test font");
        let layout = build_cached_text_layout(font, &fonts, "AB", font.line_spacing, -1, 17);

        assert!(!layout.fill_batches.is_built(TextAlign::Left));
        assert!(!layout.fill_batches.is_built(TextAlign::Center));
        assert!(!layout.fill_batches.is_built(TextAlign::Right));
        assert!(!layout.stroke_batches.is_built(TextAlign::Left));

        let left_batches = layout.fill_batches(TextAlign::Left);
        assert_eq!(left_batches.len(), 1);

        assert!(layout.fill_batches.is_built(TextAlign::Left));
        assert!(!layout.fill_batches.is_built(TextAlign::Center));
        assert!(!layout.fill_batches.is_built(TextAlign::Right));
        assert!(!layout.stroke_batches.is_built(TextAlign::Left));
    }

    #[test]
    fn prewarmed_u16_domain_builds_requested_geometry_and_directly_resolves_value() {
        let fonts = font::FontMap::from_iter([("test", test_font())]);
        let font = fonts.get("test").expect("test font");
        let mut cache = TextLayoutCache::new(1);

        cache.prewarm_u16_domain(&fonts, "test", 7, 500, None, TextAlign::Left);
        let content = TextContent::prewarmed_u16(500, 7);
        let layout = cache.get_or_build(font, &fonts, &content, None, None);

        assert_eq!(content.as_str(), "500");
        assert!(layout.fill_batches.is_built(TextAlign::Left));
        assert!(!layout.fill_batches.is_built(TextAlign::Center));
        assert!(!layout.fill_batches.is_built(TextAlign::Right));
        assert!(!layout.stroke_batches.is_built(TextAlign::Left));
        assert_eq!(cache.entry_count, 0);
    }

    #[test]
    fn text_layout_builds_stroke_batches_only_on_demand() {
        let fonts = font::FontMap::from_iter([("test", test_font_with_stroke())]);
        let font = fonts.get("test").expect("test font");
        let layout = build_cached_text_layout(font, &fonts, "AB", font.line_spacing, -1, 23);

        assert!(!layout.stroke_batches.is_built(TextAlign::Left));

        let stroke_batches = layout.stroke_batches(TextAlign::Left);
        assert_eq!(stroke_batches.len(), 1);
        assert!(layout.stroke_batches.is_built(TextAlign::Left));
        assert!(!layout.stroke_batches.is_built(TextAlign::Center));
        assert!(!layout.fill_batches.is_built(TextAlign::Left));
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn text_page_ids_shrink_cached_records() {
        assert_eq!(std::mem::size_of::<TextPageId>(), 4);
        assert_eq!(std::mem::size_of::<Option<TextPageId>>(), 4);
        assert_eq!(std::mem::size_of::<CachedGlyph>(), 56);
        assert_eq!(std::mem::size_of::<CachedTextMeshBatch>(), 32);
        assert_eq!(std::mem::size_of::<CachedTextMeshVariants>(), 48);
        assert_eq!(std::mem::size_of::<CachedTextLayout>(), 200);
        assert_eq!(std::mem::size_of::<TextMeshBatchBuilder>(), 32);
        assert_eq!(std::mem::size_of::<CachedTextPage>(), 32);
    }

    #[test]
    fn cached_layout_owns_font_pages() {
        let fill_a = Arc::<str>::from("fill_a");
        let fill_b = Arc::<str>::from("fill_b");
        let stroke_a = Arc::<str>::from("stroke_a");
        let stroke_b = Arc::<str>::from("stroke_b");
        let weak_pages = [
            Arc::downgrade(&fill_a),
            Arc::downgrade(&fill_b),
            Arc::downgrade(&stroke_a),
            Arc::downgrade(&stroke_b),
        ];
        let mut fonts = font::FontMap::from_iter([(
            "test",
            test_split_stroke_font(&fill_a, &fill_b, &stroke_a, &stroke_b),
        )]);
        let font = fonts.get("test").expect("test font");
        let key = TextLayoutKey {
            font_key: font_chain_key(font, &fonts),
            line_spacing: font.line_spacing,
            wrap_width_pixels: -1,
        };
        let mut cache = TextLayoutCache::new(1);
        cache.prewarm_text(&fonts, "test", "AB", None);

        fonts.remove("test");
        drop((fill_a, fill_b, stroke_a, stroke_b));

        let layout = cache.owned_layout(key, "AB").expect("cached layout");
        assert_eq!(layout.texture_pages.len(), 4);
        assert!(weak_pages.iter().all(|page| page.strong_count() == 1));

        cache.clear();
        assert!(weak_pages.iter().all(|page| page.strong_count() == 0));
    }

    #[test]
    fn cached_layout_dedupes_equal_page_keys() {
        let page_a = Arc::<str>::from("equal_page");
        let page_b = Arc::<str>::from(String::from("equal_page"));
        assert!(!Arc::ptr_eq(&page_a, &page_b));
        let glyph_a = test_glyph(&page_a);
        let glyph_b = test_glyph(&page_b);
        let glyph_map = HashMap::from([('A', glyph_a.clone()), ('B', glyph_b.clone())]);
        let mut ascii = std::array::from_fn(|_| None);
        ascii['A' as usize] = Some(glyph_a);
        ascii['B' as usize] = Some(glyph_b);
        let font = Font {
            glyph_map,
            ascii_glyphs: Box::new(ascii),
            default_glyph: None,
            line_spacing: 10,
            height: 10,
            fallback_font_name: None,
            cache_tag: 5,
            chain_key: 5,
            default_stroke_color: [0.0; 4],
            stroke_texture_map: HashMap::new(),
            texture_hints_map: HashMap::new(),
        };
        let fonts = font::FontMap::from_iter([("test", font)]);
        let font = fonts.get("test").expect("test font");
        let layout = build_cached_text_layout(font, &fonts, "AB", 10, -1, 29);

        assert_eq!(layout.texture_pages.len(), 1);
        assert_eq!(layout.glyphs[0].texture_page, layout.glyphs[1].texture_page);
        let batches = layout.fill_batches(TextAlign::Left);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].vertices.len(), 12);
    }

    #[test]
    fn cached_and_transient_pages_match() {
        let fill_a = Arc::<str>::from("fill_a");
        let fill_b = Arc::<str>::from("fill_b");
        let stroke_a = Arc::<str>::from("stroke_a");
        let stroke_b = Arc::<str>::from("stroke_b");
        let fonts = font::FontMap::from_iter([(
            "test",
            test_split_stroke_font(&fill_a, &fill_b, &stroke_a, &stroke_b),
        )]);
        let font = fonts.get("test").expect("test font");
        let layout = build_cached_text_layout(font, &fonts, "ABA\nBA", 10, -1, 31);
        let mut builders = Vec::new();
        let mut recycled = Vec::new();

        let fill = layout.fill_batches(TextAlign::Center);
        assert_eq!(fill.len(), 2);
        assert_eq!(
            layout.texture_page(fill[0].texture_page).key.as_ref(),
            "fill_a"
        );
        assert_eq!(
            layout.texture_page(fill[1].texture_page).key.as_ref(),
            "fill_b"
        );
        assert_eq!([fill[0].vertices.len(), fill[1].vertices.len()], [18, 12]);
        assert!(
            fill.iter()
                .all(|batch| batch.geom_cache_key != INVALID_TMESH_CACHE_KEY)
        );
        assert_ne!(fill[0].geom_cache_key, fill[1].geom_cache_key);
        build_transient_text_mesh_builders(
            &layout,
            TextAlign::Center,
            &[],
            None,
            0.0,
            false,
            &mut builders,
            &mut recycled,
        );
        assert_eq!(builders.len(), fill.len());
        for (builder, batch) in builders.iter().zip(fill) {
            assert_eq!(builder.texture_page, batch.texture_page);
            assert_eq!(builder.vertices.as_slice(), batch.vertices.as_ref());
        }

        let stroke = layout.stroke_batches(TextAlign::Center);
        assert_eq!(stroke.len(), 2);
        assert_eq!(
            layout.texture_page(stroke[0].texture_page).key.as_ref(),
            "stroke_a"
        );
        assert_eq!(
            layout.texture_page(stroke[1].texture_page).key.as_ref(),
            "stroke_b"
        );
        build_transient_text_mesh_builders(
            &layout,
            TextAlign::Center,
            &[],
            None,
            0.0,
            true,
            &mut builders,
            &mut recycled,
        );
        assert_eq!(builders.len(), stroke.len());
        for (builder, batch) in builders.iter().zip(stroke) {
            assert_eq!(builder.texture_page, batch.texture_page);
            assert_eq!(builder.vertices.as_slice(), batch.vertices.as_ref());
        }
    }

    #[test]
    fn text_page_handles_follow_registry_generation() {
        let fill_a = Arc::<str>::from("fill_a");
        let fill_b = Arc::<str>::from("fill_b");
        let stroke_a = Arc::<str>::from("stroke_a");
        let stroke_b = Arc::<str>::from("stroke_b");
        let fonts = font::FontMap::from_iter([(
            "test",
            test_split_stroke_font(&fill_a, &fill_b, &stroke_a, &stroke_b),
        )]);
        let actors = [test_stroked_text_actor()];
        let metrics = Metrics {
            left: 0.0,
            right: 200.0,
            top: 100.0,
            bottom: 0.0,
        };
        let textures = CountingPageContext::default();
        textures.generation.set(1);
        let mut cache = TextLayoutCache::new(1);

        let first = build_screen_cached_with_scratch_and_texture_context(
            &actors,
            [0.0; 4],
            &metrics,
            &fonts,
            0.0,
            &mut cache,
            &mut ComposeScratch::default(),
            &textures,
        );
        assert_eq!(textures.handle_calls.get(), 4);
        assert_eq!(
            first
                .ops
                .iter()
                .map(|op| match op {
                    DrawOp::TexturedMesh(run) => run.texture_handle,
                    _ => panic!("stroked text emits textured meshes"),
                })
                .collect::<Vec<_>>(),
            [101, 102, 103, 104]
        );

        let _ = build_screen_cached_with_scratch_and_texture_context(
            &actors,
            [0.0; 4],
            &metrics,
            &fonts,
            0.0,
            &mut cache,
            &mut ComposeScratch::default(),
            &textures,
        );
        assert_eq!(textures.handle_calls.get(), 4);

        textures.generation.set(2);
        let refreshed = build_screen_cached_with_scratch_and_texture_context(
            &actors,
            [0.0; 4],
            &metrics,
            &fonts,
            0.0,
            &mut cache,
            &mut ComposeScratch::default(),
            &textures,
        );
        assert_eq!(textures.handle_calls.get(), 8);
        assert_eq!(
            refreshed
                .ops
                .iter()
                .map(|op| match op {
                    DrawOp::TexturedMesh(run) => run.texture_handle,
                    _ => panic!("stroked text emits textured meshes"),
                })
                .collect::<Vec<_>>(),
            [201, 202, 203, 204]
        );
    }

    #[test]
    fn max_generation_never_hits_uncached_stamp() {
        let textures = CountingPageContext::default();
        textures.generation.set(u64::MAX);
        let page = CachedTextPage::new(Arc::from("fill_a"));

        let first = page.texture_handle(u64::MAX, &textures);
        let second = page.texture_handle(u64::MAX, &textures);

        assert_eq!(first, second);
        assert_eq!(textures.handle_calls.get(), 2);
    }

    #[test]
    fn wrapwidthpixels_wraps_on_spaces() {
        let lines = wrap_text_lines_by_words("A BB CCC", 3, 1, |word| word.len() as i32);
        assert_eq!(lines, boxed_lines(&["A", "BB", "CCC"]));
    }

    #[test]
    fn wrapwidthpixels_keeps_empty_lines() {
        let lines = wrap_text_lines_by_words("AA\n\nBB CC", 5, 1, |word| word.len() as i32);
        assert_eq!(lines, boxed_lines(&["AA", "", "BB CC"]));
    }

    #[test]
    fn wrapwidthpixels_keeps_long_word_on_own_line() {
        let lines = wrap_text_lines_by_words("AAAA BB", 3, 1, |word| word.len() as i32);
        assert_eq!(lines, boxed_lines(&["AAAA", "BB"]));
    }

    #[test]
    fn text_attr_cursor_uses_last_matching_attribute() {
        let attrs = [
            TextAttribute {
                start: 2,
                length: 4,
                color: [1.0, 0.0, 0.0, 1.0],
                vertex_colors: None,
                glow: None,
            },
            TextAttribute {
                start: 3,
                length: 2,
                color: [0.0, 1.0, 0.0, 1.0],
                vertex_colors: None,
                glow: None,
            },
            TextAttribute {
                start: 2,
                length: 1,
                color: [0.0, 0.0, 1.0, 1.0],
                vertex_colors: None,
                glow: None,
            },
        ];
        let mut cursor = TextAttrCursor::new(&attrs).expect("attributes should build a cursor");

        assert_eq!(cursor.tint_for(0), [1.0; 4]);
        assert_eq!(cursor.tint_for(2), [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(cursor.tint_for(3), [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(cursor.tint_for(5), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(cursor.tint_for(6), [1.0; 4]);
    }

    #[test]
    fn text_attr_cursor_keeps_slice_order_precedence_with_unsorted_starts() {
        let attrs = [
            TextAttribute {
                start: 5,
                length: 1,
                color: [0.0, 1.0, 0.0, 1.0],
                vertex_colors: None,
                glow: None,
            },
            TextAttribute {
                start: 0,
                length: 10,
                color: [1.0, 0.0, 0.0, 1.0],
                vertex_colors: None,
                glow: None,
            },
        ];
        let mut cursor = TextAttrCursor::new(&attrs).expect("attributes should build a cursor");

        assert_eq!(cursor.tint_for(5), [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn text_attr_cursor_handles_skipped_char_indices() {
        let attrs = [
            TextAttribute {
                start: 1,
                length: 1,
                color: [1.0, 0.0, 0.0, 1.0],
                vertex_colors: None,
                glow: None,
            },
            TextAttribute {
                start: 2,
                length: 3,
                color: [0.0, 1.0, 0.0, 1.0],
                vertex_colors: None,
                glow: None,
            },
            TextAttribute {
                start: 5,
                length: 2,
                color: [0.0, 0.0, 1.0, 1.0],
                vertex_colors: None,
                glow: None,
            },
        ];
        let mut cursor = TextAttrCursor::new(&attrs).expect("attributes should build a cursor");

        assert_eq!(cursor.tint_for(0), [1.0; 4]);
        assert_eq!(cursor.tint_for(3), [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(cursor.tint_for(6), [0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn mask_clip_chooses_largest_intersection() {
        let mut sprite_instances = vec![SpriteInstanceRaw {
            center: [0.0, 0.0, 0.0, 0.0],
            size: [10.0, 10.0],
            rot_sin_cos: [0.0, 1.0],
            tint: [1.0; 4],
            uv_scale: [1.0, 1.0],
            uv_offset: [0.0, 0.0],
            local_offset: [0.0, 0.0],
            local_offset_rot_sin_cos: [0.0, 1.0],
            edge_fade: [0.0; 4],
            texture_mask: 0.0,
        }];
        let mut obj = EditableDraw {
            object_type: EditablePayload::Sprite(0),
            texture_handle: 0,
            blend: BlendMode::Alpha,
            z: 0,
            order: 0,
            camera: 0,
        };

        assert!(clip_object_to_world_masks(
            &mut obj,
            &mut sprite_instances,
            &[
                WorldRect {
                    left: -2.0,
                    right: 2.0,
                    bottom: -2.0,
                    top: 2.0,
                },
                WorldRect {
                    left: -5.0,
                    right: 5.0,
                    bottom: -5.0,
                    top: 5.0,
                },
            ],
            &mut Vec::new(),
        ));

        if let EditablePayload::Sprite(index) = &obj.object_type {
            let sprite = sprite_instances[*index as usize];
            assert_eq!(sprite.size, [10.0, 10.0]);
            assert_eq!(sprite.uv_scale, [1.0, 1.0]);
            assert_eq!(sprite.uv_offset, [0.0, 0.0]);
        } else {
            panic!("expected sprite to remain in fast clip path");
        }
    }

    #[test]
    fn contained_sprite_clip_preserves_raw_instance_without_rebuild() {
        let original = SpriteInstanceRaw {
            center: [17.0, 23.0, 0.25, 1.0],
            size: [10.0, 14.0],
            rot_sin_cos: [0.0, 1.0],
            tint: [0.25, 0.5, 0.75, 0.8],
            uv_scale: [0.5, 0.75],
            uv_offset: [0.125, 0.25],
            local_offset: [2.0, -3.0],
            local_offset_rot_sin_cos: [0.0, 1.0],
            edge_fade: [0.1, 0.2, 0.3, 0.4],
            texture_mask: 1.0,
        };
        let mut sprite_instances = vec![original];
        let mut object = EditableDraw {
            object_type: EditablePayload::Sprite(0),
            texture_handle: 17,
            blend: BlendMode::Alpha,
            z: 3,
            order: 9,
            camera: 0,
        };
        let clip = WorldRect {
            left: 0.0,
            right: 64.0,
            bottom: 0.0,
            top: 64.0,
        };

        let current =
            clipped_sprite_object_to_world_rect(&object, &sprite_instances, clip, None, None)
                .unwrap();
        let legacy = clipped_sprite_object_to_world_rect_legacy(
            &object,
            &sprite_instances,
            clip,
            None,
            None,
        )
        .unwrap();
        assert!(current.sprite.is_none());
        assert_eq!(legacy.sprite, Some(original));

        assert!(clip_sprite_object_to_world_rect(
            &mut object,
            &mut sprite_instances,
            clip,
        ));
        assert!(matches!(object.object_type, EditablePayload::Sprite(0)));
        assert_eq!(sprite_instances, [original]);
    }

    #[test]
    fn partial_sprite_clip_still_matches_crop_rebuild() {
        let original = SpriteInstanceRaw {
            center: [0.0, 0.0, 0.0, 1.0],
            size: [10.0, 10.0],
            rot_sin_cos: [0.0, 1.0],
            tint: [1.0; 4],
            uv_scale: [1.0; 2],
            uv_offset: [0.0; 2],
            local_offset: [0.0; 2],
            local_offset_rot_sin_cos: [0.0, 1.0],
            edge_fade: [0.0; 4],
            texture_mask: 0.0,
        };
        let object = EditableDraw {
            object_type: EditablePayload::Sprite(0),
            texture_handle: 1,
            blend: BlendMode::Alpha,
            z: 0,
            order: 0,
            camera: 0,
        };
        let clip = WorldRect {
            left: -2.0,
            right: 5.0,
            bottom: -5.0,
            top: 5.0,
        };

        let current =
            clipped_sprite_object_to_world_rect(&object, &[original], clip, None, None).unwrap();
        let legacy =
            clipped_sprite_object_to_world_rect_legacy(&object, &[original], clip, None, None)
                .unwrap();
        assert_eq!(current.sprite, legacy.sprite);
        let clipped = current.sprite.unwrap();
        assert_eq!(clipped.center, [1.5, 0.0, 0.0, 1.0]);
        assert_eq!(clipped.size, [7.0, 10.0]);
        assert_eq!(clipped.uv_scale, [0.7, 1.0]);
        assert_eq!(clipped.uv_offset, [0.3, 0.0]);
    }

    #[test]
    fn multi_mask_transient_mesh_reuses_candidate_buffers() {
        let source = vec![TexturedMeshVertex::default(); 3];
        let mut obj = EditableDraw {
            object_type: EditablePayload::TexturedMesh {
                instance: TexturedMeshInstanceRaw::new(
                    Matrix4::IDENTITY,
                    [1.0; 4],
                    [1.0, 1.0],
                    [0.0, 0.0],
                    [0.0, 0.0],
                    false,
                ),
                vertices: deadlib_render::TexturedMeshVertices::Transient(source),
                geom_cache_key: INVALID_TMESH_CACHE_KEY,
                depth_test: false,
            },
            texture_handle: 0,
            blend: BlendMode::Alpha,
            z: 0,
            order: 0,
            camera: 0,
        };
        let first = Vec::with_capacity(3);
        let chosen = Vec::with_capacity(3);
        let chosen_ptr = chosen.as_ptr();
        let mut recycled_vertices = vec![first, chosen];

        assert!(clip_object_to_world_masks(
            &mut obj,
            &mut [],
            &[
                WorldRect {
                    left: -1.0,
                    right: 1.0,
                    bottom: -1.0,
                    top: 1.0,
                },
                WorldRect {
                    left: -2.0,
                    right: 2.0,
                    bottom: -2.0,
                    top: 2.0,
                },
            ],
            &mut recycled_vertices,
        ));

        let EditablePayload::TexturedMesh { vertices, .. } = &obj.object_type else {
            panic!("clipped object should remain a textured mesh");
        };
        let deadlib_render::TexturedMeshVertices::Transient(vertices) = vertices else {
            panic!("clipped transient geometry should remain recyclable");
        };
        assert_eq!(vertices.as_ptr(), chosen_ptr);
        assert_eq!(recycled_vertices.len(), 2);
    }

    #[test]
    fn single_mask_textured_mesh_matches_independent_bounds_clipping() {
        let textured_mesh_vertex = |pos| TexturedMeshVertex {
            pos,
            ..TexturedMeshVertex::default()
        };
        let vertices: Arc<[TexturedMeshVertex]> = Arc::from([
            textured_mesh_vertex([-4.0, -4.0, 0.0]),
            textured_mesh_vertex([4.0, -4.0, 0.0]),
            textured_mesh_vertex([4.0, 4.0, 0.0]),
            textured_mesh_vertex([-4.0, -4.0, 0.0]),
            textured_mesh_vertex([4.0, 4.0, 0.0]),
            textured_mesh_vertex([-4.0, 4.0, 0.0]),
        ]);
        let source = EditableDraw {
            object_type: EditablePayload::TexturedMesh {
                instance: TexturedMeshInstanceRaw::new(
                    Matrix4::IDENTITY,
                    [0.25, 0.5, 0.75, 1.0],
                    [0.75, 0.5],
                    [0.125, 0.25],
                    [0.0, 0.0],
                    false,
                ),
                vertices: deadlib_render::TexturedMeshVertices::Shared(vertices),
                geom_cache_key: 41,
                depth_test: true,
            },
            texture_handle: 17,
            blend: BlendMode::Add,
            z: 3,
            order: 9,
            camera: 0,
        };
        let clip = WorldRect {
            left: -1.0,
            right: 3.0,
            bottom: -3.0,
            top: 2.0,
        };
        let expected = clipped_sprite_object_to_world_rect(&source, &[], clip, None, None).unwrap();
        let mut actual = source.clone();

        assert!(clip_sprite_object_to_world_rect(
            &mut actual,
            &mut Vec::new(),
            clip,
        ));
        assert_eq!(actual.texture_handle, source.texture_handle);
        assert_eq!(actual.blend, source.blend);
        assert_eq!(actual.z, source.z);
        assert_eq!(actual.order, source.order);
        let (
            EditablePayload::TexturedMesh {
                instance: actual_instance,
                vertices: actual_vertices,
                geom_cache_key: actual_key,
                depth_test: actual_depth,
            },
            EditablePayload::TexturedMesh {
                instance: expected_instance,
                vertices: expected_vertices,
                geom_cache_key: expected_key,
                depth_test: expected_depth,
            },
        ) = (&actual.object_type, &expected.object_type)
        else {
            panic!("partial clipping should produce textured meshes");
        };
        assert_eq!(actual_instance, expected_instance);
        assert_eq!(actual_vertices.as_ref(), expected_vertices.as_ref());
        assert_eq!(actual_key, expected_key);
        assert_eq!(actual_depth, expected_depth);
    }

    #[test]
    fn affine_textured_mesh_clipping_matches_projective_math() {
        let vertex = |pos, uv| TexturedMeshVertex {
            pos,
            uv,
            color: [0.25, 0.5, 0.75, 1.0],
            tex_matrix_scale: [1.0; 2],
        };
        let vertices = [
            vertex([-4.0, -4.0, 0.0], [0.0, 1.0]),
            vertex([4.0, -4.0, 0.0], [1.0, 1.0]),
            vertex([4.0, 4.0, 0.0], [1.0, 0.0]),
            vertex([-4.0, -4.0, 0.0], [0.0, 1.0]),
            vertex([4.0, 4.0, 0.0], [1.0, 0.0]),
            vertex([-4.0, 4.0, 0.0], [0.0, 0.0]),
        ];
        let transform = Matrix4::from_translation(Vector3::new(7.0, 11.0, 0.0))
            * Matrix4::from_rotation_z(0.37)
            * Matrix4::from_scale(Vector3::new(1.5, -0.75, 1.0));
        assert!(is_affine_world_transform(&transform));
        let clip = WorldRect {
            left: 4.0,
            right: 11.0,
            bottom: 8.0,
            top: 14.0,
        };

        let current_bounds = textured_mesh_world_bounds(&vertices, transform).unwrap();
        let legacy_bounds = textured_mesh_world_bounds_legacy(&vertices, transform).unwrap();
        for (current, legacy) in [
            (current_bounds.left, legacy_bounds.left),
            (current_bounds.right, legacy_bounds.right),
            (current_bounds.bottom, legacy_bounds.bottom),
            (current_bounds.top, legacy_bounds.top),
        ] {
            assert!((current - legacy).abs() <= 1e-6);
        }

        let current = clip_textured_mesh_to_world_rect(
            [0.8, 0.6, 0.4, 1.0],
            &vertices,
            transform,
            [0.75, 0.5],
            [0.125, 0.25],
            [0.0; 2],
            clip,
            false,
            None,
        )
        .unwrap();
        let legacy = clip_textured_mesh_to_world_rect_legacy(
            [0.8, 0.6, 0.4, 1.0],
            &vertices,
            transform,
            [0.75, 0.5],
            [0.125, 0.25],
            [0.0; 2],
            clip,
            false,
            None,
        )
        .unwrap();
        let (
            EditablePayload::TexturedMesh {
                vertices: current_vertices,
                ..
            },
            EditablePayload::TexturedMesh {
                vertices: legacy_vertices,
                ..
            },
        ) = (&current.object_type, &legacy.object_type)
        else {
            panic!("clipping should produce textured meshes");
        };
        assert_eq!(current_vertices.len(), legacy_vertices.len());
        for (current, legacy) in current_vertices.iter().zip(legacy_vertices.iter()) {
            for (current, legacy) in current.pos.iter().zip(legacy.pos) {
                assert!((*current - legacy).abs() <= 1e-5);
            }
            for (current, legacy) in current.uv.iter().zip(legacy.uv) {
                assert!((*current - legacy).abs() <= 1e-6);
            }
            assert_eq!(current.color, legacy.color);
            assert_eq!(current.tex_matrix_scale, legacy.tex_matrix_scale);
        }
    }

    #[test]
    fn projective_textured_mesh_bounds_keep_perspective_divide() {
        let mut transform = Matrix4::IDENTITY;
        transform.x_axis.w = 0.25;
        assert!(!is_affine_world_transform(&transform));
        let vertices = [
            TexturedMeshVertex {
                pos: [-2.0, 3.0, 0.0],
                ..TexturedMeshVertex::default()
            },
            TexturedMeshVertex {
                pos: [2.0, 3.0, 0.0],
                ..TexturedMeshVertex::default()
            },
        ];

        let bounds = textured_mesh_world_bounds(&vertices, transform).unwrap();
        let legacy = textured_mesh_world_bounds_legacy(&vertices, transform).unwrap();

        assert_eq!(bounds.left, -4.0);
        assert_eq!(bounds.right, 4.0 / 3.0);
        assert_eq!(bounds.bottom, 2.0);
        assert_eq!(bounds.top, 6.0);
        assert_eq!(bounds.left, legacy.left);
        assert_eq!(bounds.right, legacy.right);
        assert_eq!(bounds.bottom, legacy.bottom);
        assert_eq!(bounds.top, legacy.top);
    }

    #[test]
    fn rotated_clip_preserves_texture_handle() {
        let mut sprite_instances = vec![SpriteInstanceRaw {
            center: [0.0, 0.0, 0.0, 0.0],
            size: [10.0, 10.0],
            rot_sin_cos: [45.0_f32.to_radians().sin(), 45.0_f32.to_radians().cos()],
            tint: [0.25, 0.5, 0.75, 1.0],
            uv_scale: [1.0, 1.0],
            uv_offset: [0.0, 0.0],
            local_offset: [0.0, 0.0],
            local_offset_rot_sin_cos: [0.0, 1.0],
            edge_fade: [0.0; 4],
            texture_mask: 0.0,
        }];
        let mut obj = EditableDraw {
            object_type: EditablePayload::Sprite(0),
            texture_handle: 17,
            blend: BlendMode::Alpha,
            z: 0,
            order: 0,
            camera: 0,
        };

        assert!(clip_sprite_object_to_world_rect(
            &mut obj,
            &mut sprite_instances,
            WorldRect {
                left: -3.0,
                right: 3.0,
                bottom: -3.0,
                top: 3.0,
            },
        ));

        match &obj.object_type {
            EditablePayload::TexturedMesh {
                instance, vertices, ..
            } => {
                assert_eq!(obj.texture_handle, 17);
                assert_eq!(instance.transform(), Matrix4::IDENTITY);
                assert!(!vertices.is_empty());
            }
            _ => panic!("expected rotated clip to produce textured mesh"),
        }
    }

    #[test]
    fn texture_lookup_cache_uses_frame_ptr_tables() {
        let mut cache = TextureLookupCache::default();
        let mut texture_ctx = TestTextureContext {
            generation: 1,
            ..TestTextureContext::default()
        };
        let key = Arc::<str>::from("frame_tex");
        let key_ptr = key.as_ref() as *const str;
        let key_addr = TextureLookupCache::ptr_cache_key(key_ptr);
        let fingerprint = TextureLookupCache::key_fingerprint(key.as_ref());
        cache.begin_frame(&texture_ctx);
        texture_ctx
            .dims
            .insert(String::from(key.as_ref()), TextureMeta { w: 64, h: 32 });
        texture_ctx.handles.insert(String::from(key.as_ref()), 11);
        cache.dims.insert(
            key_addr,
            TextureCacheEntry {
                fingerprint,
                validated_frame: cache.frame,
                value: TextureMeta { w: 64, h: 32 },
            },
        );
        cache.sheets.insert(
            key_addr,
            TextureCacheEntry {
                fingerprint,
                validated_frame: cache.frame,
                value: (4, 2),
            },
        );
        cache.handles.insert(
            key_addr,
            TextureCacheEntry {
                fingerprint,
                validated_frame: cache.frame,
                value: 11,
            },
        );

        let Some(meta) = cache.texture_dims(&texture_ctx, key_ptr, key.as_ref()) else {
            panic!("expected cached texture dims");
        };
        assert_eq!(meta.w, 64);
        assert_eq!(meta.h, 32);
        assert_eq!(
            cache.sprite_sheet_dims(&texture_ctx, key_ptr, key.as_ref()),
            (4, 2)
        );
        assert_eq!(
            cache.texture_handle(&texture_ctx, key_ptr, key.as_ref()),
            11
        );

        cache.begin_frame(&texture_ctx);
        assert!(
            cache
                .texture_dims(&texture_ctx, key_ptr, "other_frame_tex")
                .is_none()
        );
        assert_eq!(
            cache.texture_dims(&texture_ctx, key_ptr, key.as_ref()),
            Some(TextureMeta { w: 64, h: 32 })
        );
        assert_eq!(cache.dims[&key_addr].validated_frame, cache.frame);

        assert_eq!(cache.dims.len(), 1);
        assert_eq!(cache.sheets.len(), 1);
        assert_eq!(cache.handles.len(), 1);

        cache.generation = cache.generation.wrapping_sub(1);
        cache.begin_frame(&texture_ctx);

        assert!(cache.dims.is_empty());
        assert!(cache.sheets.is_empty());
        assert!(cache.handles.is_empty());
    }

    #[test]
    fn arena_texture_actor_composes_like_owned_texture_actor() {
        let key: Arc<str> = Arc::from("noteskin/tap");
        let mut texture_ctx = TestTextureContext {
            generation: 3,
            ..TestTextureContext::default()
        };
        texture_ctx.handles.insert(key.to_string(), 17);
        let metrics = Metrics {
            left: 0.0,
            right: 100.0,
            top: 100.0,
            bottom: 0.0,
        };
        let fonts = font::FontMap::default();

        let owned = [test_sprite(SpriteSource::TextureHandle {
            key: Arc::clone(&key),
            handle: 17,
            generation: 3,
        })];
        let arena = ActorResourceArena::new(1);
        let cached = AtomicU64::new(0);
        let arena_actor = [test_sprite(arena.texture_source(&key, 17, 3, &cached))];
        let mut owned_cache = TextLayoutCache::default();
        let mut owned_scratch = ComposeScratch::default();
        let mut arena_cache = TextLayoutCache::default();
        let mut arena_scratch = ComposeScratch::default();

        let owned_render = build_screen_cached_with_scratch_and_texture_context(
            &owned,
            [0.0; 4],
            &metrics,
            &fonts,
            0.0,
            &mut owned_cache,
            &mut owned_scratch,
            &texture_ctx,
        );
        let arena_render = build_screen_cached_with_scratch_and_texture_context_and_actor_resources(
            &arena_actor,
            [0.0; 4],
            &metrics,
            &fonts,
            0.0,
            &mut arena_cache,
            &mut arena_scratch,
            &texture_ctx,
            &arena,
        );

        assert_eq!(arena_render.ops, owned_render.ops);
        assert_eq!(arena_render.sprite_instances, owned_render.sprite_instances);
        assert_eq!(sprite_run(&arena_render, 0).texture_handle, 17);
    }

    #[test]
    fn custom_texture_rect_does_not_change_native_sprite_size() {
        const KEY: &str = "compose_test/custom_rect_size.png";
        let mut texture_ctx = TestTextureContext::default();
        texture_ctx
            .dims
            .insert(String::from(KEY), TextureMeta { w: 256, h: 128 });
        let mut cache = TextureLookupCache::default();

        let plain = resolve_sprite_size_like_sm(
            [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            false,
            KEY,
            str_ptr(KEY),
            None,
            None,
            None,
            [1.0, 1.0],
            &mut cache,
            &texture_ctx,
        );
        let repeated = resolve_sprite_size_like_sm(
            [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            false,
            KEY,
            str_ptr(KEY),
            Some([0.0, 0.0, 60.0, 60.0]),
            None,
            None,
            [1.0, 1.0],
            &mut cache,
            &texture_ctx,
        );
        let zoomed = resolve_sprite_size_like_sm(
            [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            false,
            KEY,
            str_ptr(KEY),
            Some([0.0, 0.0, 60.0, 60.0]),
            None,
            None,
            [20.0, 20.0],
            &mut cache,
            &texture_ctx,
        );

        fn assert_px_size(size: [SizeSpec; 2], want: [f32; 2]) {
            let [SizeSpec::Px(got_w), SizeSpec::Px(got_h)] = size else {
                panic!("expected pixel size, got {size:?}");
            };
            assert!(
                (got_w - want[0]).abs() <= 1e-6,
                "width mismatch: {got_w} vs {}",
                want[0]
            );
            assert!(
                (got_h - want[1]).abs() <= 1e-6,
                "height mismatch: {got_h} vs {}",
                want[1]
            );
        }

        assert_px_size(plain, [256.0, 128.0]);
        assert_px_size(repeated, [256.0, 128.0]);
        assert_px_size(zoomed, [5120.0, 2560.0]);
    }

    #[test]
    fn sprite_rotationy_180_folds_to_horizontal_flip() {
        let (flip_x, flip_y, size_x, size_y) =
            fold_sprite_xy_rot(false, false, 22.0, 10.0, 0.0, 180.0);
        assert!(flip_x);
        assert!(!flip_y);
        assert!((size_x - 22.0).abs() < 0.0001);
        assert!((size_y - 10.0).abs() < 0.0001);
    }

    #[test]
    fn sprite_rotationx_180_folds_to_vertical_flip() {
        let (flip_x, flip_y, size_x, size_y) =
            fold_sprite_xy_rot(false, false, 22.0, 10.0, 180.0, 0.0);
        assert!(!flip_x);
        assert!(flip_y);
        assert!((size_x - 22.0).abs() < 0.0001);
        assert!((size_y - 10.0).abs() < 0.0001);
    }

    #[test]
    fn lock_growth_saturates_future_inserts() {
        let key = TextLayoutKey {
            font_key: 7,
            line_spacing: 10,
            wrap_width_pixels: -1,
        };
        let mut cache = TextLayoutCache::new(4);
        assert!(
            cache
                .insert_owned_layout(key, "alpha", Box::new(test_layout()))
                .is_some()
        );
        assert_eq!(cache.entry_count, 1);

        cache.lock_growth();

        assert_eq!(cache.max_entries, 1);
        assert_eq!(cache.max_aliases, 0);
        assert!(
            cache
                .insert_owned_layout(key, "beta", Box::new(test_layout()))
                .is_none()
        );
        assert_eq!(cache.entry_count, 1);
        assert!(cache.owned_layout(key, "beta").is_none());
        assert!(cache.uncached_layout.is_some());
    }

    #[test]
    fn lock_growth_with_reserve_retains_late_owned_misses_until_full() {
        let fonts = font::FontMap::from_iter([("test", test_font())]);
        let mut cache = TextLayoutCache::new(3);
        cache.prewarm_text(&fonts, "test", "A", None);
        cache.lock_growth_with_reserve(1);
        cache.begin_frame_stats(true);

        cache.prewarm_text(&fonts, "test", "B", None);
        cache.prewarm_text(&fonts, "test", "B", None);
        cache.prewarm_text(&fonts, "test", "AB", None);
        cache.prewarm_text(&fonts, "test", "AB", None);

        let stats = cache.frame_stats();
        assert_eq!(stats.owned_hits, 1);
        assert_eq!(stats.misses, 3);
        assert_eq!(stats.owned_entries, 2);
    }

    #[test]
    fn frame_inline_layout_matches_owned_without_cache_growth() {
        fn signature(
            layout: &CachedTextLayout,
        ) -> (i32, i32, usize, Vec<(i32, usize)>, Vec<(i32, usize)>) {
            (
                layout.line_spacing,
                layout.max_logical_width_i,
                layout.glyph_count,
                layout
                    .lines
                    .iter()
                    .map(|line| (line.width_i32, line.glyph_len))
                    .collect(),
                layout
                    .glyphs
                    .iter()
                    .map(|glyph| (glyph.advance_i32, glyph.char_index))
                    .collect(),
            )
        }

        let fonts = font::FontMap::from_iter([("test", test_font())]);
        let font = &fonts["test"];
        let key = TextLayoutKey {
            font_key: font_chain_key(font, &fonts),
            line_spacing: font.line_spacing,
            wrap_width_pixels: -1,
        };
        let mut cache = TextLayoutCache::new(4);
        let owned = signature(cache.get_or_build_owned(key, font, &fonts, "ABAB"));
        let text = InlineText::copy_from("ABAB").expect("test text fits inline");
        let frame = signature(cache.get_or_build_frame_inline_slot(key, font, &fonts, text, 0));

        assert_eq!(frame, owned);
        assert_eq!(cache.entry_count, 1);

        cache.begin_frame_stats(true);
        let other = InlineText::copy_from("BABA").expect("test text fits inline");
        let _ = cache.get_or_build_frame_inline_slot(key, font, &fonts, other, 0);
        let _ = cache.get_or_build_frame_inline_slot(key, font, &fonts, other, 0);
        let stats = cache.frame_stats();
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.owned_hits, 1);
        assert_eq!(stats.owned_entries, 1);
    }

    #[test]
    fn frame_inline_prewarm_sizes_reusable_vertex_buffers() {
        let fonts = font::FontMap::from_iter([("test", test_font_split_pages())]);
        let mut cache = TextLayoutCache::new(4);
        let mut scratch = ComposeScratch::default();
        let text = InlineText::copy_from("ABABABABABA").expect("test text fits inline");

        prewarm_frame_inline_text_slot(&mut cache, &mut scratch, &fonts, "test", text, 0, 4);

        assert_eq!(cache.entry_count, 0);
        assert_eq!(scratch.recycled_text_mesh_vertices.len(), 8);
        assert!(
            scratch
                .recycled_text_mesh_vertices
                .iter()
                .all(|vertices| vertices.capacity() >= text.as_str().len() * 6)
        );
    }

    #[test]
    fn frame_inline_slots_retain_independent_last_values() {
        let fonts = font::FontMap::from_iter([("test", test_font())]);
        let font = &fonts["test"];
        let key = TextLayoutKey {
            font_key: font_chain_key(font, &fonts),
            line_spacing: font.line_spacing,
            wrap_width_pixels: -1,
        };
        let mut cache = TextLayoutCache::new(1);
        let mut scratch = ComposeScratch::default();
        let first = InlineText::copy_from("AB").expect("test text fits");
        let second = InlineText::copy_from("BA").expect("test text fits");
        prewarm_frame_inline_text_slot(&mut cache, &mut scratch, &fonts, "test", first, 3, 2);
        prewarm_frame_inline_text_slot(&mut cache, &mut scratch, &fonts, "test", second, 4, 2);
        cache.begin_frame_stats(true);

        let _ = cache.get_or_build_frame_inline_slot(key, font, &fonts, first, 3);
        let _ = cache.get_or_build_frame_inline_slot(key, font, &fonts, second, 4);
        let changed = InlineText::copy_from("ABA").expect("test text fits");
        let _ = cache.get_or_build_frame_inline_slot(key, font, &fonts, changed, 3);
        let _ = cache.get_or_build_frame_inline_slot(key, font, &fonts, second, 4);
        let _ = cache.get_or_build_frame_inline_slot(key, font, &fonts, changed, 3);

        let stats = cache.frame_stats();
        assert_eq!(stats.owned_hits, 4);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.owned_entries, 0);
        assert_eq!(cache.frame_inline_slots.len(), 5);
    }

    #[test]
    fn lock_growth_with_reserve_retains_late_shared_aliases_until_full() {
        let fonts = font::FontMap::from_iter([("test", test_font())]);
        let font = &fonts["test"];
        let key = TextLayoutKey {
            font_key: font_chain_key(font, &fonts),
            line_spacing: font.line_spacing,
            wrap_width_pixels: -1,
        };
        let mut cache = TextLayoutCache::new(3);
        let first = Arc::<str>::from("A");
        let retained = Arc::<str>::from("B");
        let saturated = Arc::<str>::from("AB");
        let _ = cache.get_or_build_shared(key, font, &fonts, &first);
        cache.lock_growth_with_reserve(1);
        cache.begin_frame_stats(true);

        let _ = cache.get_or_build_shared(key, font, &fonts, &retained);
        let _ = cache.get_or_build_shared(key, font, &fonts, &retained);
        let _ = cache.get_or_build_shared(key, font, &fonts, &saturated);
        let _ = cache.get_or_build_shared(key, font, &fonts, &saturated);

        let stats = cache.frame_stats();
        assert_eq!(stats.shared_hits, 1);
        assert_eq!(stats.misses, 3);
        assert_eq!(stats.shared_aliases, 2);
    }

    #[test]
    fn shared_text_aliases_remain_separate_across_layout_keys() {
        let fonts = font::FontMap::from_iter([("test", test_font())]);
        let font = &fonts["test"];
        let default_key = TextLayoutKey {
            font_key: font_chain_key(font, &fonts),
            line_spacing: font.line_spacing,
            wrap_width_pixels: -1,
        };
        let spaced_key = TextLayoutKey {
            line_spacing: font.line_spacing + 3,
            ..default_key
        };
        let text = Arc::<str>::from("AB");
        let mut cache = TextLayoutCache::new(4);
        cache.begin_frame_stats(true);

        let default_layout =
            cache.get_or_build_shared(default_key, font, &fonts, &text) as *const CachedTextLayout;
        let spaced_layout =
            cache.get_or_build_shared(spaced_key, font, &fonts, &text) as *const CachedTextLayout;
        let default_hit =
            cache.get_or_build_shared(default_key, font, &fonts, &text) as *const CachedTextLayout;
        let spaced_hit =
            cache.get_or_build_shared(spaced_key, font, &fonts, &text) as *const CachedTextLayout;

        assert_ne!(default_layout, spaced_layout);
        assert_eq!(default_layout, default_hit);
        assert_eq!(spaced_layout, spaced_hit);
        let stats = cache.frame_stats();
        assert_eq!(stats.shared_hits, 2);
        assert_eq!(stats.misses, 2);
        assert_eq!(stats.shared_aliases, 2);
    }

    #[test]
    fn shared_text_alias_reuses_matching_owned_layout() {
        let fonts = font::FontMap::from_iter([("test", test_font())]);
        let font = &fonts["test"];
        let key = TextLayoutKey {
            font_key: font_chain_key(font, &fonts),
            line_spacing: font.line_spacing,
            wrap_width_pixels: -1,
        };
        let text = Arc::<str>::from("AB");
        let mut cache = TextLayoutCache::new(4);
        cache.prewarm_text(&fonts, "test", text.as_ref(), None);
        cache.begin_frame_stats(true);

        let first = cache.get_or_build_shared(key, font, &fonts, &text) as *const CachedTextLayout;
        let second = cache.get_or_build_shared(key, font, &fonts, &text) as *const CachedTextLayout;

        assert_eq!(first, second);
        assert_eq!(cache.layouts.len(), 1);
        let stats = cache.frame_stats();
        assert_eq!(stats.owned_hits, 1);
        assert_eq!(stats.shared_hits, 1);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.shared_aliases, 1);
    }

    #[test]
    fn text_layout_frame_stats_are_opt_in() {
        let mut cache = TextLayoutCache::new(4);
        let layout = test_layout();

        cache.record_layout_build(&layout);
        assert_eq!(cache.frame_stats().misses, 0);

        cache.begin_frame_stats(true);
        cache.record_layout_build(&layout);
        let stats = cache.frame_stats();
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.built_lines, layout.lines.len() as u32);
        assert_eq!(stats.built_glyphs, layout.glyph_count as u32);

        cache.begin_frame_stats(false);
        cache.record_layout_build(&layout);
        assert_eq!(cache.frame_stats().misses, 0);
    }

    #[test]
    fn text_layout_cache_saturates_at_capacity() {
        let key = TextLayoutKey {
            font_key: 7,
            line_spacing: 10,
            wrap_width_pixels: -1,
        };
        let mut cache = TextLayoutCache::new(1);

        assert!(
            cache
                .insert_owned_layout(key, "alpha", Box::new(test_layout()))
                .is_some()
        );
        assert!(
            cache
                .insert_owned_layout(key, "beta", Box::new(test_layout()))
                .is_none()
        );
        assert_eq!(cache.entry_count, 1);
        assert!(cache.owned_layout(key, "alpha").is_some());
        assert!(cache.owned_layout(key, "beta").is_none());
        assert!(cache.uncached_layout.is_some());
    }

    #[test]
    fn recycle_frame_recovers_transient_textured_mesh_vertices() {
        let mut scratch = ComposeScratch::default();
        let mut render = deadlib_render::RenderFrame {
            clear_color: [0.0, 0.0, 0.0, 1.0],
            cameras: Vec::new(),
            sprite_instances: Vec::new(),
            mesh_vertices: Vec::new(),
            tmesh_instances: Vec::new(),
            tmesh_geometries: vec![deadlib_render::TexturedMeshGeometry {
                vertices: deadlib_render::TexturedMeshVertices::Transient(vec![
                    TexturedMeshVertex::default();
                    6
                ]),
                cache_key: INVALID_TMESH_CACHE_KEY,
            }],
            ops: Vec::new(),
        };

        scratch.recycle_frame(&mut render);

        assert!(scratch.frame_builder.items.is_empty());
        assert_eq!(scratch.recycled_text_mesh_vertices.len(), 1);
        assert!(scratch.recycled_text_mesh_vertices[0].is_empty());
        assert!(scratch.recycled_text_mesh_vertices[0].capacity() >= 6);
        let storage = scratch.storage_stats();
        let recycle_slot = super::COMPOSE_STORAGE_NAMES
            .iter()
            .position(|name| *name == "text_recycle")
            .expect("text recycle storage slot exists");
        let vertices_slot = super::COMPOSE_STORAGE_NAMES
            .iter()
            .position(|name| *name == "text_vertices")
            .expect("text vertex storage slot exists");
        assert!(storage.capacities[recycle_slot] >= 1);
        assert!(storage.capacities[vertices_slot] >= 6);
    }

    #[test]
    fn transient_mesh_shadow_reuses_vertex_buffer() {
        let source = vec![TexturedMeshVertex::default(); 6];
        let mut objects = FrameBuilder::default();
        objects.push(EditableDraw {
            object_type: EditablePayload::TexturedMesh {
                instance: TexturedMeshInstanceRaw::new(
                    Matrix4::IDENTITY,
                    [1.0; 4],
                    [1.0, 1.0],
                    [0.0, 0.0],
                    [0.0, 0.0],
                    false,
                ),
                vertices: deadlib_render::TexturedMeshVertices::Transient(source.clone()),
                geom_cache_key: INVALID_TMESH_CACHE_KEY,
                depth_test: false,
            },
            texture_handle: 9,
            blend: BlendMode::Alpha,
            z: 0,
            order: 0,
            camera: 0,
        });
        let mut sprite_instances = Vec::new();
        let recycled = Vec::with_capacity(source.len());
        let recycled_ptr = recycled.as_ptr();
        let mut recycled_vertices = vec![recycled];

        push_shadow_objects_for_range(
            &mut objects,
            &mut sprite_instances,
            &mut recycled_vertices,
            0,
            1,
            [2.0, 3.0],
            [0.5; 4],
        );

        assert!(recycled_vertices.is_empty());
        let shadow = objects
            .textured_meshes
            .get(objects.items[1].payload_index as usize)
            .and_then(Option::as_ref)
            .expect("shadow should remain a textured mesh");
        let vertices = &shadow.vertices;
        let deadlib_render::TexturedMeshVertices::Transient(vertices) = vertices else {
            panic!("transient shadow should keep recyclable ownership");
        };
        assert_eq!(vertices.as_ptr(), recycled_ptr);
        assert_eq!(vertices.len(), source.len());
        assert!(vertices.iter().zip(&source).all(|(actual, expected)| {
            actual.pos == expected.pos
                && actual.uv == expected.uv
                && actual.color == expected.color
                && actual.tex_matrix_scale == expected.tex_matrix_scale
        }));
    }

    #[test]
    fn compact_draw_items_sort_without_moving_payloads() {
        let mut builder = FrameBuilder::default();
        for (z, order) in [(5, 3), (4, 0), (5, 1), (5, 2)] {
            builder.push_mesh(
                deadlib_render::INVALID_TEXTURE_HANDLE,
                order,
                z,
                BlendMode::Alpha,
                0,
                super::MeshPayload {
                    transform: Matrix4::IDENTITY,
                    tint: [1.0; 4],
                    vertices: super::MeshVertices::Shared(Arc::from([])),
                },
            );
        }
        let payload_addresses = builder
            .meshes
            .iter()
            .map(|payload| payload.as_ref().expect("live payload") as *const _)
            .collect::<Vec<_>>();
        let mut scratch = ComposeScratch::default();

        sort_draw_items(&mut builder.items, &mut scratch);

        assert_eq!(
            builder
                .items
                .iter()
                .map(|item| (item.z, item.order, item.payload_index))
                .collect::<Vec<_>>(),
            vec![(4, 0, 1), (5, 1, 2), (5, 2, 3), (5, 3, 0)]
        );
        assert_eq!(
            payload_addresses,
            builder
                .meshes
                .iter()
                .map(|payload| payload.as_ref().expect("live payload") as *const _)
                .collect::<Vec<_>>()
        );
        assert_eq!(std::mem::size_of::<DrawItem>(), 24);
        assert!(std::mem::size_of::<DrawItem>() < std::mem::size_of::<EditableDraw>());
    }

    #[test]
    fn compose_frame_stats_report_only_opted_in_sort_fallbacks() {
        let mut high = test_sprite(SpriteSource::Solid);
        let mut low = test_sprite(SpriteSource::Solid);
        let Actor::Sprite { z: high_z, .. } = &mut high else {
            unreachable!();
        };
        let Actor::Sprite { z: low_z, .. } = &mut low else {
            unreachable!();
        };
        *high_z = 2;
        *low_z = 1;
        let actors = [high, low];
        let metrics = Metrics {
            left: 0.0,
            right: 100.0,
            top: 100.0,
            bottom: 0.0,
        };
        let fonts = font::FontMap::default();
        let mut text_cache = TextLayoutCache::default();
        let mut scratch = ComposeScratch::default();

        scratch.begin_frame_stats(true);
        let mut render = build_screen_cached_with_scratch(
            &actors,
            [0.0; 4],
            &metrics,
            &fonts,
            0.0,
            &mut text_cache,
            &mut scratch,
        );
        assert!(scratch.frame_stats().sort_fallback);
        assert_eq!(render.ops.len(), 2);
        assert_eq!(sprite_run(&render, 0).instance_count, 1);
        assert_eq!(sprite_run(&render, 1).instance_count, 1);

        scratch.recycle_frame(&mut render);
        scratch.begin_frame_stats(false);
        let _ = build_screen_cached_with_scratch(
            &actors,
            [0.0; 4],
            &metrics,
            &fonts,
            0.0,
            &mut text_cache,
            &mut scratch,
        );
        assert_eq!(scratch.frame_stats(), super::ComposeFrameStats::default());
    }

    #[test]
    fn composed_render_sort_preserves_monotonic_draw_order_within_layers() {
        const Z_VALUES: [i16; 7] = [-30_000, -8, 0, 1, 90, 2_101, 32_000];
        let mut items = (0usize..512)
            .map(|index| {
                DrawItem::synthetic(
                    Z_VALUES[index.wrapping_mul(11).wrapping_add(3) % Z_VALUES.len()],
                    index as u32,
                    index as u32,
                )
            })
            .collect::<Vec<_>>();
        let mut expected = items.clone();
        expected.sort_unstable_by_key(|item| item.sort_key());

        sort_composed_draw_items(&mut items, &mut ComposeScratch::default());

        assert_eq!(
            items
                .iter()
                .map(|item| (item.z, item.order, item.payload_index))
                .collect::<Vec<_>>(),
            expected
                .iter()
                .map(|item| (item.z, item.order, item.payload_index))
                .collect::<Vec<_>>()
        );

        let mut shadow_interleaved = [(5, 0), (5, 1), (4, 0), (4, 1), (5, 2), (4, 2)]
            .into_iter()
            .enumerate()
            .map(|(index, (z, order))| DrawItem::synthetic(z, order, index as u32))
            .collect::<Vec<_>>();

        sort_composed_draw_items(&mut shadow_interleaved, &mut ComposeScratch::default());

        assert_eq!(
            shadow_interleaved
                .iter()
                .map(|item| (item.z, item.order))
                .collect::<Vec<_>>(),
            vec![(4, 0), (4, 1), (4, 2), (5, 0), (5, 1), (5, 2)]
        );
    }

    #[test]
    fn composed_render_sort_falls_back_for_wide_or_unordered_layers() {
        fn assert_matches_key_sort(mut items: Vec<DrawItem>) {
            let mut expected = items.clone();
            expected.sort_unstable_by_key(|item| item.sort_key());

            sort_composed_draw_items(&mut items, &mut ComposeScratch::default());

            let fingerprint = |items: &[DrawItem]| {
                items
                    .iter()
                    .map(|item| (item.z, item.order, item.payload_index))
                    .collect::<Vec<_>>()
            };
            assert_eq!(fingerprint(&items), fingerprint(&expected));
        }

        assert_matches_key_sort(
            (0usize..130)
                .map(|index| {
                    DrawItem::synthetic(
                        (index.wrapping_mul(17) % 65) as i16,
                        index as u32,
                        index as u32,
                    )
                })
                .collect(),
        );
        assert_matches_key_sort(
            [(2, 4), (1, 3), (2, 2), (1, 1)]
                .into_iter()
                .enumerate()
                .map(|(index, (z, order))| DrawItem::synthetic(z, order, index as u32))
                .collect(),
        );
    }

    #[test]
    fn sparse_draw_item_sort_matches_legacy_order() {
        const Z_VALUES: [i16; 9] = [-30_000, -99, 0, 50, 90, 91, 2_101, 8_000, 32_000];
        let source: Vec<_> = (0usize..1_024)
            .map(|index| {
                DrawItem::synthetic(
                    Z_VALUES[index.wrapping_mul(17).wrapping_add(5) % Z_VALUES.len()],
                    index as u32,
                    index as u32,
                )
            })
            .collect();
        let mut legacy = source.clone();
        let mut indirect = source;

        sort_draw_items_legacy(&mut legacy, &mut ComposeScratch::default());
        sort_draw_items(&mut indirect, &mut ComposeScratch::default());

        assert_eq!(
            indirect
                .iter()
                .map(|item| (item.z, item.order, item.payload_index))
                .collect::<Vec<_>>(),
            legacy
                .iter()
                .map(|item| (item.z, item.order, item.payload_index))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn sparse_render_sort_reuses_lookup_and_matches_legacy_fallbacks() {
        fn draw_items(z_values: &[i16], count: usize) -> Vec<DrawItem> {
            (0..count)
                .map(|index| {
                    DrawItem::synthetic(
                        z_values[index.wrapping_mul(17).wrapping_add(5) % z_values.len()],
                        index as u32,
                        index as u32,
                    )
                })
                .collect()
        }

        fn assert_matches_legacy(source: Vec<DrawItem>, scratch: &mut ComposeScratch) {
            let mut legacy = source.clone();
            let mut optimized = source;

            sort_draw_items_legacy(&mut legacy, &mut ComposeScratch::default());
            sort_draw_items(&mut optimized, scratch);

            let fingerprint = |items: &[DrawItem]| {
                items
                    .iter()
                    .map(|item| (item.z, item.order, item.payload_index))
                    .collect::<Vec<_>>()
            };
            assert_eq!(fingerprint(&optimized), fingerprint(&legacy));
        }

        let mut scratch = ComposeScratch::default();
        assert_matches_legacy(draw_items(&[-30_000, -1_000, 0, 30_000], 256), &mut scratch);
        assert_matches_legacy(draw_items(&[-29_999, -999, 1, 29_999], 256), &mut scratch);

        let sixty_five_layers: Vec<_> = (0..65)
            .map(|index| (-30_000 + index * 900) as i16)
            .collect();
        assert_matches_legacy(draw_items(&sixty_five_layers, 130), &mut scratch);

        let descending_within_layers = vec![
            test_draw_item(-30_000, 4),
            test_draw_item(30_000, 3),
            test_draw_item(-30_000, 2),
            test_draw_item(30_000, 1),
        ];
        assert_matches_legacy(descending_within_layers, &mut scratch);
    }

    #[test]
    fn shadowed_textured_mesh_keeps_geom_cache_key() {
        const CACHE_KEY: TMeshCacheKey = 77;
        let metrics = Metrics {
            left: 0.0,
            right: 100.0,
            top: 100.0,
            bottom: 0.0,
        };
        let mesh = Actor::TexturedMesh {
            align: [0.0, 0.0],
            offset: [10.0, 20.0],
            world_z: 0.0,
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            local_transform: Matrix4::IDENTITY,
            texture: Arc::from("mesh"),
            tint: [0.25, 0.5, 0.75, 0.8],
            glow: [1.0, 1.0, 1.0, 0.0],
            vertices: Arc::from(vec![TexturedMeshVertex::default(); 3]),
            geom_cache_key: CACHE_KEY,
            uv_scale: [1.0, 1.0],
            uv_offset: [0.0, 0.0],
            uv_tex_shift: [0.0, 0.0],
            depth_test: false,
            visible: true,
            blend: BlendMode::Alpha,
            z: 5,
        };
        let actors = [Actor::Shadow {
            len: [4.0, 3.0],
            color: [0.5, 0.25, 0.75, 0.5],
            child: Box::new(mesh),
        }];
        let fonts = font::FontMap::default();
        let render = build_screen(&actors, [0.0, 0.0, 0.0, 1.0], &metrics, &fonts, 0.0);

        assert_eq!(render.ops.len(), 2);
        let (_, shadow_instance, shadow_geometry) = tmesh_draw(&render, 0);
        let (_, original_instance, original_geometry) = tmesh_draw(&render, 1);
        assert_eq!(shadow_geometry.cache_key, CACHE_KEY);
        assert_eq!(original_geometry.cache_key, CACHE_KEY);
        assert_eq!(original_instance.tint, [0.25, 0.5, 0.75, 0.8]);
        assert_eq!(shadow_instance.tint, [0.125, 0.125, 0.5625, 0.4]);
    }

    #[test]
    fn retained_frame_composes_once_then_reuses_compact_output() {
        let metrics = Metrics {
            left: 0.0,
            right: 100.0,
            top: 100.0,
            bottom: 0.0,
        };
        let sprite = Actor::Sprite {
            align: [0.0, 0.0],
            offset: [12.0, 18.0],
            world_z: 0.0,
            size: [SizeSpec::Px(10.0), SizeSpec::Px(14.0)],
            source: SpriteSource::Solid,
            tint: [0.8, 0.6, 0.4, 0.5],
            glow: [0.0; 4],
            z: 7,
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
            shadow_len: [0.0; 2],
            shadow_color: [0.0; 4],
            effect: Default::default(),
        };
        let frame = Arc::new(RetainedActorFrame::new(vec![sprite]));
        let actors = [Actor::RetainedFrame {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Fill, SizeSpec::Fill],
            frame,
            z: 3,
            tint: [0.5, 0.25, 1.0, 0.5],
            blend: None,
            visible: true,
        }];
        let fonts = font::FontMap::default();
        let mut text_cache = TextLayoutCache::new(1);
        let mut scratch = ComposeScratch::default();

        let mut first = build_screen_cached_with_scratch(
            &actors,
            [0.0; 4],
            &metrics,
            &fonts,
            0.0,
            &mut text_cache,
            &mut scratch,
        );
        let first_instance = first.sprite_instances[0];
        assert_eq!(sprite_run(&first, 0).instance_count, 1);
        assert_eq!(scratch.retained_frame_stats().misses, 1);
        assert_eq!(scratch.retained_frame_stats().entries, 1);
        scratch.recycle_frame(&mut first);

        let second = build_screen_cached_with_scratch(
            &actors,
            [0.0; 4],
            &metrics,
            &fonts,
            1.0,
            &mut text_cache,
            &mut scratch,
        );
        assert_eq!(second.ops.len(), 1);
        assert_eq!(sprite_run(&second, 0).instance_count, 1);
        assert_eq!(second.sprite_instances, vec![first_instance]);
        assert_eq!(second.sprite_instances[0].tint, [0.4, 0.15, 0.4, 0.25]);
        let stats = scratch.retained_frame_stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.entries, 1);
    }

    #[test]
    fn retained_frame_clear_starts_a_fresh_song_working_set() {
        let mut scratch = ComposeScratch::default();
        scratch.retained_frames.stats.hits = 4;
        scratch.retained_frames.stats.misses = 2;

        scratch.clear_retained_frames();

        assert_eq!(
            scratch.retained_frame_stats(),
            super::RetainedFrameCacheStats::default()
        );
    }

    #[test]
    fn mesh_and_shared_frame_tints_modulate_shared_vertices() {
        let metrics = Metrics {
            left: 0.0,
            right: 100.0,
            top: 100.0,
            bottom: 0.0,
        };
        let mesh = Actor::Mesh {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Px(1.0), SizeSpec::Px(1.0)],
            tint: [0.5, 0.5, 0.5, 0.5],
            vertices: Arc::from(vec![MeshVertex {
                pos: [0.0, 0.0],
                color: [0.8, 0.6, 0.4, 0.5],
            }]),
            visible: true,
            blend: BlendMode::Alpha,
            z: 0,
        };
        let actors = [Actor::SharedFrame {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Fill, SizeSpec::Fill],
            children: Arc::from(vec![mesh]),
            background: None,
            z: 0,
            tint: [0.5, 0.25, 0.1, 0.5],
            blend: None,
        }];
        let fonts = font::FontMap::default();
        let render = build_screen(&actors, [0.0, 0.0, 0.0, 1.0], &metrics, &fonts, 0.0);

        let run = mesh_run(&render, 0);
        assert_eq!(run.vertex_count, 1);
        assert_eq!(
            render.mesh_vertices[run.vertex_start as usize].color,
            [0.2, 0.075, 0.020000001, 0.125]
        );
    }

    #[test]
    fn reusable_textured_mesh_preserves_shared_vec_storage() {
        let metrics = Metrics {
            left: 0.0,
            right: 100.0,
            top: 100.0,
            bottom: 0.0,
        };
        let vertices = Arc::new(vec![TexturedMeshVertex::default(); 6]);
        let actor = Actor::ReusableTexturedMesh {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            world_z: 0.0,
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            local_transform: Matrix4::IDENTITY,
            texture: Arc::from("reusable"),
            tint: [1.0; 4],
            glow: [1.0, 1.0, 1.0, 0.0],
            vertices: Arc::clone(&vertices),
            geom_cache_key: deadlib_render::INVALID_TMESH_CACHE_KEY,
            uv_scale: [1.0; 2],
            uv_offset: [0.0; 2],
            uv_tex_shift: [0.0; 2],
            depth_test: true,
            visible: true,
            blend: BlendMode::Alpha,
            z: 0,
        };
        let render = build_screen(
            &[actor],
            [0.0, 0.0, 0.0, 1.0],
            &metrics,
            &font::FontMap::default(),
            0.0,
        );

        let (_, _, geometry) = tmesh_draw(&render, 0);
        let deadlib_render::TexturedMeshVertices::Reusable(render_vertices) = &geometry.vertices
        else {
            panic!("reusable actor should preserve reusable renderer storage");
        };
        assert!(Arc::ptr_eq(render_vertices, &vertices));
    }

    #[test]
    fn shared_frame_tint_modulates_textured_mesh() {
        let metrics = Metrics {
            left: 0.0,
            right: 100.0,
            top: 100.0,
            bottom: 0.0,
        };
        let mesh = Actor::TexturedMesh {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            world_z: 0.0,
            size: [SizeSpec::Px(1.0), SizeSpec::Px(1.0)],
            local_transform: Matrix4::IDENTITY,
            texture: Arc::from("mesh"),
            tint: [0.8, 0.6, 0.4, 0.5],
            glow: [0.5, 0.25, 1.0, 0.4],
            vertices: Arc::from(vec![TexturedMeshVertex::default(); 3]),
            geom_cache_key: INVALID_TMESH_CACHE_KEY,
            uv_scale: [1.0, 1.0],
            uv_offset: [0.0, 0.0],
            uv_tex_shift: [0.0, 0.0],
            depth_test: false,
            visible: true,
            blend: BlendMode::Alpha,
            z: 0,
        };
        let actors = [Actor::SharedFrame {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Fill, SizeSpec::Fill],
            children: Arc::from(vec![mesh]),
            background: None,
            z: 0,
            tint: [0.5, 0.25, 0.1, 0.5],
            blend: None,
        }];
        let fonts = font::FontMap::default();
        let render = build_screen(&actors, [0.0, 0.0, 0.0, 1.0], &metrics, &fonts, 0.0);

        assert_eq!(render.ops.len(), 1);
        let (run, base, _) = tmesh_draw(&render, 0);
        assert_eq!(run.instance_count, 2);
        let glow = &render.tmesh_instances[run.instance_start as usize + 1];
        assert_eq!(base.tint, [0.4, 0.15, 0.040000003, 0.25]);
        assert_eq!(glow.tint, [0.25, 0.0625, 0.1, 0.2]);
    }

    #[test]
    fn shared_frame_tint_modulates_sprite_glow() {
        let metrics = Metrics {
            left: 0.0,
            right: 100.0,
            top: 100.0,
            bottom: 0.0,
        };
        let sprite = Actor::Sprite {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            world_z: 0.0,
            size: [SizeSpec::Px(10.0), SizeSpec::Px(10.0)],
            source: SpriteSource::Solid,
            tint: [0.8, 0.6, 0.4, 0.5],
            glow: [0.5, 0.25, 1.0, 0.4],
            z: 0,
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
            shadow_color: [0.0, 0.0, 0.0, 0.0],
            effect: Default::default(),
        };
        let actors = [Actor::SharedFrame {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Fill, SizeSpec::Fill],
            children: Arc::from(vec![sprite]),
            background: None,
            z: 0,
            tint: [0.5, 0.25, 0.1, 0.5],
            blend: None,
        }];
        let fonts = font::FontMap::default();
        let render = build_screen(&actors, [0.0, 0.0, 0.0, 1.0], &metrics, &fonts, 0.0);

        assert_eq!(render.ops.len(), 1);
        let run = sprite_run(&render, 0);
        assert_eq!(run.instance_count, 2);
        let index = run.instance_start + 1;
        assert_eq!(
            render.sprite_instances[index as usize].tint,
            [0.25, 0.0625, 0.1, 0.2]
        );
    }

    #[test]
    fn simple_left_aligned_text_batches_into_textured_mesh() {
        let metrics = Metrics {
            left: 0.0,
            right: 200.0,
            top: 100.0,
            bottom: 0.0,
        };
        let actors = [Actor::Text {
            align: [0.0, 0.0],
            offset: [10.0, 20.0],
            local_transform: Matrix4::IDENTITY,
            color: [0.5, 0.75, 1.0, 1.0],
            stroke_color: None,
            glow: [0.0; 4],
            font: "test",
            content: TextContent::static_str("AB"),
            attributes: TextAttributes::default(),
            align_text: TextAlign::Left,
            z: 3,
            scale: [1.0, 1.0],
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
            effect: Default::default(),
        }];
        let fonts = font::FontMap::from_iter([("test", test_font())]);
        let render = build_screen(&actors, [0.0, 0.0, 0.0, 1.0], &metrics, &fonts, 0.0);

        assert_eq!(render.ops.len(), 1);
        let (_, instance, geometry) = tmesh_draw(&render, 0);
        assert_eq!(instance.tint, [0.5, 0.75, 1.0, 1.0]);
        assert_eq!(geometry.vertices.len(), 12);
        assert_ne!(geometry.cache_key, INVALID_TMESH_CACHE_KEY);
    }

    #[test]
    fn clipped_left_aligned_batched_text_stays_textured_mesh() {
        let metrics = Metrics {
            left: 0.0,
            right: 200.0,
            top: 100.0,
            bottom: 0.0,
        };
        let actors = [Actor::Text {
            align: [0.0, 0.0],
            offset: [10.0, 20.0],
            local_transform: Matrix4::IDENTITY,
            color: [1.0; 4],
            stroke_color: None,
            glow: [0.0; 4],
            font: "test",
            content: TextContent::static_str("AB"),
            attributes: TextAttributes::default(),
            align_text: TextAlign::Left,
            z: 0,
            scale: [1.0, 1.0],
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
            clip: Some([10.0, 20.0, 4.0, 10.0]),
            mask_dest: false,
            blend: BlendMode::Alpha,
            shadow_len: [0.0, 0.0],
            shadow_color: [0.0, 0.0, 0.0, 0.5],
            effect: Default::default(),
        }];
        let fonts = font::FontMap::from_iter([("test", test_font())]);
        let render = build_screen(&actors, [0.0, 0.0, 0.0, 1.0], &metrics, &fonts, 0.0);

        assert_eq!(render.ops.len(), 1);
        let (_, _, geometry) = tmesh_draw(&render, 0);
        assert!(!geometry.vertices.is_empty());
        assert_eq!(geometry.cache_key, INVALID_TMESH_CACHE_KEY);
    }

    #[test]
    fn fully_inside_clipped_text_keeps_cached_mesh() {
        let metrics = Metrics {
            left: 0.0,
            right: 200.0,
            top: 100.0,
            bottom: 0.0,
        };
        let actors = [Actor::Text {
            align: [0.0, 0.0],
            offset: [10.0, 20.0],
            local_transform: Matrix4::IDENTITY,
            color: [1.0; 4],
            stroke_color: None,
            glow: [0.0; 4],
            font: "test",
            content: TextContent::static_str("AB"),
            attributes: TextAttributes::default(),
            align_text: TextAlign::Left,
            z: 0,
            scale: [1.0, 1.0],
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
            clip: Some([0.0, 0.0, 200.0, 100.0]),
            mask_dest: false,
            blend: BlendMode::Alpha,
            shadow_len: [0.0, 0.0],
            shadow_color: [0.0, 0.0, 0.0, 0.5],
            effect: Default::default(),
        }];
        let fonts = font::FontMap::from_iter([("test", test_font())]);
        let render = build_screen(&actors, [0.0, 0.0, 0.0, 1.0], &metrics, &fonts, 0.0);

        assert_eq!(render.ops.len(), 1);
        assert_ne!(tmesh_draw(&render, 0).2.cache_key, INVALID_TMESH_CACHE_KEY);
    }

    #[test]
    fn centered_text_batches_into_textured_mesh() {
        let metrics = Metrics {
            left: 0.0,
            right: 200.0,
            top: 100.0,
            bottom: 0.0,
        };
        let actors = [Actor::Text {
            align: [0.0, 0.0],
            offset: [10.0, 20.0],
            local_transform: Matrix4::IDENTITY,
            color: [1.0; 4],
            stroke_color: None,
            glow: [0.0; 4],
            font: "test",
            content: TextContent::static_str("AB"),
            attributes: TextAttributes::default(),
            align_text: TextAlign::Center,
            z: 0,
            scale: [1.0, 1.0],
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
            effect: Default::default(),
        }];
        let fonts = font::FontMap::from_iter([("test", test_font())]);
        let render = build_screen(&actors, [0.0, 0.0, 0.0, 1.0], &metrics, &fonts, 0.0);

        assert_eq!(render.ops.len(), 1);
        let (_, _, geometry) = tmesh_draw(&render, 0);
        assert_eq!(geometry.vertices.len(), 12);
        assert_ne!(geometry.cache_key, INVALID_TMESH_CACHE_KEY);
    }

    #[test]
    fn attributed_text_batches_into_transient_textured_mesh() {
        let metrics = Metrics {
            left: 0.0,
            right: 200.0,
            top: 100.0,
            bottom: 0.0,
        };
        let actors = [Actor::Text {
            align: [0.0, 0.0],
            offset: [10.0, 20.0],
            local_transform: Matrix4::IDENTITY,
            color: [1.0; 4],
            stroke_color: None,
            glow: [0.0; 4],
            font: "test",
            content: TextContent::static_str("AB"),
            attributes: vec![TextAttribute {
                start: 1,
                length: 1,
                color: [0.0, 1.0, 0.0, 1.0],
                vertex_colors: None,
                glow: None,
            }]
            .into(),
            align_text: TextAlign::Left,
            z: 0,
            scale: [1.0, 1.0],
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
            effect: Default::default(),
        }];
        let fonts = font::FontMap::from_iter([("test", test_font())]);
        let render = build_screen(&actors, [0.0, 0.0, 0.0, 1.0], &metrics, &fonts, 0.0);

        assert_eq!(render.ops.len(), 1);
        let (_, _, geometry) = tmesh_draw(&render, 0);
        assert_eq!(geometry.vertices.len(), 12);
        assert_eq!(geometry.cache_key, INVALID_TMESH_CACHE_KEY);
        assert_eq!(geometry.vertices[0].color, [1.0; 4]);
        assert_eq!(geometry.vertices[6].color, [0.0, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn attributed_text_applies_corner_colors_to_glyph_vertices() {
        let metrics = Metrics {
            left: 0.0,
            right: 200.0,
            top: 100.0,
            bottom: 0.0,
        };
        let colors = [
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
            [1.0, 1.0, 0.0, 1.0],
        ];
        let actors = [Actor::Text {
            align: [0.0, 0.0],
            offset: [10.0, 20.0],
            local_transform: Matrix4::IDENTITY,
            color: [1.0; 4],
            stroke_color: None,
            glow: [0.0; 4],
            font: "test",
            content: TextContent::static_str("A"),
            attributes: vec![TextAttribute {
                start: 0,
                length: 1,
                color: colors[0],
                vertex_colors: Some(colors),
                glow: None,
            }]
            .into(),
            align_text: TextAlign::Left,
            z: 0,
            scale: [1.0, 1.0],
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
            effect: Default::default(),
        }];
        let fonts = font::FontMap::from_iter([("test", test_font())]);
        let render = build_screen(&actors, [1.0; 4], &metrics, &fonts, 0.0);

        let vertices = &tmesh_draw(&render, 0).2.vertices;
        assert_eq!(vertices[0].color, colors[0]);
        assert_eq!(vertices[1].color, colors[2]);
        assert_eq!(vertices[2].color, colors[3]);
        assert_eq!(vertices[3].color, colors[0]);
        assert_eq!(vertices[4].color, colors[3]);
        assert_eq!(vertices[5].color, colors[1]);
    }

    #[test]
    fn jittered_text_uses_transient_offset_vertices() {
        let metrics = Metrics {
            left: 0.0,
            right: 200.0,
            top: 100.0,
            bottom: 0.0,
        };
        let mut actor = Actor::Text {
            align: [0.0, 0.0],
            offset: [10.0, 20.0],
            local_transform: Matrix4::IDENTITY,
            color: [1.0; 4],
            stroke_color: None,
            glow: [0.0; 4],
            font: "test",
            content: TextContent::static_str("A"),
            attributes: TextAttributes::default(),
            align_text: TextAlign::Left,
            z: 0,
            scale: [1.0, 1.0],
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
            effect: Default::default(),
        };
        let fonts = font::FontMap::from_iter([("test", test_font())]);
        let base = build_screen(&[actor.clone()], [1.0; 4], &metrics, &fonts, 0.25);
        let Actor::Text { jitter, .. } = &mut actor else {
            panic!("expected text actor");
        };
        *jitter = true;
        let jittered = build_screen(&[actor], [1.0; 4], &metrics, &fonts, 0.25);

        let base_vertices = &tmesh_draw(&base, 0).2.vertices;
        let (_, _, jittered_geometry) = tmesh_draw(&jittered, 0);
        let jittered_vertices = &jittered_geometry.vertices;
        assert_eq!(jittered_geometry.cache_key, INVALID_TMESH_CACHE_KEY);
        assert_ne!(jittered_vertices[0].pos, base_vertices[0].pos);
    }

    #[test]
    fn distorted_text_uses_transient_corner_offsets() {
        let metrics = Metrics {
            left: 0.0,
            right: 200.0,
            top: 100.0,
            bottom: 0.0,
        };
        let mut actor = Actor::Text {
            align: [0.0, 0.0],
            offset: [10.0, 20.0],
            local_transform: Matrix4::IDENTITY,
            color: [1.0; 4],
            stroke_color: None,
            glow: [0.0; 4],
            font: "test",
            content: TextContent::static_str("A"),
            attributes: TextAttributes::default(),
            align_text: TextAlign::Left,
            z: 0,
            scale: [1.0, 1.0],
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
            effect: Default::default(),
        };
        let fonts = font::FontMap::from_iter([("test", test_font())]);
        let base = build_screen(&[actor.clone()], [1.0; 4], &metrics, &fonts, 0.0);
        let Actor::Text { distortion, .. } = &mut actor else {
            panic!("expected text actor");
        };
        *distortion = 0.5;
        let distorted = build_screen(&[actor], [1.0; 4], &metrics, &fonts, 0.0);

        let base_vertices = &tmesh_draw(&base, 0).2.vertices;
        let (_, _, distorted_geometry) = tmesh_draw(&distorted, 0);
        let distorted_vertices = &distorted_geometry.vertices;
        assert_eq!(distorted_geometry.cache_key, INVALID_TMESH_CACHE_KEY);
        assert!(
            base_vertices
                .iter()
                .zip(distorted_vertices.iter())
                .any(|(base, distorted)| base.pos != distorted.pos)
        );
    }

    #[test]
    fn attributed_text_keeps_colors_across_texture_batches() {
        let metrics = Metrics {
            left: 0.0,
            right: 200.0,
            top: 100.0,
            bottom: 0.0,
        };
        let actors = [Actor::Text {
            align: [0.0, 0.0],
            offset: [10.0, 20.0],
            local_transform: Matrix4::IDENTITY,
            color: [1.0; 4],
            stroke_color: None,
            glow: [0.0; 4],
            font: "test",
            content: TextContent::static_str("AB"),
            attributes: vec![TextAttribute {
                start: 1,
                length: 1,
                color: [0.0, 1.0, 0.0, 1.0],
                vertex_colors: None,
                glow: None,
            }]
            .into(),
            align_text: TextAlign::Left,
            z: 0,
            scale: [1.0, 1.0],
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
            effect: Default::default(),
        }];
        let fonts = font::FontMap::from_iter([("test", test_font_split_pages())]);
        let render = build_screen(&actors, [0.0, 0.0, 0.0, 1.0], &metrics, &fonts, 0.0);

        assert_eq!(render.ops.len(), 2);
        let (_, _, first) = tmesh_draw(&render, 0);
        let (_, _, second) = tmesh_draw(&render, 1);
        assert_eq!(first.cache_key, INVALID_TMESH_CACHE_KEY);
        assert_eq!(second.cache_key, INVALID_TMESH_CACHE_KEY);
        assert_eq!(first.vertices[0].color, [1.0; 4]);
        assert_eq!(second.vertices[0].color, [0.0, 1.0, 0.0, 1.0]);
    }
}
