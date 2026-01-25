use crate::core::gfx::BlendMode;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum Background {
    Color([f32; 4]),
    #[allow(dead_code)]
    Texture(&'static str),
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default)]
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
    Texture(String),
    Solid,
}

#[derive(Clone, Debug)]
pub enum Actor {
    /// Unified Sprite
    Sprite {
        align: [f32; 2],
        offset: [f32; 2],
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
        rot_z_deg: f32,
        texcoordvelocity: Option<[f32; 2]>,
        animate: bool,
        state_delay: f32,
        scale: [f32; 2],
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
        align_text: TextAlign, // talign: left/center/right
        z: i16,
        scale: [f32; 2],
        fit_width: Option<f32>,
        fit_height: Option<f32>,
        max_width: Option<f32>,
        max_height: Option<f32>,
        max_w_pre_zoom: bool,
        max_h_pre_zoom: bool,
        /// Clip rect in parent TL space: [x, y, w, h].
        clip: Option<[f32; 4]>,
        blend: BlendMode,
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

    /// Shadow wrapper: draws child's objects once more with an offset and tint,
    /// matching `StepMania`'s `shadowlength*` and `shadowcolor` behavior.
    Shadow {
        len: [f32; 2],     // (x, y) shadow length in screen units
        color: [f32; 4],   // shadow color; alpha multiplies the child's alpha
        child: Box<Self>, // wrapped actor
    },
}

#[derive(Clone, Debug)]
pub enum TextContent {
    Owned(String),
    Shared(Arc<str>),
}

impl TextContent {
    #[inline(always)]
    pub fn as_str(&self) -> &str {
        match self {
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
        Self::Owned(String::new())
    }
}

impl From<String> for TextContent {
    fn from(value: String) -> Self {
        Self::Owned(value)
    }
}

impl From<&String> for TextContent {
    fn from(value: &String) -> Self {
        Self::Owned(value.clone())
    }
}

impl From<&str> for TextContent {
    fn from(value: &str) -> Self {
        Self::Owned(value.to_owned())
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
