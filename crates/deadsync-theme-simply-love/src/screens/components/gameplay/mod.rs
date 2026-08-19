mod display_mods;
pub mod gameplay_stats;
pub mod notefield;
pub mod score_counter;
pub mod step_stats_gifs;

// Song-prewarmed frame-layout slots for short values that can change during
// play. Slots are independent so a stable actor hits its own last layout even
// when other HUD values change. The inline namespace covers two mini indicators,
// two offsets, six live-timing values, seven notefield counters per player, two
// life values, four clocks, the visible step-stat count rows, and current BPM.
// Four fallback buffers per actor cover fill/stroke plus their shadow copies.
// Combo numbers use a separate retained-digit namespace.
pub(crate) const FRAME_TEXT_MINI_BASE: u8 = 0;
pub(crate) const FRAME_TEXT_OFFSET_BASE: u8 = 2;
pub(crate) const FRAME_TEXT_LIVE_TIMING_BASE: u8 = 4;
pub(crate) const FRAME_TEXT_COUNTER_BASE: u8 = 10;
// Prepared-u32 slots use a separate cache namespace from frame-inline slots.
pub(crate) const FRAME_TEXT_COMBO_BASE: u8 = 10;
pub(crate) const FRAME_TEXT_COUNTDOWN_BASE: u8 = FRAME_TEXT_COMBO_BASE + 2;
pub(crate) const FRAME_TEXT_LIFE_BASE: u8 =
    FRAME_TEXT_COUNTER_BASE + deadsync_notefield::COUNTER_TEXT_SLOTS_PER_PLAYER * 2;
pub(crate) const FRAME_TEXT_TIME_BASE: u8 = FRAME_TEXT_LIFE_BASE + 2;
pub(crate) const FRAME_TEXT_STATS_COUNT_LEFT_BASE: u8 = FRAME_TEXT_TIME_BASE + 4;
pub(crate) const FRAME_TEXT_STATS_COUNT_ROWS: u8 = 7;
pub(crate) const FRAME_TEXT_STATS_COUNT_RIGHT_BASE: u8 =
    FRAME_TEXT_STATS_COUNT_LEFT_BASE + FRAME_TEXT_STATS_COUNT_ROWS * 2;
pub(crate) const FRAME_TEXT_BPM: u8 =
    FRAME_TEXT_STATS_COUNT_RIGHT_BASE + FRAME_TEXT_STATS_COUNT_ROWS;
pub(crate) const FRAME_TEXT_SLOT_COUNT: usize = FRAME_TEXT_BPM as usize + 1;
pub(crate) const FRAME_TEXT_VERTEX_BUFFERS: usize = FRAME_TEXT_SLOT_COUNT * 4;
