use crate::engine::gfx::{BlendMode, MeshVertex, TMeshCacheKey, TextureHandle, TexturedMeshVertex};
use crate::engine::present::anim;
use glam::Mat4 as Matrix4;
use std::sync::Arc;

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
            Self::Texture(key) => Some(key.as_ref()),
            Self::Solid => None,
        }
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
            Self::TexturedMesh { tint, glow, .. } => {
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

/// Host-side normalization of the hot-reload actor boundary.
///
/// When a hot-reloaded cdylib produces a `Vec<Actor>`, any `&'static str`/static
/// key it bakes in (font keys, `TextureStatic*` keys, `Background::Texture`,
/// `TextContent::Static`) points into the *cdylib image*. Those references dangle
/// the moment the generation is unloaded. This pass walks the returned tree once,
/// while the cdylib is still mapped, and re-homes every such key into host-owned
/// memory (host-interned `&'static` or heap `Arc<str>`), so the resulting actors
/// own nothing that lives in the unloadable library.
///
/// This is the generic, screen-agnostic boundary contract: it runs at the single
/// hot dispatch site and covers every present and future hot surface. It is
/// compiled only under the `hot` feature; non-hot builds render in-process with
/// keys already owned by the executable image and never call this.
#[cfg(feature = "hot")]
mod host_intern {
    use std::collections::HashSet;
    use std::sync::{Arc, LazyLock, Mutex};

    static STATIC_KEYS: LazyLock<Mutex<HashSet<&'static str>>> =
        LazyLock::new(|| Mutex::new(HashSet::new()));
    static ARC_KEYS: LazyLock<Mutex<HashSet<Arc<str>>>> =
        LazyLock::new(|| Mutex::new(HashSet::new()));

    /// Return a host-owned `&'static str` equal to `key`. The first time a given
    /// string is seen it is leaked once (`Box::leak`) into a host-owned table; the
    /// distinct set is bounded (font + texture keys — a few dozen), so the leak is
    /// effectively a one-time intern, never per-frame growth.
    pub fn intern_static(key: &str) -> &'static str {
        let mut table = STATIC_KEYS.lock().unwrap();
        if let Some(existing) = table.get(key) {
            return existing;
        }
        let leaked: &'static str = Box::leak(key.to_owned().into_boxed_str());
        table.insert(leaked);
        leaked
    }

    /// Return a host-owned `Arc<str>` equal to `key`, de-duplicated so repeated
    /// keys across frames share one allocation instead of re-allocating each frame.
    pub fn intern_arc(key: &str) -> Arc<str> {
        let mut table = ARC_KEYS.lock().unwrap();
        if let Some(existing) = table.get(key) {
            return Arc::clone(existing);
        }
        let arc: Arc<str> = Arc::from(key);
        table.insert(Arc::clone(&arc));
        arc
    }
}

/// Re-home every cdylib-owned static key in a hot-produced actor slice into
/// host-owned memory. Call this at the hot dispatch boundary, immediately after
/// the cdylib returns its actors and before the host composes or caches anything.
#[cfg(feature = "hot")]
pub fn normalize_hot_actors(actors: &mut [Actor]) {
    for actor in actors {
        actor.rehome_to_host_owned();
    }
}

#[cfg(feature = "hot")]
impl Actor {
    /// Replace any `&'static`/static key that points into the loaded cdylib image
    /// with a host-owned equivalent, recursing through all child-bearing variants.
    ///
    /// The match is intentionally exhaustive (no `_` arm), mirroring `mul_alpha`,
    /// so that any future `Actor` variant or new key-bearing field fails to compile
    /// until it is explicitly normalized here — the boundary can never silently
    /// regress to leaking a cdylib reference.
    fn rehome_to_host_owned(&mut self) {
        match self {
            Self::Sprite { source, .. } => source.rehome_to_host_owned(),
            Self::Text { font, content, .. } => {
                *font = host_intern::intern_static(font);
                if let TextContent::Static(s) = content {
                    *content = TextContent::Shared(host_intern::intern_arc(s));
                }
            }
            Self::Mesh { .. } | Self::TexturedMesh { .. } => {}
            Self::Frame {
                background,
                children,
                ..
            } => {
                Background::rehome_opt(background);
                for child in children {
                    child.rehome_to_host_owned();
                }
            }
            Self::SharedFrame {
                background,
                children,
                ..
            } => {
                Background::rehome_opt(background);
                // `children` is an immutable `Arc<[Self]>`; rebuild it so each child
                // is normalized, then re-share. At the hot boundary these actors are
                // freshly produced each frame, so there is no aliasing to preserve.
                let mut rebuilt: Vec<Self> = children.iter().cloned().collect();
                for child in &mut rebuilt {
                    child.rehome_to_host_owned();
                }
                *children = Arc::from(rebuilt);
            }
            Self::Camera { children, .. } => {
                for child in children {
                    child.rehome_to_host_owned();
                }
            }
            Self::CameraPush { .. } | Self::CameraPop => {}
            Self::Shadow { child, .. } => child.rehome_to_host_owned(),
        }
    }
}

