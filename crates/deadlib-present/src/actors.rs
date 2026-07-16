use crate::anim;
use deadlib_render::{BlendMode, MeshVertex, TMeshCacheKey, TextureHandle, TexturedMeshVertex};
use glam::Mat4 as Matrix4;
use std::cell::{Cell, Ref, RefCell};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

static NEXT_RESOURCE_ARENA_ID: AtomicU32 = AtomicU32::new(1);
static NEXT_RETAINED_FRAME_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorTextureId(pub(crate) u32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActorResourceStats {
    pub texture_hits: u32,
    pub texture_misses: u32,
    pub texture_saturated: u32,
    pub textures: u32,
}

struct ActorResourceStorage {
    textures: Vec<Arc<str>>,
    texture_ids: HashMap<usize, ActorTextureId>,
}

/// Song-lifetime ownership arena for resources referenced by transient actors.
///
/// Owner/thread model: the gameplay state owns this arena and the main thread
/// alone builds and composes actors from it. Capacity is a fixed texture count,
/// populated during gameplay transition prewarm. A miss clones one texture key
/// and inserts it until growth is locked; later misses fall back to their owned
/// source without allocating or scanning.
/// Entries are never evicted and are destroyed with gameplay state at the
/// screen transition. Hit/miss/saturation counters are exposed below. Each
/// miss is one bounded hash insertion; hits are one relaxed atomic load.
pub struct ActorResourceArena {
    arena_id: u32,
    max_textures: usize,
    storage: RefCell<ActorResourceStorage>,
    texture_hits: Cell<u32>,
    texture_misses: Cell<u32>,
    texture_saturated: Cell<u32>,
    growth_locked: Cell<bool>,
}

impl ActorResourceArena {
    pub fn new(max_textures: usize) -> Self {
        let max_textures = max_textures.min((u32::MAX - 1) as usize);
        let arena_id = NEXT_RESOURCE_ARENA_ID
            .fetch_add(1, Ordering::Relaxed)
            .max(1);
        Self {
            arena_id,
            max_textures,
            storage: RefCell::new(ActorResourceStorage {
                textures: Vec::with_capacity(max_textures.min(256)),
                texture_ids: HashMap::with_capacity(max_textures.min(256)),
            }),
            texture_hits: Cell::new(0),
            texture_misses: Cell::new(0),
            texture_saturated: Cell::new(0),
            growth_locked: Cell::new(false),
        }
    }

    #[inline(always)]
    pub fn texture_source(
        &self,
        key: &Arc<str>,
        handle: TextureHandle,
        generation: u64,
        cached_arena_texture: &AtomicU64,
    ) -> SpriteSource {
        let cached = cached_arena_texture.load(Ordering::Relaxed);
        let cached_arena = (cached >> 32) as u32;
        let cached_id = cached as u32;
        if cached_arena == self.arena_id && cached_id != 0 {
            self.texture_hits
                .set(self.texture_hits.get().saturating_add(1));
            return SpriteSource::ArenaTextureHandle {
                id: ActorTextureId(cached_id - 1),
                handle,
                generation,
            };
        }

        let key_ptr = key.as_ptr() as usize;
        let mut storage = self.storage.borrow_mut();
        let id = if let Some(id) = storage.texture_ids.get(&key_ptr).copied() {
            self.texture_hits
                .set(self.texture_hits.get().saturating_add(1));
            id
        } else if !self.growth_locked.get() && storage.textures.len() < self.max_textures {
            let id = ActorTextureId(storage.textures.len() as u32);
            storage.textures.push(Arc::clone(key));
            storage.texture_ids.insert(key_ptr, id);
            self.texture_misses
                .set(self.texture_misses.get().saturating_add(1));
            id
        } else {
            self.texture_saturated
                .set(self.texture_saturated.get().saturating_add(1));
            return SpriteSource::TextureHandle {
                key: Arc::clone(key),
                handle,
                generation,
            };
        };
        drop(storage);
        cached_arena_texture.store(
            (u64::from(self.arena_id) << 32) | u64::from(id.0.saturating_add(1)),
            Ordering::Relaxed,
        );
        SpriteSource::ArenaTextureHandle {
            id,
            handle,
            generation,
        }
    }

    #[inline(always)]
    pub(crate) fn texture_keys(&self) -> Ref<'_, [Arc<str>]> {
        Ref::map(self.storage.borrow(), |storage| storage.textures.as_slice())
    }

    pub fn stats(&self) -> ActorResourceStats {
        ActorResourceStats {
            texture_hits: self.texture_hits.get(),
            texture_misses: self.texture_misses.get(),
            texture_saturated: self.texture_saturated.get(),
            textures: self.storage.borrow().textures.len().min(u32::MAX as usize) as u32,
        }
    }

    pub fn reset_stats(&self) {
        self.texture_hits.set(0);
        self.texture_misses.set(0);
        self.texture_saturated.set(0);
    }

    pub fn lock_growth(&self) {
        self.growth_locked.set(true);
    }
}

