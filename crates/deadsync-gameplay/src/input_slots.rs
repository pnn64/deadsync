pub const EMPTY_ACTIVE_INPUT_SLOT: ActiveInputSlot = ActiveInputSlot {
    source: InputSource::Keyboard,
    input_slot: 0,
    lane_mask: 0,
};

#[derive(Clone, Copy, Debug)]
/// A lookup result that stays valid until the matching edge mutates the table.
/// No active-slot update may occur between `prepare` and `update_prepared`.
pub(crate) struct PreparedInputSlotUpdate {
    key: u64,
    lane_idx: u8,
    index: u8,
    slot_was_down: bool,
}

const PREPARED_INPUT_SLOT_NONE: u8 = u8::MAX;
const PREPARED_INPUT_LANE_NONE: u8 = 0x0f;
const _: () = assert!(MAX_ACTIVE_INPUT_SLOTS < PREPARED_INPUT_SLOT_NONE as usize);
const _: () = assert!(MAX_COLS < PREPARED_INPUT_LANE_NONE as usize);

impl PreparedInputSlotUpdate {
    #[inline(always)]
    pub(crate) const fn slot_was_down(self) -> bool {
        self.slot_was_down
    }

    #[inline(always)]
    pub(crate) const fn lane_idx(self) -> usize {
        self.lane_idx as usize
    }

    #[inline(always)]
    const fn index(self) -> Option<usize> {
        if self.index == PREPARED_INPUT_SLOT_NONE {
            None
        } else {
            Some(self.index as usize)
        }
    }

    #[inline(always)]
    pub(crate) const fn source(self) -> InputSource {
        if self.key & (1 << ACTIVE_INPUT_SOURCE_SHIFT) == 0 {
            InputSource::Keyboard
        } else {
            InputSource::Gamepad
        }
    }

    #[inline(always)]
    pub(crate) const fn input_slot(self) -> u32 {
        self.key as u32
    }
}

/// Game-thread active input bindings in fixed song-lifetime storage.
///
/// Each record packs the full slot id, source, and ten-lane mask into one word.
/// `slot_count` bounds every scan and removal swap-fills holes. The table never
/// allocates, grows, or hashes.
#[derive(Clone, Debug)]
struct ActiveInputSlots {
    entries: [u64; MAX_ACTIVE_INPUT_SLOTS],
    slot_count: usize,
}

const ACTIVE_INPUT_SOURCE_SHIFT: u32 = 32;
const ACTIVE_INPUT_LANE_SHIFT: u32 = 33;
const ACTIVE_INPUT_ID_MASK: u64 = (1_u64 << ACTIVE_INPUT_LANE_SHIFT) - 1;
const ACTIVE_INPUT_LANE_MASK: u64 =
    ((1_u64 << MAX_COLS) - 1) << ACTIVE_INPUT_LANE_SHIFT;
const _: () = assert!(ACTIVE_INPUT_LANE_SHIFT + MAX_COLS as u32 <= u64::BITS);

impl Default for ActiveInputSlots {
    fn default() -> Self {
        Self {
            entries: [0; MAX_ACTIVE_INPUT_SLOTS],
            slot_count: 0,
        }
    }
}

impl ActiveInputSlots {
    #[inline(always)]
    fn find(&self, key: u64) -> Option<usize> {
        if self.slot_count == 0 {
            return None;
        }
        self.entries[..self.slot_count]
            .iter()
            .position(|&entry| entry & ACTIVE_INPUT_ID_MASK == key)
    }

    #[inline(always)]
    fn prepare(
        &self,
        lane_idx: usize,
        source: InputSource,
        input_slot: u32,
    ) -> PreparedInputSlotUpdate {
        let key = active_input_key(source, input_slot);
        let index = self.find(key);
        let slot_was_down = index.is_some_and(|index| {
            lane_idx < MAX_COLS && self.entries[index] & active_input_lane_bit(lane_idx) != 0
        });
        let lane_idx = if lane_idx < MAX_COLS {
            lane_idx as u8
        } else {
            PREPARED_INPUT_LANE_NONE
        };
        let index = index.map_or(PREPARED_INPUT_SLOT_NONE, |index| index as u8);
        PreparedInputSlotUpdate {
            key,
            lane_idx,
            index,
            slot_was_down,
        }
    }

