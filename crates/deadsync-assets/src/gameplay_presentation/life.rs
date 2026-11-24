//! Animation state for the displayed life meter.

use deadsync_profile as profile_data;

pub const LIFE_BOUNCE_S: f32 = 0.1;
pub const LIFE_SMOOTH_S: f32 = 0.2;
pub const LIFE_PERCENT_FADE_S: f32 = 1.0;

#[derive(Clone, Copy, Debug)]
pub struct LifeMeterVisual {
    from_life: f32,
    target_life: f32,
    tween_age: f32,
    pub hot_age: f32,
    pub is_hot: bool,
    pub vertical_swoosh_alpha: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LifePercentVisual {
    pub outer_w: f32,
    pub inner_w: f32,
    pub alpha: f32,
}

impl LifeMeterVisual {
    pub fn new(life: f32, dead: bool) -> Self {
        let target_life = visible_life(life, dead);
        let is_hot = !dead && target_life >= 1.0;
        Self {
            from_life: target_life,
            target_life,
            tween_age: LIFE_SMOOTH_S,
            hot_age: 0.0,
            is_hot,
            vertical_swoosh_alpha: 0.2,
        }
    }

    pub fn update(&mut self, life: f32, dead: bool, delta_time: f32) {
        let target_life = visible_life(life, dead);
        let is_hot = !dead && target_life >= 1.0;
        let delta_time = finite_nonnegative(delta_time);

        if target_life != self.target_life {
            self.from_life = self.target_life;
            self.vertical_swoosh_alpha = if target_life > self.target_life || target_life >= 1.0 {
                1.0
            } else {
                0.2
            };
            self.target_life = target_life;
            self.tween_age = 0.0;
        } else {
            self.tween_age += delta_time;
        }

        if is_hot != self.is_hot {
            self.is_hot = is_hot;
            self.hot_age = 0.0;
        } else {
            self.hot_age += delta_time;
        }
    }

    pub fn rendered_life(self, lifemeter_type: profile_data::LifeMeterType) -> f32 {
        let (duration, progress) = match lifemeter_type {
            profile_data::LifeMeterType::Surround => {
                (LIFE_SMOOTH_S, smooth_life_p(self.tween_age / LIFE_SMOOTH_S))
            }
            profile_data::LifeMeterType::Standard | profile_data::LifeMeterType::Vertical => (
                LIFE_BOUNCE_S,
                deadlib_present::anim::bouncebegin_p(self.tween_age / LIFE_BOUNCE_S),
            ),
        };
        if self.tween_age >= duration {
            self.target_life
        } else {
            (self.target_life - self.from_life).mul_add(progress, self.from_life)
        }
    }

    pub fn percent_visual(self) -> LifePercentVisual {
        if !self.is_hot {
            return LifePercentVisual {
                outer_w: 44.0,
                inner_w: 42.0,
                alpha: 1.0,
            };
        }
        let t = (self.hot_age / LIFE_PERCENT_FADE_S).clamp(0.0, 1.0);
        LifePercentVisual {
            outer_w: 52.0,
            inner_w: 50.0,
            alpha: 1.0 - t * t,
        }
    }
}

#[inline]
pub const fn visible_life(life: f32, dead: bool) -> f32 {
    if dead || !life.is_finite() {
        0.0
    } else {
        life.clamp(0.0, 1.0)
    }
}

#[inline]
pub const fn finite_nonnegative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

#[inline]
fn smooth_life_p(value: f32) -> f32 {
    let t = value.clamp(0.0, 1.0);
    if t <= 0.5 {
        2.0 * t * t
    } else {
        (2.0 * (1.0 - t)).mul_add(-(1.0 - t), 1.0)
    }
}
