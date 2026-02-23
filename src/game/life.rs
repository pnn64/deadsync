pub const REGEN_COMBO_AFTER_MISS: u32 = 5;

// In SM, life regeneration is tied to LifePercentChangeHeld. Simply Love sets
// TimingWindowSecondsHold to 0.32s, so mirror that grace window. Reference:
// itgmania/Themes/Simply Love/Scripts/SL_Init.lua

pub const LIFE_FANTASTIC: f32 = 0.008;
pub const LIFE_EXCELLENT: f32 = 0.008;
pub const LIFE_GREAT: f32 = 0.004;
pub const LIFE_DECENT: f32 = 0.0;
pub const LIFE_WAY_OFF: f32 = -0.050;
pub const LIFE_MISS: f32 = -0.100;
pub const LIFE_HIT_MINE: f32 = -0.050;
pub const LIFE_HELD: f32 = 0.008;
pub const LIFE_LET_GO: f32 = -0.080;
