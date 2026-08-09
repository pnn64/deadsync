mod display_mods;
pub mod gameplay_stats;
pub mod notefield;
pub mod score_counter;
pub mod step_stats_gifs;

// Song-prewarmed frame-layout slots for short values that can change during
// play. Slots are independent so a stable actor hits its own last layout even
// when other HUD values change. The inline namespace covers two mini indicators,
// two offsets, six timing pairs, and two run timers. Four fallback buffers per
// actor cover fill/stroke plus their shadow copies. Combo numbers use a separate
// retained-digit namespace.
pub(crate) const FRAME_TEXT_MINI_BASE: u8 = 0;
pub(crate) const FRAME_TEXT_OFFSET_BASE: u8 = 2;
pub(crate) const FRAME_TEXT_LIVE_TIMING_BASE: u8 = 4;
pub(crate) const FRAME_TEXT_TIMER_BASE: u8 = 10;
pub(crate) const FRAME_TEXT_COMBO_BASE: u8 = 10;
pub(crate) const FRAME_TEXT_VERTEX_BUFFERS: usize = 12 * 4;
