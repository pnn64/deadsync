pub(crate) const HOLD_BODY_LEGACY_SEGMENT_LIMIT: usize = 512;
pub(crate) const HOLD_BODY_SEGMENT_SAFETY_MAX: usize = 65_536;
pub(crate) const BUMPY_Z_MAGNITUDE: f32 = 40.0;
pub(crate) const BUMPY_Z_ANGLE_DIVISOR: f32 = 16.0;
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
pub const COLUMN_CUE_Y_OFFSET: f32 = 80.0;
pub(crate) const CROSSOVER_CUE_HEIGHT_REDUCTION: f32 = 270.0;
pub(crate) const COLUMN_FLASH_DEFAULT_Y_OFFSET: f32 = 80.0;
pub(crate) const COLUMN_FLASH_COMPACT_Y_OFFSET: f32 = 70.0;
pub(crate) const COLUMN_FLASH_COMPACT_HEIGHT_TRIM: f32 = 270.0;
pub(crate) const COLUMN_FLASH_DEFAULT_FADE: f32 = 0.333;
pub(crate) const COLUMN_FLASH_COMPACT_FADE: f32 = 0.2;
pub(crate) const COLUMN_FLASH_NORMAL_ALPHA: f32 = 0.66;
pub(crate) const COLUMN_FLASH_DIMMED_ALPHA: f32 = 0.3;
pub(crate) const ERROR_BAR_SEG_ALPHA_BASE: f32 = 0.3;
pub(crate) const BOOST_MOD_MIN_CLAMP: f32 = -400.0;
pub(crate) const BOOST_MOD_MAX_CLAMP: f32 = 400.0;
pub(crate) const WAVE_MOD_MAGNITUDE: f32 = 20.0;
pub(crate) const WAVE_MOD_HEIGHT: f32 = 38.0;
pub(crate) const EXPAND_MULTIPLIER_FREQUENCY: f32 = 3.0;
pub(crate) const EXPAND_MULTIPLIER_SCALE_TO_LOW: f32 = 0.75;
pub(crate) const EXPAND_MULTIPLIER_SCALE_TO_HIGH: f32 = 1.75;
pub(crate) const MAX_NOTES_AFTER: usize = 64;
pub(crate) const FANTASTIC_BLUE_RGBA: [f32; 4] = rgba8_const(0x21, 0xcc, 0xe8);
pub(crate) const EXCELLENT_RGBA: [f32; 4] = rgba8_const(0xe2, 0x9c, 0x18);
pub(crate) const GREAT_RGBA: [f32; 4] = rgba8_const(0x66, 0xc9, 0x55);
pub(crate) const DECENT_RGBA: [f32; 4] = rgba8_const(0xb4, 0x5c, 0xff);
pub(crate) const WAY_OFF_RGBA: [f32; 4] = rgba8_const(0xc9, 0x85, 0x5e);
pub(crate) const FA_PLUS_WHITE_RGBA: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

const fn rgba8_const(r: u8, g: u8, b: u8) -> [f32; 4] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}
