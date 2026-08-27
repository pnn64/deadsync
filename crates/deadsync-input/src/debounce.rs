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

    #[inline(always)]
    pub(crate) const fn has_scheduled_work(&self) -> bool {
        !self.due_slots.is_empty()
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
            self.slots[slot].due_at = new_due_at;
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
            (Some(_), Some(due_at)) => {
                self.slots[slot].due_at = Some(due_at);
                self.repair_due_slot(slot);
            }
            (None, None) => self.slots[slot].due_at = None,
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
        let ix = self.sift_up(ix);
        self.sift_down(ix);
    }

    #[inline(always)]
    fn take_next_due_slot(&mut self, now: Instant) -> Option<usize> {
        let &slot = self.due_slots.first()?;
        let due_at = self.slots[slot]
            .due_at
            .expect("scheduled debounce slot must have a deadline");
        if due_at > now {
            return None;
        }
        self.unschedule_slot(slot);
        Some(slot)
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
    )
}

pub fn debounce_input_edge_in_store_mut(
    states: &mut DebounceStore,
    slot: usize,
    action_mask: u32,
    source: InputSource,
    pressed: bool,
    timestamp: Instant,
    timestamp_host_nanos: u64,
    windows: DebounceWindows,
) -> DebounceEdges {
    states.ensure_slot(slot);
    // Native backends can repeat a held value (IOHID deliberately does). Once
    // raw and reported state agree, another identical value cannot emit,
    // reschedule, or update timestamps, so avoid the platform clock entirely.
    if states.slots[slot]
        .state
        .is_some_and(|state| state.held_raw == pressed && state.held_reported == pressed)
    {
        return DebounceEdges::default();
    }
    let pending_now = states.slots[slot].state.and_then(|state| {
        (state.held_raw == pressed && state.held_reported != pressed).then(Instant::now)
    });
    if pending_now.is_some_and(|now| {
        let state = states.slots[slot].state.expect("pending state exists");
        now.duration_since(state.last_report_time) < windows.window
    }) {
        return DebounceEdges::default();
    }
    let input_slot = slot.min(u32::MAX as usize) as u32;
    let debug_log = log::log_enabled!(log::Level::Debug);
    if states.slots[slot].state.is_none() && !pressed && !debug_log {
        return DebounceEdges::default();
    }
    let now = pending_now.unwrap_or_else(Instant::now);
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
    emit_due_debounce_edges_from_mut(&mut states, now, windows, &mut emit)
}

