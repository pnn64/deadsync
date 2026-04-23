use crate::engine::gfx::{BlendMode, MeshMode, MeshVertex, TMeshCacheKey, TexturedMeshVertex};
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
        effect: anim::EffectState,
    },

    /// Text actor (BitmapText-like)
    Text {
        align: [f32; 2],  // halign/valign pivot inside line box
        offset: [f32; 2], // parent top-left space
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
        wrap_width_pixels: Option<i32>,
        max_width: Option<f32>,
        max_height: Option<f32>,
        max_w_pre_zoom: bool,
        max_h_pre_zoom: bool,
        /// Clip rect in parent TL space: [x, y, w, h].
        clip: Option<[f32; 4]>,
        mask_dest: bool,
        blend: BlendMode,
        effect: anim::EffectState,
    },

    /// Mesh actor (ActorMultiVertex-like)
    Mesh {
        align: [f32; 2],
        offset: [f32; 2],
        size: [SizeSpec; 2],
        vertices: Arc<[MeshVertex]>,
        mode: MeshMode,
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
        vertices: Arc<[TexturedMeshVertex]>,
        geom_cache_key: TMeshCacheKey,
        mode: MeshMode,
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

    /// Camera wrapper: renders all child actors using the provided view-projection matrix.
    /// The matrix is expected to map world coordinates to clip space.
    Camera {
        view_proj: Matrix4,
        children: Vec<Self>,
    },

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
            Self::Sprite { tint, .. } => tint[3] *= alpha,
            Self::Text { color, .. } => color[3] *= alpha,
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
            Self::TexturedMesh { tint, .. } => tint[3] *= alpha,
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
            Self::Camera { children, .. } => {
                for child in children {
                    child.mul_alpha(alpha);
                }
            }
            Self::Shadow { color, child, .. } => {
                color[3] *= alpha;
                child.mul_alpha(alpha);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextAttribute {
    pub start: usize,
    pub length: usize,
    pub color: [f32; 4],
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
    use crate::engine::gfx::{BlendMode, MeshMode};

    fn approx_eq(lhs: f32, rhs: f32) {
        assert!((lhs - rhs).abs() < 1e-6, "expected {rhs}, got {lhs}");
    }

    fn text(color: [f32; 4]) -> Actor {
        Actor::Text {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
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
            wrap_width_pixels: None,
            max_width: None,
            max_height: None,
            max_w_pre_zoom: false,
            max_h_pre_zoom: false,
            clip: None,
            mask_dest: false,
            blend: BlendMode::Alpha,
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
            mode: MeshMode::Triangles,
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
