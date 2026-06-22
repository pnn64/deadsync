#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActiveInputSlot {
    pub source: InputSource,
    pub input_slot: u32,
    pub lane_mask: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LaneInputUpdate {
    pub was_down: bool,
    pub is_down: bool,
    pub slot_was_down: bool,
    pub slot_table_full: bool,
}

#[derive(Clone, Debug)]
pub struct GameplayInputState {
    pub prev_inputs: [bool; MAX_COLS],
    pub lane_pressed_since_ns: [Option<SongTimeNs>; MAX_COLS],
    slots: [ActiveInputSlot; MAX_ACTIVE_INPUT_SLOTS],
    slot_count: usize,
    lane_counts: [u16; MAX_COLS],
}

impl Default for GameplayInputState {
    fn default() -> Self {
        Self {
            prev_inputs: [false; MAX_COLS],
            lane_pressed_since_ns: [None; MAX_COLS],
            slots: [EMPTY_ACTIVE_INPUT_SLOT; MAX_ACTIVE_INPUT_SLOTS],
            slot_count: 0,
            lane_counts: [0; MAX_COLS],
        }
    }
}

impl GameplayInputState {
    #[inline(always)]
    pub fn lane_is_pressed(&self, col: usize) -> bool {
        self.lane_counts.get(col).copied().unwrap_or(0) != 0
    }

    #[inline(always)]
    pub fn lane_counts(&self) -> &[u16; MAX_COLS] {
        &self.lane_counts
    }

    #[inline(always)]
    pub fn slot_lane_is_down(&self, lane_idx: usize, source: InputSource, input_slot: u32) -> bool {
        active_input_slot_lane_is_down(&self.slots, self.slot_count, lane_idx, source, input_slot)
    }

    #[inline(always)]
    pub fn update_slot(
        &mut self,
        lane_idx: usize,
        source: InputSource,
        input_slot: u32,
        pressed: bool,
    ) -> LaneInputUpdate {
        update_active_input_slot(
            &mut self.slots,
            &mut self.slot_count,
            &mut self.lane_counts,
            lane_idx,
            source,
            input_slot,
            pressed,
        )
    }

    #[inline(always)]
    pub fn press_lane(&mut self, lane_idx: usize, event_music_time_ns: SongTimeNs) {
        if let Some(slot) = self.lane_pressed_since_ns.get_mut(lane_idx) {
            *slot = Some(event_music_time_ns);
        }
    }

    #[inline(always)]
    pub fn release_lane(&mut self, lane_idx: usize) {
        if let Some(slot) = self.lane_pressed_since_ns.get_mut(lane_idx) {
            *slot = None;
        }
    }

    #[inline(always)]
    pub fn reset_live_state(&mut self) {
        self.prev_inputs.fill(false);
        self.lane_pressed_since_ns.fill(None);
        self.slot_count = 0;
        self.lane_counts.fill(0);
        self.slots = [EMPTY_ACTIVE_INPUT_SLOT; MAX_ACTIVE_INPUT_SLOTS];
    }
}

