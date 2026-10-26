//! Frozen production module from 0f7ded20c, excluding its test module.
#[cfg(test)]
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::InputSource;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DebounceState {
    action_mask: u32,
    source: InputSource,
    held_raw: bool,
    held_reported: bool,
    last_raw_change_time: Instant,
    last_raw_change_host_nanos: u64,
    last_raw_store_time: Instant,
    last_report_time: Instant,
}

const NOT_SCHEDULED: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SlotState {
    state: Option<DebounceState>,
    due_at: Option<Instant>,
    scheduled_ix: u32,
    epoch: u32,
}

impl Default for SlotState {
    fn default() -> Self {
        Self {
            state: None,
            due_at: None,
            scheduled_ix: NOT_SCHEDULED,
            epoch: 0,
        }
    }
}

#[derive(Debug, Default)]
pub struct DebounceStore {
    slots: Vec<SlotState>,
    // Indexed min-heap: each slot occurs at most once, so input chatter updates
    // bounded storage instead of appending stale records until the next drain.
    due_slots: Vec<usize>,
    active_len: usize,
    epoch: u32,
}

impl DebounceStore {
    #[inline(always)]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            slots: Vec::new(),
            due_slots: Vec::new(),
            active_len: 0,
            epoch: 0,
        }
    }

    #[inline(always)]
    pub fn clear_and_reserve(&mut self, cap: usize) {
        self.due_slots.clear();
        self.active_len = 0;
        self.epoch = self.epoch.wrapping_add(1);
        if self.epoch == 0 {
            self.slots.fill(SlotState::default());
            self.epoch = 1;
        }
        if self.slots.capacity() < cap {
            self.slots.reserve(cap.saturating_sub(self.slots.len()));
        }
        self.due_slots.reserve(cap);
    }

    #[inline(always)]
    pub fn prepare_slots(&mut self, len: usize) {
        self.clear_and_reserve(len);
        if self.slots.len() < len {
            self.slots.resize(len, SlotState::default());
        } else {
            self.slots.truncate(len);
        }
    }

    pub(crate) const fn has_scheduled_work(&self) -> bool {
        !self.due_slots.is_empty()
    }

    pub(crate) fn is_down(&self, slot: usize) -> bool {
        self.slots
            .get(slot)
            .filter(|slot| slot.epoch == self.epoch)
            .and_then(|slot| slot.state)
            .is_some_and(|state| state.held_reported)
    }

    #[inline(always)]
    fn ensure_slot(&mut self, slot: usize) {
        if slot >= self.slots.len() {
            self.slots.resize(slot + 1, SlotState::default());
        }
        if self.slots[slot].epoch != self.epoch {
            self.slots[slot] = SlotState {
                epoch: self.epoch,
                ..SlotState::default()
            };
        }
    }

    #[inline(always)]
    fn refresh_due_slot(
        &mut self,
        slot: usize,
        old_due_at: Option<Instant>,
        new_due_at: Option<Instant>,
    ) {
        if old_due_at == new_due_at {
            return;
        }
        match (old_due_at, new_due_at) {
            (None, Some(due_at)) => {
                let scheduled_ix = self.due_slots.len();
                self.slots[slot].due_at = Some(due_at);
                self.slots[slot].scheduled_ix = scheduled_ix.min(u32::MAX as usize) as u32;
                self.due_slots.push(slot);
                self.sift_up(scheduled_ix);
            }
            (Some(_), None) => self.unschedule_slot(slot),
            (Some(old_due_at), Some(due_at)) => {
                self.slots[slot].due_at = Some(due_at);
                let ix = self.slots[slot].scheduled_ix as usize;
                // Only one heap direction can be disturbed by a deadline change.
                if due_at < old_due_at {
                    self.sift_up(ix);
                } else {
                    self.sift_down(ix);
                }
            }
            (None, None) => {}
        }
    }

    #[inline(always)]
    fn unschedule_slot(&mut self, slot: usize) {
        let scheduled_ix = self.slots[slot].scheduled_ix as usize;
        debug_assert!(scheduled_ix < self.due_slots.len());
        self.due_slots.swap_remove(scheduled_ix);
        self.slots[slot].due_at = None;
        self.slots[slot].scheduled_ix = NOT_SCHEDULED;
        if scheduled_ix < self.due_slots.len() {
            let moved_slot = self.due_slots[scheduled_ix];
            self.slots[moved_slot].scheduled_ix = scheduled_ix as u32;
            self.repair_due_slot(moved_slot);
        }
    }

    #[inline(always)]
    fn due_slot_lt(&self, lhs: usize, rhs: usize) -> bool {
        let lhs_due = self.slots[lhs]
            .due_at
            .expect("scheduled debounce slot must have a deadline");
        let rhs_due = self.slots[rhs]
            .due_at
            .expect("scheduled debounce slot must have a deadline");
        (lhs_due, lhs) < (rhs_due, rhs)
    }

    #[inline(always)]
    fn swap_due_slots(&mut self, lhs: usize, rhs: usize) {
        self.due_slots.swap(lhs, rhs);
        self.slots[self.due_slots[lhs]].scheduled_ix = lhs as u32;
        self.slots[self.due_slots[rhs]].scheduled_ix = rhs as u32;
    }

    #[inline(always)]
    fn sift_up(&mut self, mut ix: usize) -> usize {
        while ix != 0 {
            let parent = (ix - 1) / 2;
            if !self.due_slot_lt(self.due_slots[ix], self.due_slots[parent]) {
                break;
            }
            self.swap_due_slots(ix, parent);
            ix = parent;
        }
        ix
    }

    #[inline(always)]
    fn sift_down(&mut self, mut ix: usize) {
        loop {
            let left = ix.saturating_mul(2).saturating_add(1);
            if left >= self.due_slots.len() {
                return;
            }
            let right = left + 1;
            let child = if right < self.due_slots.len()
                && self.due_slot_lt(self.due_slots[right], self.due_slots[left])
            {
                right
            } else {
                left
            };
            if !self.due_slot_lt(self.due_slots[child], self.due_slots[ix]) {
                return;
            }
            self.swap_due_slots(ix, child);
            ix = child;
        }
    }

    #[inline(always)]
    fn repair_due_slot(&mut self, slot: usize) {
        let ix = self.slots[slot].scheduled_ix as usize;
        // Moving upward already establishes order with both children.
        if self.sift_up(ix) == ix {
            self.sift_down(ix);
        }
    }

    #[inline(always)]
    fn next_due_slot(&self, now: Instant) -> Option<(usize, Instant)> {
        let &slot = self.due_slots.first()?;
        let due_at = self.slots[slot]
            .due_at
            .expect("scheduled debounce slot must have a deadline");
        if due_at > now {
            return None;
        }
        Some((slot, due_at))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DebouncedEdge {
    pub action_mask: u32,
    pub input_slot: u32,
    pub pressed: bool,
    pub source: InputSource,
    pub timestamp: Instant,
    pub timestamp_host_nanos: u64,
    pub stored_at: Instant,
    pub emitted_at: Instant,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DebounceEdges {
    pub first: Option<DebouncedEdge>,
    pub second: Option<DebouncedEdge>,
}

#[derive(Clone, Copy, Debug)]
pub struct DebounceWindows {
    window: Duration,
}

impl DebounceWindows {
    #[inline(always)]
    #[must_use]
    pub const fn uniform(window: Duration) -> Self {
        // ITGmania InputFilter parity: one global debounce window gates both
        // press and release edges for every input binding.
        Self { window }
    }

    #[inline(always)]
    const fn prune_window(self) -> Duration {
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
const fn debounced_edge(
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
fn debounce_emit_if_due(
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
fn debounce_step(
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
    let second = if state.held_raw != pressed {
        state.action_mask = action_mask;
        state.source = source;
        state.held_raw = pressed;
        state.last_raw_change_time = timestamp;
        state.last_raw_change_host_nanos = timestamp_host_nanos;
        state.last_raw_store_time = now;
        debounce_emit_if_due(state, input_slot, now, windows)
    } else {
        // The first check already handled this unchanged raw state.
        None
    };
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
    if state.held_raw && state.held_reported {
        return None;
    }
    // Pending edges and fully released slots share the same deadline. Retaining
    // released state for one window keeps a rapid repress gated by its release.
    state.last_report_time.checked_add(windows.window)
}

#[inline(always)]
fn resolve_due_slot(
    slot_state: &mut SlotState,
    input_slot: u32,
    now: Instant,
    windows: DebounceWindows,
) -> (Option<DebouncedEdge>, bool, Option<Instant>) {
    let Some(mut state) = slot_state.state else {
        return (None, false, None);
    };
    if state.held_raw && state.held_reported {
        return (None, false, None);
    }
    if now.duration_since(state.last_report_time) < windows.window {
        return (
            None,
            false,
            state.last_report_time.checked_add(windows.window),
        );
    }
    if !state.held_raw && !state.held_reported {
        slot_state.state = None;
        return (None, true, None);
    }

    state.last_report_time = now;
    state.held_reported = state.held_raw;
    let edge = debounced_edge(state, input_slot, state.held_reported, now);
    if !state.held_raw && windows.window.is_zero() {
        slot_state.state = None;
        return (Some(edge), true, None);
    }
    let new_due_at = if state.held_raw {
        None
    } else {
        now.checked_add(windows.prune_window())
    };
    slot_state.state = Some(state);
    (Some(edge), false, new_due_at)
}

#[cfg(test)]
fn debounce_input_edge_in_store(
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
        || timestamp,
    )
}

/// Debounces a physical edge, reading receipt time only when its state can change.
///
/// `now` is called at most once; raw timestamps are preserved independently.
///
/// # Panics
///
/// Panics if an internal state invariant is violated.
pub fn debounce_input_edge_in_store_mut(
    states: &mut DebounceStore,
    slot: usize,
    action_mask: u32,
    source: InputSource,
    pressed: bool,
    timestamp: Instant,
    timestamp_host_nanos: u64,
    windows: DebounceWindows,
    now: impl FnOnce() -> Instant,
) -> DebounceEdges {
    states.ensure_slot(slot);
    // Native backends can repeat a held value (IOHID deliberately does). Once
    // raw and reported state agree, another identical value cannot emit,
    // reschedule, or update timestamps.
    if states.slots[slot]
        .state
        .is_some_and(|state| state.held_raw == pressed && state.held_reported == pressed)
    {
        return DebounceEdges::default();
    }
    let debug_log = log::log_enabled!(log::Level::Debug);
    if states.slots[slot].state.is_none() && !pressed && !debug_log {
        return DebounceEdges::default();
    }
    let now = now();
    if states.slots[slot].state.is_some_and(|state| {
        state.held_raw == pressed
            && state.held_reported != pressed
            && now.duration_since(state.last_report_time) < windows.window
    }) {
        return DebounceEdges::default();
    }
    let input_slot = slot.min(u32::MAX as usize) as u32;
    let was_empty = states.slots[slot].state.is_none();
    let old_due_at = states.slots[slot].due_at;
    let before_state = if debug_log {
        states.slots[slot].state
    } else {
        None
    };

    let (edges, prune, new_due_at) = {
        let slot_state = &mut states.slots[slot];
        let mut state = slot_state.state.unwrap_or_else(|| DebounceState {
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
fn emit_due_debounce_edges_from(
    states: &Mutex<DebounceStore>,
    now: Instant,
    windows: DebounceWindows,
    mut emit: impl FnMut(DebouncedEdge),
) -> bool {
    let mut states = states.lock().unwrap();
    let mut flushed = false;
    while let Some(edge) = next_due_edge(&mut states, now, windows) {
        flushed = true;
        emit(edge);
    }
    flushed
}

/// Removes one due edge, preserving its physical timestamp.
///
/// Maintenance of expired slots precedes the next edge and uses the per-slot
/// due heap; no live-slot scan is required.
pub(crate) fn next_due_edge(
    states: &mut DebounceStore,
    now: Instant,
    windows: DebounceWindows,
) -> Option<DebouncedEdge> {
    while let Some((next_slot, old_due_at)) = states.next_due_slot(now) {
        let (edge, remove, new_due_at, after_state) = {
            let slot_state = &mut states.slots[next_slot];
            let input_slot = next_slot.min(u32::MAX as usize) as u32;
            let (edge, remove, new_due_at) = resolve_due_slot(slot_state, input_slot, now, windows);
            (edge, remove, new_due_at, slot_state.state)
        };

        if remove {
            states.active_len = states.active_len.saturating_sub(1);
        }
        // A delayed release keeps its slot for cleanup. Update its deadline
        // directly instead of removing and reinserting the same heap entry.
        states.refresh_due_slot(next_slot, Some(old_due_at), new_due_at);
        if let Some(edge) = edge
            && log::log_enabled!(log::Level::Debug)
        {
            log_debounce_due(
                next_slot,
                edge,
                after_state,
                states.slots[next_slot].due_at,
                states.active_len,
                now,
            );
        }
        if edge.is_some() {
            return edge;
        }
    }
    None
}
