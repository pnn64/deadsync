#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActiveInputSlot {
    pub source: InputSource,
    pub input_slot: u32,
    pub lane_mask: LaneMask,
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
    active_slots: ActiveInputSlots,
    lane_counts: [u16; MAX_COLS],
    // Bit `i` is set exactly when `lane_counts[i] != 0`; `update_slot` owns both.
    pressed_lane_mask: LaneMask,
}

impl Default for GameplayInputState {
    fn default() -> Self {
        Self {
            prev_inputs: [false; MAX_COLS],
            lane_pressed_since_ns: [None; MAX_COLS],
            active_slots: ActiveInputSlots::default(),
            lane_counts: [0; MAX_COLS],
            pressed_lane_mask: 0,
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
    pub const fn pressed_lane_mask(&self) -> LaneMask {
        self.pressed_lane_mask
    }

    #[inline(always)]
    pub fn slot_lane_is_down(&self, lane_idx: usize, source: InputSource, input_slot: u32) -> bool {
        self.active_slots
            .slot_lane_is_down(lane_idx, source, input_slot)
    }

    #[inline(always)]
    pub(crate) fn prepare_slot_update(
        &self,
        lane_idx: usize,
        source: InputSource,
        input_slot: u32,
    ) -> PreparedInputSlotUpdate {
        self.active_slots.prepare(lane_idx, source, input_slot)
    }

    #[inline(always)]
    pub(crate) fn update_prepared_slot(
        &mut self,
        prepared: PreparedInputSlotUpdate,
        pressed: bool,
    ) -> LaneInputUpdate {
        let lane_idx = prepared.lane_idx();
        let update = self
            .active_slots
            .update_prepared(&mut self.lane_counts, prepared, pressed);
        self.update_pressed_lane_mask(lane_idx, update.is_down);
        update
    }

    #[inline(always)]
    pub fn update_slot(
        &mut self,
        lane_idx: usize,
        source: InputSource,
        input_slot: u32,
        pressed: bool,
    ) -> LaneInputUpdate {
        let prepared = self.prepare_slot_update(lane_idx, source, input_slot);
        self.update_prepared_slot(prepared, pressed)
    }

    #[inline(always)]
    fn update_pressed_lane_mask(&mut self, lane_idx: usize, is_down: bool) {
        if lane_idx < MAX_COLS {
            let bit = input_lane_bit(lane_idx);
            if is_down {
                self.pressed_lane_mask |= bit;
            } else {
                self.pressed_lane_mask &= !bit;
            }
        }
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
        self.active_slots.clear();
        self.lane_counts.fill(0);
        self.pressed_lane_mask = 0;
    }
}
