#[derive(Debug, Clone, Copy)]
pub enum GameplayTween {
    Linear,
    Accelerate,
    Decelerate,
}

impl GameplayTween {
    #[inline(always)]
    pub fn ease(self, progress: f32) -> f32 {
        let t = progress.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::Accelerate => t * t,
            Self::Decelerate => 1.0 - (1.0 - t) * (1.0 - t),
        }
    }
}

#[inline(always)]
fn song_lua_pow_in(t: f32, power: f32) -> f32 {
    t.powf(power)
}

#[inline(always)]
fn song_lua_pow_out(t: f32, power: f32) -> f32 {
    1.0 - (1.0 - t).powf(power)
}

#[inline(always)]
fn song_lua_pow_in_out(t: f32, power: f32) -> f32 {
    if t < 0.5 {
        0.5 * (2.0 * t).powf(power)
    } else {
        1.0 - 0.5 * (2.0 * (1.0 - t)).powf(power)
    }
}

#[inline(always)]
fn song_lua_pow_out_in(t: f32, power: f32) -> f32 {
    if t < 0.5 {
        0.5 * song_lua_pow_out(t * 2.0, power)
    } else {
        0.5 + 0.5 * song_lua_pow_in((t * 2.0) - 1.0, power)
    }
}

fn song_lua_out_bounce(t: f32) -> f32 {
    const N1: f32 = 7.5625;
    const D1: f32 = 2.75;
    if t < 1.0 / D1 {
        N1 * t * t
    } else if t < 2.0 / D1 {
        let t = t - 1.5 / D1;
        N1 * t * t + 0.75
    } else if t < 2.5 / D1 {
        let t = t - 2.25 / D1;
        N1 * t * t + 0.9375
    } else {
        let t = t - 2.625 / D1;
        N1 * t * t + 0.984_375
    }
}

#[inline(always)]
fn song_lua_in_bounce(t: f32) -> f32 {
    1.0 - song_lua_out_bounce(1.0 - t)
}

#[inline(always)]
fn song_lua_in_out_bounce(t: f32) -> f32 {
    if t < 0.5 {
        0.5 * song_lua_in_bounce(t * 2.0)
    } else {
        0.5 + 0.5 * song_lua_out_bounce((t * 2.0) - 1.0)
    }
}

