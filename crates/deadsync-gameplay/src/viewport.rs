#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameplayViewport {
    width: f32,
    height: f32,
}

impl GameplayViewport {
    pub const fn design() -> Self {
        Self {
            width: 854.0,
            height: 480.0,
        }
    }

    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width: if width.is_finite() && width > 0.0 {
                width
            } else {
                Self::design().width
            },
            height: if height.is_finite() && height > 0.0 {
                height
            } else {
                Self::design().height
            },
        }
    }

    #[inline(always)]
    pub const fn width(self) -> f32 {
        self.width
    }

    #[inline(always)]
    pub const fn height(self) -> f32 {
        self.height
    }

    #[inline(always)]
    pub const fn center_x(self) -> f32 {
        self.width * 0.5
    }

    #[inline(always)]
    pub const fn center_y(self) -> f32 {
        self.height * 0.5
    }

    #[inline(always)]
    pub fn is_wide(self) -> bool {
        self.width / self.height >= 1.6
    }
}

impl Default for GameplayViewport {
    fn default() -> Self {
        Self::design()
    }
}

pub const RECEPTOR_Y_OFFSET_FROM_CENTER: f32 = -125.0;
pub const RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE: f32 = 145.0;
pub const DRAW_DISTANCE_BEFORE_TARGETS_MULTIPLIER: f32 = 1.5;
pub const DRAW_DISTANCE_AFTER_TARGETS: f32 = 130.0;

#[inline(always)]
pub fn scroll_receptor_y(
    reverse_percent: f32,
    centered_percent: f32,
    normal_y: f32,
    reverse_y: f32,
    centered_y: f32,
) -> f32 {
    let reverse_y = lerp(normal_y, reverse_y, reverse_percent.clamp(0.0, 1.0));
    (centered_y - reverse_y).mul_add(centered_percent, reverse_y)
}

#[inline(always)]
pub fn draw_distance_before_targets(viewport_height: f32, draw_scale: f32) -> f32 {
    viewport_height * DRAW_DISTANCE_BEFORE_TARGETS_MULTIPLIER * draw_scale
}

#[inline(always)]
pub fn draw_distance_after_targets(
    viewport_height: f32,
    draw_scale: f32,
    centered_percent: f32,
) -> f32 {
    lerp(
        DRAW_DISTANCE_AFTER_TARGETS * draw_scale,
        viewport_height * 0.6 * draw_scale,
        centered_percent.clamp(0.0, 1.0),
    )
}

