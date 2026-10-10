/// ITGmania PlayerOptions timer selectors; enum changes never approach.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ModTimerType {
    Game = 0,
    Beat = 1,
    Song = 2,
    #[default]
    Default = 3,
}

impl ModTimerType {
    pub const fn from_value(value: f32) -> Option<Self> {
        match value {
            0.0 => Some(Self::Game),
            1.0 => Some(Self::Beat),
            2.0 => Some(Self::Song),
            3.0 => Some(Self::Default),
            _ => None,
        }
    }
}

#[inline(always)]
#[must_use]
pub fn effective_mini_percent(
    active_mini_percent: Option<f32>,
    fallback_mini_percent: f32,
    base_cleared: bool,
) -> f32 {
    active_mini_percent
        .filter(|v| v.is_finite())
        .unwrap_or(if base_cleared {
            0.0
        } else {
            fallback_mini_percent
        })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MiniAttackMode {
    Absolute,
    Delta,
}

#[inline(always)]
#[must_use]
pub fn attack_mini_target_percent(value: f32, mode: MiniAttackMode, base: f32) -> f32 {
    match mode {
        MiniAttackMode::Absolute => value,
        MiniAttackMode::Delta => base + value,
    }
}

#[inline(always)]
pub fn approach_attack_value(
    current: &mut Option<f32>,
    target: Option<f32>,
    base: f32,
    speed: Option<f32>,
    delta_time: f32,
    unit_scale: f32,
) {
    let Some(target) = target.filter(|value| value.is_finite()) else {
        *current = None;
        return;
    };
    let Some(speed) = speed.filter(|value| value.is_finite()) else {
        *current = Some(target);
        return;
    };
    let step = delta_time.max(0.0) * speed.max(0.0) * unit_scale;
    if step <= f32::EPSILON {
        // PlayerOptions::Approach preserves Current at zero elapsed time.
        *current = Some(current.filter(|value| value.is_finite()).unwrap_or(base));
        return;
    }
    let mut value = current.filter(|value| value.is_finite()).unwrap_or(base);
    approach_f32(&mut value, target, step);
    *current = Some(value);
}

#[inline(always)]
pub fn approach_attack_mini_percent_to_target(
    current: &mut Option<f32>,
    target: Option<f32>,
    base: f32,
    speed: Option<f32>,
    delta_time: f32,
) {
    approach_attack_value(current, target, base, speed, delta_time, 100.0);
}

#[inline(always)]
#[must_use]
pub fn mini_value_for_percent(
    mini_percent: f32,
    fallback_mini_percent: f32,
    big_active: bool,
) -> f32 {
    let mut mini = if mini_percent.is_finite() {
        mini_percent
    } else {
        fallback_mini_percent
    };
    if big_active {
        // ITG _fallback/ArrowCloud map Effect Big to mod,-100% mini.
        mini -= 100.0;
    }
    mini / 100.0
}

#[inline(always)]
#[must_use]
pub fn mini_value_for_visual_mask(
    mini_percent: f32,
    fallback_mini_percent: f32,
    visual_mask: u16,
) -> f32 {
    mini_value_for_percent(
        mini_percent,
        fallback_mini_percent,
        (visual_mask & VISUAL_MASK_BIT_BIG) != 0,
    )
}

#[inline(always)]
#[must_use]
pub fn player_draw_scale_for_mini(tilt: f32, mini_value: f32) -> f32 {
    0.5f32.mul_add(tilt.abs(), 1.0) * (1.0 + mini_value.abs())
}

#[inline(always)]
#[must_use]
pub fn player_draw_scale_for_visual_mask(
    tilt: f32,
    mini_percent: f32,
    fallback_mini_percent: f32,
    visual_mask: u16,
) -> f32 {
    let mini = mini_value_for_visual_mask(mini_percent, fallback_mini_percent, visual_mask);
    player_draw_scale_for_mini(tilt, mini)
}

const ACCEL_MASK_BIT_BOOST: u8 = 1u8 << 0;
const ACCEL_MASK_BIT_BRAKE: u8 = 1u8 << 1;
const ACCEL_MASK_BIT_WAVE: u8 = 1u8 << 2;
const ACCEL_MASK_BIT_EXPAND: u8 = 1u8 << 3;
const ACCEL_MASK_BIT_BOOMERANG: u8 = 1u8 << 4;
const VISUAL_MASK_BIT_DRUNK: u16 = 1u16 << 0;
const VISUAL_MASK_BIT_DIZZY: u16 = 1u16 << 1;
const VISUAL_MASK_BIT_CONFUSION: u16 = 1u16 << 2;
pub const VISUAL_MASK_BIT_BIG: u16 = 1u16 << 3;
const VISUAL_MASK_BIT_FLIP: u16 = 1u16 << 4;
const VISUAL_MASK_BIT_INVERT: u16 = 1u16 << 5;
const VISUAL_MASK_BIT_TORNADO: u16 = 1u16 << 6;
const VISUAL_MASK_BIT_TIPSY: u16 = 1u16 << 7;
const VISUAL_MASK_BIT_BUMPY: u16 = 1u16 << 8;
const VISUAL_MASK_BIT_BEAT: u16 = 1u16 << 9;
const APPEARANCE_MASK_BIT_HIDDEN: u8 = 1u8 << 0;
const APPEARANCE_MASK_BIT_SUDDEN: u8 = 1u8 << 1;
const APPEARANCE_MASK_BIT_STEALTH: u8 = 1u8 << 2;
const APPEARANCE_MASK_BIT_BLINK: u8 = 1u8 << 3;
const APPEARANCE_MASK_BIT_RANDOM_VANISH: u8 = 1u8 << 4;
const APPEARANCE_MASK_BIT_DYNAMIC_SUDDEN: u8 = 1u8 << 5;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AccelOverrides {
    pub boost: Option<f32>,
    pub brake: Option<f32>,
    pub wave: Option<f32>,
    pub wave_period: Option<f32>,
    pub expand: Option<f32>,
    pub boomerang: Option<f32>,
}

impl AccelOverrides {
    #[inline(always)]
    #[must_use]
    pub const fn any(self) -> bool {
        self.boost.is_some()
            || self.brake.is_some()
            || self.wave.is_some()
            || self.wave_period.is_some()
            || self.expand.is_some()
            || self.boomerang.is_some()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VisualOverrides {
    pub mod_timer_type: Option<ModTimerType>,
    pub dizzy_holds: Option<bool>,
    pub z_buffer: Option<bool>,
    pub cosecant: Option<bool>,
    pub drunk: Option<f32>,
    pub drunk_offset: Option<f32>,
    pub drunk_speed: Option<f32>,
    pub drunk_period: Option<f32>,
    pub dizzy: Option<f32>,
    pub twirl: Option<f32>,
    pub roll: Option<f32>,
    pub parabola_x: Option<f32>,
    pub attenuate_x: Option<f32>,
    pub parabola_y: Option<f32>,
    pub attenuate_y: Option<f32>,
    pub mod_timer_mult: Option<f32>,
    pub mod_timer_offset: Option<f32>,
    pub bumpy_x: Option<f32>,
    pub bumpy_x_offset: Option<f32>,
    pub bumpy_x_period: Option<f32>,
    pub tan_bumpy: Option<f32>,
    pub tan_bumpy_offset: Option<f32>,
    pub tan_bumpy_period: Option<f32>,
    pub tan_bumpy_x: Option<f32>,
    pub tan_bumpy_x_offset: Option<f32>,
    pub tan_bumpy_x_period: Option<f32>,
    pub drunk_z: Option<f32>,
    pub drunk_z_offset: Option<f32>,
    pub drunk_z_speed: Option<f32>,
    pub drunk_z_period: Option<f32>,
    pub tan_drunk: Option<f32>,
    pub tan_drunk_offset: Option<f32>,
    pub tan_drunk_speed: Option<f32>,
    pub tan_drunk_period: Option<f32>,
    pub tan_drunk_z: Option<f32>,
    pub tan_drunk_z_offset: Option<f32>,
    pub tan_drunk_z_speed: Option<f32>,
    pub tan_drunk_z_period: Option<f32>,
    pub draw_size: Option<f32>,
    pub draw_size_back: Option<f32>,
    pub square: Option<f32>,
    pub digital: Option<f32>,
    pub square_offset: Option<f32>,
    pub digital_steps: Option<f32>,
    pub digital_offset: Option<f32>,
    pub square_period: Option<f32>,
    pub digital_period: Option<f32>,
    pub square_z: Option<f32>,
    pub zigzag: Option<f32>,
    pub zigzag_z: Option<f32>,
    pub square_z_offset: Option<f32>,
    pub zigzag_offset: Option<f32>,
    pub zigzag_z_offset: Option<f32>,
    pub square_z_period: Option<f32>,
    pub zigzag_period: Option<f32>,
    pub zigzag_z_period: Option<f32>,
    pub xmode: Option<f32>,
    pub bounce: Option<f32>,
    pub bounce_period: Option<f32>,
    pub bounce_offset: Option<f32>,
    pub tornado_period: Option<f32>,
    pub tornado_offset: Option<f32>,
    pub parabola_z: Option<f32>,
    pub attenuate_z: Option<f32>,
    pub confusion: Option<f32>,
    pub confusion_offset: Option<f32>,
    pub confusion_x_offset: Option<f32>,
    pub confusion_offset_cols: [Option<f32>; MAX_COLS],
    pub flip: Option<f32>,
    pub invert: Option<f32>,
    pub tornado: Option<f32>,
    pub tipsy: Option<f32>,
    pub tipsy_offset: Option<f32>,
    pub tipsy_speed: Option<f32>,
    pub tiny: Option<f32>,
    pub bumpy: Option<f32>,
    pub bumpy_offset: Option<f32>,
    pub bumpy_period: Option<f32>,
    pub bumpy_cols: [Option<f32>; MAX_COLS],
    pub tiny_cols: [Option<f32>; MAX_COLS],
    pub move_x_cols: [Option<f32>; MAX_COLS],
    pub move_y_cols: [Option<f32>; MAX_COLS],
    pub pulse_inner: Option<f32>,
    pub pulse_outer: Option<f32>,
    pub pulse_period: Option<f32>,
    pub beat_period: Option<f32>,
    pub shrink_linear: Option<f32>,
    pub shrink_mult: Option<f32>,
    pub bounce_z: Option<f32>,
    pub bounce_z_offset: Option<f32>,
    pub bounce_z_period: Option<f32>,
    pub digital_z: Option<f32>,
    pub digital_z_offset: Option<f32>,
    pub digital_z_period: Option<f32>,
    pub digital_z_steps: Option<f32>,
    pub tornado_z: Option<f32>,
    pub tornado_z_offset: Option<f32>,
    pub tornado_z_period: Option<f32>,
    pub sawtooth: Option<f32>,
    pub sawtooth_period: Option<f32>,
    pub sawtooth_z: Option<f32>,
    pub sawtooth_z_period: Option<f32>,
    pub confusion_x: Option<f32>,
    pub confusion_y: Option<f32>,
    pub confusion_y_offset: Option<f32>,
    pub beat_offset: Option<f32>,
    pub beat_mult: Option<f32>,
    pub beat_y: Option<f32>,
    pub beat_y_offset: Option<f32>,
    pub beat_y_mult: Option<f32>,
    pub beat_y_period: Option<f32>,
    pub beat_z: Option<f32>,
    pub beat_z_offset: Option<f32>,
    pub beat_z_mult: Option<f32>,
    pub beat_z_period: Option<f32>,
    pub pulse_offset: Option<f32>,
    pub beat: Option<f32>,
    pub random_speed: Option<f32>,
}

impl Default for VisualOverrides {
    fn default() -> Self {
        Self {
            mod_timer_type: None,
            dizzy_holds: None,
            z_buffer: None,
            cosecant: None,
            drunk: None,
            drunk_offset: None,
            drunk_speed: None,
            drunk_period: None,
            dizzy: None,
            twirl: None,
            roll: None,
            parabola_x: None,
            attenuate_x: None,
            parabola_y: None,
            attenuate_y: None,
            mod_timer_mult: None,
            mod_timer_offset: None,
            bumpy_x: None,
            bumpy_x_offset: None,
            bumpy_x_period: None,
            tan_bumpy: None,
            tan_bumpy_offset: None,
            tan_bumpy_period: None,
            tan_bumpy_x: None,
            tan_bumpy_x_offset: None,
            tan_bumpy_x_period: None,
            drunk_z: None,
            drunk_z_offset: None,
            drunk_z_speed: None,
            drunk_z_period: None,
            tan_drunk: None,
            tan_drunk_offset: None,
            tan_drunk_speed: None,
            tan_drunk_period: None,
            tan_drunk_z: None,
            tan_drunk_z_offset: None,
            tan_drunk_z_speed: None,
            tan_drunk_z_period: None,
            draw_size: None,
            draw_size_back: None,
            square: None,
            digital: None,
            square_offset: None,
            digital_steps: None,
            digital_offset: None,
            square_period: None,
            digital_period: None,
            square_z: None,
            zigzag: None,
            zigzag_z: None,
            square_z_offset: None,
            zigzag_offset: None,
            zigzag_z_offset: None,
            square_z_period: None,
            zigzag_period: None,
            zigzag_z_period: None,
            xmode: None,
            bounce: None,
            bounce_period: None,
            bounce_offset: None,
            tornado_period: None,
            tornado_offset: None,
            parabola_z: None,
            attenuate_z: None,
            confusion: None,
            confusion_offset: None,
            confusion_x_offset: None,
            confusion_offset_cols: [None; MAX_COLS],
            flip: None,
            invert: None,
            tornado: None,
            tipsy: None,
            tipsy_offset: None,
            tipsy_speed: None,
            tiny: None,
            bumpy: None,
            bumpy_offset: None,
            bumpy_period: None,
            bumpy_cols: [None; MAX_COLS],
            tiny_cols: [None; MAX_COLS],
            move_x_cols: [None; MAX_COLS],
            move_y_cols: [None; MAX_COLS],
            pulse_inner: None,
            pulse_outer: None,
            pulse_period: None,
            beat_period: None,
            shrink_linear: None,
            shrink_mult: None,
            bounce_z: None,
            bounce_z_offset: None,
            bounce_z_period: None,
            digital_z: None,
            digital_z_offset: None,
            digital_z_period: None,
            digital_z_steps: None,
            tornado_z: None,
            tornado_z_offset: None,
            tornado_z_period: None,
            sawtooth: None,
            sawtooth_period: None,
            sawtooth_z: None,
            sawtooth_z_period: None,
            confusion_x: None,
            confusion_y: None,
            confusion_y_offset: None,
            beat_offset: None,
            beat_mult: None,
            beat_y: None,
            beat_y_offset: None,
            beat_y_mult: None,
            beat_y_period: None,
            beat_z: None,
            beat_z_offset: None,
            beat_z_mult: None,
            beat_z_period: None,
            pulse_offset: None,
            beat: None,
            random_speed: None,
        }
    }
}

impl VisualOverrides {
    #[inline(always)]
    pub fn any(self) -> bool {
        self.mod_timer_type.is_some()
            || self.dizzy_holds.is_some()
            || self.z_buffer.is_some()
            || self.cosecant.is_some()
            || self.drunk.is_some()
            || self.drunk_offset.is_some()
            || self.drunk_speed.is_some()
            || self.drunk_period.is_some()
            || self.dizzy.is_some()
            || self.twirl.is_some()
            || self.roll.is_some()
            || self.parabola_x.is_some()
            || self.attenuate_x.is_some()
            || self.parabola_y.is_some()
            || self.attenuate_y.is_some()
            || self.mod_timer_mult.is_some()
            || self.mod_timer_offset.is_some()
            || self.bumpy_x.is_some()
            || self.bumpy_x_offset.is_some()
            || self.bumpy_x_period.is_some()
            || self.tan_bumpy.is_some()
            || self.tan_bumpy_offset.is_some()
            || self.tan_bumpy_period.is_some()
            || self.tan_bumpy_x.is_some()
            || self.tan_bumpy_x_offset.is_some()
            || self.tan_bumpy_x_period.is_some()
            || self.drunk_z.is_some()
            || self.drunk_z_offset.is_some()
            || self.drunk_z_speed.is_some()
            || self.drunk_z_period.is_some()
            || self.tan_drunk.is_some()
            || self.tan_drunk_offset.is_some()
            || self.tan_drunk_speed.is_some()
            || self.tan_drunk_period.is_some()
            || self.tan_drunk_z.is_some()
            || self.tan_drunk_z_offset.is_some()
            || self.tan_drunk_z_speed.is_some()
            || self.tan_drunk_z_period.is_some()
            || self.draw_size.is_some()
            || self.draw_size_back.is_some()
            || self.square.is_some()
            || self.digital.is_some()
            || self.square_offset.is_some()
            || self.digital_steps.is_some()
            || self.digital_offset.is_some()
            || self.square_period.is_some()
            || self.digital_period.is_some()
            || self.square_z.is_some()
            || self.zigzag.is_some()
            || self.zigzag_z.is_some()
            || self.square_z_offset.is_some()
            || self.zigzag_offset.is_some()
            || self.zigzag_z_offset.is_some()
            || self.square_z_period.is_some()
            || self.zigzag_period.is_some()
            || self.zigzag_z_period.is_some()
            || self.xmode.is_some()
            || self.bounce.is_some()
            || self.bounce_period.is_some()
            || self.bounce_offset.is_some()
            || self.tornado_period.is_some()
            || self.tornado_offset.is_some()
            || self.parabola_z.is_some()
            || self.attenuate_z.is_some()
            || self.confusion.is_some()
            || self.confusion_offset.is_some()
            || self.confusion_x_offset.is_some()
            || self.confusion_offset_cols.iter().any(Option::is_some)
            || self.flip.is_some()
            || self.invert.is_some()
            || self.tornado.is_some()
            || self.tipsy.is_some()
            || self.tipsy_offset.is_some()
            || self.tipsy_speed.is_some()
            || self.tiny.is_some()
            || self.bumpy.is_some()
            || self.bumpy_offset.is_some()
            || self.bumpy_period.is_some()
            || self.bumpy_cols.iter().any(Option::is_some)
            || self.tiny_cols.iter().any(Option::is_some)
            || self.move_x_cols.iter().any(Option::is_some)
            || self.move_y_cols.iter().any(Option::is_some)
            || self.pulse_inner.is_some()
            || self.pulse_outer.is_some()
            || self.pulse_period.is_some()
            || self.beat_period.is_some()
            || self.shrink_linear.is_some()
            || self.shrink_mult.is_some()
            || self.bounce_z.is_some()
            || self.bounce_z_offset.is_some()
            || self.bounce_z_period.is_some()
            || self.digital_z.is_some()
            || self.digital_z_offset.is_some()
            || self.digital_z_period.is_some()
            || self.digital_z_steps.is_some()
            || self.tornado_z.is_some()
            || self.tornado_z_offset.is_some()
            || self.tornado_z_period.is_some()
            || self.sawtooth.is_some()
            || self.sawtooth_period.is_some()
            || self.sawtooth_z.is_some()
            || self.sawtooth_z_period.is_some()
            || self.confusion_x.is_some()
            || self.confusion_y.is_some()
            || self.confusion_y_offset.is_some()
            || self.beat_offset.is_some()
            || self.beat_mult.is_some()
            || self.beat_y.is_some()
            || self.beat_y_offset.is_some()
            || self.beat_y_mult.is_some()
            || self.beat_y_period.is_some()
            || self.beat_z.is_some()
            || self.beat_z_offset.is_some()
            || self.beat_z_mult.is_some()
            || self.beat_z_period.is_some()
            || self.pulse_offset.is_some()
            || self.beat.is_some()
            || self.random_speed.is_some()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AppearanceOverrides {
    pub hidden: Option<f32>,
    pub hidden_offset: Option<f32>,
    pub sudden: Option<f32>,
    pub sudden_offset: Option<f32>,
    pub stealth: Option<f32>,
    pub stealth_cols: [Option<f32>; MAX_COLS],
    pub stealth_type: Option<bool>,
    pub stealth_past_receptors: Option<bool>,
    pub blink: Option<f32>,
    pub random_vanish: Option<f32>,
}

impl AppearanceOverrides {
    #[inline(always)]
    #[must_use]
    pub fn any(self) -> bool {
        self.hidden.is_some()
            || self.hidden_offset.is_some()
            || self.sudden.is_some()
            || self.sudden_offset.is_some()
            || self.stealth.is_some()
            || self.stealth_cols.iter().any(Option::is_some)
            || self.stealth_type.is_some()
            || self.stealth_past_receptors.is_some()
            || self.blink.is_some()
            || self.random_vanish.is_some()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VisibilityOverrides {
    pub dark: Option<f32>,
    pub dark_cols: [Option<f32>; MAX_COLS],
    pub blind: Option<f32>,
    pub cover: Option<f32>,
}

impl VisibilityOverrides {
    #[inline(always)]
    #[must_use]
    pub fn any(self) -> bool {
        self.dark.is_some()
            || self.blind.is_some()
            || self.cover.is_some()
            || self.dark_cols.iter().any(Option::is_some)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollOverrides {
    pub reverse: Option<f32>,
    pub split: Option<f32>,
    pub alternate: Option<f32>,
    pub cross: Option<f32>,
    pub centered: Option<f32>,
}

impl ScrollOverrides {
    #[inline(always)]
    #[must_use]
    pub const fn any(self) -> bool {
        self.reverse.is_some()
            || self.split.is_some()
            || self.alternate.is_some()
            || self.cross.is_some()
            || self.centered.is_some()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PerspectiveOverrides {
    pub tilt: Option<f32>,
    pub skew: Option<f32>,
}

impl PerspectiveOverrides {
    #[inline(always)]
    #[must_use]
    pub const fn any(self) -> bool {
        self.tilt.is_some() || self.skew.is_some()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AccelEffects {
    pub boost: f32,
    pub brake: f32,
    pub wave: f32,
    pub wave_period: f32,
    pub expand: f32,
    pub boomerang: f32,
}

impl AccelEffects {
    #[inline(always)]
    #[must_use]
    pub fn from_mask_bits(mask: u8) -> Self {
        Self {
            boost: f32::from((mask & ACCEL_MASK_BIT_BOOST) != 0),
            brake: f32::from((mask & ACCEL_MASK_BIT_BRAKE) != 0),
            wave: f32::from((mask & ACCEL_MASK_BIT_WAVE) != 0),
            wave_period: 0.0,
            expand: f32::from((mask & ACCEL_MASK_BIT_EXPAND) != 0),
            boomerang: f32::from((mask & ACCEL_MASK_BIT_BOOMERANG) != 0),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct VisualEffects {
    pub mod_timer_type: ModTimerType,
    pub dizzy_holds: bool,
    pub z_buffer: bool,
    pub cosecant: bool,
    pub drunk: f32,
    pub drunk_offset: f32,
    pub drunk_speed: f32,
    pub drunk_period: f32,
    pub dizzy: f32,
    pub twirl: f32,
    pub roll: f32,
    pub parabola_x: f32,
    pub attenuate_x: f32,
    pub parabola_y: f32,
    pub attenuate_y: f32,
    pub mod_timer_mult: f32,
    pub mod_timer_offset: f32,
    pub bumpy_x: f32,
    pub bumpy_x_offset: f32,
    pub bumpy_x_period: f32,
    pub tan_bumpy: f32,
    pub tan_bumpy_offset: f32,
    pub tan_bumpy_period: f32,
    pub tan_bumpy_x: f32,
    pub tan_bumpy_x_offset: f32,
    pub tan_bumpy_x_period: f32,
    pub drunk_z: f32,
    pub drunk_z_offset: f32,
    pub drunk_z_speed: f32,
    pub drunk_z_period: f32,
    pub tan_drunk: f32,
    pub tan_drunk_offset: f32,
    pub tan_drunk_speed: f32,
    pub tan_drunk_period: f32,
    pub tan_drunk_z: f32,
    pub tan_drunk_z_offset: f32,
    pub tan_drunk_z_speed: f32,
    pub tan_drunk_z_period: f32,
    pub draw_size: f32,
    pub draw_size_back: f32,
    pub square: f32,
    pub digital: f32,
    pub square_offset: f32,
    pub digital_steps: f32,
    pub digital_offset: f32,
    pub square_period: f32,
    pub digital_period: f32,
    pub square_z: f32,
    pub zigzag: f32,
    pub zigzag_z: f32,
    pub square_z_offset: f32,
    pub zigzag_offset: f32,
    pub zigzag_z_offset: f32,
    pub square_z_period: f32,
    pub zigzag_period: f32,
    pub zigzag_z_period: f32,
    pub xmode: f32,
    pub bounce: f32,
    pub bounce_period: f32,
    pub bounce_offset: f32,
    pub tornado_period: f32,
    pub tornado_offset: f32,
    pub parabola_z: f32,
    pub attenuate_z: f32,
    pub confusion: f32,
    pub confusion_offset: f32,
    pub confusion_x_offset: f32,
    pub confusion_offset_cols: [f32; MAX_COLS],
    pub big: f32,
    pub flip: f32,
    pub invert: f32,
    pub tornado: f32,
    pub tipsy: f32,
    pub tipsy_offset: f32,
    pub tipsy_speed: f32,
    pub tiny: f32,
    pub bumpy: f32,
    pub bumpy_offset: f32,
    pub bumpy_period: f32,
    pub bumpy_cols: [f32; MAX_COLS],
    pub tiny_cols: [f32; MAX_COLS],
    pub move_x_cols: [f32; MAX_COLS],
    pub move_y_cols: [f32; MAX_COLS],
    pub pulse_inner: f32,
    pub pulse_outer: f32,
    pub pulse_period: f32,
    pub beat_period: f32,
    pub shrink_linear: f32,
    pub shrink_mult: f32,
    pub bounce_z: f32,
    pub bounce_z_offset: f32,
    pub bounce_z_period: f32,
    pub digital_z: f32,
    pub digital_z_offset: f32,
    pub digital_z_period: f32,
    pub digital_z_steps: f32,
    pub tornado_z: f32,
    pub tornado_z_offset: f32,
    pub tornado_z_period: f32,
    pub sawtooth: f32,
    pub sawtooth_period: f32,
    pub sawtooth_z: f32,
    pub sawtooth_z_period: f32,
    pub confusion_x: f32,
    pub confusion_y: f32,
    pub confusion_y_offset: f32,
    pub beat_offset: f32,
    pub beat_mult: f32,
    pub beat_y: f32,
    pub beat_y_offset: f32,
    pub beat_y_mult: f32,
    pub beat_y_period: f32,
    pub beat_z: f32,
    pub beat_z_offset: f32,
    pub beat_z_mult: f32,
    pub beat_z_period: f32,
    pub pulse_offset: f32,
    pub beat: f32,
    pub random_speed: f32,
}

impl VisualEffects {
    #[inline(always)]
    fn signed_active(value: f32) -> bool {
        value.is_finite() && value.abs() > f32::EPSILON
    }

    #[inline(always)]
    #[must_use]
    pub fn from_mask_bits(mask: u16) -> Self {
        Self {
            mod_timer_type: ModTimerType::Default,
            dizzy_holds: false,
            z_buffer: false,
            cosecant: false,
            drunk: f32::from((mask & VISUAL_MASK_BIT_DRUNK) != 0),
            drunk_offset: 0.0,
            drunk_speed: 0.0,
            drunk_period: 0.0,
            dizzy: f32::from((mask & VISUAL_MASK_BIT_DIZZY) != 0),
            twirl: 0.0,
            roll: 0.0,
            parabola_x: 0.0,
            attenuate_x: 0.0,
            parabola_y: 0.0,
            attenuate_y: 0.0,
            mod_timer_mult: 0.0,
            mod_timer_offset: 0.0,
            bumpy_x: 0.0,
            bumpy_x_offset: 0.0,
            bumpy_x_period: 0.0,
            tan_bumpy: 0.0,
            tan_bumpy_offset: 0.0,
            tan_bumpy_period: 0.0,
            tan_bumpy_x: 0.0,
            tan_bumpy_x_offset: 0.0,
            tan_bumpy_x_period: 0.0,
            drunk_z: 0.0,
            drunk_z_offset: 0.0,
            drunk_z_speed: 0.0,
            drunk_z_period: 0.0,
            tan_drunk: 0.0,
            tan_drunk_offset: 0.0,
            tan_drunk_speed: 0.0,
            tan_drunk_period: 0.0,
            tan_drunk_z: 0.0,
            tan_drunk_z_offset: 0.0,
            tan_drunk_z_speed: 0.0,
            tan_drunk_z_period: 0.0,
            draw_size: 0.0,
            draw_size_back: 0.0,
            square: 0.0,
            digital: 0.0,
            square_offset: 0.0,
            digital_steps: 0.0,
            digital_offset: 0.0,
            square_period: 0.0,
            digital_period: 0.0,
            square_z: 0.0,
            zigzag: 0.0,
            zigzag_z: 0.0,
            square_z_offset: 0.0,
            zigzag_offset: 0.0,
            zigzag_z_offset: 0.0,
            square_z_period: 0.0,
            zigzag_period: 0.0,
            zigzag_z_period: 0.0,
            xmode: 0.0,
            bounce: 0.0,
            bounce_period: 0.0,
            bounce_offset: 0.0,
            tornado_period: 0.0,
            tornado_offset: 0.0,
            parabola_z: 0.0,
            attenuate_z: 0.0,
            confusion: f32::from((mask & VISUAL_MASK_BIT_CONFUSION) != 0),
            confusion_offset: 0.0,
            confusion_x_offset: 0.0,
            confusion_offset_cols: [0.0; MAX_COLS],
            big: f32::from((mask & VISUAL_MASK_BIT_BIG) != 0),
            flip: f32::from((mask & VISUAL_MASK_BIT_FLIP) != 0),
            invert: f32::from((mask & VISUAL_MASK_BIT_INVERT) != 0),
            tornado: f32::from((mask & VISUAL_MASK_BIT_TORNADO) != 0),
            tipsy: f32::from((mask & VISUAL_MASK_BIT_TIPSY) != 0),
            tipsy_offset: 0.0,
            tipsy_speed: 0.0,
            tiny: 0.0,
            bumpy: f32::from((mask & VISUAL_MASK_BIT_BUMPY) != 0),
            bumpy_offset: 0.0,
            bumpy_period: 0.0,
            bumpy_cols: [0.0; MAX_COLS],
            tiny_cols: [0.0; MAX_COLS],
            move_x_cols: [0.0; MAX_COLS],
            move_y_cols: [0.0; MAX_COLS],
            pulse_inner: 0.0,
            pulse_outer: 0.0,
            pulse_period: 0.0,
            beat_period: 0.0,
            shrink_linear: 0.0,
            shrink_mult: 0.0,
            bounce_z: 0.0,
            bounce_z_offset: 0.0,
            bounce_z_period: 0.0,
            digital_z: 0.0,
            digital_z_offset: 0.0,
            digital_z_period: 0.0,
            digital_z_steps: 0.0,
            tornado_z: 0.0,
            tornado_z_offset: 0.0,
            tornado_z_period: 0.0,
            sawtooth: 0.0,
            sawtooth_period: 0.0,
            sawtooth_z: 0.0,
            sawtooth_z_period: 0.0,
            confusion_x: 0.0,
            confusion_y: 0.0,
            confusion_y_offset: 0.0,
            beat_offset: 0.0,
            beat_mult: 0.0,
            beat_y: 0.0,
            beat_y_offset: 0.0,
            beat_y_mult: 0.0,
            beat_y_period: 0.0,
            beat_z: 0.0,
            beat_z_offset: 0.0,
            beat_z_mult: 0.0,
            beat_z_period: 0.0,
            pulse_offset: 0.0,
            beat: f32::from((mask & VISUAL_MASK_BIT_BEAT) != 0),
            random_speed: 0.0,
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn to_mask_bits(self) -> u16 {
        let mut mask = 0;
        if Self::signed_active(self.drunk) {
            mask |= VISUAL_MASK_BIT_DRUNK;
        }
        if Self::signed_active(self.dizzy) {
            mask |= VISUAL_MASK_BIT_DIZZY;
        }
        if Self::signed_active(self.confusion) {
            mask |= VISUAL_MASK_BIT_CONFUSION;
        }
        if self.big > f32::EPSILON {
            mask |= VISUAL_MASK_BIT_BIG;
        }
        if Self::signed_active(self.flip) {
            mask |= VISUAL_MASK_BIT_FLIP;
        }
        if Self::signed_active(self.invert) {
            mask |= VISUAL_MASK_BIT_INVERT;
        }
        if Self::signed_active(self.tornado) {
            mask |= VISUAL_MASK_BIT_TORNADO;
        }
        if Self::signed_active(self.tipsy) {
            mask |= VISUAL_MASK_BIT_TIPSY;
        }
        if Self::signed_active(self.bumpy)
            || self.bumpy_cols.iter().any(|v| Self::signed_active(*v))
        {
            mask |= VISUAL_MASK_BIT_BUMPY;
        }
        if Self::signed_active(self.beat) {
            mask |= VISUAL_MASK_BIT_BEAT;
        }
        mask
    }
}

const OUTRO_ATTACK_CLEAR_RATE: f32 = 1.0;
const OUTRO_ATTACK_CLEAR_EPSILON: f32 = 0.0001;

#[inline(always)]
fn approach_optional_visual(value: &mut Option<f32>, target: f32, step: f32) {
    let Some(current) = value.as_mut() else {
        return;
    };
    approach_f32(current, target, step);
    if (*current - target).abs() <= OUTRO_ATTACK_CLEAR_EPSILON {
        *value = None;
    }
}

#[inline(always)]
fn approach_optional_visual_cols(
    values: &mut [Option<f32>; MAX_COLS],
    targets: [f32; MAX_COLS],
    step: f32,
) {
    for (value, target) in values.iter_mut().zip(targets) {
        approach_optional_visual(value, target, step);
    }
}

pub fn approach_visual_overrides_to_base(
    visual: &mut VisualOverrides,
    base: VisualEffects,
    delta_time: f32,
) {
    visual.mod_timer_type = None;
    visual.dizzy_holds = None;
    visual.z_buffer = None;
    visual.cosecant = None;
    let step = delta_time * OUTRO_ATTACK_CLEAR_RATE;
    approach_optional_visual(&mut visual.drunk, base.drunk, step);
    approach_optional_visual(&mut visual.drunk_offset, base.drunk_offset, step);
    approach_optional_visual(&mut visual.drunk_speed, base.drunk_speed, step);
    approach_optional_visual(&mut visual.drunk_period, base.drunk_period, step);
    approach_optional_visual(&mut visual.dizzy, base.dizzy, step);
    approach_optional_visual(&mut visual.twirl, base.twirl, step);
    approach_optional_visual(&mut visual.roll, base.roll, step);
    approach_optional_visual(&mut visual.parabola_x, base.parabola_x, step);
    approach_optional_visual(&mut visual.attenuate_x, base.attenuate_x, step);
    approach_optional_visual(&mut visual.parabola_y, base.parabola_y, step);
    approach_optional_visual(&mut visual.attenuate_y, base.attenuate_y, step);
    approach_optional_visual(&mut visual.mod_timer_mult, base.mod_timer_mult, step);
    approach_optional_visual(&mut visual.mod_timer_offset, base.mod_timer_offset, step);
    approach_optional_visual(&mut visual.bumpy_x, base.bumpy_x, step);
    approach_optional_visual(&mut visual.bumpy_x_offset, base.bumpy_x_offset, step);
    approach_optional_visual(&mut visual.bumpy_x_period, base.bumpy_x_period, step);
    approach_optional_visual(&mut visual.tan_bumpy, base.tan_bumpy, step);
    approach_optional_visual(&mut visual.tan_bumpy_offset, base.tan_bumpy_offset, step);
    approach_optional_visual(&mut visual.tan_bumpy_period, base.tan_bumpy_period, step);
    approach_optional_visual(&mut visual.tan_bumpy_x, base.tan_bumpy_x, step);
    approach_optional_visual(
        &mut visual.tan_bumpy_x_offset,
        base.tan_bumpy_x_offset,
        step,
    );
    approach_optional_visual(
        &mut visual.tan_bumpy_x_period,
        base.tan_bumpy_x_period,
        step,
    );
    approach_optional_visual(&mut visual.drunk_z, base.drunk_z, step);
    approach_optional_visual(&mut visual.drunk_z_offset, base.drunk_z_offset, step);
    approach_optional_visual(&mut visual.drunk_z_speed, base.drunk_z_speed, step);
    approach_optional_visual(&mut visual.drunk_z_period, base.drunk_z_period, step);
    approach_optional_visual(&mut visual.tan_drunk, base.tan_drunk, step);
    approach_optional_visual(&mut visual.tan_drunk_offset, base.tan_drunk_offset, step);
    approach_optional_visual(&mut visual.tan_drunk_speed, base.tan_drunk_speed, step);
    approach_optional_visual(&mut visual.tan_drunk_period, base.tan_drunk_period, step);
    approach_optional_visual(&mut visual.tan_drunk_z, base.tan_drunk_z, step);
    approach_optional_visual(
        &mut visual.tan_drunk_z_offset,
        base.tan_drunk_z_offset,
        step,
    );
    approach_optional_visual(&mut visual.tan_drunk_z_speed, base.tan_drunk_z_speed, step);
    approach_optional_visual(
        &mut visual.tan_drunk_z_period,
        base.tan_drunk_z_period,
        step,
    );
    approach_optional_visual(&mut visual.draw_size, base.draw_size, step);
    approach_optional_visual(&mut visual.draw_size_back, base.draw_size_back, step);
    approach_optional_visual(&mut visual.square, base.square, step);
    approach_optional_visual(&mut visual.digital, base.digital, step);
    approach_optional_visual(&mut visual.square_offset, base.square_offset, step);
    approach_optional_visual(&mut visual.digital_steps, base.digital_steps, step);
    approach_optional_visual(&mut visual.digital_offset, base.digital_offset, step);
    approach_optional_visual(&mut visual.square_period, base.square_period, step);
    approach_optional_visual(&mut visual.digital_period, base.digital_period, step);
    approach_optional_visual(&mut visual.square_z, base.square_z, step);
    approach_optional_visual(&mut visual.zigzag, base.zigzag, step);
    approach_optional_visual(&mut visual.zigzag_z, base.zigzag_z, step);
    approach_optional_visual(&mut visual.square_z_offset, base.square_z_offset, step);
    approach_optional_visual(&mut visual.zigzag_offset, base.zigzag_offset, step);
    approach_optional_visual(&mut visual.zigzag_z_offset, base.zigzag_z_offset, step);
    approach_optional_visual(&mut visual.square_z_period, base.square_z_period, step);
    approach_optional_visual(&mut visual.zigzag_period, base.zigzag_period, step);
    approach_optional_visual(&mut visual.zigzag_z_period, base.zigzag_z_period, step);
    approach_optional_visual(&mut visual.xmode, base.xmode, step);
    approach_optional_visual(&mut visual.bounce, base.bounce, step);
    approach_optional_visual(&mut visual.bounce_period, base.bounce_period, step);
    approach_optional_visual(&mut visual.bounce_offset, base.bounce_offset, step);
    approach_optional_visual(&mut visual.tornado_period, base.tornado_period, step);
    approach_optional_visual(&mut visual.tornado_offset, base.tornado_offset, step);
    approach_optional_visual(&mut visual.parabola_z, base.parabola_z, step);
    approach_optional_visual(&mut visual.attenuate_z, base.attenuate_z, step);
    approach_optional_visual(&mut visual.confusion, base.confusion, step);
    approach_optional_visual(&mut visual.confusion_offset, base.confusion_offset, step);
    approach_optional_visual(
        &mut visual.confusion_x_offset,
        base.confusion_x_offset,
        step,
    );
    approach_optional_visual_cols(
        &mut visual.confusion_offset_cols,
        base.confusion_offset_cols,
        step,
    );
    approach_optional_visual(&mut visual.flip, base.flip, step);
    approach_optional_visual(&mut visual.invert, base.invert, step);
    approach_optional_visual(&mut visual.tornado, base.tornado, step);
    approach_optional_visual(&mut visual.tipsy, base.tipsy, step);
    approach_optional_visual(&mut visual.tipsy_offset, base.tipsy_offset, step);
    approach_optional_visual(&mut visual.tipsy_speed, base.tipsy_speed, step);
    approach_optional_visual(&mut visual.tiny, base.tiny, step);
    approach_optional_visual(&mut visual.bumpy, base.bumpy, step);
    approach_optional_visual(&mut visual.bumpy_offset, base.bumpy_offset, step);
    approach_optional_visual(&mut visual.bumpy_period, base.bumpy_period, step);
    approach_optional_visual_cols(&mut visual.bumpy_cols, base.bumpy_cols, step);
    approach_optional_visual_cols(&mut visual.tiny_cols, base.tiny_cols, step);
    approach_optional_visual_cols(&mut visual.move_x_cols, base.move_x_cols, step);
    approach_optional_visual_cols(&mut visual.move_y_cols, base.move_y_cols, step);
    approach_optional_visual(&mut visual.pulse_inner, base.pulse_inner, step);
    approach_optional_visual(&mut visual.pulse_outer, base.pulse_outer, step);
    approach_optional_visual(&mut visual.pulse_period, base.pulse_period, step);
    approach_optional_visual(&mut visual.beat_period, base.beat_period, step);
    approach_optional_visual(&mut visual.shrink_linear, base.shrink_linear, step);
    approach_optional_visual(&mut visual.shrink_mult, base.shrink_mult, step);
    approach_optional_visual(&mut visual.bounce_z, base.bounce_z, step);
    approach_optional_visual(&mut visual.bounce_z_offset, base.bounce_z_offset, step);
    approach_optional_visual(&mut visual.bounce_z_period, base.bounce_z_period, step);
    approach_optional_visual(&mut visual.digital_z, base.digital_z, step);
    approach_optional_visual(&mut visual.digital_z_offset, base.digital_z_offset, step);
    approach_optional_visual(&mut visual.digital_z_period, base.digital_z_period, step);
    approach_optional_visual(&mut visual.digital_z_steps, base.digital_z_steps, step);
    approach_optional_visual(&mut visual.tornado_z, base.tornado_z, step);
    approach_optional_visual(&mut visual.tornado_z_offset, base.tornado_z_offset, step);
    approach_optional_visual(&mut visual.tornado_z_period, base.tornado_z_period, step);
    approach_optional_visual(&mut visual.sawtooth, base.sawtooth, step);
    approach_optional_visual(&mut visual.sawtooth_period, base.sawtooth_period, step);
    approach_optional_visual(&mut visual.sawtooth_z, base.sawtooth_z, step);
    approach_optional_visual(&mut visual.sawtooth_z_period, base.sawtooth_z_period, step);
    approach_optional_visual(&mut visual.confusion_x, base.confusion_x, step);
    approach_optional_visual(&mut visual.confusion_y, base.confusion_y, step);
    approach_optional_visual(
        &mut visual.confusion_y_offset,
        base.confusion_y_offset,
        step,
    );
    approach_optional_visual(&mut visual.beat_offset, base.beat_offset, step);
    approach_optional_visual(&mut visual.beat_mult, base.beat_mult, step);
    approach_optional_visual(&mut visual.beat_y, base.beat_y, step);
    approach_optional_visual(&mut visual.beat_y_offset, base.beat_y_offset, step);
    approach_optional_visual(&mut visual.beat_y_mult, base.beat_y_mult, step);
    approach_optional_visual(&mut visual.beat_y_period, base.beat_y_period, step);
    approach_optional_visual(&mut visual.beat_z, base.beat_z, step);
    approach_optional_visual(&mut visual.beat_z_offset, base.beat_z_offset, step);
    approach_optional_visual(&mut visual.beat_z_mult, base.beat_z_mult, step);
    approach_optional_visual(&mut visual.beat_z_period, base.beat_z_period, step);
    approach_optional_visual(&mut visual.pulse_offset, base.pulse_offset, step);
    approach_optional_visual(&mut visual.beat, base.beat, step);
    approach_optional_visual(&mut visual.random_speed, base.random_speed, step);
}

#[inline(always)]
fn approach_attack_cols(
    current: &mut [Option<f32>; MAX_COLS],
    target: [Option<f32>; MAX_COLS],
    base: [f32; MAX_COLS],
    speed: [Option<f32>; MAX_COLS],
    delta_time: f32,
) {
    for (((current, target), base), speed) in current.iter_mut().zip(target).zip(base).zip(speed) {
        approach_attack_value(current, target, base, speed, delta_time, 1.0);
    }
}

pub fn approach_visual_overrides_to_target(
    current: &mut VisualOverrides,
    target: VisualOverrides,
    speed: VisualOverrides,
    base: VisualEffects,
    delta_time: f32,
) {
    current.mod_timer_type = target.mod_timer_type;
    current.dizzy_holds = target.dizzy_holds;
    current.z_buffer = target.z_buffer;
    current.cosecant = target.cosecant;
    approach_attack_value(
        &mut current.drunk,
        target.drunk,
        base.drunk,
        speed.drunk,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.drunk_offset,
        target.drunk_offset,
        base.drunk_offset,
        speed.drunk_offset,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.drunk_speed,
        target.drunk_speed,
        base.drunk_speed,
        speed.drunk_speed,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.drunk_period,
        target.drunk_period,
        base.drunk_period,
        speed.drunk_period,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.dizzy,
        target.dizzy,
        base.dizzy,
        speed.dizzy,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.twirl,
        target.twirl,
        base.twirl,
        speed.twirl,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.roll,
        target.roll,
        base.roll,
        speed.roll,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.parabola_x,
        target.parabola_x,
        base.parabola_x,
        speed.parabola_x,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.attenuate_x,
        target.attenuate_x,
        base.attenuate_x,
        speed.attenuate_x,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.parabola_y,
        target.parabola_y,
        base.parabola_y,
        speed.parabola_y,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.attenuate_y,
        target.attenuate_y,
        base.attenuate_y,
        speed.attenuate_y,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.mod_timer_mult,
        target.mod_timer_mult,
        base.mod_timer_mult,
        speed.mod_timer_mult,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.mod_timer_offset,
        target.mod_timer_offset,
        base.mod_timer_offset,
        speed.mod_timer_offset,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.bumpy_x,
        target.bumpy_x,
        base.bumpy_x,
        speed.bumpy_x,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.bumpy_x_offset,
        target.bumpy_x_offset,
        base.bumpy_x_offset,
        speed.bumpy_x_offset,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.bumpy_x_period,
        target.bumpy_x_period,
        base.bumpy_x_period,
        speed.bumpy_x_period,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.tan_bumpy,
        target.tan_bumpy,
        base.tan_bumpy,
        speed.tan_bumpy,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.tan_bumpy_offset,
        target.tan_bumpy_offset,
        base.tan_bumpy_offset,
        speed.tan_bumpy_offset,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.tan_bumpy_period,
        target.tan_bumpy_period,
        base.tan_bumpy_period,
        speed.tan_bumpy_period,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.tan_bumpy_x,
        target.tan_bumpy_x,
        base.tan_bumpy_x,
        speed.tan_bumpy_x,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.tan_bumpy_x_offset,
        target.tan_bumpy_x_offset,
        base.tan_bumpy_x_offset,
        speed.tan_bumpy_x_offset,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.tan_bumpy_x_period,
        target.tan_bumpy_x_period,
        base.tan_bumpy_x_period,
        speed.tan_bumpy_x_period,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.drunk_z,
        target.drunk_z,
        base.drunk_z,
        speed.drunk_z,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.drunk_z_offset,
        target.drunk_z_offset,
        base.drunk_z_offset,
        speed.drunk_z_offset,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.drunk_z_speed,
        target.drunk_z_speed,
        base.drunk_z_speed,
        speed.drunk_z_speed,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.drunk_z_period,
        target.drunk_z_period,
        base.drunk_z_period,
        speed.drunk_z_period,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.tan_drunk,
        target.tan_drunk,
        base.tan_drunk,
        speed.tan_drunk,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.tan_drunk_offset,
        target.tan_drunk_offset,
        base.tan_drunk_offset,
        speed.tan_drunk_offset,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.tan_drunk_speed,
        target.tan_drunk_speed,
        base.tan_drunk_speed,
        speed.tan_drunk_speed,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.tan_drunk_period,
        target.tan_drunk_period,
        base.tan_drunk_period,
        speed.tan_drunk_period,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.tan_drunk_z,
        target.tan_drunk_z,
        base.tan_drunk_z,
        speed.tan_drunk_z,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.tan_drunk_z_offset,
        target.tan_drunk_z_offset,
        base.tan_drunk_z_offset,
        speed.tan_drunk_z_offset,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.tan_drunk_z_speed,
        target.tan_drunk_z_speed,
        base.tan_drunk_z_speed,
        speed.tan_drunk_z_speed,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.tan_drunk_z_period,
        target.tan_drunk_z_period,
        base.tan_drunk_z_period,
        speed.tan_drunk_z_period,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.draw_size,
        target.draw_size,
        base.draw_size,
        speed.draw_size,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.draw_size_back,
        target.draw_size_back,
        base.draw_size_back,
        speed.draw_size_back,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.square,
        target.square,
        base.square,
        speed.square,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.digital,
        target.digital,
        base.digital,
        speed.digital,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.square_offset,
        target.square_offset,
        base.square_offset,
        speed.square_offset,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.digital_steps,
        target.digital_steps,
        base.digital_steps,
        speed.digital_steps,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.digital_offset,
        target.digital_offset,
        base.digital_offset,
        speed.digital_offset,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.square_period,
        target.square_period,
        base.square_period,
        speed.square_period,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.digital_period,
        target.digital_period,
        base.digital_period,
        speed.digital_period,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.square_z,
        target.square_z,
        base.square_z,
        speed.square_z,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.zigzag,
        target.zigzag,
        base.zigzag,
        speed.zigzag,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.zigzag_z,
        target.zigzag_z,
        base.zigzag_z,
        speed.zigzag_z,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.square_z_offset,
        target.square_z_offset,
        base.square_z_offset,
        speed.square_z_offset,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.zigzag_offset,
        target.zigzag_offset,
        base.zigzag_offset,
        speed.zigzag_offset,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.zigzag_z_offset,
        target.zigzag_z_offset,
        base.zigzag_z_offset,
        speed.zigzag_z_offset,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.square_z_period,
        target.square_z_period,
        base.square_z_period,
        speed.square_z_period,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.zigzag_period,
        target.zigzag_period,
        base.zigzag_period,
        speed.zigzag_period,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.zigzag_z_period,
        target.zigzag_z_period,
        base.zigzag_z_period,
        speed.zigzag_z_period,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.xmode,
        target.xmode,
        base.xmode,
        speed.xmode,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.bounce,
        target.bounce,
        base.bounce,
        speed.bounce,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.bounce_period,
        target.bounce_period,
        base.bounce_period,
        speed.bounce_period,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.bounce_offset,
        target.bounce_offset,
        base.bounce_offset,
        speed.bounce_offset,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.tornado_period,
        target.tornado_period,
        base.tornado_period,
        speed.tornado_period,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.tornado_offset,
        target.tornado_offset,
        base.tornado_offset,
        speed.tornado_offset,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.parabola_z,
        target.parabola_z,
        base.parabola_z,
        speed.parabola_z,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.attenuate_z,
        target.attenuate_z,
        base.attenuate_z,
        speed.attenuate_z,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.confusion,
        target.confusion,
        base.confusion,
        speed.confusion,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.confusion_offset,
        target.confusion_offset,
        base.confusion_offset,
        speed.confusion_offset,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.confusion_x_offset,
        target.confusion_x_offset,
        base.confusion_x_offset,
        speed.confusion_x_offset,
        delta_time,
        1.0,
    );
    approach_attack_cols(
        &mut current.confusion_offset_cols,
        target.confusion_offset_cols,
        base.confusion_offset_cols,
        speed.confusion_offset_cols,
        delta_time,
    );
    approach_attack_value(
        &mut current.flip,
        target.flip,
        base.flip,
        speed.flip,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.invert,
        target.invert,
        base.invert,
        speed.invert,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.tornado,
        target.tornado,
        base.tornado,
        speed.tornado,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.tipsy,
        target.tipsy,
        base.tipsy,
        speed.tipsy,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.tipsy_offset,
        target.tipsy_offset,
        base.tipsy_offset,
        speed.tipsy_offset,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.tipsy_speed,
        target.tipsy_speed,
        base.tipsy_speed,
        speed.tipsy_speed,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.tiny,
        target.tiny,
        base.tiny,
        speed.tiny,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.bumpy,
        target.bumpy,
        base.bumpy,
        speed.bumpy,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.bumpy_offset,
        target.bumpy_offset,
        base.bumpy_offset,
        speed.bumpy_offset,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.bumpy_period,
        target.bumpy_period,
        base.bumpy_period,
        speed.bumpy_period,
        delta_time,
        1.0,
    );
    approach_attack_cols(
        &mut current.bumpy_cols,
        target.bumpy_cols,
        base.bumpy_cols,
        speed.bumpy_cols,
        delta_time,
    );
    approach_attack_cols(
        &mut current.tiny_cols,
        target.tiny_cols,
        base.tiny_cols,
        speed.tiny_cols,
        delta_time,
    );
    approach_attack_cols(
        &mut current.move_x_cols,
        target.move_x_cols,
        base.move_x_cols,
        speed.move_x_cols,
        delta_time,
    );
    approach_attack_cols(
        &mut current.move_y_cols,
        target.move_y_cols,
        base.move_y_cols,
        speed.move_y_cols,
        delta_time,
    );
    approach_attack_value(
        &mut current.pulse_inner,
        target.pulse_inner,
        base.pulse_inner,
        speed.pulse_inner,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.pulse_outer,
        target.pulse_outer,
        base.pulse_outer,
        speed.pulse_outer,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.pulse_period,
        target.pulse_period,
        base.pulse_period,
        speed.pulse_period,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.beat_period,
        target.beat_period,
        base.beat_period,
        speed.beat_period,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.shrink_linear,
        target.shrink_linear,
        base.shrink_linear,
        speed.shrink_linear,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.shrink_mult,
        target.shrink_mult,
        base.shrink_mult,
        speed.shrink_mult,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.bounce_z,
        target.bounce_z,
        base.bounce_z,
        speed.bounce_z,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.bounce_z_offset,
        target.bounce_z_offset,
        base.bounce_z_offset,
        speed.bounce_z_offset,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.bounce_z_period,
        target.bounce_z_period,
        base.bounce_z_period,
        speed.bounce_z_period,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.digital_z,
        target.digital_z,
        base.digital_z,
        speed.digital_z,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.digital_z_offset,
        target.digital_z_offset,
        base.digital_z_offset,
        speed.digital_z_offset,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.digital_z_period,
        target.digital_z_period,
        base.digital_z_period,
        speed.digital_z_period,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.digital_z_steps,
        target.digital_z_steps,
        base.digital_z_steps,
        speed.digital_z_steps,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.tornado_z,
        target.tornado_z,
        base.tornado_z,
        speed.tornado_z,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.tornado_z_offset,
        target.tornado_z_offset,
        base.tornado_z_offset,
        speed.tornado_z_offset,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.tornado_z_period,
        target.tornado_z_period,
        base.tornado_z_period,
        speed.tornado_z_period,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.sawtooth,
        target.sawtooth,
        base.sawtooth,
        speed.sawtooth,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.sawtooth_period,
        target.sawtooth_period,
        base.sawtooth_period,
        speed.sawtooth_period,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.sawtooth_z,
        target.sawtooth_z,
        base.sawtooth_z,
        speed.sawtooth_z,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.sawtooth_z_period,
        target.sawtooth_z_period,
        base.sawtooth_z_period,
        speed.sawtooth_z_period,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.confusion_x,
        target.confusion_x,
        base.confusion_x,
        speed.confusion_x,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.confusion_y,
        target.confusion_y,
        base.confusion_y,
        speed.confusion_y,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.confusion_y_offset,
        target.confusion_y_offset,
        base.confusion_y_offset,
        speed.confusion_y_offset,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.beat_offset,
        target.beat_offset,
        base.beat_offset,
        speed.beat_offset,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.beat_mult,
        target.beat_mult,
        base.beat_mult,
        speed.beat_mult,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.beat_y,
        target.beat_y,
        base.beat_y,
        speed.beat_y,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.beat_y_offset,
        target.beat_y_offset,
        base.beat_y_offset,
        speed.beat_y_offset,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.beat_y_mult,
        target.beat_y_mult,
        base.beat_y_mult,
        speed.beat_y_mult,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.beat_y_period,
        target.beat_y_period,
        base.beat_y_period,
        speed.beat_y_period,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.beat_z,
        target.beat_z,
        base.beat_z,
        speed.beat_z,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.beat_z_offset,
        target.beat_z_offset,
        base.beat_z_offset,
        speed.beat_z_offset,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.beat_z_mult,
        target.beat_z_mult,
        base.beat_z_mult,
        speed.beat_z_mult,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.beat_z_period,
        target.beat_z_period,
        base.beat_z_period,
        speed.beat_z_period,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.pulse_offset,
        target.pulse_offset,
        base.pulse_offset,
        speed.pulse_offset,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.beat,
        target.beat,
        base.beat,
        speed.beat,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.random_speed,
        target.random_speed,
        base.random_speed,
        speed.random_speed,
        delta_time,
        1.0,
    );
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AppearanceEffects {
    pub hidden: f32,
    pub hidden_offset: f32,
    pub sudden: f32,
    pub sudden_offset: f32,
    pub stealth: f32,
    pub stealth_cols: [f32; MAX_COLS],
    pub stealth_type: bool,
    pub stealth_past_receptors: bool,
    pub blink: f32,
    pub random_vanish: f32,
}

impl AppearanceEffects {
    #[inline(always)]
    #[must_use]
    pub fn from_mask_bits(mask: u8) -> Self {
        Self {
            hidden: f32::from((mask & APPEARANCE_MASK_BIT_HIDDEN) != 0),
            hidden_offset: 0.0,
            sudden: f32::from(
                (mask & (APPEARANCE_MASK_BIT_SUDDEN | APPEARANCE_MASK_BIT_DYNAMIC_SUDDEN)) != 0,
            ),
            sudden_offset: 0.0,
            stealth: f32::from((mask & APPEARANCE_MASK_BIT_STEALTH) != 0),
            stealth_cols: [0.0; MAX_COLS],
            stealth_type: false,
            stealth_past_receptors: false,
            blink: f32::from((mask & APPEARANCE_MASK_BIT_BLINK) != 0),
            random_vanish: f32::from((mask & APPEARANCE_MASK_BIT_RANDOM_VANISH) != 0),
        }
    }

    #[inline(always)]
    #[must_use]
    pub const fn approach_speeds() -> Self {
        Self {
            hidden: 1.0,
            hidden_offset: 1.0,
            sudden: 1.0,
            sudden_offset: 1.0,
            stealth: 1.0,
            stealth_cols: [1.0; MAX_COLS],
            stealth_type: false,
            stealth_past_receptors: false,
            blink: 1.0,
            random_vanish: 1.0,
        }
    }
}

#[inline(always)]
pub fn apply_appearance_target(
    target: &mut AppearanceEffects,
    speed: &mut AppearanceEffects,
    overrides: AppearanceOverrides,
    override_speeds: AppearanceOverrides,
) {
    if let Some(value) = overrides.hidden {
        target.hidden = value;
        speed.hidden = override_speeds.hidden.unwrap_or(1.0).max(0.0);
    }
    if let Some(value) = overrides.hidden_offset {
        target.hidden_offset = value;
        speed.hidden_offset = override_speeds.hidden_offset.unwrap_or(1.0).max(0.0);
    }
    if let Some(value) = overrides.sudden {
        target.sudden = value;
        speed.sudden = override_speeds.sudden.unwrap_or(1.0).max(0.0);
    }
    if let Some(value) = overrides.sudden_offset {
        target.sudden_offset = value;
        speed.sudden_offset = override_speeds.sudden_offset.unwrap_or(1.0).max(0.0);
    }
    if let Some(value) = overrides.stealth {
        target.stealth = value;
        speed.stealth = override_speeds.stealth.unwrap_or(1.0).max(0.0);
    }
    for col in 0..MAX_COLS {
        if let Some(value) = overrides.stealth_cols[col] {
            target.stealth_cols[col] = value;
            speed.stealth_cols[col] = override_speeds.stealth_cols[col].unwrap_or(1.0).max(0.0);
        }
    }
    if let Some(value) = overrides.stealth_type {
        target.stealth_type = value;
    }
    if let Some(value) = overrides.stealth_past_receptors {
        target.stealth_past_receptors = value;
    }
    if let Some(value) = overrides.blink {
        target.blink = value;
        speed.blink = override_speeds.blink.unwrap_or(1.0).max(0.0);
    }
    if let Some(value) = overrides.random_vanish {
        target.random_vanish = value;
        speed.random_vanish = override_speeds.random_vanish.unwrap_or(1.0).max(0.0);
    }
}

#[inline(always)]
pub fn approach_appearance_effects(
    current: &mut AppearanceEffects,
    target: AppearanceEffects,
    speed: AppearanceEffects,
    delta_time: f32,
) {
    let delta_time = delta_time.max(0.0);
    approach_f32(
        &mut current.hidden,
        target.hidden,
        delta_time * speed.hidden,
    );
    approach_f32(
        &mut current.hidden_offset,
        target.hidden_offset,
        delta_time * speed.hidden_offset,
    );
    approach_f32(
        &mut current.sudden,
        target.sudden,
        delta_time * speed.sudden,
    );
    approach_f32(
        &mut current.sudden_offset,
        target.sudden_offset,
        delta_time * speed.sudden_offset,
    );
    approach_f32(
        &mut current.stealth,
        target.stealth,
        delta_time * speed.stealth,
    );
    for col in 0..MAX_COLS {
        approach_f32(
            &mut current.stealth_cols[col],
            target.stealth_cols[col],
            delta_time * speed.stealth_cols[col],
        );
    }
    // PlayerOptions::Approach copies boolean options immediately.
    current.stealth_type = target.stealth_type;
    current.stealth_past_receptors = target.stealth_past_receptors;
    approach_f32(&mut current.blink, target.blink, delta_time * speed.blink);
    approach_f32(
        &mut current.random_vanish,
        target.random_vanish,
        delta_time * speed.random_vanish,
    );
}

#[derive(Clone, Copy, Debug, Default)]
pub struct VisibilityEffects {
    pub dark: f32,
    pub dark_cols: [f32; MAX_COLS],
    pub blind: f32,
    pub cover: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChartAttackEffects {
    pub insert_mask: u8,
    pub remove_mask: u8,
    pub holds_mask: u8,
    pub turn_bits: u16,
}

impl ChartAttackEffects {
    #[inline(always)]
    #[must_use]
    pub const fn has_note_masks(self) -> bool {
        self.insert_mask != 0 || self.remove_mask != 0 || self.holds_mask != 0
    }
}