    #[inline(always)]
    fn slot_lane_is_down(
        &self,
        lane_idx: usize,
        source: InputSource,
        input_slot: u32,
    ) -> bool {
        self.prepare(lane_idx, source, input_slot).slot_was_down()
    }

    #[inline(always)]
    fn update_prepared(
        &mut self,
        lane_counts: &mut [u16; MAX_COLS],
        prepared: PreparedInputSlotUpdate,
        pressed: bool,
    ) -> LaneInputUpdate {
        let lane_idx = prepared.lane_idx();
        if lane_idx >= MAX_COLS {
            return LaneInputUpdate::default();
        }
        let prepared_index = prepared.index();
        debug_assert!(prepared_index.is_none_or(|index| {
            index < self.slot_count
                && self.entries[index] & ACTIVE_INPUT_ID_MASK == prepared.key
        }));
        let bit = active_input_lane_bit(lane_idx);
        let was_down = lane_counts[lane_idx] != 0;
        let mut slot_table_full = false;

        if pressed {
            let index = match prepared_index {
                Some(index) => Some(index),
                None if self.slot_count < MAX_ACTIVE_INPUT_SLOTS => {
                    let index = self.slot_count;
                    self.slot_count += 1;
                    self.entries[index] = prepared.key;
                    Some(index)
                }
                None => {
                    slot_table_full = true;
                    None
                }
            };
            if let Some(index) = index
                && !prepared.slot_was_down()
            {
                self.entries[index] |= bit;
                lane_counts[lane_idx] = lane_counts[lane_idx].saturating_add(1);
            }
        } else if let Some(index) = prepared_index
            && prepared.slot_was_down()
        {
            self.entries[index] &= !bit;
            lane_counts[lane_idx] = lane_counts[lane_idx].saturating_sub(1);
            if self.entries[index] & ACTIVE_INPUT_LANE_MASK == 0 {
                self.slot_count -= 1;
                if index < self.slot_count {
                    self.entries[index] = self.entries[self.slot_count];
                }
            }
        }

        LaneInputUpdate {
            was_down,
            is_down: lane_counts[lane_idx] != 0,
            slot_was_down: prepared.slot_was_down(),
            slot_table_full,
        }
    }

    #[inline(always)]
    const fn clear(&mut self) {
        self.slot_count = 0;
    }
}

#[inline(always)]
const fn active_input_key(source: InputSource, input_slot: u32) -> u64 {
    input_slot as u64 | (matches!(source, InputSource::Gamepad) as u64) << ACTIVE_INPUT_SOURCE_SHIFT
}

#[inline(always)]
const fn active_input_lane_bit(lane_idx: usize) -> u64 {
    (input_lane_bit(lane_idx) as u64) << ACTIVE_INPUT_LANE_SHIFT
}

#[inline(always)]
#[must_use]
pub const fn remap_live_input_lane(
    play_style: GameplayInputPlayStyle,
    player_side: GameplayInputPlayerSide,
    lane: Lane,
) -> Option<Lane> {
    if play_style.is_pump() {
        let (pad, local_col) = match lane {
            Lane::Left => (GameplayInputPlayerSide::P1, 0),
            Lane::Down => (GameplayInputPlayerSide::P1, 1),
            Lane::Col8 => (GameplayInputPlayerSide::P1, 2),
            Lane::Up => (GameplayInputPlayerSide::P1, 3),
            Lane::Right => (GameplayInputPlayerSide::P1, 4),
            Lane::P2Left => (GameplayInputPlayerSide::P2, 0),
            Lane::P2Down => (GameplayInputPlayerSide::P2, 1),
            Lane::Col9 => (GameplayInputPlayerSide::P2, 2),
            Lane::P2Up => (GameplayInputPlayerSide::P2, 3),
            Lane::P2Right => (GameplayInputPlayerSide::P2, 4),
        };
        if matches!(play_style, GameplayInputPlayStyle::PumpSingle) {
            let same_pad = matches!(
                (pad, player_side),
                (GameplayInputPlayerSide::P1, GameplayInputPlayerSide::P1)
                    | (GameplayInputPlayerSide::P2, GameplayInputPlayerSide::P2)
            );
            return if same_pad {
                lane_from_column(local_col)
            } else {
                None
            };
        }
        let col = local_col
            + if matches!(pad, GameplayInputPlayerSide::P2) {
                5
            } else {
                0
            };
        return lane_from_column(col);
    }
    match (play_style, player_side, lane) {
        // Single-player: reject the other side entirely so only one set of
        // bindings can play.
        (
            GameplayInputPlayStyle::Single,
            GameplayInputPlayerSide::P1,
            Lane::P2Left | Lane::P2Down | Lane::P2Up | Lane::P2Right,
        ) => None,
        (
            GameplayInputPlayStyle::Single,
            GameplayInputPlayerSide::P2,
            Lane::Left | Lane::Down | Lane::Up | Lane::Right,
        ) => None,
        // P2-only single: remap P2 lanes into the 4-col field.
        (GameplayInputPlayStyle::Single, GameplayInputPlayerSide::P2, Lane::P2Left) => {
            Some(Lane::Left)
        }
        (GameplayInputPlayStyle::Single, GameplayInputPlayerSide::P2, Lane::P2Down) => {
            Some(Lane::Down)
        }
        (GameplayInputPlayStyle::Single, GameplayInputPlayerSide::P2, Lane::P2Up) => Some(Lane::Up),
        (GameplayInputPlayStyle::Single, GameplayInputPlayerSide::P2, Lane::P2Right) => {
            Some(Lane::Right)
        }
        _ => Some(lane),
    }
}