impl Default for ActorResourceArena {
    fn default() -> Self {
        Self::new(4096)
    }
}

#[derive(Clone, Debug)]
pub enum Background {
    Color([f32; 4]),
    #[allow(dead_code)]
    Texture(&'static str),
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub enum SizeSpec {
    Px(f32),
    Fill,
}

/// A sprite can be sourced from either a texture or a solid color.
/// For `Solid`, the final color is `tint` (no sampling).
#[derive(Clone, Debug)]
pub enum SpriteSource {
    TextureStatic(&'static str),
    TextureStaticHandle {
        key: &'static str,
        handle: TextureHandle,
        generation: u64,
    },
    TextureHandle {
        key: Arc<str>,
        handle: TextureHandle,
        generation: u64,
    },
    ArenaTextureHandle {
        id: ActorTextureId,
        handle: TextureHandle,
        generation: u64,
    },
    Texture(Arc<str>),
    Solid,
}

impl SpriteSource {
    #[inline(always)]
    pub const fn static_texture(key: &'static str) -> Self {
        Self::TextureStatic(key)
    }

    #[inline(always)]
    pub fn texture_key(&self) -> Option<&str> {
        match self {
            Self::TextureStatic(key) => Some(key),
            Self::TextureStaticHandle { key, .. } => Some(key),
            Self::TextureHandle { key, .. } => Some(key.as_ref()),
            Self::ArenaTextureHandle { .. } => None,
            Self::Texture(key) => Some(key.as_ref()),
            Self::Solid => None,
        }
    }
}

pub trait IntoTextureKey {
    fn into_texture_key(self) -> Arc<str>;

    #[inline(always)]
    fn into_sprite_source(self) -> SpriteSource
    where
        Self: Sized,
    {
        SpriteSource::Texture(self.into_texture_key())
    }
}

pub struct TextureKeyHandle {
    pub key: Arc<str>,
    pub handle: TextureHandle,
    pub generation: u64,
}

impl IntoTextureKey for TextureKeyHandle {
    #[inline(always)]
    fn into_texture_key(self) -> Arc<str> {
        self.key
    }

    #[inline(always)]
    fn into_sprite_source(self) -> SpriteSource {
        SpriteSource::TextureHandle {
            key: self.key,
            handle: self.handle,
            generation: self.generation,
        }
    }
}

impl IntoTextureKey for Arc<str> {
    #[inline(always)]
    fn into_texture_key(self) -> Arc<str> {
        self
    }
}

impl IntoTextureKey for &Arc<str> {
    #[inline(always)]
    fn into_texture_key(self) -> Arc<str> {
        self.clone()
    }
}

impl IntoTextureKey for String {
    #[inline(always)]
    fn into_texture_key(self) -> Arc<str> {
        Arc::<str>::from(self)
    }
}

impl IntoTextureKey for &String {
    #[inline(always)]
    fn into_texture_key(self) -> Arc<str> {
        Arc::<str>::from(self.as_str())
    }
}

impl IntoTextureKey for &str {
    #[inline(always)]
    fn into_texture_key(self) -> Arc<str> {
        Arc::<str>::from(self)
    }
}

#[derive(Clone, Debug)]
pub enum Actor {
    /// Unified Sprite
    Sprite {
        align: [f32; 2],
        offset: [f32; 2],
        world_z: f32,
        size: [SizeSpec; 2],
        source: SpriteSource,
        tint: [f32; 4],
        #[allow(dead_code)]
        glow: [f32; 4],
        z: i16,
        cell: Option<(u32, u32)>,
        grid: Option<(u32, u32)>,
        uv_rect: Option<[f32; 4]>,
        visible: bool,
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
        mask_source: bool,
        mask_dest: bool,
        rot_x_deg: f32,
        rot_y_deg: f32,
        rot_z_deg: f32,
        local_offset: [f32; 2],
        local_offset_rot_sin_cos: [f32; 2],
        texcoordvelocity: Option<[f32; 2]>,
        animate: bool,
        state_delay: f32,
        scale: [f32; 2],
        shadow_len: [f32; 2],
        shadow_color: [f32; 4],
        effect: anim::EffectState,
    },

    /// Text actor (BitmapText-like)
    Text {
        align: [f32; 2],  // halign/valign pivot inside line box
        offset: [f32; 2], // parent top-left space
        local_transform: Matrix4,
        color: [f32; 4],
        stroke_color: Option<[f32; 4]>,
        #[allow(dead_code)]
        glow: [f32; 4],
        font: &'static str,
        content: TextContent,
        attributes: Vec<TextAttribute>,
        align_text: TextAlign, // talign: left/center/right
        z: i16,
        scale: [f32; 2],
        fit_width: Option<f32>,
        fit_height: Option<f32>,
        line_spacing: Option<i32>,
        wrap_width_pixels: Option<i32>,
        max_width: Option<f32>,
        max_height: Option<f32>,
        max_w_pre_zoom: bool,
        max_h_pre_zoom: bool,
        jitter: bool,
        distortion: f32,
        /// Clip rect in parent TL space: [x, y, w, h].
        clip: Option<[f32; 4]>,
        mask_dest: bool,
        blend: BlendMode,
        shadow_len: [f32; 2],
        shadow_color: [f32; 4],
        effect: anim::EffectState,
    },

    /// Mesh actor (ActorMultiVertex-like)
    Mesh {
        align: [f32; 2],
        offset: [f32; 2],
        size: [SizeSpec; 2],
        vertices: Arc<[MeshVertex]>,
        visible: bool,
        blend: BlendMode,
        z: i16,
    },

    /// Textured mesh actor (model-style triangles with UVs)
    TexturedMesh {
        align: [f32; 2],
        offset: [f32; 2],
        world_z: f32,
        size: [SizeSpec; 2],
        local_transform: Matrix4,
        texture: Arc<str>,
        tint: [f32; 4],
        glow: [f32; 4],
        vertices: Arc<[TexturedMeshVertex]>,
        geom_cache_key: TMeshCacheKey,
        uv_scale: [f32; 2],
        uv_offset: [f32; 2],
        uv_tex_shift: [f32; 2],
        depth_test: bool,
        visible: bool,
        blend: BlendMode,
        z: i16,
    },

    /// Dynamic textured mesh backed by storage retained by its owner across frames.
    /// The owner must not mutate the buffer while any cloned actor is live.
    ReusableTexturedMesh {
        align: [f32; 2],
        offset: [f32; 2],
        world_z: f32,
        size: [SizeSpec; 2],
        local_transform: Matrix4,
        texture: Arc<str>,
        tint: [f32; 4],
        glow: [f32; 4],
        vertices: Arc<Vec<TexturedMeshVertex>>,
        geom_cache_key: TMeshCacheKey,
        uv_scale: [f32; 2],
        uv_offset: [f32; 2],
        uv_tex_shift: [f32; 2],
        depth_test: bool,
        visible: bool,
        blend: BlendMode,
        z: i16,
    },

    /// Frame/group box
    Frame {
        align: [f32; 2],
        offset: [f32; 2],
        size: [SizeSpec; 2],
        children: Vec<Self>,
        background: Option<Background>,
        z: i16,
    },

    /// Frame whose children are shared by capture/proxy render paths.
    SharedFrame {
        align: [f32; 2],
        offset: [f32; 2],
        size: [SizeSpec; 2],
        children: Arc<[Self]>,
        background: Option<Background>,
        z: i16,
        tint: [f32; 4],
        blend: Option<BlendMode>,
    },

    /// Immutable actor children whose composed output may be retained by the
    /// song-local composition cache. The wrapper stays compact and may change
    /// placement, visibility, tint, blend, and z without rebuilding children.
    RetainedFrame {
        align: [f32; 2],
        offset: [f32; 2],
        size: [SizeSpec; 2],
        frame: Arc<RetainedActorFrame>,
        z: i16,
        tint: [f32; 4],
        blend: Option<BlendMode>,
        visible: bool,
    },

    /// Camera wrapper: renders all child actors using the provided view-projection matrix.
    /// The matrix is expected to map world coordinates to clip space.
    Camera {
        view_proj: Matrix4,
        children: Vec<Self>,
    },

    /// Begin a flat camera scope for subsequent sibling actors.
    CameraPush { view_proj: Matrix4 },

    /// End the most recent flat camera scope.
    CameraPop,

    /// Shadow wrapper: draws child's objects once more with an offset and tint,
    /// matching `StepMania`'s `shadowlength*` and `shadowcolor` behavior.
    Shadow {
        len: [f32; 2],    // (x, y) shadow length in screen units
        color: [f32; 4],  // shadow color; alpha multiplies the child's alpha
        child: Box<Self>, // wrapped actor
    },
}

impl Actor {
    fn retained_static(&self) -> bool {
        match self {
            Self::Sprite {
                texcoordvelocity,
                animate,
                effect,
                ..
            } => texcoordvelocity.is_none() && !*animate && effect.mode == anim::EffectMode::None,
            Self::Text { jitter, effect, .. } => !*jitter && effect.mode == anim::EffectMode::None,
            Self::Frame { children, .. } | Self::Camera { children, .. } => {
                children.iter().all(Self::retained_static)
            }
            Self::SharedFrame { children, .. } => children.iter().all(Self::retained_static),
            Self::Shadow { child, .. } => child.retained_static(),
            Self::Mesh { .. }
            | Self::TexturedMesh { .. }
            | Self::ReusableTexturedMesh { .. }
            | Self::RetainedFrame { .. }
            | Self::CameraPush { .. }
            | Self::CameraPop => true,
        }
    }

    pub fn mul_alpha(&mut self, alpha: f32) {
        match self {
            Self::Sprite {
                tint,
                glow,
                shadow_color,
                ..
            } => {
                tint[3] *= alpha;
                glow[3] *= alpha;
                shadow_color[3] *= alpha;
            }
            Self::Text {
                color,
                shadow_color,
                ..
            } => {
                color[3] *= alpha;
                shadow_color[3] *= alpha;
            }
            Self::Mesh { vertices, .. } => {
                let mut out = Vec::with_capacity(vertices.len());
                for vertex in vertices.iter() {
                    let mut color = vertex.color;
                    color[3] *= alpha;
                    out.push(MeshVertex {
                        pos: vertex.pos,
                        color,
                    });
                }
                *vertices = Arc::from(out);
            }
            Self::TexturedMesh { tint, glow, .. }
            | Self::ReusableTexturedMesh { tint, glow, .. } => {
                tint[3] *= alpha;
                glow[3] *= alpha;
            }
            Self::Frame {
                background,
                children,
                ..
            } => {
                if let Some(Background::Color(color)) = background {
                    color[3] *= alpha;
                }
                for child in children {
                    child.mul_alpha(alpha);
                }
            }
            Self::SharedFrame {
                background, tint, ..
            } => {
                if let Some(Background::Color(color)) = background {
                    color[3] *= alpha;
                }
                tint[3] *= alpha;
            }
            Self::RetainedFrame { tint, .. } => tint[3] *= alpha,
            Self::Camera { children, .. } => {
                for child in children {
                    child.mul_alpha(alpha);
                }
            }
            Self::CameraPush { .. } | Self::CameraPop => {}
            Self::Shadow { color, child, .. } => {
                color[3] *= alpha;
                child.mul_alpha(alpha);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActorTreeStats {
    pub total: u32,
    pub sprites: u32,
    pub texts: u32,
    pub meshes: u32,
    pub textured_meshes: u32,
    pub frames: u32,
    pub cameras: u32,
    pub shadows: u32,
    pub text_chars: u32,
}

#[inline(always)]
const fn saturating_u32(value: usize) -> u32 {
    if value > u32::MAX as usize {
        u32::MAX
    } else {
        value as u32
    }
}

pub fn actor_tree_stats(actors: &[Actor]) -> ActorTreeStats {
    fn visit(stats: &mut ActorTreeStats, actor: &Actor) {
        stats.total = stats.total.saturating_add(1);
        match actor {
            Actor::Sprite { .. } => {
                stats.sprites = stats.sprites.saturating_add(1);
            }
            Actor::Text { content, .. } => {
                stats.texts = stats.texts.saturating_add(1);
                stats.text_chars = stats
                    .text_chars
                    .saturating_add(saturating_u32(content.len()));
            }
            Actor::Mesh { .. } => {
                stats.meshes = stats.meshes.saturating_add(1);
            }
            Actor::TexturedMesh { .. } | Actor::ReusableTexturedMesh { .. } => {
                stats.textured_meshes = stats.textured_meshes.saturating_add(1);
            }
            Actor::Frame { children, .. } => {
                stats.frames = stats.frames.saturating_add(1);
                for child in children {
                    visit(stats, child);
                }
            }
            Actor::SharedFrame { children, .. } => {
                stats.frames = stats.frames.saturating_add(1);
                for child in children.iter() {
                    visit(stats, child);
                }
            }
            Actor::RetainedFrame { frame, .. } => {
                stats.frames = stats.frames.saturating_add(1);
                for child in frame.children() {
                    visit(stats, child);
                }
            }
            Actor::Camera { children, .. } => {
                stats.cameras = stats.cameras.saturating_add(1);
                for child in children {
                    visit(stats, child);
                }
            }
            Actor::CameraPush { .. } => {
                stats.cameras = stats.cameras.saturating_add(1);
            }
            Actor::CameraPop => {}
            Actor::Shadow { child, .. } => {
                stats.shadows = stats.shadows.saturating_add(1);
                visit(stats, child);
            }
        }
    }

    let mut stats = ActorTreeStats::default();
    for actor in actors {
        visit(&mut stats, actor);
    }
    stats
}

/// Song-owned immutable presentation fragment.
///
/// The screen state owns this value for one song and may emit cheap
/// `Actor::RetainedFrame` wrappers every frame. Children must not depend on a
/// frame clock; animation belongs in the wrapper's compact live values or in a
/// separate dynamic actor fragment. Composition output is retained outside this
/// value and is explicitly cleared at gameplay prewarm/transition boundaries.
#[derive(Debug)]
pub struct RetainedActorFrame {
    id: u64,
    children: Arc<[Actor]>,
}

impl RetainedActorFrame {
    pub fn new(children: Vec<Actor>) -> Self {
        debug_assert!(
            children.iter().all(Actor::retained_static),
            "retained actor frames cannot contain frame-clock animation"
        );
        Self {
            id: NEXT_RETAINED_FRAME_ID
                .fetch_add(1, Ordering::Relaxed)
                .max(1),
            children: Arc::from(children),
        }
    }

    #[inline(always)]
    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    #[inline(always)]
    pub fn children(&self) -> &[Actor] {
        &self.children
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextAttribute {
    pub start: usize,
    pub length: usize,
    pub color: [f32; 4],
    pub vertex_colors: Option<[[f32; 4]; 4]>,
    pub glow: Option<[f32; 4]>,
}

impl TextAttribute {
    #[inline(always)]
    pub fn colors(self) -> [[f32; 4]; 4] {
        self.vertex_colors.unwrap_or([self.color; 4])
    }
}

#[derive(Clone, Debug)]
pub enum TextContent {
    Static(&'static str),
    Owned(String),
    Shared(Arc<str>),
}

impl TextContent {
    #[inline(always)]
    pub const fn static_str(value: &'static str) -> Self {
        Self::Static(value)
    }

    #[inline(always)]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Static(s) => s,
            Self::Owned(s) => s.as_str(),
            Self::Shared(s) => s.as_ref(),
        }
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.as_str().len()
    }
}

impl Default for TextContent {
    fn default() -> Self {
        Self::Static("")
    }
}

impl From<String> for TextContent {
    fn from(value: String) -> Self {
        Self::Owned(value)
    }
}

impl From<&'static str> for TextContent {
    fn from(value: &'static str) -> Self {
        Self::Static(value)
    }
}

impl From<Arc<str>> for TextContent {
    fn from(value: Arc<str>) -> Self {
        Self::Shared(value)
    }
}

impl From<&Arc<str>> for TextContent {
    fn from(value: &Arc<str>) -> Self {
        Self::Shared(value.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_render::BlendMode;
    use std::sync::atomic::AtomicU64;

    fn approx_eq(lhs: f32, rhs: f32) {
        assert!((lhs - rhs).abs() < 1e-6, "expected {rhs}, got {lhs}");
    }

    fn text(color: [f32; 4]) -> Actor {
        Actor::Text {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            local_transform: Matrix4::IDENTITY,
            color,
            stroke_color: None,
            glow: [0.0, 0.0, 0.0, 0.0],
            font: "test",
            content: TextContent::Static("x"),
            attributes: Vec::new(),
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
            effect: anim::EffectState::default(),
        }
    }

    #[test]
    fn mul_alpha_recurses_through_wrappers() {
        let mut actor = Actor::Frame {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            children: vec![Actor::Shadow {
                len: [0.0, 0.0],
                color: [1.0, 1.0, 1.0, 0.8],
                child: Box::new(text([1.0, 1.0, 1.0, 0.6])),
            }],
            background: Some(Background::Color([0.0, 0.0, 0.0, 0.4])),
            z: 0,
        };

        actor.mul_alpha(0.5);

        let Actor::Frame {
            background: Some(Background::Color(bg)),
            children,
            ..
        } = actor
        else {
            panic!("expected frame actor");
        };
        approx_eq(bg[3], 0.2);

        let Actor::Shadow { color, child, .. } = &children[0] else {
            panic!("expected shadow child");
        };
        approx_eq(color[3], 0.4);

        let Actor::Text { color, .. } = child.as_ref() else {
            panic!("expected text child");
        };
        approx_eq(color[3], 0.3);
    }

    #[test]
    fn mul_alpha_rebuilds_mesh_vertices() {
        let original: Arc<[MeshVertex]> = Arc::from(vec![MeshVertex {
            pos: [4.0, 8.0],
            color: [1.0, 1.0, 1.0, 0.8],
        }]);
        let mut actor = Actor::Mesh {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            vertices: Arc::clone(&original),
            visible: true,
            blend: BlendMode::Alpha,
            z: 0,
        };

        actor.mul_alpha(0.25);

        approx_eq(original[0].color[3], 0.8);

        let Actor::Mesh { vertices, .. } = actor else {
            panic!("expected mesh actor");
        };
        assert!(!Arc::ptr_eq(&vertices, &original));
        approx_eq(vertices[0].color[3], 0.2);
    }

    #[test]
    fn actor_tree_stats_counts_nested_actors() {
        let actors = vec![
            Actor::Frame {
                align: [0.0, 0.0],
                offset: [0.0, 0.0],
                size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
                children: vec![Actor::Shadow {
                    len: [0.0, 0.0],
                    color: [0.0, 0.0, 0.0, 0.5],
                    child: Box::new(text([1.0, 1.0, 1.0, 1.0])),
                }],
                background: None,
                z: 0,
            },
            Actor::CameraPush {
                view_proj: Matrix4::IDENTITY,
            },
        ];

        let stats = actor_tree_stats(&actors);

        assert_eq!(stats.total, 4);
        assert_eq!(stats.frames, 1);
        assert_eq!(stats.shadows, 1);
        assert_eq!(stats.texts, 1);
        assert_eq!(stats.text_chars, 1);
        assert_eq!(stats.cameras, 1);
    }

    #[test]
    fn actor_resource_arena_owns_texture_once() {
        let key: Arc<str> = Arc::from("noteskin/tap");
        let cached = AtomicU64::new(0);
        let arena = ActorResourceArena::new(4);

        let first = arena.texture_source(&key, 17, 3, &cached);
        let second = arena.texture_source(&key, 17, 3, &cached);

        let (
            SpriteSource::ArenaTextureHandle { id: first_id, .. },
            SpriteSource::ArenaTextureHandle { id: second_id, .. },
        ) = (first, second)
        else {
            panic!("warmed texture sources should use arena IDs");
        };
        assert_eq!(first_id, second_id);
        assert_eq!(Arc::strong_count(&key), 2);
        assert_eq!(
            arena.texture_keys()[first_id.0 as usize].as_ref(),
            key.as_ref()
        );
        assert_eq!(
            arena.stats(),
            ActorResourceStats {
                texture_hits: 1,
                texture_misses: 1,
                texture_saturated: 0,
                textures: 1,
            }
        );
    }

    #[test]
    fn actor_resource_arena_saturates_without_eviction() {
        let key: Arc<str> = Arc::from("noteskin/tap");
        let cached = AtomicU64::new(0);
        let arena = ActorResourceArena::new(0);

        let source = arena.texture_source(&key, 17, 3, &cached);

        assert!(matches!(source, SpriteSource::TextureHandle { .. }));
        assert_eq!(Arc::strong_count(&key), 2);
        assert_eq!(arena.stats().texture_saturated, 1);
        assert_eq!(arena.stats().textures, 0);
    }

    #[test]
    fn actor_resource_arena_saturates_new_keys_after_growth_lock() {
        let first_key: Arc<str> = Arc::from("noteskin/tap");
        let second_key: Arc<str> = Arc::from("noteskin/mine");
        let first_cached = AtomicU64::new(0);
        let second_cached = AtomicU64::new(0);
        let arena = ActorResourceArena::new(4);
        let _ = arena.texture_source(&first_key, 17, 3, &first_cached);
        arena.lock_growth();

        let source = arena.texture_source(&second_key, 18, 3, &second_cached);

        assert!(matches!(source, SpriteSource::TextureHandle { .. }));
        assert_eq!(arena.stats().textures, 1);
        assert_eq!(arena.stats().texture_saturated, 1);
    }

    #[test]
    fn cached_texture_id_rebinds_to_new_arena() {
        let key: Arc<str> = Arc::from("noteskin/tap");
        let cached = AtomicU64::new(0);
        let first_arena = ActorResourceArena::new(1);
        let second_arena = ActorResourceArena::new(1);

        let _ = first_arena.texture_source(&key, 17, 3, &cached);
        let source = second_arena.texture_source(&key, 17, 3, &cached);

        assert!(matches!(source, SpriteSource::ArenaTextureHandle { .. }));
        assert_eq!(first_arena.stats().texture_misses, 1);
        assert_eq!(second_arena.stats().texture_misses, 1);
    }
}
