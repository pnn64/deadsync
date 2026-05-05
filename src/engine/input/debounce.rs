use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;
#[cfg(test)]
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::InputSource;

#[derive(Clone, Copy, Debug)]
pub(super) struct DebounceState {
    pub(super) action_mask: u32,
    pub(super) source: InputSource,
    pub(super) held_raw: bool,
    pub(super) held_reported: bool,
    pub(super) last_raw_change_time: Instant,
    pub(super) last_raw_change_host_nanos: u64,
    pub(super) last_raw_store_time: Instant,
    pub(super) last_report_time: Instant,
}

#[derive(Clone, Copy, Debug, Default)]
struct SlotState {
    state: Option<DebounceState>,
    due_at: Option<Instant>,
    generation: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DueSlot {
    due_at: Instant,
    slot: usize,
    generation: u32,
}

impl Ord for DueSlot {
    fn cmp(&self, other: &Self) -> Ordering {
        self.due_at
            .cmp(&other.due_at)
            .then_with(|| self.slot.cmp(&other.slot))
            .then_with(|| self.generation.cmp(&other.generation))
    }
}

impl PartialOrd for DueSlot {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Default)]
pub(super) struct DebounceStore {
    slots: Vec<SlotState>,
    due_slots: BinaryHeap<Reverse<DueSlot>>,
    active_len: usize,
}

impl DebounceStore {
    #[inline(always)]
    pub(super) fn new() -> Self {
        Self {
            slots: Vec::new(),
            due_slots: BinaryHeap::new(),
            active_len: 0,
        }
    }

    #[inline(always)]
    pub(super) fn clear_and_reserve(&mut self, cap: usize) {
        self.slots.clear();
        self.due_slots.clear();
        self.active_len = 0;
        self.slots.reserve(cap);
        let due_cap = cap.saturating_mul(2);
        self.due_slots.reserve(due_cap);
    }

    #[inline(always)]
    pub(super) fn prepare_slots(&mut self, len: usize) {
        self.clear_and_reserve(len);
        if len != 0 {
            self.slots.resize(len, SlotState::default());
        }
    }

    #[inline(always)]
    fn ensure_slot(&mut self, slot: usize) {
        if slot >= self.slots.len() {
            self.slots.resize(slot + 1, SlotState::default());
        }
    }