#[cfg(feature = "hot")]
impl SpriteSource {
    fn rehome_to_host_owned(&mut self) {
        match self {
            // Static keys live in the cdylib image -> move to a host-owned `Arc<str>`.
            // For the `*Handle` variants we deliberately drop the cdylib-resolved
            // `handle`/`generation`: they were resolved against the cdylib's *own*
            // duplicated texture registry, so forcing host re-resolution from the key
            // is both safe (no dangling reference, no stale handle) and more correct.
            Self::TextureStatic(key) => {
                *self = SpriteSource::Texture(host_intern::intern_arc(key));
            }
            Self::TextureStaticHandle { key, .. } => {
                *self = SpriteSource::Texture(host_intern::intern_arc(key));
            }
            Self::TextureHandle { key, .. } => {
                *self = SpriteSource::Texture(host_intern::intern_arc(key));
            }
            // `Texture(Arc<str>)` is already a host-heap key the host re-resolves.
            Self::Texture(_) | Self::Solid => {}
        }
    }
}

#[cfg(feature = "hot")]
impl Background {
    fn rehome_opt(background: &mut Option<Background>) {
        if let Some(Background::Texture(key)) = background {
            *key = host_intern::intern_static(key);
        }
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
    use crate::engine::gfx::BlendMode;

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
}

#[cfg(all(test, feature = "hot"))]
mod hot_tests {
    use super::*;
    use crate::engine::gfx::BlendMode;

