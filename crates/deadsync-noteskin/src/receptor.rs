use crate::TweenType;

#[derive(Debug, Clone, Copy)]
pub struct ReceptorGlowBehavior {
    pub press_duration: f32,
    pub press_alpha_start: f32,
    pub press_alpha_end: f32,
    pub press_zoom_start: f32,
    pub press_zoom_end: f32,
    pub press_tween: TweenType,
    pub duration: f32,
    pub alpha_start: f32,
    pub alpha_end: f32,
    pub zoom_start: f32,
    pub zoom_end: f32,
    pub tween: TweenType,
    pub blend_add: bool,
}

impl ReceptorGlowBehavior {
    pub fn sample_press(self, timer_remaining: f32) -> (f32, f32) {
        let duration = self.press_duration.max(0.0);
        if duration <= f32::EPSILON {
            return (
                self.press_alpha_end.clamp(0.0, 1.0),
                self.press_zoom_end.max(0.0),
            );
        }
        let elapsed = (duration - timer_remaining.clamp(0.0, duration)).clamp(0.0, duration);
        let progress = elapsed / duration;
        let eased = self.press_tween.ease(progress);
        let alpha =
            (self.press_alpha_end - self.press_alpha_start).mul_add(eased, self.press_alpha_start);
        let zoom =
            (self.press_zoom_end - self.press_zoom_start).mul_add(eased, self.press_zoom_start);
        (alpha.clamp(0.0, 1.0), zoom.max(0.0))
    }

    pub fn sample_lift(
        self,
        timer_remaining: f32,
        start_alpha: f32,
        start_zoom: f32,
    ) -> (f32, f32) {
        let duration = self.duration.max(0.0);
        if duration <= f32::EPSILON {
            return (self.alpha_end.clamp(0.0, 1.0), self.zoom_end.max(0.0));
        }
        let elapsed = (duration - timer_remaining.clamp(0.0, duration)).clamp(0.0, duration);
        let progress = elapsed / duration;
        let eased = self.tween.ease(progress);
        let alpha = (self.alpha_end - start_alpha).mul_add(eased, start_alpha);
        let zoom = (self.zoom_end - start_zoom).mul_add(eased, start_zoom);
        (alpha.clamp(0.0, 1.0), zoom.max(0.0))
    }
}