#[inline(always)]
#[must_use]
pub const fn live_input_lane_for_queue(
    autoplay_enabled: bool,
    play_style: GameplayInputPlayStyle,
    player_side: GameplayInputPlayerSide,
    lane: Lane,
    num_cols: usize,
) -> Option<Lane> {
    if autoplay_enabled {
        return None;
    }
    let Some(lane) = remap_live_input_lane(play_style, player_side, lane) else {
        return None;
    };
    if lane.index() >= num_cols {
        return None;
    }
    Some(lane)
}

#[inline(always)]
#[must_use]
pub const fn input_lane_bit(lane_idx: usize) -> LaneMask {
    1u16 << lane_idx
}

#[inline(always)]
#[must_use]
pub const fn input_lane_mask(num_cols: usize) -> LaneMask {
    if num_cols >= MAX_COLS {
        (1u16 << MAX_COLS) - 1
    } else {
        (1u16 << num_cols) - 1
    }
}

#[inline(always)]
#[must_use]
pub const fn lane_inputs_from_mask(mask: LaneMask, num_cols: usize) -> [bool; MAX_COLS] {
    let mut inputs = [false; MAX_COLS];
    let mut lanes = mask & input_lane_mask(num_cols);
    while lanes != 0 {
        let col = lanes.trailing_zeros() as usize;
        inputs[col] = true;
        lanes &= lanes - 1;
    }
    inputs
}

#[inline(always)]
#[must_use]
pub const fn normalized_input_slot(input_slot: u32, fallback_slot: u32, invalid_slot: u32) -> u32 {
    if input_slot == invalid_slot {
        fallback_slot
    } else {
        input_slot
    }
}

#[inline(always)]
#[must_use]
pub const fn should_warn_unmapped_input_clock(
    last_warn_ns: SongTimeNs,
    song_time_ns: SongTimeNs,
) -> bool {
    last_warn_ns == UNMAPPED_INPUT_CLOCK_WARN_NEVER_NS
        || song_time_ns < last_warn_ns
        || song_time_ns.saturating_sub(last_warn_ns) >= UNMAPPED_INPUT_CLOCK_WARN_INTERVAL_NS
}

static LAST_UNMAPPED_INPUT_CLOCK_WARN_NS: AtomicI64 =
    AtomicI64::new(UNMAPPED_INPUT_CLOCK_WARN_NEVER_NS);

#[inline(always)]
pub fn record_unmapped_input_clock_warning(song_time_ns: SongTimeNs) -> bool {
    let last = LAST_UNMAPPED_INPUT_CLOCK_WARN_NS.load(Ordering::Relaxed);
    let should_warn = should_warn_unmapped_input_clock(last, song_time_ns);
    if should_warn {
        LAST_UNMAPPED_INPUT_CLOCK_WARN_NS.store(song_time_ns, Ordering::Relaxed);
    }
    should_warn
}

