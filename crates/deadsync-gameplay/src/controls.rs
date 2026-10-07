#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameplayTimingTickMode {
    #[default]
    Off,
    Assist,
    Hit,
}

#[inline(always)]
#[must_use]
pub const fn next_timing_tick_mode(mode: GameplayTimingTickMode) -> GameplayTimingTickMode {
    match mode {
        GameplayTimingTickMode::Off => GameplayTimingTickMode::Assist,
        GameplayTimingTickMode::Assist => GameplayTimingTickMode::Hit,
        GameplayTimingTickMode::Hit => GameplayTimingTickMode::Off,
    }
}

#[inline(always)]
#[must_use]
pub const fn timing_tick_mode_status_line(mode: GameplayTimingTickMode) -> Option<&'static str> {
    match mode {
        GameplayTimingTickMode::Off => None,
        GameplayTimingTickMode::Assist => Some("Assist Tick"),
        GameplayTimingTickMode::Hit => Some("Hit Tick"),
    }
}

#[inline(always)]
#[must_use]
pub const fn timing_tick_mode_debug_label(mode: GameplayTimingTickMode) -> &'static str {
    match mode {
        GameplayTimingTickMode::Off => "off",
        GameplayTimingTickMode::Assist => "assist tick",
        GameplayTimingTickMode::Hit => "hit tick",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameplayOffsetAdjustKey {
    Decrease,
    Increase,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameplayRawKeyInput {
    Restart,
    Autosync,
    TimingTick,
    Autoplay,
    OffsetAdjust(GameplayOffsetAdjustKey),
    #[default]
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameplayRawModifierKey {
    Shift,
    Ctrl,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameplayOffsetAdjustTarget {
    Global,
    Song,
    #[default]
    None,
}

#[inline(always)]
#[must_use]
pub const fn offset_adjust_target(
    shift_held: bool,
    course_active: bool,
) -> GameplayOffsetAdjustTarget {
    if shift_held {
        GameplayOffsetAdjustTarget::Global
    } else if course_active {
        GameplayOffsetAdjustTarget::None
    } else {
        GameplayOffsetAdjustTarget::Song
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameplayRawKeyPlan {
    Restart,
    /// Re-read the simfile from disk (refresh the chart cache) and restart.
    Reload,
    SetAutosyncMode(AutosyncMode),
    SetTimingTickMode(GameplayTimingTickMode),
    SetAutoplayEnabled(bool),
    StartOffsetAdjust {
        key: GameplayOffsetAdjustKey,
        target: GameplayOffsetAdjustTarget,
    },
    ClearOffsetAdjust(GameplayOffsetAdjustKey),
    #[default]
    None,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RawKeyAction {
    #[default]
    None,
    Restart,
    /// Refresh the chart cache from disk, then restart.
    Reload,
}

#[inline(always)]
#[must_use]
pub const fn offset_adjust_slot_for_key(key: GameplayOffsetAdjustKey) -> usize {
    match key {
        GameplayOffsetAdjustKey::Decrease => 0,
        GameplayOffsetAdjustKey::Increase => 1,
    }
}

#[inline(always)]
#[must_use]
pub const fn offset_adjust_delta_for_key(key: GameplayOffsetAdjustKey) -> f32 {
    match key {
        GameplayOffsetAdjustKey::Decrease => -OFFSET_ADJUST_STEP_SECONDS,
        GameplayOffsetAdjustKey::Increase => OFFSET_ADJUST_STEP_SECONDS,
    }
}

#[inline(always)]
#[must_use]
pub fn offset_adjust_repeat_ready(held_elapsed: Duration, last_elapsed: Duration) -> bool {
    held_elapsed >= OFFSET_ADJUST_REPEAT_DELAY && last_elapsed >= OFFSET_ADJUST_REPEAT_INTERVAL
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GameplayOffsetAdjustHoldState {
    held_since: [Option<Instant>; 2],
    last_at: [Option<Instant>; 2],
}

impl GameplayOffsetAdjustHoldState {
    #[inline(always)]
    pub fn is_active(&self) -> bool {
        self.held_since.iter().any(Option::is_some)
    }

    #[inline(always)]
    pub const fn start(&mut self, key: GameplayOffsetAdjustKey, at: Instant) -> f32 {
        start_offset_adjust_hold_state(&mut self.held_since, &mut self.last_at, key, at)
    }

    #[inline(always)]
    pub const fn clear(&mut self, key: GameplayOffsetAdjustKey) {
        clear_offset_adjust_hold_state(&mut self.held_since, &mut self.last_at, key);
    }

    #[inline(always)]
    pub fn tick(&mut self, key: GameplayOffsetAdjustKey, now: Instant) -> Option<f32> {
        tick_offset_adjust_hold_state(&self.held_since, &mut self.last_at, key, now)
    }

    #[inline(always)]
    #[must_use]
    pub const fn held_since_for_key(self, key: GameplayOffsetAdjustKey) -> Option<Instant> {
        self.held_since[offset_adjust_slot_for_key(key)]
    }

    #[inline(always)]
    #[must_use]
    pub const fn last_at_for_key(self, key: GameplayOffsetAdjustKey) -> Option<Instant> {
        self.last_at[offset_adjust_slot_for_key(key)]
    }
}

pub const fn start_offset_adjust_hold_state(
    held_since: &mut [Option<Instant>; 2],
    last_at: &mut [Option<Instant>; 2],
    key: GameplayOffsetAdjustKey,
    at: Instant,
) -> f32 {
    let slot = offset_adjust_slot_for_key(key);
    held_since[slot] = Some(at);
    last_at[slot] = Some(at);
    offset_adjust_delta_for_key(key)
}

pub const fn clear_offset_adjust_hold_state(
    held_since: &mut [Option<Instant>; 2],
    last_at: &mut [Option<Instant>; 2],
    key: GameplayOffsetAdjustKey,
) {
    let slot = offset_adjust_slot_for_key(key);
    held_since[slot] = None;
    last_at[slot] = None;
}

pub fn tick_offset_adjust_hold_state(
    held_since: &[Option<Instant>; 2],
    last_at: &mut [Option<Instant>; 2],
    key: GameplayOffsetAdjustKey,
    now: Instant,
) -> Option<f32> {
    let slot = offset_adjust_slot_for_key(key);
    let (Some(held_since), Some(previous_at)) = (held_since[slot], last_at[slot]) else {
        return None;
    };
    if !offset_adjust_repeat_ready(
        now.duration_since(held_since),
        now.duration_since(previous_at),
    ) {
        return None;
    }
    last_at[slot] = Some(now);
    Some(offset_adjust_delta_for_key(key))
}

#[inline(always)]
#[must_use]
pub fn offset_delta_target_seconds(old_offset: f32, delta: f32) -> Option<f32> {
    let new_offset = old_offset + delta;
    ((new_offset - old_offset).abs() >= OFFSET_DELTA_EPSILON_SECONDS).then_some(new_offset)
}

#[inline(always)]
fn mutate_timing_arc(timing: &mut Arc<TimingData>, mut apply: impl FnMut(&mut TimingData)) {
    #[inline(never)]
    fn mutate_shared(timing: &mut Arc<TimingData>, apply: &mut impl FnMut(&mut TimingData)) {
        apply(Arc::make_mut(timing));
    }

    if let Some(inner) = Arc::get_mut(timing) {
        apply(inner);
        return;
    }
    mutate_shared(timing, &mut apply);
}

#[cfg(test)]
mod timing_arc_tests {
    use super::*;

    #[test]
    fn timing_arc_updates_preserve_weak_refs_shared_snapshots_and_callback_count() {
        for ownership in 0..4 {
            let segments = TimingSegments {
                bpms: vec![(0.0, 120.0)],
                speeds: vec![deadsync_rules::timing::SpeedSegment {
                    beat: 0.0,
                    ratio: 0.5,
                    delay: 2.0,
                    unit: deadsync_rules::timing::SpeedUnit::Seconds,
                }],
                ..Default::default()
            };
            let mut timing = Arc::new(TimingData::from_segments(0.0, 0.0, &segments, &[0.0, 4.0]));
            let original_ptr = Arc::as_ptr(&timing);
            let shared = (ownership >= 2).then(|| Arc::clone(&timing));
            let weak = (ownership == 1 || ownership == 3).then(|| Arc::downgrade(&timing));
            let mut expected = (*timing).clone();
            expected.set_global_offset_seconds(0.125);
            let mut calls = 0;
            mutate_timing_arc(&mut timing, |value| {
                calls += 1;
                value.set_global_offset_seconds(0.125);
            });
            assert_eq!(calls, 1);
            assert_eq!(Arc::strong_count(&timing), 1);
            assert_eq!(Arc::weak_count(&timing), 0);
            assert_eq!(original_ptr == Arc::as_ptr(&timing), ownership == 0);
            if let Some(shared) = &shared {
                assert_eq!(shared.get_time_for_beat(0.0), 0.0);
                assert!(!Arc::ptr_eq(shared, &timing));
            }
            if let Some(weak) = weak {
                assert_eq!(weak.upgrade().is_some(), ownership == 3);
            }
            for beat in [-4.0, 0.0, 1.0, 4.0, f32::INFINITY, f32::NAN] {
                assert_eq!(
                    timing.get_time_for_beat_ns(beat),
                    expected.get_time_for_beat_ns(beat)
                );
                assert_eq!(
                    timing.get_speed_multiplier(beat, 0.5).to_bits(),
                    expected.get_speed_multiplier(beat, 0.5).to_bits()
                );
                assert_eq!(timing.get_beat_for_row(1), Some(4.0));
            }
        }
    }
}