    #[inline(always)]
    fn refresh_due_slot(
        &mut self,
        slot: usize,
        old_due_at: Option<Instant>,
        new_due_at: Option<Instant>,
    ) {
        let slot_state = &mut self.slots[slot];
        if old_due_at == new_due_at {
            slot_state.due_at = new_due_at;
            return;
        }
        slot_state.generation = slot_state.generation.wrapping_add(1);
        slot_state.due_at = new_due_at;
        if let Some(due_at) = new_due_at {
            self.due_slots.push(Reverse(DueSlot {
                due_at,
                slot,
                generation: slot_state.generation,
            }));
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DebouncedEdge {
    pub(super) action_mask: u32,
    pub(super) input_slot: u32,
    pub(super) pressed: bool,
    pub(super) source: InputSource,
    pub(super) timestamp: Instant,
    pub(super) timestamp_host_nanos: u64,
    pub(super) stored_at: Instant,
    pub(super) emitted_at: Instant,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct DebounceEdges {
    pub(super) first: Option<DebouncedEdge>,
    pub(super) second: Option<DebouncedEdge>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct DebounceWindows {
    pub(super) window: Duration,
}

impl DebounceWindows {
    #[inline(always)]
    pub(super) const fn uniform(window: Duration) -> Self {
        // ITGmania InputFilter parity: one global debounce window gates both
        // press and release edges for every input binding.
        Self { window }
    }

    #[inline(always)]
    pub(super) fn prune_window(self) -> Duration {
        self.window
    }
}

#[inline(always)]
fn instant_delta_us(target: Instant, now: Instant) -> i128 {
    if target >= now {
        target.duration_since(now).as_micros() as i128
    } else {
        -(now.duration_since(target).as_micros() as i128)
    }
}

#[inline(always)]
fn due_delta_us(due_at: Option<Instant>, now: Instant) -> Option<i128> {
    due_at.map(|due_at| instant_delta_us(due_at, now))
}

#[inline(always)]
fn debounced_edge(
    state: DebounceState,
    input_slot: u32,
    pressed: bool,
    emitted_at: Instant,
) -> DebouncedEdge {
    DebouncedEdge {
        action_mask: state.action_mask,
        input_slot,
        pressed,
        source: state.source,
        timestamp: state.last_raw_change_time,
        timestamp_host_nanos: state.last_raw_change_host_nanos,
        stored_at: state.last_raw_store_time,
        emitted_at,
    }
}

#[inline(always)]
pub(super) fn debounce_emit_if_due(
    state: &mut DebounceState,
    input_slot: u32,
    now: Instant,
    windows: DebounceWindows,
) -> Option<DebouncedEdge> {
    // ITGmania parity: the debounce gate compares against the last reported edge,
    // not just the last raw edge, so chatter inside the window is suppressed.
    if state.held_raw == state.held_reported {
        return None;
    }
    if now.duration_since(state.last_report_time) < windows.window {
        return None;
    }
    state.last_report_time = now;
    state.held_reported = state.held_raw;
    Some(debounced_edge(*state, input_slot, state.held_reported, now))
}

#[inline(always)]
pub(super) fn debounce_step(
    state: &mut DebounceState,
    action_mask: u32,
    source: InputSource,
    pressed: bool,
    timestamp: Instant,
    timestamp_host_nanos: u64,
    now: Instant,
    input_slot: u32,
    windows: DebounceWindows,
) -> DebounceEdges {
    // ITGmania InputFilter parity: flush any now-due delayed edge before storing
    // the new raw state, so a delayed release can still report just ahead of a
    // later repress instead of being silently lost.
    let first = debounce_emit_if_due(state, input_slot, now, windows);
    if state.held_raw != pressed {
        state.action_mask = action_mask;
        state.source = source;
        state.held_raw = pressed;
        state.last_raw_change_time = timestamp;
        state.last_raw_change_host_nanos = timestamp_host_nanos;
        state.last_raw_store_time = now;
    }
    let second = debounce_emit_if_due(state, input_slot, now, windows);
    DebounceEdges { first, second }
}

#[cold]
fn log_debounce_store(
    slot: usize,
    action_mask: u32,
    source: InputSource,
    pressed: bool,
    before_state: Option<DebounceState>,
    after_state: Option<DebounceState>,
    edges: DebounceEdges,
    due_at: Option<Instant>,
    active_len: usize,
    now: Instant,
) {
    log::debug!(
        concat!(
            "INPUT DEBOUNCE EDGE: slot={} source={:?} action_mask={:#010x} raw_pressed={} ",
            "before_held_raw={:?} before_held_reported={:?} ",
            "after_held_raw={:?} after_held_reported={:?} ",
            "emitted_first={} emitted_second={} first_pressed={:?} second_pressed={:?} ",
            "due_us={:?} active_len={}"
        ),
        slot,
        source,
        action_mask,
        pressed,
        before_state.map(|state| state.held_raw),
        before_state.map(|state| state.held_reported),
        after_state.map(|state| state.held_raw),
        after_state.map(|state| state.held_reported),
        edges.first.is_some(),
        edges.second.is_some(),
        edges.first.map(|edge| edge.pressed),
        edges.second.map(|edge| edge.pressed),
        due_delta_us(due_at, now),
        active_len,
    );
}

#[cold]
fn log_debounce_due(
    slot: usize,
    edge: DebouncedEdge,
    after_state: Option<DebounceState>,
    due_at: Option<Instant>,
    active_len: usize,
    now: Instant,
) {
    log::debug!(
        concat!(
            "INPUT DEBOUNCE DUE: slot={} source={:?} action_mask={:#010x} pressed={} ",
            "after_held_raw={:?} after_held_reported={:?} next_due_us={:?} active_len={}"
        ),
        slot,
        edge.source,
        edge.action_mask,
        edge.pressed,
        after_state.map(|state| state.held_raw),
        after_state.map(|state| state.held_reported),
        due_delta_us(due_at, now),
        active_len,
    );
}

#[inline(always)]
fn should_prune_debounce_state(
    state: DebounceState,
    now: Instant,
    windows: DebounceWindows,
) -> bool {
    !state.held_raw
        && !state.held_reported
        && now.duration_since(state.last_report_time) >= windows.prune_window()
}

#[inline(always)]
fn debounce_due_at(state: DebounceState, windows: DebounceWindows) -> Option<Instant> {
    if state.held_raw != state.held_reported {
        return state.last_report_time.checked_add(windows.window);
    }
    // Keep a fully released slot around for one more window so a rapid repress
    // is still compared against the last reported release before we drop state.
    if !state.held_raw && !state.held_reported {
        return state.last_report_time.checked_add(windows.prune_window());
    }
    None
}

#[cfg(test)]
pub(super) fn debounce_input_edge_in_store(
    states: &Mutex<DebounceStore>,
    slot: usize,
    action_mask: u32,
    source: InputSource,
    pressed: bool,
    timestamp: Instant,
    timestamp_host_nanos: u64,
    windows: DebounceWindows,
) -> DebounceEdges {
    let mut states = states.lock().unwrap();
    debounce_input_edge_in_store_mut(
        &mut states,
        slot,
        action_mask,
        source,
        pressed,
        timestamp,
        timestamp_host_nanos,
        windows,
    )
}

pub(super) fn debounce_input_edge_in_store_mut(
    states: &mut DebounceStore,
    slot: usize,
    action_mask: u32,
    source: InputSource,
    pressed: bool,
    timestamp: Instant,
    timestamp_host_nanos: u64,
    windows: DebounceWindows,
) -> DebounceEdges {
    let now = Instant::now();
    states.ensure_slot(slot);
    let input_slot = slot.min(u32::MAX as usize) as u32;
    let debug_log = log::log_enabled!(log::Level::Debug);
    let was_empty = states.slots[slot].state.is_none();
    let old_due_at = states.slots[slot].due_at;
    let before_state = if debug_log {
        states.slots[slot].state
    } else {
        None
    };

    let (edges, prune, new_due_at) = {
        let slot_state = &mut states.slots[slot];
        let mut state = slot_state.state.unwrap_or(DebounceState {
            action_mask,
            source,
            held_raw: false,
            held_reported: false,
            last_raw_change_time: timestamp,
            last_raw_change_host_nanos: timestamp_host_nanos,
            last_raw_store_time: now,
            last_report_time: now.checked_sub(windows.prune_window()).unwrap_or(now),
        });
        let edges = debounce_step(
            &mut state,
            action_mask,
            source,
            pressed,
            timestamp,
            timestamp_host_nanos,
            now,
            input_slot,
            windows,
        );
        let prune = should_prune_debounce_state(state, now, windows);
        let new_due_at = if prune {
            slot_state.state = None;
            None
        } else {
            slot_state.state = Some(state);
            debounce_due_at(state, windows)
        };
        (edges, prune, new_due_at)
    };

    if was_empty {
        if !prune {
            states.active_len += 1;
        }
    } else if prune {
        states.active_len = states.active_len.saturating_sub(1);
    }
    states.refresh_due_slot(slot, old_due_at, new_due_at);
    if debug_log {
        log_debounce_store(
            slot,
            action_mask,
            source,
            pressed,
            before_state,
            states.slots[slot].state,
            edges,
            states.slots[slot].due_at,
            states.active_len,
            now,
        );
    }
    edges
}

#[cfg(test)]
pub(super) fn emit_due_debounce_edges_from(
    states: &Mutex<DebounceStore>,
    now: Instant,
    windows: DebounceWindows,
    mut emit: impl FnMut(DebouncedEdge),
) -> bool {
    let mut states = states.lock().unwrap();
    emit_due_debounce_edges_from_mut(&mut states, now, windows, &mut emit)
}

pub(super) fn emit_due_debounce_edges_from_mut(
    states: &mut DebounceStore,
    now: Instant,
    windows: DebounceWindows,
    mut emit: impl FnMut(DebouncedEdge),
) -> bool {
    // ITGmania Update() parity: delayed edges are surfaced later, but they still
    // carry the original raw timestamp that caused the debounce holdoff.
    let mut flushed = false;

    while let Some(Reverse(next)) = states.due_slots.peek().copied() {
        if next.due_at > now {
            break;
        }
        states.due_slots.pop();
        if next.slot >= states.slots.len() {
            continue;
        }

        let (edge, remove, old_due_at, new_due_at, after_state) = {
            let slot_state = &mut states.slots[next.slot];
            if slot_state.generation != next.generation || slot_state.due_at != Some(next.due_at) {
                continue;
            }
            let Some(mut state) = slot_state.state else {
                slot_state.due_at = None;
                continue;
            };
            let old_due_at = slot_state.due_at;
            let input_slot = next.slot.min(u32::MAX as usize) as u32;
            let edge = debounce_emit_if_due(&mut state, input_slot, now, windows);
            let remove = should_prune_debounce_state(state, now, windows);
            let new_due_at = if remove {
                slot_state.state = None;
                None
            } else {
                slot_state.state = Some(state);
                debounce_due_at(state, windows)
            };
            (edge, remove, old_due_at, new_due_at, slot_state.state)
        };

        if let Some(edge) = edge {
            flushed = true;
            emit(edge);
        }
        if remove {
            states.active_len = states.active_len.saturating_sub(1);
        }
        states.refresh_due_slot(next.slot, old_due_at, new_due_at);
        if let Some(edge) = edge
            && log::log_enabled!(log::Level::Debug)
        {
            log_debounce_due(
                next.slot,
                edge,
                after_state,
                states.slots[next.slot].due_at,
                states.active_len,
                now,
            );
        }
    }
    flushed
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_MASK: u32 = 1 << 3;
    const TEST_SLOT: u32 = 7;

    fn base_state(now: Instant, window: Duration) -> DebounceState {
        DebounceState {
            action_mask: TEST_MASK,
            source: InputSource::Keyboard,
            held_raw: false,
            held_reported: false,
            last_raw_change_time: now,
            last_raw_change_host_nanos: 0,
            last_raw_store_time: now,
            last_report_time: now.checked_sub(window).unwrap_or(now),
        }
    }

    fn assert_edge(
        edge: Option<DebouncedEdge>,
        action_mask: u32,
        source: InputSource,
        pressed: bool,
        timestamp: Instant,
        timestamp_host_nanos: u64,
        stored_at: Instant,
        emitted_at: Instant,
    ) {
        let edge = edge.expect("expected debounced edge");
        assert_eq!(edge.action_mask, action_mask);
        assert_eq!(edge.input_slot, TEST_SLOT);
        assert_eq!(edge.source, source);
        assert_eq!(edge.pressed, pressed);
        assert_eq!(edge.timestamp, timestamp);
        assert_eq!(edge.timestamp_host_nanos, timestamp_host_nanos);
        assert_eq!(edge.stored_at, stored_at);
        assert_eq!(edge.emitted_at, emitted_at);
    }

    #[test]
    fn clear_and_reserve_presizes_due_queue_with_stale_slack() {
        let mut store = DebounceStore::new();
        store.clear_and_reserve(8);
        assert!(store.slots.capacity() >= 8);
        assert!(store.due_slots.capacity() >= 16);

        store.clear_and_reserve(16);
        assert!(store.slots.capacity() >= 16);
        assert!(store.due_slots.capacity() >= 32);
    }

    #[test]
    fn debounce_keeps_short_tap_and_delays_release() {
        let window = Duration::from_millis(20);
        let windows = DebounceWindows::uniform(window);
        let t0 = Instant::now();
        let t0_host = 100;
        let mut state = base_state(t0, window);

        let press = debounce_step(
            &mut state,
            TEST_MASK,
            InputSource::Keyboard,
            true,
            t0,
            t0_host,
            t0,
            TEST_SLOT,
            windows,
        );
        assert!(press.first.is_none());
        assert_edge(
            press.second,
            TEST_MASK,
            InputSource::Keyboard,
            true,
            t0,
            t0_host,
            t0,
            t0,
        );

        let release_ts = t0 + Duration::from_millis(1);
        let release_host = 101;
        let release = debounce_step(
            &mut state,
            TEST_MASK,
            InputSource::Keyboard,
            false,
            release_ts,
            release_host,
            release_ts,
            TEST_SLOT,
            windows,
        );
        assert!(release.first.is_none());
        assert!(release.second.is_none());

        assert_edge(
            debounce_emit_if_due(
                &mut state,
                TEST_SLOT,
                t0 + Duration::from_millis(21),
                windows,
            ),
            TEST_MASK,
            InputSource::Keyboard,
            false,
            release_ts,
            release_host,
            release_ts,
            t0 + Duration::from_millis(21),
        );
    }

    #[test]
    fn debounce_cancels_quick_release_repress_chatter() {
        let window = Duration::from_millis(20);
        let windows = DebounceWindows::uniform(window);
        let t0 = Instant::now();
        let mut state = base_state(t0, window);

        let press = debounce_step(
            &mut state,
            TEST_MASK,
            InputSource::Keyboard,
            true,
            t0,
            100,
            t0,
            TEST_SLOT,
            windows,
        );
        assert!(press.first.is_none());
        assert!(press.second.is_some());

        let release_ts = t0 + Duration::from_millis(1);
        let release = debounce_step(
            &mut state,
            TEST_MASK,
            InputSource::Keyboard,
            false,
            release_ts,
            101,
            release_ts,
            TEST_SLOT,
            windows,
        );
        assert!(release.first.is_none());
        assert!(release.second.is_none());

        let repress_ts = t0 + Duration::from_millis(5);
        let repress = debounce_step(
            &mut state,
            TEST_MASK,
            InputSource::Keyboard,
            true,
            repress_ts,
            105,
            repress_ts,
            TEST_SLOT,
            windows,
        );
        assert!(repress.first.is_none());
        assert!(repress.second.is_none());

        assert_eq!(
            debounce_emit_if_due(
                &mut state,
                TEST_SLOT,
                t0 + Duration::from_millis(25),
                windows,
            ),
            None
        );
    }

    #[test]
    fn debounce_flushes_due_release_before_new_press() {
        let window = Duration::from_millis(20);
        let windows = DebounceWindows::uniform(window);
        let t0 = Instant::now();
        let mut state = base_state(t0, window);

        let press = debounce_step(
            &mut state,
            TEST_MASK,
            InputSource::Keyboard,
            true,
            t0,
            100,
            t0,
            TEST_SLOT,
            windows,
        );
        assert!(press.first.is_none());
        assert!(press.second.is_some());

        let release_ts = t0 + Duration::from_millis(1);
        let release = debounce_step(
            &mut state,
            TEST_MASK,
            InputSource::Keyboard,
            false,
            release_ts,
            101,
            release_ts,
            TEST_SLOT,
            windows,
        );
        assert!(release.first.is_none());
        assert!(release.second.is_none());

        let repress_ts = t0 + Duration::from_millis(30);
        let repress = debounce_step(
            &mut state,
            TEST_MASK,
            InputSource::Keyboard,
            true,
            repress_ts,
            130,
            repress_ts,
            TEST_SLOT,
            windows,
        );
        assert_edge(
            repress.first,
            TEST_MASK,
            InputSource::Keyboard,
            false,
            release_ts,
            101,
            release_ts,
            repress_ts,
        );
        assert!(repress.second.is_none());

        assert_edge(
            debounce_emit_if_due(
                &mut state,
                TEST_SLOT,
                t0 + Duration::from_millis(50),
                windows,
            ),
            TEST_MASK,
            InputSource::Keyboard,
            true,
            repress_ts,
            130,
            repress_ts,
            t0 + Duration::from_millis(50),
        );
    }

    #[test]
    fn due_queue_emits_slots_in_due_order() {
        let window = Duration::from_millis(20);
        let windows = DebounceWindows::uniform(window);
        let states = Mutex::new(DebounceStore::new());
        let t0 = Instant::now();

        let press0 = debounce_input_edge_in_store(
            &states,
            0,
            TEST_MASK,
            InputSource::Keyboard,
            true,
            t0,
            100,
            windows,
        );
        assert!(press0.first.is_none());
        assert!(press0.second.is_some());

        let release0_ts = t0 + Duration::from_millis(1);
        let release0 = debounce_input_edge_in_store(
            &states,
            0,
            TEST_MASK,
            InputSource::Keyboard,
            false,
            release0_ts,
            101,
            windows,
        );
        assert!(release0.first.is_none());
        assert!(release0.second.is_none());

        let press1_ts = t0 + Duration::from_millis(5);
        let press1 = debounce_input_edge_in_store(
            &states,
            1,
            TEST_MASK << 1,
            InputSource::Gamepad,
            true,
            press1_ts,
            200,
            windows,
        );
        assert!(press1.first.is_none());
        assert!(press1.second.is_some());

        let release1_ts = t0 + Duration::from_millis(10);
        let release1 = debounce_input_edge_in_store(
            &states,
            1,
            TEST_MASK << 1,
            InputSource::Gamepad,
            false,
            release1_ts,
            201,
            windows,
        );
        assert!(release1.first.is_none());
        assert!(release1.second.is_none());

        let (due0, due1) = {
            let guard = states.lock().unwrap();
            (
                guard.slots[0].due_at.expect("slot 0 due time"),
                guard.slots[1].due_at.expect("slot 1 due time"),
            )
        };
        assert!(due0 <= due1, "earlier release should become due first");

        let mut emitted = Vec::new();
        assert!(emit_due_debounce_edges_from(
            &states,
            due0,
            windows,
            |edge| emitted.push(edge)
        ));
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].action_mask, TEST_MASK);
        assert!(!emitted[0].pressed);
        assert_eq!(states.lock().unwrap().active_len, 2);

        emitted.clear();
        assert!(emit_due_debounce_edges_from(
            &states,
            due1,
            windows,
            |edge| emitted.push(edge)
        ));
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].action_mask, TEST_MASK << 1);
        assert!(!emitted[0].pressed);
        assert_eq!(states.lock().unwrap().active_len, 2);

        emitted.clear();
        assert!(!emit_due_debounce_edges_from(
            &states,
            due0 + window,
            windows,
            |edge| emitted.push(edge)
        ));
        assert!(emitted.is_empty());
        assert_eq!(states.lock().unwrap().active_len, 1);

        emitted.clear();
        assert!(!emit_due_debounce_edges_from(
            &states,
            due1 + window,
            windows,
            |edge| emitted.push(edge)
        ));
        assert!(emitted.is_empty());
        assert_eq!(states.lock().unwrap().active_len, 0);
    }

    #[test]
    fn due_queue_ignores_stale_slots_after_chatter_cancel() {
        let window = Duration::from_millis(20);
        let windows = DebounceWindows::uniform(window);
        let states = Mutex::new(DebounceStore::new());
        let t0 = Instant::now();

        let press = debounce_input_edge_in_store(
            &states,
            3,
            TEST_MASK,
            InputSource::Keyboard,
            true,
            t0,
            100,
            windows,
        );
        assert!(press.second.is_some());

        let release_ts = t0 + Duration::from_millis(1);
        let release = debounce_input_edge_in_store(
            &states,
            3,
            TEST_MASK,
            InputSource::Keyboard,
            false,
            release_ts,
            101,
            windows,
        );
        assert!(release.first.is_none());
        assert!(release.second.is_none());
        let due_at = states.lock().unwrap().slots[3].due_at.expect("pending due");

        let repress_ts = t0 + Duration::from_millis(5);
        let repress = debounce_input_edge_in_store(
            &states,
            3,
            TEST_MASK,
            InputSource::Keyboard,
            true,
            repress_ts,
            105,
            windows,
        );
        assert!(repress.first.is_none());
        assert!(repress.second.is_none());
        assert!(states.lock().unwrap().slots[3].due_at.is_none());

        let mut emitted = Vec::new();
        assert!(!emit_due_debounce_edges_from(
            &states,
            due_at,
            windows,
            |edge| emitted.push(edge),
        ));
        assert!(emitted.is_empty());
    }
}