    fn text(font: &'static str, content: TextContent) -> Actor {
        Actor::Text {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            local_transform: Matrix4::IDENTITY,
            color: [1.0, 1.0, 1.0, 1.0],
            stroke_color: None,
            glow: [0.0, 0.0, 0.0, 0.0],
            font,
            content,
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

    fn sprite(source: SpriteSource) -> Actor {
        Actor::Sprite {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            world_z: 0.0,
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            source,
            tint: [1.0, 1.0, 1.0, 1.0],
            glow: [0.0, 0.0, 0.0, 0.0],
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
            effect: anim::EffectState::default(),
        }
    }

    fn dummy_handle() -> crate::engine::gfx::TextureHandle {
        0
    }

    /// Assert no actor in the tree carries a cdylib-owned static key after
    /// normalization: no `TextureStatic`/`TextureStaticHandle` sprite sources and
    /// no `TextContent::Static` text content remain anywhere.
    fn assert_no_cdylib_static_keys(actor: &Actor) {
        match actor {
            Actor::Sprite { source, .. } => {
                assert!(
                    !matches!(
                        source,
                        SpriteSource::TextureStatic(_)
                            | SpriteSource::TextureStaticHandle { .. }
                            | SpriteSource::TextureHandle { .. }
                    ),
                    "sprite still holds a cdylib-resolved texture source: {source:?}"
                );
            }
            Actor::Text { content, .. } => {
                assert!(
                    !matches!(content, TextContent::Static(_)),
                    "text still holds static content: {content:?}"
                );
            }
            Actor::Frame { children, .. } | Actor::Camera { children, .. } => {
                for child in children {
                    assert_no_cdylib_static_keys(child);
                }
            }
            Actor::SharedFrame { children, .. } => {
                for child in children.iter() {
                    assert_no_cdylib_static_keys(child);
                }
            }
            Actor::Shadow { child, .. } => assert_no_cdylib_static_keys(child),
            Actor::Mesh { .. }
            | Actor::TexturedMesh { .. }
            | Actor::CameraPush { .. }
            | Actor::CameraPop => {}
        }
    }

    #[test]
    fn sprite_static_keys_become_host_arcs() {
        let mut a = sprite(SpriteSource::TextureStatic("dance.png"));
        a.rehome_to_host_owned();
        let Actor::Sprite { source, .. } = &a else {
            panic!("expected sprite");
        };
        match source {
            SpriteSource::Texture(key) => assert_eq!(key.as_ref(), "dance.png"),
            other => panic!("expected Texture(Arc), got {other:?}"),
        }

        let mut b = sprite(SpriteSource::TextureStaticHandle {
            key: "logo.png",
            handle: dummy_handle(),
            generation: 7,
        });
        b.rehome_to_host_owned();
        let Actor::Sprite { source, .. } = &b else {
            panic!("expected sprite");
        };
        match source {
            // handle/generation dropped -> host re-resolves from the key.
            SpriteSource::Texture(key) => assert_eq!(key.as_ref(), "logo.png"),
            other => panic!("expected Texture(Arc), got {other:?}"),
        }

        // A cdylib-resolved `TextureHandle { key: Arc<str>, .. }` is also downgraded
        // to a key-only `Texture`, so no cdylib-resolved handle escapes the boundary.
        let mut c = sprite(SpriteSource::TextureHandle {
            key: Arc::from("hud.png"),
            handle: dummy_handle(),
            generation: 3,
        });
        c.rehome_to_host_owned();
        let Actor::Sprite { source, .. } = &c else {
            panic!("expected sprite");
        };
        match source {
            SpriteSource::Texture(key) => assert_eq!(key.as_ref(), "hud.png"),
            other => panic!("expected Texture(Arc), got {other:?}"),
        }
    }

    #[test]
    fn text_font_interned_and_static_content_shared() {
        let mut a = text("miso", TextContent::Static("Hello"));
        a.rehome_to_host_owned();
        let Actor::Text { font, content, .. } = &a else {
            panic!("expected text");
        };
        assert_eq!(*font, "miso");
        // Font pointer is now host-owned and pointer-stable across interning.
        assert!(std::ptr::eq(*font, host_intern::intern_static("miso")));
        match content {
            TextContent::Shared(s) => assert_eq!(s.as_ref(), "Hello"),
            other => panic!("expected Shared(Arc), got {other:?}"),
        }
    }

    #[test]
    fn background_texture_interned_in_place() {
        let mut bg = Some(Background::Texture("bg.png"));
        Background::rehome_opt(&mut bg);
        let Some(Background::Texture(key)) = bg else {
            panic!("expected texture background");
        };
        assert_eq!(key, "bg.png");
        assert!(std::ptr::eq(key, host_intern::intern_static("bg.png")));
    }

    #[test]
    fn normalize_recurses_through_every_container() {
        let shared_child = sprite(SpriteSource::TextureStatic("shared.png"));
        let mut actors = vec![Actor::Frame {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            background: Some(Background::Texture("frame_bg.png")),
            z: 0,
            children: vec![
                Actor::Camera {
                    view_proj: Matrix4::IDENTITY,
                    children: vec![Actor::Shadow {
                        len: [0.0, 0.0],
                        color: [0.0, 0.0, 0.0, 1.0],
                        child: Box::new(text("camfont", TextContent::Static("deep"))),
                    }],
                },
                Actor::SharedFrame {
                    align: [0.0, 0.0],
                    offset: [0.0, 0.0],
                    size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
                    children: Arc::from(vec![shared_child]),
                    background: Some(Background::Texture("shared_bg.png")),
                    z: 0,
                    tint: [1.0, 1.0, 1.0, 1.0],
                    blend: None,
                },
            ],
        }];

        normalize_hot_actors(&mut actors);

        for actor in &actors {
            assert_no_cdylib_static_keys(actor);
        }

        // The deep Text under Camera>Shadow was reached.
        let Actor::Frame { children, .. } = &actors[0] else {
            panic!("expected frame");
        };
        let Actor::Camera { children: cam, .. } = &children[0] else {
            panic!("expected camera");
        };
        let Actor::Shadow { child, .. } = &cam[0] else {
            panic!("expected shadow");
        };
        assert!(matches!(
            child.as_ref(),
            Actor::Text {
                content: TextContent::Shared(_),
                ..
            }
        ));

        // SharedFrame's Arc children were rebuilt and normalized.
        let Actor::SharedFrame {
            children,
            background: Some(Background::Texture(bg)),
            ..
        } = &children[1]
        else {
            panic!("expected shared frame with texture background");
        };
        assert_eq!(*bg, "shared_bg.png");
        assert!(matches!(
            &children[0],
            Actor::Sprite {
                source: SpriteSource::Texture(_),
                ..
            }
        ));
    }
}