impl Default for ReceptorGlowBehavior {
    fn default() -> Self {
        Self {
            press_duration: 0.0,
            press_alpha_start: 1.0,
            press_alpha_end: 1.0,
            press_zoom_start: 1.0,
            press_zoom_end: 1.0,
            press_tween: TweenType::Linear,
            duration: 0.2,
            alpha_start: 1.0,
            alpha_end: 0.0,
            zoom_start: 1.0,
            zoom_end: 1.0,
            tween: TweenType::Decelerate,
            blend_add: true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ReceptorStepBehavior {
    pub duration: f32,
    pub zoom_start: f32,
    pub zoom_end: f32,
    pub tween: TweenType,
    pub interrupts: bool,
}

impl ReceptorStepBehavior {
    pub const fn identity() -> Self {
        Self {
            duration: 0.0,
            zoom_start: 1.0,
            zoom_end: 1.0,
            tween: TweenType::Linear,
            interrupts: false,
        }
    }

    pub fn sample_zoom(self, timer_remaining: f32) -> f32 {
        let duration = self.duration.max(0.0);
        if duration <= f32::EPSILON {
            return self.zoom_end.max(0.0);
        }
        let elapsed = (duration - timer_remaining.clamp(0.0, duration)).clamp(0.0, duration);
        let progress = elapsed / duration;
        let eased = self.tween.ease(progress);
        (self.zoom_end - self.zoom_start)
            .mul_add(eased, self.zoom_start)
            .max(0.0)
    }
}

impl Default for ReceptorStepBehavior {
    fn default() -> Self {
        Self {
            duration: 0.11,
            zoom_start: 0.75,
            zoom_end: 1.0,
            tween: TweenType::Linear,
            interrupts: true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ReceptorStepBehaviors {
    none: ReceptorStepBehavior,
    miss: ReceptorStepBehavior,
    windows: [ReceptorStepBehavior; 5],
}

impl ReceptorStepBehaviors {
    pub const fn new(
        none: ReceptorStepBehavior,
        miss: ReceptorStepBehavior,
        windows: [ReceptorStepBehavior; 5],
    ) -> Self {
        Self {
            none,
            miss,
            windows,
        }
    }

    pub fn for_window(self, window: Option<&str>) -> ReceptorStepBehavior {
        match window {
            Some("W1") => self.windows[0],
            Some("W2") => self.windows[1],
            Some("W3") => self.windows[2],
            Some("W4") => self.windows[3],
            Some("W5") => self.windows[4],
            Some("Miss") => self.miss,
            _ => self.none,
        }
    }
}

impl Default for ReceptorStepBehaviors {
    fn default() -> Self {
        Self {
            none: ReceptorStepBehavior::default(),
            miss: ReceptorStepBehavior::identity(),
            windows: [ReceptorStepBehavior::identity(); 5],
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ReceptorReverseState {
    pub base_rotation_z: Option<f32>,
    pub vert_align: Option<f32>,
}

impl ReceptorReverseState {
    #[inline(always)]
    pub fn base_rotation_z(self) -> f32 {
        self.base_rotation_z.unwrap_or(0.0)
    }

    #[inline(always)]
    pub fn vert_align(self) -> f32 {
        self.vert_align.unwrap_or(0.5)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ReceptorReverseBehavior {
    pub reverse_off: ReceptorReverseState,
    pub reverse_on: ReceptorReverseState,
}

impl ReceptorReverseBehavior {
    #[inline(always)]
    pub const fn state(self, reverse: bool) -> ReceptorReverseState {
        if reverse {
            self.reverse_on
        } else {
            self.reverse_off
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ReceptorPulse {
    pub effect_color1: [f32; 4],
    pub effect_color2: [f32; 4],
    pub effect_period: f32,
    pub ramp_to_half: f32,
    pub hold_at_half: f32,
    pub ramp_to_full: f32,
    pub hold_at_full: f32,
    pub hold_at_zero: f32,
    pub effect_offset: f32,
}

impl ReceptorPulse {
    pub fn total_period(&self) -> f32 {
        let mut total = 0.0;
        total += self.ramp_to_half.max(0.0);
        total += self.hold_at_half.max(0.0);
        total += self.ramp_to_full.max(0.0);
        total += self.hold_at_full.max(0.0);
        total += self.hold_at_zero.max(0.0);
        total
    }

    pub fn color_for_beat(&self, beat: f32) -> [f32; 4] {
        let cycle = self.total_period();
        if cycle <= f32::EPSILON {
            return self.effect_color2;
        }
        let phase = (beat + self.effect_offset).rem_euclid(cycle);

        let ramp_to_half = self.ramp_to_half.max(0.0);
        let hold_at_half = self.hold_at_half.max(0.0);
        let ramp_to_full = self.ramp_to_full.max(0.0);
        let hold_at_full = self.hold_at_full.max(0.0);

        let ramp_and_hold_half = ramp_to_half + hold_at_half;
        let through_ramp_full = ramp_and_hold_half + ramp_to_full;
        let through_hold_full = through_ramp_full + hold_at_full;

        let percent = if ramp_to_half > 0.0 && phase < ramp_to_half {
            (phase / ramp_to_half) * 0.5
        } else if phase < ramp_and_hold_half {
            0.5
        } else if ramp_to_full > 0.0 && phase < through_ramp_full {
            ((phase - ramp_and_hold_half) / ramp_to_full).mul_add(0.5, 0.5)
        } else if phase < through_hold_full {
            1.0
        } else {
            0.0
        };

        let mut color = [0.0; 4];
        for (i, channel) in color.iter_mut().enumerate() {
            *channel =
                self.effect_color1[i].mul_add(percent, self.effect_color2[i] * (1.0 - percent));
        }
        color
    }
}

impl Default for ReceptorPulse {
    fn default() -> Self {
        Self {
            effect_color1: [1.0, 1.0, 1.0, 1.0],
            effect_color2: [1.0, 1.0, 1.0, 1.0],
            effect_period: 1.0,
            ramp_to_half: 0.5,
            hold_at_half: 0.0,
            ramp_to_full: 0.5,
            hold_at_full: 0.0,
            hold_at_zero: 0.0,
            effect_offset: 0.0,
        }
    }
}
