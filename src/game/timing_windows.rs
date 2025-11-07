// Shared timing window definitions to keep gameplay and visuals in sync.

// All base windows are in seconds.
pub const TIMING_WINDOW_ADD_S: f32 = 0.0015; // +1.5ms padding applied by ITG/SM

pub const BASE_W1_S: f32 = 0.0215;
pub const BASE_W2_S: f32 = 0.0430;
pub const BASE_W3_S: f32 = 0.1020;
pub const BASE_W4_S: f32 = 0.1350;
pub const BASE_W5_S: f32 = 0.1800;

// Mines use a distinct base window
pub const BASE_MINE_S: f32 = 0.0700;

#[inline(always)]
pub fn effective_windows_s() -> [f32; 5] {
    [
        BASE_W1_S + TIMING_WINDOW_ADD_S,
        BASE_W2_S + TIMING_WINDOW_ADD_S,
        BASE_W3_S + TIMING_WINDOW_ADD_S,
        BASE_W4_S + TIMING_WINDOW_ADD_S,
        BASE_W5_S + TIMING_WINDOW_ADD_S,
    ]
}

#[inline(always)]
pub fn effective_windows_ms() -> [f32; 5] {
    let s = effective_windows_s();
    [
        s[0] * 1000.0,
        s[1] * 1000.0,
        s[2] * 1000.0,
        s[3] * 1000.0,
        s[4] * 1000.0,
    ]
}

#[inline(always)]
pub fn mine_window_s() -> f32 { BASE_MINE_S + TIMING_WINDOW_ADD_S }