pub fn song_lua_ease_factor(
    easing: Option<&str>,
    t: f32,
    opt1: Option<f32>,
    opt2: Option<f32>,
) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let overshoot = opt1.filter(|v| v.is_finite()).unwrap_or(1.70158);
    let elastic_period = opt1.filter(|v| v.is_finite() && *v > 0.0).unwrap_or(0.3);
    let elastic_tau = std::f32::consts::TAU / elastic_period;
    match easing.unwrap_or("linear") {
        "instant" => 1.0,
        "linear" => t,
        "inQuad" => song_lua_pow_in(t, 2.0),
        "outQuad" => song_lua_pow_out(t, 2.0),
        "inOutQuad" => song_lua_pow_in_out(t, 2.0),
        "outInQuad" => song_lua_pow_out_in(t, 2.0),
        "inCubic" => song_lua_pow_in(t, 3.0),
        "outCubic" => song_lua_pow_out(t, 3.0),
        "inOutCubic" => song_lua_pow_in_out(t, 3.0),
        "outInCubic" => song_lua_pow_out_in(t, 3.0),
        "inQuart" => song_lua_pow_in(t, 4.0),
        "outQuart" => song_lua_pow_out(t, 4.0),
        "inOutQuart" => song_lua_pow_in_out(t, 4.0),
        "outInQuart" => song_lua_pow_out_in(t, 4.0),
        "inQuint" => song_lua_pow_in(t, 5.0),
        "outQuint" => song_lua_pow_out(t, 5.0),
        "inOutQuint" => song_lua_pow_in_out(t, 5.0),
        "outInQuint" => song_lua_pow_out_in(t, 5.0),
        "inSine" => 1.0 - (t * std::f32::consts::FRAC_PI_2).cos(),
        "outSine" => (t * std::f32::consts::FRAC_PI_2).sin(),
        "inOutSine" => -((std::f32::consts::PI * t).cos() - 1.0) * 0.5,
        "outInSine" => {
            if t < 0.5 {
                0.5 * ((t * std::f32::consts::PI).sin())
            } else {
                0.5 + 0.5 * (1.0 - (((t * 2.0) - 1.0) * std::f32::consts::FRAC_PI_2).cos())
            }
        }
        "inExpo" => {
            if t <= 0.0 {
                0.0
            } else {
                2.0_f32.powf((10.0 * t) - 10.0)
            }
        }
        "outExpo" => {
            if t >= 1.0 {
                1.0
            } else {
                1.0 - 2.0_f32.powf(-10.0 * t)
            }
        }
        "inOutExpo" => {
            if t <= 0.0 {
                0.0
            } else if t >= 1.0 {
                1.0
            } else if t < 0.5 {
                0.5 * 2.0_f32.powf((20.0 * t) - 10.0)
            } else {
                1.0 - (0.5 * 2.0_f32.powf((-20.0 * t) + 10.0))
            }
        }
        "outInExpo" => {
            if t < 0.5 {
                0.5 * (1.0 - 2.0_f32.powf(-20.0 * t))
            } else if t >= 1.0 {
                1.0
            } else {
                0.5 + 0.5 * 2.0_f32.powf((20.0 * t) - 20.0)
            }
        }
        "inCirc" => 1.0 - (1.0 - (t * t)).sqrt(),
        "outCirc" => (1.0 - ((t - 1.0) * (t - 1.0))).sqrt(),
        "inOutCirc" => {
            if t < 0.5 {
                0.5 * (1.0 - (1.0 - 4.0 * t * t).sqrt())
            } else {
                0.5 * ((1.0 - ((-2.0 * t + 2.0) * (-2.0 * t + 2.0))).sqrt() + 1.0)
            }
        }
        "outInCirc" => {
            if t < 0.5 {
                0.5 * (1.0 - ((2.0 * t - 1.0) * (2.0 * t - 1.0))).sqrt()
            } else {
                0.5 + 0.5 * (1.0 - (1.0 - ((2.0 * t - 1.0) * (2.0 * t - 1.0))).sqrt())
            }
        }
        "inElastic" => {
            if t <= 0.0 {
                0.0
            } else if t >= 1.0 {
                1.0
            } else {
                let u = t - 1.0;
                -(2.0_f32.powf(10.0 * u)) * ((u - elastic_period * 0.25) * elastic_tau).sin()
            }
        }
        "outElastic" => {
            if t <= 0.0 {
                0.0
            } else if t >= 1.0 {
                1.0
            } else {
                2.0_f32.powf(-10.0 * t) * ((t - elastic_period * 0.25) * elastic_tau).sin() + 1.0
            }
        }
        "inOutElastic" => {
            if t <= 0.0 {
                0.0
            } else if t >= 1.0 {
                1.0
            } else if t < 0.5 {
                let u = (2.0 * t) - 1.0;
                -0.5 * 2.0_f32.powf(10.0 * u) * ((u - elastic_period * 0.375) * elastic_tau).sin()
            } else {
                let u = (2.0 * t) - 1.0;
                0.5 * 2.0_f32.powf(-10.0 * u) * ((u - elastic_period * 0.375) * elastic_tau).sin()
                    + 1.0
            }
        }
        "outInElastic" => {
            if t < 0.5 {
                0.5 * song_lua_ease_factor(Some("outElastic"), t * 2.0, opt1, opt2)
            } else {
                0.5 + 0.5 * song_lua_ease_factor(Some("inElastic"), (t * 2.0) - 1.0, opt1, opt2)
            }
        }
        "inBack" => t * t * (((overshoot + 1.0) * t) - overshoot),
        "outBack" => {
            let u = t - 1.0;
            (u * u * (((overshoot + 1.0) * u) + overshoot)) + 1.0
        }
        "inOutBack" => {
            let s = overshoot * 1.525;
            if t < 0.5 {
                let u = 2.0 * t;
                0.5 * (u * u * (((s + 1.0) * u) - s))
            } else {
                let u = (2.0 * t) - 2.0;
                0.5 * (u * u * (((s + 1.0) * u) + s) + 2.0)
            }
        }
        "outInBack" => {
            if t < 0.5 {
                0.5 * song_lua_ease_factor(Some("outBack"), t * 2.0, opt1, opt2)
            } else {
                0.5 + 0.5 * song_lua_ease_factor(Some("inBack"), (t * 2.0) - 1.0, opt1, opt2)
            }
        }
        "inBounce" => song_lua_in_bounce(t),
        "outBounce" => song_lua_out_bounce(t),
        "inOutBounce" => song_lua_in_out_bounce(t),
        "outInBounce" => {
            if t < 0.5 {
                0.5 * song_lua_out_bounce(t * 2.0)
            } else {
                0.5 + 0.5 * song_lua_in_bounce((t * 2.0) - 1.0)
            }
        }
        _ => t,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GameplayReceptorGlowBehavior {
    pub press_duration: f32,
    pub press_alpha_start: f32,
    pub press_alpha_end: f32,
    pub press_zoom_start: f32,
    pub press_zoom_end: f32,
    pub press_tween: GameplayTween,
    pub duration: f32,
    pub alpha_start: f32,
    pub alpha_end: f32,
    pub zoom_start: f32,
    pub zoom_end: f32,
    pub tween: GameplayTween,
    pub blend_add: bool,
}

impl GameplayReceptorGlowBehavior {
    #[inline(always)]
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

    #[inline(always)]
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

impl Default for GameplayReceptorGlowBehavior {
    fn default() -> Self {
        Self {
            press_duration: 0.0,
            press_alpha_start: 1.0,
            press_alpha_end: 1.0,
            press_zoom_start: 1.0,
            press_zoom_end: 1.0,
            press_tween: GameplayTween::Linear,
            duration: 0.2,
            alpha_start: 1.0,
            alpha_end: 0.0,
            zoom_start: 1.0,
            zoom_end: 1.0,
            tween: GameplayTween::Decelerate,
            blend_add: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GameplayReceptorGlowState {
    pub press_timer: f32,
    pub lift_timer: f32,
    pub lift_start_alpha: f32,
    pub lift_start_zoom: f32,
    pub lane_pressed: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GameplayReceptorGlowTimers {
    pub press_timer: f32,
    pub lift_timer: f32,
    pub lift_start_alpha: f32,
    pub lift_start_zoom: f32,
}

#[derive(Clone, Debug)]
pub struct GameplayReceptorFeedbackState {
    pub glow_lift_timers: [f32; MAX_COLS],
    pub glow_press_timers: [f32; MAX_COLS],
    pub glow_lift_start_alpha: [f32; MAX_COLS],
    pub glow_lift_start_zoom: [f32; MAX_COLS],
    pub bop_timers: [f32; MAX_COLS],
    pub bop_behaviors: [GameplayReceptorStepBehavior; MAX_COLS],
}

impl Default for GameplayReceptorFeedbackState {
    fn default() -> Self {
        Self {
            glow_lift_timers: [0.0; MAX_COLS],
            glow_press_timers: [0.0; MAX_COLS],
            glow_lift_start_alpha: [0.0; MAX_COLS],
            glow_lift_start_zoom: [1.0; MAX_COLS],
            bop_timers: [0.0; MAX_COLS],
            bop_behaviors: [GameplayReceptorStepBehavior::identity(); MAX_COLS],
        }
    }
}

impl GameplayReceptorFeedbackState {
    #[inline(always)]
    pub fn reset_for_autoplay(&mut self) {
        self.glow_lift_timers.fill(0.0);
        self.glow_press_timers.fill(0.0);
        self.glow_lift_start_alpha.fill(0.0);
        self.glow_lift_start_zoom.fill(1.0);
    }

    #[inline(always)]
    pub fn reset_for_practice(&mut self) {
        self.glow_lift_timers.fill(0.0);
        self.glow_press_timers.fill(0.0);
        self.glow_lift_start_alpha.fill(0.0);
        self.glow_lift_start_zoom.fill(0.0);
        self.bop_timers.fill(0.0);
        self.bop_behaviors
            .fill(GameplayReceptorStepBehavior::identity());
    }

    #[inline(always)]
    pub fn set_glow_timers(&mut self, col: usize, timers: GameplayReceptorGlowTimers) {
        if col >= MAX_COLS {
            return;
        }
        self.glow_press_timers[col] = timers.press_timer;
        self.glow_lift_timers[col] = timers.lift_timer;
        self.glow_lift_start_alpha[col] = timers.lift_start_alpha;
        self.glow_lift_start_zoom[col] = timers.lift_start_zoom;
    }

    #[inline(always)]
    pub fn start_bop(&mut self, col: usize, behavior: GameplayReceptorStepBehavior) {
        if col < MAX_COLS && (behavior.duration > f32::EPSILON || behavior.interrupts) {
            self.bop_behaviors[col] = behavior;
            self.bop_timers[col] = behavior.duration.max(0.0);
        }
    }

    #[inline(always)]
    pub fn clear_lift_glow(&mut self, col: usize) {
        if let Some(timer) = self.glow_lift_timers.get_mut(col) {
            *timer = 0.0;
        }
    }

    #[inline(always)]
    pub fn receptor_glow_state(&self, col: usize, lane_pressed: bool) -> GameplayReceptorGlowState {
        if col >= MAX_COLS {
            return GameplayReceptorGlowState {
                lane_pressed,
                ..GameplayReceptorGlowState::default()
            };
        }
        GameplayReceptorGlowState {
            press_timer: self.glow_press_timers[col],
            lift_timer: self.glow_lift_timers[col],
            lift_start_alpha: self.glow_lift_start_alpha[col],
            lift_start_zoom: self.glow_lift_start_zoom[col],
            lane_pressed,
        }
    }

    #[inline(always)]
    pub fn bop_zoom(&self, col: usize) -> f32 {
        let Some(timer) = self.bop_timers.get(col).copied() else {
            return 1.0;
        };
        if timer > 0.0 {
            self.bop_behaviors[col].sample_zoom(timer)
        } else {
            1.0
        }
    }

    #[inline(always)]
    pub fn set_bop_timer_for_benchmark(&mut self, col: usize, timer: f32) {
        if let Some(slot) = self.bop_timers.get_mut(col) {
            *slot = timer;
        }
    }

    pub fn tick(
        &mut self,
        noteskin_effects: &GameplayNoteskinEffects,
        num_cols: usize,
        num_players: usize,
        cols_per_player: usize,
        input_lane_counts: &[u16],
        delta_time: f32,
    ) {
        tick_receptor_glow_columns(
            noteskin_effects,
            num_cols,
            num_players,
            cols_per_player,
            input_lane_counts,
            &mut self.glow_press_timers,
            &mut self.glow_lift_timers,
            &mut self.glow_lift_start_alpha,
            &mut self.glow_lift_start_zoom,
            delta_time,
        );
        for timer in &mut self.bop_timers {
            tick_positive_timer(timer, delta_time);
        }
    }
}

#[inline(always)]
pub fn receptor_glow_duration(behavior: GameplayReceptorGlowBehavior) -> f32 {
    Some(behavior.duration)
        .filter(|duration| *duration > f32::EPSILON)
        .unwrap_or(RECEPTOR_GLOW_DURATION)
}

#[inline(always)]
pub fn receptor_glow_visual(
    behavior: GameplayReceptorGlowBehavior,
    state: GameplayReceptorGlowState,
) -> Option<(f32, f32)> {
    if state.press_timer > f32::EPSILON && behavior.press_duration > f32::EPSILON {
        return Some(behavior.sample_press(state.press_timer));
    }
    if state.lane_pressed {
        return Some((behavior.press_alpha_end, behavior.press_zoom_end));
    }
    if state.lift_timer > f32::EPSILON {
        return Some(behavior.sample_lift(
            state.lift_timer,
            state.lift_start_alpha,
            state.lift_start_zoom,
        ));
    }
    None
}

#[inline(always)]
pub fn receptor_glow_pulse_timers(
    behavior: GameplayReceptorGlowBehavior,
) -> GameplayReceptorGlowTimers {
    GameplayReceptorGlowTimers {
        press_timer: 0.0,
        lift_timer: receptor_glow_duration(behavior),
        lift_start_alpha: behavior.press_alpha_start,
        lift_start_zoom: behavior.press_zoom_start,
    }
}

#[inline(always)]
pub fn receptor_glow_press_timers(
    behavior: GameplayReceptorGlowBehavior,
) -> GameplayReceptorGlowTimers {
    GameplayReceptorGlowTimers {
        press_timer: behavior.press_duration,
        lift_timer: 0.0,
        lift_start_alpha: behavior.press_alpha_end,
        lift_start_zoom: behavior.press_zoom_end,
    }
}

#[inline(always)]
pub fn receptor_glow_lift_start(
    behavior: GameplayReceptorGlowBehavior,
    press_timer: f32,
) -> (f32, f32) {
    if press_timer > f32::EPSILON && behavior.press_duration > f32::EPSILON {
        behavior.sample_press(press_timer)
    } else {
        (behavior.press_alpha_end, behavior.press_zoom_end)
    }
}

#[inline(always)]
pub fn receptor_glow_release_timers(
    behavior: GameplayReceptorGlowBehavior,
    press_timer: f32,
) -> GameplayReceptorGlowTimers {
    let (alpha, zoom) = receptor_glow_lift_start(behavior, press_timer);
    GameplayReceptorGlowTimers {
        press_timer: 0.0,
        lift_timer: receptor_glow_duration(behavior),
        lift_start_alpha: alpha,
        lift_start_zoom: zoom,
    }
}

#[inline(always)]
pub fn tick_receptor_glow_timers(
    behavior: GameplayReceptorGlowBehavior,
    timers: GameplayReceptorGlowTimers,
    lane_pressed: bool,
    delta_time: f32,
) -> GameplayReceptorGlowTimers {
    if lane_pressed {
        return GameplayReceptorGlowTimers {
            press_timer: (timers.press_timer - delta_time).max(0.0),
            lift_timer: 0.0,
            ..timers
        };
    }
    if timers.press_timer > f32::EPSILON {
        if timers.press_timer <= delta_time {
            receptor_glow_release_timers(behavior, timers.press_timer)
        } else {
            GameplayReceptorGlowTimers {
                press_timer: timers.press_timer - delta_time,
                ..timers
            }
        }
    } else {
        GameplayReceptorGlowTimers {
            lift_timer: (timers.lift_timer - delta_time).max(0.0),
            ..timers
        }
    }
}

pub fn tick_receptor_glow_columns(
    noteskin_effects: &GameplayNoteskinEffects,
    num_cols: usize,
    num_players: usize,
    cols_per_player: usize,
    input_lane_counts: &[u16],
    press_timers: &mut [f32],
    lift_timers: &mut [f32],
    lift_start_alpha: &mut [f32],
    lift_start_zoom: &mut [f32],
    delta_time: f32,
) {
    let col_count = num_cols
        .min(input_lane_counts.len())
        .min(press_timers.len())
        .min(lift_timers.len())
        .min(lift_start_alpha.len())
        .min(lift_start_zoom.len());
    for col in 0..col_count {
        let player = player_index_for_column(num_players, cols_per_player, col);
        let timers = tick_receptor_glow_timers(
            noteskin_effects.receptor_glow_behavior_for_player(player),
            GameplayReceptorGlowTimers {
                press_timer: press_timers[col],
                lift_timer: lift_timers[col],
                lift_start_alpha: lift_start_alpha[col],
                lift_start_zoom: lift_start_zoom[col],
            },
            input_lane_counts[col] != 0,
            delta_time,
        );
        press_timers[col] = timers.press_timer;
        lift_timers[col] = timers.lift_timer;
        lift_start_alpha[col] = timers.lift_start_alpha;
        lift_start_zoom[col] = timers.lift_start_zoom;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GameplayReceptorStepBehavior {
    pub duration: f32,
    pub zoom_start: f32,
    pub zoom_end: f32,
    pub tween: GameplayTween,
    pub interrupts: bool,
}

impl GameplayReceptorStepBehavior {
    pub const fn identity() -> Self {
        Self {
            duration: 0.0,
            zoom_start: 1.0,
            zoom_end: 1.0,
            tween: GameplayTween::Linear,
            interrupts: false,
        }
    }

    #[inline(always)]
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

impl Default for GameplayReceptorStepBehavior {
    fn default() -> Self {
        Self {
            duration: 0.11,
            zoom_start: 0.75,
            zoom_end: 1.0,
            tween: GameplayTween::Linear,
            interrupts: true,
        }
    }
}

#[inline(always)]
pub fn default_receptor_step_behavior_for_window(
    window: Option<&str>,
) -> GameplayReceptorStepBehavior {
    match window {
        Some("W1" | "W2" | "W3" | "W4" | "W5" | "Miss") => GameplayReceptorStepBehavior::identity(),
        _ => GameplayReceptorStepBehavior::default(),
    }
}

#[inline(always)]
pub fn receptor_step_window_index(window: Option<&str>) -> usize {
    match window {
        Some("W1") => 1,
        Some("W2") => 2,
        Some("W3") => 3,
        Some("W4") => 4,
        Some("W5") => 5,
        Some("Miss") => 6,
        _ => 0,
    }
}

#[inline(always)]
pub fn tap_explosion_window_index(window: &str) -> Option<usize> {
    match window {
        "W1" => Some(0),
        "W2" => Some(1),
        "W3" => Some(2),
        "W4" => Some(3),
        "W5" => Some(4),
        "Miss" => Some(5),
        "Held" => Some(6),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TapExplosionOptions {
    pub fantastic: bool,
    pub excellent: bool,
    pub great: bool,
    pub decent: bool,
    pub way_off: bool,
    pub miss: bool,
    pub held: bool,
    pub holding: bool,
}

#[inline(always)]
pub fn tap_explosion_options_from_profile<Profile: GameplayProfileData>(
    profile: &Profile,
) -> TapExplosionOptions {
    profile.tap_explosion_options()
}

#[inline(always)]
pub fn tap_explosion_enabled_for_options(options: TapExplosionOptions, window: &str) -> bool {
    match window {
        "W0" | "W1" => options.fantastic,
        "W2" => options.excellent,
        "W3" => options.great,
        "W4" => options.decent,
        "W5" => options.way_off,
        "Miss" => options.miss,
        "Held" => options.held,
        _ => false,
    }
}

#[inline(always)]
pub const fn hold_explosion_enabled_for_options(options: TapExplosionOptions) -> bool {
    options.holding
}

#[derive(Clone, Debug)]
pub struct GameplayNoteskinEffects {
    receptor_glow_behavior: [GameplayReceptorGlowBehavior; MAX_PLAYERS],
    receptor_step_behaviors:
        [[[GameplayReceptorStepBehavior; RECEPTOR_STEP_WINDOW_COUNT]; MAX_COLS]; MAX_PLAYERS],
    tap_explosion_durations:
        [[[[Option<f32>; 2]; TAP_EXPLOSION_WINDOW_COUNT]; MAX_COLS]; MAX_PLAYERS],
    mine_explosion_duration: [f32; MAX_PLAYERS],
}

impl GameplayNoteskinEffects {
    #[inline(always)]
    pub fn set_receptor_glow_behavior(
        &mut self,
        player: usize,
        behavior: GameplayReceptorGlowBehavior,
    ) {
        if player < MAX_PLAYERS {
            self.receptor_glow_behavior[player] = behavior;
        }
    }

    #[inline(always)]
    pub fn set_receptor_step_behavior(
        &mut self,
        player: usize,
        local_col: usize,
        window: Option<&str>,
        behavior: GameplayReceptorStepBehavior,
    ) {
        if player < MAX_PLAYERS && local_col < MAX_COLS {
            self.receptor_step_behaviors[player][local_col][receptor_step_window_index(window)] =
                behavior;
        }
    }

    #[inline(always)]
    pub fn set_tap_explosion_duration(
        &mut self,
        player: usize,
        local_col: usize,
        window: &str,
        bright: bool,
        duration: Option<f32>,
    ) {
        if player < MAX_PLAYERS
            && local_col < MAX_COLS
            && let Some(window_idx) = tap_explosion_window_index(window)
        {
            self.tap_explosion_durations[player][local_col][window_idx][usize::from(bright)] =
                duration;
        }
    }

    #[inline(always)]
    pub fn set_mine_explosion_duration(&mut self, player: usize, duration: f32) {
        if player < MAX_PLAYERS {
            self.mine_explosion_duration[player] = duration;
        }
    }

    #[inline(always)]
    pub fn receptor_glow_behavior_for_player(&self, player: usize) -> GameplayReceptorGlowBehavior {
        self.receptor_glow_behavior[player.min(MAX_PLAYERS - 1)]
    }

    #[inline(always)]
    pub fn receptor_step_behavior_for_col(
        &self,
        player: usize,
        local_col: usize,
        window: Option<&str>,
    ) -> GameplayReceptorStepBehavior {
        self.receptor_step_behaviors[player.min(MAX_PLAYERS - 1)][local_col.min(MAX_COLS - 1)]
            [receptor_step_window_index(window)]
    }

    #[inline(always)]
    pub fn tap_explosion_duration(
        &self,
        player: usize,
        local_col: usize,
        window: &str,
        bright: bool,
    ) -> Option<f32> {
        tap_explosion_window_index(window).and_then(|window_idx| {
            self.tap_explosion_durations[player.min(MAX_PLAYERS - 1)][local_col.min(MAX_COLS - 1)]
                [window_idx][usize::from(bright)]
        })
    }

    #[inline(always)]
    pub fn mine_explosion_duration(&self, player: usize) -> f32 {
        self.mine_explosion_duration[player.min(MAX_PLAYERS - 1)]
    }
}

impl Default for GameplayNoteskinEffects {
    fn default() -> Self {
        let receptor_step_behaviors = std::array::from_fn(|_| {
            std::array::from_fn(|_| {
                std::array::from_fn(|idx| {
                    default_receptor_step_behavior_for_window(RECEPTOR_STEP_WINDOWS[idx])
                })
            })
        });
        Self {
            receptor_glow_behavior: std::array::from_fn(|_| {
                GameplayReceptorGlowBehavior::default()
            }),
            receptor_step_behaviors,
            tap_explosion_durations: std::array::from_fn(|_| {
                std::array::from_fn(|_| std::array::from_fn(|_| [None, None]))
            }),
            mine_explosion_duration: [MINE_EXPLOSION_DURATION; MAX_PLAYERS],
        }
    }
}

pub struct GameplayNoteskinData {
    pub effects: GameplayNoteskinEffects,
}

impl Default for GameplayNoteskinData {
    fn default() -> Self {
        Self {
            effects: GameplayNoteskinEffects::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComboMilestoneKind {
    Hundred,
    Thousand,
}

#[derive(Clone, Debug)]
pub struct ActiveComboMilestone {
    pub kind: ComboMilestoneKind,
    pub elapsed: f32,
}

pub const MINE_HIT_INCREMENTS_MISS_COMBO: bool = false;
pub const HOLD_SUCCESS_RESETS_MISS_COMBO: bool = false;
pub const COMBO_BREAK_ON_IMMEDIATE_HOLD_LET_GO: bool = false;

pub fn trigger_combo_milestone(
    milestones: &mut Vec<ActiveComboMilestone>,
    kind: ComboMilestoneKind,
) {
    if let Some(index) = milestones
        .iter()
        .position(|milestone| milestone.kind == kind)
    {
        milestones[index].elapsed = 0.0;
    } else {
        milestones.push(ActiveComboMilestone { kind, elapsed: 0.0 });
    }
}

#[inline(always)]
pub const fn combo_milestone_duration(kind: ComboMilestoneKind) -> f32 {
    match kind {
        ComboMilestoneKind::Hundred => COMBO_HUNDRED_MILESTONE_DURATION,
        ComboMilestoneKind::Thousand => COMBO_THOUSAND_MILESTONE_DURATION,
    }
}

pub fn tick_combo_milestones(milestones: &mut Vec<ActiveComboMilestone>, delta_time: f32) {
    milestones.retain_mut(|milestone| {
        milestone.elapsed += delta_time;
        milestone.elapsed < combo_milestone_duration(milestone.kind)
    });
}

pub fn apply_combo_update_feedback(
    current_combo_window_counts: &mut WindowCounts,
    milestones: &mut Vec<ActiveComboMilestone>,
    update: ComboUpdate,
) {
    if update.combo_broken {
        *current_combo_window_counts = WindowCounts::default();
    }
    if update.hit_thousand_milestone {
        trigger_combo_milestone(milestones, ComboMilestoneKind::Thousand);
    }
    if update.hit_hundred_milestone {
        trigger_combo_milestone(milestones, ComboMilestoneKind::Hundred);
    }
}

pub fn apply_mine_hit_combo_policy(state: &mut ComboState) -> ComboUpdate {
    if MINE_HIT_INCREMENTS_MISS_COMBO {
        combo::break_combo_state(state, 1)
    } else {
        ComboUpdate::default()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MineHitPlayerState {
    pub mines_hit: u32,
    pub mines_hit_for_score: u32,
    pub combo: ComboState,
}

#[inline(always)]
pub fn mine_hit_player_state(player: &PlayerRuntime) -> MineHitPlayerState {
    MineHitPlayerState {
        mines_hit: player.mines_hit,
        mines_hit_for_score: player.mines_hit_for_score,
        combo: player_combo_state(player),
    }
}

#[inline(always)]
pub fn write_mine_hit_player_state(player: &mut PlayerRuntime, state: MineHitPlayerState) {
    player.mines_hit = state.mines_hit;
    player.mines_hit_for_score = state.mines_hit_for_score;
    write_player_combo_state(player, state.combo);
}

#[inline(always)]
pub fn apply_mine_hit_player_update(
    player: &mut PlayerRuntime,
    state: MineHitPlayerState,
    update: MineHitPlayerUpdate,
) {
    write_mine_hit_player_state(player, state);
    apply_combo_update(player, update.combo_update);
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MineHitPlayerUpdate {
    pub counted_hit: bool,
    pub counted_for_score: bool,
    pub combo_update: ComboUpdate,
    pub life_delta: f32,
    pub apply_life_change: bool,
    pub capture_failed_ex_score_inputs: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MineHitSideEffectPlan {
    pub life_delta: f32,
    pub apply_life_change: bool,
    pub capture_failed_ex_score_inputs: bool,
}

pub fn mine_hit_side_effect_plan(scoring_blocked: bool) -> MineHitSideEffectPlan {
    MineHitSideEffectPlan {
        life_delta: deadsync_rules::life::LIFE_HIT_MINE,
        apply_life_change: !scoring_blocked,
        capture_failed_ex_score_inputs: !scoring_blocked,
    }
}

pub fn apply_mine_hit_player_state(
    state: &mut MineHitPlayerState,
    scoring_blocked: bool,
    player_dead_after_life: bool,
) -> MineHitPlayerUpdate {
    let side_effects = mine_hit_side_effect_plan(scoring_blocked);
    if scoring_blocked {
        return MineHitPlayerUpdate {
            life_delta: side_effects.life_delta,
            apply_life_change: side_effects.apply_life_change,
            capture_failed_ex_score_inputs: side_effects.capture_failed_ex_score_inputs,
            ..MineHitPlayerUpdate::default()
        };
    }
    state.mines_hit = state.mines_hit.saturating_add(1);
    let counted_for_score = !player_dead_after_life;
    if counted_for_score {
        state.mines_hit_for_score = state.mines_hit_for_score.saturating_add(1);
    }
    MineHitPlayerUpdate {
        counted_hit: true,
        counted_for_score,
        combo_update: apply_mine_hit_combo_policy(&mut state.combo),
        life_delta: side_effects.life_delta,
        apply_life_change: side_effects.apply_life_change,
        capture_failed_ex_score_inputs: side_effects.capture_failed_ex_score_inputs,
    }
}

pub fn apply_hold_success_combo_policy(state: &mut ComboState) -> ComboUpdate {
    // ITG dance/pump scoring does not let Held / Roll Held reset miss combo.
    if HOLD_SUCCESS_RESETS_MISS_COMBO {
        state.miss_combo = 0;
    }
    ComboUpdate::default()
}

pub fn apply_hold_let_go_combo_policy(state: &mut ComboState) -> ComboUpdate {
    if COMBO_BREAK_ON_IMMEDIATE_HOLD_LET_GO {
        combo::break_combo_state(state, 1)
    } else {
        combo::clear_full_combo_state(state);
        ComboUpdate::default()
    }
}

// Simply Love danger overlay semantics (ScreenGameplay underlay/PerPlayer/Danger.lua).
// Metrics: itgmania/Themes/Simply Love/metrics.ini -> DangerThreshold=0.2
const DANGER_THRESHOLD: f32 = 0.2;
const DANGER_BASE_ALPHA: f32 = 0.7;
const DANGER_FADE_IN_S: f32 = 0.3;
const DANGER_HIDE_FADE_S: f32 = 0.3;
const DANGER_FLASH_IN_S: f32 = 0.3;
const DANGER_FLASH_OUT_S: f32 = 0.3;
const DANGER_FLASH_ALPHA: f32 = 0.8;
const DANGER_EFFECT_PERIOD_S: f32 = 1.0;
const DANGER_EC1_RGBA: [f32; 4] = [1.0, 0.0, 0.24, 0.1];
const DANGER_EC2_RGBA: [f32; 4] = [1.0, 0.0, 0.0, 0.35];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HealthState {
    #[default]
    Alive,
    Danger,
    Dead,
}

#[derive(Clone, Copy, Debug, Default)]
enum DangerAnim {
    #[default]
    Hidden,
    Danger {
        started_at: f32,
        alpha_start: f32,
    },
    FadeOut {
        started_at: f32,
        rgba_start: [f32; 4],
    },
    Flash {
        started_at: f32,
        rgb: [f32; 3],
    },
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DangerFx {
    last_health: HealthState,
    prev_health: HealthState,
    anim: DangerAnim,
}

#[inline(always)]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    (b - a).mul_add(t.clamp(0.0, 1.0), a)
}

#[inline(always)]
fn danger_flash_alpha(age: f32) -> f32 {
    if !age.is_finite() || age <= 0.0 {
        return 0.0;
    }
    if age < DANGER_FLASH_IN_S {
        return DANGER_FLASH_ALPHA * (age / DANGER_FLASH_IN_S).clamp(0.0, 1.0);
    }
    let t2 = age - DANGER_FLASH_IN_S;
    if t2 < DANGER_FLASH_OUT_S {
        return DANGER_FLASH_ALPHA * (1.0 - (t2 / DANGER_FLASH_OUT_S).clamp(0.0, 1.0));
    }
    0.0
}

#[inline(always)]
fn danger_effect_rgba(age: f32, base_alpha: f32) -> [f32; 4] {
    let period = DANGER_EFFECT_PERIOD_S;
    if !age.is_finite() || !base_alpha.is_finite() || base_alpha <= 0.0 || period <= 0.0 {
        return [0.0, 0.0, 0.0, 0.0];
    }
    let phase = (age.rem_euclid(period) / period).clamp(0.0, 1.0);
    let f = ((phase + 0.25) * std::f32::consts::TAU)
        .sin()
        .mul_add(0.5, 0.5);
    let inv = 1.0 - f;

    let r = DANGER_EC1_RGBA[0] * f + DANGER_EC2_RGBA[0] * inv;
    let g = DANGER_EC1_RGBA[1] * f + DANGER_EC2_RGBA[1] * inv;
    let b = DANGER_EC1_RGBA[2] * f + DANGER_EC2_RGBA[2] * inv;
    let a = (DANGER_EC1_RGBA[3] * f + DANGER_EC2_RGBA[3] * inv) * base_alpha;
    [r, g, b, a]
}

#[inline(always)]
fn danger_anim_base_alpha(anim: &DangerAnim, now: f32) -> f32 {
    let now = if now.is_finite() { now } else { 0.0 };
    match *anim {
        DangerAnim::Hidden => 0.0,
        DangerAnim::Danger {
            started_at,
            alpha_start,
        } => {
            let age = now - started_at;
            if !age.is_finite() || age <= 0.0 {
                alpha_start
            } else if age < DANGER_FADE_IN_S {
                lerp(alpha_start, DANGER_BASE_ALPHA, age / DANGER_FADE_IN_S)
            } else {
                DANGER_BASE_ALPHA
            }
        }
        DangerAnim::FadeOut {
            started_at,
            rgba_start,
        } => {
            let age = now - started_at;
            if !age.is_finite() || age <= 0.0 {
                rgba_start[3]
            } else if age < DANGER_HIDE_FADE_S {
                lerp(rgba_start[3], 0.0, age / DANGER_HIDE_FADE_S)
            } else {
                0.0
            }
        }
        DangerAnim::Flash { started_at, .. } => danger_flash_alpha(now - started_at),
    }
}

#[inline(always)]
fn danger_anim_rgba(anim: &DangerAnim, now: f32) -> [f32; 4] {
    let now = if now.is_finite() { now } else { 0.0 };
    match *anim {
        DangerAnim::Hidden => [0.0, 0.0, 0.0, 0.0],
        DangerAnim::Danger {
            started_at,
            alpha_start,
        } => {
            let age = now - started_at;
            let base_alpha = if !age.is_finite() || age <= 0.0 {
                alpha_start
            } else if age < DANGER_FADE_IN_S {
                lerp(alpha_start, DANGER_BASE_ALPHA, age / DANGER_FADE_IN_S)
            } else {
                DANGER_BASE_ALPHA
            };
            danger_effect_rgba(age, base_alpha)
        }
        DangerAnim::FadeOut {
            started_at,
            rgba_start,
        } => {
            let age = now - started_at;
            let a = if !age.is_finite() || age <= 0.0 {
                rgba_start[3]
            } else if age < DANGER_HIDE_FADE_S {
                lerp(rgba_start[3], 0.0, age / DANGER_HIDE_FADE_S)
            } else {
                0.0
            };
            [rgba_start[0], rgba_start[1], rgba_start[2], a]
        }
        DangerAnim::Flash { started_at, rgb } => {
            let a = danger_flash_alpha(now - started_at);
            [rgb[0], rgb[1], rgb[2], a]
        }
    }
}

#[inline(always)]
pub fn danger_health_state(life: f32, is_failing: bool) -> HealthState {
    if is_failing || life <= 0.0 {
        HealthState::Dead
    } else if life < DANGER_THRESHOLD {
        HealthState::Danger
    } else {
        HealthState::Alive
    }
}

#[inline(always)]
pub fn player_health_state(player: &PlayerRuntime) -> HealthState {
    danger_health_state(player.life, player.is_failing)
}

#[inline(always)]
pub fn danger_fx_rgba(fx: &DangerFx, now: f32) -> [f32; 4] {
    danger_anim_rgba(&fx.anim, now)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GameplayDangerFxState {
    effects: [DangerFx; MAX_PLAYERS],
}

impl GameplayDangerFxState {
    #[inline(always)]
    pub fn reset_player(&mut self, player: usize) {
        if player < MAX_PLAYERS {
            self.effects[player] = DangerFx::default();
        }
    }

    #[inline(always)]
    pub fn rgba(&self, player: usize, now: f32) -> [f32; 4] {
        self.effects
            .get(player)
            .map(|fx| danger_fx_rgba(fx, now))
            .unwrap_or([0.0, 0.0, 0.0, 0.0])
    }

    #[inline(always)]
    pub fn update_player(
        &mut self,
        player: usize,
        health: HealthState,
        now: f32,
        hide_danger: bool,
    ) {
        if let Some(fx) = self.effects.get_mut(player) {
            update_danger_fx_for_health(fx, health, now, hide_danger);
        }
    }
}

#[inline(always)]
pub fn update_danger_fx_for_health(
    fx: &mut DangerFx,
    health: HealthState,
    now: f32,
    hide_danger: bool,
) {
    if fx.last_health == health {
        return;
    }

    if hide_danger {
        if health == HealthState::Dead {
            fx.anim = DangerAnim::Flash {
                started_at: now,
                rgb: [1.0, 0.0, 0.0],
            };
        }
        fx.last_health = health;
        return;
    }

    match health {
        HealthState::Danger => {
            fx.anim = DangerAnim::Danger {
                started_at: now,
                alpha_start: danger_anim_base_alpha(&fx.anim, now),
            };
            fx.prev_health = HealthState::Danger;
        }
        HealthState::Dead => {
            fx.anim = DangerAnim::Flash {
                started_at: now,
                rgb: [1.0, 0.0, 0.0],
            };
        }
        HealthState::Alive => {
            fx.anim = if fx.prev_health == HealthState::Danger {
                DangerAnim::Flash {
                    started_at: now,
                    rgb: [0.0, 1.0, 0.0],
                }
            } else {
                DangerAnim::FadeOut {
                    started_at: now,
                    rgba_start: danger_anim_rgba(&fx.anim, now),
                }
            };
            fx.prev_health = HealthState::Alive;
        }
    }
    fx.last_health = health;
}

