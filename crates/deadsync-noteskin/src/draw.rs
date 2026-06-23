use std::sync::Arc;

#[derive(Debug, Clone, Copy)]
pub enum TweenType {
    Linear,
    Accelerate,
    Decelerate,
}

impl TweenType {
    pub fn ease(self, progress: f32) -> f32 {
        let t = progress.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::Accelerate => t * t,
            Self::Decelerate => 1.0 - (1.0 - t) * (1.0 - t),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ModelVertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub tex_matrix_scale: [f32; 2],
}

#[derive(Debug, Clone)]
pub struct ModelMesh {
    pub vertices: Arc<[ModelVertex]>,
    pub bounds: [f32; 6], // min_x, min_y, min_z, max_x, max_y, max_z
}

impl ModelMesh {
    #[inline(always)]
    pub fn size(&self) -> [f32; 2] {
        [
            (self.bounds[3] - self.bounds[0]).max(0.0),
            (self.bounds[4] - self.bounds[1]).max(0.0),
        ]
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ModelDrawState {
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    pub zoom: [f32; 3],
    pub tint: [f32; 4],
    pub glow: [f32; 4],
    pub vert_align: f32,
    pub blend_add: bool,
    pub visible: bool,
}

impl Default for ModelDrawState {
    fn default() -> Self {
        Self {
            pos: [0.0, 0.0, 0.0],
            rot: [0.0, 0.0, 0.0],
            zoom: [1.0, 1.0, 1.0],
            tint: [1.0, 1.0, 1.0, 1.0],
            glow: [1.0, 1.0, 1.0, 0.0],
            vert_align: 0.5,
            blend_add: false,
            visible: true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ModelTweenSegment {
    pub start: f32,
    pub duration: f32,
    pub tween: TweenType,
    pub from: ModelDrawState,
    pub to: ModelDrawState,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ModelAutoRotKey {
    pub frame: f32,
    pub z_deg: f32,
}
