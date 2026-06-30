pub const EMPTY_ACTIVE_INPUT_SLOT: ActiveInputSlot = ActiveInputSlot {
    source: InputSource::Keyboard,
    input_slot: 0,
    lane_mask: 0,
};

#[inline(always)]
pub const fn remap_live_input_lane(
    play_style: GameplayInputPlayStyle,
    player_side: GameplayInputPlayerSide,
    lane: Lane,
) -> Option<Lane> {
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
pub const fn input_lane_bit(lane_idx: usize) -> u8 {
    1u8 << lane_idx
}

#[inline(always)]
pub const fn normalized_input_slot(input_slot: u32, fallback_slot: u32, invalid_slot: u32) -> u32 {
    if input_slot == invalid_slot {
        fallback_slot
    } else {
        input_slot
    }
}

#[inline(always)]
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

pub fn active_input_slot_lane_is_down(
    slots: &[ActiveInputSlot],
    slot_count: usize,
    lane_idx: usize,
    source: InputSource,
    input_slot: u32,
) -> bool {
    let bit = input_lane_bit(lane_idx);
    slots[..slot_count.min(slots.len())].iter().any(|slot| {
        slot.source == source && slot.input_slot == input_slot && slot.lane_mask & bit != 0
    })
}

#[inline(always)]
fn find_active_input_slot(
    slots: &[ActiveInputSlot],
    slot_count: usize,
    source: InputSource,
    input_slot: u32,
) -> Option<usize> {
    slots[..slot_count.min(slots.len())]
        .iter()
        .position(|slot| slot.source == source && slot.input_slot == input_slot)
}

#[inline(always)]
fn insert_active_input_slot(
    slots: &mut [ActiveInputSlot],
    slot_count: &mut usize,
    source: InputSource,
    input_slot: u32,
) -> Option<usize> {
    if let Some(idx) = find_active_input_slot(slots, *slot_count, source, input_slot) {
        return Some(idx);
    }
    if *slot_count >= slots.len() {
        return None;
    }
    let idx = *slot_count;
    slots[idx] = ActiveInputSlot {
        source,
        input_slot,
        lane_mask: 0,
    };
    *slot_count += 1;
    Some(idx)
}

#[inline(always)]
fn remove_active_input_slot_if_empty(
    slots: &mut [ActiveInputSlot],
    slot_count: &mut usize,
    idx: usize,
) {
    if idx >= *slot_count || slots[idx].lane_mask != 0 {
        return;
    }
    *slot_count = (*slot_count).saturating_sub(1);
    if idx < *slot_count {
        slots[idx] = slots[*slot_count];
    }
}

pub fn update_active_input_slot(
    slots: &mut [ActiveInputSlot],
    slot_count: &mut usize,
    lane_counts: &mut [u16],
    lane_idx: usize,
    source: InputSource,
    input_slot: u32,
    pressed: bool,
) -> LaneInputUpdate {
    if lane_idx >= lane_counts.len() || lane_idx >= MAX_COLS {
        return LaneInputUpdate::default();
    }
    *slot_count = (*slot_count).min(slots.len());
    let bit = input_lane_bit(lane_idx);
    let was_down = lane_counts[lane_idx] != 0;
    let mut slot_was_down = false;
    let mut slot_table_full = false;

    if pressed {
        if let Some(idx) = insert_active_input_slot(slots, slot_count, source, input_slot) {
            slot_was_down = slots[idx].lane_mask & bit != 0;
            if !slot_was_down {
                slots[idx].lane_mask |= bit;
                lane_counts[lane_idx] = lane_counts[lane_idx].saturating_add(1);
            }
        } else {
            slot_table_full = true;
        }
    } else if let Some(idx) = find_active_input_slot(slots, *slot_count, source, input_slot) {
        slot_was_down = slots[idx].lane_mask & bit != 0;
        if slot_was_down {
            slots[idx].lane_mask &= !bit;
            lane_counts[lane_idx] = lane_counts[lane_idx].saturating_sub(1);
            remove_active_input_slot_if_empty(slots, slot_count, idx);
        }
    }

    LaneInputUpdate {
        was_down,
        is_down: lane_counts[lane_idx] != 0,
        slot_was_down,
        slot_table_full,
    }
}