pub fn emit_due_debounce_edges_from_mut(
    states: &mut DebounceStore,
    now: Instant,
    windows: DebounceWindows,
    mut emit: impl FnMut(DebouncedEdge),
) -> bool {
    // ITGmania Update() parity: delayed edges are surfaced later, but they still
    // carry the original raw timestamp that caused the debounce holdoff.
    let mut flushed = false;

    while let Some(next_slot) = states.take_next_due_slot(now) {
        let (edge, remove, new_due_at, after_state) = {
            let slot_state = &mut states.slots[next_slot];
            let input_slot = next_slot.min(u32::MAX as usize) as u32;
            let (edge, remove, new_due_at) = resolve_due_slot(slot_state, input_slot, now, windows);
            (edge, remove, new_due_at, slot_state.state)
        };

        if let Some(edge) = edge {
            flushed = true;
            emit(edge);
        }
        if remove {
            states.active_len = states.active_len.saturating_sub(1);
        }
        states.refresh_due_slot(next_slot, None, new_due_at);
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
    }
    flushed
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub mod bench_support {
    use super::*;

    const MASK: u32 = 1 << 3;

    #[inline(always)]
    fn edge_checksum(edge: Option<DebouncedEdge>) -> u64 {
        edge.map_or(0, |edge| {
            u64::from(edge.action_mask)
                ^ u64::from(edge.input_slot).rotate_left(7)
                ^ edge.timestamp_host_nanos.rotate_left(13)
                ^ (u64::from(edge.pressed) << 63)
        })
    }

    #[inline(always)]
    fn state_checksum(slot_state: SlotState) -> u64 {
        slot_state.state.map_or(0, |state| {
            u64::from(state.held_raw)
                | (u64::from(state.held_reported) << 1)
                | (u64::from(slot_state.due_at.is_some()) << 2)
        })
    }

    pub fn first_release_old(store: &mut DebounceStore, events: usize) -> u64 {
        let timestamp = Instant::now();
        let windows = DebounceWindows::uniform(Duration::from_millis(20));
        let mut checksum = 0u64;
        for index in 0..events {
            let slot = 0;
            store.ensure_slot(slot);
            let now = Instant::now();
            let input_slot = slot as u32;
            let old_due_at = store.slots[slot].due_at;
            let mut state = store.slots[slot].state.unwrap_or_else(|| DebounceState {
                action_mask: MASK,
                source: InputSource::Keyboard,
                held_raw: false,
                held_reported: false,
                last_raw_change_time: timestamp,
                last_raw_change_host_nanos: index as u64,
                last_raw_store_time: now,
                last_report_time: now.checked_sub(windows.prune_window()).unwrap_or(now),
            });
            let edges = debounce_step(
                &mut state,
                MASK,
                InputSource::Keyboard,
                false,
                timestamp,
                index as u64,
                now,
                input_slot,
                windows,
            );
            let prune = should_prune_debounce_state(state, now, windows);
            let new_due_at = if prune {
                store.slots[slot].state = None;
                None
            } else {
                store.slots[slot].state = Some(state);
                debounce_due_at(state, windows)
            };
            store.refresh_due_slot(slot, old_due_at, new_due_at);
            checksum = checksum
                .wrapping_add(edge_checksum(edges.first))
                .wrapping_add(edge_checksum(edges.second));
        }
        checksum
    }

    pub fn first_release_new(store: &mut DebounceStore, events: usize) -> u64 {
        let timestamp = Instant::now();
        let windows = DebounceWindows::uniform(Duration::from_millis(20));
        let mut checksum = 0u64;
        for index in 0..events {
            let edges = debounce_input_edge_in_store_mut(
                store,
                0,
                MASK,
                InputSource::Keyboard,
                false,
                timestamp,
                index as u64,
                windows,
            );
            checksum = checksum
                .wrapping_add(edge_checksum(edges.first))
                .wrapping_add(edge_checksum(edges.second));
        }
        checksum
    }

    fn pending_release(now: Instant, window: Duration) -> SlotState {
        let raw_at = now.checked_sub(window).unwrap_or(now);
        SlotState {
            state: Some(DebounceState {
                action_mask: MASK,
                source: InputSource::Gamepad,
                held_raw: false,
                held_reported: true,
                last_raw_change_time: raw_at,
                last_raw_change_host_nanos: 77,
                last_raw_store_time: raw_at,
                last_report_time: raw_at,
            }),
            due_at: Some(now),
            scheduled_ix: 0,
            epoch: 1,
        }
    }

    pub fn due_release_old(events: usize) -> u64 {
        let window = Duration::from_millis(20);
        let windows = DebounceWindows::uniform(window);
        let now = Instant::now();
        let template = pending_release(now, window);
        let mut checksum = 0u64;
        for _ in 0..events {
            let mut slot_state = template;
            let mut state = slot_state.state.expect("benchmark state initialized");
            let edge = debounce_emit_if_due(&mut state, 7, now, windows);
            let remove = should_prune_debounce_state(state, now, windows);
            let new_due_at = if remove {
                slot_state.state = None;
                None
            } else {
                slot_state.state = Some(state);
                debounce_due_at(state, windows)
            };
            checksum = checksum
                .wrapping_add(edge_checksum(edge))
                .wrapping_add(u64::from(remove))
                .wrapping_add(u64::from(new_due_at.is_some()))
                .wrapping_add(state_checksum(slot_state));
        }
        checksum
    }

    pub fn due_release_new(events: usize) -> u64 {
        let window = Duration::from_millis(20);
        let windows = DebounceWindows::uniform(window);
        let now = Instant::now();
        let template = pending_release(now, window);
        let mut checksum = 0u64;
        for _ in 0..events {
            let mut slot_state = template;
            let (edge, remove, new_due_at) = resolve_due_slot(&mut slot_state, 7, now, windows);
            checksum = checksum
                .wrapping_add(edge_checksum(edge))
                .wrapping_add(u64::from(remove))
                .wrapping_add(u64::from(new_due_at.is_some()))
                .wrapping_add(state_checksum(slot_state));
        }
        checksum
    }

    pub fn pending_store(_: usize) -> DebounceStore {
        let window = Duration::from_millis(200);
        let now = Instant::now();
        let mut store = DebounceStore::new();
        store.prepare_slots(1);
        store.slots[0] = SlotState {
            state: Some(DebounceState {
                action_mask: MASK,
                source: InputSource::Keyboard,
                held_raw: false,
                held_reported: true,
                last_raw_change_time: now,
                last_raw_change_host_nanos: 88,
                last_raw_store_time: now,
                last_report_time: now,
            }),
            due_at: now.checked_add(window),
            scheduled_ix: 0,
            epoch: store.epoch,
        };
        store.due_slots.push(0);
        store
    }

    pub fn pending_duplicate_old(store: &mut DebounceStore, events: usize) -> u64 {
        let timestamp = Instant::now();
        let windows = DebounceWindows::uniform(Duration::from_millis(200));
        let mut checksum = 0u64;
        for index in 0..events {
            let slot = 0;
            store.ensure_slot(slot);
            let now = Instant::now();
            let old_due_at = store.slots[slot].due_at;
            let mut state = store.slots[slot]
                .state
                .expect("pending benchmark state initialized");
            let edges = debounce_step(
                &mut state,
                MASK,
                InputSource::Keyboard,
                false,
                timestamp,
                index as u64,
                now,
                slot as u32,
                windows,
            );
            let prune = should_prune_debounce_state(state, now, windows);
            let new_due_at = if prune {
                store.slots[slot].state = None;
                None
            } else {
                store.slots[slot].state = Some(state);
                debounce_due_at(state, windows)
            };
            store.refresh_due_slot(slot, old_due_at, new_due_at);
            checksum = checksum
                .wrapping_add(edge_checksum(edges.first))
                .wrapping_add(edge_checksum(edges.second))
                .wrapping_add(state_checksum(store.slots[slot]));
        }
        checksum
    }

    pub fn pending_duplicate_new(store: &mut DebounceStore, events: usize) -> u64 {
        let timestamp = Instant::now();
        let windows = DebounceWindows::uniform(Duration::from_millis(200));
        let mut checksum = 0u64;
        for index in 0..events {
            let edges = debounce_input_edge_in_store_mut(
                store,
                0,
                MASK,
                InputSource::Keyboard,
                false,
                timestamp,
                index as u64,
                windows,
            );
            checksum = checksum
                .wrapping_add(edge_checksum(edges.first))
                .wrapping_add(edge_checksum(edges.second))
                .wrapping_add(state_checksum(store.slots[0]));
        }
        checksum
    }
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

    fn resolve_due_reference(
        slot_state: &mut SlotState,
        input_slot: u32,
        now: Instant,
        windows: DebounceWindows,
    ) -> (Option<DebouncedEdge>, bool, Option<Instant>) {
        let Some(mut state) = slot_state.state else {
            return (None, false, None);
        };
        let edge = debounce_emit_if_due(&mut state, input_slot, now, windows);
        let remove = should_prune_debounce_state(state, now, windows);
        let new_due_at = if remove {
            slot_state.state = None;
            None
        } else {
            slot_state.state = Some(state);
            debounce_due_at(state, windows)
        };
        (edge, remove, new_due_at)
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
    fn clear_and_reserve_presizes_bounded_schedule() {
        let mut store = DebounceStore::new();
        store.clear_and_reserve(8);
        assert!(store.slots.capacity() >= 8);
        assert!(store.due_slots.capacity() >= 8);

        store.clear_and_reserve(16);
        assert!(store.slots.capacity() >= 16);
        assert!(store.due_slots.capacity() >= 16);
    }

    #[test]
    fn first_released_value_keeps_slot_empty_and_next_press_exact() {
        let windows = DebounceWindows::uniform(Duration::from_millis(20));
        let timestamp = Instant::now();
        let mut store = DebounceStore::new();
        store.prepare_slots(1);

        let released = debounce_input_edge_in_store_mut(
            &mut store,
            0,
            TEST_MASK,
            InputSource::Keyboard,
            false,
            timestamp,
            10,
            windows,
        );
        assert!(released.first.is_none());
        assert!(released.second.is_none());
        assert!(store.slots[0].state.is_none());
        assert!(store.slots[0].due_at.is_none());
        assert!(store.due_slots.is_empty());
        assert_eq!(store.active_len, 0);

        let pressed = debounce_input_edge_in_store_mut(
            &mut store,
            0,
            TEST_MASK,
            InputSource::Keyboard,
            true,
            timestamp,
            11,
            windows,
        );
        let edge = pressed.second.expect("first press emits immediately");
        assert!(pressed.first.is_none());
        assert!(edge.pressed);
        assert_eq!(edge.timestamp, timestamp);
        assert_eq!(edge.timestamp_host_nanos, 11);
        assert_eq!(store.active_len, 1);
    }

    #[test]
    fn due_resolution_matches_generic_state_machine() {
        let original_window = Duration::from_millis(20);
        let original_now = Instant::now();
        let last_report_time = original_now
            .checked_sub(original_window)
            .unwrap_or(original_now);

        for (held_raw, held_reported, window, advance) in [
            (false, true, original_window, Duration::ZERO),
            (true, false, original_window, Duration::ZERO),
            (false, false, original_window, Duration::ZERO),
            (true, true, original_window, Duration::ZERO),
            (false, true, Duration::from_millis(40), Duration::ZERO),
            (false, true, Duration::ZERO, Duration::ZERO),
            (false, false, original_window, Duration::from_millis(5)),
        ] {
            let now = original_now + advance;
            let state = DebounceState {
                action_mask: TEST_MASK,
                source: InputSource::Keyboard,
                held_raw,
                held_reported,
                last_raw_change_time: last_report_time,
                last_raw_change_host_nanos: 456,
                last_raw_store_time: last_report_time,
                last_report_time,
            };
            let mut old_slot = SlotState {
                state: Some(state),
                due_at: Some(original_now),
                scheduled_ix: 0,
                epoch: 1,
            };
            let mut new_slot = old_slot;
            let windows = DebounceWindows::uniform(window);

            let old = resolve_due_reference(&mut old_slot, TEST_SLOT, now, windows);
            let new = resolve_due_slot(&mut new_slot, TEST_SLOT, now, windows);
            assert_eq!(new, old);
            assert_eq!(new_slot.state, old_slot.state);
        }
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
    fn settled_duplicate_skips_clock_independent_state_changes() {
        let window = Duration::from_millis(20);
        let windows = DebounceWindows::uniform(window);
        let timestamp = Instant::now();
        let mut store = DebounceStore::new();
        store.prepare_slots(1);

        let press = debounce_input_edge_in_store_mut(
            &mut store,
            0,
            TEST_MASK,
            InputSource::Keyboard,
            true,
            timestamp,
            100,
            windows,
        );
        assert!(press.second.is_some());
        let before = store.slots[0];

        let duplicate = debounce_input_edge_in_store_mut(
            &mut store,
            0,
            TEST_MASK,
            InputSource::Keyboard,
            true,
            timestamp + Duration::from_secs(1),
            999,
            windows,
        );

        assert!(duplicate.first.is_none());
        assert!(duplicate.second.is_none());
        assert_eq!(store.slots[0].due_at, before.due_at);
        let after = store.slots[0].state.expect("settled state");
        let before = before.state.expect("settled state");
        assert_eq!(after.last_raw_change_time, before.last_raw_change_time);
        assert_eq!(
            after.last_raw_change_host_nanos,
            before.last_raw_change_host_nanos
        );
        assert_eq!(after.last_raw_store_time, before.last_raw_store_time);
        assert_eq!(after.last_report_time, before.last_report_time);
    }

    #[test]
    fn pending_duplicate_preserves_state_until_current_window_is_due() {
        let long_window = Duration::from_millis(200);
        let timestamp = Instant::now();
        let mut store = DebounceStore::new();
        store.prepare_slots(1);

        let press = debounce_input_edge_in_store_mut(
            &mut store,
            0,
            TEST_MASK,
            InputSource::Keyboard,
            true,
            timestamp,
            100,
            DebounceWindows::uniform(long_window),
        );
        assert!(press.second.is_some());
        let release = debounce_input_edge_in_store_mut(
            &mut store,
            0,
            TEST_MASK,
            InputSource::Keyboard,
            false,
            timestamp,
            101,
            DebounceWindows::uniform(long_window),
        );
        assert!(release.first.is_none());
        assert!(release.second.is_none());
        let before = store.slots[0];

        let duplicate = debounce_input_edge_in_store_mut(
            &mut store,
            0,
            TEST_MASK,
            InputSource::Keyboard,
            false,
            timestamp + Duration::from_secs(1),
            999,
            DebounceWindows::uniform(long_window),
        );
        assert!(duplicate.first.is_none());
        assert!(duplicate.second.is_none());
        assert_eq!(store.slots[0], before);

        let state = store.slots[0].state.as_mut().expect("pending release");
        state.last_report_time = Instant::now()
            .checked_sub(Duration::from_millis(20))
            .expect("test time supports subtraction");
        let shortened = debounce_input_edge_in_store_mut(
            &mut store,
            0,
            TEST_MASK,
            InputSource::Keyboard,
            false,
            timestamp + Duration::from_secs(2),
            1_000,
            DebounceWindows::uniform(Duration::from_millis(10)),
        );
        assert!(shortened.first.is_some_and(|edge| !edge.pressed));
        assert!(shortened.second.is_none());
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
    fn due_schedule_removes_slot_after_chatter_cancel() {
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
        assert!(states.lock().unwrap().due_slots.is_empty());

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
