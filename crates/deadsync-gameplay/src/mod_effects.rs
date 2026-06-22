pub const MINI_PERCENT_MIN: f32 = -100.0;
pub const MINI_PERCENT_MAX: f32 = 150.0;

#[inline(always)]
pub fn effective_mini_percent(
    active_mini_percent: Option<f32>,
    fallback_mini_percent: f32,
    base_cleared: bool,
) -> f32 {
    let mini = active_mini_percent
        .filter(|v| v.is_finite())
        .unwrap_or(if base_cleared {
            0.0
        } else {
            fallback_mini_percent
        });
    mini.clamp(MINI_PERCENT_MIN, MINI_PERCENT_MAX)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MiniAttackMode {
    Absolute,
    Delta,
}

#[inline(always)]
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
    if delta_time <= f32::EPSILON {
        *current = Some(target);
        return;
    }
    let Some(speed) = speed.filter(|value| value.is_finite()) else {
        *current = Some(target);
        return;
    };
    let step = delta_time.max(0.0) * speed.max(0.0) * unit_scale;
    if step <= f32::EPSILON {
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
    if let Some(value) = current.as_mut() {
        *value = value.clamp(MINI_PERCENT_MIN, MINI_PERCENT_MAX);
    }
}

#[inline(always)]
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
    mini.clamp(MINI_PERCENT_MIN, MINI_PERCENT_MAX) / 100.0
}

#[inline(always)]
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
pub fn player_draw_scale_for_mini(tilt: f32, mini_value: f32) -> f32 {
    (1.0 + 0.5 * tilt.abs()) * (1.0 + mini_value.abs())
}

#[inline(always)]
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

#[derive(Clone, Copy, Debug, Default)]
pub struct AccelOverrides {
    pub boost: Option<f32>,
    pub brake: Option<f32>,
    pub wave: Option<f32>,
    pub expand: Option<f32>,
    pub boomerang: Option<f32>,
}

