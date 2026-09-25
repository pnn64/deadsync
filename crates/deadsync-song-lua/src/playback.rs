// Chart actor playback and capture semantics, independent of theme and asset loaders.
use crate::gameplay::{
    SongLuaRuntimeOverlayStateDelta, song_lua_runtime_column_offset_windows,
    song_lua_runtime_ease_windows, song_lua_runtime_mod_windows,
};
use crate::{
    SongLuaCapturedActor, SongLuaCapturedChildActor, SongLuaOverlayBlendMode,
    SongLuaOverlayCommandBlock, SongLuaOverlayMeshVertex, SongLuaOverlayMessageCommand,
    SongLuaOverlayModelDraw, SongLuaOverlayState, SongLuaProxyTarget, SongLuaTextGlowMode,
};
use crate::{apply_overlay_delta, overlay_state_lerp};
use deadlib_assets::AssetManager;
use deadlib_present::actors::{
    Actor, FlatDraw, SharedActorFrameScratch, SizeSpec, SpriteSource, TextAttribute,
    TextAttributes, TextContent,
};
use deadlib_present::anim::EffectState;
use deadlib_present::compose::{ActorSegment, ActorXFold, FlatProxyStyle, TextLayoutCache};
use deadlib_present::font;
use deadlib_present::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use deadlib_render_core::{
    BlendMode, INVALID_TMESH_CACHE_KEY, MeshVertex, TMeshCacheKey, TextureHandle,
    TexturedMeshVertex, render_target_sample_handle, render_target_texture_handle,
};
use deadsync_chart::SongData;
use deadsync_core::input::{MAX_COLS, MAX_PLAYERS};
use deadsync_gameplay::{
    SongLuaEase, SongLuaOverlayMessageRuntime, SongLuaRuntimeVisuals, song_lua_ease_factor,
};
use deadsync_notefield::ViewOverride as NotefieldViewOverride;
use deadsync_notefield::{
    CapturedActorSource, FieldPlacement, ModelMeshCache, NotefieldCameraCache,
    ProxyCaptureRequests, SongLuaPlayerTransformRequest, ViewOverride, actor_from_flat_draw,
    noteskin_model_actor_from_draw, noteskin_model_actor_from_draw_cached,
    song_lua_player_skew_x_matrix, song_lua_player_skew_y_matrix, song_lua_player_transform_matrix,
    song_lua_player_y_fold_actor,
};
use deadsync_noteskin::ModelDrawState;
use deadsync_noteskin::NoteskinSlot;
use deadsync_rules::judgment::JudgeGrade;
use glam::{Mat2 as Matrix2, Mat4 as Matrix4, Vec2 as Vector2, Vec3 as Vector3, Vec4 as Vector4};
use std::collections::HashSet;
use std::hash::Hasher;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Arc, OnceLock};
use std::time::Instant;
type SongLuaOverlayModelLayer = crate::SongLuaOverlayModelLayer<TexturedMeshVertex>;
type SongLuaOverlayKind<S> = crate::SongLuaOverlayKind<S, TexturedMeshVertex, TextAttribute>;
type SongLuaOverlayActor<S> = crate::SongLuaOverlayActor<SongLuaOverlayKind<S>>;
type CompiledSongLua<S> = crate::CompiledSongLua<SongLuaOverlayActor<S>>;
pub type GameplayCoreState<P, S> = deadsync_gameplay::GameplayRuntimeState<
    P,
    SongLuaOverlayActor<S>,
    SongLuaCapturedActor,
    SongLuaRuntimeOverlayStateDelta,
>;
#[path = "playback/prepare.rs"]
mod prepare;
use deadlib_assets::present_dsl::cover_sprite;
pub use prepare::{PreparedGameplaySongLua, prepare_song_lua, song_lua_sound_paths};
use prepare::{SongLuaOverlayEaseWindowRuntime, SongLuaSoundEvent};
use smallvec::SmallVec;
static NEXT_SONG_LUA_AFT_ID: AtomicU64 = AtomicU64::new(1);
const SONG_LUA_CHILD_ORDER_STATIC: u8 = 0;
const SONG_LUA_CHILD_ORDER_DRAW: u8 = 1;
const SONG_LUA_CHILD_ORDER_Z: u8 = 2;

/// Song-lifetime state/order execution plan for one compiled overlay tree.
///
/// The gameplay frame thread owns it without synchronization. Screen entry
/// sizes every child list, records the only actors whose state can change,
/// propagates that set through descendants, and flattens immutable root order.
/// Gameplay performs no insertion, growth, eviction, pruning, or destruction;
/// dynamic ordering saturates at the compiled actor count and only re-sorts a
/// bounded sibling list after a key change. All storage is released at the
/// gameplay transition.
#[derive(Default)]
struct SongLuaOverlayOrderCache {
    child_lists: Vec<Vec<usize>>,
    dynamic_draw_order: Vec<bool>,
    // Built once at song load; at most one entry per captured grade command.
    // The frame loop updates only effect epochs and reads judgment slots.
    tap_commands: Vec<SongLuaTapCommand>,
    // Song-lifetime execution plan: only these actors can change local state,
    // and only these composed states depend on changing local/ancestor state.
    dynamic_local_indices: Box<[usize]>,
    dynamic_composed_indices: Box<[usize]>,
    // Most Song Lua trees never mutate ordering fields. Flatten those trees at
    // screen entry so the frame loop can copy one contiguous index slice.
    static_root_order: Option<Box<[usize]>>,
    // Update tracks are immutable and sorted by overlay. Store their ranges
    // once, then retain one sample cursor per track across gameplay frames.
    // Nearby frames advance from the prior boundary; seeks retain logarithmic
    // fallback through `partition_point_from_hint`.
    update_actor_index: SongLuaOverlayIndex,
    update_ranges: Box<[std::ops::Range<usize>]>,
    visible_update_indices: SmallVec<[usize; 8]>,
    update_cursors: Vec<usize>,
    sort_modes: Vec<u8>,
    // Dynamic-capable lists usually keep the same keys for many frames. Remember
    // those keys so we only pay O(n log n) when their effective order changes.
    last_draw_orders: Vec<i32>,
    last_z_keys: Vec<u32>,
}

struct SongLuaTapCommand {
    overlay: usize,
    command: usize,
    player: usize,
    column: usize,
    grade: JudgeGrade,
    glow_started_at: Option<f32>,
}

// Built once per song or visual layer so frame rendering does not repeatedly
// walk actor parents or resolve ActorFrameTexture names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
struct SongLuaOverlayIndex(Option<NonZeroUsize>);

impl SongLuaOverlayIndex {
    fn new(index: Option<usize>) -> Self {
        Self(
            index
                .and_then(|index| index.checked_add(1))
                .and_then(NonZeroUsize::new),
        )
    }

    fn get(self) -> Option<usize> {
        self.0.map(|index| index.get() - 1)
    }
}

/// Song-lifetime topology lookup for Song Lua overlay rendering.
///
/// The gameplay screen owns one immutable index for the root overlays and one
/// per visual layer. Construction runs during screen entry, groups AFT sprites
/// by capture name and allocates a stable render-target handle for each AFT.
/// Static camera scopes are resolved at screen entry; trees whose camera FOV can
/// change retain the indexed ancestor fallback. There is no synchronization,
/// gameplay allocation, miss, eviction, or pruning path. Storage is bounded by
/// the compiled overlay count and is released with the gameplay screen.
#[derive(Default)]
struct SongLuaOverlayTopologyIndex {
    aft_ancestors: Vec<SongLuaOverlayIndex>,
    aft_sprite_targets: Vec<SongLuaOverlayIndex>,
    aft_sprite_indices: Box<[usize]>,
    aft_texture_handles: Vec<TextureHandle>,
    camera_ancestors: Vec<SongLuaOverlayIndex>,
    camera_states: Vec<SongLuaOverlayIndex>,
    dynamic_camera_scope: bool,
}

impl SongLuaOverlayTopologyIndex {
    fn new<S: NoteskinSlot + Clone>(overlays: &[SongLuaOverlayActor<S>]) -> Self {
        let aft_ancestors = (0..overlays.len())
            .map(|index| SongLuaOverlayIndex::new(song_lua_overlay_aft_ancestor(overlays, index)))
            .collect::<Vec<_>>();
        let aft_sprite_indices = overlays
            .iter()
            .enumerate()
            .filter_map(|(index, overlay)| {
                matches!(overlay.kind, SongLuaOverlayKind::AftSprite { .. }).then_some(index)
            })
            .collect::<Box<[_]>>();
        let mut aft_sprite_targets = vec![SongLuaOverlayIndex::default(); overlays.len()];
        for &index in &aft_sprite_indices {
            let SongLuaOverlayKind::AftSprite { capture_name } = &overlays[index].kind else {
                unreachable!("AFT sprite index was built from the same immutable actor list");
            };
            aft_sprite_targets[index] = SongLuaOverlayIndex::new(
                song_lua_overlay_capture_index_by_name(overlays, capture_name),
            );
        }
        let aft_texture_handles = overlays
            .iter()
            .map(|overlay| {
                matches!(overlay.kind, SongLuaOverlayKind::ActorFrameTexture { .. })
                    .then(|| {
                        render_target_texture_handle(
                            NEXT_SONG_LUA_AFT_ID.fetch_add(1, AtomicOrdering::Relaxed),
                        )
                    })
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        let camera_ancestors = overlays
            .iter()
            .map(|overlay| {
                SongLuaOverlayIndex::new(song_lua_overlay_camera_ancestor(
                    overlays,
                    overlay.parent_index,
                ))
            })
            .collect();
        let mut camera_states = vec![SongLuaOverlayIndex::default(); overlays.len()];
        Self::fill_camera_states(
            overlays,
            |index| overlays.get(index).map(|overlay| overlay.initial_state),
            &mut camera_states,
        );
        let dynamic_camera_scope = overlays.iter().any(|overlay| {
            matches!(
                overlay.kind,
                SongLuaOverlayKind::ActorFrame | SongLuaOverlayKind::ActorFrameTexture { .. }
            ) && overlay
                .message_commands
                .iter()
                .any(|command| command.blocks.iter().any(|block| block.delta.fov.is_some()))
        });
        Self {
            aft_ancestors,
            aft_sprite_targets,
            aft_sprite_indices,
            aft_texture_handles,
            camera_ancestors,
            camera_states,
            dynamic_camera_scope,
        }
    }

    fn fill_camera_states<S: NoteskinSlot + Clone>(
        overlays: &[SongLuaOverlayActor<S>],
        mut state_at: impl FnMut(usize) -> Option<SongLuaOverlayState>,
        camera_states: &mut [SongLuaOverlayIndex],
    ) {
        camera_states.fill(SongLuaOverlayIndex::default());
        for (index, overlay) in overlays.iter().enumerate() {
            let camera_index = overlay.parent_index.and_then(|parent_index| {
                let parent = overlays.get(parent_index)?;
                let parent_state = state_at(parent_index)?;
                if parent_index >= index {
                    return None;
                }
                if matches!(
                    parent.kind,
                    SongLuaOverlayKind::ActorFrame | SongLuaOverlayKind::ActorFrameTexture { .. }
                ) && parent_state.fov.is_some()
                {
                    Some(parent_index)
                } else {
                    camera_states[parent_index].get()
                }
            });
            camera_states[index] = SongLuaOverlayIndex::new(camera_index);
        }
    }

    fn include_camera_eases<S: NoteskinSlot + Clone>(
        &mut self,
        overlays: &[SongLuaOverlayActor<S>],
        overlay_eases: &[SongLuaOverlayEaseWindowRuntime],
    ) {
        self.dynamic_camera_scope |= overlay_eases.iter().any(|ease| {
            overlays.get(ease.overlay_index).is_some_and(|overlay| {
                matches!(
                    overlay.kind,
                    SongLuaOverlayKind::ActorFrame | SongLuaOverlayKind::ActorFrameTexture { .. }
                ) && (ease.from.delta.fov.is_some() || ease.to.delta.fov.is_some())
            })
        });
    }

    #[inline(always)]
    fn camera_state(
        &self,
        overlay_states: &[SongLuaOverlayState],
        index: usize,
    ) -> Option<SongLuaOverlayState> {
        if self.dynamic_camera_scope {
            let mut candidate = self
                .camera_ancestors
                .get(index)
                .copied()
                .and_then(SongLuaOverlayIndex::get);
            while let Some(current) = candidate {
                let state = overlay_states.get(current).copied()?;
                if state.fov.is_some() {
                    return Some(state);
                }
                candidate = self
                    .camera_ancestors
                    .get(current)
                    .copied()
                    .and_then(SongLuaOverlayIndex::get);
            }
            return None;
        }
        self.camera_states
            .get(index)
            .copied()
            .and_then(SongLuaOverlayIndex::get)
            .and_then(|camera_index| overlay_states.get(camera_index))
            .copied()
    }
}

const PROJECTED_MESH_VERTEX_CAPACITY: usize = 54;
const SONG_LUA_RAINBOW_TEXT_PREWARM_MAX_CHARS: usize = 64;

/// Screen-owned reusable storage for one dynamic Song Lua mesh.
///
/// One scratch slot is prewarmed for every compiled projected Sprite/Quad and
/// `ActorMultiVertex` during gameplay entry. The game/render thread is the sole
/// writer and clones the base/glow `Arc<Vec<_>>` buffers into actors each frame.
/// A normal next frame recovers each uniquely owned buffer, clears it, and
/// performs no allocation; if an older actor still owns a buffer, the slot
/// replaces it rather than mutating shared data. Capacity is fixed to the
/// compiled vertex count or the 4x4 fade grid's 54 triangle vertices, with no
/// eviction or pruning. Buffers and replaced generations are freed outside
/// their live actor use, ultimately with the screen. Replacement and capacity
/// counters bound worst-case frame work to that actor's compiled vertex count.
/// Static `BitmapText` uppercase content and the seven rainbow-scroll phases are
/// also compiled into this overlay-local song-lifetime storage during screen
/// entry. Rainbow phase storage is capped at 64 characters per actor; larger
/// text reuses one current-phase buffer sized from the compiled text instead of
/// retaining seven copies or allocating during gameplay. `GraphDisplay` body/line geometry and its two-child frame are sized
/// from the compiled point count. Static Model glow vertices and stable
/// renderer geometry keys are computed once per layer during entry.
/// `NoteskinActor` slots get an exact-capacity, sealed model cache plus one
/// pre-whitened glow array per model slot. A gameplay miss preserves output but
/// counts as failed prewarm and may allocate; registered slots never grow,
/// prune, or evict, and all model storage is destroyed with the song screen.
/// `BitmapText` gets two buffers sized for its compiled
/// spans plus one whole-text span: one for dynamic diffuse composition and one
/// for glow/stroke extraction. Gameplay frames only refill or clone these
/// prewarmed buffers. The fallback conversion also supports test-only builders
/// that do not provide screen scratch.
/// Authored text changes reserve their largest string length and one uppercase
/// string per change at entry. They never grow or evict during playback and
/// are released with this song; selection costs at most two binary searches per draw.
#[derive(Default)]
struct SongLuaProjectedMeshScratch {
    sprite_key: Option<Arc<str>>,
    sprite_revision: u64,
    sprite_binding: Option<deadlib_assets::BoundTexture>,
    textured_vertices: Option<Arc<Vec<TexturedMeshVertex>>>,
    textured_glow_vertices: Option<Arc<Vec<TexturedMeshVertex>>>,
    mesh_vertices: Option<Arc<Vec<MeshVertex>>>,
    graph_body_vertices: Option<Arc<Vec<MeshVertex>>>,
    graph_line_vertices: Option<Arc<Vec<MeshVertex>>>,
    graph_body_key: Option<[u32; 10]>,
    graph_line_key: Option<[u32; 11]>,
    graph_frame: Option<SharedActorFrameScratch>,
    model_geometry_keys: Option<Vec<TMeshCacheKey>>,
    model_glow_vertices: Option<Vec<Arc<[TexturedMeshVertex]>>>,
    noteskin_model_cache: Option<ModelMeshCache>,
    noteskin_glow_vertices: Option<Vec<Option<Arc<[TexturedMeshVertex]>>>>,
    text_diffuse_attributes: Option<Arc<Vec<TextAttribute>>>,
    text_glow_attributes: Option<Arc<Vec<TextAttribute>>>,
    uppercase_text: Option<Arc<str>>,
    uppercase_changes: Vec<Arc<str>>,
    rainbow_text_attributes: Option<[Arc<[TextAttribute]>; SONG_LUA_TEXT_RAINBOW_COLORS.len()]>,
    capacity: usize,
    text_attribute_capacity: usize,
    replacements: u64,
}

impl SongLuaProjectedMeshScratch {
    // One binding per overlay, owned by the screen's existing scratch. Prewarmed
    // for all sprites at entry, including hidden layers. No eviction or growth:
    // a steady draw only checks the revision and Arc identity. A changed key or
    // store revision performs one local lookup (and metadata fallback if needed).
    // Missing assets stay missing until that revision changes; no draw-time I/O.
    fn bind_sprite<T>(
        &mut self,
        key: &Arc<str>,
        textures: &deadlib_assets::TextureStore<T>,
    ) -> Option<deadlib_assets::BoundTexture> {
        let revision = textures.revision();
        if self.sprite_revision != revision
            || !self
                .sprite_key
                .as_ref()
                .is_some_and(|previous| Arc::ptr_eq(previous, key))
        {
            self.sprite_binding = textures.bind_texture(key);
            self.sprite_key = Some(Arc::clone(key));
            self.sprite_revision = revision;
        }
        self.sprite_binding
    }

    fn textured(capacity: usize) -> Self {
        Self {
            sprite_key: None,
            sprite_revision: 0,
            sprite_binding: None,
            textured_vertices: Some(Arc::new(Vec::with_capacity(capacity))),
            textured_glow_vertices: Some(Arc::new(Vec::with_capacity(capacity))),
            mesh_vertices: None,
            graph_body_vertices: None,
            graph_line_vertices: None,
            graph_body_key: None,
            graph_line_key: None,
            graph_frame: None,
            model_geometry_keys: None,
            model_glow_vertices: None,
            noteskin_model_cache: None,
            noteskin_glow_vertices: None,
            text_diffuse_attributes: None,
            text_glow_attributes: None,
            uppercase_text: None,
            uppercase_changes: Vec::new(),
            rainbow_text_attributes: None,
            capacity,
            text_attribute_capacity: 0,
            replacements: 0,
        }
    }

    fn mesh(capacity: usize) -> Self {
        Self {
            sprite_key: None,
            sprite_revision: 0,
            sprite_binding: None,
            textured_vertices: None,
            textured_glow_vertices: None,
            mesh_vertices: Some(Arc::new(Vec::with_capacity(capacity))),
            graph_body_vertices: None,
            graph_line_vertices: None,
            graph_body_key: None,
            graph_line_key: None,
            graph_frame: None,
            model_geometry_keys: None,
            model_glow_vertices: None,
            noteskin_model_cache: None,
            noteskin_glow_vertices: None,
            text_diffuse_attributes: None,
            text_glow_attributes: None,
            uppercase_text: None,
            uppercase_changes: Vec::new(),
            rainbow_text_attributes: None,
            capacity,
            text_attribute_capacity: 0,
            replacements: 0,
        }
    }

    fn graph(capacity: usize) -> Self {
        Self {
            graph_body_vertices: Some(Arc::new(Vec::with_capacity(capacity))),
            graph_line_vertices: Some(Arc::new(Vec::with_capacity(capacity))),
            graph_frame: Some(SharedActorFrameScratch::with_capacity(2)),
            capacity,
            ..Self::default()
        }
    }

    fn model(layers: &[SongLuaOverlayModelLayer]) -> Self {
        let model_geometry_keys = layers
            .iter()
            .map(|layer| song_lua_model_geometry_key(&layer.vertices))
            .collect();
        let model_glow_vertices = layers
            .iter()
            .map(|layer| song_lua_static_glow_vertices(&layer.vertices))
            .collect();
        Self {
            model_geometry_keys: Some(model_geometry_keys),
            model_glow_vertices: Some(model_glow_vertices),
            ..Self::default()
        }
    }

    fn noteskin<S: NoteskinSlot + Clone>(slots: &[S]) -> Self {
        let mut model_cache = ModelMeshCache::with_capacity(slots.len());
        for slot in slots {
            model_cache.prewarm_slot(slot);
        }
        let noteskin_glow_vertices = slots
            .iter()
            .map(|slot| {
                model_cache
                    .model_geometry(slot)
                    .map(|(_, vertices)| song_lua_static_glow_vertices(&vertices))
            })
            .collect();
        model_cache.seal();
        Self {
            noteskin_model_cache: Some(model_cache),
            noteskin_glow_vertices: Some(noteskin_glow_vertices),
            ..Self::default()
        }
    }

    fn prewarm_text_attributes(&mut self, attribute_count: usize, text_char_count: usize) {
        let capacity = attribute_count.saturating_add(1).max(text_char_count);
        self.text_diffuse_attributes = Some(Arc::new(Vec::with_capacity(capacity)));
        self.text_glow_attributes = Some(Arc::new(Vec::with_capacity(capacity)));
        self.text_attribute_capacity = capacity;
    }

    fn update_projected(
        &mut self,
        grid: &[TexturedMeshVertex],
        width: usize,
        height: usize,
        edge_fade: [f32; 4],
    ) -> Arc<Vec<TexturedMeshVertex>> {
        update_song_lua_shared_vec(
            &mut self.textured_vertices,
            self.capacity,
            &mut self.replacements,
            |vertices| {
                append_projected_mesh_vertices(grid, width, height, edge_fade, vertices);
            },
        )
    }

    fn update_textured(
        &mut self,
        fill: impl FnOnce(&mut Vec<TexturedMeshVertex>),
    ) -> Arc<Vec<TexturedMeshVertex>> {
        update_song_lua_shared_vec(
            &mut self.textured_vertices,
            self.capacity,
            &mut self.replacements,
            fill,
        )
    }

    fn update_textured_glow(
        &mut self,
        vertices: &[TexturedMeshVertex],
    ) -> Arc<Vec<TexturedMeshVertex>> {
        update_song_lua_shared_vec(
            &mut self.textured_glow_vertices,
            self.capacity,
            &mut self.replacements,
            |out| {
                out.extend(vertices.iter().copied().map(|mut vertex| {
                    vertex.color = [1.0, 1.0, 1.0, vertex.color[3]];
                    vertex
                }));
            },
        )
    }

    fn rainbow_attributes(&mut self, text: &str, total_elapsed: f32) -> TextAttributes {
        let phase = song_lua_rainbow_scroll_phase(total_elapsed);
        if let Some(attributes) = self.rainbow_text_attributes.as_ref() {
            return TextAttributes::from(Arc::clone(&attributes[phase]));
        }
        self.update_text_diffuse(|out| {
            append_song_lua_rainbow_scroll_attributes_at_phase(text, phase, out);
        })
    }

    fn update_mesh(&mut self, fill: impl FnOnce(&mut Vec<MeshVertex>)) -> Arc<Vec<MeshVertex>> {
        update_song_lua_shared_vec(
            &mut self.mesh_vertices,
            self.capacity,
            &mut self.replacements,
            fill,
        )
    }

    fn update_graph_body(
        &mut self,
        key: [u32; 10],
        fill: impl FnOnce(&mut Vec<MeshVertex>),
    ) -> Arc<Vec<MeshVertex>> {
        if self.graph_body_key == Some(key)
            && let Some(vertices) = self.graph_body_vertices.as_ref()
        {
            return Arc::clone(vertices);
        }
        self.graph_body_key = Some(key);
        update_song_lua_shared_vec(
            &mut self.graph_body_vertices,
            self.capacity,
            &mut self.replacements,
            fill,
        )
    }

    fn update_graph_line(
        &mut self,
        key: [u32; 11],
        fill: impl FnOnce(&mut Vec<MeshVertex>),
    ) -> Arc<Vec<MeshVertex>> {
        if self.graph_line_key == Some(key)
            && let Some(vertices) = self.graph_line_vertices.as_ref()
        {
            return Arc::clone(vertices);
        }
        self.graph_line_key = Some(key);
        update_song_lua_shared_vec(
            &mut self.graph_line_vertices,
            self.capacity,
            &mut self.replacements,
            fill,
        )
    }

    fn update_text_diffuse(
        &mut self,
        fill: impl FnOnce(&mut Vec<TextAttribute>),
    ) -> TextAttributes {
        TextAttributes::from(update_song_lua_shared_vec(
            &mut self.text_diffuse_attributes,
            self.text_attribute_capacity,
            &mut self.replacements,
            fill,
        ))
    }

    fn update_text_glow(&mut self, fill: impl FnOnce(&mut Vec<TextAttribute>)) -> TextAttributes {
        TextAttributes::from(update_song_lua_shared_vec(
            &mut self.text_glow_attributes,
            self.text_attribute_capacity,
            &mut self.replacements,
            fill,
        ))
    }
}

fn song_lua_static_glow_vertices(vertices: &[TexturedMeshVertex]) -> Arc<[TexturedMeshVertex]> {
    Arc::from(
        vertices
            .iter()
            .copied()
            .map(|mut vertex| {
                vertex.color = [1.0, 1.0, 1.0, vertex.color[3]];
                vertex
            })
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    )
}

fn song_lua_model_geometry_key(vertices: &[TexturedMeshVertex]) -> TMeshCacheKey {
    let mut hasher = rustc_hash::FxHasher::default();
    hasher.write(b"deadsync-song-lua-model-v1");
    hasher.write_usize(vertices.len());
    for vertex in vertices {
        for value in vertex.pos {
            hasher.write_u32(value.to_bits());
        }
        for value in vertex.uv {
            hasher.write_u32(value.to_bits());
        }
        for value in vertex.tex_matrix_scale {
            hasher.write_u32(value.to_bits());
        }
        for value in vertex.color {
            hasher.write_u32(value.to_bits());
        }
    }
    hasher.finish().max(1)
}

fn song_lua_glow_geometry_key(base: TMeshCacheKey) -> TMeshCacheKey {
    if base == INVALID_TMESH_CACHE_KEY {
        return INVALID_TMESH_CACHE_KEY;
    }
    let mut hasher = rustc_hash::FxHasher::default();
    hasher.write(b"deadsync-song-lua-glow-v1");
    hasher.write_u64(base);
    hasher.finish().max(1)
}

fn update_song_lua_shared_vec<T>(
    shared: &mut Option<Arc<Vec<T>>>,
    capacity: usize,
    replacements: &mut u64,
    fill: impl FnOnce(&mut Vec<T>),
) -> Arc<Vec<T>> {
    let shared = shared.get_or_insert_with(|| Arc::new(Vec::with_capacity(capacity)));
    let values = match Arc::get_mut(shared) {
        Some(values) => values,
        None => {
            *replacements = (*replacements).saturating_add(1);
            *shared = Arc::new(Vec::with_capacity(capacity));
            Arc::get_mut(shared).expect("shared vector was just made unique")
        }
    };
    values.clear();
    fill(values);
    Arc::clone(shared)
}

fn song_lua_projected_mesh_scratch_for<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
) -> Vec<SongLuaProjectedMeshScratch> {
    overlays
        .iter()
        .map(|overlay| {
            let mut scratch = match &overlay.kind {
                SongLuaOverlayKind::Sprite { .. } | SongLuaOverlayKind::Quad => {
                    SongLuaProjectedMeshScratch::textured(PROJECTED_MESH_VERTEX_CAPACITY)
                }
                SongLuaOverlayKind::ActorMultiVertex {
                    vertices,
                    texture_key: Some(_),
                    ..
                } => SongLuaProjectedMeshScratch::textured(vertices.len()),
                SongLuaOverlayKind::ActorMultiVertex { vertices, .. } => {
                    SongLuaProjectedMeshScratch::mesh(vertices.len())
                }
                SongLuaOverlayKind::GraphDisplay { body_values, .. } => {
                    let vertex_count = graph_display_values_or_default(body_values)
                        .len()
                        .saturating_sub(1)
                        .saturating_mul(6);
                    SongLuaProjectedMeshScratch::graph(vertex_count)
                }
                SongLuaOverlayKind::Model { layers } => SongLuaProjectedMeshScratch::model(layers),
                SongLuaOverlayKind::NoteskinActor { slots } => {
                    SongLuaProjectedMeshScratch::noteskin(slots)
                }
                _ => SongLuaProjectedMeshScratch::default(),
            };
            if let SongLuaOverlayKind::BitmapText {
                text,
                text_changes,
                attributes,
                ..
            } = &overlay.kind
            {
                let uppercase_text = Arc::<str>::from(text.to_uppercase());
                let char_count = text_changes
                    .iter()
                    .map(|(_, text)| text.chars().count())
                    .chain(std::iter::once(text.chars().count()))
                    .max()
                    .unwrap_or(0);
                scratch.prewarm_text_attributes(
                    attributes.len(),
                    char_count.max(uppercase_text.chars().count()),
                );
                scratch.uppercase_text = Some(uppercase_text);
                scratch.uppercase_changes = text_changes
                    .iter()
                    .map(|(_, text)| Arc::from(text.to_uppercase()))
                    .collect();
                if text_changes.is_empty()
                    && (1..=SONG_LUA_RAINBOW_TEXT_PREWARM_MAX_CHARS).contains(&char_count)
                {
                    scratch.rainbow_text_attributes =
                        Some(song_lua_rainbow_scroll_phases(text.as_ref()));
                }
            }
            scratch
        })
        .collect()
}

#[derive(Default)]
struct SongLuaProxyRequestIndex {
    topology: SongLuaOverlayTopologyIndex,
    root_indices: Vec<usize>,
    capture_children: Vec<Vec<usize>>,
    proxy_indices: Vec<usize>,
}

/// Song-lifetime visitation marks for nested AFT/proxy traversals.
///
/// Owner/thread model: gameplay `State`, used only by the frame-building
/// thread. Lifetime: one song. Capacity is the largest compiled overlay layer
/// and is populated at screen entry. Each traversal advances a generation, so
/// gameplay frames neither clear the full array nor allocate a recursion stack.
/// A wrapped generation performs one bounded fill. Duplicate references and
/// cycles are skipped after their first visit; there is no eviction, pruning,
/// or destruction before the gameplay transition. Worst-case frame work is one
/// visit per compiled capture rather than one visit per reference.
#[derive(Default)]
struct SongLuaCaptureVisitScratch {
    marks: Vec<u32>,
    generation: u32,
}

impl SongLuaCaptureVisitScratch {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            marks: vec![0; capacity],
            generation: 0,
        }
    }

    fn begin(&mut self, required: usize) {
        debug_assert!(
            required <= self.marks.len(),
            "song-lifetime capture marks must cover every compiled overlay"
        );
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.marks.fill(0);
            self.generation = 1;
        }
    }

    fn visit(&mut self, index: usize) -> bool {
        let Some(mark) = self.marks.get_mut(index) else {
            return false;
        };
        if *mark == self.generation {
            return false;
        }
        *mark = self.generation;
        true
    }
}

impl SongLuaProxyRequestIndex {
    fn new<S: NoteskinSlot + Clone>(overlays: &[SongLuaOverlayActor<S>]) -> Self {
        let topology = SongLuaOverlayTopologyIndex::new(overlays);
        let mut root_indices = Vec::with_capacity(overlays.len());
        let mut capture_children = vec![Vec::new(); overlays.len()];
        for (index, ancestor) in topology
            .aft_ancestors
            .iter()
            .copied()
            .map(SongLuaOverlayIndex::get)
            .enumerate()
        {
            if let Some(ancestor) = ancestor {
                if let Some(children) = capture_children.get_mut(ancestor) {
                    children.push(index);
                }
            } else {
                root_indices.push(index);
            }
        }
        let proxy_indices = overlays
            .iter()
            .enumerate()
            .filter_map(|(index, overlay)| {
                matches!(overlay.kind, SongLuaOverlayKind::ActorProxy { .. }).then_some(index)
            })
            .collect();
        Self {
            topology,
            root_indices,
            capture_children,
            proxy_indices,
        }
    }

    fn direct_proxy_capacity<S: NoteskinSlot + Clone>(
        &self,
        overlays: &[SongLuaOverlayActor<S>],
    ) -> usize {
        self.root_indices
            .iter()
            .map(
                |&index| match overlays.get(index).map(|overlay| &overlay.kind) {
                    Some(SongLuaOverlayKind::ActorProxy {
                        target: SongLuaProxyTarget::Player { .. },
                    }) => 2,
                    Some(SongLuaOverlayKind::ActorProxy {
                        target:
                            SongLuaProxyTarget::NoteField { .. }
                            | SongLuaProxyTarget::Judgment { .. }
                            | SongLuaProxyTarget::Combo { .. },
                    }) => 1,
                    _ => 0,
                },
            )
            .sum()
    }
}

/// Game-thread, song-lifetime cursor for one overlay's immutable message list.
///
/// It owns no variable-capacity storage and needs no synchronization. The first
/// frame/event transition advances source cursors and compiles the active
/// block's easing enum; steady tween frames only evaluate that enum. Backward
/// seeks reset and replay message/block cursors, with work bounded by crossed
/// events and blocks. There are no allocations, misses, eviction, or separate
/// destruction; the cache drops with frame scratch. Parity tests cover seeks.
#[derive(Clone, Copy, Debug)]
struct SongLuaMessageStateCache {
    initialized: bool,
    next_event: usize,
    processed_until: f32,
    base_state: SongLuaOverlayState,
    active_command_index: Option<usize>,
    active_start_second: f32,
    active_next_block: usize,
    active_easing: Option<(usize, SongLuaEase)>,
    active_block_state: SongLuaOverlayState,
    active_last_elapsed: f32,
}

impl Default for SongLuaMessageStateCache {
    fn default() -> Self {
        Self {
            initialized: false,
            next_event: 0,
            processed_until: f32::NEG_INFINITY,
            base_state: SongLuaOverlayState::default(),
            active_command_index: None,
            active_start_second: 0.0,
            active_next_block: 0,
            active_easing: None,
            active_block_state: SongLuaOverlayState::default(),
            active_last_elapsed: f32::NEG_INFINITY,
        }
    }
}

impl SongLuaMessageStateCache {
    #[inline(always)]
    const fn reset(&mut self, initial_state: SongLuaOverlayState) {
        self.initialized = true;
        self.next_event = 0;
        self.processed_until = f32::NEG_INFINITY;
        self.base_state = initial_state;
        self.active_command_index = None;
        self.active_start_second = 0.0;
        self.reset_active_blocks(initial_state);
    }

    #[inline(always)]
    const fn reset_active_blocks(&mut self, state: SongLuaOverlayState) {
        self.active_next_block = 0;
        self.active_easing = None;
        self.active_block_state = state;
        self.active_last_elapsed = f32::NEG_INFINITY;
    }
}

fn song_lua_overlay_child_list_index(parent_index: Option<usize>) -> usize {
    parent_index.map_or(0, |idx| idx + 1)
}

fn song_lua_sort_static_children<S>(overlays: &[SongLuaOverlayActor<S>], children: &mut [usize]) {
    children.sort_by_key(|&idx| (overlays[idx].initial_state.draw_order, idx));
}

fn song_lua_push_static_order(
    child_lists: &[Vec<usize>],
    parent_index: Option<usize>,
    out: &mut Vec<usize>,
) {
    let list_idx = song_lua_overlay_child_list_index(parent_index);
    let Some(children) = child_lists.get(list_idx) else {
        return;
    };
    for &index in children {
        out.push(index);
        song_lua_push_static_order(child_lists, Some(index), out);
    }
}

fn song_lua_overlay_order_cache_from<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    overlay_eases: &[SongLuaOverlayEaseWindowRuntime],
) -> SongLuaOverlayOrderCache {
    let mut child_lists = vec![Vec::new(); overlays.len() + 1];
    for (idx, overlay) in overlays.iter().enumerate() {
        let list_idx = match overlay.parent_index {
            Some(parent_index) if parent_index < overlays.len() => parent_index + 1,
            Some(_) => continue,
            None => 0,
        };
        child_lists[list_idx].push(idx);
    }
    for children in &mut child_lists {
        song_lua_sort_static_children(overlays, children);
    }
    let mut tap_commands = Vec::new();
    for (overlay, actor) in overlays.iter().enumerate() {
        for (command, callback) in actor.message_commands.iter().enumerate() {
            let Some(key) = callback.message.strip_prefix("__songlua_tap_") else {
                continue;
            };
            let mut parts = key.split('_');
            let player = parts
                .next()
                .and_then(|part| part.parse::<usize>().ok())
                .and_then(|value| value.checked_sub(1));
            let column = parts
                .next()
                .and_then(|part| part.parse::<usize>().ok())
                .and_then(|value| value.checked_sub(1));
            let grade = match parts.next() {
                Some("W1") => JudgeGrade::Fantastic,
                Some("W2") => JudgeGrade::Excellent,
                Some("W3") => JudgeGrade::Great,
                Some("W4") => JudgeGrade::Decent,
                Some("W5") => JudgeGrade::WayOff,
                _ => continue,
            };
            if let (Some(player), Some(column)) = (player, column) {
                tap_commands.push(SongLuaTapCommand {
                    overlay,
                    command,
                    player,
                    column,
                    grade,
                    glow_started_at: None,
                });
            }
        }
    }
    let mut dynamic_actor_draw_order = vec![false; overlays.len()];
    let mut dynamic_local = vec![false; overlays.len()];
    let mut has_dynamic_z_order = overlays
        .iter()
        .any(|overlay| overlay.initial_state.draw_by_z_position);
    for (idx, overlay) in overlays.iter().enumerate() {
        dynamic_local[idx] = !overlay.message_commands.is_empty();
        dynamic_actor_draw_order[idx] = overlay.message_commands.iter().any(|command| {
            command
                .blocks
                .iter()
                .any(|block| block.delta.draw_order.is_some())
        });
        has_dynamic_z_order |= overlay.message_commands.iter().any(|command| {
            command
                .blocks
                .iter()
                .any(|block| block.delta.z.is_some() || block.delta.draw_by_z_position.is_some())
        });
    }
    for ease in overlay_eases {
        if ease.overlay_index < dynamic_actor_draw_order.len() {
            dynamic_local[ease.overlay_index] = true;
            if ease.from.delta.draw_order.is_some() || ease.to.delta.draw_order.is_some() {
                dynamic_actor_draw_order[ease.overlay_index] = true;
            }
            has_dynamic_z_order |= ease.from.delta.z.is_some()
                || ease.to.delta.z.is_some()
                || ease.from.delta.draw_by_z_position.is_some()
                || ease.to.delta.draw_by_z_position.is_some();
        }
    }
    let update_actor_index = SongLuaOverlayIndex::new(
        overlays
            .iter()
            .position(|overlay| matches!(overlay.kind, SongLuaOverlayKind::UpdateTracks { .. })),
    );
    let overlay_updates = song_lua_overlay_runtime_updates_at(overlays, update_actor_index);
    let mut visible_update_indices = SmallVec::new();
    for (track_index, track) in overlay_updates.iter().enumerate() {
        if track.target == crate::SongLuaOverlayUpdateTarget::Visible {
            visible_update_indices.push(track_index);
        }
        if track.overlay_index >= dynamic_local.len() {
            continue;
        }
        dynamic_local[track.overlay_index] = true;
        dynamic_actor_draw_order[track.overlay_index] |=
            track.target == crate::SongLuaOverlayUpdateTarget::DrawOrder;
        has_dynamic_z_order |= matches!(
            track.target,
            crate::SongLuaOverlayUpdateTarget::Z
                | crate::SongLuaOverlayUpdateTarget::DrawByZPosition
        );
    }

    let mut update_ranges = Vec::with_capacity(overlays.len());
    let mut track_index = 0;
    for overlay_index in 0..overlays.len() {
        while overlay_updates
            .get(track_index)
            .is_some_and(|track| track.overlay_index < overlay_index)
        {
            track_index += 1;
        }
        let start = track_index;
        while overlay_updates
            .get(track_index)
            .is_some_and(|track| track.overlay_index == overlay_index)
        {
            track_index += 1;
        }
        update_ranges.push(start..track_index);
    }

    let dynamic_local_indices = dynamic_local
        .iter()
        .enumerate()
        .filter_map(|(index, dynamic)| dynamic.then_some(index))
        .collect::<Box<[_]>>();
    let mut dynamic_composed = Vec::with_capacity(overlays.len());
    let mut dynamic_composed_indices = Vec::new();
    for (index, overlay) in overlays.iter().enumerate() {
        let dynamic = dynamic_local[index]
            || overlay
                .parent_index
                .and_then(|parent| dynamic_composed.get(parent))
                .copied()
                .unwrap_or(false);
        dynamic_composed.push(dynamic);
        if dynamic {
            dynamic_composed_indices.push(index);
        }
    }

    let dynamic_draw_order = child_lists
        .iter()
        .map(|children| {
            children
                .iter()
                .any(|&idx| dynamic_actor_draw_order.get(idx).copied().unwrap_or(false))
        })
        .collect::<Vec<_>>();
    let static_root_order =
        if dynamic_actor_draw_order.iter().any(|dynamic| *dynamic) || has_dynamic_z_order {
            None
        } else {
            let mut order = Vec::with_capacity(overlays.len());
            song_lua_push_static_order(&child_lists, None, &mut order);
            Some(order.into_boxed_slice())
        };
    let sort_modes = vec![SONG_LUA_CHILD_ORDER_STATIC; child_lists.len()];
    SongLuaOverlayOrderCache {
        child_lists,
        dynamic_draw_order,
        tap_commands,
        dynamic_local_indices,
        dynamic_composed_indices: dynamic_composed_indices.into_boxed_slice(),
        static_root_order,
        update_actor_index,
        update_ranges: update_ranges.into_boxed_slice(),
        visible_update_indices,
        update_cursors: vec![0; overlay_updates.len()],
        sort_modes,
        last_draw_orders: vec![0; overlays.len()],
        last_z_keys: vec![0; overlays.len()],
    }
}
/// Song-lifetime activation index for one visual-layer list.
///
/// The gameplay thread owns three fixed-capacity buffers. Screen entry separates
/// source-order starts from their sorted index order and reserves the complete
/// active capacity. Two scalar deadlines make unchanged frames O(1) without
/// touching either boxed table. Chronological sources extend or truncate the
/// active prefix directly; unsorted sources retain the bounded binary
/// insert/remove and O(n log n) seek fallback, preserving source draw order.
/// There are no gameplay allocations, misses, eviction, pruning,
/// synchronization, or destruction before screen exit; all storage drops with
/// the screen.
struct SongLuaLayerActivity {
    start_seconds: Box<[f32]>,
    start_order: Box<[usize]>,
    active: Vec<usize>,
    next_start: usize,
    next_start_second: f32,
    previous_start_second: f32,
    source_ordered: bool,
}

impl Default for SongLuaLayerActivity {
    fn default() -> Self {
        Self {
            start_seconds: Box::default(),
            start_order: Box::default(),
            active: Vec::new(),
            next_start: 0,
            next_start_second: f32::INFINITY,
            previous_start_second: f32::NEG_INFINITY,
            source_ordered: true,
        }
    }
}

impl SongLuaLayerActivity {
    fn new(starts: impl IntoIterator<Item = f32>, now: f32) -> Self {
        let start_seconds = starts.into_iter().collect::<Box<[_]>>();
        let mut start_order = (0..start_seconds.len()).collect::<Vec<_>>();
        start_order.sort_by(|&left, &right| {
            start_seconds[left]
                .total_cmp(&start_seconds[right])
                .then_with(|| left.cmp(&right))
        });
        let source_ordered = start_order
            .iter()
            .enumerate()
            .all(|(position, &source_index)| position == source_index);
        let next_start_second = start_order
            .first()
            .map_or(f32::INFINITY, |&index| start_seconds[index]);
        let mut activity = Self {
            active: Vec::with_capacity(start_seconds.len()),
            start_seconds,
            start_order: start_order.into_boxed_slice(),
            next_start: 0,
            next_start_second,
            previous_start_second: f32::NEG_INFINITY,
            source_ordered,
        };
        activity.sync(now);
        activity
    }

    #[inline]
    fn sync(&mut self, now: f32) -> &[usize] {
        let old_next_start = self.next_start;
        while self.next_start < self.start_order.len()
            && now.partial_cmp(&self.next_start_second) != Some(std::cmp::Ordering::Less)
        {
            self.previous_start_second = self.next_start_second;
            self.next_start += 1;
            self.next_start_second = self
                .start_order
                .get(self.next_start)
                .map_or(f32::INFINITY, |&index| self.start_seconds[index]);
        }
        while self.next_start > 0 && now < self.previous_start_second {
            self.next_start_second = self.previous_start_second;
            self.next_start -= 1;
            self.previous_start_second = self
                .next_start
                .checked_sub(1)
                .map_or(f32::NEG_INFINITY, |position| {
                    self.start_seconds[self.start_order[position]]
                });
        }
        if self.next_start == old_next_start {
            return &self.active;
        }
        if self.source_ordered {
            if self.next_start > old_next_start {
                self.active.extend(old_next_start..self.next_start);
            } else {
                self.active.truncate(self.next_start);
            }
            return &self.active;
        }
        match self.next_start.abs_diff(old_next_start) {
            0 => {}
            1 if self.next_start > old_next_start => {
                let index = self.start_order[old_next_start];
                let insert_at = self.active.binary_search(&index).unwrap_or_else(|at| at);
                self.active.insert(insert_at, index);
            }
            1 => {
                let index = self.start_order[self.next_start];
                if let Ok(remove_at) = self.active.binary_search(&index) {
                    self.active.remove(remove_at);
                }
            }
            _ => {
                self.active.clear();
                self.active
                    .extend_from_slice(&self.start_order[..self.next_start]);
                self.active.sort_unstable();
            }
        }
        &self.active
    }
}
const SONG_LUA_FG_OWNER_ROOT: u8 = 0;
const SONG_LUA_FG_OWNER_BACKGROUND: u8 = 1;
const SONG_LUA_FG_OWNER_FOREGROUND: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SongLuaForegroundOwnerRef {
    source: u8,
    layer_index: usize,
    overlay_index: usize,
}

struct SongLuaForegroundOwnerPath {
    path: PathBuf,
    owner_start: usize,
    owner_end: usize,
}

/// Path-specific Song Lua ownership index for the active simfile foreground.
///
/// The gameplay thread owns immutable, exactly-sized path and owner tables built
/// at screen entry. Foreground events select a path with binary search; gameplay
/// frames read only its contiguous owner range. Misses saturate to an empty
/// range, there is no growth, eviction, synchronization, or live-frame pruning,
/// and storage is freed at screen exit.
#[derive(Default)]
struct SongLuaForegroundOwnerIndex {
    paths: Box<[SongLuaForegroundOwnerPath]>,
    owners: Box<[SongLuaForegroundOwnerRef]>,
    active_start: usize,
    active_end: usize,
}

impl SongLuaForegroundOwnerIndex {
    fn new<S: NoteskinSlot + Clone>(
        visuals: &SongLuaRuntimeVisuals<
            SongLuaOverlayActor<S>,
            SongLuaCapturedActor,
            SongLuaRuntimeOverlayStateDelta,
        >,
    ) -> Self {
        let capacity = visuals.overlays.len()
            + visuals
                .background_visual_layers
                .iter()
                .map(|layer| layer.overlays.len())
                .sum::<usize>()
            + visuals
                .foreground_visual_layers
                .iter()
                .map(|layer| layer.overlays.len())
                .sum::<usize>();
        let mut entries = Vec::with_capacity(capacity);
        Self::push_entries(&mut entries, SONG_LUA_FG_OWNER_ROOT, 0, &visuals.overlays);
        for (layer_index, layer) in visuals.background_visual_layers.iter().enumerate() {
            Self::push_entries(
                &mut entries,
                SONG_LUA_FG_OWNER_BACKGROUND,
                layer_index,
                &layer.overlays,
            );
        }
        for (layer_index, layer) in visuals.foreground_visual_layers.iter().enumerate() {
            Self::push_entries(
                &mut entries,
                SONG_LUA_FG_OWNER_FOREGROUND,
                layer_index,
                &layer.overlays,
            );
        }
        entries.sort_by(|left, right| {
            left.0
                .cmp(right.0)
                .then_with(|| left.1.source.cmp(&right.1.source))
                .then_with(|| left.1.layer_index.cmp(&right.1.layer_index))
                .then_with(|| left.1.overlay_index.cmp(&right.1.overlay_index))
        });

        let mut paths = Vec::new();
        let mut owners = Vec::with_capacity(entries.len());
        let mut cursor = 0;
        while cursor < entries.len() {
            let owner_start = owners.len();
            let path = entries[cursor].0.to_path_buf();
            while cursor < entries.len() && entries[cursor].0 == path.as_path() {
                owners.push(entries[cursor].1);
                cursor += 1;
            }
            paths.push(SongLuaForegroundOwnerPath {
                path,
                owner_start,
                owner_end: owners.len(),
            });
        }
        Self {
            paths: paths.into_boxed_slice(),
            owners: owners.into_boxed_slice(),
            active_start: 0,
            active_end: 0,
        }
    }

    fn push_entries<'a, S: NoteskinSlot + Clone>(
        out: &mut Vec<(&'a Path, SongLuaForegroundOwnerRef)>,
        source: u8,
        layer_index: usize,
        overlays: &'a [SongLuaOverlayActor<S>],
    ) {
        out.extend(
            overlays
                .iter()
                .enumerate()
                .filter_map(|(overlay_index, overlay)| match &overlay.kind {
                    SongLuaOverlayKind::Sprite { texture_path, .. } => Some((
                        texture_path.as_path(),
                        SongLuaForegroundOwnerRef {
                            source,
                            layer_index,
                            overlay_index,
                        },
                    )),
                    _ => None,
                }),
        );
    }

    fn select(&mut self, path: Option<&Path>) {
        let Some(path) = path else {
            self.active_start = 0;
            self.active_end = 0;
            return;
        };
        let Ok(index) = self
            .paths
            .binary_search_by(|candidate| candidate.path.as_path().cmp(path))
        else {
            self.active_start = 0;
            self.active_end = 0;
            return;
        };
        self.active_start = self.paths[index].owner_start;
        self.active_end = self.paths[index].owner_end;
    }

    fn owns<S: NoteskinSlot + Clone>(
        &self,
        now: f32,
        visuals: &SongLuaRuntimeVisuals<
            SongLuaOverlayActor<S>,
            SongLuaCapturedActor,
            SongLuaRuntimeOverlayStateDelta,
        >,
        overlay_states: &[SongLuaOverlayState],
        background_layer_states: &[Vec<SongLuaOverlayState>],
        foreground_layer_states: &[Vec<SongLuaOverlayState>],
    ) -> bool {
        self.owners[self.active_start..self.active_end]
            .iter()
            .any(|owner| {
                let state = match owner.source {
                    SONG_LUA_FG_OWNER_ROOT => overlay_states.get(owner.overlay_index),
                    SONG_LUA_FG_OWNER_BACKGROUND => visuals
                        .background_visual_layers
                        .get(owner.layer_index)
                        .filter(|layer| !(now < layer.start_second))
                        .and_then(|_| background_layer_states.get(owner.layer_index))
                        .and_then(|states| states.get(owner.overlay_index)),
                    SONG_LUA_FG_OWNER_FOREGROUND => visuals
                        .foreground_visual_layers
                        .get(owner.layer_index)
                        .filter(|layer| !(now < layer.start_second))
                        .and_then(|_| foreground_layer_states.get(owner.layer_index))
                        .and_then(|states| states.get(owner.overlay_index)),
                    _ => None,
                };
                state.is_some_and(|state| state.visible && state.diffuse[3] > f32::EPSILON)
            })
    }
}
#[inline(always)]
fn song_lua_overlay_space_width<
    P: deadsync_gameplay::GameplayProfileData,
    S: NoteskinSlot + Clone,
>(
    state: &GameplayCoreState<P, S>,
) -> f32 {
    state.song_lua_visuals().screen_width.max(1.0)
}

#[inline(always)]
fn song_lua_overlay_space_height<
    P: deadsync_gameplay::GameplayProfileData,
    S: NoteskinSlot + Clone,
>(
    state: &GameplayCoreState<P, S>,
) -> f32 {
    state.song_lua_visuals().screen_height.max(1.0)
}

#[inline(always)]
fn song_lua_valid_sprite_state_index(state: SongLuaOverlayState) -> Option<u32> {
    crate::song_lua_valid_sprite_state_index(state.sprite_state_index)
}

#[inline(always)]
fn song_lua_sprite_sheet_index(
    state: SongLuaOverlayState,
    sheet: (u32, u32),
    states: &[crate::SongLuaSpriteState],
    total_elapsed: f32,
) -> Option<u32> {
    let start = song_lua_valid_sprite_state_index(state).unwrap_or(0);
    let (cols, rows) = sheet;
    let total = cols.saturating_mul(rows).max(1);
    let logical_state = if state.sprite_animate && states.len() > 1 {
        crate::sprite_custom_animation_state_from(
            states,
            start,
            total_elapsed,
            state.sprite_playback_rate,
            state.sprite_loop,
        )
    } else if state.sprite_animate && state.sprite_state_delay > 0.0 && total > 1 {
        crate::sprite_animation_state_from(
            start,
            total_elapsed,
            state.sprite_playback_rate,
            state.sprite_state_delay,
            total,
            state.sprite_loop,
        )
    } else {
        start
    };
    (total > 1
        || !states.is_empty()
        || state.sprite_animate
        || song_lua_valid_sprite_state_index(state).is_some())
    .then_some(
        states
            .get(logical_state as usize)
            .map_or(logical_state, |sprite_state| sprite_state.frame),
    )
}

fn song_lua_overlay_sprite_size(
    state: SongLuaOverlayState,
    binding: deadlib_assets::BoundTexture,
) -> Option<[f32; 2]> {
    if let Some(size) = state.size {
        return Some(size);
    }
    let tex = binding.dimensions?;
    let (width, height) = crate::sprite_image_frame_size(
        Some((tex.w as f32, tex.h as f32)),
        state.sprite_animate,
        state.sprite_state_index,
        Some(binding.sheet),
    )?;
    Some([width, height])
}

fn song_lua_overlay_sprite_offscreen(
    state: SongLuaOverlayState,
    size: [f32; 2],
    overlay_space_width: f32,
    overlay_space_height: f32,
) -> bool {
    const EDGE_SLOP: f32 = 12.0;

    if state.rot_x_deg.abs() > f32::EPSILON
        || state.rot_y_deg.abs() > f32::EPSILON
        || state.rot_z_deg.abs() > f32::EPSILON
        || state.skew_x.abs() > f32::EPSILON
        || state.skew_y.abs() > f32::EPSILON
        || state.vibrate
        || !matches!(state.effect_mode, deadlib_present::anim::EffectMode::None)
    {
        return false;
    }
    let [scale_x, scale_y] = song_lua_overlay_axis_scale(state).map(f32::abs);
    let width = size[0] * scale_x;
    let height = size[1] * scale_y;
    let left = width.mul_add(-state.halign, state.x);
    let top = height.mul_add(-state.valign, state.y);
    left >= overlay_space_width - EDGE_SLOP
        || left + width <= EDGE_SLOP
        || top >= overlay_space_height - EDGE_SLOP
        || top + height <= EDGE_SLOP
}

fn song_lua_overlay_uv_rect(
    state: SongLuaOverlayState,
    sheet_dims: Option<(u32, u32)>,
    states: &[crate::SongLuaSpriteState],
    total_elapsed: f32,
) -> Option<[f32; 4]> {
    let state_index = sheet_dims
        .and_then(|sheet| song_lua_sprite_sheet_index(state, sheet, states, total_elapsed));
    crate::sprite_texture_rect_with_offset(
        state.custom_texture_rect,
        state_index,
        sheet_dims,
        state.texcoord_offset,
    )
}

#[inline(always)]
fn song_lua_overlay_axis_scale(state: SongLuaOverlayState) -> [f32; 2] {
    crate::overlay_state_axis_scale(state)
}

#[inline(always)]
fn song_lua_overlay_z_scale(state: SongLuaOverlayState) -> f32 {
    crate::overlay_state_z_scale(state)
}

#[inline(always)]
fn song_lua_overlay_parent_uses_center_origin<S: NoteskinSlot + Clone>(
    parent_kind: &SongLuaOverlayKind<S>,
    parent_axis: f32,
    overlay_space_axis: f32,
) -> bool {
    matches!(
        parent_kind,
        SongLuaOverlayKind::Actor
            | SongLuaOverlayKind::ActorFrame
            | SongLuaOverlayKind::ActorFrameTexture { .. }
    ) && 0.5f32.mul_add(-overlay_space_axis, parent_axis).abs() <= 0.01
}

#[inline(always)]
fn song_lua_overlay_linear_2d(state: SongLuaOverlayState) -> Matrix2 {
    let [scale_x, scale_y] = song_lua_overlay_axis_scale(state);
    let skew_x = Matrix2::from_cols_array(&[1.0, 0.0, state.skew_x, 1.0]);
    let skew_y = Matrix2::from_cols_array(&[1.0, state.skew_y, 0.0, 1.0]);
    Matrix2::from_angle(state.rot_z_deg.to_radians())
        * skew_x
        * skew_y
        * Matrix2::from_diagonal(Vector2::new(scale_x, scale_y))
}

fn song_lua_overlay_set_linear_2d(
    state: &mut SongLuaOverlayState,
    linear: Matrix2,
    z_scale: f32,
) -> bool {
    const EPS: f32 = 1.0e-6;
    if !linear.is_finite() || !z_scale.is_finite() {
        return false;
    }
    let scale_x = linear.x_axis.length();
    if scale_x <= EPS {
        return false;
    }
    let rotation = linear.x_axis.y.atan2(linear.x_axis.x);
    let local = Matrix2::from_angle(-rotation) * linear;
    let scale_y = local.y_axis.y;
    if scale_y.abs() <= EPS || local.x_axis.y.abs() > 0.001 {
        return false;
    }

    state.rot_z_deg = rotation.to_degrees();
    state.skew_x = local.y_axis.x / scale_y;
    state.skew_y = 0.0;
    state.basezoom = 1.0;
    state.zoom = 1.0;
    state.basezoom_x = scale_x;
    state.zoom_x = 1.0;
    state.basezoom_y = scale_y;
    state.zoom_y = 1.0;
    state.basezoom_z = z_scale;
    state.zoom_z = 1.0;
    true
}

fn song_lua_overlay_compose_state<S: NoteskinSlot + Clone>(
    parent_kind: &SongLuaOverlayKind<S>,
    parent: SongLuaOverlayState,
    mut child: SongLuaOverlayState,
    overlay_space_width: f32,
    overlay_space_height: f32,
) -> SongLuaOverlayState {
    let [parent_scale_x, parent_scale_y] = song_lua_overlay_axis_scale(parent);
    child.z = parent.z + child.z * song_lua_overlay_z_scale(parent);
    let epsilon = 0.01;
    let raw_local_x = if matches!(
        parent_kind,
        SongLuaOverlayKind::Actor
            | SongLuaOverlayKind::ActorFrame
            | SongLuaOverlayKind::ActorFrameTexture { .. }
    ) && song_lua_overlay_parent_uses_center_origin(
        parent_kind,
        parent.x,
        overlay_space_width,
    ) && 0.5f32.mul_add(-overlay_space_width, child.x).abs() <= epsilon
    {
        0.0
    } else {
        child.x
    };
    let raw_local_y = if matches!(
        parent_kind,
        SongLuaOverlayKind::Actor
            | SongLuaOverlayKind::ActorFrame
            | SongLuaOverlayKind::ActorFrameTexture { .. }
    ) && song_lua_overlay_parent_uses_center_origin(
        parent_kind,
        parent.y,
        overlay_space_height,
    ) && 0.5f32.mul_add(-overlay_space_height, child.y).abs() <= epsilon
    {
        0.0
    } else {
        child.y
    };
    let local_x = raw_local_x * parent_scale_x;
    let local_y = raw_local_y * parent_scale_y;
    // Compare radians, the unit used by rotation-matrix coefficients. Legacy
    // Lua degree/radian cancellation can leave a few millionths of a degree;
    // that roundoff must not change parent-scale/child-rotation composition.
    let affine_2d = parent.rot_x_deg.to_radians().abs() <= f32::EPSILON
        && parent.rot_y_deg.to_radians().abs() <= f32::EPSILON
        && child.rot_x_deg.to_radians().abs() <= f32::EPSILON
        && child.rot_y_deg.to_radians().abs() <= f32::EPSILON
        && (parent.skew_x.abs() > f32::EPSILON
            || parent.skew_y.abs() > f32::EPSILON
            || child.skew_x.abs() > f32::EPSILON
            || child.skew_y.abs() > f32::EPSILON
            || ((parent_scale_x - parent_scale_y).abs() > f32::EPSILON
                && child.rot_z_deg.abs() > f32::EPSILON));
    if affine_2d {
        let parent_linear = song_lua_overlay_linear_2d(parent);
        let child_linear = song_lua_overlay_linear_2d(child);
        let position = Vector2::new(parent.x, parent.y)
            + parent_linear * Vector2::new(raw_local_x, raw_local_y);
        let z_scale = song_lua_overlay_z_scale(parent) * song_lua_overlay_z_scale(child);
        if song_lua_overlay_set_linear_2d(&mut child, parent_linear * child_linear, z_scale) {
            child.x = position.x;
            child.y = position.y;
        } else {
            let (sin_z, cos_z) = parent.rot_z_deg.to_radians().sin_cos();
            child.x = parent.x + local_x * cos_z - local_y * sin_z;
            child.y = parent.y + local_x * sin_z + local_y * cos_z;
            child.basezoom *= parent.basezoom * parent.zoom;
            child.basezoom_x *= parent.basezoom_x * parent.zoom_x;
            child.basezoom_y *= parent.basezoom_y * parent.zoom_y;
            child.basezoom_z *= parent.basezoom_z * parent.zoom_z;
            child.rot_z_deg += parent.rot_z_deg;
            child.skew_x += parent.skew_x;
            child.skew_y += parent.skew_y;
        }
    } else {
        let (sin_z, cos_z) = parent.rot_z_deg.to_radians().sin_cos();
        child.x = parent.x + local_x * cos_z - local_y * sin_z;
        child.y = parent.y + local_x * sin_z + local_y * cos_z;
        child.basezoom *= parent.basezoom * parent.zoom;
        child.basezoom_x *= parent.basezoom_x * parent.zoom_x;
        child.basezoom_y *= parent.basezoom_y * parent.zoom_y;
        child.basezoom_z *= parent.basezoom_z * parent.zoom_z;
        child.rot_z_deg += parent.rot_z_deg;
        child.skew_x += parent.skew_x;
        child.skew_y += parent.skew_y;
    }
    for i in 0..4 {
        child.diffuse[i] *= parent.diffuse[i];
    }
    child.texcoord_offset = match (parent.texcoord_offset, child.texcoord_offset) {
        (Some(parent), Some(child)) => Some([parent[0] + child[0], parent[1] + child[1]]),
        (Some(parent), None) => Some(parent),
        (None, child) => child,
    };
    child.visible = parent.visible && child.visible;
    child.mask_source |= parent.mask_source;
    child.mask_dest |= parent.mask_dest;
    for (axis, inherited) in child.inherited_vibrate.iter_mut().enumerate() {
        *inherited += parent.inherited_vibrate[axis]
            + if parent.vibrate {
                parent.effect_magnitude[axis]
            } else {
                0.0
            };
    }
    child.rot_x_deg += parent.rot_x_deg;
    child.rot_y_deg += parent.rot_y_deg;
    if let Some([left, top, right, bottom]) = child.stretch_rect
        && parent.rot_x_deg.abs() <= f32::EPSILON
        && parent.rot_y_deg.abs() <= f32::EPSILON
        && parent.rot_z_deg.abs() <= f32::EPSILON
        && parent.skew_x.abs() <= f32::EPSILON
        && parent.skew_y.abs() <= f32::EPSILON
    {
        child.stretch_rect = Some([
            left.mul_add(parent_scale_x, parent.x),
            top.mul_add(parent_scale_y, parent.y),
            right.mul_add(parent_scale_x, parent.x),
            bottom.mul_add(parent_scale_y, parent.y),
        ]);
    }
    child
}
fn song_lua_overlay_local_states_into<S: NoteskinSlot + Clone>(
    now: f32,
    overlays: &[SongLuaOverlayActor<S>],
    overlay_events: &[Vec<SongLuaOverlayMessageRuntime>],
    overlay_eases: &[SongLuaOverlayEaseWindowRuntime],
    overlay_ease_ranges: &[std::ops::Range<usize>],
    dynamic_indices: &[usize],
    update_actor_index: SongLuaOverlayIndex,
    update_ranges: &[std::ops::Range<usize>],
    visible_update_indices: &[usize],
    update_cursors: &mut [usize],
    message_caches: &mut Vec<SongLuaMessageStateCache>,
    out: &mut Vec<SongLuaOverlayState>,
) -> bool {
    let overlay_updates = song_lua_overlay_runtime_updates_at(overlays, update_actor_index);
    let update_snap =
        song_lua_overlay_update_snap(now, overlay_updates, visible_update_indices, update_cursors);
    let mut changed = false;
    if out.len() != overlays.len() {
        out.clear();
        out.extend(overlays.iter().map(|overlay| overlay.initial_state));
        changed = true;
    }
    message_caches.resize(overlays.len(), SongLuaMessageStateCache::default());
    for &idx in dynamic_indices {
        let Some(overlay) = overlays.get(idx) else {
            continue;
        };
        let next = song_lua_overlay_render_state_from(
            now,
            idx,
            overlay,
            overlay_events,
            overlay_eases,
            overlay_ease_ranges,
            overlay_updates,
            update_ranges.get(idx).cloned().unwrap_or(0..0),
            update_cursors,
            update_snap,
            &mut message_caches[idx],
        );
        changed |= out[idx] != next;
        out[idx] = next;
    }
    changed
}

fn song_lua_overlay_runtime_updates_at<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    update_actor_index: SongLuaOverlayIndex,
) -> &[crate::SongLuaOverlayRuntimeUpdateTrack] {
    update_actor_index
        .get()
        .and_then(|index| overlays.get(index))
        .and_then(|overlay| match &overlay.kind {
            SongLuaOverlayKind::UpdateTracks { tracks } => Some(tracks.as_slice()),
            _ => None,
        })
        .unwrap_or(&[])
}

#[cfg(test)]
fn song_lua_overlay_states_from_local<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    local_states: &[SongLuaOverlayState],
    screen_width: f32,
    screen_height: f32,
) -> Vec<SongLuaOverlayState> {
    let mut out = Vec::with_capacity(overlays.len());
    song_lua_overlay_states_from_local_all_into(
        overlays,
        local_states,
        screen_width,
        screen_height,
        &mut out,
    );
    out
}

fn song_lua_overlay_states_from_local_all_into<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    local_states: &[SongLuaOverlayState],
    screen_width: f32,
    screen_height: f32,
    out: &mut Vec<SongLuaOverlayState>,
) {
    out.clear();
    out.reserve(overlays.len());
    for (idx, overlay) in overlays.iter().enumerate() {
        let local = local_states.get(idx).copied().unwrap_or_default();
        let composed = overlay
            .parent_index
            .and_then(|parent_index| {
                out.get(parent_index)
                    .copied()
                    .zip(overlays.get(parent_index))
            })
            .map(|(parent, parent_overlay)| {
                song_lua_overlay_compose_state(
                    &parent_overlay.kind,
                    parent,
                    local,
                    screen_width,
                    screen_height,
                )
            })
            .unwrap_or(local);
        out.push(composed);
    }
}

fn song_lua_overlay_states_from_local_into<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    local_states: &[SongLuaOverlayState],
    dynamic_indices: &[usize],
    screen_width: f32,
    screen_height: f32,
    out: &mut Vec<SongLuaOverlayState>,
) {
    if out.len() != overlays.len() {
        song_lua_overlay_states_from_local_all_into(
            overlays,
            local_states,
            screen_width,
            screen_height,
            out,
        );
        return;
    }
    for &idx in dynamic_indices {
        let Some(overlay) = overlays.get(idx) else {
            continue;
        };
        let local = local_states.get(idx).copied().unwrap_or_default();
        out[idx] = overlay
            .parent_index
            .and_then(|parent_index| {
                out.get(parent_index)
                    .copied()
                    .zip(overlays.get(parent_index))
            })
            .map(|(parent, parent_overlay)| {
                song_lua_overlay_compose_state(
                    &parent_overlay.kind,
                    parent,
                    local,
                    screen_width,
                    screen_height,
                )
            })
            .unwrap_or(local);
    }
}

fn song_lua_overlay_initial_state_sets<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    screen_width: f32,
    screen_height: f32,
) -> (Vec<SongLuaOverlayState>, Vec<SongLuaOverlayState>) {
    let local = overlays
        .iter()
        .map(|overlay| overlay.initial_state)
        .collect::<Vec<_>>();
    let mut composed = Vec::with_capacity(overlays.len());
    song_lua_overlay_states_from_local_all_into(
        overlays,
        &local,
        screen_width,
        screen_height,
        &mut composed,
    );
    (local, composed)
}

fn song_lua_overlay_state_sets_from_into<S: NoteskinSlot + Clone>(
    now: f32,
    overlays: &[SongLuaOverlayActor<S>],
    overlay_events: &[Vec<SongLuaOverlayMessageRuntime>],
    overlay_eases: &[SongLuaOverlayEaseWindowRuntime],
    overlay_ease_ranges: &[std::ops::Range<usize>],
    screen_width: f32,
    screen_height: f32,
    order_cache: &mut SongLuaOverlayOrderCache,
    message_caches: &mut Vec<SongLuaMessageStateCache>,
    local_out: &mut Vec<SongLuaOverlayState>,
    overlay_out: &mut Vec<SongLuaOverlayState>,
) {
    if overlays.is_empty() {
        message_caches.clear();
        local_out.clear();
        overlay_out.clear();
        return;
    }
    song_lua_overlay_state_sets_active_into(
        now,
        overlays,
        overlay_events,
        overlay_eases,
        overlay_ease_ranges,
        screen_width,
        screen_height,
        order_cache,
        message_caches,
        local_out,
        overlay_out,
    );
}

fn song_lua_overlay_state_sets_active_into<S: NoteskinSlot + Clone>(
    now: f32,
    overlays: &[SongLuaOverlayActor<S>],
    overlay_events: &[Vec<SongLuaOverlayMessageRuntime>],
    overlay_eases: &[SongLuaOverlayEaseWindowRuntime],
    overlay_ease_ranges: &[std::ops::Range<usize>],
    screen_width: f32,
    screen_height: f32,
    order_cache: &mut SongLuaOverlayOrderCache,
    message_caches: &mut Vec<SongLuaMessageStateCache>,
    local_out: &mut Vec<SongLuaOverlayState>,
    overlay_out: &mut Vec<SongLuaOverlayState>,
) {
    let SongLuaOverlayOrderCache {
        dynamic_local_indices,
        dynamic_composed_indices,
        update_actor_index,
        update_ranges,
        visible_update_indices,
        update_cursors,
        ..
    } = order_cache;
    let local_changed = song_lua_overlay_local_states_into(
        now,
        overlays,
        overlay_events,
        overlay_eases,
        overlay_ease_ranges,
        dynamic_local_indices,
        *update_actor_index,
        update_ranges,
        visible_update_indices,
        update_cursors,
        message_caches,
        local_out,
    );
    if local_changed || overlay_out.len() != overlays.len() {
        song_lua_overlay_states_from_local_into(
            overlays,
            local_out,
            dynamic_composed_indices,
            screen_width,
            screen_height,
            overlay_out,
        );
    }
}
fn apply_song_lua_taps<S: NoteskinSlot + Clone>(
    feedback: &deadsync_gameplay::GameplayVisualFeedbackState,
    cols_per_player: usize,
    now: f32,
    effect_time: f32,
    overlays: &[SongLuaOverlayActor<S>],
    order: &mut SongLuaOverlayOrderCache,
    local: &mut [SongLuaOverlayState],
    composed: &mut Vec<SongLuaOverlayState>,
    screen: [f32; 2],
) {
    let mut changed = false;
    for tap in &mut order.tap_commands {
        if tap.player >= MAX_PLAYERS || tap.column >= cols_per_player {
            continue;
        }
        let Some(judgment) = feedback.last_tap_judgment(tap.player * cols_per_player + tap.column)
        else {
            continue;
        };
        if judgment.grade != tap.grade {
            continue;
        }
        let elapsed = now - judgment.at_screen_s;
        if elapsed < 0.0 {
            continue;
        }
        let current = local[tap.overlay];
        let mut next = crate::overlay_state_after_blocks(
            current,
            &overlays[tap.overlay].message_commands[tap.command].blocks,
            elapsed,
        );
        if next.effect_mode == deadlib_present::anim::EffectMode::GlowShift
            && next.effect_clock == deadlib_present::anim::EffectClock::Time
        {
            // Actor::ResetEffectTimeIfDifferent leaves repeated glowshift
            // commands running, including while the explosion is invisible.
            let start = tap.glow_started_at.get_or_insert(judgment.at_screen_s);
            next.effect_offset += now - *start - effect_time;
        }
        changed |= next != current;
        local[tap.overlay] = next;
    }
    if changed {
        song_lua_overlay_states_from_local_into(
            overlays,
            local,
            &order.dynamic_composed_indices,
            screen[0],
            screen[1],
            composed,
        );
    }
}

fn song_lua_proxy_target_has_source(
    target: &SongLuaProxyTarget,
    proxy_sources: &[SongLuaPlayerProxySources<'_>; 2],
) -> bool {
    match target {
        SongLuaProxyTarget::Player { player_index } => {
            proxy_sources.get(*player_index).is_some_and(|sources| {
                sources.direct_player || sources.player.is_some_and(|source| !source.is_empty())
            })
        }
        SongLuaProxyTarget::NoteField { player_index } => {
            proxy_sources.get(*player_index).is_some_and(|sources| {
                sources.direct_note_field
                    || sources.note_field.is_some_and(|source| !source.is_empty())
            })
        }
        SongLuaProxyTarget::Judgment { player_index } => proxy_sources
            .get(*player_index)
            .and_then(|sources| sources.judgment)
            .is_some_and(|source| !source.is_empty()),
        SongLuaProxyTarget::Combo { player_index } => {
            proxy_sources.get(*player_index).is_some_and(|sources| {
                sources.direct_combo || sources.combo.is_some_and(|source| !source.is_empty())
            })
        }
        SongLuaProxyTarget::Underlay { .. }
        | SongLuaProxyTarget::Overlay { .. }
        | SongLuaProxyTarget::Actor { .. } => false,
    }
}
fn song_lua_proxy_active_players_indexed<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    overlay_states: &[SongLuaOverlayState],
    proxy_sources: &[SongLuaPlayerProxySources<'_>; 2],
    index: &SongLuaProxyRequestIndex,
) -> [bool; 2] {
    let mut out = [false; 2];
    for &overlay_index in &index.proxy_indices {
        let SongLuaOverlayKind::ActorProxy { target } = &overlays[overlay_index].kind else {
            continue;
        };
        let player_index = match target {
            SongLuaProxyTarget::Player { player_index }
            | SongLuaProxyTarget::NoteField { player_index } => *player_index,
            _ => continue,
        };
        if player_index >= out.len() || !song_lua_proxy_target_has_source(target, proxy_sources) {
            continue;
        }
        if overlay_states
            .get(overlay_index)
            .copied()
            .is_some_and(song_lua_overlay_is_visible)
        {
            out[player_index] = true;
        }
    }
    out
}

fn song_lua_replacement_active_players_indexed<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    overlay_states: &[SongLuaOverlayState],
    proxy_sources: &[SongLuaPlayerProxySources<'_>; 2],
    index: &SongLuaProxyRequestIndex,
    visit_scratch: &mut SongLuaCaptureVisitScratch,
) -> [bool; 2] {
    if index.proxy_indices.is_empty() {
        return [false; 2];
    }
    song_lua_replacement_active_players_indexed_active(
        overlays,
        overlay_states,
        proxy_sources,
        index,
        visit_scratch,
    )
}

fn song_lua_replacement_active_players_indexed_active<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    overlay_states: &[SongLuaOverlayState],
    proxy_sources: &[SongLuaPlayerProxySources<'_>; 2],
    index: &SongLuaProxyRequestIndex,
    visit_scratch: &mut SongLuaCaptureVisitScratch,
) -> [bool; 2] {
    let mut out =
        song_lua_proxy_active_players_indexed(overlays, overlay_states, proxy_sources, index);
    let mut requests = SongLuaScreenProxyRequests::default();
    visit_scratch.begin(overlays.len());
    for &overlay_index in &index.topology.aft_sprite_indices {
        let Some(capture_index) = index.topology.aft_sprite_targets[overlay_index].get() else {
            continue;
        };
        let Some(overlay_state) = overlay_states.get(overlay_index) else {
            continue;
        };
        if !song_lua_overlay_is_visible(*overlay_state) {
            continue;
        }
        song_lua_collect_capture_requests_indexed(
            overlays,
            overlay_states,
            capture_index,
            index,
            &mut requests,
            visit_scratch,
        );
    }
    for (player_index, active) in out.iter_mut().enumerate() {
        let player_requests = requests.players[player_index];
        *active |= (player_requests.player
            && (proxy_sources[player_index].direct_player
                || proxy_sources[player_index].player.is_some()))
            || (player_requests.note_field && proxy_sources[player_index].note_field.is_some());
    }
    out
}

fn song_lua_overlay_aft_ancestor<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    mut index: usize,
) -> Option<usize> {
    while let Some(parent_index) = overlays.get(index).and_then(|overlay| overlay.parent_index) {
        match overlays.get(parent_index).map(|overlay| &overlay.kind) {
            Some(SongLuaOverlayKind::ActorFrameTexture { .. }) => return Some(parent_index),
            Some(_) => index = parent_index,
            None => return None,
        }
    }
    None
}

fn song_lua_overlay_camera_ancestor<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    mut index: Option<usize>,
) -> Option<usize> {
    while let Some(current) = index {
        let overlay = overlays.get(current)?;
        if matches!(
            overlay.kind,
            SongLuaOverlayKind::ActorFrame | SongLuaOverlayKind::ActorFrameTexture { .. }
        ) {
            return Some(current);
        }
        index = overlay.parent_index;
    }
    None
}

fn song_lua_overlay_capture_index_by_name<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    capture_name: &str,
) -> Option<usize> {
    overlays.iter().position(|overlay| {
        matches!(overlay.kind, SongLuaOverlayKind::ActorFrameTexture { .. })
            && overlay
                .name
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case(capture_name))
    })
}

#[derive(Clone, Copy)]
struct SongLuaProxySource<'a> {
    segments: &'a [Arc<[Actor]>],
    offset: [f32; 2],
    pool_class: usize,
    source_view_proj: Option<Matrix4>,
}

impl<'a> SongLuaProxySource<'a> {
    #[inline(always)]
    const fn new(segments: &'a [Arc<[Actor]>]) -> Self {
        Self {
            segments,
            offset: [0.0, 0.0],
            source_view_proj: None,
            pool_class: if segments.len() == 1 {
                SONG_LUA_SMALL_PROXY_CLASS
            } else {
                SONG_LUA_MULTI_PROXY_CLASS
            },
        }
    }

    #[inline(always)]
    const fn offset(segments: &'a [Arc<[Actor]>], offset: [f32; 2]) -> Self {
        Self {
            segments,
            offset,
            source_view_proj: None,
            pool_class: if segments.len() == 1 {
                SONG_LUA_SMALL_PROXY_CLASS
            } else {
                SONG_LUA_MULTI_PROXY_CLASS
            },
        }
    }

    #[inline(always)]
    const fn with_pool_class(self, pool_class: usize) -> Self {
        Self { pool_class, ..self }
    }

    #[inline(always)]
    const fn is_empty(self) -> bool {
        self.segments.is_empty()
    }
}

#[derive(Clone, Copy, Default)]
struct SongLuaPlayerProxySources<'a> {
    player: Option<SongLuaProxySource<'a>>,
    direct_player: bool,
    note_field: Option<SongLuaProxySource<'a>>,
    direct_note_field: bool,
    judgment: Option<SongLuaProxySource<'a>>,
    combo: Option<SongLuaProxySource<'a>>,
    direct_combo: bool,
}

#[derive(Clone, Copy)]
struct SongLuaDirectProxySource {
    draws: SongLuaDirectDraws,
    draw_start: usize,
    draw_end: usize,
    target: [f32; 2],
    tint: [f32; 4],
    x_fold: Option<ActorXFold>,
    camera: Option<Matrix4>,
    player_camera: Option<SongLuaDirectPlayerCamera>,
}

#[derive(Clone, Copy)]
struct SongLuaDirectPlayerCamera {
    base: Matrix4,
    suffix: Matrix4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SongLuaDirectDraws {
    Field,
    Hud,
    Judgment,
    Combo,
}

type SongLuaDirectPlayerSource = [SongLuaDirectProxySource; 2];

#[derive(Clone, Copy, Debug, PartialEq)]
struct SongLuaDirectProxy {
    actor_insert: usize,
    player: usize,
    draws: SongLuaDirectDraws,
    draw_start: usize,
    draw_end: usize,
    offset: [f32; 2],
    z: i16,
    style: FlatProxyStyle,
    blend: BlendMode,
    enclosing_camera: Option<Matrix4>,
    camera: Option<Matrix4>,
    tail: Option<SongLuaDirectProxyPart>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct SongLuaDirectProxyPart {
    draws: SongLuaDirectDraws,
    draw_start: usize,
    draw_end: usize,
    camera: Option<Matrix4>,
}

/// Song-lifetime backing for direct root `ActorProxy` and exact AFT destinations.
///
/// Owner/thread model: gameplay frame scratch, used only by the game/render
/// thread. Lifetime: one song. Capacity is the compiled maximum: one entry per
/// `NoteField`, Judgment, Combo, or exact single-source AFT destination and two
/// entries per root whole-Player destination across the main and every
/// foreground layer, reserved at screen entry. Exact AFT Player destinations
/// retain both source runs in one entry. Each frame clears and refills the
/// retained storage; exceeding the compiled bound is a topology invariant
/// violation.
/// There is no miss path, eviction, pruning, synchronization, or gameplay-time
/// destruction. Focused proxy fixtures and warmed allocation tests cover the
/// capacity contract. Worst-case frame work is one append per visible compiled
/// direct destination, with two appends for a visible root whole-Player source.
#[derive(Debug, Default, PartialEq)]
struct SongLuaDirectProxies {
    entries: Vec<SongLuaDirectProxy>,
}

impl SongLuaDirectProxies {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
        }
    }

    fn clear(&mut self) {
        self.entries.clear();
    }

    fn push(&mut self, proxy: SongLuaDirectProxy) {
        assert!(
            self.entries.len() < self.entries.capacity(),
            "direct proxy plan exceeded its compiled root-destination bound"
        );
        self.entries.push(proxy);
    }

    const fn len(&self) -> usize {
        self.entries.len()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SongLuaPlayerProxyRequests {
    player: bool,
    note_field: bool,
    judgment: bool,
    combo: bool,
}

#[derive(Clone, Copy, Default)]
struct SongLuaScreenProxySources<'a> {
    players: [SongLuaPlayerProxySources<'a>; 2],
    direct_players: [Option<SongLuaDirectPlayerSource>; 2],
    direct_note_fields: [Option<SongLuaDirectProxySource>; 2],
    direct_judgments: [Option<SongLuaDirectProxySource>; 2],
    direct_combos: [Option<SongLuaDirectProxySource>; 2],
    underlay: Option<&'a [Arc<[Actor]>]>,
    overlay: Option<&'a [Arc<[Actor]>]>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SongLuaScreenProxyRequests {
    players: [SongLuaPlayerProxyRequests; 2],
    underlay: bool,
    overlay: bool,
    hide_underlay: bool,
    hide_overlay: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SongLuaProxyRequestAnalysis {
    all: SongLuaScreenProxyRequests,
    captured: SongLuaScreenProxyRequests,
    root_players: [u8; MAX_PLAYERS],
    root_note_fields: [u8; MAX_PLAYERS],
    root_judgments: [u8; MAX_PLAYERS],
    root_combos: [u8; MAX_PLAYERS],
}

type SongLuaSingleSource = [Arc<[Actor]>; 1];
type SongLuaActorSegments = SmallVec<[Arc<[Actor]>; 5]>;

struct PreparedProxySource {
    segments: SongLuaSingleSource,
    // Remove the source origin after its own transform, before the proxy's.
    offset: [f32; 2],
    root_camera: bool,
}

impl PreparedProxySource {
    #[inline(always)]
    const fn new(segments: SongLuaSingleSource) -> Self {
        Self {
            segments,
            offset: [0.0, 0.0],
            root_camera: false,
        }
    }

    fn view(&self) -> SongLuaProxySource<'_> {
        // Prepared Player/child transforms use the Player root camera. Decode
        // that same depth range before an outer proxy rotates the source.
        SongLuaProxySource {
            source_view_proj: self
                .root_camera
                .then(|| song_lua_player_root_camera(Matrix4::IDENTITY)),
            ..SongLuaProxySource::offset(&self.segments, self.offset)
        }
    }
}

#[derive(Clone, Copy)]
enum ProxyCapturePart {
    Field,
    Hud,
}

const SONG_LUA_SCREEN_CAPTURE_SLOTS: usize = 64;
const SONG_LUA_SCREEN_CAPTURE_CAPACITY: usize = 32;
const SONG_LUA_PROXY_FRAME_BANKS: usize = 2;
const SONG_LUA_PROXY_SEGMENTS_PER_ACTOR: usize = 5;
const SONG_LUA_AFT_FRAME_BANKS: usize = 2;
const SONG_LUA_PLAYER_PROXY_SOURCE_COUNT: usize = 4;
const SONG_LUA_FIELD_PROXY_SOURCE: usize = 0;
const SONG_LUA_JUDGMENT_PROXY_SOURCE: usize = 1;
const SONG_LUA_COMBO_PROXY_SOURCE: usize = 2;
const SONG_LUA_PLAYER_PROXY_SOURCE: usize = 3;
const SONG_LUA_PROXY_POOL_CLASSES: usize = 4;
const SONG_LUA_SMALL_PROXY_CLASS: usize = 0;
const SONG_LUA_NOTEFIELD_PROXY_CLASS: usize = 1;
const SONG_LUA_PLAYER_PROXY_CLASS: usize = 2;
const SONG_LUA_MULTI_PROXY_CLASS: usize = 3;
const SONG_LUA_PROXY_SEGMENTS_PER_GROUP: [usize; SONG_LUA_PROXY_POOL_CLASSES] =
    [1, 1, 1, SONG_LUA_PROXY_SEGMENTS_PER_ACTOR];
const SONG_LUA_PROXY_SEGMENT_CAPACITIES: [usize; SONG_LUA_PROXY_POOL_CLASSES] = [
    SONG_LUA_SCREEN_CAPTURE_CAPACITY,
    NOTEFIELD_ACTOR_SCRATCH_CAPACITY,
    PLAYER_ACTOR_SCRATCH_CAPACITY,
    SONG_LUA_SCREEN_CAPTURE_CAPACITY,
];

const fn song_lua_proxy_pool_class(target: &SongLuaProxyTarget) -> usize {
    match target {
        SongLuaProxyTarget::Player { .. } => SONG_LUA_PLAYER_PROXY_CLASS,
        SongLuaProxyTarget::NoteField { .. } => SONG_LUA_NOTEFIELD_PROXY_CLASS,
        SongLuaProxyTarget::Judgment { .. } | SongLuaProxyTarget::Combo { .. } => {
            SONG_LUA_SMALL_PROXY_CLASS
        }
        SongLuaProxyTarget::Underlay { .. } | SongLuaProxyTarget::Overlay { .. } => {
            SONG_LUA_MULTI_PROXY_CLASS
        }
        SongLuaProxyTarget::Actor { .. } => SONG_LUA_SMALL_PROXY_CLASS,
    }
}

fn song_lua_proxy_pool_counts<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    index: &SongLuaProxyRequestIndex,
) -> [usize; SONG_LUA_PROXY_POOL_CLASSES] {
    let mut counts = [0usize; SONG_LUA_PROXY_POOL_CLASSES];
    for &proxy_index in &index.proxy_indices {
        let Some(SongLuaOverlayKind::ActorProxy { target }) =
            overlays.get(proxy_index).map(|overlay| &overlay.kind)
        else {
            continue;
        };
        let class = song_lua_proxy_pool_class(target);
        counts[class] = counts[class].saturating_add(1);
    }
    counts
}

struct SongLuaProxyJoinScratch {
    sources: [Arc<[Actor]>; SONG_LUA_PROXY_SEGMENTS_PER_ACTOR],
    _replacements: u64,
}

impl SongLuaProxyJoinScratch {
    fn new() -> Self {
        Self {
            sources: std::array::from_fn(|index| Self::new_source(index + 1)),
            _replacements: 0,
        }
    }

    fn new_source(len: usize) -> Arc<[Actor]> {
        Arc::from(
            (0..len)
                .map(|_| Actor::SharedFrame {
                    align: [0.0, 0.0],
                    offset: [0.0, 0.0],
                    size: [SizeSpec::Fill, SizeSpec::Fill],
                    children: Arc::from([]),
                    background: None,
                    z: 0,
                    tint: [1.0; 4],
                    blend: None,
                })
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        )
    }

    fn refill(
        &mut self,
        segment_count: usize,
        diffuse: [f32; 4],
        blend: Option<BlendMode>,
        mut segment: impl FnMut(usize) -> Arc<[Actor]>,
    ) -> Arc<[Actor]> {
        let source = &mut self.sources[segment_count - 1];
        if Arc::strong_count(source) != 1 {
            *source = Self::new_source(segment_count);
            self._replacements = self._replacements.saturating_add(1);
        }
        let actors =
            Arc::get_mut(source).expect("proxy join source is uniquely owned after replacement");
        for (index, actor) in actors.iter_mut().enumerate() {
            let Actor::SharedFrame {
                children,
                tint,
                blend: actor_blend,
                ..
            } = actor
            else {
                panic!("proxy join source must remain shared frames");
            };
            *children = segment(index);
            *tint = diffuse;
            *actor_blend = blend;
        }
        Arc::clone(source)
    }
}

/// Song-lifetime backing for Song Lua `ActorProxy` sources.
///
/// Owner/thread model: gameplay `State`, exclusively mutated during actor
/// composition on the game/render frame thread. Lifetime: one song, allocated
/// only when a compiled overlay contains a proxy. Capacity/warmup: 64 bounded
/// screen segments and four pre-sized player sources per active player are
/// reserved at screen entry. Player, note-field, judgment, and combo proxies
/// each reserve their only possible normalized source segment. Underlay and
/// overlay proxies reserve up to five bounded segments and a prebuilt join
/// frame. Fixed target pools retain O(1) reservation. Two banks let composition
/// proceed while the renderer still owns the preceding frame. A
/// miss after slot saturation bypasses insertion and uses the owned fallback;
/// child-vector overflow grows only the affected slot. There is no lookup,
/// eviction, scan, pruning, or mid-song destruction. Replacements/growths are
/// tracked by each backing slot. Worst-case steady frame work is linear in
/// actors already emitted for the requested captures.
struct SongLuaProxyActorBank {
    screen: [SharedActorFrameScratch; SONG_LUA_SCREEN_CAPTURE_SLOTS],
    screen_used: usize,
    players: [[SharedActorFrameScratch; SONG_LUA_PLAYER_PROXY_SOURCE_COUNT]; MAX_PLAYERS],
    proxy_segments: Vec<SharedActorFrameScratch>,
    proxy_frames: Vec<SongLuaProxyJoinScratch>,
    proxy_segment_offsets: [usize; SONG_LUA_PROXY_POOL_CLASSES + 1],
    proxy_groups_used: [usize; SONG_LUA_PROXY_POOL_CLASSES],
}

impl SongLuaProxyActorBank {
    fn new(active_players: usize, proxy_counts: [usize; SONG_LUA_PROXY_POOL_CLASSES]) -> Self {
        let mut proxy_segment_offsets = [0usize; SONG_LUA_PROXY_POOL_CLASSES + 1];
        for class in 0..SONG_LUA_PROXY_POOL_CLASSES {
            proxy_segment_offsets[class + 1] = proxy_segment_offsets[class].saturating_add(
                proxy_counts[class].saturating_mul(SONG_LUA_PROXY_SEGMENTS_PER_GROUP[class]),
            );
        }
        let mut proxy_segments =
            Vec::with_capacity(proxy_segment_offsets[SONG_LUA_PROXY_POOL_CLASSES]);
        for class in 0..SONG_LUA_PROXY_POOL_CLASSES {
            for _ in 0..proxy_counts[class] {
                for _ in 0..SONG_LUA_PROXY_SEGMENTS_PER_GROUP[class] {
                    proxy_segments.push(SharedActorFrameScratch::with_capacity(
                        SONG_LUA_PROXY_SEGMENT_CAPACITIES[class],
                    ));
                }
            }
        }
        Self {
            screen: std::array::from_fn(|_| {
                SharedActorFrameScratch::with_capacity(SONG_LUA_SCREEN_CAPTURE_CAPACITY)
            }),
            screen_used: 0,
            players: std::array::from_fn(|player| {
                let active = player < active_players;
                [
                    SharedActorFrameScratch::with_capacity(
                        usize::from(active) * NOTEFIELD_ACTOR_SCRATCH_CAPACITY,
                    ),
                    SharedActorFrameScratch::with_capacity(
                        usize::from(active) * NOTEFIELD_HUD_ACTOR_SCRATCH_CAPACITY,
                    ),
                    SharedActorFrameScratch::with_capacity(
                        usize::from(active) * NOTEFIELD_HUD_ACTOR_SCRATCH_CAPACITY,
                    ),
                    SharedActorFrameScratch::with_capacity(
                        usize::from(active) * PLAYER_ACTOR_SCRATCH_CAPACITY,
                    ),
                ]
            }),
            proxy_segments,
            proxy_frames: (0..proxy_counts[SONG_LUA_MULTI_PROXY_CLASS])
                .map(|_| SongLuaProxyJoinScratch::new())
                .collect(),
            proxy_segment_offsets,
            proxy_groups_used: [0; SONG_LUA_PROXY_POOL_CLASSES],
        }
    }

    fn begin_frame(&mut self) {
        self.screen_used = 0;
        self.proxy_groups_used.fill(0);
        for player in &mut self.players {
            for source in player {
                source.clear();
            }
        }
    }
}

struct SongLuaProxyActorScratch {
    banks: [SongLuaProxyActorBank; SONG_LUA_PROXY_FRAME_BANKS],
    active_bank: usize,
    frame_banks: usize,
}

impl SongLuaProxyActorScratch {
    #[cfg(test)]
    fn new(active_players: usize) -> Self {
        Self::with_proxy_counts(active_players, [1, 0, 0, 0])
    }

    fn with_proxy_counts(
        active_players: usize,
        proxy_counts: [usize; SONG_LUA_PROXY_POOL_CLASSES],
    ) -> Self {
        Self::with_proxy_counts_and_banks(active_players, proxy_counts, SONG_LUA_PROXY_FRAME_BANKS)
    }

    fn with_proxy_counts_and_banks(
        active_players: usize,
        proxy_counts: [usize; SONG_LUA_PROXY_POOL_CLASSES],
        frame_banks: usize,
    ) -> Self {
        let frame_banks = frame_banks.clamp(1, SONG_LUA_PROXY_FRAME_BANKS);
        Self {
            banks: std::array::from_fn(|_| {
                SongLuaProxyActorBank::new(active_players, proxy_counts)
            }),
            active_bank: frame_banks - 1,
            frame_banks,
        }
    }

    fn begin_frame(&mut self) {
        self.active_bank = (self.active_bank + 1) % self.frame_banks;
        self.banks[self.active_bank].begin_frame();
    }

    fn next_screen(&mut self) -> Option<&mut SharedActorFrameScratch> {
        let bank = &mut self.banks[self.active_bank];
        let slot = bank.screen.get_mut(bank.screen_used)?;
        bank.screen_used += 1;
        Some(slot)
    }

    fn player(&mut self, player: usize, source: usize) -> Option<&mut SharedActorFrameScratch> {
        self.banks[self.active_bank]
            .players
            .get_mut(player)?
            .get_mut(source)
    }

    fn player_source(&self, player: usize, source: usize) -> Option<&[Actor]> {
        self.banks[self.active_bank]
            .players
            .get(player)?
            .get(source)
            .map(SharedActorFrameScratch::actors)
    }

    fn reserve_proxy_group(&mut self, pool_class: usize) -> Option<SongLuaProxyGroup> {
        let bank = &mut self.banks[self.active_bank];
        let used = bank.proxy_groups_used.get_mut(pool_class)?;
        let local_group = *used;
        *used = used.saturating_add(1);
        let segment_start = bank.proxy_segment_offsets[pool_class].saturating_add(
            local_group.saturating_mul(SONG_LUA_PROXY_SEGMENTS_PER_GROUP[pool_class]),
        );
        if segment_start >= bank.proxy_segment_offsets[pool_class + 1] {
            return None;
        }
        Some(SongLuaProxyGroup {
            frame_index: (pool_class == SONG_LUA_MULTI_PROXY_CLASS).then_some(local_group),
            segment_start,
        })
    }

    fn normalize_segment(&mut self, segment: &Arc<[Actor]>, slot_index: usize) -> Arc<[Actor]> {
        let bank = &mut self.banks[self.active_bank];
        song_lua_normalize_proxy_segment(segment, bank.proxy_segments.get_mut(slot_index))
    }

    fn join_proxy_segments(
        &mut self,
        group: SongLuaProxyGroup,
        source: &[Arc<[Actor]>],
        diffuse: [f32; 4],
        blend: Option<BlendMode>,
    ) -> Arc<[Actor]> {
        let bank = &mut self.banks[self.active_bank];
        let proxy_frames = &mut bank.proxy_frames;
        let proxy_segments = &mut bank.proxy_segments;
        let frame_index = group
            .frame_index
            .expect("multi-segment proxy group requires join storage");
        proxy_frames[frame_index].refill(source.len(), diffuse, blend, |index| {
            song_lua_normalize_proxy_segment(
                &source[index],
                proxy_segments.get_mut(group.segment_start + index),
            )
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SongLuaProxyGroup {
    frame_index: Option<usize>,
    segment_start: usize,
}

fn song_lua_normalize_proxy_segment(
    segment: &Arc<[Actor]>,
    slot: Option<&mut SharedActorFrameScratch>,
) -> Arc<[Actor]> {
    if !segment.iter().any(song_lua_proxy_actor_has_z) {
        return Arc::clone(segment);
    }
    let Some(slot) = slot else {
        return song_lua_proxy_source_segment_owned(segment);
    };
    let (offset, actors) = song_lua_proxy_segment_actors(segment);
    slot.refill(offset, |children| {
        song_lua_proxy_local_children_into(actors.iter().cloned(), children);
    })
    .unwrap_or_else(|| Arc::from([]))
}

/// Song-lifetime backing for `ActorFrameTexture` capture output.
///
/// Owner/thread model: gameplay `State`, exclusively mutated during actor
/// composition on the game/render frame thread. Lifetime: one song. Capacity:
/// each AFT has two frame banks sized at screen entry from its compiled
/// capture descendants, including the maximum two actors emitted by glow and
/// model layers. Warmup occurs during screen initialization. Gameplay misses
/// grow only the affected slot and retain that high-water mark; there is no
/// lookup, eviction, pruning, or gameplay destruction. Two banks cover the
/// renderer retaining the preceding frame. Per-slot replacement/growth counts
/// are available from `SharedActorFrameScratch::stats`. Worst-case frame work
/// remains linear in the already-required captured actors.
#[derive(Default)]
struct SongLuaAftCaptureScratch {
    slots: Vec<Option<[SharedActorFrameScratch; SONG_LUA_AFT_FRAME_BANKS]>>,
    active_bank: usize,
}

impl SongLuaAftCaptureScratch {
    fn new<S: NoteskinSlot + Clone>(
        overlays: &[SongLuaOverlayActor<S>],
        topology: &SongLuaOverlayTopologyIndex,
    ) -> Self {
        let slots = overlays
            .iter()
            .enumerate()
            .map(|(index, overlay)| {
                matches!(overlay.kind, SongLuaOverlayKind::ActorFrameTexture { .. }).then(|| {
                    let capacity = song_lua_aft_capture_capacity(overlays, topology, index);
                    std::array::from_fn(|_| SharedActorFrameScratch::with_capacity(capacity))
                })
            })
            .collect();
        Self {
            slots,
            active_bank: SONG_LUA_AFT_FRAME_BANKS - 1,
        }
    }

    const fn begin_frame(&mut self) {
        self.active_bank = (self.active_bank + 1) % SONG_LUA_AFT_FRAME_BANKS;
    }

    fn overlay(&mut self, index: usize) -> Option<&mut SharedActorFrameScratch> {
        self.slots
            .get_mut(index)?
            .as_mut()
            .map(|banks| &mut banks[self.active_bank])
    }
}

fn song_lua_aft_actor_capacity<S: NoteskinSlot + Clone>(kind: &SongLuaOverlayKind<S>) -> usize {
    match kind {
        SongLuaOverlayKind::Actor
        | SongLuaOverlayKind::ActorFrame
        | SongLuaOverlayKind::ActorFrameTexture { .. }
        | SongLuaOverlayKind::Sound { .. } => 0,
        SongLuaOverlayKind::AftSprite { .. } => 2,
        SongLuaOverlayKind::ActorProxy { .. } => 1,
        SongLuaOverlayKind::Model { layers } => layers.len().saturating_mul(2),
        SongLuaOverlayKind::NoteskinActor { slots } => slots.len().saturating_mul(2),
        _ => 2,
    }
}

fn song_lua_aft_capture_capacity<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    topology: &SongLuaOverlayTopologyIndex,
    capture_index: usize,
) -> usize {
    overlays
        .iter()
        .enumerate()
        .filter(|(index, _)| {
            topology
                .aft_ancestors
                .get(*index)
                .copied()
                .and_then(SongLuaOverlayIndex::get)
                == Some(capture_index)
        })
        .fold(4usize, |capacity, (_, overlay)| {
            capacity.saturating_add(song_lua_aft_actor_capacity(&overlay.kind))
        })
}

#[derive(Clone, Copy)]
struct SongLuaCaptureTransform {
    z_shift: i16,
    tint: [f32; 4],
    blend: Option<BlendMode>,
    playfield_center_x: f32,
    target_x: f32,
    target_y: f32,
    rotation_x: f32,
    rotation_z: f32,
    rotation_y: f32,
    skew_x: f32,
    skew_y: f32,
    zoom_x: f32,
    zoom_y: f32,
    zoom_z: f32,
}

#[inline(always)]
fn song_lua_overlay_is_visible(state: SongLuaOverlayState) -> bool {
    state.visible && state.diffuse[3] > f32::EPSILON
}

#[inline(always)]
fn song_lua_capture_new_actors(
    dest: &mut Option<SongLuaActorSegments>,
    actors: &mut Vec<Actor>,
    start: usize,
    scratch: Option<&mut SongLuaProxyActorScratch>,
    retain_original: bool,
) {
    let Some(dest) = dest.as_mut() else {
        return;
    };
    let children = scratch
        .and_then(SongLuaProxyActorScratch::next_screen)
        .and_then(|scratch| scratch.capture_range(actors, start))
        .or_else(|| song_lua_capture_new_actors_owned(actors, start));
    let Some(children) = children else { return };
    dest.push(Arc::clone(&children));
    if retain_original {
        actors.push(Actor::SharedFrame {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Fill, SizeSpec::Fill],
            children,
            background: None,
            z: 0,
            tint: [1.0; 4],
            blend: None,
        });
    }
}

fn song_lua_capture_new_actors_owned(
    actors: &mut Vec<Actor>,
    start: usize,
) -> Option<Arc<[Actor]>> {
    if start >= actors.len() {
        return None;
    }
    Some(Arc::from_iter(actors.drain(start..)))
}

#[cfg(test)]
fn song_lua_player_child_proxy_source(
    actors: &mut Vec<Actor>,
    origin_x: f32,
    origin_y: f32,
    scratch: &mut SharedActorFrameScratch,
) -> Option<SongLuaSingleSource> {
    scratch
        .refill([-origin_x, -origin_y], |children| children.append(actors))
        .map(|source| [source])
}

fn song_lua_render_captured_source(
    field_source: Option<&CapturedActorSource>,
    hud_source: Option<&CapturedActorSource>,
    transform: SongLuaCaptureTransform,
    scratch: &mut SharedActorFrameScratch,
) -> Option<SongLuaSingleSource> {
    if field_source.is_none() && hud_source.is_none() {
        return None;
    }
    let field_len = field_source.map_or(0, |source| {
        source
            .iter()
            .map(|segment| song_lua_note_field_proxy_actors(segment).len())
            .sum()
    });
    let hud_len = hud_source.map_or(0, |source| {
        source
            .iter()
            .map(|segment| song_lua_captured_segment_actors(segment).len())
            .sum()
    });
    let field_has_camera = field_source.is_some_and(|source| {
        source.iter().any(|segment| {
            song_lua_note_field_proxy_actors(segment)
                .iter()
                .any(|actor| {
                    matches!(
                        actor,
                        Actor::Camera { .. } | Actor::CameraPush { .. } | Actor::CameraPop
                    )
                })
        })
    });
    let field_actors = field_source
        .into_iter()
        .flat_map(|source| source.iter())
        .flat_map(|segment| song_lua_note_field_proxy_actors(segment).iter().cloned());
    let hud_actors = hud_source
        .into_iter()
        .flat_map(|source| source.iter())
        .flat_map(|segment| song_lua_captured_segment_actors(segment).iter().cloned());
    scratch
        .refill([0.0, 0.0], |out| {
            append_song_lua_player_transform(
                field_actors,
                hud_actors,
                field_len,
                hud_len,
                field_has_camera,
                out,
                transform.z_shift,
                transform.tint,
                transform.blend,
                transform.playfield_center_x,
                transform.target_x,
                transform.target_y,
                transform.rotation_x,
                transform.rotation_z,
                transform.rotation_y,
                transform.skew_x,
                transform.skew_y,
                transform.zoom_x,
                transform.zoom_y,
                transform.zoom_z,
            );
        })
        .map(|source| [source])
}

fn prepare_proxy_source(
    source: CapturedActorSource,
    part: ProxyCapturePart,
    transform: SongLuaCaptureTransform,
    scratch: &mut SharedActorFrameScratch,
) -> Option<PreparedProxySource> {
    if song_lua_player_transform_is_direct_identity(transform) {
        if matches!(part, ProxyCapturePart::Field)
            && source.iter().any(|segment| {
                song_lua_note_field_proxy_actors(segment).len()
                    != song_lua_captured_segment_actors(segment).len()
            })
        {
            let segments = scratch
                .refill([0.0, 0.0], |out| {
                    out.extend(source.iter().flat_map(|segment| {
                        song_lua_note_field_proxy_actors(segment).iter().cloned()
                    }));
                })
                .map(|source| [source])?;
            return Some(PreparedProxySource {
                segments,
                offset: [-transform.target_x, -transform.target_y],
                root_camera: false,
            });
        }
        return Some(PreparedProxySource {
            segments: source,
            offset: [-transform.target_x, -transform.target_y],
            root_camera: false,
        });
    }
    let segments = match part {
        ProxyCapturePart::Field => {
            song_lua_render_captured_source(Some(&source), None, transform, scratch)
        }
        ProxyCapturePart::Hud => {
            song_lua_render_captured_source(None, Some(&source), transform, scratch)
        }
    }?;
    Some(PreparedProxySource {
        segments,
        offset: [-transform.target_x, -transform.target_y],
        root_camera: true,
    })
}

fn prepare_flat_proxy_source(
    draws: &[FlatDraw],
    transform: SongLuaCaptureTransform,
    scratch: &mut SharedActorFrameScratch,
) -> Option<PreparedProxySource> {
    if draws.is_empty() {
        return None;
    }
    if song_lua_player_transform_is_direct_identity(transform) {
        let segments = scratch
            .refill([0.0, 0.0], |out| {
                out.extend(draws.iter().cloned().map(actor_from_flat_draw));
            })
            .map(|source| [source])?;
        return Some(PreparedProxySource {
            segments,
            offset: [-transform.target_x, -transform.target_y],
            root_camera: false,
        });
    }

    let segments = scratch
        .refill([0.0, 0.0], |out| {
            append_song_lua_player_transform(
                std::iter::empty(),
                draws.iter().cloned().map(actor_from_flat_draw),
                0,
                draws.len(),
                false,
                out,
                transform.z_shift,
                transform.tint,
                transform.blend,
                transform.playfield_center_x,
                transform.target_x,
                transform.target_y,
                transform.rotation_x,
                transform.rotation_z,
                transform.rotation_y,
                transform.skew_x,
                transform.skew_y,
                transform.zoom_x,
                transform.zoom_y,
                transform.zoom_z,
            );
        })
        .map(|source| [source])?;
    Some(PreparedProxySource {
        segments,
        offset: [-transform.target_x, -transform.target_y],
        root_camera: true,
    })
}

fn prepare_field_proxy_source(
    actors: &[Actor],
    draws: &[FlatDraw],
    camera: Option<Matrix4>,
    transform: SongLuaCaptureTransform,
    scratch: &mut SharedActorFrameScratch,
) -> Option<PreparedProxySource> {
    if actors.is_empty() && draws.is_empty() {
        return None;
    }
    let field_len = actors.len() + draws.len() + usize::from(camera.is_some()) * 2;
    let field_actors = || {
        camera
            .map(|view_proj| Actor::CameraPush { view_proj })
            .into_iter()
            .chain(actors.iter().cloned())
            .chain(draws.iter().cloned().map(actor_from_flat_draw))
            .chain(camera.map(|_| Actor::CameraPop))
    };
    if song_lua_player_transform_is_direct_identity(transform) {
        let segments = scratch
            .refill([0.0, 0.0], |out| out.extend(field_actors()))
            .map(|source| [source])?;
        return Some(PreparedProxySource {
            segments,
            offset: [-transform.target_x, -transform.target_y],
            root_camera: false,
        });
    }
    let segments = scratch
        .refill([0.0, 0.0], |out| {
            append_song_lua_player_transform(
                field_actors(),
                std::iter::empty(),
                field_len,
                0,
                camera.is_some(),
                out,
                transform.z_shift,
                transform.tint,
                transform.blend,
                transform.playfield_center_x,
                transform.target_x,
                transform.target_y,
                transform.rotation_x,
                transform.rotation_z,
                transform.rotation_y,
                transform.skew_x,
                transform.skew_y,
                transform.zoom_x,
                transform.zoom_y,
                transform.zoom_z,
            );
        })
        .map(|source| [source])?;
    Some(PreparedProxySource {
        segments,
        offset: [-transform.target_x, -transform.target_y],
        root_camera: true,
    })
}

fn capture_player_source(
    field_actors: &mut Vec<Actor>,
    hud_actors: &mut Vec<Actor>,
    transform: SongLuaCaptureTransform,
    scratch: &mut SharedActorFrameScratch,
) -> Option<SongLuaSingleSource> {
    if song_lua_player_transform_is_direct_identity(transform) {
        return scratch
            .refill([0.0, 0.0], |out| {
                out.reserve(field_actors.len().saturating_add(hud_actors.len()));
                out.append(hud_actors);
                out.append(field_actors);
            })
            .map(|source| [source]);
    }

    let field_len = field_actors.len();
    let hud_len = hud_actors.len();
    let field_has_camera = field_actors.iter().any(|actor| {
        matches!(
            actor,
            Actor::Camera { .. } | Actor::CameraPush { .. } | Actor::CameraPop
        )
    });
    scratch
        .refill([0.0, 0.0], |out| {
            append_song_lua_player_transform(
                field_actors.drain(..),
                hud_actors.drain(..),
                field_len,
                hud_len,
                field_has_camera,
                out,
                transform.z_shift,
                transform.tint,
                transform.blend,
                transform.playfield_center_x,
                transform.target_x,
                transform.target_y,
                transform.rotation_x,
                transform.rotation_z,
                transform.rotation_y,
                transform.skew_x,
                transform.skew_y,
                transform.zoom_x,
                transform.zoom_y,
                transform.zoom_z,
            );
        })
        .map(|source| [source])
}

#[allow(clippy::too_many_arguments)]
fn capture_flat_player_source(
    field_actors: &mut Vec<Actor>,
    field_draws: &[FlatDraw],
    field_camera: Option<Matrix4>,
    hud_actors: &mut Vec<Actor>,
    hud_draws: &[FlatDraw],
    transform: SongLuaCaptureTransform,
    scratch: &mut SharedActorFrameScratch,
) -> Option<SongLuaSingleSource> {
    hud_actors.extend(hud_draws.iter().cloned().map(actor_from_flat_draw));
    if let Some(view_proj) = field_camera {
        field_actors.insert(0, Actor::CameraPush { view_proj });
    }
    field_actors.extend(field_draws.iter().cloned().map(actor_from_flat_draw));
    if field_camera.is_some() {
        field_actors.push(Actor::CameraPop);
    }
    capture_player_source(field_actors, hud_actors, transform, scratch)
}

fn song_lua_captured_segment_actors(segment: &[Actor]) -> &[Actor] {
    match segment {
        [
            Actor::Frame {
                align: [0.0, 0.0],
                offset: [0.0, 0.0],
                size: [SizeSpec::Fill, SizeSpec::Fill],
                children,
                background: None,
                z: 0,
            },
        ] => children,
        _ => segment,
    }
}

fn song_lua_note_field_proxy_actors(segment: &[Actor]) -> &[Actor] {
    let actors = song_lua_captured_segment_actors(segment);
    match actors {
        [Actor::CameraPush { .. }, contents @ .., Actor::CameraPop] => contents,
        _ => actors,
    }
}

#[inline(always)]
fn song_lua_proxy_source<'a>(
    target: &SongLuaProxyTarget,
    proxy_sources: &SongLuaScreenProxySources<'a>,
) -> Option<SongLuaProxySource<'a>> {
    let source = match target {
        SongLuaProxyTarget::Player { player_index } => proxy_sources
            .players
            .get(*player_index)
            .and_then(|sources| sources.player.filter(|source| !source.is_empty())),
        SongLuaProxyTarget::NoteField { player_index } => proxy_sources
            .players
            .get(*player_index)
            .and_then(|sources| sources.note_field.filter(|source| !source.is_empty())),
        SongLuaProxyTarget::Judgment { player_index } => proxy_sources
            .players
            .get(*player_index)
            .and_then(|sources| sources.judgment.filter(|source| !source.is_empty())),
        SongLuaProxyTarget::Combo { player_index } => proxy_sources
            .players
            .get(*player_index)
            .and_then(|sources| sources.combo.filter(|source| !source.is_empty())),
        SongLuaProxyTarget::Underlay { .. } => proxy_sources
            .underlay
            .filter(|segments| !segments.is_empty())
            .map(SongLuaProxySource::new),
        SongLuaProxyTarget::Overlay { .. } => proxy_sources
            .overlay
            .filter(|segments| !segments.is_empty())
            .map(SongLuaProxySource::new),
        SongLuaProxyTarget::Actor { .. } => None,
    };
    source.map(|source| source.with_pool_class(song_lua_proxy_pool_class(target)))
}

fn song_lua_mark_proxy_target(
    requests: &mut SongLuaScreenProxyRequests,
    target: &SongLuaProxyTarget,
) {
    match target {
        SongLuaProxyTarget::Player { player_index } => {
            if let Some(player) = requests.players.get_mut(*player_index) {
                player.player = true;
            }
        }
        SongLuaProxyTarget::NoteField { player_index } => {
            if let Some(player) = requests.players.get_mut(*player_index) {
                player.note_field = true;
            }
        }
        SongLuaProxyTarget::Judgment { player_index } => {
            if let Some(player) = requests.players.get_mut(*player_index) {
                player.judgment = true;
            }
        }
        SongLuaProxyTarget::Combo { player_index } => {
            if let Some(player) = requests.players.get_mut(*player_index) {
                player.combo = true;
            }
        }
        SongLuaProxyTarget::Underlay { hidden } => {
            requests.underlay = true;
            requests.hide_underlay |= *hidden;
        }
        SongLuaProxyTarget::Overlay { hidden } => {
            requests.overlay = true;
            requests.hide_overlay |= *hidden;
        }
        SongLuaProxyTarget::Actor { .. } => {}
    }
}
fn song_lua_collect_capture_requests_indexed<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    overlay_states: &[SongLuaOverlayState],
    capture_index: usize,
    index: &SongLuaProxyRequestIndex,
    requests: &mut SongLuaScreenProxyRequests,
    visit_scratch: &mut SongLuaCaptureVisitScratch,
) {
    if !visit_scratch.visit(capture_index) {
        return;
    }
    let Some(children) = index.capture_children.get(capture_index) else {
        return;
    };
    for &overlay_index in children {
        let Some(overlay_state) = overlay_states.get(overlay_index).copied() else {
            continue;
        };
        if !song_lua_overlay_is_visible(overlay_state) {
            continue;
        }
        match &overlays[overlay_index].kind {
            SongLuaOverlayKind::ActorProxy { target } => {
                song_lua_mark_proxy_target(requests, target);
            }
            SongLuaOverlayKind::AftSprite { .. } => {
                if let Some(nested_capture) = index
                    .topology
                    .aft_sprite_targets
                    .get(overlay_index)
                    .copied()
                    .and_then(SongLuaOverlayIndex::get)
                {
                    song_lua_collect_capture_requests_indexed(
                        overlays,
                        overlay_states,
                        nested_capture,
                        index,
                        requests,
                        visit_scratch,
                    );
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
fn song_lua_proxy_requests_indexed<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    overlay_states: &[SongLuaOverlayState],
    index: &SongLuaProxyRequestIndex,
    visit_scratch: &mut SongLuaCaptureVisitScratch,
) -> SongLuaScreenProxyRequests {
    if index.proxy_indices.is_empty() {
        return SongLuaScreenProxyRequests::default();
    }
    song_lua_proxy_request_analysis_indexed_active(overlays, overlay_states, index, visit_scratch)
        .all
}

fn song_lua_proxy_request_analysis_indexed<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    overlay_states: &[SongLuaOverlayState],
    index: &SongLuaProxyRequestIndex,
    visit_scratch: &mut SongLuaCaptureVisitScratch,
) -> SongLuaProxyRequestAnalysis {
    if index.proxy_indices.is_empty() {
        return SongLuaProxyRequestAnalysis::default();
    }
    song_lua_proxy_request_analysis_indexed_active(overlays, overlay_states, index, visit_scratch)
}

fn song_lua_proxy_request_analysis_indexed_active<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    overlay_states: &[SongLuaOverlayState],
    index: &SongLuaProxyRequestIndex,
    visit_scratch: &mut SongLuaCaptureVisitScratch,
) -> SongLuaProxyRequestAnalysis {
    let mut analysis = SongLuaProxyRequestAnalysis::default();
    visit_scratch.begin(overlays.len());
    for &overlay_index in &index.root_indices {
        let Some(overlay_state) = overlay_states.get(overlay_index).copied() else {
            continue;
        };
        if !song_lua_overlay_is_visible(overlay_state) {
            continue;
        }
        match &overlays[overlay_index].kind {
            SongLuaOverlayKind::ActorProxy { target } => {
                song_lua_mark_proxy_target(&mut analysis.all, target);
                match target {
                    SongLuaProxyTarget::Player { player_index } => {
                        if let Some(count) = analysis.root_players.get_mut(*player_index) {
                            *count = count.saturating_add(1);
                        }
                    }
                    SongLuaProxyTarget::NoteField { player_index } => {
                        if let Some(count) = analysis.root_note_fields.get_mut(*player_index) {
                            *count = count.saturating_add(1);
                        }
                    }
                    SongLuaProxyTarget::Judgment { player_index } => {
                        if let Some(count) = analysis.root_judgments.get_mut(*player_index) {
                            *count = count.saturating_add(1);
                        }
                    }
                    SongLuaProxyTarget::Combo { player_index } => {
                        if let Some(count) = analysis.root_combos.get_mut(*player_index) {
                            *count = count.saturating_add(1);
                        }
                    }
                    _ => {}
                }
            }
            SongLuaOverlayKind::AftSprite { .. } => {
                if let Some(capture_index) = index
                    .topology
                    .aft_sprite_targets
                    .get(overlay_index)
                    .copied()
                    .and_then(SongLuaOverlayIndex::get)
                {
                    song_lua_collect_capture_requests_indexed(
                        overlays,
                        overlay_states,
                        capture_index,
                        index,
                        &mut analysis.captured,
                        visit_scratch,
                    );
                }
            }
            _ => {}
        }
    }
    song_lua_merge_proxy_requests(&mut analysis.all, analysis.captured);
    analysis
}

fn song_lua_covering_capture_requests<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    local_overlay_states: &[SongLuaOverlayState],
    overlay_states: &[SongLuaOverlayState],
    index: &SongLuaProxyRequestIndex,
    overlay_space_width: f32,
    overlay_space_height: f32,
    visit_scratch: &mut SongLuaCaptureVisitScratch,
) -> SongLuaScreenProxyRequests {
    if index.proxy_indices.is_empty() {
        return SongLuaScreenProxyRequests::default();
    }
    let mut requests = SongLuaScreenProxyRequests::default();
    for &overlay_index in &index.root_indices {
        let Some(overlay_state) = overlay_states.get(overlay_index).copied() else {
            continue;
        };
        let SongLuaOverlayKind::AftSprite { .. } = &overlays[overlay_index].kind else {
            continue;
        };
        let Some(capture_index) = index
            .topology
            .aft_sprite_targets
            .get(overlay_index)
            .copied()
            .and_then(SongLuaOverlayIndex::get)
        else {
            continue;
        };
        visit_scratch.begin(overlays.len());
        let Some(opaque_rect) = song_lua_capture_opaque_rect(
            overlays,
            local_overlay_states,
            capture_index,
            index,
            overlay_space_width,
            overlay_space_height,
            visit_scratch,
        ) else {
            continue;
        };
        let Some(capture_state) = local_overlay_states.get(capture_index).copied() else {
            continue;
        };
        let texture_size = capture_state
            .size
            .unwrap_or([overlay_space_width, overlay_space_height]);
        let Some(screen_rect) = song_lua_map_opaque_rect(overlay_state, texture_size, opaque_rect)
        else {
            continue;
        };
        if !song_lua_rect_covers(
            screen_rect,
            SongLuaOpaqueRect::new(0.0, 0.0, overlay_space_width, overlay_space_height),
        ) {
            continue;
        }
        visit_scratch.begin(overlays.len());
        song_lua_collect_capture_requests_indexed(
            overlays,
            overlay_states,
            capture_index,
            index,
            &mut requests,
            visit_scratch,
        );
    }
    requests
}

#[derive(Clone, Copy, Debug)]
struct SongLuaOpaqueRect {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl SongLuaOpaqueRect {
    const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left: left.min(right),
            top: top.min(bottom),
            right: left.max(right),
            bottom: top.max(bottom),
        }
    }

    fn area(self) -> f32 {
        (self.right - self.left).max(0.0) * (self.bottom - self.top).max(0.0)
    }
}

fn song_lua_capture_opaque_rect<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    local_overlay_states: &[SongLuaOverlayState],
    capture_index: usize,
    index: &SongLuaProxyRequestIndex,
    overlay_space_width: f32,
    overlay_space_height: f32,
    visit_scratch: &mut SongLuaCaptureVisitScratch,
) -> Option<SongLuaOpaqueRect> {
    if !visit_scratch.visit(capture_index) {
        return None;
    }
    let state = local_overlay_states.get(capture_index).copied()?;
    if !state.visible {
        return None;
    }
    let SongLuaOverlayKind::ActorFrameTexture {
        alpha_buffer,
        preserve_texture,
        ..
    } = overlays.get(capture_index).map(|overlay| &overlay.kind)?
    else {
        return None;
    };
    let size = state
        .size
        .unwrap_or([overlay_space_width, overlay_space_height]);
    if !*alpha_buffer {
        return Some(SongLuaOpaqueRect::new(0.0, 0.0, size[0], size[1]));
    }
    if *preserve_texture {
        return None;
    }

    let direct_children = index
        .capture_children
        .get(capture_index)?
        .iter()
        .copied()
        .filter(|&child_index| overlays[child_index].parent_index == Some(capture_index));
    // A strip compositor can cover the target only as the union of many
    // sprites. Treating its largest strip as opaque hides the live screen
    // behind effects such as 7th Gear, so only recurse through one AFT image.
    if direct_children
        .clone()
        .filter(|&child_index| {
            matches!(
                overlays[child_index].kind,
                SongLuaOverlayKind::AftSprite { .. }
            )
        })
        .count()
        > 1
    {
        return None;
    }

    direct_children.fold(None, |largest, child_index| {
        let Some(child_state) = local_overlay_states.get(child_index).copied() else {
            return largest;
        };
        let rect = match overlays.get(child_index).map(|overlay| &overlay.kind) {
            Some(SongLuaOverlayKind::Quad) => {
                let Some(child_size) = child_state.size.or_else(|| {
                    child_state.stretch_rect.map(|[left, top, right, bottom]| {
                        [(right - left).abs(), (bottom - top).abs()]
                    })
                }) else {
                    return largest;
                };
                song_lua_map_opaque_rect(
                    child_state,
                    child_size,
                    SongLuaOpaqueRect::new(0.0, 0.0, child_size[0], child_size[1]),
                )
            }
            Some(SongLuaOverlayKind::AftSprite { .. }) => {
                let Some(target_index) = index
                    .topology
                    .aft_sprite_targets
                    .get(child_index)
                    .copied()
                    .and_then(SongLuaOverlayIndex::get)
                else {
                    return largest;
                };
                let Some(target_state) = local_overlay_states.get(target_index).copied() else {
                    return largest;
                };
                let target_size = target_state
                    .size
                    .unwrap_or([overlay_space_width, overlay_space_height]);
                let Some(source_rect) = song_lua_capture_opaque_rect(
                    overlays,
                    local_overlay_states,
                    target_index,
                    index,
                    overlay_space_width,
                    overlay_space_height,
                    visit_scratch,
                ) else {
                    return largest;
                };
                song_lua_map_opaque_rect(child_state, target_size, source_rect)
            }
            _ => None,
        };
        match (largest, rect) {
            (Some(current), Some(candidate)) if candidate.area() > current.area() => {
                Some(candidate)
            }
            (None, Some(candidate)) => Some(candidate),
            (current, _) => current,
        }
    })
}

fn song_lua_map_opaque_rect(
    state: SongLuaOverlayState,
    texture_size: [f32; 2],
    source: SongLuaOpaqueRect,
) -> Option<SongLuaOpaqueRect> {
    let opaque = song_lua_overlay_is_visible(state)
        && state.diffuse[3] >= 1.0 - f32::EPSILON
        && state
            .vertex_colors
            .is_none_or(|colors| colors.iter().all(|color| color[3] >= 1.0 - f32::EPSILON))
        && state.blend == SongLuaOverlayBlendMode::Alpha
        && !state.mask_source
        && !state.mask_dest
        && !state.depth_test
        && state.cropleft <= f32::EPSILON
        && state.cropright <= f32::EPSILON
        && state.croptop <= f32::EPSILON
        && state.cropbottom <= f32::EPSILON
        && state.fadeleft <= f32::EPSILON
        && state.faderight <= f32::EPSILON
        && state.fadetop <= f32::EPSILON
        && state.fadebottom <= f32::EPSILON
        && state.rot_x_deg.abs() <= f32::EPSILON
        && state.rot_y_deg.abs() <= f32::EPSILON
        && state.rot_z_deg.abs() <= f32::EPSILON
        && state.skew_x.abs() <= f32::EPSILON
        && state.skew_y.abs() <= f32::EPSILON
        && !state.vibrate
        && state.effect_mode == deadlib_present::anim::EffectMode::None
        && !state.texture_wrapping
        && state.texcoord_offset.is_none()
        && state.custom_texture_rect.is_none()
        && state.texcoord_velocity.is_none();
    if !opaque {
        return None;
    }

    let [axis_x, axis_y] = song_lua_overlay_axis_scale(state);
    let [texture_width, texture_height] = texture_size;
    if texture_width <= f32::EPSILON || texture_height <= f32::EPSILON {
        return None;
    }
    let (origin_x, origin_y, scale_x, scale_y) =
        if let Some([left, top, right, bottom]) = state.stretch_rect {
            (
                left,
                top,
                (right - left) * axis_x / texture_width,
                (bottom - top) * axis_y / texture_height,
            )
        } else {
            (
                (state.halign * texture_width).mul_add(-axis_x, state.x),
                (state.valign * texture_height).mul_add(-axis_y, state.y),
                axis_x,
                axis_y,
            )
        };
    Some(SongLuaOpaqueRect::new(
        source.left.mul_add(scale_x, origin_x),
        source.top.mul_add(scale_y, origin_y),
        source.right.mul_add(scale_x, origin_x),
        source.bottom.mul_add(scale_y, origin_y),
    ))
}

fn song_lua_rect_covers(outer: SongLuaOpaqueRect, inner: SongLuaOpaqueRect) -> bool {
    const EDGE_EPSILON: f32 = 0.5;
    outer.left <= inner.left + EDGE_EPSILON
        && outer.top <= inner.top + EDGE_EPSILON
        && outer.right >= inner.right - EDGE_EPSILON
        && outer.bottom >= inner.bottom - EDGE_EPSILON
}

fn song_lua_merge_proxy_requests(
    into: &mut SongLuaScreenProxyRequests,
    from: SongLuaScreenProxyRequests,
) {
    for player_index in 0..into.players.len() {
        into.players[player_index].player |= from.players[player_index].player;
        into.players[player_index].note_field |= from.players[player_index].note_field;
        into.players[player_index].judgment |= from.players[player_index].judgment;
        into.players[player_index].combo |= from.players[player_index].combo;
    }
    into.underlay |= from.underlay;
    into.overlay |= from.overlay;
    into.hide_underlay |= from.hide_underlay;
    into.hide_overlay |= from.hide_overlay;
}

fn song_lua_merge_proxy_analysis(
    into: &mut SongLuaProxyRequestAnalysis,
    from: SongLuaProxyRequestAnalysis,
) {
    song_lua_merge_proxy_requests(&mut into.all, from.all);
    song_lua_merge_proxy_requests(&mut into.captured, from.captured);
    for (count, add) in into.root_players.iter_mut().zip(from.root_players) {
        *count = count.saturating_add(add);
    }
    for (count, add) in into.root_note_fields.iter_mut().zip(from.root_note_fields) {
        *count = count.saturating_add(add);
    }
    for (count, add) in into.root_judgments.iter_mut().zip(from.root_judgments) {
        *count = count.saturating_add(add);
    }
    for (count, add) in into.root_combos.iter_mut().zip(from.root_combos) {
        *count = count.saturating_add(add);
    }
}

#[cfg(test)]
fn song_lua_build_proxy_actor(
    state: SongLuaOverlayState,
    z: i16,
    source: &[Arc<[Actor]>],
    overlay_space_width: f32,
    overlay_space_height: f32,
) -> Option<Actor> {
    song_lua_build_proxy_actor_with_scratch(
        state,
        z,
        SongLuaProxySource::new(source),
        overlay_space_width,
        overlay_space_height,
        None,
    )
}

fn song_lua_build_proxy_actor_with_scratch(
    state: SongLuaOverlayState,
    z: i16,
    source: SongLuaProxySource<'_>,
    overlay_space_width: f32,
    overlay_space_height: f32,
    scratch: Option<&mut SongLuaProxyActorScratch>,
) -> Option<Actor> {
    song_lua_build_proxy_actor_in_space_with_scratch(
        state,
        z,
        source,
        overlay_space_width,
        overlay_space_height,
        screen_width(),
        screen_height(),
        scratch,
    )
}

#[allow(clippy::too_many_arguments)]
fn song_lua_build_proxy_actor_in_space_with_scratch(
    state: SongLuaOverlayState,
    z: i16,
    source: SongLuaProxySource<'_>,
    overlay_space_width: f32,
    overlay_space_height: f32,
    render_space_width: f32,
    render_space_height: f32,
    mut scratch: Option<&mut SongLuaProxyActorScratch>,
) -> Option<Actor> {
    if !state.visible || state.diffuse[3] <= f32::EPSILON || source.is_empty() {
        return None;
    }
    let blend = Some(song_lua_overlay_blend(state.blend));
    let offset = song_lua_proxy_offset(
        state,
        source.offset,
        overlay_space_width,
        overlay_space_height,
    );
    let transform = song_lua_proxy_needs_transform(state).then(|| {
        song_lua_proxy_transform(
            state,
            source.offset,
            overlay_space_width,
            overlay_space_height,
            render_space_width,
            render_space_height,
        )
    });
    if let [segment] = source.segments {
        let slot_index = scratch
            .as_deref_mut()
            .and_then(|scratch| scratch.reserve_proxy_group(source.pool_class))
            .map(|group| group.segment_start);
        let children = match (scratch.as_deref_mut(), slot_index) {
            (Some(scratch), Some(slot_index)) => scratch.normalize_segment(segment, slot_index),
            _ => song_lua_proxy_source_segment_owned(segment),
        };
        return Some(song_lua_proxy_wrapper(
            children,
            offset,
            transform,
            source.source_view_proj,
            z,
            state.diffuse,
            blend,
        ));
    }
    song_lua_build_proxy_frame_actor_in_space_with_scratch(
        state, z, source, offset, transform, scratch,
    )
}

fn song_lua_proxy_offset(
    state: SongLuaOverlayState,
    source_offset: [f32; 2],
    overlay_space_width: f32,
    overlay_space_height: f32,
) -> [f32; 2] {
    [
        state.x * screen_width() / overlay_space_width.max(1.0) + source_offset[0],
        state.y * screen_height() / overlay_space_height.max(1.0) + source_offset[1],
    ]
}

fn song_lua_proxy_needs_transform(state: SongLuaOverlayState) -> bool {
    let [scale_x, scale_y] = song_lua_overlay_axis_scale(state);
    (scale_x - 1.0).abs() > f32::EPSILON
        || (scale_y - 1.0).abs() > f32::EPSILON
        || (song_lua_overlay_z_scale(state) - 1.0).abs() > f32::EPSILON
        || state.z.abs() > f32::EPSILON
        || state.rot_x_deg.abs() > f32::EPSILON
        || state.rot_y_deg.abs() > f32::EPSILON
        || state.rot_z_deg.abs() > f32::EPSILON
        || state.skew_x.abs() > f32::EPSILON
        || state.skew_y.abs() > f32::EPSILON
}

fn song_lua_proxy_wrapper(
    children: Arc<[Actor]>,
    offset: [f32; 2],
    transform: Option<Matrix4>,
    source_view_proj: Option<Matrix4>,
    z: i16,
    tint: [f32; 4],
    blend: Option<BlendMode>,
) -> Actor {
    if let Some(transform) = transform {
        return Actor::SharedTransform {
            transform,
            source_view_proj: source_view_proj.unwrap_or_else(song_lua_proxy_source_view_proj),
            children,
            z,
            tint,
            blend,
        };
    }
    Actor::SharedFrame {
        align: [0.0, 0.0],
        offset,
        size: [SizeSpec::Fill, SizeSpec::Fill],
        children,
        background: None,
        z,
        tint,
        blend,
    }
}

fn song_lua_proxy_source_view_proj() -> Matrix4 {
    glam::camera::rh::proj::opengl::orthographic(
        -0.5 * screen_width(),
        0.5 * screen_width(),
        -0.5 * screen_height(),
        0.5 * screen_height(),
        -1.0,
        1.0,
    )
}

fn song_lua_proxy_transform(
    state: SongLuaOverlayState,
    source_offset: [f32; 2],
    overlay_space_width: f32,
    overlay_space_height: f32,
    render_space_width: f32,
    render_space_height: f32,
) -> Matrix4 {
    let render_width = render_space_width.max(1.0);
    let render_height = render_space_height.max(1.0);
    let x = state.x * screen_width() / overlay_space_width.max(1.0);
    let y = state.y * screen_height() / overlay_space_height.max(1.0);
    let [scale_x, scale_y] = song_lua_overlay_axis_scale(state);
    // Proxy sources are already in Y-up world coordinates. Conjugate the
    // screen-space actor rotation/skew across Y before applying it to them.
    Matrix4::from_translation(Vector3::new(
        (-0.5f32).mul_add(render_width, x),
        0.5f32.mul_add(render_height, -y),
        state.z,
    )) * song_lua_overlay_local_transform(
        [-state.rot_x_deg, state.rot_y_deg, -state.rot_z_deg],
        -state.skew_x,
        -state.skew_y,
    ) * Matrix4::from_scale(Vector3::new(
        scale_x,
        scale_y,
        song_lua_overlay_z_scale(state),
    )) * Matrix4::from_translation(Vector3::new(source_offset[0], -source_offset[1], 0.0))
        * Matrix4::from_translation(Vector3::new(0.5 * render_width, -0.5 * render_height, 0.0))
}

fn song_lua_direct_proxy_source(
    target: &SongLuaProxyTarget,
    proxy_sources: &SongLuaScreenProxySources<'_>,
) -> Option<(usize, SongLuaDirectProxySource)> {
    match target {
        SongLuaProxyTarget::NoteField { player_index } => proxy_sources
            .direct_note_fields
            .get(*player_index)
            .copied()
            .flatten()
            .map(|source| (*player_index, source)),
        SongLuaProxyTarget::Judgment { player_index } => proxy_sources
            .direct_judgments
            .get(*player_index)
            .copied()
            .flatten()
            .map(|source| (*player_index, source)),
        SongLuaProxyTarget::Combo { player_index } => proxy_sources
            .direct_combos
            .get(*player_index)
            .copied()
            .flatten()
            .map(|source| (*player_index, source)),
        SongLuaProxyTarget::Player { .. }
        | SongLuaProxyTarget::Underlay { .. }
        | SongLuaProxyTarget::Overlay { .. }
        | SongLuaProxyTarget::Actor { .. } => None,
    }
}

fn song_lua_direct_player_proxy_source(
    target: &SongLuaProxyTarget,
    proxy_sources: &SongLuaScreenProxySources<'_>,
) -> Option<(usize, SongLuaDirectPlayerSource)> {
    let SongLuaProxyTarget::Player { player_index } = target else {
        return None;
    };
    proxy_sources
        .direct_players
        .get(*player_index)
        .copied()
        .flatten()
        .map(|source| (*player_index, source))
}

fn song_lua_direct_proxy(
    state: SongLuaOverlayState,
    z: i16,
    source: SongLuaDirectProxySource,
    player: usize,
    actor_insert: usize,
    overlay_space_width: f32,
    overlay_space_height: f32,
) -> Option<SongLuaDirectProxy> {
    if !song_lua_overlay_is_visible(state) || source.draw_start >= source.draw_end {
        return None;
    }
    let transformed = song_lua_proxy_needs_transform(state);
    let (offset, enclosing_camera, camera) = if transformed {
        let transform = song_lua_proxy_transform(
            state,
            [-source.target[0], -source.target[1]],
            overlay_space_width,
            overlay_space_height,
            screen_width(),
            screen_height(),
        );
        if let Some(player_camera) = source.player_camera {
            // ITGmania's ActorProxy draws its target with the proxy transform
            // already on the world stack. Player then installs the notefield
            // projection and applies its own transform, so the order is
            // projection * proxy * Player. In particular, an enclosing
            // zoomz(0) must flatten Bumpy before perspective projection.
            (
                [0.0, 0.0],
                None,
                Some(player_camera.base * transform * player_camera.suffix),
            )
        } else {
            let source_view_proj = song_lua_proxy_source_view_proj();
            let prefix = source_view_proj * transform * source_view_proj.inverse();
            (
                [0.0, 0.0],
                Some(source_view_proj * transform),
                source.camera.map(|camera| prefix * camera),
            )
        }
    } else {
        (
            [
                state.x * screen_width() / overlay_space_width.max(1.0) - source.target[0],
                state.y * screen_height() / overlay_space_height.max(1.0) - source.target[1],
            ],
            None,
            source.camera,
        )
    };
    Some(SongLuaDirectProxy {
        actor_insert,
        player,
        draws: source.draws,
        draw_start: source.draw_start,
        draw_end: source.draw_end,
        offset,
        z,
        style: FlatProxyStyle::new(source.tint, state.diffuse, source.x_fold),
        blend: song_lua_overlay_blend(state.blend),
        enclosing_camera,
        camera,
        tail: None,
    })
}

#[cfg(test)]
fn song_lua_build_proxy_frame_actor_with_scratch(
    state: SongLuaOverlayState,
    z: i16,
    source: SongLuaProxySource<'_>,
    overlay_space_width: f32,
    overlay_space_height: f32,
    scratch: Option<&mut SongLuaProxyActorScratch>,
) -> Option<Actor> {
    let offset = song_lua_proxy_offset(
        state,
        source.offset,
        overlay_space_width,
        overlay_space_height,
    );
    let transform = song_lua_proxy_needs_transform(state).then(|| {
        song_lua_proxy_transform(
            state,
            source.offset,
            overlay_space_width,
            overlay_space_height,
            screen_width(),
            screen_height(),
        )
    });
    song_lua_build_proxy_frame_actor_in_space_with_scratch(
        state, z, source, offset, transform, scratch,
    )
}

fn song_lua_build_proxy_frame_actor_in_space_with_scratch(
    state: SongLuaOverlayState,
    z: i16,
    source: SongLuaProxySource<'_>,
    offset: [f32; 2],
    transform: Option<Matrix4>,
    mut scratch: Option<&mut SongLuaProxyActorScratch>,
) -> Option<Actor> {
    if !state.visible || state.diffuse[3] <= f32::EPSILON || source.is_empty() {
        return None;
    }
    let blend = Some(song_lua_overlay_blend(state.blend));
    if source.segments.len() <= SONG_LUA_PROXY_SEGMENTS_PER_ACTOR
        && let Some(group) = scratch
            .as_deref_mut()
            .and_then(|scratch| scratch.reserve_proxy_group(source.pool_class))
    {
        let children = scratch
            .as_deref_mut()
            .expect("reserved proxy group requires scratch")
            .join_proxy_segments(group, source.segments, state.diffuse, blend);
        return Some(song_lua_proxy_wrapper(
            children,
            offset,
            transform,
            source.source_view_proj,
            z,
            [1.0; 4],
            None,
        ));
    }

    let mut children = Vec::with_capacity(source.segments.len());
    for segment in source.segments {
        children.push(Actor::SharedFrame {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Fill, SizeSpec::Fill],
            children: song_lua_proxy_source_segment_owned(segment),
            background: None,
            z: 0,
            tint: state.diffuse,
            blend,
        });
    }
    Some(song_lua_proxy_wrapper(
        Arc::from(children),
        offset,
        transform,
        source.source_view_proj,
        z,
        [1.0; 4],
        None,
    ))
}

fn song_lua_proxy_source_segment_owned(segment: &Arc<[Actor]>) -> Arc<[Actor]> {
    if !segment.iter().any(song_lua_proxy_actor_has_z) {
        return Arc::clone(segment);
    }
    let (offset, actors) = song_lua_proxy_segment_actors(segment);
    let mut children = Vec::with_capacity(actors.len());
    song_lua_proxy_local_children_into(actors.iter().cloned(), &mut children);
    if offset == [0.0, 0.0] {
        Arc::from(children)
    } else {
        Arc::from([Actor::Frame {
            align: [0.0, 0.0],
            offset,
            size: [SizeSpec::Fill, SizeSpec::Fill],
            children,
            background: None,
            z: 0,
        }])
    }
}

fn song_lua_proxy_segment_actors(segment: &[Actor]) -> ([f32; 2], &[Actor]) {
    let [
        Actor::Frame {
            align,
            offset,
            size,
            children,
            background,
            z,
        },
    ] = segment
    else {
        return ([0.0, 0.0], segment);
    };
    if *align == [0.0, 0.0]
        && matches!(*size, [SizeSpec::Fill, SizeSpec::Fill])
        && background.is_none()
        && *z == 0
    {
        (*offset, children)
    } else {
        ([0.0, 0.0], segment)
    }
}

fn song_lua_proxy_actor_has_z(actor: &Actor) -> bool {
    match actor {
        Actor::Sprite { z, .. }
        | Actor::Text { z, .. }
        | Actor::Mesh { z, .. }
        | Actor::ReusableMesh { z, .. }
        | Actor::TexturedMesh { z, .. }
        | Actor::ReusableTexturedMesh { z, .. } => *z != 0,
        Actor::Frame { z, children, .. } => {
            *z != 0 || children.iter().any(song_lua_proxy_actor_has_z)
        }
        Actor::SharedFrame { z, children, .. } | Actor::SharedTransform { z, children, .. } => {
            *z != 0 || children.iter().any(song_lua_proxy_actor_has_z)
        }
        Actor::RetainedFrame { z, frame, .. } => {
            *z != 0 || frame.children().iter().any(song_lua_proxy_actor_has_z)
        }
        Actor::Camera { children, .. } => children.iter().any(song_lua_proxy_actor_has_z),
        Actor::Shadow { child, .. } => song_lua_proxy_actor_has_z(child),
        Actor::CameraPush { .. } | Actor::CameraPop => false,
    }
}

fn song_lua_proxy_actor_z(actor: &Actor) -> i16 {
    match actor {
        Actor::Sprite { z, .. }
        | Actor::Text { z, .. }
        | Actor::Mesh { z, .. }
        | Actor::ReusableMesh { z, .. }
        | Actor::TexturedMesh { z, .. }
        | Actor::ReusableTexturedMesh { z, .. }
        | Actor::Frame { z, .. }
        | Actor::SharedFrame { z, .. }
        | Actor::SharedTransform { z, .. }
        | Actor::RetainedFrame { z, .. } => *z,
        Actor::Shadow { child, .. } => song_lua_proxy_actor_z(child),
        Actor::Camera { .. } | Actor::CameraPush { .. } | Actor::CameraPop => 0,
    }
}

fn song_lua_proxy_local_children_into(children: impl Iterator<Item = Actor>, out: &mut Vec<Actor>) {
    out.extend(children);
    song_lua_proxy_local_children_in_place(out);
}

fn song_lua_proxy_expand_retained(children: &mut Vec<Actor>) {
    let mut index = 0;
    while index < children.len() {
        let frame = match &children[index] {
            Actor::RetainedFrame {
                align: [0.0, 0.0],
                offset: [0.0, 0.0],
                size: [SizeSpec::Fill, SizeSpec::Fill],
                frame,
                z: 0,
                tint: [1.0, 1.0, 1.0, 1.0],
                blend: None,
                visible: true,
            } => Some(Arc::clone(frame)),
            _ => None,
        };
        let Some(frame) = frame else {
            index += 1;
            continue;
        };
        // The exact-size iterator lets Vec move the tail once and reserve at
        // most once, instead of shifting every large Actor for every child.
        children.splice(index..index + 1, frame.children().iter().cloned());
        // Inspect the inserted actors too in case a retained static fragment
        // contains another identity retained frame.
    }
}

fn song_lua_proxy_local_children_in_place(children: &mut Vec<Actor>) {
    // Static gameplay fragments retain their authored absolute z values behind
    // an identity wrapper. ActorProxy establishes a new draw plane, so expose
    // those children before sorting and zeroing their local z values.
    song_lua_proxy_expand_retained(children);
    let mut run_start = 0;
    for index in 0..children.len() {
        if matches!(children[index], Actor::CameraPush { .. } | Actor::CameraPop) {
            song_lua_proxy_local_run(&mut children[run_start..index]);
            song_lua_proxy_zero_local_z(&mut children[index]);
            run_start = index + 1;
        }
    }
    song_lua_proxy_local_run(&mut children[run_start..]);
}

fn song_lua_proxy_local_run(children: &mut [Actor]) {
    if children.len() <= 8 || children.len() > PLAYER_ACTOR_SCRATCH_CAPACITY {
        // Tiny and overflow runs avoid auxiliary setup while retaining stable
        // equal-z ordering and a hard zero-allocation fallback.
        for index in 1..children.len() {
            let z = song_lua_proxy_actor_z(&children[index]);
            let mut insert = index;
            while insert > 0 && song_lua_proxy_actor_z(&children[insert - 1]) > z {
                children.swap(insert - 1, insert);
                insert -= 1;
            }
        }
    } else {
        // Sort compact indices by (z, original position), then apply the
        // permutation to the large Actor values. This is stable and uses no
        // heap-backed merge buffer.
        let mut order = [0u16; PLAYER_ACTOR_SCRATCH_CAPACITY];
        let mut target = [0u16; PLAYER_ACTOR_SCRATCH_CAPACITY];
        for (index, slot) in order[..children.len()].iter_mut().enumerate() {
            *slot = index as u16;
        }
        order[..children.len()].sort_unstable_by_key(|&index| {
            (song_lua_proxy_actor_z(&children[index as usize]), index)
        });
        for (new_index, &old_index) in order[..children.len()].iter().enumerate() {
            target[old_index as usize] = new_index as u16;
        }
        for index in 0..children.len() {
            while target[index] as usize != index {
                let swap_index = target[index] as usize;
                children.swap(index, swap_index);
                target.swap(index, swap_index);
            }
        }
    }
    for child in children {
        song_lua_proxy_zero_local_z(child);
    }
}

fn song_lua_proxy_zero_local_z(actor: &mut Actor) {
    match actor {
        Actor::Sprite { z, .. }
        | Actor::Text { z, .. }
        | Actor::Mesh { z, .. }
        | Actor::ReusableMesh { z, .. }
        | Actor::TexturedMesh { z, .. }
        | Actor::ReusableTexturedMesh { z, .. }
        | Actor::RetainedFrame { z, .. } => *z = 0,
        Actor::Frame { z, children, .. } => {
            *z = 0;
            song_lua_proxy_local_children_in_place(children);
        }
        Actor::SharedFrame { z, children, .. } | Actor::SharedTransform { z, children, .. } => {
            *z = 0;
            *children = song_lua_proxy_source_segment_owned(children);
        }
        Actor::Camera { children, .. } => song_lua_proxy_local_children_in_place(children),
        Actor::Shadow { child, .. } => song_lua_proxy_zero_local_z(child),
        Actor::CameraPush { .. } | Actor::CameraPop => {}
    }
}

#[cfg(test)]
fn song_lua_overlay_order<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    overlay_states: &[SongLuaOverlayState],
    parent_index: Option<usize>,
) -> Vec<usize> {
    let mut cache = song_lua_overlay_order_cache_from(overlays, &[]);
    let mut out = Vec::with_capacity(overlays.len());
    song_lua_overlay_order_into(overlays, overlay_states, &mut cache, parent_index, &mut out);
    out
}

fn song_lua_overlay_order_into<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    overlay_states: &[SongLuaOverlayState],
    order_cache: &mut SongLuaOverlayOrderCache,
    parent_index: Option<usize>,
    out: &mut Vec<usize>,
) {
    out.clear();
    out.reserve(overlays.len());
    if parent_index.is_none()
        && let Some(static_order) = order_cache.static_root_order.as_deref()
    {
        out.extend_from_slice(static_order);
        return;
    }
    song_lua_push_order(overlays, overlay_states, order_cache, parent_index, out);
}

fn song_lua_push_order<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    overlay_states: &[SongLuaOverlayState],
    order_cache: &mut SongLuaOverlayOrderCache,
    parent_index: Option<usize>,
    out: &mut Vec<usize>,
) {
    let list_idx = song_lua_overlay_child_list_index(parent_index);
    if list_idx >= order_cache.child_lists.len() || order_cache.child_lists[list_idx].is_empty() {
        return;
    }
    let draw_by_z_position = parent_index.is_some_and(|idx| {
        overlay_states
            .get(idx)
            .map_or(overlays[idx].initial_state.draw_by_z_position, |state| {
                state.draw_by_z_position
            })
    });
    if draw_by_z_position {
        let mut changed = order_cache.sort_modes[list_idx] != SONG_LUA_CHILD_ORDER_Z;
        for &idx in &order_cache.child_lists[list_idx] {
            let z = overlay_states
                .get(idx)
                .map_or(overlays[idx].initial_state.z, |state| state.z);
            let key = z.to_bits();
            if order_cache.last_z_keys[idx] != key {
                order_cache.last_z_keys[idx] = key;
                changed = true;
            }
        }
        if changed {
            order_cache.child_lists[list_idx].sort_by(|&left, &right| {
                let left_z = f32::from_bits(order_cache.last_z_keys[left]);
                let right_z = f32::from_bits(order_cache.last_z_keys[right]);
                left_z.total_cmp(&right_z).then_with(|| left.cmp(&right))
            });
        }
        order_cache.sort_modes[list_idx] = SONG_LUA_CHILD_ORDER_Z;
    } else if order_cache
        .dynamic_draw_order
        .get(list_idx)
        .copied()
        .unwrap_or(false)
    {
        let mut changed = order_cache.sort_modes[list_idx] != SONG_LUA_CHILD_ORDER_DRAW;
        for &idx in &order_cache.child_lists[list_idx] {
            let draw_order = overlay_states
                .get(idx)
                .map_or(overlays[idx].initial_state.draw_order, |state| {
                    state.draw_order
                });
            if order_cache.last_draw_orders[idx] != draw_order {
                order_cache.last_draw_orders[idx] = draw_order;
                changed = true;
            }
        }
        if changed {
            order_cache.child_lists[list_idx]
                .sort_by_key(|&idx| (order_cache.last_draw_orders[idx], idx));
        }
        order_cache.sort_modes[list_idx] = SONG_LUA_CHILD_ORDER_DRAW;
    } else if order_cache
        .sort_modes
        .get(list_idx)
        .copied()
        .unwrap_or(SONG_LUA_CHILD_ORDER_STATIC)
        != SONG_LUA_CHILD_ORDER_STATIC
    {
        song_lua_sort_static_children(overlays, &mut order_cache.child_lists[list_idx]);
        order_cache.sort_modes[list_idx] = SONG_LUA_CHILD_ORDER_STATIC;
    }
    let child_count = order_cache.child_lists[list_idx].len();
    for child_pos in 0..child_count {
        let idx = order_cache.child_lists[list_idx][child_pos];
        out.push(idx);
        let child_list_idx = song_lua_overlay_child_list_index(Some(idx));
        if order_cache
            .child_lists
            .get(child_list_idx)
            .is_some_and(|children| !children.is_empty())
        {
            song_lua_push_order(overlays, overlay_states, order_cache, Some(idx), out);
        }
    }
}

fn song_lua_capture_root_state(state: SongLuaOverlayState) -> SongLuaOverlayState {
    SongLuaOverlayState {
        draw_order: state.draw_order,
        draw_by_z_position: state.draw_by_z_position,
        glow: state.glow,
        fov: state.fov,
        vanishpoint: state.vanishpoint,
        diffuse: state.diffuse,
        visible: state.visible,
        mask_source: state.mask_source,
        mask_dest: state.mask_dest,
        depth_test: state.depth_test,
        blend: state.blend,
        ..SongLuaOverlayState::default()
    }
}

fn song_lua_capture_overlay_states_into_scratch<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    overlay_states: &[SongLuaOverlayState],
    local_overlay_states: &[SongLuaOverlayState],
    order_cache: &SongLuaOverlayOrderCache,
    capture_index: usize,
    overlay_space_width: f32,
    overlay_space_height: f32,
    out: &mut Vec<SongLuaOverlayState>,
) {
    if out.len() != overlays.len() {
        out.resize(overlays.len(), SongLuaOverlayState::default());
    }
    song_lua_fill_capture_overlay_states(
        overlays,
        overlay_states,
        local_overlay_states,
        order_cache,
        capture_index,
        overlay_space_width,
        overlay_space_height,
        out,
    );
}

fn song_lua_fill_capture_overlay_states<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    overlay_states: &[SongLuaOverlayState],
    local_overlay_states: &[SongLuaOverlayState],
    order_cache: &SongLuaOverlayOrderCache,
    capture_index: usize,
    overlay_space_width: f32,
    overlay_space_height: f32,
    out: &mut [SongLuaOverlayState],
) {
    let Some(capture_state) = overlay_states.get(capture_index).copied() else {
        return;
    };
    // AFTs capture in texture space; placement transforms apply to the sprite
    // that consumes the texture, not to the captured children.
    out[capture_index] = song_lua_capture_root_state(capture_state);
    song_lua_capture_overlay_child_states(
        overlays,
        local_overlay_states,
        order_cache,
        capture_index,
        overlay_space_width,
        overlay_space_height,
        out,
    );
}

fn song_lua_capture_overlay_child_states<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    local_overlay_states: &[SongLuaOverlayState],
    order_cache: &SongLuaOverlayOrderCache,
    parent_index: usize,
    overlay_space_width: f32,
    overlay_space_height: f32,
    out: &mut [SongLuaOverlayState],
) {
    let list_idx = song_lua_overlay_child_list_index(Some(parent_index));
    let Some(children) = order_cache.child_lists.get(list_idx) else {
        return;
    };
    for &idx in children {
        let Some(overlay) = overlays.get(idx) else {
            continue;
        };
        let local = local_overlay_states.get(idx).copied().unwrap_or_default();
        let parent = out.get(parent_index).copied().unwrap_or_default();
        let parent_overlay = &overlays[parent_index];
        out[idx] = song_lua_overlay_compose_state(
            &parent_overlay.kind,
            parent,
            local,
            overlay_space_width,
            overlay_space_height,
        );
        if !matches!(overlay.kind, SongLuaOverlayKind::ActorFrameTexture { .. }) {
            song_lua_capture_overlay_child_states(
                overlays,
                local_overlay_states,
                order_cache,
                idx,
                overlay_space_width,
                overlay_space_height,
                out,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn song_lua_append_local_proxy_target<S: NoteskinSlot + Clone>(
    out: &mut Vec<Actor>,
    overlays: &[SongLuaOverlayActor<S>],
    overlay_states: &[SongLuaOverlayState],
    local_overlay_states: &[SongLuaOverlayState],
    order_cache: &SongLuaOverlayOrderCache,
    topology_index: &SongLuaOverlayTopologyIndex,
    asset_manager: &AssetManager,
    index: usize,
    state: SongLuaOverlayState,
    overlay_space_width: f32,
    overlay_space_height: f32,
    effect_time: f32,
    effect_beat: f32,
    total_elapsed: f32,
    projected_mesh_scratch: &mut [SongLuaProjectedMeshScratch],
) {
    let Some(overlay) = overlays.get(index) else {
        return;
    };
    match overlay.kind {
        SongLuaOverlayKind::Actor | SongLuaOverlayKind::ActorFrame => {
            let list_idx = song_lua_overlay_child_list_index(Some(index));
            let Some(children) = order_cache.child_lists.get(list_idx) else {
                return;
            };
            for &child_index in children {
                if child_index >= overlays.len() {
                    continue;
                }
                let local = local_overlay_states
                    .get(child_index)
                    .copied()
                    .unwrap_or_default();
                let child_state = song_lua_overlay_compose_state(
                    &overlay.kind,
                    state,
                    local,
                    overlay_space_width,
                    overlay_space_height,
                );
                song_lua_append_local_proxy_target(
                    out,
                    overlays,
                    overlay_states,
                    local_overlay_states,
                    order_cache,
                    topology_index,
                    asset_manager,
                    child_index,
                    child_state,
                    overlay_space_width,
                    overlay_space_height,
                    effect_time,
                    effect_beat,
                    total_elapsed,
                    projected_mesh_scratch,
                );
            }
        }
        SongLuaOverlayKind::ActorFrameTexture { .. }
        | SongLuaOverlayKind::ActorProxy { .. }
        | SongLuaOverlayKind::AftSprite { .. }
        | SongLuaOverlayKind::Sound { .. } => {}
        _ => {
            let z = out.len().min(i16::MAX as usize) as i16;
            if append_song_lua_multi_actor_overlay(
                out,
                overlay,
                state,
                asset_manager,
                z,
                overlay_space_width,
                overlay_space_height,
                effect_time,
                effect_beat,
                total_elapsed,
                projected_mesh_scratch.get_mut(index),
            )
            .is_none()
                && let Some(actors) = build_song_lua_overlay_actor_with_scratch(
                    overlay,
                    state,
                    topology_index.camera_state(overlay_states, index),
                    asset_manager,
                    z,
                    overlay_space_width,
                    overlay_space_height,
                    effect_time,
                    effect_beat,
                    total_elapsed,
                    projected_mesh_scratch.get_mut(index),
                )
            {
                out.extend(actors);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn song_lua_build_local_proxy_actor<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    overlay_states: &[SongLuaOverlayState],
    local_overlay_states: &[SongLuaOverlayState],
    order_cache: &SongLuaOverlayOrderCache,
    topology_index: &SongLuaOverlayTopologyIndex,
    asset_manager: &AssetManager,
    target_index: usize,
    proxy_state: SongLuaOverlayState,
    z: i16,
    overlay_space_width: f32,
    overlay_space_height: f32,
    render_space_width: f32,
    render_space_height: f32,
    effect_time: f32,
    effect_beat: f32,
    total_elapsed: f32,
    projected_mesh_scratch: &mut [SongLuaProjectedMeshScratch],
    mut proxy_actor_scratch: Option<&mut SongLuaProxyActorScratch>,
) -> Option<Actor> {
    let mut target_state = local_overlay_states.get(target_index).copied()?;
    // ITG ActorProxy::DrawPrimitives temporarily unhides its target for the
    // proxied draw, then restores the target's hidden state.
    target_state.visible = true;
    let source = if let Some(slot) = proxy_actor_scratch
        .as_deref_mut()
        .and_then(SongLuaProxyActorScratch::next_screen)
    {
        slot.refill([0.0, 0.0], |out| {
            song_lua_append_local_proxy_target(
                out,
                overlays,
                overlay_states,
                local_overlay_states,
                order_cache,
                topology_index,
                asset_manager,
                target_index,
                target_state,
                overlay_space_width,
                overlay_space_height,
                effect_time,
                effect_beat,
                total_elapsed,
                projected_mesh_scratch,
            );
        })?
    } else {
        let mut out = Vec::new();
        song_lua_append_local_proxy_target(
            &mut out,
            overlays,
            overlay_states,
            local_overlay_states,
            order_cache,
            topology_index,
            asset_manager,
            target_index,
            target_state,
            overlay_space_width,
            overlay_space_height,
            effect_time,
            effect_beat,
            total_elapsed,
            projected_mesh_scratch,
        );
        if out.is_empty() {
            return None;
        }
        Arc::from(out)
    };
    let segments = [source];
    song_lua_build_proxy_actor_in_space_with_scratch(
        proxy_state,
        z,
        SongLuaProxySource::new(&segments),
        overlay_space_width,
        overlay_space_height,
        render_space_width,
        render_space_height,
        proxy_actor_scratch,
    )
}

fn song_lua_capture_children_into<S: NoteskinSlot + Clone>(
    out: &mut Vec<Actor>,
    overlays: &[SongLuaOverlayActor<S>],
    overlay_states: &[SongLuaOverlayState],
    local_overlay_states: &[SongLuaOverlayState],
    order_cache: &mut SongLuaOverlayOrderCache,
    topology_index: &SongLuaOverlayTopologyIndex,
    asset_manager: &AssetManager,
    capture_index: usize,
    proxy_sources: &SongLuaScreenProxySources<'_>,
    mut proxy_actor_scratch: Option<&mut SongLuaProxyActorScratch>,
    overlay_space_width: f32,
    overlay_space_height: f32,
    effect_time: f32,
    effect_beat: f32,
    total_elapsed: f32,
    capture_states: &mut Vec<SongLuaOverlayState>,
    order_scratch: &mut Vec<usize>,
    projected_mesh_scratch: &mut [SongLuaProjectedMeshScratch],
) {
    let render_space = overlay_states
        .get(capture_index)
        .and_then(|state| state.size)
        .unwrap_or([overlay_space_width, overlay_space_height]);
    song_lua_capture_overlay_states_into_scratch(
        overlays,
        overlay_states,
        local_overlay_states,
        order_cache,
        capture_index,
        overlay_space_width,
        overlay_space_height,
        capture_states,
    );
    song_lua_overlay_order_into(
        overlays,
        capture_states,
        order_cache,
        Some(capture_index),
        order_scratch,
    );
    out.reserve(order_scratch.len());
    for (draw_idx, idx) in order_scratch.iter().copied().enumerate() {
        let Some(overlay) = overlays.get(idx) else {
            continue;
        };
        if topology_index
            .aft_ancestors
            .get(idx)
            .copied()
            .and_then(SongLuaOverlayIndex::get)
            != Some(capture_index)
        {
            continue;
        }
        if matches!(
            overlay.kind,
            SongLuaOverlayKind::Actor
                | SongLuaOverlayKind::ActorFrame
                | SongLuaOverlayKind::ActorFrameTexture { .. }
        ) {
            continue;
        }
        let overlay_state = capture_states.get(idx).copied().unwrap_or_default();
        match &overlay.kind {
            SongLuaOverlayKind::ActorProxy { target } => {
                let z = draw_idx.min(i16::MAX as usize) as i16;
                let overlay_state =
                    song_lua_proxy_effect(overlay_state, effect_time, effect_beat, idx as u32);
                let actor = match target {
                    SongLuaProxyTarget::Actor { overlay_index } => {
                        song_lua_build_local_proxy_actor(
                            overlays,
                            overlay_states,
                            local_overlay_states,
                            order_cache,
                            topology_index,
                            asset_manager,
                            *overlay_index,
                            overlay_state,
                            z,
                            overlay_space_width,
                            overlay_space_height,
                            render_space[0],
                            render_space[1],
                            effect_time,
                            effect_beat,
                            total_elapsed,
                            projected_mesh_scratch,
                            proxy_actor_scratch.as_deref_mut(),
                        )
                    }
                    _ => song_lua_proxy_source(target, proxy_sources).and_then(|source| {
                        song_lua_build_proxy_actor_in_space_with_scratch(
                            overlay_state,
                            z,
                            source,
                            overlay_space_width,
                            overlay_space_height,
                            render_space[0],
                            render_space[1],
                            proxy_actor_scratch.as_deref_mut(),
                        )
                    }),
                };
                if let Some(actor) = actor {
                    out.push(actor);
                }
            }
            SongLuaOverlayKind::AftSprite { .. } => {
                let Some(target_index) = topology_index
                    .aft_sprite_targets
                    .get(idx)
                    .copied()
                    .and_then(SongLuaOverlayIndex::get)
                else {
                    continue;
                };
                let Some(&texture_handle) = topology_index.aft_texture_handles.get(target_index)
                else {
                    continue;
                };
                let texture_size = overlay_states
                    .get(target_index)
                    .and_then(|state| state.size)
                    .unwrap_or([overlay_space_width, overlay_space_height]);
                let z = draw_idx.min(i16::MAX as usize) as i16;
                if let Some(actors) = build_song_lua_aft_sprite_actor(
                    overlay_state,
                    texture_handle,
                    texture_size,
                    z,
                    overlay_space_width,
                    overlay_space_height,
                    effect_time,
                    effect_beat,
                    total_elapsed,
                    projected_mesh_scratch.get_mut(idx),
                ) {
                    out.extend(actors);
                }
            }
            _ => {
                let z = draw_idx.min(i16::MAX as usize) as i16;
                if append_song_lua_multi_actor_overlay(
                    out,
                    overlay,
                    overlay_state,
                    asset_manager,
                    z,
                    overlay_space_width,
                    overlay_space_height,
                    effect_time,
                    effect_beat,
                    total_elapsed,
                    projected_mesh_scratch.get_mut(idx),
                )
                .is_none()
                    && let Some(actors) = build_song_lua_overlay_actor_with_scratch(
                        overlay,
                        overlay_state,
                        topology_index.camera_state(capture_states, idx),
                        asset_manager,
                        z,
                        overlay_space_width,
                        overlay_space_height,
                        effect_time,
                        effect_beat,
                        total_elapsed,
                        projected_mesh_scratch.get_mut(idx),
                    )
                {
                    out.extend(actors);
                }
            }
        }
    }
}

#[cfg(test)]
fn song_lua_capture_children<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    overlay_states: &[SongLuaOverlayState],
    local_overlay_states: &[SongLuaOverlayState],
    order_cache: &mut SongLuaOverlayOrderCache,
    topology_index: &SongLuaOverlayTopologyIndex,
    asset_manager: &AssetManager,
    capture_index: usize,
    proxy_sources: &SongLuaScreenProxySources<'_>,
    overlay_space_width: f32,
    overlay_space_height: f32,
    capture_states: &mut Vec<SongLuaOverlayState>,
    order_scratch: &mut Vec<usize>,
    projected_mesh_scratch: &mut [SongLuaProjectedMeshScratch],
) -> Vec<Actor> {
    let mut out = Vec::new();
    song_lua_capture_children_into(
        &mut out,
        overlays,
        overlay_states,
        local_overlay_states,
        order_cache,
        topology_index,
        asset_manager,
        capture_index,
        proxy_sources,
        None,
        overlay_space_width,
        overlay_space_height,
        0.0,
        0.0,
        0.0,
        capture_states,
        order_scratch,
        projected_mesh_scratch,
    );
    out
}

fn song_lua_overlay_apply_blocks(
    state: SongLuaOverlayState,
    blocks: &[SongLuaOverlayCommandBlock],
    elapsed: f32,
) -> SongLuaOverlayState {
    if !elapsed.is_finite() {
        return state;
    }
    let mut current = state;
    for block in blocks {
        if elapsed < block.start {
            break;
        }
        if block.duration <= f32::EPSILON || elapsed >= block.start + block.duration {
            apply_overlay_delta(&mut current, &block.delta);
            continue;
        }
        let t = song_lua_ease_factor(
            block.easing.as_deref(),
            ((elapsed - block.start) / block.duration).clamp(0.0, 1.0),
            block.opt1,
            block.opt2,
        );
        overlay_state_lerp(&mut current, &block.delta, t);
        return current;
    }
    current
}

fn song_lua_overlay_apply_blocks_cached(
    state: SongLuaOverlayState,
    blocks: &[SongLuaOverlayCommandBlock],
    elapsed: f32,
    next_block: &mut usize,
    active_easing: &mut Option<(usize, SongLuaEase)>,
    block_state: &mut SongLuaOverlayState,
    last_elapsed: &mut f32,
) -> SongLuaOverlayState {
    if !elapsed.is_finite() {
        return state;
    }
    if elapsed < *last_elapsed {
        *next_block = 0;
        *active_easing = None;
        *block_state = state;
    }
    *last_elapsed = elapsed;

    while let Some(block) = blocks.get(*next_block) {
        if elapsed < block.start {
            break;
        }
        if block.duration <= f32::EPSILON || elapsed >= block.start + block.duration {
            apply_overlay_delta(block_state, &block.delta);
            *next_block += 1;
            continue;
        }
        let easing = match *active_easing {
            Some((index, easing)) if index == *next_block => easing,
            _ => {
                let easing = SongLuaEase::from_name(block.easing.as_deref());
                *active_easing = Some((*next_block, easing));
                easing
            }
        };
        let t = easing.factor(
            ((elapsed - block.start) / block.duration).clamp(0.0, 1.0),
            block.opt1,
            block.opt2,
        );
        let mut current = *block_state;
        overlay_state_lerp(&mut current, &block.delta, t);
        return current;
    }
    *block_state
}

fn song_lua_overlay_update_value_lerp(
    from: &crate::SongLuaOverlayUpdateValue,
    to: &crate::SongLuaOverlayUpdateValue,
    t: f32,
) -> crate::SongLuaOverlayUpdateValue {
    use crate::SongLuaOverlayUpdateValue as Value;
    let t = t.clamp(0.0, 1.0);
    match (from, to) {
        (Value::F32(from), Value::F32(to)) => Value::F32((to - from).mul_add(t, *from)),
        (Value::Vec2(from), Value::Vec2(to)) => Value::Vec2(std::array::from_fn(|i| {
            (to[i] - from[i]).mul_add(t, from[i])
        })),
        (Value::Vec3(from), Value::Vec3(to)) => Value::Vec3(std::array::from_fn(|i| {
            (to[i] - from[i]).mul_add(t, from[i])
        })),
        (Value::Vec4(from), Value::Vec4(to)) => Value::Vec4(std::array::from_fn(|i| {
            (to[i] - from[i]).mul_add(t, from[i])
        })),
        (Value::Vec5(from), Value::Vec5(to)) => Value::Vec5(std::array::from_fn(|i| {
            (to[i] - from[i]).mul_add(t, from[i])
        })),
        _ if t >= 1.0 - f32::EPSILON => to.clone(),
        _ => from.clone(),
    }
}

#[derive(Clone, Copy)]
struct SongLuaOverlayUpdateSnap {
    start_second: f32,
    end_second: f32,
    t: f32,
}

// UpdateFunction state is sampled at 10 Hz. A visibility edge can bracket a
// short geometry update, but a longer span is a dormant actor waiting for a
// future command and must not be snapped active across that whole span.
const SONG_LUA_UPDATE_SNAP_MAX_SECONDS: f32 = 0.25;

fn song_lua_overlay_update_snap(
    now: f32,
    tracks: &[crate::SongLuaOverlayRuntimeUpdateTrack],
    visible_track_indices: &[usize],
    track_cursors: &mut [usize],
) -> Option<SongLuaOverlayUpdateSnap> {
    use crate::SongLuaOverlayUpdateValue as Value;
    for &track_index in visible_track_indices {
        let track = tracks.get(track_index)?;
        let cursor = track_cursors.get_mut(track_index)?;
        let next =
            deadsync_gameplay::partition_point_from_hint(&track.samples, *cursor, |sample| {
                sample.second <= now
            });
        *cursor = next;
        let Some(from) = track.samples.get(next.wrapping_sub(1)) else {
            continue;
        };
        let Some(to) = track.samples.get(next) else {
            continue;
        };
        if to.second - from.second > SONG_LUA_UPDATE_SNAP_MAX_SECONDS {
            continue;
        }
        let t = match (&from.value, &to.value) {
            (Value::Bool(false), Value::Bool(true)) => 1.0,
            (Value::Bool(true), Value::Bool(false)) => 0.0,
            _ => continue,
        };
        return Some(SongLuaOverlayUpdateSnap {
            start_second: from.second,
            end_second: to.second,
            t,
        });
    }
    None
}

fn apply_song_lua_overlay_update_value(
    state: &mut SongLuaOverlayState,
    target: crate::SongLuaOverlayUpdateTarget,
    value: &crate::SongLuaOverlayUpdateValue,
) {
    use crate::{SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateValue as Value};
    macro_rules! set_value {
        ($target:ident, $variant:ident, $field:ident) => {
            if target == Target::$target {
                if let Value::$variant(value) = value {
                    state.$field = *value;
                }
                return;
            }
        };
    }
    macro_rules! set_option {
        ($target:ident, $variant:ident, $field:ident) => {
            if target == Target::$target {
                state.$field = match value {
                    Value::$variant(value) => Some(*value),
                    Value::None => None,
                    _ => state.$field,
                };
                return;
            }
        };
    }
    if let (Target::X | Target::Y, Value::F32(value)) = (target, value) {
        let x = if target == Target::X { *value } else { state.x };
        let y = if target == Target::Y { *value } else { state.y };
        crate::move_overlay(state, x, y);
        return;
    }
    set_value!(Z, F32, z);
    set_value!(ZBias, F32, z_bias);
    set_value!(DrawOrder, I32, draw_order);
    set_value!(DrawByZPosition, Bool, draw_by_z_position);
    set_value!(HAlign, F32, halign);
    set_value!(VAlign, F32, valign);
    set_value!(TextAlign, TextAlign, text_align);
    set_value!(Uppercase, Bool, uppercase);
    set_value!(ShadowLen, Vec2, shadow_len);
    set_value!(ShadowColor, Vec4, shadow_color);
    set_value!(Glow, Vec4, glow);
    set_option!(Fov, F32, fov);
    set_option!(Vanishpoint, Vec2, vanishpoint);
    set_value!(Diffuse, Vec4, diffuse);
    if target == Target::VertexColors {
        state.vertex_colors = match value {
            Value::VertexColors(value) => Some(**value),
            Value::None => None,
            _ => state.vertex_colors,
        };
        return;
    }
    set_value!(Visible, Bool, visible);
    set_value!(CropLeft, F32, cropleft);
    set_value!(CropRight, F32, cropright);
    set_value!(CropTop, F32, croptop);
    set_value!(CropBottom, F32, cropbottom);
    set_value!(FadeLeft, F32, fadeleft);
    set_value!(FadeRight, F32, faderight);
    set_value!(FadeTop, F32, fadetop);
    set_value!(FadeBottom, F32, fadebottom);
    set_value!(MaskSource, Bool, mask_source);
    set_value!(MaskDest, Bool, mask_dest);
    set_value!(DepthTest, Bool, depth_test);
    set_value!(Zoom, F32, zoom);
    set_value!(ZoomX, F32, zoom_x);
    set_value!(ZoomY, F32, zoom_y);
    set_value!(ZoomZ, F32, zoom_z);
    set_value!(BaseZoom, F32, basezoom);
    set_value!(BaseZoomX, F32, basezoom_x);
    set_value!(BaseZoomY, F32, basezoom_y);
    set_value!(BaseZoomZ, F32, basezoom_z);
    set_value!(RotationX, F32, rot_x_deg);
    set_value!(RotationY, F32, rot_y_deg);
    set_value!(RotationZ, F32, rot_z_deg);
    set_value!(SkewX, F32, skew_x);
    set_value!(SkewY, F32, skew_y);
    set_value!(Blend, Blend, blend);
    set_value!(Vibrate, Bool, vibrate);
    set_value!(EffectMagnitude, Vec3, effect_magnitude);
    set_value!(EffectClock, EffectClock, effect_clock);
    set_value!(EffectMode, EffectMode, effect_mode);
    set_value!(EffectColor1, Vec4, effect_color1);
    set_value!(EffectColor2, Vec4, effect_color2);
    set_value!(EffectPeriod, F32, effect_period);
    set_value!(EffectOffset, F32, effect_offset);
    set_option!(EffectTiming, Vec5, effect_timing);
    set_value!(Rainbow, Bool, rainbow);
    set_value!(RainbowScroll, Bool, rainbow_scroll);
    set_value!(TextJitter, Bool, text_jitter);
    set_value!(TextDistortion, F32, text_distortion);
    set_value!(TextGlowMode, TextGlowMode, text_glow_mode);
    set_value!(MultAttrsWithDiffuse, Bool, mult_attrs_with_diffuse);
    set_value!(SpriteAnimate, Bool, sprite_animate);
    set_value!(SpriteLoop, Bool, sprite_loop);
    set_value!(SpritePlaybackRate, F32, sprite_playback_rate);
    set_value!(SpriteStateDelay, F32, sprite_state_delay);
    set_option!(SpriteStateIndex, U32, sprite_state_index);
    set_option!(VertSpacing, I32, vert_spacing);
    set_option!(WrapWidthPixels, I32, wrap_width_pixels);
    set_option!(MaxWidth, F32, max_width);
    set_option!(MaxHeight, F32, max_height);
    set_value!(MaxWPreZoom, Bool, max_w_pre_zoom);
    set_value!(MaxHPreZoom, Bool, max_h_pre_zoom);
    set_value!(MaxDimensionUsesZoom, Bool, max_dimension_uses_zoom);
    set_value!(TextureFiltering, Bool, texture_filtering);
    set_value!(TextureWrapping, Bool, texture_wrapping);
    set_option!(TexcoordOffset, Vec2, texcoord_offset);
    set_option!(CustomTextureRect, Vec4, custom_texture_rect);
    set_option!(TexcoordVelocity, Vec2, texcoord_velocity);
    set_option!(Size, Vec2, size);
    set_option!(StretchRect, Vec4, stretch_rect);
}

fn apply_song_lua_overlay_runtime_updates_for(
    now: f32,
    tracks: &[crate::SongLuaOverlayRuntimeUpdateTrack],
    track_range: std::ops::Range<usize>,
    track_cursors: &mut [usize],
    update_snap: Option<SongLuaOverlayUpdateSnap>,
    current: &mut SongLuaOverlayState,
) {
    for track_index in track_range {
        let Some(track) = tracks.get(track_index) else {
            continue;
        };
        let samples = &track.samples;
        let Some(cursor) = track_cursors.get_mut(track_index) else {
            continue;
        };
        let next = deadsync_gameplay::partition_point_from_hint(samples, *cursor, |sample| {
            sample.second <= now
        });
        *cursor = next;
        if next == 0 {
            continue;
        }
        let from = &samples[next - 1];
        let to = samples.get(next).unwrap_or(from);
        let mut t = if to.second <= from.second + f32::EPSILON {
            1.0
        } else {
            (now - from.second) / (to.second - from.second)
        };
        if let Some(snap) = update_snap
            && (from.second - snap.start_second).abs() <= 0.000_1
            && (to.second - snap.end_second).abs() <= 0.000_1
        {
            t = snap.t;
        }
        let value = song_lua_overlay_update_value_lerp(&from.value, &to.value, t);
        apply_song_lua_overlay_update_value(current, track.target, &value);
        if track.target == crate::SongLuaOverlayUpdateTarget::SpriteStateIndex {
            current.sprite_animation_epoch = Some(if t >= 1.0 - f32::EPSILON {
                to.second
            } else {
                from.second
            });
        }
    }
}

fn apply_song_lua_overlay_runtime_eases_for(
    now: f32,
    overlay_index: usize,
    overlay_eases: &[SongLuaOverlayEaseWindowRuntime],
    overlay_ease_ranges: &[std::ops::Range<usize>],
    mut current: SongLuaOverlayState,
) -> SongLuaOverlayState {
    let Some(ease_range) = overlay_ease_ranges.get(overlay_index) else {
        return current;
    };
    for ease in &overlay_eases[ease_range.clone()] {
        debug_assert_eq!(ease.overlay_index, overlay_index);
        // Grouped ease ranges are sorted by start time.
        if now < ease.start_second {
            break;
        }
        if let Some(cutoff_second) = ease.cutoff_second
            && now >= cutoff_second
        {
            continue;
        }
        if now >= ease.sustain_end_second {
            apply_overlay_delta(&mut current, &ease.to.delta);
            if ease.to.delta.sprite_state_index.is_some() {
                current.sprite_animation_epoch = Some(ease.end_second);
            }
            continue;
        }
        if ease.end_second <= ease.start_second || now >= ease.end_second {
            apply_overlay_delta(&mut current, &ease.to.delta);
            if ease.to.delta.sprite_state_index.is_some() {
                current.sprite_animation_epoch = Some(ease.end_second);
            }
            continue;
        }
        let t = ease.easing.factor(
            ((now - ease.start_second) / (ease.end_second - ease.start_second)).clamp(0.0, 1.0),
            ease.opt1,
            ease.opt2,
        );
        apply_overlay_delta(&mut current, &ease.from.delta);
        overlay_state_lerp(&mut current, &ease.to.delta, t);
        if ease.from.delta.sprite_state_index.is_some() {
            current.sprite_animation_epoch = Some(ease.start_second);
        }
    }
    current
}

fn reapply_active_song_lua_overlay_runtime_eases_for(
    now: f32,
    overlay_index: usize,
    overlay_eases: &[SongLuaOverlayEaseWindowRuntime],
    overlay_ease_ranges: &[std::ops::Range<usize>],
    current: &mut SongLuaOverlayState,
) {
    let Some(ease_range) = overlay_ease_ranges.get(overlay_index) else {
        return;
    };
    for ease in &overlay_eases[ease_range.clone()] {
        if now < ease.start_second {
            break;
        }
        if ease
            .cutoff_second
            .is_some_and(|cutoff_second| now >= cutoff_second)
            || now >= ease.sustain_end_second
        {
            continue;
        }
        if ease.end_second <= ease.start_second || now >= ease.end_second {
            apply_overlay_delta(current, &ease.to.delta);
            continue;
        }
        let t = ease.easing.factor(
            ((now - ease.start_second) / (ease.end_second - ease.start_second)).clamp(0.0, 1.0),
            ease.opt1,
            ease.opt2,
        );
        apply_overlay_delta(current, &ease.from.delta);
        overlay_state_lerp(current, &ease.to.delta, t);
        if ease.from.delta.sprite_state_index.is_some() {
            current.sprite_animation_epoch = Some(ease.start_second);
        }
    }
}

fn song_lua_overlay_render_state_from<S: NoteskinSlot + Clone>(
    now: f32,
    overlay_index: usize,
    overlay: &SongLuaOverlayActor<S>,
    overlay_events: &[Vec<SongLuaOverlayMessageRuntime>],
    overlay_eases: &[SongLuaOverlayEaseWindowRuntime],
    overlay_ease_ranges: &[std::ops::Range<usize>],
    overlay_updates: &[crate::SongLuaOverlayRuntimeUpdateTrack],
    update_range: std::ops::Range<usize>,
    update_cursors: &mut [usize],
    update_snap: Option<SongLuaOverlayUpdateSnap>,
    message_cache: &mut SongLuaMessageStateCache,
) -> SongLuaOverlayState {
    let events = overlay_events.get(overlay_index).map(Vec::as_slice);
    let has_events = events.is_some_and(|events| !events.is_empty());
    let has_eases = overlay_ease_ranges
        .get(overlay_index)
        .is_some_and(|range| !range.is_empty());
    let has_updates = !update_range.is_empty();
    if !has_events && !has_eases && !has_updates {
        return overlay.initial_state;
    }
    song_lua_overlay_render_state_dynamic(
        now,
        overlay_index,
        overlay,
        events,
        overlay_eases,
        overlay_ease_ranges,
        overlay_updates,
        update_range,
        update_cursors,
        update_snap,
        message_cache,
    )
}

fn song_lua_overlay_render_state_dynamic<S: NoteskinSlot + Clone>(
    now: f32,
    overlay_index: usize,
    overlay: &SongLuaOverlayActor<S>,
    events: Option<&[SongLuaOverlayMessageRuntime]>,
    overlay_eases: &[SongLuaOverlayEaseWindowRuntime],
    overlay_ease_ranges: &[std::ops::Range<usize>],
    overlay_updates: &[crate::SongLuaOverlayRuntimeUpdateTrack],
    update_range: std::ops::Range<usize>,
    update_cursors: &mut [usize],
    update_snap: Option<SongLuaOverlayUpdateSnap>,
    message_cache: &mut SongLuaMessageStateCache,
) -> SongLuaOverlayState {
    let current = song_lua_message_state_cached(
        now,
        overlay.initial_state,
        &overlay.message_commands,
        events,
        message_cache,
    );
    let mut current = apply_song_lua_overlay_runtime_eases_for(
        now,
        overlay_index,
        overlay_eases,
        overlay_ease_ranges,
        current,
    );
    apply_song_lua_overlay_runtime_updates_for(
        now,
        overlay_updates,
        update_range,
        update_cursors,
        update_snap,
        &mut current,
    );
    // Exact function eases and sampled UpdateFunction tracks can describe the
    // same Lua setter. ITGmania executes that function ease every frame, so it
    // must remain authoritative while active instead of being replaced by the
    // lower-rate sampled approximation.
    reapply_active_song_lua_overlay_runtime_eases_for(
        now,
        overlay_index,
        overlay_eases,
        overlay_ease_ranges,
        &mut current,
    );
    current
}

fn replay_song_lua_message_state(
    now: f32,
    initial_state: SongLuaOverlayState,
    message_commands: &[SongLuaOverlayMessageCommand],
    events: Option<&[SongLuaOverlayMessageRuntime]>,
) -> SongLuaOverlayState {
    let Some(events) = events else {
        return initial_state;
    };
    let mut current = initial_state;
    let mut active: Option<(&[SongLuaOverlayCommandBlock], SongLuaOverlayState, f32)> = None;
    for event in events {
        let event_second = event.event_second;
        if event_second > now {
            break;
        }
        let Some(command) = message_commands.get(event.command_index) else {
            continue;
        };
        if let Some((blocks, base, start_second)) = active.take() {
            let elapsed = event_second - start_second;
            current = song_lua_overlay_apply_blocks(base, blocks, elapsed);
            if let Some(epoch) = song_lua_sprite_animation_epoch(blocks, elapsed, start_second) {
                current.sprite_animation_epoch = Some(epoch);
            }
        }
        let base = current;
        current = song_lua_overlay_apply_blocks(base, &command.blocks, 0.0);
        active = Some((&command.blocks, base, event_second));
    }
    if let Some((blocks, base, start_second)) = active {
        let elapsed = now - start_second;
        current = song_lua_overlay_apply_blocks(base, blocks, elapsed);
        if let Some(epoch) = song_lua_sprite_animation_epoch(blocks, elapsed, start_second) {
            current.sprite_animation_epoch = Some(epoch);
        }
    }
    current
}

fn song_lua_sprite_animation_epoch(
    blocks: &[SongLuaOverlayCommandBlock],
    elapsed: f32,
    command_start_second: f32,
) -> Option<f32> {
    blocks.iter().rev().find_map(|block| {
        let activation = block.start + block.duration.max(0.0);
        (block.delta.sprite_state_index.is_some() && elapsed >= activation)
            .then_some(command_start_second + activation)
    })
}

fn song_lua_message_state_cached(
    now: f32,
    initial_state: SongLuaOverlayState,
    message_commands: &[SongLuaOverlayMessageCommand],
    events: Option<&[SongLuaOverlayMessageRuntime]>,
    cache: &mut SongLuaMessageStateCache,
) -> SongLuaOverlayState {
    let Some(events) = events else {
        cache.reset(initial_state);
        return initial_state;
    };
    if !now.is_finite() {
        return replay_song_lua_message_state(now, initial_state, message_commands, Some(events));
    }
    if !cache.initialized || now < cache.processed_until {
        cache.reset(initial_state);
    }

    while let Some(event) = events.get(cache.next_event) {
        if event.event_second > now {
            break;
        }
        cache.next_event += 1;
        cache.processed_until = event.event_second;
        if message_commands.get(event.command_index).is_none() {
            continue;
        }
        if let Some(active_command_index) = cache.active_command_index
            && let Some(active_command) = message_commands.get(active_command_index)
        {
            let command_base = cache.base_state;
            let elapsed = event.event_second - cache.active_start_second;
            cache.base_state = song_lua_overlay_apply_blocks_cached(
                command_base,
                &active_command.blocks,
                elapsed,
                &mut cache.active_next_block,
                &mut cache.active_easing,
                &mut cache.active_block_state,
                &mut cache.active_last_elapsed,
            );
            if let Some(epoch) = song_lua_sprite_animation_epoch(
                &active_command.blocks,
                elapsed,
                cache.active_start_second,
            ) {
                cache.base_state.sprite_animation_epoch = Some(epoch);
            }
        }
        cache.active_command_index = Some(event.command_index);
        cache.active_start_second = event.event_second;
        cache.reset_active_blocks(cache.base_state);
    }

    let Some(command) = cache
        .active_command_index
        .and_then(|command_index| message_commands.get(command_index))
    else {
        return cache.base_state;
    };
    let elapsed = now - cache.active_start_second;
    let mut current = song_lua_overlay_apply_blocks_cached(
        cache.base_state,
        &command.blocks,
        elapsed,
        &mut cache.active_next_block,
        &mut cache.active_easing,
        &mut cache.active_block_state,
        &mut cache.active_last_elapsed,
    );
    if let Some(epoch) =
        song_lua_sprite_animation_epoch(&command.blocks, elapsed, cache.active_start_second)
    {
        current.sprite_animation_epoch = Some(epoch);
    }
    current
}

fn song_lua_player_render_state<
    P: deadsync_gameplay::GameplayProfileData,
    S: NoteskinSlot + Clone,
>(
    state: &GameplayCoreState<P, S>,
    player_index: usize,
    message_cache: &mut SongLuaMessageStateCache,
) -> SongLuaOverlayState {
    let song_lua_visuals = state.song_lua_visuals();
    let Some(actor) = song_lua_visuals.player_actors.get(player_index) else {
        return SongLuaOverlayState::default();
    };
    song_lua_captured_actor_state_from(
        state.current_music_time_display(),
        actor,
        song_lua_visuals
            .player_events
            .get(player_index)
            .map(Vec::as_slice),
        message_cache,
    )
}

fn song_lua_song_foreground_state_from(
    now: f32,
    song_foreground: &SongLuaCapturedActor,
    events: &[SongLuaOverlayMessageRuntime],
    message_cache: &mut SongLuaMessageStateCache,
) -> SongLuaOverlayState {
    song_lua_captured_actor_state_from(now, song_foreground, Some(events), message_cache)
}

fn song_lua_captured_actor_state_from(
    now: f32,
    actor: &SongLuaCapturedActor,
    events: Option<&[SongLuaOverlayMessageRuntime]>,
    message_cache: &mut SongLuaMessageStateCache,
) -> SongLuaOverlayState {
    song_lua_captured_state_from(
        now,
        actor.initial_state,
        &actor.message_commands,
        events,
        message_cache,
    )
}

fn song_lua_child_visible(
    now: f32,
    actor: &SongLuaCapturedChildActor,
    events: &[SongLuaOverlayMessageRuntime],
    message_cache: &mut SongLuaMessageStateCache,
    captured: bool,
) -> bool {
    song_lua_captured_state_from(
        now,
        actor.initial_state,
        &actor.message_commands,
        Some(events),
        message_cache,
    )
    .visible
        || captured
}

fn song_lua_captured_state_from(
    now: f32,
    initial_state: SongLuaOverlayState,
    message_commands: &[SongLuaOverlayMessageCommand],
    events: Option<&[SongLuaOverlayMessageRuntime]>,
    message_cache: &mut SongLuaMessageStateCache,
) -> SongLuaOverlayState {
    if events.is_none_or(<[_]>::is_empty) {
        message_cache.initialized = false;
        return initial_state;
    }
    song_lua_message_state_cached(now, initial_state, message_commands, events, message_cache)
}

fn song_lua_song_foreground_state<
    P: deadsync_gameplay::GameplayProfileData,
    S: NoteskinSlot + Clone,
>(
    state: &GameplayCoreState<P, S>,
    message_cache: &mut SongLuaMessageStateCache,
) -> SongLuaOverlayState {
    let song_lua_visuals = state.song_lua_visuals();
    song_lua_song_foreground_state_from(
        state.current_music_time_display(),
        &song_lua_visuals.song_foreground,
        song_lua_visuals.song_foreground_events.as_slice(),
        message_cache,
    )
}

fn song_lua_capture_tint(color: [f32; 4], tint: [f32; 4]) -> [f32; 4] {
    [
        color[0] * tint[0],
        color[1] * tint[1],
        color[2] * tint[2],
        color[3] * tint[3],
    ]
}

fn song_lua_biased_world_z(state: SongLuaOverlayState, effect_z: f32) -> f32 {
    effect_z + state.z_bias
}

fn song_lua_add_z(z: i16, delta: i16) -> i16 {
    (i32::from(z) + i32::from(delta)).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

const SONG_LUA_PLAYER_LAYER_Z_BASE: i16 = 900;
const SONG_LUA_OVERLAY_LAYER_Z_BASE: i16 = 1100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SongLuaLayerDepth {
    base: i16,
    ceiling: i16,
}

impl SongLuaLayerDepth {
    fn shifted(self, z: f32) -> Self {
        Self {
            base: song_lua_add_z(self.base, song_lua_rounded_z(z)),
            ..self
        }
    }

    fn draw_z(self, draw_idx: usize) -> i16 {
        song_lua_add_z(self.base, draw_idx.min(i16::MAX as usize) as i16).min(self.ceiling)
    }
}

const SONG_LUA_BACKGROUND_DEPTH: SongLuaLayerDepth = SongLuaLayerDepth {
    base: 0,
    ceiling: SONG_LUA_PLAYER_LAYER_Z_BASE - 1,
};
/// Highest song-Lua foreground z; theme transitions must draw above this.
pub const LUA_FOREGROUND_Z_MAX: i16 = 1199;

const SONG_LUA_FOREGROUND_DEPTH: SongLuaLayerDepth = SongLuaLayerDepth {
    base: SONG_LUA_OVERLAY_LAYER_Z_BASE,
    // ScreenWithMenuElements draws transitions above the entire foreground.
    ceiling: LUA_FOREGROUND_Z_MAX,
};

fn song_lua_rounded_z(value: f32) -> i16 {
    if !value.is_finite() {
        return 0;
    }
    value
        .round()
        .clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
}

fn song_lua_player_layer_z(
    song_lua_active: bool,
    actor: &SongLuaCapturedActor,
    current: SongLuaOverlayState,
    runtime_z: f32,
) -> i16 {
    if !song_lua_active {
        return 0;
    }
    let _ = actor;
    song_lua_add_z(
        SONG_LUA_PLAYER_LAYER_Z_BASE,
        song_lua_rounded_z(current.z + runtime_z),
    )
}

fn song_lua_style_capture_actor(
    mut actor: Actor,
    capture_tint: [f32; 4],
    blend: Option<BlendMode>,
    z_shift: i16,
) -> Actor {
    song_lua_style_capture_actor_in_place(&mut actor, capture_tint, blend, z_shift);
    actor
}

fn song_lua_style_capture_actor_in_place(
    actor: &mut Actor,
    capture_tint: [f32; 4],
    blend: Option<BlendMode>,
    z_shift: i16,
) {
    // Only live style fields change. Keep child Vec/Box storage and shared
    // geometry in place, including the style boundary of shared wrappers.
    match actor {
        Actor::Sprite {
            tint,
            glow,
            shadow_color,
            blend: actor_blend,
            z,
            ..
        } => {
            *tint = song_lua_capture_tint(*tint, capture_tint);
            *glow = song_lua_capture_tint(*glow, capture_tint);
            *shadow_color = song_lua_capture_tint(*shadow_color, capture_tint);
            *actor_blend = blend.unwrap_or(*actor_blend);
            *z = song_lua_add_z(*z, z_shift);
        }
        Actor::Text {
            color,
            stroke_color,
            glow,
            shadow_color,
            blend: actor_blend,
            z,
            ..
        } => {
            *color = song_lua_capture_tint(*color, capture_tint);
            *stroke_color = stroke_color.map(|color| song_lua_capture_tint(color, capture_tint));
            *glow = song_lua_capture_tint(*glow, capture_tint);
            *shadow_color = song_lua_capture_tint(*shadow_color, capture_tint);
            *actor_blend = blend.unwrap_or(*actor_blend);
            *z = song_lua_add_z(*z, z_shift);
        }
        Actor::Mesh {
            tint,
            blend: actor_blend,
            z,
            ..
        }
        | Actor::ReusableMesh {
            tint,
            blend: actor_blend,
            z,
            ..
        } => {
            *tint = song_lua_capture_tint(*tint, capture_tint);
            *actor_blend = blend.unwrap_or(*actor_blend);
            *z = song_lua_add_z(*z, z_shift);
        }
        Actor::TexturedMesh {
            tint,
            glow,
            blend: actor_blend,
            z,
            ..
        }
        | Actor::ReusableTexturedMesh {
            tint,
            glow,
            blend: actor_blend,
            z,
            ..
        } => {
            *tint = song_lua_capture_tint(*tint, capture_tint);
            *glow = song_lua_capture_tint(*glow, capture_tint);
            *actor_blend = blend.unwrap_or(*actor_blend);
            *z = song_lua_add_z(*z, z_shift);
        }
        Actor::Frame { children, z, .. } => {
            *z = song_lua_add_z(*z, z_shift);
            for child in children {
                song_lua_style_capture_actor_in_place(child, capture_tint, blend, z_shift);
            }
        }
        Actor::SharedFrame {
            tint,
            blend: actor_blend,
            z,
            ..
        }
        | Actor::SharedTransform {
            tint,
            blend: actor_blend,
            z,
            ..
        }
        | Actor::RetainedFrame {
            tint,
            blend: actor_blend,
            z,
            ..
        } => {
            *tint = song_lua_capture_tint(*tint, capture_tint);
            *actor_blend = blend.or(*actor_blend);
            *z = song_lua_add_z(*z, z_shift);
        }
        Actor::Camera { children, .. } => {
            for child in children {
                song_lua_style_capture_actor_in_place(child, capture_tint, blend, z_shift);
            }
        }
        Actor::Shadow { color, child, .. } => {
            *color = song_lua_capture_tint(*color, capture_tint);
            song_lua_style_capture_actor_in_place(child, capture_tint, blend, z_shift);
        }
        Actor::CameraPush { .. } | Actor::CameraPop => {}
    }
}

#[inline(always)]
const fn song_lua_overlay_blend(blend: SongLuaOverlayBlendMode) -> BlendMode {
    match blend {
        SongLuaOverlayBlendMode::Alpha => BlendMode::Alpha,
        SongLuaOverlayBlendMode::Add => BlendMode::Add,
        SongLuaOverlayBlendMode::Multiply => BlendMode::Multiply,
        SongLuaOverlayBlendMode::Subtract => BlendMode::Subtract,
    }
}

#[inline(always)]
fn song_lua_overlay_effect_state(state: SongLuaOverlayState) -> EffectState {
    let period = state.effect_period.max(f32::EPSILON);
    EffectState {
        clock: state.effect_clock,
        mode: state.effect_mode,
        color1: state.effect_color1,
        color2: state.effect_color2,
        period,
        offset: state.effect_offset,
        timing: state
            .effect_timing
            .unwrap_or([period * 0.5, 0.0, period * 0.5, 0.0, 0.0]),
        magnitude: state.effect_magnitude,
        ..deadlib_present::anim::EffectState::default()
    }
}

#[inline(always)]
fn song_lua_effect_lerp(a: f32, b: f32, t: f32) -> f32 {
    (b - a).mul_add(t, a)
}

fn song_lua_proxy_effect(
    mut state: SongLuaOverlayState,
    effect_time: f32,
    effect_beat: f32,
    actor_seed: u32,
) -> SongLuaOverlayState {
    let mut offset = [0.0; 3];
    let mut scale = [1.0; 3];
    let mut rotation = [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg];
    song_lua_apply_overlay_effect(
        song_lua_overlay_effect_state(state),
        state.rainbow,
        song_lua_overlay_vibrate_magnitude(state),
        effect_time,
        effect_beat,
        actor_seed,
        &mut state.diffuse,
        &mut state.glow,
        &mut offset,
        &mut scale,
        &mut rotation,
    );
    state.x += offset[0];
    state.y += offset[1];
    state.z += offset[2];
    state.zoom_x *= scale[0];
    state.zoom_y *= scale[1];
    state.zoom_z *= scale[2];
    [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg] = rotation;
    state
}

#[inline(always)]
fn song_lua_overlay_has_visible_output(state: SongLuaOverlayState) -> bool {
    // Actor::BeginDraw multiplies glow effects by the original diffuse alpha.
    // Only an explicit, non-effect glow can outlive a transparent actor.
    state.diffuse[3] > f32::EPSILON || state.glow[3] > f32::EPSILON
}

fn song_lua_apply_overlay_effect(
    effect: EffectState,
    rainbow: bool,
    vibrate_magnitude: [f32; 3],
    effect_time: f32,
    effect_beat: f32,
    actor_seed: u32,
    tint: &mut [f32; 4],
    glow: &mut [f32; 4],
    offset: &mut [f32; 3],
    scale: &mut [f32; 3],
    rot_deg: &mut [f32; 3],
) {
    if vibrate_magnitude
        .iter()
        .any(|value| value.abs() > f32::EPSILON)
    {
        // ITGmania chooses independent random offsets for each actor. Use
        // a stable 60 Hz frame clock so high-refresh rendering keeps the same
        // rapid shake cadence instead of becoming a several-hundred-Hz blur.
        let frame = (effect_time.max(0.0) * 60.0).floor() as u32;
        let jitter = std::array::from_fn(|axis| {
            let mut hash = frame
                ^ actor_seed.wrapping_mul(0x85eb_ca6b)
                ^ (axis as u32 + 1).wrapping_mul(0x9e37_79b9);
            hash ^= hash >> 16;
            hash = hash.wrapping_mul(0x7feb_352d);
            hash ^= hash >> 15;
            hash = hash.wrapping_mul(0x846c_a68b);
            hash ^= hash >> 16;
            (hash as f32 / u32::MAX as f32).mul_add(2.0, -1.0)
        });
        song_lua_apply_vibration(offset, vibrate_magnitude, jitter);
    }
    if matches!(effect.mode, deadlib_present::anim::EffectMode::Spin) {
        let units = deadlib_present::anim::effect_clock_units(effect, effect_time, effect_beat);
        rot_deg[0] = effect.magnitude[0]
            .mul_add(units, rot_deg[0])
            .rem_euclid(360.0);
        rot_deg[1] = effect.magnitude[1]
            .mul_add(units, rot_deg[1])
            .rem_euclid(360.0);
        rot_deg[2] = effect.magnitude[2]
            .mul_add(units, rot_deg[2])
            .rem_euclid(360.0);
    }
    if let Some(percent) = deadlib_present::anim::effect_mix(effect, effect_time, effect_beat) {
        match effect.mode {
            deadlib_present::anim::EffectMode::DiffuseBlink => {
                let color = if percent > 0.5 {
                    effect.color1
                } else {
                    effect.color2
                };
                let alpha = tint[3];
                *tint = color;
                tint[3] *= alpha;
            }
            deadlib_present::anim::EffectMode::DiffuseRamp => {
                for (idx, out) in tint.iter_mut().enumerate() {
                    let color =
                        song_lua_effect_lerp(effect.color2[idx], effect.color1[idx], percent)
                            .clamp(0.0, 1.0);
                    *out = (*out * color).clamp(0.0, 1.0);
                }
            }
            deadlib_present::anim::EffectMode::DiffuseShift => {
                let between = deadlib_present::anim::glowshift_mix(percent);
                for (idx, out) in tint.iter_mut().enumerate() {
                    let color =
                        song_lua_effect_lerp(effect.color2[idx], effect.color1[idx], between)
                            .clamp(0.0, 1.0);
                    *out = (*out * color).clamp(0.0, 1.0);
                }
            }
            deadlib_present::anim::EffectMode::GlowShift => {
                let between = deadlib_present::anim::glowshift_mix(percent);
                for (idx, out) in glow.iter_mut().enumerate() {
                    *out = song_lua_effect_lerp(effect.color2[idx], effect.color1[idx], between)
                        .clamp(0.0, 1.0);
                }
                glow[3] *= tint[3];
            }
            deadlib_present::anim::EffectMode::GlowBlink => {
                let alpha = tint[3];
                *glow = if percent > 0.5 {
                    effect.color1
                } else {
                    effect.color2
                };
                glow[3] *= alpha;
            }
            deadlib_present::anim::EffectMode::GlowRamp => {
                let alpha = tint[3];
                for (idx, out) in glow.iter_mut().enumerate() {
                    *out = song_lua_effect_lerp(effect.color2[idx], effect.color1[idx], percent)
                        .clamp(0.0, 1.0);
                }
                glow[3] *= alpha;
            }
            deadlib_present::anim::EffectMode::Pulse => {
                let pulse = (percent * std::f32::consts::PI).sin().clamp(0.0, 1.0);
                let zoom =
                    song_lua_effect_lerp(effect.magnitude[0], effect.magnitude[1], pulse).max(0.0);
                scale[0] *= zoom * song_lua_effect_lerp(effect.color1[0], effect.color2[0], pulse);
                scale[1] *= zoom * song_lua_effect_lerp(effect.color1[1], effect.color2[1], pulse);
                scale[2] *= zoom * song_lua_effect_lerp(effect.color1[2], effect.color2[2], pulse);
            }
            deadlib_present::anim::EffectMode::Bob => {
                let bob = (percent * 2.0 * std::f32::consts::PI).sin();
                for i in 0..3 {
                    offset[i] = effect.magnitude[i].mul_add(bob, offset[i]);
                }
            }
            deadlib_present::anim::EffectMode::Bounce => {
                let bounce = (percent * std::f32::consts::PI).sin();
                for i in 0..3 {
                    offset[i] = effect.magnitude[i].mul_add(bounce, offset[i]);
                }
            }
            deadlib_present::anim::EffectMode::Wag => {
                let wag = (percent * 2.0 * std::f32::consts::PI).sin();
                for i in 0..3 {
                    rot_deg[i] = effect.magnitude[i].mul_add(wag, rot_deg[i]);
                }
            }
            deadlib_present::anim::EffectMode::Spin | deadlib_present::anim::EffectMode::None => {}
        }
    }
    if rainbow {
        let color = song_lua_rainbow_color(effect_time, effect.period, effect.offset);
        tint[0] *= color[0];
        tint[1] *= color[1];
        tint[2] *= color[2];
    }
    offset[0] = offset[0].max(-1_000_000.0).min(1_000_000.0);
    offset[1] = offset[1].max(-1_000_000.0).min(1_000_000.0);
    offset[2] = offset[2].max(-1_000_000.0).min(1_000_000.0);
    tint[0] = tint[0].clamp(0.0, 1.0);
    tint[1] = tint[1].clamp(0.0, 1.0);
    tint[2] = tint[2].clamp(0.0, 1.0);
    tint[3] = tint[3].clamp(0.0, 1.0);
    glow[0] = glow[0].clamp(0.0, 1.0);
    glow[1] = glow[1].clamp(0.0, 1.0);
    glow[2] = glow[2].clamp(0.0, 1.0);
    glow[3] = glow[3].clamp(0.0, 1.0);
    scale[0] = scale[0].max(0.0);
    scale[1] = scale[1].max(0.0);
    scale[2] = scale[2].max(0.0);
}

#[inline(always)]
fn song_lua_apply_vibration(position: &mut [f32; 3], magnitude: [f32; 3], jitter: [f32; 3]) {
    for axis in 0..3 {
        position[axis] = magnitude[axis].mul_add(jitter[axis], position[axis]);
    }
}

#[inline(always)]
fn song_lua_overlay_vibrate_magnitude(state: SongLuaOverlayState) -> [f32; 3] {
    let own = if state.vibrate {
        state.effect_magnitude
    } else {
        [0.0; 3]
    };
    [
        own[0] + state.inherited_vibrate[0],
        own[1] + state.inherited_vibrate[1],
        own[2] + state.inherited_vibrate[2],
    ]
}

fn song_lua_rainbow_color(time: f32, period: f32, offset: f32) -> [f32; 3] {
    let hue = ((time + offset) / period.max(f32::EPSILON)).rem_euclid(1.0);
    let h = hue * 6.0;
    let x = 1.0 - (h.rem_euclid(2.0) - 1.0).abs();
    if h < 1.0 {
        [1.0, x, 0.0]
    } else if h < 2.0 {
        [x, 1.0, 0.0]
    } else if h < 3.0 {
        [0.0, 1.0, x]
    } else if h < 4.0 {
        [0.0, x, 1.0]
    } else if h < 5.0 {
        [x, 0.0, 1.0]
    } else {
        [1.0, 0.0, x]
    }
}

const SONG_LUA_TEXT_RAINBOW_COLORS: [[f32; 4]; 7] = [
    [1.0, 0.0, 0.4, 1.0],
    [0.8, 0.2, 0.6, 1.0],
    [0.4, 0.3, 0.5, 1.0],
    [0.2, 0.6, 1.0, 1.0],
    [0.2, 0.8, 0.8, 1.0],
    [0.2, 0.8, 0.4, 1.0],
    [1.0, 0.8, 0.2, 1.0],
];

fn song_lua_rainbow_scroll_attributes(text: &str, total_elapsed: f32) -> Vec<TextAttribute> {
    song_lua_rainbow_scroll_attributes_at_phase(text, song_lua_rainbow_scroll_phase(total_elapsed))
}

fn song_lua_rainbow_scroll_attributes_at_phase(
    text: &str,
    first_color: usize,
) -> Vec<TextAttribute> {
    let char_count = text.chars().count();
    let mut out = Vec::with_capacity(char_count);
    append_song_lua_rainbow_scroll_attributes_at_phase(text, first_color, &mut out);
    out
}

fn append_song_lua_rainbow_scroll_attributes_at_phase(
    text: &str,
    first_color: usize,
    out: &mut Vec<TextAttribute>,
) {
    let char_count = text.chars().count();
    for index in 0..char_count {
        out.push(TextAttribute {
            start: index,
            length: 1,
            color: SONG_LUA_TEXT_RAINBOW_COLORS
                [(first_color + index) % SONG_LUA_TEXT_RAINBOW_COLORS.len()],
            vertex_colors: None,
            glow: None,
        });
    }
}

#[inline(always)]
fn song_lua_rainbow_scroll_phase(total_elapsed: f32) -> usize {
    ((total_elapsed / 0.2).floor() as usize) % SONG_LUA_TEXT_RAINBOW_COLORS.len()
}

fn song_lua_rainbow_scroll_phases(
    text: &str,
) -> [Arc<[TextAttribute]>; SONG_LUA_TEXT_RAINBOW_COLORS.len()] {
    std::array::from_fn(|phase| {
        Arc::from(song_lua_rainbow_scroll_attributes_at_phase(text, phase).into_boxed_slice())
    })
}

fn song_lua_transparent_text_attributes(
    text: &str,
    scratch: Option<&mut SongLuaProjectedMeshScratch>,
) -> TextAttributes {
    let char_count = text.chars().count();
    if char_count == 0 {
        return TextAttributes::default();
    }
    let fill = |out: &mut Vec<TextAttribute>| {
        out.push(TextAttribute {
            start: 0,
            length: char_count,
            color: [1.0, 1.0, 1.0, 0.0],
            vertex_colors: None,
            glow: None,
        });
    };
    if let Some(scratch) = scratch {
        scratch.update_text_glow(fill)
    } else {
        let mut out = Vec::with_capacity(1);
        fill(&mut out);
        out.into()
    }
}

fn song_lua_text_attributes_have_glow(attributes: &[TextAttribute]) -> bool {
    attributes
        .iter()
        .any(|attr| attr.glow.is_some_and(|glow| glow[3] > f32::EPSILON))
}

fn song_lua_text_glow_attributes(
    text: &str,
    attributes: &[TextAttribute],
    glow: [f32; 4],
    scratch: Option<&mut SongLuaProjectedMeshScratch>,
) -> TextAttributes {
    let char_count = text.chars().count();
    if char_count == 0 {
        return TextAttributes::default();
    }
    let fill = |out: &mut Vec<TextAttribute>| {
        if glow[3] > f32::EPSILON {
            out.push(TextAttribute {
                start: 0,
                length: char_count,
                color: glow,
                vertex_colors: None,
                glow: None,
            });
        }
        for attr in attributes {
            let Some(glow) = attr.glow else {
                continue;
            };
            if glow[3] <= f32::EPSILON {
                continue;
            }
            out.push(TextAttribute {
                start: attr.start,
                length: attr.length,
                color: glow,
                vertex_colors: None,
                glow: None,
            });
        }
    };
    if let Some(scratch) = scratch {
        scratch.update_text_glow(fill)
    } else {
        let mut out = Vec::with_capacity(attributes.len() + usize::from(glow[3] > f32::EPSILON));
        fill(&mut out);
        out.into()
    }
}

fn song_lua_text_attributes_for_diffuse_mode(
    attributes: &Arc<[TextAttribute]>,
    color: [f32; 4],
    text: &str,
    mult_attrs_with_diffuse: bool,
    scratch: Option<&mut SongLuaProjectedMeshScratch>,
) -> (TextAttributes, [f32; 4]) {
    if attributes.is_empty() || mult_attrs_with_diffuse {
        return (TextAttributes::from(Arc::clone(attributes)), color);
    }
    let char_count = text.chars().count();
    if char_count == 0 {
        return (TextAttributes::from(Arc::clone(attributes)), color);
    }
    if color
        .iter()
        .all(|component| (*component - 1.0).abs() <= f32::EPSILON)
    {
        return (
            TextAttributes::from(Arc::clone(attributes)),
            [1.0, 1.0, 1.0, 1.0],
        );
    }
    let fill = |out: &mut Vec<TextAttribute>| {
        out.push(TextAttribute {
            start: 0,
            length: char_count,
            color,
            vertex_colors: None,
            glow: None,
        });
        out.extend_from_slice(attributes);
    };
    let attributes = if let Some(scratch) = scratch {
        scratch.update_text_diffuse(fill)
    } else {
        let mut out = Vec::with_capacity(attributes.len() + 1);
        fill(&mut out);
        out.into()
    };
    (attributes, [1.0, 1.0, 1.0, 1.0])
}

#[cfg(any(test, feature = "test-support"))]
fn song_lua_overlay_camera_state<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    overlay_states: &[SongLuaOverlayState],
    mut index: Option<usize>,
) -> Option<SongLuaOverlayState> {
    while let Some(current) = index {
        let overlay = overlays.get(current)?;
        let state = overlay_states.get(current).copied()?;
        if matches!(
            overlay.kind,
            SongLuaOverlayKind::ActorFrame | SongLuaOverlayKind::ActorFrameTexture { .. }
        ) && state.fov.is_some()
        {
            return Some(state);
        }
        index = overlay.parent_index;
    }
    None
}
fn song_lua_overlay_view_proj(
    camera_state: SongLuaOverlayState,
    overlay_space_width: f32,
    overlay_space_height: f32,
) -> Option<Matrix4> {
    let mut fov_deg = camera_state.fov?;
    if !fov_deg.is_finite() || fov_deg <= f32::EPSILON {
        return None;
    }
    fov_deg = fov_deg.clamp(0.1, 179.9);
    let width = screen_width().max(1.0);
    let height = screen_height().max(1.0);
    let x_scale = width / overlay_space_width.max(1.0);
    let y_scale = height / overlay_space_height.max(1.0);
    let vanish = camera_state
        .vanishpoint
        .unwrap_or([0.5 * overlay_space_width, 0.5 * overlay_space_height]);
    let mut vanish_x = vanish[0].mul_add(-x_scale, width);
    let mut vanish_y = vanish[1].mul_add(-y_scale, height);
    vanish_x = 0.5f32.mul_add(-width, vanish_x);
    vanish_y = 0.5f32.mul_add(-height, vanish_y);

    let theta = 0.5 * fov_deg.to_radians();
    let dist = (0.5 * width / theta.tan()).max(1.0);
    let proj = glam::camera::rh::proj::opengl::frustum(
        0.5f32.mul_add(-width, vanish_x) / dist,
        0.5f32.mul_add(width, vanish_x) / dist,
        0.5f32.mul_add(height, vanish_y) / dist,
        0.5f32.mul_add(-height, vanish_y) / dist,
        1.0,
        dist + 1000.0,
    );
    let eye_x = 0.5f32.mul_add(width, -vanish_x);
    let eye_y = 0.5f32.mul_add(height, -vanish_y);
    let view = glam::camera::rh::view::look_at_mat4(
        Vector3::new(eye_x, eye_y, dist),
        Vector3::new(eye_x, eye_y, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
    );
    Some(proj * view)
}

fn song_lua_actor_multi_vertex_mesh(
    vertices: &Arc<[SongLuaOverlayMeshVertex]>,
    tint: [f32; 4],
    x_scale: f32,
    y_scale: f32,
    actor_scale: [f32; 2],
    effect_scale: [f32; 3],
    rotation_z_deg: f32,
    skew: [f32; 2],
) -> Arc<[MeshVertex]> {
    let mut out = Vec::with_capacity(vertices.len());
    append_song_lua_actor_multi_vertex_mesh(
        &mut out,
        vertices,
        tint,
        x_scale,
        y_scale,
        actor_scale,
        effect_scale,
        rotation_z_deg,
        skew,
    );
    Arc::from(out.into_boxed_slice())
}

#[allow(clippy::too_many_arguments)]
fn append_song_lua_actor_multi_vertex_mesh(
    out: &mut Vec<MeshVertex>,
    vertices: &[SongLuaOverlayMeshVertex],
    tint: [f32; 4],
    x_scale: f32,
    y_scale: f32,
    actor_scale: [f32; 2],
    effect_scale: [f32; 3],
    rotation_z_deg: f32,
    skew: [f32; 2],
) {
    out.reserve(vertices.len());
    for vertex in vertices.iter() {
        out.push(MeshVertex {
            pos: song_lua_actor_multi_vertex_pos(
                vertex.pos,
                x_scale,
                y_scale,
                actor_scale,
                effect_scale,
                rotation_z_deg,
                skew,
            ),
            color: song_lua_capture_tint(vertex.color, tint),
        });
    }
}

fn song_lua_actor_multi_vertex_textured_mesh(
    vertices: &Arc<[SongLuaOverlayMeshVertex]>,
    x_scale: f32,
    y_scale: f32,
    actor_scale: [f32; 2],
    effect_scale: [f32; 3],
    rotation_z_deg: f32,
    skew: [f32; 2],
) -> Arc<[TexturedMeshVertex]> {
    let mut out = Vec::with_capacity(vertices.len());
    append_song_lua_actor_multi_vertex_textured_mesh(
        &mut out,
        vertices,
        x_scale,
        y_scale,
        actor_scale,
        effect_scale,
        rotation_z_deg,
        skew,
    );
    Arc::from(out.into_boxed_slice())
}

#[allow(clippy::too_many_arguments)]
fn append_song_lua_actor_multi_vertex_textured_mesh(
    out: &mut Vec<TexturedMeshVertex>,
    vertices: &[SongLuaOverlayMeshVertex],
    x_scale: f32,
    y_scale: f32,
    actor_scale: [f32; 2],
    effect_scale: [f32; 3],
    rotation_z_deg: f32,
    skew: [f32; 2],
) {
    out.reserve(vertices.len());
    for vertex in vertices.iter() {
        let pos = song_lua_actor_multi_vertex_pos(
            vertex.pos,
            x_scale,
            y_scale,
            actor_scale,
            effect_scale,
            rotation_z_deg,
            skew,
        );
        out.push(TexturedMeshVertex {
            pos: [pos[0], pos[1], 0.0],
            uv: vertex.uv,
            tex_matrix_scale: [1.0, 1.0],
            color: vertex.color,
        });
    }
}

fn song_lua_actor_multi_vertex_pos(
    pos: [f32; 2],
    x_scale: f32,
    y_scale: f32,
    actor_scale: [f32; 2],
    effect_scale: [f32; 3],
    rotation_z_deg: f32,
    skew: [f32; 2],
) -> [f32; 2] {
    let scale = [
        x_scale * actor_scale[0] * effect_scale[0],
        y_scale * actor_scale[1] * effect_scale[1],
    ];
    let (sin_z, cos_z) = rotation_z_deg.to_radians().sin_cos();
    let mut x = pos[0] * scale[0];
    let mut y = -pos[1] * scale[1];
    if skew[0].abs() > f32::EPSILON {
        x = skew[0].mul_add(y, x);
    }
    if skew[1].abs() > f32::EPSILON {
        y = skew[1].mul_add(x, y);
    }
    [y.mul_add(-sin_z, x * cos_z), y.mul_add(cos_z, x * sin_z)]
}

#[allow(clippy::too_many_arguments)]
fn append_song_lua_model_actors(
    out: &mut impl Extend<Actor>,
    layers: &[SongLuaOverlayModelLayer],
    state: SongLuaOverlayState,
    asset_manager: &AssetManager,
    z: i16,
    x_scale: f32,
    y_scale: f32,
    actor_scale: [f32; 2],
    effect_scale: [f32; 3],
    effect_rot: [f32; 3],
    effect_offset: [f32; 3],
    tint: [f32; 4],
    glow: [f32; 4],
    blend: BlendMode,
    total_elapsed: f32,
    prewarmed_geometry_keys: Option<&[TMeshCacheKey]>,
    prewarmed_glow_vertices: Option<&[Arc<[TexturedMeshVertex]>]>,
) -> bool {
    let mut emitted = false;
    let offset = [
        effect_offset[0].mul_add(x_scale, state.x * x_scale),
        effect_offset[1].mul_add(y_scale, state.y * y_scale),
    ];
    for (idx, layer) in layers.iter().enumerate() {
        if !layer.draw.visible || !asset_manager.has_texture_key(layer.texture_key.as_ref()) {
            continue;
        }
        let scroll = song_lua_model_layer_scroll(layer, total_elapsed);
        let shift = match state.texcoord_offset {
            Some([dx, dy]) => [scroll[0] + dx, scroll[1] + dy],
            None => scroll,
        };
        let uv_offset = [layer.uv_offset[0] + shift[0], layer.uv_offset[1] + shift[1]];
        let uv_tex_shift = [
            layer.uv_tex_shift[0] + shift[0],
            layer.uv_tex_shift[1] + shift[1],
        ];
        let actor = Actor::TexturedMesh {
            align: [0.0, 0.0],
            offset,
            world_z: song_lua_biased_world_z(state, effect_offset[2]),
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            local_transform: song_lua_model_local_transform(
                layer.model_size,
                layer.draw,
                x_scale,
                y_scale,
                actor_scale,
                effect_scale,
                effect_rot,
                [state.skew_x, state.skew_y],
            ),
            texture: Arc::clone(&layer.texture_key),
            tint: song_lua_capture_tint(layer.draw.tint, tint),
            glow: [1.0, 1.0, 1.0, 0.0],
            vertices: Arc::clone(&layer.vertices),
            geom_cache_key: prewarmed_geometry_keys
                .and_then(|keys| keys.get(idx))
                .copied()
                .unwrap_or(INVALID_TMESH_CACHE_KEY),
            uv_scale: layer.uv_scale,
            uv_offset,
            uv_tex_shift,
            depth_test: state.depth_test,
            visible: true,
            blend: if layer.draw.blend_add {
                BlendMode::Add
            } else {
                blend
            },
            z: song_lua_add_z(z, idx.min(i16::MAX as usize) as i16)
                .min(SONG_LUA_FOREGROUND_DEPTH.ceiling),
        };
        let glow_actor = song_lua_overlay_glow_actor_with_static_vertices(
            &actor,
            glow,
            state.text_glow_mode,
            None,
            prewarmed_glow_vertices.and_then(|vertices| vertices.get(idx)),
        );
        out.extend([actor]);
        emitted = true;
        if let Some(glow_actor) = glow_actor {
            out.extend([glow_actor]);
        }
    }
    emitted
}

fn song_lua_model_layer_scroll(layer: &SongLuaOverlayModelLayer, total_elapsed: f32) -> [f32; 2] {
    if layer.uv_velocity == [0.0, 0.0] {
        return [0.0, 0.0];
    }
    let clock = layer
        .uv_cycle_seconds
        .filter(|total| *total > f32::EPSILON && total.is_finite())
        .map_or(total_elapsed, |total| {
            total_elapsed.rem_euclid(total) / total
        });
    [layer.uv_velocity[0] * clock, layer.uv_velocity[1] * clock]
}

#[allow(clippy::too_many_arguments)]
fn append_song_lua_noteskin_actors<S: NoteskinSlot + Clone>(
    out: &mut impl Extend<Actor>,
    slots: &[S],
    state: SongLuaOverlayState,
    asset_manager: &AssetManager,
    z: i16,
    x_scale: f32,
    y_scale: f32,
    actor_scale: [f32; 2],
    effect_scale: [f32; 3],
    effect_rot: [f32; 3],
    effect_offset: [f32; 3],
    tint: [f32; 4],
    glow: [f32; 4],
    blend: BlendMode,
    total_elapsed: f32,
    effect_beat: f32,
    scratch: Option<&mut SongLuaProjectedMeshScratch>,
) -> bool {
    let mut emitted = false;
    let (mut model_cache, glow_vertices) = match scratch {
        Some(scratch) => (
            scratch.noteskin_model_cache.as_mut(),
            scratch.noteskin_glow_vertices.as_deref(),
        ),
        None => (None, None),
    };
    let center = [
        effect_offset[0].mul_add(x_scale, state.x * x_scale),
        effect_offset[1].mul_add(y_scale, state.y * y_scale),
    ];
    for (idx, slot) in slots.iter().enumerate() {
        if !asset_manager.has_texture_key(slot.texture_key_shared().as_ref()) {
            continue;
        }
        let mut draw = model_cache.as_deref_mut().map_or_else(
            || slot.model_draw_at(total_elapsed, effect_beat),
            |cache| cache.draw_at(slot, total_elapsed, effect_beat),
        );
        draw.pos[0] *= x_scale * actor_scale[0] * effect_scale[0];
        draw.pos[1] *= y_scale * actor_scale[1] * effect_scale[1];
        draw.pos[2] *= song_lua_overlay_z_scale(state) * effect_scale[2];
        draw.rot[0] += effect_rot[0];
        draw.rot[1] += effect_rot[1];
        let frame = slot.frame_index(total_elapsed, effect_beat);
        let uv = song_lua_noteskin_slot_uv(slot, frame, total_elapsed, state.texcoord_offset);
        let base_size = song_lua_noteskin_slot_size(slot);
        let size = [
            base_size[0] * x_scale * actor_scale[0] * effect_scale[0],
            base_size[1] * y_scale * actor_scale[1] * effect_scale[1],
        ];
        if size[0].abs() <= f32::EPSILON || size[1].abs() <= f32::EPSILON {
            continue;
        }
        let layer_z = song_lua_add_z(z, idx.min(i16::MAX as usize) as i16)
            .min(SONG_LUA_FOREGROUND_DEPTH.ceiling);
        let actor = if slot.model().is_some() {
            // Actor::BeginDraw scales each model axis independently. The
            // noteskin renderer fits models uniformly from size[1], so pass
            // native dimensions and put the composed Lua scale in the draw.
            draw.zoom[0] *= x_scale * actor_scale[0] * effect_scale[0];
            draw.zoom[1] *= y_scale * actor_scale[1] * effect_scale[1];
            draw.zoom[2] *= song_lua_overlay_z_scale(state) * effect_scale[2];
            let size = base_size;
            if let Some(cache) = model_cache.as_deref_mut() {
                noteskin_model_actor_from_draw_cached(
                    slot,
                    draw,
                    center,
                    size,
                    uv,
                    -(slot.sprite_def().rotation_deg as f32 + effect_rot[2]),
                    tint,
                    blend,
                    layer_z,
                    cache,
                )
            } else {
                noteskin_model_actor_from_draw(
                    slot,
                    draw,
                    center,
                    size,
                    uv,
                    -(slot.sprite_def().rotation_deg as f32 + effect_rot[2]),
                    tint,
                    blend,
                    layer_z,
                )
            }
        } else {
            song_lua_noteskin_sprite_actor(
                slot,
                draw,
                center,
                size,
                uv,
                effect_rot[2],
                tint,
                blend,
                layer_z,
            )
        };
        let Some(actor) = actor else {
            continue;
        };
        let glow_actor = song_lua_overlay_glow_actor_with_static_vertices(
            &actor,
            glow,
            state.text_glow_mode,
            None,
            glow_vertices
                .and_then(|vertices| vertices.get(idx))
                .and_then(Option::as_ref),
        );
        out.extend([actor]);
        emitted = true;
        if let Some(glow_actor) = glow_actor {
            out.extend([glow_actor]);
        }
    }
    emitted
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn song_lua_noteskin_actor<S: NoteskinSlot + Clone>(
    slots: &[S],
    state: SongLuaOverlayState,
    asset_manager: &AssetManager,
    z: i16,
    x_scale: f32,
    y_scale: f32,
    actor_scale: [f32; 2],
    effect_scale: [f32; 3],
    effect_rot: [f32; 3],
    effect_offset: [f32; 3],
    tint: [f32; 4],
    glow: [f32; 4],
    blend: BlendMode,
    total_elapsed: f32,
    effect_beat: f32,
) -> Option<SongLuaActorList> {
    let mut out = SongLuaActorList::new();
    append_song_lua_noteskin_actors(
        &mut out,
        slots,
        state,
        asset_manager,
        z,
        x_scale,
        y_scale,
        actor_scale,
        effect_scale,
        effect_rot,
        effect_offset,
        tint,
        glow,
        blend,
        total_elapsed,
        effect_beat,
        None,
    )
    .then_some(out)
}

fn song_lua_noteskin_slot_uv<S: NoteskinSlot + Clone>(
    slot: &S,
    frame: usize,
    total_elapsed: f32,
    texcoord_offset: Option<[f32; 2]>,
) -> [f32; 4] {
    let mut uv = slot.uv_for_frame_at(frame, total_elapsed);
    if let Some([dx, dy]) = texcoord_offset {
        uv[0] += dx;
        uv[1] += dy;
        uv[2] += dx;
        uv[3] += dy;
    }
    uv
}

fn song_lua_noteskin_slot_size<S: NoteskinSlot + Clone>(slot: &S) -> [f32; 2] {
    if let Some(model) = slot.model() {
        let size = model.size();
        if size[0] > f32::EPSILON && size[1] > f32::EPSILON {
            return size;
        }
    }
    slot.logical_size()
}

fn song_lua_noteskin_sprite_actor<S: NoteskinSlot + Clone>(
    slot: &S,
    draw: ModelDrawState,
    center: [f32; 2],
    size: [f32; 2],
    uv: [f32; 4],
    rotation_z: f32,
    tint: [f32; 4],
    blend: BlendMode,
    z: i16,
) -> Option<Actor> {
    if !draw.visible {
        return None;
    }
    let size = [
        size[0] * draw.zoom[0].max(0.0),
        size[1] * draw.zoom[1].max(0.0),
    ];
    if size[0].abs() <= f32::EPSILON || size[1].abs() <= f32::EPSILON {
        return None;
    }
    Some(Actor::Sprite {
        align: [0.5, 0.5],
        offset: [center[0] + draw.pos[0], center[1] - draw.pos[1]],
        world_z: 0.0,
        size: [SizeSpec::Px(size[0]), SizeSpec::Px(size[1])],
        source: SpriteSource::Texture(slot.texture_key_shared()),
        tint: [
            tint[0] * draw.tint[0],
            tint[1] * draw.tint[1],
            tint[2] * draw.tint[2],
            tint[3] * draw.tint[3],
        ],
        glow: [1.0, 1.0, 1.0, 0.0],
        z,
        cell: None,
        grid: None,
        uv_rect: Some(uv),
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
        blend: if draw.blend_add {
            BlendMode::Add
        } else {
            blend
        },
        mask_source: false,
        mask_dest: false,
        rot_x_deg: draw.rot[0],
        rot_y_deg: draw.rot[1],
        rot_z_deg: draw.rot[2] - slot.sprite_def().rotation_deg as f32 - rotation_z,
        skew: [0.0, 0.0],
        local_offset: [0.0, 0.0],
        local_offset_rot_sin_cos: [0.0, 1.0],
        texcoordvelocity: None,
        animate: false,
        state_delay: 0.1,
        scale: [1.0, 1.0],
        shadow_len: [0.0, 0.0],
        shadow_color: [0.0, 0.0, 0.0, 0.5],
        effect: deadlib_present::anim::EffectState::default(),
    })
}

fn song_lua_model_local_transform(
    model_size: [f32; 2],
    draw: SongLuaOverlayModelDraw,
    x_scale: f32,
    y_scale: f32,
    actor_scale: [f32; 2],
    effect_scale: [f32; 3],
    effect_rot: [f32; 3],
    skew: [f32; 2],
) -> Matrix4 {
    let align_y = (0.5 - draw.vert_align) * model_size[1];
    let scale = Vector3::new(
        x_scale * actor_scale[0] * effect_scale[0] * draw.zoom[0],
        y_scale * actor_scale[1] * effect_scale[1] * draw.zoom[1],
        actor_scale[1].abs() * effect_scale[2] * draw.zoom[2],
    );
    Matrix4::from_translation(Vector3::new(
        draw.pos[0] * x_scale,
        -draw.pos[1] * y_scale,
        draw.pos[2],
    )) * song_lua_overlay_local_transform(
        [
            draw.rot[0] + effect_rot[0],
            draw.rot[1] + effect_rot[1],
            draw.rot[2] + effect_rot[2],
        ],
        skew[0],
        skew[1],
    ) * Matrix4::from_translation(Vector3::new(0.0, align_y, 0.0))
        * Matrix4::from_scale(scale)
        * Matrix4::from_scale(Vector3::new(1.0, -1.0, 1.0))
}

fn song_lua_song_meter_actor(
    state: SongLuaOverlayState,
    stream_state: SongLuaOverlayState,
    stream_width: f32,
    music_length_seconds: f32,
    x_scale: f32,
    y_scale: f32,
    z: i16,
    total_elapsed: f32,
) -> Option<Actor> {
    let progress = if music_length_seconds > f32::EPSILON {
        (total_elapsed / music_length_seconds).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let parent_scale = song_lua_overlay_axis_scale(state);
    let stream_scale = song_lua_overlay_axis_scale(stream_state);
    let full_width = stream_width * parent_scale[0].abs() * stream_scale[0].abs();
    let progress_width = full_width * progress;
    if progress_width <= f32::EPSILON {
        return None;
    }
    let stream_height = stream_state.size.map_or(1.0, |size| size[1].abs())
        * parent_scale[1].abs()
        * stream_scale[1].abs();
    let left = full_width.mul_add(-0.5, stream_state.x.mul_add(parent_scale[0], state.x));
    let y = stream_state.y.mul_add(parent_scale[1], state.y);
    let tint = [
        state.diffuse[0] * stream_state.diffuse[0],
        state.diffuse[1] * stream_state.diffuse[1],
        state.diffuse[2] * stream_state.diffuse[2],
        state.diffuse[3] * stream_state.diffuse[3],
    ];
    let mut actor = deadlib_present::__act_from_builder!((
        align(0.0, stream_state.valign):
        xy(left * x_scale, y * y_scale):
        zoomto(progress_width * x_scale, stream_height * y_scale):
        diffuse(tint[0], tint[1], tint[2], tint[3]):
        z(z)
    ) deadlib_assets::SpriteBuilder::solid());
    if let Actor::Sprite {
        visible,
        blend,
        mask_source,
        mask_dest,
        ..
    } = &mut actor
    {
        *visible = state.visible && stream_state.visible;
        *blend = if stream_state.blend == SongLuaOverlayBlendMode::Alpha {
            song_lua_overlay_blend(state.blend)
        } else {
            song_lua_overlay_blend(stream_state.blend)
        };
        *mask_source = state.mask_source || stream_state.mask_source;
        *mask_dest = state.mask_dest || stream_state.mask_dest;
    }
    Some(actor)
}

#[inline(always)]
pub fn song_meter_progress(current_seconds: f32, first_second: f32, last_second: f32) -> f32 {
    if !current_seconds.is_finite() || !first_second.is_finite() || !last_second.is_finite() {
        return 0.0;
    }
    let duration = last_second - first_second;
    if duration <= f32::EPSILON {
        return 0.0;
    }
    ((current_seconds - first_second) / duration).clamp(0.0, 1.0)
}

#[allow(clippy::too_many_arguments)]
fn song_lua_graph_display_actor(
    state: SongLuaOverlayState,
    body_values: &Arc<[f32]>,
    body_state: SongLuaOverlayState,
    line_state: SongLuaOverlayState,
    size: [f32; 2],
    x_scale: f32,
    y_scale: f32,
    z: i16,
    mut scratch: Option<&mut SongLuaProjectedMeshScratch>,
) -> Option<Actor> {
    let reuse_graph = scratch
        .as_ref()
        .is_some_and(|scratch| scratch.graph_frame.is_some());
    if reuse_graph
        && let Some(frame) = scratch
            .as_deref_mut()
            .and_then(|scratch| scratch.graph_frame.as_mut())
    {
        // Release last frame's child mesh Arcs before refilling their buffers.
        frame.clear();
    }
    let mut children = SmallVec::<[Actor; 2]>::new();
    if let Some(body) = song_lua_graph_display_body_actor(
        state,
        body_values,
        body_state,
        size,
        x_scale,
        y_scale,
        z,
        if reuse_graph {
            scratch.as_deref_mut()
        } else {
            None
        },
    ) {
        children.push(body);
    }
    if let Some(line) = song_lua_graph_display_line_actor(
        state,
        body_values,
        line_state,
        size,
        x_scale,
        y_scale,
        z,
        if reuse_graph {
            scratch.as_deref_mut()
        } else {
            None
        },
    ) {
        children.push(line);
    }
    match children.len() {
        0 => None,
        1 => children.pop(),
        _ if reuse_graph => {
            let shared = scratch
                .and_then(|scratch| scratch.graph_frame.as_mut())
                .and_then(|frame| frame.refill([0.0, 0.0], |out| out.extend(children.drain(..))))
                .expect("visible GraphDisplay body and line must refill the shared frame");
            Some(Actor::SharedFrame {
                align: [0.0, 0.0],
                offset: [0.0, 0.0],
                size: [SizeSpec::Fill, SizeSpec::Fill],
                children: shared,
                background: None,
                z: 0,
                tint: [1.0, 1.0, 1.0, 1.0],
                blend: None,
            })
        }
        _ => Some(Actor::Frame {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Fill, SizeSpec::Fill],
            children: children.into_vec(),
            background: None,
            z: 0,
        }),
    }
}

#[allow(clippy::too_many_arguments)]
fn song_lua_graph_display_body_actor(
    state: SongLuaOverlayState,
    body_values: &[f32],
    body_state: SongLuaOverlayState,
    size: [f32; 2],
    x_scale: f32,
    y_scale: f32,
    z: i16,
    scratch: Option<&mut SongLuaProjectedMeshScratch>,
) -> Option<Actor> {
    if !body_state.visible || body_state.diffuse[3] <= f32::EPSILON {
        return None;
    }
    let values = graph_display_values_or_default(body_values);
    let graph_scale = song_lua_overlay_axis_scale(state);
    let body_scale = song_lua_overlay_axis_scale(body_state);
    let width = size[0] * graph_scale[0].abs() * body_scale[0].abs();
    let height = size[1] * graph_scale[1].abs() * body_scale[1].abs();
    if width <= f32::EPSILON || height <= f32::EPSILON {
        return None;
    }
    let left = body_state
        .x
        .mul_add(graph_scale[0], width.mul_add(-state.halign, state.x));
    let top = body_state.y.mul_add(
        graph_scale[1],
        (size[1] * graph_scale[1].abs()).mul_add(-state.valign, state.y),
    );
    let tint = [
        state.diffuse[0] * body_state.diffuse[0],
        state.diffuse[1] * body_state.diffuse[1],
        state.diffuse[2] * body_state.diffuse[2],
        state.diffuse[3] * body_state.diffuse[3],
    ];
    let bottom = top + height;
    let geometry_key = [
        left.to_bits(),
        top.to_bits(),
        width.to_bits(),
        height.to_bits(),
        tint[0].to_bits(),
        tint[1].to_bits(),
        tint[2].to_bits(),
        tint[3].to_bits(),
        x_scale.to_bits(),
        y_scale.to_bits(),
    ];
    let fill = |vertices: &mut Vec<MeshVertex>| {
        for (index, pair) in values.windows(2).enumerate() {
            let x0 = left + width * index as f32 / (values.len() - 1) as f32;
            let x1 = left + width * (index + 1) as f32 / (values.len() - 1) as f32;
            let y0 = (1.0 - pair[0].clamp(0.0, 1.0)).mul_add(height, top);
            let y1 = (1.0 - pair[1].clamp(0.0, 1.0)).mul_add(height, top);
            push_graph_display_tri(
                vertices,
                [x0 * x_scale, y0 * y_scale],
                [x0 * x_scale, bottom * y_scale],
                [x1 * x_scale, bottom * y_scale],
                tint,
            );
            push_graph_display_tri(
                vertices,
                [x0 * x_scale, y0 * y_scale],
                [x1 * x_scale, bottom * y_scale],
                [x1 * x_scale, y1 * y_scale],
                tint,
            );
        }
    };
    let visible = state.visible && body_state.visible;
    let blend = if body_state.blend == SongLuaOverlayBlendMode::Alpha {
        song_lua_overlay_blend(state.blend)
    } else {
        song_lua_overlay_blend(body_state.blend)
    };
    Some(if let Some(scratch) = scratch {
        Actor::ReusableMesh {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            tint: [1.0, 1.0, 1.0, 1.0],
            vertices: scratch.update_graph_body(geometry_key, fill),
            visible,
            blend,
            z,
        }
    } else {
        let mut vertices = Vec::with_capacity((values.len().saturating_sub(1)) * 6);
        fill(&mut vertices);
        Actor::Mesh {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            tint: [1.0; 4],
            vertices: Arc::from(vertices.into_boxed_slice()),
            visible,
            blend,
            z,
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn song_lua_graph_display_line_actor(
    state: SongLuaOverlayState,
    body_values: &[f32],
    line_state: SongLuaOverlayState,
    size: [f32; 2],
    x_scale: f32,
    y_scale: f32,
    z: i16,
    scratch: Option<&mut SongLuaProjectedMeshScratch>,
) -> Option<Actor> {
    if !line_state.visible || line_state.diffuse[3] <= f32::EPSILON {
        return None;
    }
    let values = graph_display_values_or_default(body_values);
    let graph_scale = song_lua_overlay_axis_scale(state);
    let line_scale = song_lua_overlay_axis_scale(line_state);
    let width = size[0] * graph_scale[0].abs() * line_scale[0].abs();
    if width <= f32::EPSILON {
        return None;
    }
    let line_height = line_state.size.map_or(1.0, |line_size| line_size[1].abs())
        * graph_scale[1].abs()
        * line_scale[1].abs();
    let left = line_state
        .x
        .mul_add(graph_scale[0], width.mul_add(-state.halign, state.x));
    let top = (size[1] * graph_scale[1].abs()).mul_add(-state.valign, state.y);
    let height = size[1] * graph_scale[1].abs();
    let y = line_state
        .y
        .mul_add(graph_scale[1], height.mul_add(0.5, top));
    let tint = [
        state.diffuse[0] * line_state.diffuse[0],
        state.diffuse[1] * line_state.diffuse[1],
        state.diffuse[2] * line_state.diffuse[2],
        state.diffuse[3] * line_state.diffuse[3],
    ];
    let stroke = line_height.max(1.0);
    let geometry_key = [
        left.to_bits(),
        y.to_bits(),
        width.to_bits(),
        height.to_bits(),
        stroke.to_bits(),
        tint[0].to_bits(),
        tint[1].to_bits(),
        tint[2].to_bits(),
        tint[3].to_bits(),
        x_scale.to_bits(),
        y_scale.to_bits(),
    ];
    let fill = |vertices: &mut Vec<MeshVertex>| {
        for (index, pair) in values.windows(2).enumerate() {
            let x0 = left + width * index as f32 / (values.len() - 1) as f32;
            let x1 = left + width * (index + 1) as f32 / (values.len() - 1) as f32;
            let y0 = (0.5 - pair[0].clamp(0.0, 1.0)).mul_add(height, y);
            let y1 = (0.5 - pair[1].clamp(0.0, 1.0)).mul_add(height, y);
            push_graph_display_line_segment(
                vertices,
                [x0 * x_scale, y0 * y_scale],
                [x1 * x_scale, y1 * y_scale],
                stroke * y_scale,
                tint,
            );
        }
    };
    let visible = state.visible && line_state.visible;
    let blend = if line_state.blend == SongLuaOverlayBlendMode::Alpha {
        song_lua_overlay_blend(state.blend)
    } else {
        song_lua_overlay_blend(line_state.blend)
    };
    Some(if let Some(scratch) = scratch {
        Actor::ReusableMesh {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            tint: [1.0, 1.0, 1.0, 1.0],
            vertices: scratch.update_graph_line(geometry_key, fill),
            visible,
            blend,
            z,
        }
    } else {
        let mut vertices = Vec::with_capacity((values.len().saturating_sub(1)) * 6);
        fill(&mut vertices);
        Actor::Mesh {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            tint: [1.0; 4],
            vertices: Arc::from(vertices.into_boxed_slice()),
            visible,
            blend,
            z,
        }
    })
}

fn graph_display_values_or_default(values: &[f32]) -> &[f32] {
    static DEFAULT: [f32; 2] = [0.5, 0.5];
    if values.len() >= 2 { values } else { &DEFAULT }
}

fn push_graph_display_line_segment(
    out: &mut Vec<MeshVertex>,
    start: [f32; 2],
    end: [f32; 2],
    stroke: f32,
    color: [f32; 4],
) {
    let dx = end[0] - start[0];
    let dy = end[1] - start[1];
    let len = (dx * dx + dy * dy).sqrt();
    if len <= f32::EPSILON {
        return;
    }
    let half = stroke * 0.5;
    let nx = -dy / len * half;
    let ny = dx / len * half;
    let a = [start[0] + nx, start[1] + ny];
    let b = [start[0] - nx, start[1] - ny];
    let c = [end[0] - nx, end[1] - ny];
    let d = [end[0] + nx, end[1] + ny];
    push_graph_display_tri(out, a, b, c, color);
    push_graph_display_tri(out, a, c, d, color);
}

fn push_graph_display_tri(
    out: &mut Vec<MeshVertex>,
    a: [f32; 2],
    b: [f32; 2],
    c: [f32; 2],
    color: [f32; 4],
) {
    out.push(MeshVertex { pos: a, color });
    out.push(MeshVertex { pos: b, color });
    out.push(MeshVertex { pos: c, color });
}

#[cfg(test)]
fn song_lua_project_overlay_point(view_proj: Matrix4, point: [f32; 3]) -> Option<[f32; 2]> {
    let clip = view_proj * Vector4::new(point[0], point[1], point[2], 1.0);
    if !clip.w.is_finite() || clip.w <= f32::EPSILON {
        return None;
    }
    let inv_w = clip.w.recip();
    let ndc_x = clip.x * inv_w;
    let ndc_y = clip.y * inv_w;
    if !(ndc_x.is_finite() && ndc_y.is_finite()) {
        return None;
    }
    Some([
        0.5f32.mul_add(ndc_x, 0.5) * screen_width(),
        0.5f32.mul_add(-ndc_y, 0.5) * screen_height(),
    ])
}

fn song_lua_overlay_rect(
    state: SongLuaOverlayState,
    default_size: [f32; 2],
    x_scale: f32,
    y_scale: f32,
    size_scale_x: f32,
    size_scale_y: f32,
) -> Option<([f32; 2], [f32; 2])> {
    let (base_center, base_size) = if let Some([left, top, right, bottom]) = state.stretch_rect {
        (
            [
                f32::midpoint(left, right) * x_scale,
                f32::midpoint(top, bottom) * y_scale,
            ],
            [
                (right - left).abs() * x_scale * size_scale_x,
                (bottom - top).abs() * y_scale * size_scale_y,
            ],
        )
    } else {
        (
            [
                (0.5 - state.halign)
                    .mul_add(default_size[0] * x_scale * size_scale_x, state.x * x_scale),
                (0.5 - state.valign)
                    .mul_add(default_size[1] * y_scale * size_scale_y, state.y * y_scale),
            ],
            [
                default_size[0] * x_scale * size_scale_x,
                default_size[1] * y_scale * size_scale_y,
            ],
        )
    };
    if base_size[0] <= f32::EPSILON || base_size[1] <= f32::EPSILON {
        return None;
    }
    let cl = state.cropleft.clamp(0.0, 1.0);
    let cr = state.cropright.clamp(0.0, 1.0);
    let ct = state.croptop.clamp(0.0, 1.0);
    let cb = state.cropbottom.clamp(0.0, 1.0);
    let sx_crop = (1.0 - cl - cr).max(0.0);
    let sy_crop = (1.0 - ct - cb).max(0.0);
    if sx_crop <= f32::EPSILON || sy_crop <= f32::EPSILON {
        return None;
    }
    Some((
        [
            ((cl - cr) * base_size[0]).mul_add(0.5, base_center[0]),
            ((ct - cb) * base_size[1]).mul_add(0.5, base_center[1]),
        ],
        [base_size[0] * sx_crop, base_size[1] * sy_crop],
    ))
}

fn song_lua_overlay_uvs(
    state: SongLuaOverlayState,
    sheet_dims: Option<(u32, u32)>,
    states: &[crate::SongLuaSpriteState],
    flip_x: bool,
    flip_y: bool,
    animation_elapsed: f32,
    texture_elapsed: f32,
) -> [[f32; 2]; 4] {
    let cl = state.cropleft.clamp(0.0, 1.0);
    let cr = state.cropright.clamp(0.0, 1.0);
    let ct = state.croptop.clamp(0.0, 1.0);
    let cb = state.cropbottom.clamp(0.0, 1.0);
    let [
        mut uv_scale_x,
        mut uv_scale_y,
        mut uv_offset_x,
        mut uv_offset_y,
    ] = if let Some([u0, v0, u1, v1]) =
        song_lua_overlay_uv_rect(state, sheet_dims, states, animation_elapsed)
    {
        [
            (u1 - u0).abs().max(1e-6),
            (v1 - v0).abs().max(1e-6),
            u0.min(u1),
            v0.min(v1),
        ]
    } else {
        [1.0, 1.0, 0.0, 0.0]
    };
    uv_offset_x = uv_scale_x.mul_add(cl, uv_offset_x);
    uv_offset_y = uv_scale_y.mul_add(ct, uv_offset_y);
    uv_scale_x *= (1.0 - cl - cr).max(0.0);
    uv_scale_y *= (1.0 - ct - cb).max(0.0);
    if flip_x {
        uv_offset_x += uv_scale_x;
        uv_scale_x = -uv_scale_x;
    }
    if flip_y {
        uv_offset_y += uv_scale_y;
        uv_scale_y = -uv_scale_y;
    }
    if let Some(velocity) = state.texcoord_velocity {
        uv_offset_x = velocity[0].mul_add(texture_elapsed, uv_offset_x);
        uv_offset_y = velocity[1].mul_add(texture_elapsed, uv_offset_y);
    }
    [
        [uv_offset_x, uv_offset_y],
        [uv_offset_x + uv_scale_x, uv_offset_y],
        [uv_offset_x + uv_scale_x, uv_offset_y + uv_scale_y],
        [uv_offset_x, uv_offset_y + uv_scale_y],
    ]
}

#[inline(always)]
fn song_lua_projected_edge_factor(t: f32, feather_l: f32, feather_r: f32) -> f32 {
    let mut left = 1.0;
    let mut right = 1.0;
    if feather_l > f32::EPSILON {
        left = ((t - 0.0) / feather_l).clamp(0.0, 1.0);
    }
    if feather_r > f32::EPSILON {
        right = ((1.0 - t) / feather_r).clamp(0.0, 1.0);
    }
    left.min(right)
}

#[inline(always)]
fn song_lua_projected_overlay_edge_fade(
    state: SongLuaOverlayState,
    flip_x: bool,
    flip_y: bool,
) -> [f32; 4] {
    let cl = state.cropleft.clamp(0.0, 1.0);
    let cr = state.cropright.clamp(0.0, 1.0);
    let ct = state.croptop.clamp(0.0, 1.0);
    let cb = state.cropbottom.clamp(0.0, 1.0);
    let sx_crop = (1.0 - cl - cr).max(0.0);
    let sy_crop = (1.0 - ct - cb).max(0.0);
    if sx_crop <= f32::EPSILON || sy_crop <= f32::EPSILON {
        return [0.0, 0.0, 0.0, 0.0];
    }

    let fl = state.fadeleft.clamp(0.0, 1.0);
    let fr = state.faderight.clamp(0.0, 1.0);
    let ft = state.fadetop.clamp(0.0, 1.0);
    let fb = state.fadebottom.clamp(0.0, 1.0);

    let mut fl_size = (fl + state.cropleft.min(0.0)).max(0.0);
    let mut fr_size = (fr + state.cropright.min(0.0)).max(0.0);
    let mut ft_size = (ft + state.croptop.min(0.0)).max(0.0);
    let mut fb_size = (fb + state.cropbottom.min(0.0)).max(0.0);

    let sum_x = fl_size + fr_size;
    if sum_x > 0.0 && sx_crop < sum_x {
        let scale = sx_crop / sum_x;
        fl_size *= scale;
        fr_size *= scale;
    }

    let sum_y = ft_size + fb_size;
    if sum_y > 0.0 && sy_crop < sum_y {
        let scale = sy_crop / sum_y;
        ft_size *= scale;
        fb_size *= scale;
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

    [fl_eff, fr_eff, ft_eff, fb_eff]
}

fn song_lua_projected_overlay_axis_slices(start_fade: f32, end_fade: f32) -> SmallVec<[f32; 4]> {
    let mut out: SmallVec<[f32; 4]> = SmallVec::new();
    out.push(0.0);
    for value in [start_fade, 1.0 - end_fade, 1.0] {
        let value = value.clamp(0.0, 1.0);
        if out
            .last()
            .is_none_or(|last| (value - *last).abs() > f32::EPSILON)
        {
            out.push(value);
        }
    }
    out
}

#[inline(always)]
fn song_lua_projected_overlay_uv_point(uv: [[f32; 2]; 4], x: f32, y: f32) -> [f32; 2] {
    let top_u = song_lua_effect_lerp(uv[0][0], uv[1][0], x);
    let top_v = song_lua_effect_lerp(uv[0][1], uv[1][1], x);
    let bottom_u = song_lua_effect_lerp(uv[3][0], uv[2][0], x);
    let bottom_v = song_lua_effect_lerp(uv[3][1], uv[2][1], x);
    [
        song_lua_effect_lerp(top_u, bottom_u, y),
        song_lua_effect_lerp(top_v, bottom_v, y),
    ]
}

#[inline(always)]
fn song_lua_projected_color_coord(t: f32, start_fade: f32, end_fade: f32) -> f32 {
    if t <= start_fade {
        0.0
    } else if t >= 1.0 - end_fade {
        1.0
    } else {
        ((t - start_fade) / (1.0 - start_fade - end_fade).max(f32::EPSILON)).clamp(0.0, 1.0)
    }
}

fn song_lua_overlay_vertex_color(
    state: SongLuaOverlayState,
    x: f32,
    y: f32,
    flip_x: bool,
    flip_y: bool,
    alpha: f32,
) -> [f32; 4] {
    let Some(colors) = state.vertex_colors else {
        return [1.0, 1.0, 1.0, alpha];
    };
    let x = if flip_x { 1.0 - x } else { x }.clamp(0.0, 1.0);
    let y = if flip_y { 1.0 - y } else { y }.clamp(0.0, 1.0);
    let mut out = [0.0; 4];
    for channel in 0..4 {
        let top = song_lua_effect_lerp(colors[0][channel], colors[1][channel], x);
        let bottom = song_lua_effect_lerp(colors[2][channel], colors[3][channel], x);
        out[channel] = song_lua_effect_lerp(top, bottom, y);
    }
    out[3] *= alpha;
    out
}

#[inline(always)]
fn song_lua_overlay_fold_xy_rot(
    mut flip_x: bool,
    mut flip_y: bool,
    mut size_x: f32,
    mut size_y: f32,
    rot_x_deg: f32,
    rot_y_deg: f32,
) -> (bool, bool, f32, f32) {
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
fn song_lua_overlay_local_transform(rot_deg: [f32; 3], skew_x: f32, skew_y: f32) -> Matrix4 {
    // RageMatrix uses row-vector storage for actor transforms. Its positive X/Y
    // rotations therefore map to negative glam angles; Z has the same sign.
    Matrix4::from_rotation_x(-rot_deg[0].to_radians())
        * Matrix4::from_rotation_y(-rot_deg[1].to_radians())
        * Matrix4::from_rotation_z(rot_deg[2].to_radians())
        * song_lua_player_skew_x_matrix(skew_x)
        * song_lua_player_skew_y_matrix(skew_y)
}

fn song_lua_projected_local_transform(view_proj: Matrix4, model: Matrix4) -> Matrix4 {
    let screen_projection = glam::camera::rh::proj::opengl::orthographic(
        0.0,
        screen_width(),
        screen_height(),
        0.0,
        -1.0,
        1.0,
    );
    screen_projection.inverse() * view_proj * model
}

fn append_projected_mesh_vertices(
    grid: &[TexturedMeshVertex],
    width: usize,
    height: usize,
    edge_fade: [f32; 4],
    vertices: &mut Vec<TexturedMeshVertex>,
) {
    vertices.reserve(width.saturating_sub(1) * height.saturating_sub(1) * 6);
    for y in 0..height.saturating_sub(1) {
        for x in 0..width.saturating_sub(1) {
            let x_fade_band = (x == 0 && edge_fade[0] > f32::EPSILON)
                || (x + 2 == width && edge_fade[1] > f32::EPSILON);
            let y_fade_band = (y == 0 && edge_fade[2] > f32::EPSILON)
                || (y + 2 == height && edge_fade[3] > f32::EPSILON);
            // RageSprite emits the center plus four edge-fade quads. It does
            // not draw diagonal corner cells where both axes are fading.
            if x_fade_band && y_fade_band {
                continue;
            }
            let tl = y * width + x;
            let tr = tl + 1;
            let bl = (y + 1) * width + x;
            let br = bl + 1;
            vertices
                .extend_from_slice(&[grid[tl], grid[tr], grid[br], grid[tl], grid[br], grid[bl]]);
        }
    }
}

struct SongLuaProjectedMeshParams {
    texture: Arc<str>,
    tint: [f32; 4],
    glow: [f32; 4],
    local_transform: Matrix4,
    world_z: f32,
    depth_test: bool,
    visible: bool,
    blend: BlendMode,
    z: i16,
}

fn song_lua_projected_mesh_actor_from_grid(
    params: SongLuaProjectedMeshParams,
    grid: &[TexturedMeshVertex],
    width: usize,
    height: usize,
    edge_fade: [f32; 4],
    scratch: Option<&mut SongLuaProjectedMeshScratch>,
) -> Actor {
    if let Some(scratch) = scratch {
        return Actor::ReusableTexturedMesh {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            world_z: params.world_z,
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            local_transform: params.local_transform,
            texture: params.texture,
            tint: params.tint,
            glow: params.glow,
            vertices: scratch.update_projected(grid, width, height, edge_fade),
            geom_cache_key: INVALID_TMESH_CACHE_KEY,
            uv_scale: [1.0, 1.0],
            uv_offset: [0.0, 0.0],
            uv_tex_shift: [0.0, 0.0],
            depth_test: params.depth_test,
            visible: params.visible,
            blend: params.blend,
            z: params.z,
        };
    }
    let mut vertices = Vec::with_capacity(
        width
            .saturating_sub(1)
            .saturating_mul(height.saturating_sub(1))
            .saturating_mul(6),
    );
    append_projected_mesh_vertices(grid, width, height, edge_fade, &mut vertices);
    Actor::TexturedMesh {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        world_z: params.world_z,
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        local_transform: params.local_transform,
        texture: params.texture,
        tint: params.tint,
        glow: params.glow,
        vertices: Arc::from(vertices.into_boxed_slice()),
        geom_cache_key: INVALID_TMESH_CACHE_KEY,
        uv_scale: [1.0, 1.0],
        uv_offset: [0.0, 0.0],
        uv_tex_shift: [0.0, 0.0],
        depth_test: params.depth_test,
        visible: params.visible,
        blend: params.blend,
        z: params.z,
    }
}

fn song_lua_flat_skewed_overlay_actor(
    texture: Arc<str>,
    tint: [f32; 4],
    blend: BlendMode,
    z: i16,
    center: [f32; 2],
    size: [f32; 2],
    rot_deg: [f32; 3],
    uv: [[f32; 2]; 4],
    state: SongLuaOverlayState,
    flip_x: bool,
    flip_y: bool,
    world_z: f32,
    scratch: Option<&mut SongLuaProjectedMeshScratch>,
) -> Option<Actor> {
    let (flip_x, flip_y, size_x, size_y) =
        song_lua_overlay_fold_xy_rot(flip_x, flip_y, size[0], size[1], rot_deg[0], rot_deg[1]);
    let half_w = 0.5 * size_x;
    let half_h = 0.5 * size_y;
    if half_w <= f32::EPSILON || half_h <= f32::EPSILON {
        return None;
    }
    let edge_fade = song_lua_projected_overlay_edge_fade(state, flip_x, flip_y);
    let xs = song_lua_projected_overlay_axis_slices(edge_fade[0], edge_fade[1]);
    let ys = song_lua_projected_overlay_axis_slices(edge_fade[2], edge_fade[3]);
    let transform = Matrix4::from_translation(Vector3::new(center[0], center[1], 0.0))
        * song_lua_overlay_local_transform(rot_deg, state.skew_x, state.skew_y);
    let mut grid = SmallVec::<[TexturedMeshVertex; 16]>::new();
    for &y in &ys {
        for &x in &xs {
            let local_x = song_lua_effect_lerp(-half_w, half_w, x);
            let local_y = song_lua_effect_lerp(-half_h, half_h, y);
            let point = transform * Vector4::new(local_x, local_y, 0.0, 1.0);
            let fade_x = song_lua_projected_edge_factor(x, edge_fade[0], edge_fade[1]);
            let fade_y = song_lua_projected_edge_factor(y, edge_fade[2], edge_fade[3]);
            let color_x = song_lua_projected_color_coord(x, edge_fade[0], edge_fade[1]);
            let color_y = song_lua_projected_color_coord(y, edge_fade[2], edge_fade[3]);
            grid.push(TexturedMeshVertex {
                pos: [point.x, point.y, 0.0],
                uv: song_lua_projected_overlay_uv_point(uv, x, y),
                tex_matrix_scale: [1.0, 1.0],
                color: song_lua_overlay_vertex_color(
                    state,
                    color_x,
                    color_y,
                    flip_x,
                    flip_y,
                    fade_x.min(fade_y),
                ),
            });
        }
    }
    Some(song_lua_projected_mesh_actor_from_grid(
        SongLuaProjectedMeshParams {
            local_transform: Matrix4::IDENTITY,
            world_z,
            depth_test: state.depth_test,
            visible: state.visible,
            glow: [1.0, 1.0, 1.0, 0.0],
            texture,
            tint,
            blend,
            z,
        },
        &grid,
        xs.len(),
        ys.len(),
        edge_fade,
        scratch,
    ))
}

fn song_lua_projected_overlay_actor(
    texture: Arc<str>,
    tint: [f32; 4],
    blend: BlendMode,
    z: i16,
    center: [f32; 3],
    size: [f32; 2],
    rot_deg: [f32; 3],
    uv: [[f32; 2]; 4],
    state: SongLuaOverlayState,
    flip_x: bool,
    flip_y: bool,
    view_proj: Matrix4,
    scratch: Option<&mut SongLuaProjectedMeshScratch>,
) -> Option<Actor> {
    let half_w = 0.5 * size[0];
    let half_h = 0.5 * size[1];
    if half_w <= f32::EPSILON || half_h <= f32::EPSILON {
        return None;
    }
    let edge_fade = song_lua_projected_overlay_edge_fade(state, flip_x, flip_y);
    let xs = song_lua_projected_overlay_axis_slices(edge_fade[0], edge_fade[1]);
    let ys = song_lua_projected_overlay_axis_slices(edge_fade[2], edge_fade[3]);
    let model = Matrix4::from_translation(Vector3::new(center[0], center[1], center[2]))
        * song_lua_overlay_local_transform(rot_deg, state.skew_x, state.skew_y);
    // Preserve homogeneous clip-space W until the GPU rasterizer.  ITGmania
    // submits the actor's world vertices under its perspective camera and lets
    // the GPU clip triangles which cross the near/camera planes.  Dividing by
    // W here made one behind-camera corner discard the whole sprite and also
    // forced affine texture interpolation across projected quads.
    let local_transform = song_lua_projected_local_transform(view_proj, model);
    let mut grid = SmallVec::<[TexturedMeshVertex; 16]>::new();
    for &y in &ys {
        for &x in &xs {
            let local_x = song_lua_effect_lerp(-half_w, half_w, x);
            let local_y = song_lua_effect_lerp(-half_h, half_h, y);
            let fade_x = song_lua_projected_edge_factor(x, edge_fade[0], edge_fade[1]);
            let fade_y = song_lua_projected_edge_factor(y, edge_fade[2], edge_fade[3]);
            let color_x = song_lua_projected_color_coord(x, edge_fade[0], edge_fade[1]);
            let color_y = song_lua_projected_color_coord(y, edge_fade[2], edge_fade[3]);
            grid.push(TexturedMeshVertex {
                pos: [local_x, local_y, 0.0],
                uv: song_lua_projected_overlay_uv_point(uv, x, y),
                tex_matrix_scale: [1.0, 1.0],
                color: song_lua_overlay_vertex_color(
                    state,
                    color_x,
                    color_y,
                    flip_x,
                    flip_y,
                    fade_x.min(fade_y),
                ),
            });
        }
    }
    Some(song_lua_projected_mesh_actor_from_grid(
        SongLuaProjectedMeshParams {
            local_transform,
            world_z: state.z_bias,
            depth_test: state.depth_test,
            visible: true,
            glow: [1.0, 1.0, 1.0, 0.0],
            texture,
            tint,
            blend,
            z,
        },
        &grid,
        xs.len(),
        ys.len(),
        edge_fade,
        scratch,
    ))
}

type SongLuaActorList = SmallVec<[Actor; 2]>;

#[allow(clippy::too_many_arguments)]
fn build_song_lua_aft_sprite_actor(
    state: SongLuaOverlayState,
    texture_handle: TextureHandle,
    texture_size: [f32; 2],
    z: i16,
    overlay_space_width: f32,
    overlay_space_height: f32,
    effect_time: f32,
    effect_beat: f32,
    total_elapsed: f32,
    projected_mesh_scratch: Option<&mut SongLuaProjectedMeshScratch>,
) -> Option<SongLuaActorList> {
    if !state.visible || !song_lua_overlay_has_visible_output(state) {
        return None;
    }
    let x_scale = screen_width() / overlay_space_width.max(1.0);
    let y_scale = screen_height() / overlay_space_height.max(1.0);
    let axis_scale = song_lua_overlay_axis_scale(state);
    let (size_scale_x, flip_x) = if axis_scale[0] < 0.0 {
        (-axis_scale[0], true)
    } else {
        (axis_scale[0], false)
    };
    let (size_scale_y, flip_y) = if axis_scale[1] < 0.0 {
        (-axis_scale[1], true)
    } else {
        (axis_scale[1], false)
    };
    let (align, mut offset, size) = if let Some([left, top, right, bottom]) = state.stretch_rect {
        (
            [0.0, 0.0],
            [left * x_scale, top * y_scale],
            [
                (right - left).abs() * x_scale * size_scale_x,
                (bottom - top).abs() * y_scale * size_scale_y,
            ],
        )
    } else {
        (
            [state.halign, state.valign],
            [state.x * x_scale, state.y * y_scale],
            [
                texture_size[0] * x_scale * size_scale_x,
                texture_size[1] * y_scale * size_scale_y,
            ],
        )
    };
    if size[0] <= f32::EPSILON || size[1] <= f32::EPSILON {
        return None;
    }
    let mut tint = state.diffuse;
    let mut glow = state.glow;
    let mut effect_offset = [0.0, 0.0, 0.0];
    let mut effect_scale = [1.0, 1.0, 1.0];
    let mut effect_rot = [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg];
    song_lua_apply_overlay_effect(
        song_lua_overlay_effect_state(state),
        state.rainbow,
        song_lua_overlay_vibrate_magnitude(state),
        effect_time,
        effect_beat,
        z as u32,
        &mut tint,
        &mut glow,
        &mut effect_offset,
        &mut effect_scale,
        &mut effect_rot,
    );
    offset[0] = effect_offset[0].mul_add(x_scale, offset[0]);
    offset[1] = effect_offset[1].mul_add(y_scale, offset[1]);
    let actor = Actor::Sprite {
        align,
        offset,
        // This sprite uses the main pass's normalized depth. ITG's menu
        // camera spans ±1000, so a ten-pixel RGB vibration must not clip it.
        world_z: song_lua_biased_world_z(state, effect_offset[2] / 1000.0),
        size: [SizeSpec::Px(size[0]), SizeSpec::Px(size[1])],
        source: SpriteSource::RenderTarget {
            handle: render_target_sample_handle(texture_handle, !state.texture_filtering),
            size: texture_size,
        },
        tint,
        glow,
        z,
        cell: None,
        grid: None,
        uv_rect: song_lua_overlay_uv_rect(state, None, &[], total_elapsed),
        visible: state.visible,
        flip_x,
        flip_y,
        cropleft: state.cropleft.clamp(0.0, 1.0),
        cropright: state.cropright.clamp(0.0, 1.0),
        croptop: state.croptop.clamp(0.0, 1.0),
        cropbottom: state.cropbottom.clamp(0.0, 1.0),
        fadeleft: state.fadeleft.clamp(0.0, 1.0),
        faderight: state.faderight.clamp(0.0, 1.0),
        fadetop: state.fadetop.clamp(0.0, 1.0),
        fadebottom: state.fadebottom.clamp(0.0, 1.0),
        blend: song_lua_overlay_blend(state.blend),
        mask_source: state.mask_source,
        mask_dest: state.mask_dest,
        rot_x_deg: effect_rot[0],
        rot_y_deg: effect_rot[1],
        // ITG actors are authored in screen space (Y down), while the AFT
        // sprite is emitted in render space (Y up). Reflect the in-plane
        // transform so the strip's authored position compensation cancels
        // its skew instead of doubling it.
        rot_z_deg: -effect_rot[2],
        skew: [-state.skew_x, -state.skew_y],
        local_offset: [0.0, 0.0],
        local_offset_rot_sin_cos: [0.0, 1.0],
        texcoordvelocity: state.texcoord_velocity,
        animate: false,
        state_delay: 0.0,
        scale: [effect_scale[0], effect_scale[1]],
        shadow_len: state.shadow_len,
        shadow_color: state.shadow_color,
        effect: deadlib_present::anim::EffectState::default(),
    };
    Some(song_lua_finalize_overlay_actor(
        state,
        actor,
        glow,
        x_scale,
        y_scale,
        projected_mesh_scratch,
    ))
}

/// Emits compiled multi-output overlays directly into the caller's reused
/// frame buffer. Returning `None` means the overlay is a single-output kind and
/// should continue through the compact inline builder.
#[allow(clippy::too_many_arguments)]
fn append_song_lua_multi_actor_overlay<S: NoteskinSlot + Clone>(
    out: &mut Vec<Actor>,
    overlay: &SongLuaOverlayActor<S>,
    state: SongLuaOverlayState,
    asset_manager: &AssetManager,
    z: i16,
    overlay_space_width: f32,
    overlay_space_height: f32,
    effect_time: f32,
    effect_beat: f32,
    total_elapsed: f32,
    scratch: Option<&mut SongLuaProjectedMeshScratch>,
) -> Option<bool> {
    if !matches!(
        overlay.kind,
        SongLuaOverlayKind::Model { .. } | SongLuaOverlayKind::NoteskinActor { .. }
    ) {
        return None;
    }
    if !state.visible || !song_lua_overlay_has_visible_output(state) {
        return Some(false);
    }

    let x_scale = screen_width() / overlay_space_width.max(1.0);
    let y_scale = screen_height() / overlay_space_height.max(1.0);
    let overlay_scale = song_lua_overlay_axis_scale(state);
    if overlay_scale
        .iter()
        .any(|scale| !scale.is_finite() || scale.abs() <= f32::EPSILON)
    {
        return Some(false);
    }
    let actor_scale = [overlay_scale[0].abs(), overlay_scale[1].abs()];
    let effect = song_lua_overlay_effect_state(state);
    let mut tint = state.diffuse;
    let mut glow = state.glow;
    let mut effect_offset = [0.0, 0.0, 0.0];
    let mut effect_scale = [1.0, 1.0, 1.0];
    let mut effect_rot = [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg];
    song_lua_apply_overlay_effect(
        effect,
        state.rainbow,
        song_lua_overlay_vibrate_magnitude(state),
        effect_time,
        effect_beat,
        z as u32,
        &mut tint,
        &mut glow,
        &mut effect_offset,
        &mut effect_scale,
        &mut effect_rot,
    );
    let blend = song_lua_overlay_blend(state.blend);

    Some(match &overlay.kind {
        SongLuaOverlayKind::Model { layers } => {
            let (geometry_keys, glow_vertices) =
                scratch.as_deref().map_or((None, None), |scratch| {
                    (
                        scratch.model_geometry_keys.as_deref(),
                        scratch.model_glow_vertices.as_deref(),
                    )
                });
            append_song_lua_model_actors(
                out,
                layers,
                state,
                asset_manager,
                z,
                x_scale,
                y_scale,
                actor_scale,
                effect_scale,
                effect_rot,
                effect_offset,
                tint,
                glow,
                blend,
                total_elapsed,
                geometry_keys,
                glow_vertices,
            )
        }
        SongLuaOverlayKind::NoteskinActor { slots } => append_song_lua_noteskin_actors(
            out,
            slots,
            state,
            asset_manager,
            z,
            x_scale,
            y_scale,
            actor_scale,
            effect_scale,
            effect_rot,
            effect_offset,
            tint,
            glow,
            blend,
            total_elapsed,
            effect_beat,
            scratch,
        ),
        _ => false,
    })
}

fn build_song_lua_overlay_actor_with_scratch<S: NoteskinSlot + Clone>(
    overlay: &SongLuaOverlayActor<S>,
    state: SongLuaOverlayState,
    camera_state: Option<SongLuaOverlayState>,
    asset_manager: &AssetManager,
    z: i16,
    overlay_space_width: f32,
    overlay_space_height: f32,
    effect_time: f32,
    effect_beat: f32,
    total_elapsed: f32,
    mut projected_mesh_scratch: Option<&mut SongLuaProjectedMeshScratch>,
) -> Option<SongLuaActorList> {
    if !state.visible || !song_lua_overlay_has_visible_output(state) {
        return None;
    }
    let x_scale = screen_width() / overlay_space_width.max(1.0);
    let y_scale = screen_height() / overlay_space_height.max(1.0);
    let overlay_scale = song_lua_overlay_axis_scale(state);
    // A zero zoom has no geometry in ITGmania. Do not pass a 0x0 sprite to
    // `deadlib-present`: generic sprites use 0x0 as the sentinel for native
    // texture size, which would resurrect a hidden sprite sheet at full size.
    if overlay_scale
        .iter()
        .any(|scale| !scale.is_finite() || scale.abs() <= f32::EPSILON)
    {
        return None;
    }
    let (size_scale_x, flip_x) = if overlay_scale[0] < 0.0 {
        (-overlay_scale[0], true)
    } else {
        (overlay_scale[0], false)
    };
    let (size_scale_y, flip_y) = if overlay_scale[1] < 0.0 {
        (-overlay_scale[1], true)
    } else {
        (overlay_scale[1], false)
    };
    let effect = song_lua_overlay_effect_state(state);
    let overlay_blend = song_lua_overlay_blend(state.blend);
    let perspective_view_proj = camera_state.and_then(|camera| {
        song_lua_overlay_view_proj(camera, overlay_space_width, overlay_space_height)
    });
    let finalize_actor = |actor, glow, scratch| {
        song_lua_finalize_overlay_actor(state, actor, glow, x_scale, y_scale, scratch)
    };
    match &overlay.kind {
        SongLuaOverlayKind::Actor => None,
        SongLuaOverlayKind::ActorFrame => None,
        SongLuaOverlayKind::UpdateTracks { .. } => None,
        SongLuaOverlayKind::ActorFrameTexture { .. } => None,
        SongLuaOverlayKind::ActorProxy { .. } => None,
        SongLuaOverlayKind::AftSprite { .. } => None,
        SongLuaOverlayKind::Sound { .. } => None,
        SongLuaOverlayKind::Sprite {
            texture_key,
            states,
            ..
        } => {
            let textures = asset_manager.texture_context();
            let binding = match projected_mesh_scratch.as_deref_mut() {
                Some(scratch) => scratch.bind_sprite(texture_key, textures),
                None => textures.bind_texture(texture_key),
            }?;
            let animation_elapsed = state
                .sprite_animation_epoch
                .map_or(total_elapsed, |epoch| (effect_time - epoch).max(0.0));
            let source_size = song_lua_overlay_sprite_size(state, binding)?;
            if camera_state.is_none()
                && state.stretch_rect.is_none()
                && song_lua_overlay_sprite_offscreen(
                    state,
                    source_size,
                    overlay_space_width,
                    overlay_space_height,
                )
            {
                return None;
            }
            if let Some(view_proj) = perspective_view_proj {
                let (center, size) = song_lua_overlay_rect(
                    state,
                    source_size,
                    x_scale,
                    y_scale,
                    size_scale_x,
                    size_scale_y,
                )?;
                let mut tint = state.diffuse;
                let mut glow = state.glow;
                let mut effect_offset = [0.0, 0.0, 0.0];
                let mut effect_scale = [1.0, 1.0, 1.0];
                let mut rot_deg = [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg];
                song_lua_apply_overlay_effect(
                    effect,
                    state.rainbow,
                    song_lua_overlay_vibrate_magnitude(state),
                    effect_time,
                    effect_beat,
                    z as u32,
                    &mut tint,
                    &mut glow,
                    &mut effect_offset,
                    &mut effect_scale,
                    &mut rot_deg,
                );
                let actor = song_lua_projected_overlay_actor(
                    Arc::clone(texture_key),
                    tint,
                    overlay_blend,
                    z,
                    [
                        effect_offset[0].mul_add(x_scale, center[0]),
                        effect_offset[1].mul_add(y_scale, center[1]),
                        effect_offset[2],
                    ],
                    [size[0] * effect_scale[0], size[1] * effect_scale[1]],
                    rot_deg,
                    song_lua_overlay_uvs(
                        state,
                        Some(binding.sheet),
                        states,
                        flip_x,
                        flip_y,
                        animation_elapsed,
                        total_elapsed,
                    ),
                    state,
                    flip_x,
                    flip_y,
                    view_proj,
                    projected_mesh_scratch.as_deref_mut(),
                )?;
                return Some(finalize_actor(
                    actor,
                    glow,
                    projected_mesh_scratch.as_deref_mut(),
                ));
            }
            if (state.skew_x.abs() > f32::EPSILON
                || state.skew_y.abs() > f32::EPSILON
                || state.vertex_colors.is_some())
                && !state.mask_source
                && !state.mask_dest
            {
                let (center, size) = song_lua_overlay_rect(
                    state,
                    source_size,
                    x_scale,
                    y_scale,
                    size_scale_x,
                    size_scale_y,
                )?;
                let mut tint = state.diffuse;
                let mut glow = state.glow;
                let mut effect_offset = [0.0, 0.0, 0.0];
                let mut effect_scale = [1.0, 1.0, 1.0];
                let mut rot_deg = [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg];
                song_lua_apply_overlay_effect(
                    effect,
                    state.rainbow,
                    song_lua_overlay_vibrate_magnitude(state),
                    effect_time,
                    effect_beat,
                    z as u32,
                    &mut tint,
                    &mut glow,
                    &mut effect_offset,
                    &mut effect_scale,
                    &mut rot_deg,
                );
                let actor = song_lua_flat_skewed_overlay_actor(
                    Arc::clone(texture_key),
                    tint,
                    overlay_blend,
                    z,
                    [
                        effect_offset[0].mul_add(x_scale, center[0]),
                        effect_offset[1].mul_add(y_scale, center[1]),
                    ],
                    [size[0] * effect_scale[0], size[1] * effect_scale[1]],
                    rot_deg,
                    song_lua_overlay_uvs(
                        state,
                        Some(binding.sheet),
                        states,
                        flip_x,
                        flip_y,
                        animation_elapsed,
                        total_elapsed,
                    ),
                    state,
                    flip_x,
                    flip_y,
                    effect_offset[2],
                    projected_mesh_scratch.as_deref_mut(),
                )?;
                return Some(finalize_actor(
                    actor,
                    glow,
                    projected_mesh_scratch.as_deref_mut(),
                ));
            }
            let source = deadlib_present::actors::TextureKeyHandle {
                key: Arc::clone(texture_key),
                handle: binding.handle,
                generation: textures.revision(),
            };
            let mut actor = if let Some([left, top, right, bottom]) = state.stretch_rect {
                deadlib_present::__act_from_builder!((
                    align(0.0, 0.0):
                    xy(left * x_scale, top * y_scale):
                    setsize(
                        (right - left).abs() * x_scale * size_scale_x,
                        (bottom - top).abs() * y_scale * size_scale_y
                    ):
                    z(z)
                ) deadlib_assets::SpriteBuilder::texture(source))
            } else {
                deadlib_present::__act_from_builder!((
                    align(state.halign, state.valign):
                    xy(state.x * x_scale, state.y * y_scale):
                    setsize(
                        source_size[0] * x_scale * size_scale_x,
                        source_size[1] * y_scale * size_scale_y
                    ):
                    z(z)
                ) deadlib_assets::SpriteBuilder::texture(source))
            };
            if let Actor::Sprite {
                tint,
                glow,
                cropleft,
                cropright,
                croptop,
                cropbottom,
                fadeleft,
                faderight,
                fadetop,
                fadebottom,
                blend,
                mask_source,
                mask_dest,
                rot_x_deg,
                rot_y_deg,
                rot_z_deg,
                offset,
                world_z,
                scale,
                uv_rect,
                texcoordvelocity,
                effect: actor_effect,
                flip_x: actor_flip_x,
                flip_y: actor_flip_y,
                visible,
                ..
            } = &mut actor
            {
                let mut effect_tint = state.diffuse;
                let mut effect_glow = state.glow;
                let mut effect_offset = [0.0, 0.0, 0.0];
                let mut effect_scale = [1.0, 1.0, 1.0];
                let mut effect_rot = [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg];
                song_lua_apply_overlay_effect(
                    effect,
                    state.rainbow,
                    song_lua_overlay_vibrate_magnitude(state),
                    effect_time,
                    effect_beat,
                    z as u32,
                    &mut effect_tint,
                    &mut effect_glow,
                    &mut effect_offset,
                    &mut effect_scale,
                    &mut effect_rot,
                );
                *tint = effect_tint;
                *glow = effect_glow;
                *cropleft = state.cropleft.clamp(0.0, 1.0);
                *cropright = state.cropright.clamp(0.0, 1.0);
                *croptop = state.croptop.clamp(0.0, 1.0);
                *cropbottom = state.cropbottom.clamp(0.0, 1.0);
                *fadeleft = state.fadeleft.clamp(0.0, 1.0);
                *faderight = state.faderight.clamp(0.0, 1.0);
                *fadetop = state.fadetop.clamp(0.0, 1.0);
                *fadebottom = state.fadebottom.clamp(0.0, 1.0);
                *blend = overlay_blend;
                *mask_source = state.mask_source;
                *mask_dest = state.mask_dest;
                *rot_x_deg = effect_rot[0];
                *rot_y_deg = effect_rot[1];
                *rot_z_deg = effect_rot[2];
                offset[0] = effect_offset[0].mul_add(x_scale, offset[0]);
                offset[1] = effect_offset[1].mul_add(y_scale, offset[1]);
                *world_z += song_lua_biased_world_z(state, effect_offset[2]);
                scale[0] *= effect_scale[0];
                scale[1] *= effect_scale[1];
                *uv_rect =
                    song_lua_overlay_uv_rect(state, Some(binding.sheet), states, animation_elapsed);
                *texcoordvelocity = state.texcoord_velocity;
                *actor_effect = deadlib_present::anim::EffectState::default();
                *actor_flip_x ^= flip_x;
                *actor_flip_y ^= flip_y;
                *visible = state.visible;
            }
            let glow = if let Actor::Sprite { glow, .. } = &actor {
                *glow
            } else {
                state.glow
            };
            Some(finalize_actor(
                actor,
                glow,
                projected_mesh_scratch.as_deref_mut(),
            ))
        }
        SongLuaOverlayKind::BitmapText {
            font_name,
            text,
            text_changes,
            stroke_color,
            attributes,
            ..
        } => {
            let text = crate::overlay_text_at(text, text_changes, effect_beat);
            let text_index = text_changes
                .partition_point(|(beat, _)| *beat <= effect_beat)
                .checked_sub(1);
            let content = if state.uppercase {
                projected_mesh_scratch
                    .as_deref()
                    .and_then(|scratch| {
                        text_index.map_or_else(
                            || scratch.uppercase_text.as_ref(),
                            |index| scratch.uppercase_changes.get(index),
                        )
                    })
                    .map_or_else(
                        || TextContent::from(text.to_uppercase()),
                        |uppercase| TextContent::from(Arc::clone(uppercase)),
                    )
            } else {
                TextContent::from(text)
            };
            let font = if asset_manager.with_font(font_name, |_| ()).is_some() {
                *font_name
            } else {
                "miso"
            };
            let mut color = state.diffuse;
            let mut glow = state.glow;
            let mut effect_offset = [0.0, 0.0, 0.0];
            let mut effect_scale = [1.0, 1.0, 1.0];
            let mut effect_rot = [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg];
            song_lua_apply_overlay_effect(
                effect,
                state.rainbow,
                song_lua_overlay_vibrate_magnitude(state),
                effect_time,
                effect_beat,
                z as u32,
                &mut color,
                &mut glow,
                &mut effect_offset,
                &mut effect_scale,
                &mut effect_rot,
            );
            let (text_attributes, color) = if state.rainbow_scroll {
                let attributes = projected_mesh_scratch.as_deref_mut().map_or_else(
                    || song_lua_rainbow_scroll_attributes(content.as_str(), total_elapsed).into(),
                    |scratch| scratch.rainbow_attributes(content.as_str(), total_elapsed),
                );
                (attributes, color)
            } else {
                song_lua_text_attributes_for_diffuse_mode(
                    attributes,
                    color,
                    content.as_str(),
                    state.mult_attrs_with_diffuse,
                    projected_mesh_scratch.as_deref_mut(),
                )
            };
            let actor = Actor::Text {
                align: [state.halign, state.valign],
                offset: [
                    effect_offset[0].mul_add(x_scale, state.x * x_scale),
                    effect_offset[1].mul_add(y_scale, state.y * y_scale),
                ],
                local_transform: song_lua_overlay_local_transform(
                    effect_rot,
                    state.skew_x,
                    state.skew_y,
                ),
                color,
                stroke_color: *stroke_color,
                glow,
                font,
                content,
                attributes: text_attributes,
                align_text: state.text_align,
                z,
                scale: [
                    size_scale_x * x_scale * effect_scale[0],
                    size_scale_y * y_scale * effect_scale[1],
                ],
                fit_width: state.size.map(|size| size[0] * x_scale),
                fit_height: state.size.map(|size| size[1] * y_scale),
                line_spacing: state
                    .vert_spacing
                    .map(|value| ((value as f32) * y_scale).round() as i32),
                wrap_width_pixels: state
                    .wrap_width_pixels
                    .map(|value| ((value as f32) * x_scale).round() as i32),
                max_width: state.max_width.map(|value| value * x_scale),
                max_height: state.max_height.map(|value| value * y_scale),
                max_w_pre_zoom: state.max_w_pre_zoom && !state.max_dimension_uses_zoom,
                max_h_pre_zoom: state.max_h_pre_zoom && !state.max_dimension_uses_zoom,
                jitter: state.text_jitter,
                distortion: state.text_distortion,
                clip: None,
                mask_dest: state.mask_dest,
                blend: overlay_blend,
                shadow_len: [0.0, 0.0],
                shadow_color: [0.0, 0.0, 0.0, 0.5],
                effect: deadlib_present::anim::EffectState::default(),
            };
            Some(finalize_actor(
                actor,
                glow,
                projected_mesh_scratch.as_deref_mut(),
            ))
        }
        SongLuaOverlayKind::ActorMultiVertex {
            vertices,
            texture_key,
            ..
        } => {
            let mut tint = state.diffuse;
            let mut glow = state.glow;
            let mut effect_offset = [0.0, 0.0, 0.0];
            let mut effect_scale = [1.0, 1.0, 1.0];
            let mut effect_rot = [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg];
            song_lua_apply_overlay_effect(
                effect,
                state.rainbow,
                song_lua_overlay_vibrate_magnitude(state),
                effect_time,
                effect_beat,
                z as u32,
                &mut tint,
                &mut glow,
                &mut effect_offset,
                &mut effect_scale,
                &mut effect_rot,
            );
            if let Some(texture_key) = texture_key {
                let key = texture_key.as_ref();
                if !asset_manager.has_texture_key(key) {
                    return None;
                }
                let mesh_actor = if let Some(scratch) = projected_mesh_scratch.as_deref_mut() {
                    let mesh = scratch.update_textured(|out| {
                        append_song_lua_actor_multi_vertex_textured_mesh(
                            out,
                            vertices,
                            x_scale,
                            y_scale,
                            [size_scale_x, size_scale_y],
                            effect_scale,
                            effect_rot[2],
                            [state.skew_x, state.skew_y],
                        );
                    });
                    Actor::ReusableTexturedMesh {
                        align: [0.0, 0.0],
                        offset: [
                            effect_offset[0].mul_add(x_scale, state.x * x_scale),
                            effect_offset[1].mul_add(y_scale, state.y * y_scale),
                        ],
                        world_z: song_lua_biased_world_z(state, effect_offset[2]),
                        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
                        local_transform: Matrix4::IDENTITY,
                        texture: Arc::clone(texture_key),
                        tint,
                        glow: [1.0, 1.0, 1.0, 0.0],
                        vertices: mesh,
                        geom_cache_key: INVALID_TMESH_CACHE_KEY,
                        uv_scale: [1.0, 1.0],
                        uv_offset: [0.0, 0.0],
                        uv_tex_shift: [0.0, 0.0],
                        depth_test: state.depth_test,
                        visible: state.visible,
                        blend: overlay_blend,
                        z,
                    }
                } else {
                    let mesh = song_lua_actor_multi_vertex_textured_mesh(
                        vertices,
                        x_scale,
                        y_scale,
                        [size_scale_x, size_scale_y],
                        effect_scale,
                        effect_rot[2],
                        [state.skew_x, state.skew_y],
                    );
                    Actor::TexturedMesh {
                        align: [0.0, 0.0],
                        offset: [
                            effect_offset[0].mul_add(x_scale, state.x * x_scale),
                            effect_offset[1].mul_add(y_scale, state.y * y_scale),
                        ],
                        world_z: song_lua_biased_world_z(state, effect_offset[2]),
                        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
                        local_transform: Matrix4::IDENTITY,
                        texture: Arc::clone(texture_key),
                        tint,
                        glow: [1.0, 1.0, 1.0, 0.0],
                        vertices: mesh,
                        geom_cache_key: INVALID_TMESH_CACHE_KEY,
                        uv_scale: [1.0, 1.0],
                        uv_offset: [0.0, 0.0],
                        uv_tex_shift: [0.0, 0.0],
                        depth_test: state.depth_test,
                        visible: state.visible,
                        blend: overlay_blend,
                        z,
                    }
                };
                return Some(finalize_actor(
                    mesh_actor,
                    glow,
                    projected_mesh_scratch.as_deref_mut(),
                ));
            }
            let mesh_actor = if let Some(scratch) = projected_mesh_scratch.as_deref_mut() {
                let mesh = scratch.update_mesh(|out| {
                    append_song_lua_actor_multi_vertex_mesh(
                        out,
                        vertices,
                        tint,
                        x_scale,
                        y_scale,
                        [size_scale_x, size_scale_y],
                        effect_scale,
                        effect_rot[2],
                        [state.skew_x, state.skew_y],
                    );
                });
                Actor::ReusableMesh {
                    align: [0.0, 0.0],
                    offset: [
                        effect_offset[0].mul_add(x_scale, state.x * x_scale),
                        effect_offset[1].mul_add(y_scale, state.y * y_scale),
                    ],
                    size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
                    tint: [1.0; 4],
                    vertices: mesh,
                    visible: state.visible,
                    blend: overlay_blend,
                    z,
                }
            } else {
                let mesh = song_lua_actor_multi_vertex_mesh(
                    vertices,
                    tint,
                    x_scale,
                    y_scale,
                    [size_scale_x, size_scale_y],
                    effect_scale,
                    effect_rot[2],
                    [state.skew_x, state.skew_y],
                );
                Actor::Mesh {
                    align: [0.0, 0.0],
                    offset: [
                        effect_offset[0].mul_add(x_scale, state.x * x_scale),
                        effect_offset[1].mul_add(y_scale, state.y * y_scale),
                    ],
                    size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
                    tint: [1.0; 4],
                    vertices: mesh,
                    visible: state.visible,
                    blend: overlay_blend,
                    z,
                }
            };
            Some(finalize_actor(
                mesh_actor,
                glow,
                projected_mesh_scratch.as_deref_mut(),
            ))
        }
        SongLuaOverlayKind::Model { layers } => {
            let mut tint = state.diffuse;
            let mut glow = state.glow;
            let mut effect_offset = [0.0, 0.0, 0.0];
            let mut effect_scale = [1.0, 1.0, 1.0];
            let mut effect_rot = [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg];
            song_lua_apply_overlay_effect(
                effect,
                state.rainbow,
                song_lua_overlay_vibrate_magnitude(state),
                effect_time,
                effect_beat,
                z as u32,
                &mut tint,
                &mut glow,
                &mut effect_offset,
                &mut effect_scale,
                &mut effect_rot,
            );
            let mut out = SongLuaActorList::new();
            let (geometry_keys, glow_vertices) =
                projected_mesh_scratch
                    .as_deref()
                    .map_or((None, None), |scratch| {
                        (
                            scratch.model_geometry_keys.as_deref(),
                            scratch.model_glow_vertices.as_deref(),
                        )
                    });
            append_song_lua_model_actors(
                &mut out,
                layers,
                state,
                asset_manager,
                z,
                x_scale,
                y_scale,
                [size_scale_x, size_scale_y],
                effect_scale,
                effect_rot,
                effect_offset,
                tint,
                glow,
                overlay_blend,
                total_elapsed,
                geometry_keys,
                glow_vertices,
            )
            .then_some(out)
        }
        SongLuaOverlayKind::NoteskinActor { slots } => {
            let mut tint = state.diffuse;
            let mut glow = state.glow;
            let mut effect_offset = [0.0, 0.0, 0.0];
            let mut effect_scale = [1.0, 1.0, 1.0];
            let mut effect_rot = [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg];
            song_lua_apply_overlay_effect(
                effect,
                state.rainbow,
                song_lua_overlay_vibrate_magnitude(state),
                effect_time,
                effect_beat,
                z as u32,
                &mut tint,
                &mut glow,
                &mut effect_offset,
                &mut effect_scale,
                &mut effect_rot,
            );
            let mut out = SongLuaActorList::new();
            append_song_lua_noteskin_actors(
                &mut out,
                slots,
                state,
                asset_manager,
                z,
                x_scale,
                y_scale,
                [size_scale_x, size_scale_y],
                effect_scale,
                effect_rot,
                effect_offset,
                tint,
                glow,
                overlay_blend,
                total_elapsed,
                effect_beat,
                projected_mesh_scratch.as_deref_mut(),
            )
            .then_some(out)
        }
        SongLuaOverlayKind::SongMeterDisplay {
            stream_width,
            stream_state,
            music_length_seconds,
        } => {
            let actor = song_lua_song_meter_actor(
                state,
                *stream_state,
                *stream_width,
                *music_length_seconds,
                x_scale,
                y_scale,
                z,
                total_elapsed,
            )?;
            let glow = [
                state.glow[0] + stream_state.glow[0],
                state.glow[1] + stream_state.glow[1],
                state.glow[2] + stream_state.glow[2],
                state.glow[3].max(stream_state.glow[3]),
            ];
            Some(finalize_actor(
                actor,
                glow,
                projected_mesh_scratch.as_deref_mut(),
            ))
        }
        SongLuaOverlayKind::GraphDisplay {
            size,
            body_values,
            body_state,
            line_state,
        } => {
            let actor = song_lua_graph_display_actor(
                state,
                body_values,
                *body_state,
                **line_state,
                *size,
                x_scale,
                y_scale,
                z,
                projected_mesh_scratch.as_deref_mut(),
            )?;
            let glow = [
                state.glow[0] + body_state.glow[0].max(line_state.glow[0]),
                state.glow[1] + body_state.glow[1].max(line_state.glow[1]),
                state.glow[2] + body_state.glow[2].max(line_state.glow[2]),
                state.glow[3].max(body_state.glow[3].max(line_state.glow[3])),
            ];
            Some(finalize_actor(
                actor,
                glow,
                projected_mesh_scratch.as_deref_mut(),
            ))
        }
        SongLuaOverlayKind::Quad => {
            if let Some(view_proj) = perspective_view_proj {
                let (center, size) = song_lua_overlay_rect(
                    state,
                    state.size.unwrap_or([1.0, 1.0]),
                    x_scale,
                    y_scale,
                    size_scale_x,
                    size_scale_y,
                )?;
                let mut tint = state.diffuse;
                let mut glow = state.glow;
                let mut effect_offset = [0.0, 0.0, 0.0];
                let mut effect_scale = [1.0, 1.0, 1.0];
                let mut rot_deg = [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg];
                song_lua_apply_overlay_effect(
                    effect,
                    state.rainbow,
                    song_lua_overlay_vibrate_magnitude(state),
                    effect_time,
                    effect_beat,
                    z as u32,
                    &mut tint,
                    &mut glow,
                    &mut effect_offset,
                    &mut effect_scale,
                    &mut rot_deg,
                );
                let actor = song_lua_projected_overlay_actor(
                    white_texture_key(),
                    tint,
                    overlay_blend,
                    z,
                    [
                        effect_offset[0].mul_add(x_scale, center[0]),
                        effect_offset[1].mul_add(y_scale, center[1]),
                        effect_offset[2],
                    ],
                    [size[0] * effect_scale[0], size[1] * effect_scale[1]],
                    rot_deg,
                    song_lua_overlay_uvs(
                        state,
                        None,
                        &[],
                        flip_x,
                        flip_y,
                        total_elapsed,
                        total_elapsed,
                    ),
                    state,
                    flip_x,
                    flip_y,
                    view_proj,
                    projected_mesh_scratch.as_deref_mut(),
                )?;
                return Some(finalize_actor(
                    actor,
                    glow,
                    projected_mesh_scratch.as_deref_mut(),
                ));
            }
            if (state.skew_x.abs() > f32::EPSILON
                || state.skew_y.abs() > f32::EPSILON
                || state.vertex_colors.is_some())
                && !state.mask_source
                && !state.mask_dest
            {
                let (center, size) = song_lua_overlay_rect(
                    state,
                    state.size.unwrap_or([1.0, 1.0]),
                    x_scale,
                    y_scale,
                    size_scale_x,
                    size_scale_y,
                )?;
                let mut tint = state.diffuse;
                let mut glow = state.glow;
                let mut effect_offset = [0.0, 0.0, 0.0];
                let mut effect_scale = [1.0, 1.0, 1.0];
                let mut rot_deg = [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg];
                song_lua_apply_overlay_effect(
                    effect,
                    state.rainbow,
                    song_lua_overlay_vibrate_magnitude(state),
                    effect_time,
                    effect_beat,
                    z as u32,
                    &mut tint,
                    &mut glow,
                    &mut effect_offset,
                    &mut effect_scale,
                    &mut rot_deg,
                );
                let actor = song_lua_flat_skewed_overlay_actor(
                    white_texture_key(),
                    tint,
                    overlay_blend,
                    z,
                    [
                        effect_offset[0].mul_add(x_scale, center[0]),
                        effect_offset[1].mul_add(y_scale, center[1]),
                    ],
                    [size[0] * effect_scale[0], size[1] * effect_scale[1]],
                    rot_deg,
                    song_lua_overlay_uvs(
                        state,
                        None,
                        &[],
                        flip_x,
                        flip_y,
                        total_elapsed,
                        total_elapsed,
                    ),
                    state,
                    flip_x,
                    flip_y,
                    effect_offset[2],
                    projected_mesh_scratch.as_deref_mut(),
                )?;
                return Some(finalize_actor(
                    actor,
                    glow,
                    projected_mesh_scratch.as_deref_mut(),
                ));
            }
            let mut actor = if let Some([left, top, right, bottom]) = state.stretch_rect {
                deadlib_present::__act_from_builder!((
                    align(0.0, 0.0):
                    xy(left * x_scale, top * y_scale):
                    zoomto(
                        (right - left).abs() * x_scale * size_scale_x,
                        (bottom - top).abs() * y_scale * size_scale_y
                    ):
                    diffuse(state.diffuse[0], state.diffuse[1], state.diffuse[2], state.diffuse[3]):
                    z(z)
                ) deadlib_assets::SpriteBuilder::solid())
            } else {
                let size = state.size.unwrap_or([1.0, 1.0]);
                deadlib_present::__act_from_builder!((
                    align(state.halign, state.valign):
                    xy(state.x * x_scale, state.y * y_scale):
                    zoomto(
                        size[0] * x_scale * size_scale_x,
                        size[1] * y_scale * size_scale_y
                    ):
                    diffuse(state.diffuse[0], state.diffuse[1], state.diffuse[2], state.diffuse[3]):
                    z(z)
                ) deadlib_assets::SpriteBuilder::solid())
            };
            if let Actor::Sprite {
                visible,
                tint,
                glow,
                cropleft,
                cropright,
                croptop,
                cropbottom,
                fadeleft,
                faderight,
                fadetop,
                fadebottom,
                blend,
                mask_source,
                mask_dest,
                rot_x_deg,
                rot_y_deg,
                rot_z_deg,
                offset,
                world_z,
                scale,
                effect: actor_effect,
                flip_x: actor_flip_x,
                flip_y: actor_flip_y,
                ..
            } = &mut actor
            {
                let mut effect_tint = state.diffuse;
                let mut effect_glow = state.glow;
                let mut effect_offset = [0.0, 0.0, 0.0];
                let mut effect_scale = [1.0, 1.0, 1.0];
                let mut effect_rot = [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg];
                song_lua_apply_overlay_effect(
                    effect,
                    state.rainbow,
                    song_lua_overlay_vibrate_magnitude(state),
                    effect_time,
                    effect_beat,
                    z as u32,
                    &mut effect_tint,
                    &mut effect_glow,
                    &mut effect_offset,
                    &mut effect_scale,
                    &mut effect_rot,
                );
                *tint = effect_tint;
                *glow = effect_glow;
                *cropleft = state.cropleft.clamp(0.0, 1.0);
                *cropright = state.cropright.clamp(0.0, 1.0);
                *croptop = state.croptop.clamp(0.0, 1.0);
                *cropbottom = state.cropbottom.clamp(0.0, 1.0);
                *fadeleft = state.fadeleft.clamp(0.0, 1.0);
                *faderight = state.faderight.clamp(0.0, 1.0);
                *fadetop = state.fadetop.clamp(0.0, 1.0);
                *fadebottom = state.fadebottom.clamp(0.0, 1.0);
                *blend = overlay_blend;
                *mask_source = state.mask_source;
                *mask_dest = state.mask_dest;
                *rot_x_deg = effect_rot[0];
                *rot_y_deg = effect_rot[1];
                *rot_z_deg = effect_rot[2];
                offset[0] = effect_offset[0].mul_add(x_scale, offset[0]);
                offset[1] = effect_offset[1].mul_add(y_scale, offset[1]);
                *world_z += song_lua_biased_world_z(state, effect_offset[2]);
                scale[0] *= effect_scale[0];
                scale[1] *= effect_scale[1];
                *actor_effect = deadlib_present::anim::EffectState::default();
                *actor_flip_x ^= flip_x;
                *actor_flip_y ^= flip_y;
                *visible = state.visible;
            }
            let glow = if let Actor::Sprite { glow, .. } = &actor {
                *glow
            } else {
                state.glow
            };
            Some(finalize_actor(
                actor,
                glow,
                projected_mesh_scratch.as_deref_mut(),
            ))
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
fn build_song_lua_overlay_actor<S: NoteskinSlot + Clone>(
    overlay: &SongLuaOverlayActor<S>,
    state: SongLuaOverlayState,
    camera_state: Option<SongLuaOverlayState>,
    asset_manager: &AssetManager,
    z: i16,
    overlay_space_width: f32,
    overlay_space_height: f32,
    effect_time: f32,
    effect_beat: f32,
    total_elapsed: f32,
) -> Option<SongLuaActorList> {
    build_song_lua_overlay_actor_with_scratch(
        overlay,
        state,
        camera_state,
        asset_manager,
        z,
        overlay_space_width,
        overlay_space_height,
        effect_time,
        effect_beat,
        total_elapsed,
        None,
    )
}

fn song_lua_wrap_overlay_shadow(
    state: SongLuaOverlayState,
    mut actor: Actor,
    x_scale: f32,
    y_scale: f32,
) -> Actor {
    if state.shadow_len[0].abs() <= f32::EPSILON && state.shadow_len[1].abs() <= f32::EPSILON {
        return actor;
    }
    let len = [state.shadow_len[0] * x_scale, state.shadow_len[1] * y_scale];
    match &mut actor {
        Actor::Sprite {
            shadow_len,
            shadow_color,
            ..
        }
        | Actor::Text {
            shadow_len,
            shadow_color,
            ..
        } if shadow_len[0].abs() <= f32::EPSILON && shadow_len[1].abs() <= f32::EPSILON => {
            *shadow_len = len;
            *shadow_color = state.shadow_color;
            actor
        }
        _ => Actor::Shadow {
            len,
            color: state.shadow_color,
            child: Box::new(actor),
        },
    }
}

fn song_lua_overlay_glow_actor(
    actor: &Actor,
    glow: [f32; 4],
    text_glow_mode: SongLuaTextGlowMode,
    scratch: Option<&mut SongLuaProjectedMeshScratch>,
) -> Option<Actor> {
    song_lua_overlay_glow_actor_with_static_vertices(actor, glow, text_glow_mode, scratch, None)
}

fn song_lua_overlay_glow_actor_with_static_vertices(
    actor: &Actor,
    glow: [f32; 4],
    text_glow_mode: SongLuaTextGlowMode,
    mut scratch: Option<&mut SongLuaProjectedMeshScratch>,
    prewarmed_static_vertices: Option<&Arc<[TexturedMeshVertex]>>,
) -> Option<Actor> {
    match actor {
        Actor::Sprite {
            align,
            offset,
            world_z,
            size,
            source,
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
            fadeleft,
            faderight,
            fadetop,
            fadebottom,
            mask_source,
            mask_dest,
            rot_x_deg,
            rot_y_deg,
            rot_z_deg,
            skew,
            local_offset,
            local_offset_rot_sin_cos,
            texcoordvelocity,
            animate,
            state_delay,
            scale,
            effect,
            blend,
            ..
        } => {
            if glow[3] <= f32::EPSILON {
                return None;
            }
            if *mask_source && !*mask_dest {
                return None;
            }
            Some(Actor::Sprite {
                align: *align,
                offset: *offset,
                world_z: *world_z,
                size: *size,
                source: source.clone(),
                // Sprite::DrawTexture uses TextureMode_Glow with the actor's
                // blend mode: whiten through texture alpha, without adding a
                // second texture-modulated diffuse pass.
                tint: [0.0; 4],
                glow,
                z: *z,
                cell: *cell,
                grid: *grid,
                uv_rect: *uv_rect,
                visible: *visible,
                flip_x: *flip_x,
                flip_y: *flip_y,
                cropleft: *cropleft,
                cropright: *cropright,
                croptop: *croptop,
                cropbottom: *cropbottom,
                fadeleft: *fadeleft,
                faderight: *faderight,
                fadetop: *fadetop,
                fadebottom: *fadebottom,
                blend: *blend,
                mask_source: false,
                mask_dest: *mask_dest,
                rot_x_deg: *rot_x_deg,
                rot_y_deg: *rot_y_deg,
                rot_z_deg: *rot_z_deg,
                skew: *skew,
                local_offset: *local_offset,
                local_offset_rot_sin_cos: *local_offset_rot_sin_cos,
                texcoordvelocity: *texcoordvelocity,
                animate: *animate,
                state_delay: *state_delay,
                scale: *scale,
                shadow_len: [0.0, 0.0],
                shadow_color: [0.0, 0.0, 0.0, 0.5],
                effect: *effect,
            })
        }
        Actor::Text {
            align,
            offset,
            local_transform,
            font,
            content,
            attributes: base_attributes,
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
            jitter: _,
            distortion,
            clip,
            mask_dest,
            effect,
            ..
        } => {
            let has_attr_glow = song_lua_text_attributes_have_glow(base_attributes.as_slice());
            if glow[3] <= f32::EPSILON && !has_attr_glow {
                return None;
            }
            let (attributes, color, stroke_color) = if has_attr_glow {
                let attributes = song_lua_text_glow_attributes(
                    content.as_str(),
                    base_attributes.as_slice(),
                    glow,
                    scratch.as_deref_mut(),
                );
                let stroke_color = (glow[3] > f32::EPSILON
                    && matches!(
                        text_glow_mode,
                        SongLuaTextGlowMode::Stroke | SongLuaTextGlowMode::Both
                    ))
                .then_some(glow);
                (attributes, [1.0, 1.0, 1.0, 1.0], stroke_color)
            } else {
                let mut attributes = base_attributes.clone();
                let (color, stroke_color) = match text_glow_mode {
                    SongLuaTextGlowMode::Inner => (glow, None),
                    SongLuaTextGlowMode::Both => (glow, Some(glow)),
                    SongLuaTextGlowMode::Stroke => {
                        attributes = song_lua_transparent_text_attributes(
                            content.as_str(),
                            scratch.as_deref_mut(),
                        );
                        ([1.0, 1.0, 1.0, 1.0], Some(glow))
                    }
                };
                (attributes, color, stroke_color)
            };
            Some(Actor::Text {
                align: *align,
                offset: *offset,
                local_transform: *local_transform,
                color,
                stroke_color,
                glow: [0.0, 0.0, 0.0, 0.0],
                font,
                content: content.clone(),
                attributes,
                align_text: *align_text,
                z: *z,
                scale: *scale,
                fit_width: *fit_width,
                fit_height: *fit_height,
                line_spacing: *line_spacing,
                wrap_width_pixels: *wrap_width_pixels,
                max_width: *max_width,
                max_height: *max_height,
                max_w_pre_zoom: *max_w_pre_zoom,
                max_h_pre_zoom: *max_h_pre_zoom,
                jitter: false,
                distortion: *distortion,
                clip: *clip,
                mask_dest: *mask_dest,
                blend: BlendMode::Add,
                shadow_len: [0.0, 0.0],
                shadow_color: [0.0, 0.0, 0.0, 0.5],
                effect: *effect,
            })
        }
        Actor::TexturedMesh {
            align,
            offset,
            world_z,
            size,
            local_transform,
            texture,
            vertices,
            geom_cache_key,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            depth_test,
            visible,
            blend,
            z,
            ..
        } => {
            if glow[3] <= f32::EPSILON {
                return None;
            }
            let glow_vertices = scratch
                .as_deref_mut()
                .map(|scratch| scratch.update_textured_glow(vertices.as_ref()));
            let actor = if let Some(vertices) = prewarmed_static_vertices {
                Actor::TexturedMesh {
                    align: *align,
                    offset: *offset,
                    world_z: *world_z,
                    size: *size,
                    local_transform: *local_transform,
                    texture: texture.clone(),
                    tint: [1.0, 1.0, 1.0, 0.0],
                    glow,
                    vertices: Arc::clone(vertices),
                    geom_cache_key: song_lua_glow_geometry_key(*geom_cache_key),
                    uv_scale: *uv_scale,
                    uv_offset: *uv_offset,
                    uv_tex_shift: *uv_tex_shift,
                    depth_test: *depth_test,
                    visible: *visible,
                    blend: *blend,
                    z: *z,
                }
            } else if let Some(vertices) = glow_vertices {
                Actor::ReusableTexturedMesh {
                    align: *align,
                    offset: *offset,
                    world_z: *world_z,
                    size: *size,
                    local_transform: *local_transform,
                    texture: texture.clone(),
                    tint: [1.0, 1.0, 1.0, 0.0],
                    glow,
                    vertices,
                    geom_cache_key: INVALID_TMESH_CACHE_KEY,
                    uv_scale: *uv_scale,
                    uv_offset: *uv_offset,
                    uv_tex_shift: *uv_tex_shift,
                    depth_test: *depth_test,
                    visible: *visible,
                    blend: *blend,
                    z: *z,
                }
            } else {
                let mut glow_vertices = vertices.as_ref().to_vec();
                for vertex in &mut glow_vertices {
                    vertex.color = [1.0, 1.0, 1.0, vertex.color[3]];
                }
                Actor::TexturedMesh {
                    align: *align,
                    offset: *offset,
                    world_z: *world_z,
                    size: *size,
                    local_transform: *local_transform,
                    texture: texture.clone(),
                    tint: [1.0, 1.0, 1.0, 0.0],
                    glow,
                    vertices: Arc::from(glow_vertices.into_boxed_slice()),
                    geom_cache_key: INVALID_TMESH_CACHE_KEY,
                    uv_scale: *uv_scale,
                    uv_offset: *uv_offset,
                    uv_tex_shift: *uv_tex_shift,
                    depth_test: *depth_test,
                    visible: *visible,
                    blend: *blend,
                    z: *z,
                }
            };
            Some(actor)
        }
        Actor::ReusableTexturedMesh {
            align,
            offset,
            world_z,
            size,
            local_transform,
            texture,
            vertices,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            depth_test,
            visible,
            blend,
            z,
            ..
        } => {
            if glow[3] <= f32::EPSILON {
                return None;
            }
            let glow_vertices = scratch.as_deref_mut().map_or_else(
                || {
                    let mut out = Vec::with_capacity(vertices.len());
                    out.extend(vertices.iter().copied().map(|mut vertex| {
                        vertex.color = [1.0, 1.0, 1.0, vertex.color[3]];
                        vertex
                    }));
                    Arc::new(out)
                },
                |scratch| scratch.update_textured_glow(vertices),
            );
            Some(Actor::ReusableTexturedMesh {
                align: *align,
                offset: *offset,
                world_z: *world_z,
                size: *size,
                local_transform: *local_transform,
                texture: texture.clone(),
                tint: [1.0, 1.0, 1.0, 0.0],
                glow,
                vertices: glow_vertices,
                geom_cache_key: INVALID_TMESH_CACHE_KEY,
                uv_scale: *uv_scale,
                uv_offset: *uv_offset,
                uv_tex_shift: *uv_tex_shift,
                depth_test: *depth_test,
                visible: *visible,
                blend: *blend,
                z: *z,
            })
        }
        _ => None,
    }
}

fn song_lua_finalize_overlay_actor(
    state: SongLuaOverlayState,
    mut actor: Actor,
    glow: [f32; 4],
    x_scale: f32,
    y_scale: f32,
    scratch: Option<&mut SongLuaProjectedMeshScratch>,
) -> SongLuaActorList {
    let glow_actor = if let Actor::Sprite {
        glow: actor_glow, ..
    } = &mut actor
    {
        // Sprite composition already emits TextureMode_Glow after diffuse.
        // A separate glow actor would draw that pass a second time.
        *actor_glow = glow;
        None
    } else {
        song_lua_overlay_glow_actor(&actor, glow, state.text_glow_mode, scratch)
    };
    let actor = song_lua_wrap_overlay_shadow(state, actor, x_scale, y_scale);
    let mut out = SmallVec::new();
    out.push(actor);
    if let Some(glow_actor) = glow_actor {
        out.push(glow_actor);
    }
    out
}

#[inline(always)]
fn push_song_lua_capture_actor(
    out: &mut Vec<Actor>,
    actor: Actor,
    tint: [f32; 4],
    blend: Option<BlendMode>,
    z_shift: i16,
) {
    if z_shift == 0 && tint == [1.0; 4] && blend.is_none() {
        out.push(actor);
    } else {
        out.push(song_lua_style_capture_actor(actor, tint, blend, z_shift));
    }
}

#[derive(Clone, Copy, Debug)]
struct PlayerTransformMetrics {
    screen_width: f32,
    screen_height: f32,
    screen_center_y: f32,
}

impl PlayerTransformMetrics {
    #[inline(always)]
    fn current() -> Self {
        Self {
            screen_width: screen_width(),
            screen_height: screen_height(),
            screen_center_y: screen_center_y(),
        }
    }
}

#[inline(always)]
fn song_lua_player_root_camera_with_metrics(
    player_transform: Matrix4,
    metrics: PlayerTransformMetrics,
) -> Matrix4 {
    glam::camera::rh::proj::opengl::orthographic(
        -0.5 * metrics.screen_width,
        0.5 * metrics.screen_width,
        -0.5 * metrics.screen_height,
        0.5 * metrics.screen_height,
        -4096.0,
        4096.0,
    ) * player_transform
}

#[inline(always)]
fn song_lua_player_root_camera(player_transform: Matrix4) -> Matrix4 {
    song_lua_player_root_camera_with_metrics(player_transform, PlayerTransformMetrics::current())
}

#[allow(clippy::too_many_arguments)]
fn append_song_lua_player_transform<F, H>(
    field_actors: F,
    hud_actors: H,
    field_len: usize,
    hud_len: usize,
    field_has_camera: bool,
    out: &mut Vec<Actor>,
    z_shift: i16,
    tint: [f32; 4],
    blend: Option<BlendMode>,
    playfield_center_x: f32,
    target_x: f32,
    target_y: f32,
    rotation_x_deg: f32,
    rotation_z_deg: f32,
    rotation_y_deg: f32,
    skew_x: f32,
    skew_y: f32,
    zoom_x: f32,
    zoom_y: f32,
    zoom_z: f32,
) where
    F: IntoIterator<Item = Actor>,
    H: IntoIterator<Item = Actor>,
{
    let fold_y = |actor| {
        if rotation_y_deg.is_finite() && rotation_y_deg.abs() > f32::EPSILON {
            song_lua_player_y_fold_actor(actor, playfield_center_x, rotation_y_deg)
        } else {
            actor
        }
    };
    let Some(player_transform) = song_lua_player_transform_matrix(SongLuaPlayerTransformRequest {
        screen_width: screen_width(),
        screen_height: screen_height(),
        screen_center_y: screen_center_y(),
        playfield_center_x,
        target_x,
        target_y,
        rotation_x_deg,
        rotation_z_deg,
        skew_x,
        skew_y,
        zoom_x,
        zoom_y,
        zoom_z,
    }) else {
        out.reserve(field_len.saturating_add(hud_len));
        for actor in hud_actors.into_iter().map(fold_y) {
            push_song_lua_capture_actor(out, actor, [1.0; 4], None, z_shift);
        }
        for actor in field_actors.into_iter().map(fold_y) {
            push_song_lua_capture_actor(out, actor, [1.0; 4], None, z_shift);
        }
        return;
    };

    let root_camera = song_lua_player_root_camera(player_transform);
    out.reserve(field_len.saturating_add(hud_len).saturating_add(4));
    if !field_has_camera {
        if field_len + hud_len == 0 {
            return;
        }
        push_song_lua_capture_actor(
            out,
            Actor::CameraPush {
                view_proj: root_camera,
            },
            tint,
            blend,
            z_shift,
        );
        for actor in hud_actors.into_iter().map(fold_y) {
            push_song_lua_capture_actor(out, actor, tint, blend, z_shift);
        }
        for actor in field_actors.into_iter().map(fold_y) {
            push_song_lua_capture_actor(out, actor, tint, blend, z_shift);
        }
        push_song_lua_capture_actor(out, Actor::CameraPop, tint, blend, z_shift);
        return;
    }

    let mut root_open = hud_len > 0;
    if root_open {
        push_song_lua_capture_actor(
            out,
            Actor::CameraPush {
                view_proj: root_camera,
            },
            tint,
            blend,
            z_shift,
        );
        for actor in hud_actors.into_iter().map(fold_y) {
            push_song_lua_capture_actor(out, actor, tint, blend, z_shift);
        }
    }
    let mut field_camera_depth = 0usize;
    for actor in field_actors.into_iter().map(fold_y) {
        match actor {
            Actor::Camera {
                view_proj,
                children,
            } => {
                if field_camera_depth == 0 && root_open {
                    push_song_lua_capture_actor(out, Actor::CameraPop, tint, blend, z_shift);
                    root_open = false;
                }
                push_song_lua_capture_actor(
                    out,
                    Actor::CameraPush {
                        view_proj: view_proj * player_transform,
                    },
                    tint,
                    blend,
                    z_shift,
                );
                for child in children {
                    push_song_lua_capture_actor(out, child, tint, blend, z_shift);
                }
                push_song_lua_capture_actor(out, Actor::CameraPop, tint, blend, z_shift);
            }
            Actor::CameraPush { view_proj } => {
                if field_camera_depth == 0 && root_open {
                    push_song_lua_capture_actor(out, Actor::CameraPop, tint, blend, z_shift);
                    root_open = false;
                }
                push_song_lua_capture_actor(
                    out,
                    Actor::CameraPush {
                        view_proj: view_proj * player_transform,
                    },
                    tint,
                    blend,
                    z_shift,
                );
                field_camera_depth = field_camera_depth.saturating_add(1);
            }
            Actor::CameraPop => {
                push_song_lua_capture_actor(out, Actor::CameraPop, tint, blend, z_shift);
                field_camera_depth = field_camera_depth.saturating_sub(1);
            }
            other if field_camera_depth > 0 => {
                push_song_lua_capture_actor(out, other, tint, blend, z_shift);
            }
            other => {
                if !root_open {
                    push_song_lua_capture_actor(
                        out,
                        Actor::CameraPush {
                            view_proj: root_camera,
                        },
                        tint,
                        blend,
                        z_shift,
                    );
                    root_open = true;
                }
                push_song_lua_capture_actor(out, other, tint, blend, z_shift);
            }
        }
    }
    if root_open {
        push_song_lua_capture_actor(out, Actor::CameraPop, tint, blend, z_shift);
    }
}

#[inline(always)]
fn song_lua_player_transform_has_identity_geometry_with_metrics(
    transform: SongLuaCaptureTransform,
    metrics: PlayerTransformMetrics,
) -> bool {
    metrics.screen_width.is_finite()
        && metrics.screen_height.is_finite()
        && metrics.screen_center_y.is_finite()
        && transform.playfield_center_x.is_finite()
        && transform.target_x.is_finite()
        && transform.target_y.is_finite()
        && transform.rotation_x.is_finite()
        && transform.rotation_x.abs() <= f32::EPSILON
        && transform.rotation_z.is_finite()
        && transform.rotation_z.abs() <= f32::EPSILON
        && transform.rotation_y.is_finite()
        && transform.rotation_y.abs() <= f32::EPSILON
        && transform.skew_x.is_finite()
        && transform.skew_x.abs() <= f32::EPSILON
        && transform.skew_y.is_finite()
        && transform.skew_y.abs() <= f32::EPSILON
        && transform.zoom_x.is_finite()
        && (transform.zoom_x - 1.0).abs() <= f32::EPSILON
        && transform.zoom_y.is_finite()
        && (transform.zoom_y - 1.0).abs() <= f32::EPSILON
        && transform.zoom_z.is_finite()
        && (transform.zoom_z - 1.0).abs() <= f32::EPSILON
        && (transform.target_x - transform.playfield_center_x).abs() <= f32::EPSILON
        && (metrics.screen_center_y - transform.target_y).abs() <= f32::EPSILON
}

#[inline(always)]
fn song_lua_player_transform_has_identity_geometry(transform: SongLuaCaptureTransform) -> bool {
    song_lua_player_transform_has_identity_geometry_with_metrics(
        transform,
        PlayerTransformMetrics::current(),
    )
}

#[inline(always)]
fn song_lua_player_transform_is_direct_identity(transform: SongLuaCaptureTransform) -> bool {
    transform.z_shift == 0
        && transform.tint == [1.0; 4]
        && transform.blend.is_none()
        && song_lua_player_transform_has_identity_geometry(transform)
}

#[inline(always)]
fn song_lua_player_transform_is_direct_proxy(transform: SongLuaCaptureTransform) -> bool {
    transform.tint.into_iter().all(f32::is_finite)
        && transform.playfield_center_x.is_finite()
        && transform.target_x.is_finite()
        && transform.target_y.is_finite()
        && transform.rotation_x.is_finite()
        && transform.rotation_z.is_finite()
        && transform.rotation_y.is_finite()
        && transform.skew_x.is_finite()
        && transform.skew_y.is_finite()
        && transform.zoom_x.is_finite()
        && transform.zoom_y.is_finite()
        && transform.zoom_z.is_finite()
}

#[inline(always)]
fn song_lua_player_x_fold(transform: SongLuaCaptureTransform) -> Option<ActorXFold> {
    (transform.playfield_center_x.is_finite()
        && transform.rotation_y.is_finite()
        && transform.rotation_y.abs() > f32::EPSILON)
        .then(|| {
            ActorXFold::new(
                transform.playfield_center_x,
                transform.rotation_y.to_radians().cos(),
            )
        })
}

#[inline(always)]
fn song_lua_player_transform_is_direct_hud_proxy(transform: SongLuaCaptureTransform) -> bool {
    song_lua_player_transform_is_direct_proxy(transform)
}

#[cfg(test)]
fn song_lua_direct_field_camera(
    field_camera: Option<Matrix4>,
    transform: SongLuaCaptureTransform,
) -> Option<Matrix4> {
    let player_transform = song_lua_player_camera_suffix(transform);
    match (field_camera, player_transform) {
        (Some(camera), Some(player_transform)) => Some(camera * player_transform),
        (None, Some(player_transform)) => Some(song_lua_player_root_camera(player_transform)),
        (camera, None) => camera,
    }
}

fn song_lua_player_camera_suffix(transform: SongLuaCaptureTransform) -> Option<Matrix4> {
    song_lua_player_transform_matrix(SongLuaPlayerTransformRequest {
        screen_width: screen_width(),
        screen_height: screen_height(),
        screen_center_y: screen_center_y(),
        playfield_center_x: transform.playfield_center_x,
        target_x: transform.target_x,
        target_y: transform.target_y,
        rotation_x_deg: transform.rotation_x,
        rotation_z_deg: transform.rotation_z,
        skew_x: transform.skew_x,
        skew_y: transform.skew_y,
        zoom_x: transform.zoom_x,
        zoom_y: transform.zoom_y,
        zoom_z: transform.zoom_z,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PlayerActorAssembly {
    Captured,
    Hidden,
    DirectZ {
        z_shift: i16,
    },
    DirectFold {
        z_shift: i16,
        x_fold: ActorXFold,
    },
    DirectTransform {
        z_shift: i16,
        tint: [f32; 4],
        blend: Option<BlendMode>,
        root_camera: Matrix4,
        field_camera_suffix: Matrix4,
        x_fold: Option<ActorXFold>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PlayerActorAssemblyKey {
    requests_player_proxy: bool,
    visible: bool,
    z_shift: i16,
    blend: Option<BlendMode>,
    metrics: [u32; 3],
    tint: [u32; 4],
    geometry: [u32; 11],
}

impl PlayerActorAssemblyKey {
    fn new(
        requests_player_proxy: bool,
        visible: bool,
        transform: SongLuaCaptureTransform,
        metrics: PlayerTransformMetrics,
    ) -> Self {
        Self {
            requests_player_proxy,
            visible,
            z_shift: transform.z_shift,
            blend: transform.blend,
            metrics: [
                metrics.screen_width.to_bits(),
                metrics.screen_height.to_bits(),
                metrics.screen_center_y.to_bits(),
            ],
            tint: transform.tint.map(f32::to_bits),
            geometry: [
                transform.playfield_center_x.to_bits(),
                transform.target_x.to_bits(),
                transform.target_y.to_bits(),
                transform.rotation_x.to_bits(),
                transform.rotation_z.to_bits(),
                transform.rotation_y.to_bits(),
                transform.skew_x.to_bits(),
                transform.skew_y.to_bits(),
                transform.zoom_x.to_bits(),
                transform.zoom_y.to_bits(),
                transform.zoom_z.to_bits(),
            ],
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PlayerActorAssemblyCacheStats {
    hits: u64,
    rebuilds: u64,
}

/// Per-player transform plan retained by one gameplay screen.
///
/// The gameplay/presentation thread owns the fixed two-entry screen-lifetime
/// storage for non-identity transforms. Exact source-bit or viewport changes
/// rebuild one bounded plan synchronously through the canonical resolver;
/// stable transformed frames copy it. Identity, hidden, and captured players
/// retain their cheaper direct decisions without constructing a key. There is
/// no allocation, synchronization, overflow, pruning, or eviction scan. Screen
/// teardown drops both inline entries, counters expose hit/rebuild behavior,
/// and worst-case frame work is one plan rebuild per active player.
#[derive(Clone, Copy, Debug, Default)]
struct PlayerActorAssemblyCache {
    key: Option<PlayerActorAssemblyKey>,
    assembly: Option<PlayerActorAssembly>,
    stats: PlayerActorAssemblyCacheStats,
}

impl PlayerActorAssemblyCache {
    #[inline(always)]
    fn resolve(
        &mut self,
        requests_player_proxy: bool,
        visible: bool,
        transform: SongLuaCaptureTransform,
    ) -> PlayerActorAssembly {
        let metrics = PlayerTransformMetrics::current();
        if let Some(assembly) =
            direct_player_actor_assembly(requests_player_proxy, visible, transform, metrics)
        {
            return assembly;
        }
        self.resolve_nonidentity(requests_player_proxy, visible, transform, metrics)
    }

    #[cfg(test)]
    fn resolve_with_metrics(
        &mut self,
        requests_player_proxy: bool,
        visible: bool,
        transform: SongLuaCaptureTransform,
        metrics: PlayerTransformMetrics,
    ) -> PlayerActorAssembly {
        if let Some(assembly) =
            direct_player_actor_assembly(requests_player_proxy, visible, transform, metrics)
        {
            return assembly;
        }
        self.resolve_nonidentity(requests_player_proxy, visible, transform, metrics)
    }

    fn resolve_nonidentity(
        &mut self,
        requests_player_proxy: bool,
        visible: bool,
        transform: SongLuaCaptureTransform,
        metrics: PlayerTransformMetrics,
    ) -> PlayerActorAssembly {
        debug_assert!(
            visible
                && !requests_player_proxy
                && !song_lua_player_transform_has_identity_geometry_with_metrics(
                    transform, metrics
                ),
            "only non-identity visible player plans enter retained storage"
        );
        let key = PlayerActorAssemblyKey::new(requests_player_proxy, visible, transform, metrics);
        if self.key == Some(key) {
            self.stats.hits = self.stats.hits.saturating_add(1);
            return self
                .assembly
                .expect("a retained player transform key has an assembly");
        }
        let assembly = player_actor_assembly_for_transform_with_metrics(
            requests_player_proxy,
            visible,
            transform,
            metrics,
        );
        self.key = Some(key);
        self.assembly = Some(assembly);
        self.stats.rebuilds = self.stats.rebuilds.saturating_add(1);
        assembly
    }

    #[cfg(test)]
    const fn stats(&self) -> PlayerActorAssemblyCacheStats {
        self.stats
    }

    const fn generation(&self) -> u64 {
        self.stats.rebuilds
    }
}

#[inline(always)]
fn direct_player_actor_assembly(
    requests_player_proxy: bool,
    visible: bool,
    transform: SongLuaCaptureTransform,
    metrics: PlayerTransformMetrics,
) -> Option<PlayerActorAssembly> {
    if !visible {
        Some(PlayerActorAssembly::Hidden)
    } else if requests_player_proxy {
        Some(PlayerActorAssembly::Captured)
    } else if song_lua_player_transform_has_identity_geometry_with_metrics(transform, metrics) {
        Some(PlayerActorAssembly::DirectZ {
            z_shift: transform.z_shift,
        })
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PlayerFieldCameraCacheStats {
    hits: u64,
    rebuilds: u64,
}

/// Final flat-field camera retained from two upstream source generations.
///
/// The gameplay/presentation thread owns exactly one screen-lifetime entry per
/// player. Only non-identity direct transforms enter it. Stable frames compare
/// two `u64` tokens and copy one inline matrix; either upstream rebuild performs
/// one bounded matrix product. There is no allocation, synchronization,
/// overflow, pruning, or eviction scan. Screen teardown drops both entries,
/// counters expose hit/rebuild behavior, and worst-case frame work is one
/// matrix product per active transformed player.
#[derive(Clone, Copy, Debug, Default)]
struct PlayerFieldCameraCache {
    key: Option<(u64, u64)>,
    camera: Option<Matrix4>,
    stats: PlayerFieldCameraCacheStats,
}

impl PlayerFieldCameraCache {
    fn resolve(
        &mut self,
        field_generation: u64,
        transform_generation: u64,
        field_camera: Option<Matrix4>,
        root_camera: Matrix4,
        field_camera_suffix: Matrix4,
    ) -> Matrix4 {
        let key = (field_generation, transform_generation);
        if self.key == Some(key) {
            self.stats.hits = self.stats.hits.saturating_add(1);
            return self
                .camera
                .expect("a retained transformed field-camera key has a matrix");
        }
        let camera = field_camera.map_or(root_camera, |camera| camera * field_camera_suffix);
        self.key = Some(key);
        self.camera = Some(camera);
        self.stats.rebuilds = self.stats.rebuilds.saturating_add(1);
        camera
    }

    #[cfg(test)]
    const fn stats(&self) -> PlayerFieldCameraCacheStats {
        self.stats
    }
}

#[derive(Clone, Copy, Debug)]
struct PlayerActorSegment {
    player: usize,
    assembly: PlayerActorAssembly,
    field_camera: Option<Matrix4>,
    manual_hud_draw: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct GameplayActorSegments {
    insert: usize,
    players: [Option<PlayerActorSegment>; 2],
    direct_proxy_len: usize,
}

impl GameplayActorSegments {
    #[cfg(any(test, feature = "test-support"))]
    pub const fn direct_proxy_count(&self) -> usize {
        self.direct_proxy_len
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn has_direct_field_proxy(&self, state: &FrameScratch) -> bool {
        self.direct_field_proxy_count(state) != 0
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn direct_field_proxy_count(&self, state: &FrameScratch) -> usize {
        state
            .song_lua_direct_proxies
            .entries
            .iter()
            .take(self.direct_proxy_len)
            .map(|proxy| {
                usize::from(
                    proxy.draws == SongLuaDirectDraws::Field && proxy.draw_start < proxy.draw_end,
                ) + usize::from(proxy.tail.is_some_and(|tail| {
                    tail.draws == SongLuaDirectDraws::Field && tail.draw_start < tail.draw_end
                }))
            })
            .sum()
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn direct_combo_proxy_count(&self, state: &FrameScratch) -> usize {
        state
            .song_lua_direct_proxies
            .entries
            .iter()
            .take(self.direct_proxy_len)
            .filter(|proxy| {
                proxy.draws == SongLuaDirectDraws::Combo && proxy.draw_start < proxy.draw_end
            })
            .count()
    }

    /// # Panics
    ///
    /// Panics if an internal state invariant is violated.
    pub fn segments<'a>(
        &'a self,
        state: &'a FrameScratch,
        actors: &'a [Actor],
    ) -> impl Clone + Iterator<Item = ActorSegment<'a>> + 'a {
        let scratch = state;
        GameplayActorSegmentIter {
            owner: self,
            scratch,
            actors,
            phase: 0,
            player: 0,
            player_part: 0,
            proxy: 0,
            proxy_draw: false,
            actor_start: self.insert.min(actors.len()),
        }
    }
}

#[derive(Clone)]
struct GameplayActorSegmentIter<'a> {
    owner: &'a GameplayActorSegments,
    scratch: &'a FrameScratch,
    actors: &'a [Actor],
    phase: u8,
    player: usize,
    player_part: u8,
    proxy: usize,
    proxy_draw: bool,
    actor_start: usize,
}

impl<'a> GameplayActorSegmentIter<'a> {
    fn direct_draws(
        &self,
        player: usize,
        draws: SongLuaDirectDraws,
        start: usize,
        end: usize,
    ) -> &'a [FlatDraw] {
        let source = match draws {
            SongLuaDirectDraws::Field => &self.scratch.notefield_flat_draw_scratch[player],
            SongLuaDirectDraws::Hud | SongLuaDirectDraws::Judgment | SongLuaDirectDraws::Combo => {
                &self.scratch.notefield_hud_flat_draw_scratch[player]
            }
        };
        source.get(start..end).unwrap_or(&[])
    }

    fn player_segment(
        &self,
        segment: &'a PlayerActorSegment,
        part: u8,
    ) -> Option<ActorSegment<'a>> {
        let player = segment.player;
        match (&segment.assembly, part) {
            (PlayerActorAssembly::Captured, 0) => {
                debug_assert!(self.scratch.notefield_flat_draw_scratch[player].is_empty());
                debug_assert!(self.scratch.notefield_hud_flat_draw_scratch[player].is_empty());
                let source = self
                    .scratch
                    .song_lua_proxy_actor_scratch
                    .as_ref()
                    .and_then(|scratch| scratch.player_source(player, SONG_LUA_PLAYER_PROXY_SOURCE))
                    .expect("player proxy capture has song-lifetime backing");
                Some(ActorSegment::new(source))
            }
            (PlayerActorAssembly::DirectZ { z_shift }, 0) => Some(
                ActorSegment::shifted(&self.scratch.notefield_hud_actor_scratch[player], *z_shift)
                    .with_flat_draws(&self.scratch.notefield_hud_flat_draw_scratch[player], None),
            ),
            (PlayerActorAssembly::DirectZ { z_shift }, 1) => Some(
                ActorSegment::shifted(&self.scratch.notefield_actor_scratch[player], *z_shift)
                    .with_flat_draws(
                        &self.scratch.notefield_flat_draw_scratch[player],
                        segment.field_camera.as_ref(),
                    ),
            ),
            (PlayerActorAssembly::DirectFold { z_shift, x_fold }, 0) => {
                let hud = if segment.manual_hud_draw {
                    ActorSegment::shifted(
                        &self.scratch.notefield_hud_actor_scratch[player],
                        *z_shift,
                    )
                } else {
                    ActorSegment::folded(
                        &self.scratch.notefield_hud_actor_scratch[player],
                        *z_shift,
                        *x_fold,
                    )
                };
                Some(
                    hud.with_flat_draws(
                        &self.scratch.notefield_hud_flat_draw_scratch[player],
                        None,
                    ),
                )
            }
            (PlayerActorAssembly::DirectFold { z_shift, x_fold }, 1) => Some(
                ActorSegment::folded(
                    &self.scratch.notefield_actor_scratch[player],
                    *z_shift,
                    *x_fold,
                )
                .with_flat_draws(
                    &self.scratch.notefield_flat_draw_scratch[player],
                    segment.field_camera.as_ref(),
                ),
            ),
            (
                PlayerActorAssembly::DirectTransform {
                    z_shift,
                    tint,
                    blend,
                    root_camera,
                    field_camera_suffix: _,
                    x_fold,
                },
                0,
            ) => {
                let (hud, camera) = if segment.manual_hud_draw {
                    (
                        ActorSegment::shifted(
                            &self.scratch.notefield_hud_actor_scratch[player],
                            *z_shift,
                        ),
                        None,
                    )
                } else {
                    (
                        ActorSegment::transformed(
                            &self.scratch.notefield_hud_actor_scratch[player],
                            *z_shift,
                            tint,
                            *blend,
                            root_camera,
                            &Matrix4::IDENTITY,
                            *x_fold,
                        ),
                        Some(root_camera),
                    )
                };
                Some(hud.with_flat_draws(
                    &self.scratch.notefield_hud_flat_draw_scratch[player],
                    camera,
                ))
            }
            (
                PlayerActorAssembly::DirectTransform {
                    z_shift,
                    tint,
                    blend,
                    root_camera,
                    field_camera_suffix,
                    x_fold,
                },
                1,
            ) => Some(
                ActorSegment::transformed(
                    &self.scratch.notefield_actor_scratch[player],
                    *z_shift,
                    tint,
                    *blend,
                    root_camera,
                    field_camera_suffix,
                    *x_fold,
                )
                .with_flat_draws(
                    &self.scratch.notefield_flat_draw_scratch[player],
                    segment.field_camera.as_ref(),
                ),
            ),
            (PlayerActorAssembly::Captured | PlayerActorAssembly::Hidden, _)
            | (PlayerActorAssembly::DirectZ { .. }, _)
            | (PlayerActorAssembly::DirectFold { .. }, _)
            | (PlayerActorAssembly::DirectTransform { .. }, _) => None,
        }
    }
}

impl<'a> Iterator for GameplayActorSegmentIter<'a> {
    type Item = ActorSegment<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.phase {
                0 => {
                    self.phase = 1;
                    return Some(ActorSegment::new(&self.actors[..self.actor_start]));
                }
                1 => {
                    while self.player < self.owner.players.len() {
                        let player = self.player;
                        let part = self.player_part;
                        self.player_part += 1;
                        if self.player_part == 2 {
                            self.player += 1;
                            self.player_part = 0;
                        }
                        let Some(segment) = self.owner.players[player].as_ref() else {
                            continue;
                        };
                        if let Some(segment) = self.player_segment(segment, part) {
                            return Some(segment);
                        }
                    }
                    self.phase = 2;
                }
                2 => {
                    debug_assert!(
                        self.owner.direct_proxy_len
                            <= self.scratch.song_lua_direct_proxies.entries.len()
                    );
                    let proxy_len = self
                        .owner
                        .direct_proxy_len
                        .min(self.scratch.song_lua_direct_proxies.entries.len());
                    let proxies = &self.scratch.song_lua_direct_proxies.entries[..proxy_len];
                    let Some(proxy) = proxies.get(self.proxy) else {
                        self.phase = 3;
                        continue;
                    };
                    let actor_insert = proxy
                        .actor_insert
                        .clamp(self.actor_start, self.actors.len());
                    if !self.proxy_draw {
                        self.proxy_draw = true;
                        return Some(ActorSegment::new(
                            &self.actors[self.actor_start..actor_insert],
                        ));
                    }
                    self.proxy_draw = false;
                    self.proxy += 1;
                    self.actor_start = actor_insert;
                    let draws = self.direct_draws(
                        proxy.player,
                        proxy.draws,
                        proxy.draw_start,
                        proxy.draw_end,
                    );
                    if let Some(tail) = proxy.tail.as_ref() {
                        let tail_draws = self.direct_draws(
                            proxy.player,
                            tail.draws,
                            tail.draw_start,
                            tail.draw_end,
                        );
                        return Some(ActorSegment::flat_proxy_pair_styled(
                            draws,
                            tail_draws,
                            proxy.offset,
                            proxy.z,
                            &proxy.style,
                            proxy.blend,
                            proxy.enclosing_camera.as_ref(),
                            [proxy.camera.as_ref(), tail.camera.as_ref()],
                        ));
                    }
                    return Some(ActorSegment::flat_proxy_styled_with_cameras(
                        draws,
                        proxy.offset,
                        proxy.z,
                        &proxy.style,
                        proxy.blend,
                        proxy.enclosing_camera.as_ref(),
                        proxy.camera.as_ref(),
                    ));
                }
                3 => {
                    self.phase = 4;
                    return Some(ActorSegment::new(&self.actors[self.actor_start..]));
                }
                _ => return None,
            }
        }
    }
}

#[inline(always)]
#[cfg(test)]
fn player_actor_assembly_for_transform(
    requests_player_proxy: bool,
    visible: bool,
    transform: SongLuaCaptureTransform,
) -> PlayerActorAssembly {
    player_actor_assembly_for_transform_with_metrics(
        requests_player_proxy,
        visible,
        transform,
        PlayerTransformMetrics::current(),
    )
}

#[inline(always)]
fn player_actor_assembly_for_transform_with_metrics(
    requests_player_proxy: bool,
    visible: bool,
    transform: SongLuaCaptureTransform,
    metrics: PlayerTransformMetrics,
) -> PlayerActorAssembly {
    if let Some(assembly) =
        direct_player_actor_assembly(requests_player_proxy, visible, transform, metrics)
    {
        return assembly;
    }
    let x_fold = song_lua_player_x_fold(transform);
    let field_camera_suffix = song_lua_player_transform_matrix(SongLuaPlayerTransformRequest {
        screen_width: metrics.screen_width,
        screen_height: metrics.screen_height,
        screen_center_y: metrics.screen_center_y,
        playfield_center_x: transform.playfield_center_x,
        target_x: transform.target_x,
        target_y: transform.target_y,
        rotation_x_deg: transform.rotation_x,
        rotation_z_deg: transform.rotation_z,
        skew_x: transform.skew_x,
        skew_y: transform.skew_y,
        zoom_x: transform.zoom_x,
        zoom_y: transform.zoom_y,
        zoom_z: transform.zoom_z,
    });
    match (field_camera_suffix, x_fold) {
        (Some(field_camera_suffix), x_fold) => PlayerActorAssembly::DirectTransform {
            z_shift: transform.z_shift,
            tint: transform.tint,
            blend: transform.blend,
            root_camera: song_lua_player_root_camera_with_metrics(field_camera_suffix, metrics),
            field_camera_suffix,
            x_fold,
        },
        (None, Some(x_fold)) => PlayerActorAssembly::DirectFold {
            z_shift: transform.z_shift,
            x_fold,
        },
        (None, None) => PlayerActorAssembly::DirectZ {
            z_shift: transform.z_shift,
        },
    }
}

#[inline(always)]
fn clear_player_actor_bundle(
    field_scratch: &mut Vec<Actor>,
    flat_draw_scratch: &mut Vec<FlatDraw>,
    hud_scratch: &mut Vec<Actor>,
    hud_flat_draw_scratch: &mut Vec<FlatDraw>,
) {
    field_scratch.clear();
    flat_draw_scratch.clear();
    hud_scratch.clear();
    hud_flat_draw_scratch.clear();
}

fn song_lua_player_target_x(
    explicit_x: Option<f32>,
    player_state_x: f32,
    layout_center_x: f32,
    notefield_view: ViewOverride,
) -> f32 {
    explicit_x.unwrap_or(if notefield_view.force_center_1player {
        layout_center_x
    } else {
        player_state_x
    })
}

fn prepare_song_lua_layer<S: NoteskinSlot + Clone>(
    out: &mut Vec<Actor>,
    overlays: &[SongLuaOverlayActor<S>],
    overlay_states: &[SongLuaOverlayState],
    song_foreground_state: SongLuaOverlayState,
    order_cache: &mut SongLuaOverlayOrderCache,
    order_scratch: &mut Vec<usize>,
    aft_capture_scratch: &mut SongLuaAftCaptureScratch,
    depth: SongLuaLayerDepth,
) -> Option<SongLuaLayerDepth> {
    if overlays.is_empty() {
        order_scratch.clear();
        return None;
    }
    Some(prepare_active_song_lua_layer(
        out,
        overlays,
        overlay_states,
        song_foreground_state,
        order_cache,
        order_scratch,
        aft_capture_scratch,
        depth,
    ))
}

fn prepare_active_song_lua_layer<S: NoteskinSlot + Clone>(
    out: &mut Vec<Actor>,
    overlays: &[SongLuaOverlayActor<S>],
    overlay_states: &[SongLuaOverlayState],
    song_foreground_state: SongLuaOverlayState,
    order_cache: &mut SongLuaOverlayOrderCache,
    order_scratch: &mut Vec<usize>,
    aft_capture_scratch: &mut SongLuaAftCaptureScratch,
    depth: SongLuaLayerDepth,
) -> SongLuaLayerDepth {
    aft_capture_scratch.begin_frame();
    out.reserve(overlays.len());
    song_lua_overlay_order_into(overlays, overlay_states, order_cache, None, order_scratch);
    depth.shifted(song_foreground_state.z)
}

/// Foreground::Update advances actors from the FGCHANGE's start, undoing
/// music rate with ITGmania's default RateModsAffectFGChanges=false. Hidden
/// children continue animating; the gameplay-screen lead-in is not actor age.
#[must_use]
pub fn foreground_elapsed(music_second: f32, start_second: f32, music_rate: f32) -> f32 {
    (music_second - start_second).max(0.0) / music_rate
}

fn push_song_lua_layer_actors<S: NoteskinSlot + Clone>(
    out: &mut Vec<Actor>,
    targets: &mut Vec<deadlib_present::actors::RenderTarget>,
    overlays: &[SongLuaOverlayActor<S>],
    order_cache: &mut SongLuaOverlayOrderCache,
    topology_index: &SongLuaOverlayTopologyIndex,
    local_overlay_states: &[SongLuaOverlayState],
    overlay_states: &[SongLuaOverlayState],
    song_foreground_state: SongLuaOverlayState,
    proxy_sources: &SongLuaScreenProxySources<'_>,
    mut direct_proxies: Option<&mut SongLuaDirectProxies>,
    mut proxy_actor_scratch: Option<&mut SongLuaProxyActorScratch>,
    asset_manager: &AssetManager,
    space_width: f32,
    space_height: f32,
    effect_time: f32,
    effect_beat: f32,
    total_elapsed: f32,
    order_scratch: &mut Vec<usize>,
    capture_states: &mut Vec<SongLuaOverlayState>,
    capture_order_scratch: &mut Vec<usize>,
    aft_capture_scratch: &mut SongLuaAftCaptureScratch,
    projected_mesh_scratch: &mut [SongLuaProjectedMeshScratch],
    depth: SongLuaLayerDepth,
) {
    let Some(song_lua_depth) = prepare_song_lua_layer(
        out,
        overlays,
        overlay_states,
        song_foreground_state,
        order_cache,
        order_scratch,
        aft_capture_scratch,
        depth,
    ) else {
        return;
    };
    for (draw_idx, idx) in order_scratch.iter().copied().enumerate() {
        let Some(overlay) = overlays.get(idx) else {
            continue;
        };
        if topology_index
            .aft_ancestors
            .get(idx)
            .copied()
            .and_then(SongLuaOverlayIndex::get)
            .is_some()
        {
            continue;
        }
        // These kinds never build actors; their children are drawn on their own.
        if matches!(
            overlay.kind,
            SongLuaOverlayKind::Actor
                | SongLuaOverlayKind::ActorFrame
                | SongLuaOverlayKind::UpdateTracks { .. }
                | SongLuaOverlayKind::Sound { .. }
        ) {
            continue;
        }
        let overlay_state = overlay_states
            .get(idx)
            .copied()
            .unwrap_or_else(SongLuaOverlayState::default);
        let z = song_lua_depth.draw_z(draw_idx);
        match &overlay.kind {
            SongLuaOverlayKind::ActorProxy { target } => {
                let overlay_state =
                    song_lua_proxy_effect(overlay_state, effect_time, effect_beat, idx as u32);
                if let Some((player_index, sources)) =
                    song_lua_direct_player_proxy_source(target, proxy_sources)
                {
                    if let Some(dest) = direct_proxies.as_deref_mut() {
                        for source in sources {
                            if let Some(proxy) = song_lua_direct_proxy(
                                overlay_state,
                                z,
                                source,
                                player_index,
                                out.len(),
                                space_width,
                                space_height,
                            ) {
                                dest.push(proxy);
                            }
                        }
                    }
                    continue;
                }
                let direct_source = song_lua_direct_proxy_source(target, proxy_sources);
                if let Some((player_index, source)) = direct_source {
                    if let Some(proxy) = song_lua_direct_proxy(
                        overlay_state,
                        z,
                        source,
                        player_index,
                        out.len(),
                        space_width,
                        space_height,
                    ) && let Some(dest) = direct_proxies.as_deref_mut()
                    {
                        dest.push(proxy);
                    }
                    continue;
                }
                if let Some(actor) =
                    song_lua_proxy_source(target, proxy_sources).and_then(|source| {
                        song_lua_build_proxy_actor_with_scratch(
                            overlay_state,
                            z,
                            source,
                            space_width,
                            space_height,
                            proxy_actor_scratch.as_deref_mut(),
                        )
                    })
                {
                    out.push(actor);
                }
            }
            SongLuaOverlayKind::ActorFrameTexture {
                alpha_buffer,
                depth_buffer,
                preserve_texture,
            } => {
                if !overlay_state.visible {
                    continue;
                }
                let Some(&texture_handle) = topology_index.aft_texture_handles.get(idx) else {
                    continue;
                };
                let size = overlay_state.size.unwrap_or([space_width, space_height]);
                let target_size = [
                    size[0].round().clamp(1.0, u32::MAX as f32) as u32,
                    size[1].round().clamp(1.0, u32::MAX as f32) as u32,
                ];
                let Some(capture_scratch) = aft_capture_scratch.overlay(idx) else {
                    continue;
                };
                let Some(children) = capture_scratch.refill([0.0, 0.0], |source| {
                    // RageTextureRenderTarget::BeginRenderingTo resets the
                    // camera to LoadMenuPerspective(0), whose depth is ±1000.
                    // The default presentation camera clips at ±1 instead.
                    source.push(Actor::CameraPush {
                        view_proj: glam::camera::rh::proj::opengl::orthographic(
                            -size[0] * 0.5,
                            size[0] * 0.5,
                            -size[1] * 0.5,
                            size[1] * 0.5,
                            -1000.0,
                            1000.0,
                        ),
                    });
                    song_lua_capture_children_into(
                        source,
                        overlays,
                        overlay_states,
                        local_overlay_states,
                        order_cache,
                        topology_index,
                        asset_manager,
                        idx,
                        proxy_sources,
                        proxy_actor_scratch.as_deref_mut(),
                        space_width,
                        space_height,
                        effect_time,
                        effect_beat,
                        total_elapsed,
                        capture_states,
                        capture_order_scratch,
                        projected_mesh_scratch,
                    );
                    source.push(Actor::CameraPop);
                }) else {
                    continue;
                };
                targets.push(deadlib_present::actors::RenderTarget {
                    texture_handle,
                    size: target_size,
                    logical_size: size,
                    alpha: *alpha_buffer,
                    depth: *depth_buffer,
                    preserve: *preserve_texture,
                    children,
                });
            }
            SongLuaOverlayKind::AftSprite { .. } => {
                let Some(capture_index) = topology_index
                    .aft_sprite_targets
                    .get(idx)
                    .copied()
                    .and_then(SongLuaOverlayIndex::get)
                else {
                    continue;
                };
                let Some(&texture_handle) = topology_index.aft_texture_handles.get(capture_index)
                else {
                    continue;
                };
                let texture_size = overlay_states
                    .get(capture_index)
                    .and_then(|state| state.size)
                    .unwrap_or([space_width, space_height]);
                if let Some(actors) = build_song_lua_aft_sprite_actor(
                    overlay_state,
                    texture_handle,
                    texture_size,
                    z,
                    space_width,
                    space_height,
                    effect_time,
                    effect_beat,
                    total_elapsed,
                    projected_mesh_scratch.get_mut(idx),
                ) {
                    out.extend(actors);
                }
            }
            _ => {
                if append_song_lua_multi_actor_overlay(
                    out,
                    overlay,
                    overlay_state,
                    asset_manager,
                    z,
                    space_width,
                    space_height,
                    effect_time,
                    effect_beat,
                    total_elapsed,
                    projected_mesh_scratch.get_mut(idx),
                )
                .is_none()
                    && let Some(actors) = build_song_lua_overlay_actor_with_scratch(
                        overlay,
                        overlay_state,
                        topology_index.camera_state(overlay_states, idx),
                        asset_manager,
                        z,
                        space_width,
                        space_height,
                        effect_time,
                        effect_beat,
                        total_elapsed,
                        projected_mesh_scratch.get_mut(idx),
                    )
                {
                    out.extend(actors);
                }
            }
        }
    }
}

/// Song-lifetime playback storage, owned by the game/render frame thread.
///
/// `new` sizes layer caches from the compiled chart and reserves field/proxy
/// buffers before play. Texture and text warmup are explicit separate steps.
/// There is no synchronization, eviction, or scan-pruning; storage is released
/// at the song transition. Existing capture pools and geometry caches document
/// their own capacity, overflow, and instrumentation policies. Frame work scales
/// with active chart actors and requested captures, not the whole song history.
#[derive(Default)]
pub struct FrameScratch {
    render_targets: Vec<deadlib_present::actors::RenderTarget>,
    song_lua_overlay_order: SongLuaOverlayOrderCache,
    song_lua_background_visual_layer_orders: Vec<SongLuaOverlayOrderCache>,
    song_lua_foreground_visual_layer_orders: Vec<SongLuaOverlayOrderCache>,
    song_lua_background_layer_activity: SongLuaLayerActivity,
    song_lua_foreground_layer_activity: SongLuaLayerActivity,
    song_lua_proxy_request_index: SongLuaProxyRequestIndex,
    song_lua_background_overlay_topology_indices: Vec<SongLuaOverlayTopologyIndex>,
    song_lua_foreground_proxy_request_indices: Vec<SongLuaProxyRequestIndex>,
    song_lua_aft_capture_scratch: SongLuaAftCaptureScratch,
    song_lua_background_aft_capture_scratch: Vec<SongLuaAftCaptureScratch>,
    song_lua_foreground_aft_capture_scratch: Vec<SongLuaAftCaptureScratch>,
    song_lua_projected_mesh_scratch: Vec<SongLuaProjectedMeshScratch>,
    song_lua_background_projected_mesh_scratch: Vec<Vec<SongLuaProjectedMeshScratch>>,
    song_lua_foreground_projected_mesh_scratch: Vec<Vec<SongLuaProjectedMeshScratch>>,
    song_lua_message_state_cache: Vec<SongLuaMessageStateCache>,
    song_lua_background_layer_message_state_cache: Vec<Vec<SongLuaMessageStateCache>>,
    song_lua_foreground_layer_message_state_cache: Vec<Vec<SongLuaMessageStateCache>>,
    song_lua_player_message_state_cache: [SongLuaMessageStateCache; MAX_PLAYERS],
    song_lua_player_judgment_message_state_cache: [SongLuaMessageStateCache; MAX_PLAYERS],
    song_lua_player_combo_message_state_cache: [SongLuaMessageStateCache; MAX_PLAYERS],
    song_lua_song_foreground_message_state_cache: SongLuaMessageStateCache,
    song_lua_background_song_foreground_message_state_cache: Vec<SongLuaMessageStateCache>,
    song_lua_foreground_song_foreground_message_state_cache: Vec<SongLuaMessageStateCache>,
    song_lua_local_state_scratch: Vec<SongLuaOverlayState>,
    song_lua_overlay_state_scratch: Vec<SongLuaOverlayState>,
    song_lua_background_layer_local_state_scratch: Vec<Vec<SongLuaOverlayState>>,
    song_lua_background_layer_state_scratch: Vec<Vec<SongLuaOverlayState>>,
    song_lua_foreground_layer_local_state_scratch: Vec<Vec<SongLuaOverlayState>>,
    song_lua_foreground_layer_state_scratch: Vec<Vec<SongLuaOverlayState>>,
    song_lua_capture_state_scratch: Vec<SongLuaOverlayState>,
    song_lua_order_scratch: Vec<usize>,
    song_lua_capture_order_scratch: Vec<usize>,
    song_lua_capture_visit_scratch: SongLuaCaptureVisitScratch,
    song_lua_proxy_actor_scratch: Option<SongLuaProxyActorScratch>,
    song_lua_direct_proxies: SongLuaDirectProxies,
    notefield_actor_scratch: [Vec<Actor>; MAX_PLAYERS],
    notefield_flat_draw_scratch: [Vec<FlatDraw>; MAX_PLAYERS],
    notefield_hud_actor_scratch: [Vec<Actor>; MAX_PLAYERS],
    notefield_hud_flat_draw_scratch: [Vec<FlatDraw>; MAX_PLAYERS],
    notefield_camera_cache: [NotefieldCameraCache; MAX_PLAYERS],
    player_actor_assembly_cache: [PlayerActorAssemblyCache; MAX_PLAYERS],
    player_field_camera_cache: [PlayerFieldCameraCache; MAX_PLAYERS],
}

impl FrameScratch {
    pub fn new<P: deadsync_gameplay::GameplayProfileData, S: NoteskinSlot + Clone>(
        gameplay: &GameplayCoreState<P, S>,
    ) -> Self {
        let active_players = gameplay.num_players();
        let notefield_actor_scratch =
            player_scratch(active_players, NOTEFIELD_ACTOR_SCRATCH_CAPACITY);
        let notefield_flat_draw_scratch =
            player_scratch(active_players, NOTEFIELD_ACTOR_SCRATCH_CAPACITY);
        let notefield_hud_actor_scratch =
            player_scratch(active_players, NOTEFIELD_HUD_ACTOR_SCRATCH_CAPACITY);
        let notefield_hud_flat_draw_scratch =
            player_scratch(active_players, NOTEFIELD_HUD_FLAT_DRAW_SCRATCH_CAPACITY);
        let song_lua_visuals = gameplay.song_lua_visuals();
        let song_lua_overlay_order = song_lua_overlay_order_cache_from(
            &song_lua_visuals.overlays,
            &song_lua_visuals.overlay_eases,
        );
        let song_lua_background_visual_layer_orders = song_lua_visuals
            .background_visual_layers
            .iter()
            .map(|layer| song_lua_overlay_order_cache_from(&layer.overlays, &layer.overlay_eases))
            .collect();
        let song_lua_foreground_visual_layer_orders = song_lua_visuals
            .foreground_visual_layers
            .iter()
            .map(|layer| song_lua_overlay_order_cache_from(&layer.overlays, &layer.overlay_eases))
            .collect();
        let mut song_lua_proxy_request_index =
            SongLuaProxyRequestIndex::new(&song_lua_visuals.overlays);
        song_lua_proxy_request_index
            .topology
            .include_camera_eases(&song_lua_visuals.overlays, &song_lua_visuals.overlay_eases);
        let song_lua_background_overlay_topology_indices: Vec<SongLuaOverlayTopologyIndex> =
            song_lua_visuals
                .background_visual_layers
                .iter()
                .map(|layer| {
                    let mut topology = SongLuaOverlayTopologyIndex::new(&layer.overlays);
                    topology.include_camera_eases(&layer.overlays, &layer.overlay_eases);
                    topology
                })
                .collect();
        let song_lua_foreground_proxy_request_indices: Vec<SongLuaProxyRequestIndex> =
            song_lua_visuals
                .foreground_visual_layers
                .iter()
                .map(|layer| {
                    let mut index = SongLuaProxyRequestIndex::new(&layer.overlays);
                    index
                        .topology
                        .include_camera_eases(&layer.overlays, &layer.overlay_eases);
                    index
                })
                .collect();
        let song_lua_direct_proxy_capacity = song_lua_proxy_request_index
            .direct_proxy_capacity(&song_lua_visuals.overlays)
            .saturating_add(
                song_lua_foreground_proxy_request_indices
                    .iter()
                    .zip(song_lua_visuals.foreground_visual_layers.iter())
                    .map(|(index, layer)| index.direct_proxy_capacity(&layer.overlays))
                    .sum::<usize>(),
            );
        let song_lua_aft_capture_scratch = SongLuaAftCaptureScratch::new(
            &song_lua_visuals.overlays,
            &song_lua_proxy_request_index.topology,
        );
        let song_lua_background_aft_capture_scratch: Vec<_> = song_lua_visuals
            .background_visual_layers
            .iter()
            .zip(song_lua_background_overlay_topology_indices.iter())
            .map(|(layer, topology)| SongLuaAftCaptureScratch::new(&layer.overlays, topology))
            .collect();
        let song_lua_foreground_aft_capture_scratch: Vec<_> = song_lua_visuals
            .foreground_visual_layers
            .iter()
            .zip(song_lua_foreground_proxy_request_indices.iter())
            .map(|(layer, index)| SongLuaAftCaptureScratch::new(&layer.overlays, &index.topology))
            .collect();
        let mut proxy_pool_counts =
            song_lua_proxy_pool_counts(&song_lua_visuals.overlays, &song_lua_proxy_request_index);
        for (index, layer) in song_lua_foreground_proxy_request_indices
            .iter()
            .zip(song_lua_visuals.foreground_visual_layers.iter())
        {
            let layer_counts = song_lua_proxy_pool_counts(&layer.overlays, index);
            for class in 0..SONG_LUA_PROXY_POOL_CLASSES {
                proxy_pool_counts[class] =
                    proxy_pool_counts[class].saturating_add(layer_counts[class]);
            }
        }
        let song_lua_proxy_count = proxy_pool_counts.iter().sum::<usize>();
        let song_lua_projected_mesh_scratch =
            song_lua_projected_mesh_scratch_for(&song_lua_visuals.overlays);
        let song_lua_background_projected_mesh_scratch = song_lua_visuals
            .background_visual_layers
            .iter()
            .map(|layer| song_lua_projected_mesh_scratch_for(&layer.overlays))
            .collect();
        let song_lua_foreground_projected_mesh_scratch = song_lua_visuals
            .foreground_visual_layers
            .iter()
            .map(|layer| song_lua_projected_mesh_scratch_for(&layer.overlays))
            .collect();
        let song_lua_message_state_cache =
            vec![SongLuaMessageStateCache::default(); song_lua_visuals.overlays.len()];
        let song_lua_background_layer_message_state_cache = song_lua_visuals
            .background_visual_layers
            .iter()
            .map(|layer| vec![SongLuaMessageStateCache::default(); layer.overlays.len()])
            .collect();
        let song_lua_foreground_layer_message_state_cache = song_lua_visuals
            .foreground_visual_layers
            .iter()
            .map(|layer| vec![SongLuaMessageStateCache::default(); layer.overlays.len()])
            .collect();
        let song_lua_background_song_foreground_message_state_cache = vec![
            SongLuaMessageStateCache::default();
            song_lua_visuals.background_visual_layers.len()
        ];
        let song_lua_foreground_song_foreground_message_state_cache = vec![
            SongLuaMessageStateCache::default();
            song_lua_visuals.foreground_visual_layers.len()
        ];
        let song_lua_max_overlay_count = std::iter::once(song_lua_visuals.overlays.len())
            .chain(
                song_lua_visuals
                    .background_visual_layers
                    .iter()
                    .map(|layer| layer.overlays.len()),
            )
            .chain(
                song_lua_visuals
                    .foreground_visual_layers
                    .iter()
                    .map(|layer| layer.overlays.len()),
            )
            .max()
            .unwrap_or(0);
        let (song_lua_local_state_scratch, song_lua_overlay_state_scratch) =
            song_lua_overlay_initial_state_sets(
                &song_lua_visuals.overlays,
                song_lua_visuals.screen_width,
                song_lua_visuals.screen_height,
            );
        let (
            song_lua_background_layer_local_state_scratch,
            song_lua_background_layer_state_scratch,
        ) = song_lua_visuals
            .background_visual_layers
            .iter()
            .map(|layer| {
                song_lua_overlay_initial_state_sets(
                    &layer.overlays,
                    layer.screen_width,
                    layer.screen_height,
                )
            })
            .unzip();
        let (
            song_lua_foreground_layer_local_state_scratch,
            song_lua_foreground_layer_state_scratch,
        ) = song_lua_visuals
            .foreground_visual_layers
            .iter()
            .map(|layer| {
                song_lua_overlay_initial_state_sets(
                    &layer.overlays,
                    layer.screen_width,
                    layer.screen_height,
                )
            })
            .unzip();
        let song_lua_now = gameplay.current_music_time_display();
        let song_lua_background_layer_activity = SongLuaLayerActivity::new(
            song_lua_visuals
                .background_visual_layers
                .iter()
                .map(|layer| layer.start_second),
            song_lua_now,
        );
        let song_lua_foreground_layer_activity = SongLuaLayerActivity::new(
            song_lua_visuals
                .foreground_visual_layers
                .iter()
                .map(|layer| layer.start_second),
            song_lua_now,
        );
        let target_capacity = std::iter::once(&song_lua_aft_capture_scratch)
            .chain(&song_lua_background_aft_capture_scratch)
            .chain(&song_lua_foreground_aft_capture_scratch)
            .map(|scratch| scratch.slots.iter().filter(|slot| slot.is_some()).count())
            .sum();
        let frame_scratch = Self {
            render_targets: Vec::with_capacity(target_capacity),
            song_lua_overlay_order,
            song_lua_background_visual_layer_orders,
            song_lua_foreground_visual_layer_orders,
            song_lua_background_layer_activity,
            song_lua_foreground_layer_activity,
            song_lua_proxy_request_index,
            song_lua_background_overlay_topology_indices,
            song_lua_foreground_proxy_request_indices,
            song_lua_aft_capture_scratch,
            song_lua_background_aft_capture_scratch,
            song_lua_foreground_aft_capture_scratch,
            song_lua_projected_mesh_scratch,
            song_lua_background_projected_mesh_scratch,
            song_lua_foreground_projected_mesh_scratch,
            song_lua_message_state_cache,
            song_lua_background_layer_message_state_cache,
            song_lua_foreground_layer_message_state_cache,
            song_lua_player_message_state_cache: [SongLuaMessageStateCache::default(); MAX_PLAYERS],
            song_lua_player_judgment_message_state_cache: [SongLuaMessageStateCache::default();
                MAX_PLAYERS],
            song_lua_player_combo_message_state_cache: [SongLuaMessageStateCache::default();
                MAX_PLAYERS],
            song_lua_song_foreground_message_state_cache: SongLuaMessageStateCache::default(),
            song_lua_background_song_foreground_message_state_cache,
            song_lua_foreground_song_foreground_message_state_cache,
            song_lua_local_state_scratch,
            song_lua_overlay_state_scratch,
            song_lua_background_layer_local_state_scratch,
            song_lua_background_layer_state_scratch,
            song_lua_foreground_layer_local_state_scratch,
            song_lua_foreground_layer_state_scratch,
            song_lua_capture_state_scratch: Vec::with_capacity(song_lua_max_overlay_count),
            song_lua_order_scratch: Vec::with_capacity(song_lua_max_overlay_count),
            song_lua_capture_order_scratch: Vec::with_capacity(song_lua_max_overlay_count),
            song_lua_capture_visit_scratch: SongLuaCaptureVisitScratch::with_capacity(
                song_lua_max_overlay_count,
            ),
            song_lua_proxy_actor_scratch: (song_lua_proxy_count != 0).then(|| {
                SongLuaProxyActorScratch::with_proxy_counts(active_players, proxy_pool_counts)
            }),
            song_lua_direct_proxies: SongLuaDirectProxies::with_capacity(
                song_lua_direct_proxy_capacity,
            ),
            notefield_actor_scratch,
            notefield_flat_draw_scratch,
            notefield_hud_actor_scratch,
            notefield_hud_flat_draw_scratch,
            notefield_camera_cache: [NotefieldCameraCache::default(); MAX_PLAYERS],
            player_actor_assembly_cache: [PlayerActorAssemblyCache::default(); MAX_PLAYERS],
            player_field_camera_cache: [PlayerFieldCameraCache::default(); MAX_PLAYERS],
        };
        debug_assert_eq!(
            frame_scratch
                .song_lua_background_layer_local_state_scratch
                .len(),
            song_lua_visuals.background_visual_layers.len()
        );
        debug_assert_eq!(
            frame_scratch.song_lua_background_layer_state_scratch.len(),
            song_lua_visuals.background_visual_layers.len()
        );
        debug_assert_eq!(
            frame_scratch
                .song_lua_background_layer_message_state_cache
                .len(),
            song_lua_visuals.background_visual_layers.len()
        );
        debug_assert_eq!(
            frame_scratch
                .song_lua_foreground_layer_local_state_scratch
                .len(),
            song_lua_visuals.foreground_visual_layers.len()
        );
        debug_assert_eq!(
            frame_scratch.song_lua_foreground_layer_state_scratch.len(),
            song_lua_visuals.foreground_visual_layers.len()
        );
        debug_assert_eq!(
            frame_scratch
                .song_lua_foreground_layer_message_state_cache
                .len(),
            song_lua_visuals.foreground_visual_layers.len()
        );
        frame_scratch
    }
    pub fn render_targets(&self) -> &[deadlib_present::actors::RenderTarget] {
        &self.render_targets
    }
}

/// Song-lifetime media schedules advanced by the shell on the game thread.
/// Event lists are built once; frame updates advance cursors and reuse the video
/// list. Seeks rewind those cursors. There is no shared access or eviction.
pub struct SongMedia {
    pub sound_paths: Vec<PathBuf>,
    song_lua_sound_events: Vec<SongLuaSoundEvent>,
    next_song_lua_sound_event_ix: usize,
    active_song_lua_video_paths: Vec<PathBuf>,
    static_song_lua_video_path_count: usize,
    // Authored primary foreground start in the gameplay music clock, resolved
    // once at screen entry. Actor animation must exclude the screen lead-in.
    foreground_start_second: f32,
    foreground_media_initialized: bool,
    next_foreground_change_ix: usize,
    current_foreground_path: Option<PathBuf>,
    current_foreground_key: Option<Arc<str>>,
    song_lua_foreground_owner_index: SongLuaForegroundOwnerIndex,
}
impl SongMedia {
    pub fn new<P: deadsync_gameplay::GameplayProfileData, S: NoteskinSlot + Clone>(
        gameplay: &GameplayCoreState<P, S>,
        sound_paths: Vec<PathBuf>,
    ) -> Self {
        let visuals = gameplay.song_lua_visuals();
        let active_song_lua_video_paths = prepare::song_lua_video_paths(visuals);
        let static_song_lua_video_path_count = active_song_lua_video_paths.len();
        let foreground_start_second = gameplay
            .song()
            .foreground_lua_changes
            .iter()
            .find(|change| change.start_beat <= 0.0 && change.path.is_file())
            .map_or(0.0, |change| {
                gameplay.music_time_for_beat(change.start_beat)
            });
        let mut media = Self {
            sound_paths,
            song_lua_sound_events: prepare::song_lua_sound_events(visuals),
            next_song_lua_sound_event_ix: 0,
            active_song_lua_video_paths,
            static_song_lua_video_path_count,
            foreground_start_second,
            foreground_media_initialized: false,
            next_foreground_change_ix: 0,
            current_foreground_path: None,
            current_foreground_key: None,
            song_lua_foreground_owner_index: SongLuaForegroundOwnerIndex::new(visuals),
        };
        media.refresh_foreground(gameplay);
        media
    }
    pub fn refresh_foreground<
        P: deadsync_gameplay::GameplayProfileData,
        S: NoteskinSlot + Clone,
    >(
        &mut self,
        gameplay: &GameplayCoreState<P, S>,
    ) -> bool {
        let beat = gameplay.current_beat();
        let changes = &gameplay.song().foreground_changes;
        let mut next_ix = self.next_foreground_change_ix.min(changes.len());
        if !self.foreground_media_initialized {
            next_ix = changes.partition_point(|change| change.start_beat <= beat);
        } else {
            while next_ix < changes.len() && changes[next_ix].start_beat <= beat {
                next_ix += 1;
            }
            while next_ix > 0 && changes[next_ix - 1].start_beat > beat {
                next_ix -= 1;
            }
            if next_ix == self.next_foreground_change_ix {
                return false;
            }
        }

        let next_path = next_ix
            .checked_sub(1)
            .and_then(|ix| changes.get(ix))
            .map(|change| &change.path)
            .filter(|path| path.is_file())
            .cloned();
        let changed = next_path != self.current_foreground_path;
        self.foreground_media_initialized = true;
        self.next_foreground_change_ix = next_ix;
        if !changed {
            return false;
        }

        self.current_foreground_key = next_path.as_deref().map(deadlib_assets::media_path_key);
        self.current_foreground_path = next_path;
        self.song_lua_foreground_owner_index
            .select(self.current_foreground_path.as_deref());
        self.active_song_lua_video_paths
            .truncate(self.static_song_lua_video_path_count);
        if let Some(path) = self.current_foreground_path.as_ref()
            && deadlib_assets::dynamic::is_dynamic_video_path(path)
            && !self
                .active_song_lua_video_paths
                .iter()
                .any(|existing| existing == path)
        {
            self.active_song_lua_video_paths.push(path.clone());
        }
        true
    }
    pub fn for_each_sound_event(&mut self, previous: f32, now: f32, mut visit: impl FnMut(&Path)) {
        prepare::visit_scheduled_song_lua_sound_events(
            &self.song_lua_sound_events,
            &mut self.next_song_lua_sound_event_ix,
            previous,
            now,
            &mut visit,
        );
    }

    pub fn video_paths(&self) -> &[PathBuf] {
        &self.active_song_lua_video_paths
    }
}

#[derive(Clone, Copy)]
/// Game/session viewing options, including editor overrides; not theme policy.
pub struct FrameOptions {
    pub play_style: deadsync_gameplay::GameplayInputPlayStyle,
    pub player_side: deadsync_gameplay::GameplayInputPlayerSide,
    pub notefield: NotefieldViewOverride,
    pub hide_song_bg: [bool; MAX_PLAYERS],
    pub hide_gameplay_hud: bool,
    pub apply_attacks: bool,
    pub show_song_visuals: bool,
}

/// Fragments retain their original placement around the shared chart layers.
/// Capture visibility and target meanings belong to this compositor.
#[derive(Clone, Copy)]
pub enum ScreenLayer {
    Background,
    ExitPrompt,
    Lobby,
    ExitFade,
    Danger,
    Filter,
    Header,
    Hud,
    System,
}

#[derive(Clone, Copy, Default)]
pub struct FieldLayout {
    pub per_player_fields: [(usize, f32); MAX_PLAYERS],
    pub playfield_center_x: f32,
}

pub struct FieldFrame {
    pub player: usize,
    pub placement: FieldPlacement,
    pub judgment_visible: bool,
    pub combo_visible: bool,
    pub capture: ProxyCaptureRequests,
}

const fn hidden_gameplay_hud_layers(
    hide_gameplay_hud: bool,
    song_lua_hidden_layers: [bool; 2],
    captured_layers: [bool; 2],
) -> [bool; 2] {
    [
        hide_gameplay_hud || (song_lua_hidden_layers[0] && !captured_layers[0]),
        hide_gameplay_hud || (song_lua_hidden_layers[1] && !captured_layers[1]),
    ]
}

/// Compose chart actors around theme fragments using the shared target semantics.
/// The immutable simulation is never advanced here. Keep `frame_scratch` and
/// `actors` alive while consuming the returned segments; reuse both next frame.
/// This state machine deliberately keeps capture ordering and draw ordering in
/// one place, since moving a fragment across a capture boundary changes charts.
pub fn compose_frame<P: deadsync_gameplay::GameplayProfileData, S: NoteskinSlot + Clone>(
    actors: &mut Vec<Actor>,
    state: &GameplayCoreState<P, S>,
    media: &SongMedia,
    frame_scratch: &mut FrameScratch,
    asset_manager: &AssetManager,
    view: FrameOptions,
    mut draw_layer: impl FnMut(ScreenLayer, &mut Vec<Actor>, FieldLayout),
    mut draw_field: impl FnMut(
        FieldFrame,
        &mut [NotefieldCameraCache; MAX_PLAYERS],
        &mut Vec<Actor>,
        &mut Vec<FlatDraw>,
        &mut Vec<Actor>,
        &mut Vec<FlatDraw>,
    ) -> deadsync_notefield::BuiltNotefield,
) -> GameplayActorSegments {
    let FrameScratch {
        render_targets,
        song_lua_overlay_order,
        song_lua_background_visual_layer_orders,
        song_lua_foreground_visual_layer_orders,
        song_lua_background_layer_activity,
        song_lua_foreground_layer_activity,
        song_lua_proxy_request_index,
        song_lua_background_overlay_topology_indices,
        song_lua_foreground_proxy_request_indices,
        song_lua_aft_capture_scratch,
        song_lua_background_aft_capture_scratch,
        song_lua_foreground_aft_capture_scratch,
        song_lua_projected_mesh_scratch,
        song_lua_background_projected_mesh_scratch,
        song_lua_foreground_projected_mesh_scratch,
        song_lua_message_state_cache,
        song_lua_background_layer_message_state_cache,
        song_lua_foreground_layer_message_state_cache,
        song_lua_player_message_state_cache,
        song_lua_player_judgment_message_state_cache,
        song_lua_player_combo_message_state_cache,
        song_lua_song_foreground_message_state_cache,
        song_lua_background_song_foreground_message_state_cache,
        song_lua_foreground_song_foreground_message_state_cache,
        song_lua_local_state_scratch,
        song_lua_overlay_state_scratch,
        song_lua_background_layer_local_state_scratch,
        song_lua_background_layer_state_scratch,
        song_lua_foreground_layer_local_state_scratch,
        song_lua_foreground_layer_state_scratch,
        song_lua_capture_state_scratch,
        song_lua_order_scratch,
        song_lua_capture_order_scratch,
        song_lua_capture_visit_scratch,
        song_lua_proxy_actor_scratch,
        song_lua_direct_proxies,
        notefield_actor_scratch,
        notefield_flat_draw_scratch,
        notefield_hud_actor_scratch,
        notefield_hud_flat_draw_scratch,
        notefield_camera_cache,
        player_actor_assembly_cache,
        player_field_camera_cache,
    } = frame_scratch;
    render_targets.clear();
    if let Some(scratch) = song_lua_proxy_actor_scratch.as_mut() {
        scratch.begin_frame();
    }
    song_lua_direct_proxies.clear();

    let notefield_view = view.notefield;
    let hide_gameplay_hud = view.hide_gameplay_hud;
    let apply_attacks = view.apply_attacks;
    let show_song_visuals = view.show_song_visuals;
    actors.reserve(96);
    let play_style = view.play_style;
    let player_side = view.player_side;
    let runtime_player_is_p2 =
        deadsync_gameplay::gameplay_runtime_player_is_p2(play_style, player_side);
    let song_lua_visuals = state.song_lua_visuals();
    let hidden_song_layers = if show_song_visuals {
        song_lua_visuals.hidden_screen_layers
    } else {
        [false; 2]
    };
    let song_lua_space_width = song_lua_overlay_space_width(state);
    let song_lua_space_height = song_lua_overlay_space_height(state);
    let song_lua_now = state.current_music_time_display();
    song_lua_overlay_state_sets_from_into(
        song_lua_now,
        &song_lua_visuals.overlays,
        &song_lua_visuals.overlay_events,
        &song_lua_visuals.overlay_eases,
        &song_lua_visuals.overlay_ease_ranges,
        song_lua_visuals.screen_width,
        song_lua_visuals.screen_height,
        song_lua_overlay_order,
        song_lua_message_state_cache,
        song_lua_local_state_scratch,
        song_lua_overlay_state_scratch,
    );
    apply_song_lua_taps(
        &state.display.visual_feedback,
        state.cols_per_player(),
        state.total_elapsed_in_screen(),
        song_lua_now,
        &song_lua_visuals.overlays,
        song_lua_overlay_order,
        song_lua_local_state_scratch,
        song_lua_overlay_state_scratch,
        [
            song_lua_visuals.screen_width,
            song_lua_visuals.screen_height,
        ],
    );
    let song_lua_background_active_layers: &[usize] = if show_song_visuals {
        song_lua_background_layer_activity.sync(song_lua_now)
    } else {
        &[]
    };
    let song_lua_foreground_active_layers: &[usize] = if show_song_visuals {
        song_lua_foreground_layer_activity.sync(song_lua_now)
    } else {
        &[]
    };
    for &layer_idx in song_lua_background_active_layers {
        let layer = &song_lua_visuals.background_visual_layers[layer_idx];
        let local_states = &mut song_lua_background_layer_local_state_scratch[layer_idx];
        let layer_states = &mut song_lua_background_layer_state_scratch[layer_idx];
        let message_caches = &mut song_lua_background_layer_message_state_cache[layer_idx];
        song_lua_overlay_state_sets_from_into(
            song_lua_now,
            &layer.overlays,
            &layer.overlay_events,
            &layer.overlay_eases,
            &layer.overlay_ease_ranges,
            layer.screen_width,
            layer.screen_height,
            &mut song_lua_background_visual_layer_orders[layer_idx],
            message_caches,
            local_states,
            layer_states,
        );
        apply_song_lua_taps(
            &state.display.visual_feedback,
            state.cols_per_player(),
            state.total_elapsed_in_screen(),
            song_lua_now,
            &layer.overlays,
            &mut song_lua_background_visual_layer_orders[layer_idx],
            local_states,
            layer_states,
            [layer.screen_width, layer.screen_height],
        );
    }
    for &layer_idx in song_lua_foreground_active_layers {
        let layer = &song_lua_visuals.foreground_visual_layers[layer_idx];
        let local_states = &mut song_lua_foreground_layer_local_state_scratch[layer_idx];
        let layer_states = &mut song_lua_foreground_layer_state_scratch[layer_idx];
        let message_caches = &mut song_lua_foreground_layer_message_state_cache[layer_idx];
        song_lua_overlay_state_sets_from_into(
            song_lua_now,
            &layer.overlays,
            &layer.overlay_events,
            &layer.overlay_eases,
            &layer.overlay_ease_ranges,
            layer.screen_width,
            layer.screen_height,
            &mut song_lua_foreground_visual_layer_orders[layer_idx],
            message_caches,
            local_states,
            layer_states,
        );
        apply_song_lua_taps(
            &state.display.visual_feedback,
            state.cols_per_player(),
            state.total_elapsed_in_screen(),
            song_lua_now,
            &layer.overlays,
            &mut song_lua_foreground_visual_layer_orders[layer_idx],
            local_states,
            layer_states,
            [layer.screen_width, layer.screen_height],
        );
    }
    let mut proxy_analysis = if show_song_visuals {
        song_lua_proxy_request_analysis_indexed(
            &song_lua_visuals.overlays,
            song_lua_overlay_state_scratch,
            song_lua_proxy_request_index,
            song_lua_capture_visit_scratch,
        )
    } else {
        SongLuaProxyRequestAnalysis::default()
    };
    for &layer_idx in song_lua_foreground_active_layers {
        let layer = &song_lua_visuals.foreground_visual_layers[layer_idx];
        let layer_states = &song_lua_foreground_layer_state_scratch[layer_idx];
        let request_index = &song_lua_foreground_proxy_request_indices[layer_idx];
        song_lua_merge_proxy_analysis(
            &mut proxy_analysis,
            song_lua_proxy_request_analysis_indexed(
                &layer.overlays,
                layer_states,
                request_index,
                song_lua_capture_visit_scratch,
            ),
        );
    }
    let proxy_requests = proxy_analysis.all;
    // ActorProxy::DrawPrimitives temporarily makes its target visible. Hidden
    // originals still need populated HUD sources when a proxy draws them.
    let [hide_underlay_hud, hide_overlay_hud] = hidden_gameplay_hud_layers(
        hide_gameplay_hud,
        hidden_song_layers,
        [proxy_requests.underlay, proxy_requests.overlay],
    );
    let mut covering_proxy_requests = if show_song_visuals {
        song_lua_covering_capture_requests(
            &song_lua_visuals.overlays,
            song_lua_local_state_scratch,
            song_lua_overlay_state_scratch,
            song_lua_proxy_request_index,
            song_lua_space_width,
            song_lua_space_height,
            song_lua_capture_visit_scratch,
        )
    } else {
        SongLuaScreenProxyRequests::default()
    };
    for &layer_idx in song_lua_foreground_active_layers {
        let layer = &song_lua_visuals.foreground_visual_layers[layer_idx];
        let layer_states = &song_lua_foreground_layer_state_scratch[layer_idx];
        let request_index = &song_lua_foreground_proxy_request_indices[layer_idx];
        let covering = song_lua_covering_capture_requests(
            &layer.overlays,
            &song_lua_foreground_layer_local_state_scratch[layer_idx],
            layer_states,
            request_index,
            layer.screen_width.max(1.0),
            layer.screen_height.max(1.0),
            song_lua_capture_visit_scratch,
        );
        song_lua_merge_proxy_requests(&mut covering_proxy_requests, covering);
    }
    let retain_underlay_original = !song_lua_visuals.hidden_screen_layers[0]
        && !proxy_requests.hide_underlay
        && !covering_proxy_requests.underlay;
    let retain_overlay_original = !song_lua_visuals.hidden_screen_layers[1]
        && !proxy_requests.hide_overlay
        && !covering_proxy_requests.overlay;
    let direct_player_candidates: [bool; MAX_PLAYERS] = std::array::from_fn(|player| {
        proxy_analysis.root_players[player] != 0
            && !proxy_analysis.captured.players[player].player
            && !proxy_analysis.captured.players[player].note_field
            && !proxy_analysis.captured.players[player].judgment
            && !proxy_analysis.captured.players[player].combo
            && !notefield_view.edit_beat_bars
    });
    let direct_note_field_candidates: [bool; MAX_PLAYERS] = std::array::from_fn(|player| {
        proxy_analysis.root_note_fields[player] != 0
            && !proxy_analysis.captured.players[player].note_field
            && (!proxy_requests.players[player].player || direct_player_candidates[player])
    });
    let direct_judgment_candidates: [bool; MAX_PLAYERS] = std::array::from_fn(|player| {
        proxy_analysis.root_judgments[player] != 0
            && !proxy_analysis.captured.players[player].judgment
            && (!proxy_requests.players[player].player || direct_player_candidates[player])
    });
    let direct_combo_candidates: [bool; MAX_PLAYERS] = std::array::from_fn(|player| {
        proxy_analysis.root_combos[player] != 0
            && !proxy_analysis.captured.players[player].combo
            && (!proxy_requests.players[player].player || direct_player_candidates[player])
    });
    let mut underlay_proxy_source = proxy_requests
        .underlay
        .then_some(SongLuaActorSegments::new());
    let mut overlay_proxy_source = proxy_requests
        .overlay
        .then_some(SongLuaActorSegments::new());
    // --- Background and Filter ---
    let underlay_start = actors.len();
    if show_song_visuals {
        draw_layer(ScreenLayer::Background, actors, FieldLayout::default());
    }
    for &layer_idx in song_lua_background_active_layers {
        let layer = &song_lua_visuals.background_visual_layers[layer_idx];
        let local_states = &song_lua_background_layer_local_state_scratch[layer_idx];
        let layer_states = &song_lua_background_layer_state_scratch[layer_idx];
        let Some(order_cache) = song_lua_background_visual_layer_orders.get_mut(layer_idx) else {
            continue;
        };
        let Some(topology_index) = song_lua_background_overlay_topology_indices.get_mut(layer_idx)
        else {
            continue;
        };
        let Some(aft_capture_scratch) = song_lua_background_aft_capture_scratch.get_mut(layer_idx)
        else {
            continue;
        };
        let Some(projected_mesh_scratch) =
            song_lua_background_projected_mesh_scratch.get_mut(layer_idx)
        else {
            continue;
        };
        let song_foreground_state = song_lua_song_foreground_state_from(
            song_lua_now,
            &layer.song_foreground,
            layer.song_foreground_events.as_slice(),
            &mut song_lua_background_song_foreground_message_state_cache[layer_idx],
        );
        push_song_lua_layer_actors(
            actors,
            render_targets,
            &layer.overlays,
            order_cache,
            topology_index,
            local_states,
            layer_states,
            song_foreground_state,
            &SongLuaScreenProxySources::default(),
            None,
            None,
            asset_manager,
            layer.screen_width.max(1.0),
            layer.screen_height.max(1.0),
            song_lua_now,
            state.current_beat(),
            state.total_elapsed_in_screen(),
            song_lua_order_scratch,
            song_lua_capture_state_scratch,
            song_lua_capture_order_scratch,
            aft_capture_scratch,
            projected_mesh_scratch,
            SONG_LUA_BACKGROUND_DEPTH,
        );
    }
    song_lua_capture_new_actors(
        &mut underlay_proxy_source,
        actors,
        underlay_start,
        song_lua_proxy_actor_scratch.as_mut(),
        retain_underlay_original,
    );
    let cover_alpha = |player_idx: usize| -> f32 {
        if player_idx >= state.num_players() {
            return 0.0;
        }
        let profile_cover = f32::from(view.hide_song_bg[player_idx]);
        let attack_cover = if apply_attacks {
            state
                .effective_visibility_effects_for_player(player_idx)
                .cover
        } else {
            0.0
        };
        profile_cover.max(attack_cover).clamp(0.0, 1.0)
    };
    let left_cover = cover_alpha(0);
    let right_cover = if state.num_players() > 1 {
        cover_alpha(1)
    } else {
        left_cover
    };
    let sw = screen_width();
    let sh = screen_height();
    let cx = screen_center_x();
    if left_cover > 0.0 || right_cover > 0.0 {
        if (left_cover - right_cover).abs() <= 0.001 {
            actors.push(deadlib_present::__act_from_builder!((
                align(0.0, 0.0): xy(0.0, 0.0):
                zoomto(sw, sh):
                diffuse(0.0, 0.0, 0.0, left_cover.max(right_cover)):
                z(-99)
            ) deadlib_assets::SpriteBuilder::solid()));
        } else {
            actors.push(deadlib_present::__act_from_builder!((
                align(0.0, 0.0): xy(0.0, 0.0):
                zoomto(cx, sh):
                faderight(0.1):
                diffuse(0.0, 0.0, 0.0, left_cover):
                z(-99)
            ) deadlib_assets::SpriteBuilder::solid()));
            actors.push(deadlib_present::__act_from_builder!((
                align(0.0, 0.0): xy(cx, 0.0):
                zoomto(sw - cx, sh):
                fadeleft(0.1):
                diffuse(0.0, 0.0, 0.0, right_cover):
                z(-99)
            ) deadlib_assets::SpriteBuilder::solid()));
        }
    }

    // Native exit prompts participate in the Overlay capture target.
    if !hide_overlay_hud {
        let overlay_start = actors.len();
        draw_layer(ScreenLayer::ExitPrompt, actors, FieldLayout::default());
        song_lua_capture_new_actors(
            &mut overlay_proxy_source,
            actors,
            overlay_start,
            song_lua_proxy_actor_scratch.as_mut(),
            retain_overlay_original,
        );
    }

    if !hide_overlay_hud {
        let overlay_start = actors.len();
        draw_layer(ScreenLayer::Lobby, actors, FieldLayout::default());
        song_lua_capture_new_actors(
            &mut overlay_proxy_source,
            actors,
            overlay_start,
            song_lua_proxy_actor_scratch.as_mut(),
            retain_overlay_original,
        );
    }

    // Exit visuals participate in the Overlay capture target.
    let overlay_start = actors.len();
    draw_layer(ScreenLayer::ExitFade, actors, FieldLayout::default());
    song_lua_capture_new_actors(
        &mut overlay_proxy_source,
        actors,
        overlay_start,
        song_lua_proxy_actor_scratch.as_mut(),
        retain_overlay_original,
    );

    let mut build_player_bundle =
        |player_idx: usize, placement: FieldPlacement, requests: SongLuaPlayerProxyRequests| {
            let field_scratch = &mut notefield_actor_scratch[player_idx];
            let flat_draw_scratch = &mut notefield_flat_draw_scratch[player_idx];
            let hud_scratch = &mut notefield_hud_actor_scratch[player_idx];
            let hud_flat_draw_scratch = &mut notefield_hud_flat_draw_scratch[player_idx];
            let player_actor = &song_lua_visuals.player_actors[player_idx];
            let song_lua_now = state.current_music_time_display();
            let (judgment_visible, combo_visible) = if show_song_visuals {
                let judgment_visible = song_lua_child_visible(
                    song_lua_now,
                    &player_actor.judgment,
                    &song_lua_visuals.player_judgment_events[player_idx],
                    &mut song_lua_player_judgment_message_state_cache[player_idx],
                    requests.judgment,
                );
                let combo_visible = song_lua_child_visible(
                    song_lua_now,
                    &player_actor.combo,
                    &song_lua_visuals.player_combo_events[player_idx],
                    &mut song_lua_player_combo_message_state_cache[player_idx],
                    requests.combo,
                );
                (judgment_visible, combo_visible)
            } else {
                (true, true)
            };
            let deadsync_notefield::BuiltNotefield {
                layout_center_x,
                field_camera,
                field_camera_generation,
                field_actors,
                field_draw_range,
                judgment_actors,
                judgment_draw_range,
                combo_actors,
                combo_draw_range,
            } = draw_field(
                FieldFrame {
                    player: player_idx,
                    placement,
                    judgment_visible,
                    combo_visible,
                    capture: ProxyCaptureRequests {
                        player: requests.player && !direct_player_candidates[player_idx],
                        // Whole-player captures consume the same field source, so
                        // materialize compact notes into that cold actor capture.
                        note_field: (requests.note_field
                            && !direct_note_field_candidates[player_idx])
                            || (requests.player && !direct_player_candidates[player_idx]),
                        direct_note_field: requests.note_field
                            && direct_note_field_candidates[player_idx],
                        judgment: requests.judgment && !direct_judgment_candidates[player_idx],
                        direct_judgment: requests.judgment
                            && direct_judgment_candidates[player_idx],
                        combo: requests.combo && !direct_combo_candidates[player_idx],
                        direct_combo: requests.combo && direct_combo_candidates[player_idx],
                    },
                },
                notefield_camera_cache,
                field_scratch,
                flat_draw_scratch,
                hud_scratch,
                hud_flat_draw_scratch,
            );
            let player_state = if show_song_visuals {
                song_lua_player_render_state(
                    state,
                    player_idx,
                    &mut song_lua_player_message_state_cache[player_idx],
                )
            } else {
                SongLuaOverlayState {
                    x: layout_center_x,
                    y: screen_center_y(),
                    ..SongLuaOverlayState::default()
                }
            };
            let player_transform = if apply_attacks {
                state.song_lua_player_transform(player_idx)
            } else {
                deadsync_gameplay::SongLuaPlayerTransform::default()
            };
            let song_lua_active =
                show_song_visuals && !state.song().foreground_lua_changes.is_empty();
            let rotation_x = player_state.rot_x_deg + player_transform.rotation_x;
            let rotation_z = player_state.rot_z_deg + player_transform.rotation_z;
            let rotation_y = player_state.rot_y_deg + player_transform.rotation_y;
            let skew_x = player_transform.skew_x;
            let skew_y = player_transform.skew_y;
            let [player_scale_x, player_scale_y] = song_lua_overlay_axis_scale(player_state);
            let player_scale_z = song_lua_overlay_z_scale(player_state);
            let zoom_x = player_scale_x * player_transform.zoom_x;
            let zoom_y = player_scale_y * player_transform.zoom_y;
            let zoom_z = player_scale_z * player_transform.zoom_z;
            let target_x = song_lua_player_target_x(
                player_transform.x,
                player_state.x,
                layout_center_x,
                notefield_view,
            );
            let target_y = player_transform.y.unwrap_or(player_state.y);
            let z_shift = song_lua_player_layer_z(
                song_lua_active,
                player_actor,
                player_state,
                player_transform.z,
            );
            let player_blend = match player_state.blend {
                SongLuaOverlayBlendMode::Alpha => None,
                SongLuaOverlayBlendMode::Add => Some(BlendMode::Add),
                SongLuaOverlayBlendMode::Multiply => Some(BlendMode::Multiply),
                SongLuaOverlayBlendMode::Subtract => Some(BlendMode::Subtract),
            };
            let capture_transform = SongLuaCaptureTransform {
                z_shift,
                tint: player_state.diffuse,
                blend: player_blend,
                playfield_center_x: layout_center_x,
                target_x,
                target_y,
                rotation_x,
                rotation_z,
                rotation_y,
                skew_x,
                skew_y,
                zoom_x,
                zoom_y,
                zoom_z,
            };
            let direct_x_fold = song_lua_player_x_fold(capture_transform);
            let proxy_field_camera = field_camera;
            let assembly = player_actor_assembly_cache[player_idx].resolve(
                requests.player && !direct_player_candidates[player_idx],
                player_state.visible && !covering_proxy_requests.players[player_idx].player,
                capture_transform,
            );
            let field_camera = match assembly {
                PlayerActorAssembly::DirectTransform {
                    root_camera,
                    field_camera_suffix,
                    ..
                } => Some(player_field_camera_cache[player_idx].resolve(
                    field_camera_generation,
                    player_actor_assembly_cache[player_idx].generation(),
                    field_camera,
                    root_camera,
                    field_camera_suffix,
                )),
                _ => field_camera,
            };
            let direct_player = direct_player_candidates[player_idx]
                && song_lua_player_transform_is_direct_proxy(capture_transform)
                && field_scratch.is_empty()
                && hud_scratch.is_empty();
            let needs_direct_camera = direct_player
                || direct_note_field_candidates[player_idx]
                || direct_judgment_candidates[player_idx]
                || direct_combo_candidates[player_idx];
            let direct_camera_suffix = needs_direct_camera
                .then(|| song_lua_player_camera_suffix(capture_transform))
                .flatten();
            let direct_hud_camera = direct_camera_suffix.map(song_lua_player_root_camera);
            let direct_field_camera = match (proxy_field_camera, direct_camera_suffix) {
                (Some(camera), Some(suffix)) => Some(camera * suffix),
                (None, Some(suffix)) => Some(song_lua_player_root_camera(suffix)),
                (camera, None) => camera,
            };
            let captured_player_source = if requests.player && !direct_player {
                let scratch = song_lua_proxy_actor_scratch
                    .as_mut()
                    .and_then(|scratch| scratch.player(player_idx, SONG_LUA_PLAYER_PROXY_SOURCE))
                    .expect("compiled player proxy has song-lifetime capture backing");
                if direct_player_candidates[player_idx] {
                    capture_flat_player_source(
                        field_scratch,
                        flat_draw_scratch,
                        proxy_field_camera,
                        hud_scratch,
                        hud_flat_draw_scratch,
                        capture_transform,
                        scratch,
                    )
                } else {
                    capture_player_source(field_scratch, hud_scratch, capture_transform, scratch)
                }
            } else {
                None
            };
            let player_source = captured_player_source.map(PreparedProxySource::new);
            let direct_player_source = direct_player
                .then(|| {
                    // Whole-Player capture retains the Player transform in the
                    // source and lets ActorProxy add its authored placement.
                    let target = [0.0, 0.0];
                    let suffix = direct_camera_suffix.unwrap_or(Matrix4::IDENTITY);
                    let root_base = song_lua_player_root_camera(Matrix4::IDENTITY);
                    [
                        SongLuaDirectProxySource {
                            draws: SongLuaDirectDraws::Hud,
                            draw_start: 0,
                            draw_end: hud_flat_draw_scratch.len(),
                            target,
                            tint: capture_transform.tint,
                            x_fold: direct_x_fold,
                            camera: direct_hud_camera,
                            player_camera: Some(SongLuaDirectPlayerCamera {
                                base: root_base,
                                suffix,
                            }),
                        },
                        SongLuaDirectProxySource {
                            draws: SongLuaDirectDraws::Field,
                            draw_start: 0,
                            draw_end: flat_draw_scratch.len(),
                            target,
                            tint: capture_transform.tint,
                            x_fold: direct_x_fold,
                            camera: direct_field_camera,
                            player_camera: Some(SongLuaDirectPlayerCamera {
                                base: proxy_field_camera.unwrap_or(root_base),
                                suffix,
                            }),
                        },
                    ]
                })
                .filter(|sources| {
                    sources
                        .iter()
                        .any(|source| source.draw_start < source.draw_end)
                });
            let direct_note_field = direct_note_field_candidates[player_idx]
                && song_lua_player_transform_is_direct_proxy(capture_transform)
                && field_scratch.is_empty();
            let direct_note_field_source = direct_note_field
                .then_some(field_draw_range.as_ref())
                .flatten()
                .map(|range| SongLuaDirectProxySource {
                    draws: SongLuaDirectDraws::Field,
                    draw_start: range.start,
                    draw_end: range.end,
                    target: [capture_transform.target_x, capture_transform.target_y],
                    tint: capture_transform.tint,
                    x_fold: direct_x_fold,
                    camera: direct_field_camera,
                    player_camera: None,
                });
            let note_field_source = if direct_note_field_candidates[player_idx] {
                (!direct_note_field)
                    .then(|| {
                        let range = field_draw_range.clone().unwrap_or(0..0);
                        let draws = flat_draw_scratch.get(range).unwrap_or(&[]);
                        (draws, field_scratch)
                    })
                    .and_then(|(draws, actors)| {
                        let scratch = song_lua_proxy_actor_scratch
                            .as_mut()?
                            .player(player_idx, SONG_LUA_FIELD_PROXY_SOURCE)?;
                        prepare_field_proxy_source(
                            actors,
                            draws,
                            proxy_field_camera,
                            capture_transform,
                            scratch,
                        )
                    })
            } else {
                field_actors.and_then(|source| {
                    let scratch = song_lua_proxy_actor_scratch
                        .as_mut()?
                        .player(player_idx, SONG_LUA_FIELD_PROXY_SOURCE)?;
                    prepare_proxy_source(
                        source,
                        ProxyCapturePart::Field,
                        capture_transform,
                        scratch,
                    )
                })
            };
            let direct_judgment = direct_judgment_candidates[player_idx]
                && song_lua_player_transform_is_direct_hud_proxy(capture_transform);
            let direct_judgment_source = direct_judgment
                .then_some(judgment_draw_range.as_ref())
                .flatten()
                .map(|range| SongLuaDirectProxySource {
                    draws: SongLuaDirectDraws::Judgment,
                    draw_start: range.start,
                    draw_end: range.end,
                    target: [capture_transform.target_x, capture_transform.target_y],
                    tint: capture_transform.tint,
                    x_fold: direct_x_fold,
                    camera: direct_hud_camera,
                    player_camera: None,
                });
            let judgment_source = if direct_judgment_candidates[player_idx] {
                (!direct_judgment)
                    .then_some(judgment_draw_range.as_ref())
                    .flatten()
                    .and_then(|range| {
                        let scratch = song_lua_proxy_actor_scratch
                            .as_mut()?
                            .player(player_idx, SONG_LUA_JUDGMENT_PROXY_SOURCE)?;
                        prepare_flat_proxy_source(
                            &hud_flat_draw_scratch[range.clone()],
                            capture_transform,
                            scratch,
                        )
                    })
            } else {
                judgment_actors.and_then(|source| {
                    let scratch = song_lua_proxy_actor_scratch
                        .as_mut()?
                        .player(player_idx, SONG_LUA_JUDGMENT_PROXY_SOURCE)?;
                    prepare_proxy_source(source, ProxyCapturePart::Hud, capture_transform, scratch)
                })
            };
            let direct_combo = direct_combo_candidates[player_idx]
                && song_lua_player_transform_is_direct_hud_proxy(capture_transform);
            let direct_combo_source = direct_combo
                .then_some(combo_draw_range.as_ref())
                .flatten()
                .map(|range| SongLuaDirectProxySource {
                    draws: SongLuaDirectDraws::Combo,
                    draw_start: range.start,
                    draw_end: range.end,
                    target: [capture_transform.target_x, capture_transform.target_y],
                    tint: capture_transform.tint,
                    x_fold: direct_x_fold,
                    camera: direct_hud_camera,
                    player_camera: None,
                });
            let combo_source = if direct_combo_candidates[player_idx] {
                (!direct_combo)
                    .then_some(combo_draw_range.as_ref())
                    .flatten()
                    .and_then(|range| {
                        let scratch = song_lua_proxy_actor_scratch
                            .as_mut()?
                            .player(player_idx, SONG_LUA_COMBO_PROXY_SOURCE)?;
                        prepare_flat_proxy_source(
                            &hud_flat_draw_scratch[range.clone()],
                            capture_transform,
                            scratch,
                        )
                    })
            } else {
                combo_actors.and_then(|source| {
                    let scratch = song_lua_proxy_actor_scratch
                        .as_mut()?
                        .player(player_idx, SONG_LUA_COMBO_PROXY_SOURCE)?;
                    prepare_proxy_source(source, ProxyCapturePart::Hud, capture_transform, scratch)
                })
            };
            let proxy_sources = [note_field_source, judgment_source, combo_source];
            (
                layout_center_x,
                player_source,
                direct_player_source,
                proxy_sources,
                direct_note_field_source,
                direct_judgment_source,
                direct_combo_source,
                assembly,
                field_camera,
            )
        };

    let (
        has_p2_actors,
        p1_player_proxy_source,
        p2_player_proxy_source,
        p1_direct_player,
        p2_direct_player,
        p1_proxy_sources,
        p2_proxy_sources,
        p1_direct_note_field,
        p2_direct_note_field,
        p1_direct_judgment,
        p2_direct_judgment,
        p1_direct_combo,
        p2_direct_combo,
        p1_actor_assembly,
        p2_actor_assembly,
        p1_field_camera,
        p2_field_camera,
        playfield_center_x,
        per_player_fields,
    ): (
        bool,
        Option<PreparedProxySource>,
        Option<PreparedProxySource>,
        Option<SongLuaDirectPlayerSource>,
        Option<SongLuaDirectPlayerSource>,
        [Option<PreparedProxySource>; 3],
        [Option<PreparedProxySource>; 3],
        Option<SongLuaDirectProxySource>,
        Option<SongLuaDirectProxySource>,
        Option<SongLuaDirectProxySource>,
        Option<SongLuaDirectProxySource>,
        Option<SongLuaDirectProxySource>,
        Option<SongLuaDirectProxySource>,
        PlayerActorAssembly,
        PlayerActorAssembly,
        Option<Matrix4>,
        Option<Matrix4>,
        f32,
        [(usize, f32); 2],
    ) = match play_style {
        deadsync_gameplay::GameplayInputPlayStyle::Versus
        | deadsync_gameplay::GameplayInputPlayStyle::PumpVersus => {
            let (
                p1_x,
                p1_player_source,
                p1_direct_player,
                p1_sources,
                p1_direct_field,
                p1_direct_judgment,
                p1_direct_combo,
                p1_assembly,
                p1_camera,
            ) = build_player_bundle(0, FieldPlacement::P1, proxy_requests.players[0]);
            let (
                p2_x,
                p2_player_source,
                p2_direct_player,
                p2_sources,
                p2_direct_field,
                p2_direct_judgment,
                p2_direct_combo,
                p2_assembly,
                p2_camera,
            ) = build_player_bundle(1, FieldPlacement::P2, proxy_requests.players[1]);
            (
                true,
                p1_player_source,
                p2_player_source,
                p1_direct_player,
                p2_direct_player,
                p1_sources,
                p2_sources,
                p1_direct_field,
                p2_direct_field,
                p1_direct_judgment,
                p2_direct_judgment,
                p1_direct_combo,
                p2_direct_combo,
                p1_assembly,
                p2_assembly,
                p1_camera,
                p2_camera,
                p1_x,
                [(0, p1_x), (1, p2_x)],
            )
        }
        _ => {
            let placement = if runtime_player_is_p2 {
                FieldPlacement::P2
            } else {
                FieldPlacement::P1
            };
            let (
                nf_x,
                nf_player_source,
                nf_direct_player,
                nf_sources,
                nf_direct_field,
                nf_direct_judgment,
                nf_direct_combo,
                nf_assembly,
                nf_camera,
            ) = build_player_bundle(0, placement, proxy_requests.players[0]);
            (
                false,
                nf_player_source,
                None,
                nf_direct_player,
                None,
                nf_sources,
                [None, None, None],
                nf_direct_field,
                None,
                nf_direct_judgment,
                None,
                nf_direct_combo,
                None,
                nf_assembly,
                PlayerActorAssembly::Hidden,
                nf_camera,
                None,
                nf_x,
                [(0, nf_x), (usize::MAX, 0.0)],
            )
        }
    };
    let replacement_proxy_sources = [
        SongLuaPlayerProxySources {
            player: p1_player_proxy_source
                .as_ref()
                .map(PreparedProxySource::view),
            direct_player: p1_direct_player.is_some(),
            note_field: p1_proxy_sources[0].as_ref().map(PreparedProxySource::view),
            direct_note_field: p1_direct_note_field.is_some(),
            judgment: p1_proxy_sources[1].as_ref().map(PreparedProxySource::view),
            combo: p1_proxy_sources[2].as_ref().map(PreparedProxySource::view),
            direct_combo: p1_direct_combo.is_some(),
        },
        SongLuaPlayerProxySources {
            player: p2_player_proxy_source
                .as_ref()
                .map(PreparedProxySource::view),
            direct_player: p2_direct_player.is_some(),
            note_field: p2_proxy_sources[0].as_ref().map(PreparedProxySource::view),
            direct_note_field: p2_direct_note_field.is_some(),
            judgment: p2_proxy_sources[1].as_ref().map(PreparedProxySource::view),
            combo: p2_proxy_sources[2].as_ref().map(PreparedProxySource::view),
            direct_combo: p2_direct_combo.is_some(),
        },
    ];
    let mut replacement_active_players = if show_song_visuals {
        song_lua_replacement_active_players_indexed(
            &song_lua_visuals.overlays,
            song_lua_overlay_state_scratch,
            &replacement_proxy_sources,
            song_lua_proxy_request_index,
            song_lua_capture_visit_scratch,
        )
    } else {
        [false; MAX_PLAYERS]
    };
    for &layer_idx in song_lua_foreground_active_layers {
        let layer_active = song_lua_replacement_active_players_indexed(
            &song_lua_visuals.foreground_visual_layers[layer_idx].overlays,
            &song_lua_foreground_layer_state_scratch[layer_idx],
            &replacement_proxy_sources,
            &song_lua_foreground_proxy_request_indices[layer_idx],
            song_lua_capture_visit_scratch,
        );
        for (active, layer_active) in replacement_active_players.iter_mut().zip(layer_active) {
            *active |= layer_active;
        }
    }

    let layout = FieldLayout {
        per_player_fields,
        playfield_center_x,
    };
    // Danger visuals participate in the Underlay capture target.
    if !hide_underlay_hud {
        let underlay_start = actors.len();
        draw_layer(ScreenLayer::Danger, actors, layout);
        song_lua_capture_new_actors(
            &mut underlay_proxy_source,
            actors,
            underlay_start,
            song_lua_proxy_actor_scratch.as_mut(),
            retain_underlay_original,
        );
    }

    // Per-player filtering is part of the Underlay capture target.
    let underlay_start = actors.len();
    draw_layer(ScreenLayer::Filter, actors, layout);
    song_lua_capture_new_actors(
        &mut underlay_proxy_source,
        actors,
        underlay_start,
        song_lua_proxy_actor_scratch.as_mut(),
        retain_underlay_original,
    );

    // Header and HUD fragments belong to Underlay, before foreground Lua.
    if !hide_underlay_hud {
        let underlay_start = actors.len();
        draw_layer(ScreenLayer::Header, actors, layout);
        song_lua_capture_new_actors(
            &mut underlay_proxy_source,
            actors,
            underlay_start,
            song_lua_proxy_actor_scratch.as_mut(),
            retain_underlay_original,
        );
    }

    actors.reserve(48);
    let segment_insert = actors.len();
    let mut segment_players = [None; 2];
    if has_p2_actors {
        if !replacement_active_players[1] {
            segment_players[0] = Some(PlayerActorSegment {
                player: 1,
                assembly: p2_actor_assembly,
                field_camera: p2_field_camera,
                manual_hud_draw: show_song_visuals
                    && song_lua_visuals.player_actors[1].manual_hud_draw,
            });
        } else if p2_direct_player.is_none() && p2_direct_note_field.is_none() {
            clear_player_actor_bundle(
                &mut notefield_actor_scratch[1],
                &mut notefield_flat_draw_scratch[1],
                &mut notefield_hud_actor_scratch[1],
                &mut notefield_hud_flat_draw_scratch[1],
            );
        }
    }
    if !replacement_active_players[0] {
        segment_players[1] = Some(PlayerActorSegment {
            player: 0,
            assembly: p1_actor_assembly,
            field_camera: p1_field_camera,
            manual_hud_draw: show_song_visuals && song_lua_visuals.player_actors[0].manual_hud_draw,
        });
    } else if p1_direct_player.is_none() && p1_direct_note_field.is_none() {
        clear_player_actor_bundle(
            &mut notefield_actor_scratch[0],
            &mut notefield_flat_draw_scratch[0],
            &mut notefield_hud_actor_scratch[0],
            &mut notefield_hud_flat_draw_scratch[0],
        );
    }
    if !hide_underlay_hud {
        let underlay_tail_start = actors.len();
        draw_layer(ScreenLayer::Hud, actors, layout);
        song_lua_capture_new_actors(
            &mut underlay_proxy_source,
            actors,
            underlay_tail_start,
            song_lua_proxy_actor_scratch.as_mut(),
            retain_underlay_original,
        );
    }
    let song_foreground_state =
        song_lua_song_foreground_state(state, song_lua_song_foreground_message_state_cache);
    let p1_proxy_slices = [
        p1_proxy_sources[0].as_ref().map(PreparedProxySource::view),
        p1_proxy_sources[1].as_ref().map(PreparedProxySource::view),
        p1_proxy_sources[2].as_ref().map(PreparedProxySource::view),
    ];
    let p2_proxy_slices = [
        p2_proxy_sources[0].as_ref().map(PreparedProxySource::view),
        p2_proxy_sources[1].as_ref().map(PreparedProxySource::view),
        p2_proxy_sources[2].as_ref().map(PreparedProxySource::view),
    ];
    let p1_player_proxy_slice = p1_player_proxy_source
        .as_ref()
        .map(PreparedProxySource::view);
    let p2_player_proxy_slice = p2_player_proxy_source
        .as_ref()
        .map(PreparedProxySource::view);
    let underlay_proxy_slice = underlay_proxy_source.as_deref();
    let overlay_proxy_slice = overlay_proxy_source.as_deref();
    let proxy_sources = SongLuaScreenProxySources {
        players: [
            SongLuaPlayerProxySources {
                player: p1_player_proxy_slice,
                direct_player: p1_direct_player.is_some(),
                note_field: p1_proxy_slices[0],
                direct_note_field: p1_direct_note_field.is_some(),
                judgment: p1_proxy_slices[1],
                combo: p1_proxy_slices[2],
                direct_combo: p1_direct_combo.is_some(),
            },
            SongLuaPlayerProxySources {
                player: p2_player_proxy_slice,
                direct_player: p2_direct_player.is_some(),
                note_field: p2_proxy_slices[0],
                direct_note_field: p2_direct_note_field.is_some(),
                judgment: p2_proxy_slices[1],
                combo: p2_proxy_slices[2],
                direct_combo: p2_direct_combo.is_some(),
            },
        ],
        direct_players: [p1_direct_player, p2_direct_player],
        direct_note_fields: [p1_direct_note_field, p2_direct_note_field],
        direct_judgments: [p1_direct_judgment, p2_direct_judgment],
        direct_combos: [p1_direct_combo, p2_direct_combo],
        underlay: underlay_proxy_slice,
        overlay: overlay_proxy_slice,
    };
    if show_song_visuals {
        push_song_lua_layer_actors(
            actors,
            render_targets,
            &song_lua_visuals.overlays,
            song_lua_overlay_order,
            &mut song_lua_proxy_request_index.topology,
            song_lua_local_state_scratch,
            song_lua_overlay_state_scratch,
            song_foreground_state,
            &proxy_sources,
            Some(&mut *song_lua_direct_proxies),
            song_lua_proxy_actor_scratch.as_mut(),
            asset_manager,
            song_lua_space_width,
            song_lua_space_height,
            state.current_music_time_display(),
            state.current_beat(),
            foreground_elapsed(
                song_lua_now,
                media.foreground_start_second,
                state.music_rate(),
            ),
            song_lua_order_scratch,
            song_lua_capture_state_scratch,
            song_lua_capture_order_scratch,
            song_lua_aft_capture_scratch,
            song_lua_projected_mesh_scratch,
            SONG_LUA_FOREGROUND_DEPTH,
        );
        if let Some(actor) = build_foreground_media(
            state,
            media,
            song_lua_overlay_state_scratch,
            song_lua_background_layer_state_scratch,
            song_lua_foreground_layer_state_scratch,
        ) {
            actors.push(actor);
        }
    }
    for &layer_idx in song_lua_foreground_active_layers {
        let layer = &song_lua_visuals.foreground_visual_layers[layer_idx];
        let local_states = &song_lua_foreground_layer_local_state_scratch[layer_idx];
        let layer_states = &song_lua_foreground_layer_state_scratch[layer_idx];
        let Some(order_cache) = song_lua_foreground_visual_layer_orders.get_mut(layer_idx) else {
            continue;
        };
        let Some(topology_index) = song_lua_foreground_proxy_request_indices.get_mut(layer_idx)
        else {
            continue;
        };
        let Some(aft_capture_scratch) = song_lua_foreground_aft_capture_scratch.get_mut(layer_idx)
        else {
            continue;
        };
        let Some(projected_mesh_scratch) =
            song_lua_foreground_projected_mesh_scratch.get_mut(layer_idx)
        else {
            continue;
        };
        let song_foreground_state = song_lua_song_foreground_state_from(
            song_lua_now,
            &layer.song_foreground,
            layer.song_foreground_events.as_slice(),
            &mut song_lua_foreground_song_foreground_message_state_cache[layer_idx],
        );
        push_song_lua_layer_actors(
            actors,
            render_targets,
            &layer.overlays,
            order_cache,
            &mut topology_index.topology,
            local_states,
            layer_states,
            song_foreground_state,
            &proxy_sources,
            Some(&mut *song_lua_direct_proxies),
            song_lua_proxy_actor_scratch.as_mut(),
            asset_manager,
            layer.screen_width.max(1.0),
            layer.screen_height.max(1.0),
            song_lua_now,
            state.current_beat(),
            foreground_elapsed(song_lua_now, layer.start_second, state.music_rate()),
            song_lua_order_scratch,
            song_lua_capture_state_scratch,
            song_lua_capture_order_scratch,
            aft_capture_scratch,
            projected_mesh_scratch,
            SONG_LUA_FOREGROUND_DEPTH,
        );
    }
    if !hide_gameplay_hud {
        // These are separate top-screen actors in ITGmania. Append them only
        // after every ScreenGameplay/song-local layer so ActorProxy/AFT effects
        // cannot capture or post-process them.
        draw_layer(ScreenLayer::System, actors, layout);
    }
    let direct_proxy_len = song_lua_direct_proxies.len();
    GameplayActorSegments {
        insert: segment_insert,
        players: segment_players,
        direct_proxy_len,
    }
}

fn build_foreground_media<P: deadsync_gameplay::GameplayProfileData, S: NoteskinSlot + Clone>(
    state: &GameplayCoreState<P, S>,
    media: &SongMedia,
    overlay_states: &[SongLuaOverlayState],
    background_layer_states: &[Vec<SongLuaOverlayState>],
    foreground_layer_states: &[Vec<SongLuaOverlayState>],
) -> Option<Actor> {
    let texture_key = media.current_foreground_key.clone()?;
    if media.song_lua_foreground_owner_index.owns(
        state.current_music_time_display(),
        state.song_lua_visuals(),
        overlay_states,
        background_layer_states,
        foreground_layer_states,
    ) {
        return None;
    }
    Some(cover_sprite(
        texture_key,
        screen_center_x(),
        screen_center_y(),
        screen_width(),
        screen_height(),
        1.0,
        1000,
    ))
}

pub fn prewarm_texture_bindings<
    P: deadsync_gameplay::GameplayProfileData,
    S: NoteskinSlot + Clone,
>(
    state: &GameplayCoreState<P, S>,
    scratch: &mut FrameScratch,
    assets: &AssetManager,
) {
    let visuals = state.song_lua_visuals();
    let layers = std::iter::once((
        visuals.overlays.as_slice(),
        scratch.song_lua_projected_mesh_scratch.as_mut_slice(),
    ))
    .chain(
        visuals
            .background_visual_layers
            .iter()
            .zip(
                scratch
                    .song_lua_background_projected_mesh_scratch
                    .iter_mut(),
            )
            .map(|(layer, scratch)| (layer.overlays.as_slice(), scratch.as_mut_slice())),
    )
    .chain(
        visuals
            .foreground_visual_layers
            .iter()
            .zip(
                scratch
                    .song_lua_foreground_projected_mesh_scratch
                    .iter_mut(),
            )
            .map(|(layer, scratch)| (layer.overlays.as_slice(), scratch.as_mut_slice())),
    );
    for (overlays, scratch) in layers {
        for (overlay, scratch) in overlays.iter().zip(scratch) {
            if let SongLuaOverlayKind::Sprite { texture_key, .. } = &overlay.kind {
                scratch.bind_sprite(texture_key, assets.texture_context());
            }
        }
    }
}

pub const NOTEFIELD_ACTOR_SCRATCH_CAPACITY: usize = 384;
pub const NOTEFIELD_HUD_ACTOR_SCRATCH_CAPACITY: usize = 32;
// Worst case: colorful and highlight each emit a background, twelve bands,
// and ten ticks; monochrome emits a background, center, twelve bounds, and
// fifteen ticks; average emits a center plus five ticks; long average adds one.
const ERROR_BAR_HUD_FLAT_DRAW_CAPACITY: usize =
    82 + deadsync_notefield::ERROR_BAR_TEXT_SLOTS_PER_PLAYER as usize;
// Tap plus split overlay and one held-miss plus hold-result sprite per column.
const JUDGMENT_HUD_FLAT_DRAW_CAPACITY: usize = 2 + MAX_COLS * 2;
// One unique active hundred milestone emits four sprites, one unique thousand
// milestone emits two, and the visible combo contributes one prepared digit
// run; gameplay retains at most one milestone of each kind.
const COMBO_HUD_FLAT_DRAW_CAPACITY: usize = 7;
// Five lookahead/current counters, one broken-run counter, one run timer, and
// one mini score indicator.
const ZMOD_HUD_FLAT_DRAW_CAPACITY: usize =
    deadsync_notefield::COUNTER_TEXT_SLOTS_PER_PLAYER as usize + 1;
// One regular cue plus the two legitimate crossover-overlap countdowns.
const CUE_COUNTDOWN_FLAT_DRAW_CAPACITY: usize =
    deadsync_notefield::COLUMN_COUNTDOWN_SLOTS_PER_PLAYER as usize;
const NOTEFIELD_HUD_FLAT_DRAW_SCRATCH_CAPACITY: usize = ERROR_BAR_HUD_FLAT_DRAW_CAPACITY
    + JUDGMENT_HUD_FLAT_DRAW_CAPACITY
    + COMBO_HUD_FLAT_DRAW_CAPACITY
    + ZMOD_HUD_FLAT_DRAW_CAPACITY
    + CUE_COUNTDOWN_FLAT_DRAW_CAPACITY;
pub const PLAYER_ACTOR_SCRATCH_CAPACITY: usize =
    NOTEFIELD_ACTOR_SCRATCH_CAPACITY + NOTEFIELD_HUD_ACTOR_SCRATCH_CAPACITY;
fn white_texture_key() -> Arc<str> {
    static WHITE_TEXTURE_KEY: OnceLock<Arc<str>> = OnceLock::new();
    Arc::clone(WHITE_TEXTURE_KEY.get_or_init(|| Arc::from("__white")))
}

pub fn prewarm_text_layout<P: deadsync_gameplay::GameplayProfileData, S: NoteskinSlot + Clone>(
    cache: &mut TextLayoutCache,
    fonts: &font::FontMap,
    state: &GameplayCoreState<P, S>,
) {
    let visuals = state.song_lua_visuals();
    for overlay in visuals.overlays.iter().chain(
        visuals
            .background_visual_layers
            .iter()
            .chain(&visuals.foreground_visual_layers)
            .flat_map(|layer| &layer.overlays),
    ) {
        if let SongLuaOverlayKind::BitmapText {
            font_name,
            text_changes,
            ..
        } = &overlay.kind
        {
            for (_, text) in text_changes.iter() {
                cache.prewarm_text(fonts, font_name, text, None);
            }
        }
    }
}

/// Access to the production capture paths for differential integration tests.
#[cfg(feature = "test-support")]
#[doc(hidden)]
pub mod actor_capture_test_support {
    use super::*;

    pub const PLAYER_ACTOR_CAPACITY: usize = PLAYER_ACTOR_SCRATCH_CAPACITY;

    pub fn style(actor: Actor, tint: [f32; 4], blend: Option<BlendMode>, z: i16) -> Actor {
        song_lua_style_capture_actor(actor, tint, blend, z)
    }

    pub fn expand(children: &mut Vec<Actor>) {
        song_lua_proxy_expand_retained(children);
    }

    pub fn normalize(children: &mut Vec<Actor>) {
        song_lua_proxy_local_children_in_place(children);
    }
}

#[cfg(feature = "test-support")]
#[path = "playback/actor_conformance.rs"]
pub mod actor_conformance;

fn player_scratch<T>(active_players: usize, capacity: usize) -> [Vec<T>; MAX_PLAYERS] {
    std::array::from_fn(|player| {
        if player < active_players {
            Vec::with_capacity(capacity)
        } else {
            Vec::new()
        }
    })
}
