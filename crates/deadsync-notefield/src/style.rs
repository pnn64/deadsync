// Canonical DeadSync field metrics, independent of HUD/theme styling.
// Player options, noteskins, view modes, and chart modifiers are applied by the
// composers on top of these established defaults; themes cannot replace them.
pub(crate) const LAYOUT_WIDTH_MIN: f32 = 640.0;
pub(crate) const LAYOUT_WIDTH_MAX: f32 = 854.0;
pub(crate) const SIDE_CENTER_X_RATIO: f32 = 0.25;
pub(crate) const RECEPTOR_NORMAL_Y: f32 = -125.0;
pub(crate) const RECEPTOR_REVERSE_Y: f32 = 145.0;
pub(crate) const RECEPTOR_Z: i16 = 100;
pub(crate) const RECEPTOR_GLOW_Z: i16 = 105;
pub(crate) const HOLD_EXPLOSION_Z: i16 = 145;
pub(crate) const HOLD_BODY_Z: i16 = 110;
pub(crate) const HOLD_CAP_Z: i16 = 110;
pub(crate) const HOLD_GLOW_Z: i16 = 111;
pub(crate) const TAP_EXPLOSION_Z: i16 = 150;
pub(crate) const MINE_EXPLOSION_Z: i16 = 101;
pub(crate) const NOTE_Z: i16 = 140;
pub(crate) const MINE_CORE_SIZE_RATIO: f32 = 0.45;
pub(crate) const MEASURE_LINE_OVERSCAN_Y: f32 = 400.0;
pub(crate) const MEASURE_LINE_Z: i16 = 80;

pub(crate) const HOLD_BODY_LEGACY_SEGMENT_LIMIT: usize = 512;
pub(crate) const HOLD_BODY_SEGMENT_SAFETY_MAX: usize = 65_536;
pub(crate) const BUMPY_Z_MAGNITUDE: f32 = 40.0;
pub(crate) const BUMPY_Z_ANGLE_DIVISOR: f32 = 16.0;
pub(crate) const BEAT_OFFSET_HEIGHT: f32 = 15.0;
pub(crate) const BEAT_PI_HEIGHT: f32 = 2.0;
pub(crate) const BLINK_MOD_FREQUENCY: f32 = 0.3333;
pub(crate) const CENTER_LINE_Y: f32 = 160.0;
pub(crate) const DRUNK_COLUMN_FREQUENCY: f32 = 0.2;
pub(crate) const DRUNK_OFFSET_FREQUENCY: f32 = 10.0;
pub(crate) const DRUNK_ARROW_MAGNITUDE: f32 = 0.5;
pub(crate) const FADE_DIST_Y: f32 = 40.0;
pub(crate) const TORNADO_X_OFFSET_FREQUENCY: f32 = 6.0;
pub(crate) const TIPSY_TIMER_FREQUENCY: f32 = 1.2;
pub(crate) const TIPSY_COLUMN_FREQUENCY: f32 = 1.8;
pub(crate) const TIPSY_ARROW_MAGNITUDE: f32 = 0.4;
pub(crate) const ARROW_EFFECT_PIXEL_SIZE: f32 = 64.0;
pub(crate) const BOOST_MOD_MIN_CLAMP: f32 = -400.0;
pub(crate) const BOOST_MOD_MAX_CLAMP: f32 = 400.0;
pub(crate) const BRAKE_MOD_MIN_CLAMP: f32 = -400.0;
pub(crate) const BRAKE_MOD_MAX_CLAMP: f32 = 400.0;
pub(crate) const WAVE_MOD_MAGNITUDE: f32 = 20.0;
pub(crate) const WAVE_MOD_HEIGHT: f32 = 38.0;
pub(crate) const EXPAND_MULTIPLIER_FREQUENCY: f32 = 3.0;
pub(crate) const EXPAND_MULTIPLIER_SCALE_FROM_LOW: f32 = -1.0;
pub(crate) const EXPAND_MULTIPLIER_SCALE_FROM_HIGH: f32 = 1.0;
pub(crate) const EXPAND_MULTIPLIER_SCALE_TO_LOW: f32 = 0.75;
pub(crate) const EXPAND_MULTIPLIER_SCALE_TO_HIGH: f32 = 1.75;
pub(crate) const EXPAND_SPEED_SCALE_FROM_LOW: f32 = 0.0;
pub(crate) const EXPAND_SPEED_SCALE_FROM_HIGH: f32 = 1.0;
pub(crate) const EXPAND_SPEED_SCALE_TO_LOW: f32 = 1.0;
pub(crate) const MAX_NOTES_AFTER: usize = 64;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receptor_glow_draws_under_hold_body() {
        assert!(RECEPTOR_Z < HOLD_BODY_Z);
        assert!(RECEPTOR_GLOW_Z < HOLD_BODY_Z);
    }

    #[test]
    fn hold_glow_draws_over_hold_body_like_itg_second_pass() {
        assert!(HOLD_BODY_Z < HOLD_GLOW_Z);
        assert!(HOLD_GLOW_Z < NOTE_Z);
    }
}