impl AccelOverrides {
    #[inline(always)]
    pub fn any(self) -> bool {
        self.boost.is_some()
            || self.brake.is_some()
            || self.wave.is_some()
            || self.expand.is_some()
            || self.boomerang.is_some()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct VisualOverrides {
    pub drunk: Option<f32>,
    pub dizzy: Option<f32>,
    pub confusion: Option<f32>,
    pub confusion_offset: Option<f32>,
    pub confusion_offset_cols: [Option<f32>; MAX_COLS],
    pub flip: Option<f32>,
    pub invert: Option<f32>,
    pub tornado: Option<f32>,
    pub tipsy: Option<f32>,
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
    pub pulse_offset: Option<f32>,
    pub beat: Option<f32>,
}

impl Default for VisualOverrides {
    fn default() -> Self {
        Self {
            drunk: None,
            dizzy: None,
            confusion: None,
            confusion_offset: None,
            confusion_offset_cols: [None; MAX_COLS],
            flip: None,
            invert: None,
            tornado: None,
            tipsy: None,
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
            pulse_offset: None,
            beat: None,
        }
    }
}

impl VisualOverrides {
    #[inline(always)]
    pub fn any(self) -> bool {
        self.drunk.is_some()
            || self.dizzy.is_some()
            || self.confusion.is_some()
            || self.confusion_offset.is_some()
            || self.confusion_offset_cols.iter().any(Option::is_some)
            || self.flip.is_some()
            || self.invert.is_some()
            || self.tornado.is_some()
            || self.tipsy.is_some()
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
            || self.pulse_offset.is_some()
            || self.beat.is_some()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AppearanceOverrides {
    pub hidden: Option<f32>,
    pub hidden_offset: Option<f32>,
    pub sudden: Option<f32>,
    pub sudden_offset: Option<f32>,
    pub stealth: Option<f32>,
    pub blink: Option<f32>,
    pub random_vanish: Option<f32>,
}

impl AppearanceOverrides {
    #[inline(always)]
    pub fn any(self) -> bool {
        self.hidden.is_some()
            || self.hidden_offset.is_some()
            || self.sudden.is_some()
            || self.sudden_offset.is_some()
            || self.stealth.is_some()
            || self.blink.is_some()
            || self.random_vanish.is_some()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct VisibilityOverrides {
    pub dark: Option<f32>,
    pub blind: Option<f32>,
    pub cover: Option<f32>,
}

impl VisibilityOverrides {
    #[inline(always)]
    pub fn any(self) -> bool {
        self.dark.is_some() || self.blind.is_some() || self.cover.is_some()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ScrollOverrides {
    pub reverse: Option<f32>,
    pub split: Option<f32>,
    pub alternate: Option<f32>,
    pub cross: Option<f32>,
    pub centered: Option<f32>,
}

impl ScrollOverrides {
    #[inline(always)]
    pub fn any(self) -> bool {
        self.reverse.is_some()
            || self.split.is_some()
            || self.alternate.is_some()
            || self.cross.is_some()
            || self.centered.is_some()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PerspectiveOverrides {
    pub tilt: Option<f32>,
    pub skew: Option<f32>,
}

impl PerspectiveOverrides {
    #[inline(always)]
    pub fn any(self) -> bool {
        self.tilt.is_some() || self.skew.is_some()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AccelEffects {
    pub boost: f32,
    pub brake: f32,
    pub wave: f32,
    pub expand: f32,
    pub boomerang: f32,
}

impl AccelEffects {
    #[inline(always)]
    pub fn from_mask_bits(mask: u8) -> Self {
        Self {
            boost: f32::from((mask & ACCEL_MASK_BIT_BOOST) != 0),
            brake: f32::from((mask & ACCEL_MASK_BIT_BRAKE) != 0),
            wave: f32::from((mask & ACCEL_MASK_BIT_WAVE) != 0),
            expand: f32::from((mask & ACCEL_MASK_BIT_EXPAND) != 0),
            boomerang: f32::from((mask & ACCEL_MASK_BIT_BOOMERANG) != 0),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct VisualEffects {
    pub drunk: f32,
    pub dizzy: f32,
    pub confusion: f32,
    pub confusion_offset: f32,
    pub confusion_offset_cols: [f32; MAX_COLS],
    pub big: f32,
    pub flip: f32,
    pub invert: f32,
    pub tornado: f32,
    pub tipsy: f32,
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
    pub pulse_offset: f32,
    pub beat: f32,
}

impl VisualEffects {
    #[inline(always)]
    pub fn from_mask_bits(mask: u16) -> Self {
        Self {
            drunk: f32::from((mask & VISUAL_MASK_BIT_DRUNK) != 0),
            dizzy: f32::from((mask & VISUAL_MASK_BIT_DIZZY) != 0),
            confusion: f32::from((mask & VISUAL_MASK_BIT_CONFUSION) != 0),
            confusion_offset: 0.0,
            confusion_offset_cols: [0.0; MAX_COLS],
            big: f32::from((mask & VISUAL_MASK_BIT_BIG) != 0),
            flip: f32::from((mask & VISUAL_MASK_BIT_FLIP) != 0),
            invert: f32::from((mask & VISUAL_MASK_BIT_INVERT) != 0),
            tornado: f32::from((mask & VISUAL_MASK_BIT_TORNADO) != 0),
            tipsy: f32::from((mask & VISUAL_MASK_BIT_TIPSY) != 0),
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
            pulse_offset: 0.0,
            beat: f32::from((mask & VISUAL_MASK_BIT_BEAT) != 0),
        }
    }

    #[inline(always)]
    pub fn to_mask_bits(self) -> u16 {
        let mut mask = 0;
        if self.drunk > f32::EPSILON {
            mask |= VISUAL_MASK_BIT_DRUNK;
        }
        if self.dizzy > f32::EPSILON {
            mask |= VISUAL_MASK_BIT_DIZZY;
        }
        if self.confusion > f32::EPSILON {
            mask |= VISUAL_MASK_BIT_CONFUSION;
        }
        if self.big > f32::EPSILON {
            mask |= VISUAL_MASK_BIT_BIG;
        }
        if self.flip > f32::EPSILON {
            mask |= VISUAL_MASK_BIT_FLIP;
        }
        if self.invert > f32::EPSILON {
            mask |= VISUAL_MASK_BIT_INVERT;
        }
        if self.tornado > f32::EPSILON {
            mask |= VISUAL_MASK_BIT_TORNADO;
        }
        if self.tipsy > f32::EPSILON {
            mask |= VISUAL_MASK_BIT_TIPSY;
        }
        if self.bumpy > f32::EPSILON || self.bumpy_cols.iter().any(|v| *v > f32::EPSILON) {
            mask |= VISUAL_MASK_BIT_BUMPY;
        }
        if self.beat > f32::EPSILON {
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
    let step = delta_time * OUTRO_ATTACK_CLEAR_RATE;
    approach_optional_visual(&mut visual.drunk, base.drunk, step);
    approach_optional_visual(&mut visual.dizzy, base.dizzy, step);
    approach_optional_visual(&mut visual.confusion, base.confusion, step);
    approach_optional_visual(&mut visual.confusion_offset, base.confusion_offset, step);
    approach_optional_visual_cols(
        &mut visual.confusion_offset_cols,
        base.confusion_offset_cols,
        step,
    );
    approach_optional_visual(&mut visual.flip, base.flip, step);
    approach_optional_visual(&mut visual.invert, base.invert, step);
    approach_optional_visual(&mut visual.tornado, base.tornado, step);
    approach_optional_visual(&mut visual.tipsy, base.tipsy, step);
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
    approach_optional_visual(&mut visual.pulse_offset, base.pulse_offset, step);
    approach_optional_visual(&mut visual.beat, base.beat, step);
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
    approach_attack_value(
        &mut current.drunk,
        target.drunk,
        base.drunk,
        speed.drunk,
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
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AppearanceEffects {
    pub hidden: f32,
    pub hidden_offset: f32,
    pub sudden: f32,
    pub sudden_offset: f32,
    pub stealth: f32,
    pub blink: f32,
    pub random_vanish: f32,
}

impl AppearanceEffects {
    #[inline(always)]
    pub fn from_mask_bits(mask: u8) -> Self {
        Self {
            hidden: f32::from((mask & APPEARANCE_MASK_BIT_HIDDEN) != 0),
            hidden_offset: 0.0,
            sudden: f32::from((mask & APPEARANCE_MASK_BIT_SUDDEN) != 0),
            sudden_offset: 0.0,
            stealth: f32::from((mask & APPEARANCE_MASK_BIT_STEALTH) != 0),
            blink: f32::from((mask & APPEARANCE_MASK_BIT_BLINK) != 0),
            random_vanish: f32::from((mask & APPEARANCE_MASK_BIT_RANDOM_VANISH) != 0),
        }
    }

    #[inline(always)]
    pub fn approach_speeds() -> Self {
        Self {
            hidden: 1.0,
            hidden_offset: 1.0,
            sudden: 1.0,
            sudden_offset: 1.0,
            stealth: 1.0,
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
    pub const fn has_note_masks(self) -> bool {
        self.insert_mask != 0 || self.remove_mask != 0 || self.holds_mask != 0
    }
}

