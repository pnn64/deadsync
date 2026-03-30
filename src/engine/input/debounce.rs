use std::sync::Mutex;
use std::time::{Duration, Instant};

use winit::keyboard::KeyCode;

use super::{InputSource, PadCode, PadDir, PadId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum DebounceBinding {
    Keyboard(KeyCode),
    PadDir {
        id: PadId,
        dir: PadDir,
    },
    PadButton {
        id: PadId,
        code: PadCode,
        uuid: [u8; 16],
    },
}

#[derive(Clone, Copy, Debug)]
pub(super) struct DebounceState {
    pub(super) held_raw: bool,
    pub(super) held_reported: bool,
    pub(super) last_raw_change_time: Instant,
    pub(super) last_raw_change_host_nanos: u64,
    pub(super) last_raw_store_time: Instant,
    pub(super) last_report_time: Instant,
}

#[derive(Clone, Copy, Debug)]
struct DebounceEntry {
    binding: DebounceBinding,
    state: DebounceState,
}

#[derive(Debug, Default)]
pub(super) struct DebounceStore {
    entries: Vec<DebounceEntry>,
    next_due_at: Option<Instant>,
}

impl DebounceStore {
    #[inline(always)]
    pub(super) const fn new() -> Self {
        Self {
            entries: Vec::new(),
            next_due_at: None,
        }
    }

    #[inline(always)]
    pub(super) fn clear_and_reserve(&mut self, cap: usize) {
        self.entries.clear();
        self.next_due_at = None;
        let needed = cap.saturating_sub(self.entries.capacity());
        if needed > 0 {
            self.entries.reserve(needed);
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    #[inline(always)]
    pub(super) fn capacity(&self) -> usize {
        self.entries.capacity()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    #[inline(always)]
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }

    #[inline(always)]
    fn find_index(&self, binding: DebounceBinding) -> Option<usize> {
        self.entries
            .iter()
            .position(|entry| entry.binding == binding)
    }

    #[inline(always)]
    fn recalc_next_due_at(&mut self, windows: DebounceWindows) {
        self.next_due_at = self
            .entries
            .iter()
            .filter_map(|entry| debounce_due_at(entry.state, windows))
            .min();
    }

    #[inline(always)]
    fn refresh_next_due_at_after_entry_change(
        &mut self,
        old_due_at: Option<Instant>,
        new_due_at: Option<Instant>,
        windows: DebounceWindows,
    ) {
        if old_due_at == new_due_at {
            return;
        }
        if let Some(next_due_at) = self.next_due_at {
            if let Some(new_due_at) = new_due_at
                && new_due_at < next_due_at
            {
                self.next_due_at = Some(new_due_at);
                return;
            }
            if old_due_at == Some(next_due_at) {
                self.recalc_next_due_at(windows);
            }
            return;
        }
        self.next_due_at = new_due_at;
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct DebouncedEdge {
    pub(super) binding: DebounceBinding,
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
    #[cfg_attr(not(test), allow(dead_code))]
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
pub(super) fn debounce_emit_if_due(
    state: &mut DebounceState,
    now: Instant,
    windows: DebounceWindows,
) -> Option<(bool, Instant, u64, Instant)> {
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
    Some((
        state.held_reported,
        state.last_raw_change_time,
        state.last_raw_change_host_nanos,
        state.last_raw_store_time,
    ))
}

#[inline(always)]
fn debounce_binding_source(binding: DebounceBinding) -> InputSource {
    match binding {
        DebounceBinding::Keyboard(_) => InputSource::Keyboard,
        DebounceBinding::PadDir { .. } | DebounceBinding::PadButton { .. } => InputSource::Gamepad,
    }
}

#[inline(always)]
fn debounced_edge(
    binding: DebounceBinding,
    pressed: bool,
    timestamp: Instant,
    timestamp_host_nanos: u64,
    stored_at: Instant,
    emitted_at: Instant,
) -> DebouncedEdge {
    DebouncedEdge {
        binding,
        pressed,
        source: debounce_binding_source(binding),
        timestamp,
        timestamp_host_nanos,
        stored_at,
        emitted_at,
    }
}

#[inline(always)]
pub(super) fn debounce_step(
    state: &mut DebounceState,
    binding: DebounceBinding,
    pressed: bool,
    timestamp: Instant,
    timestamp_host_nanos: u64,
    now: Instant,
    windows: DebounceWindows,
) -> DebounceEdges {
    // ITGmania InputFilter parity: flush any now-due delayed edge before storing
    // the new raw state, so a delayed release can still report just ahead of a
    // later repress instead of being silently lost.
    let first = debounce_emit_if_due(state, now, windows).map(
        |(debounced_pressed, ts, ts_host_nanos, stored_at)| {
            debounced_edge(
                binding,
                debounced_pressed,
                ts,
                ts_host_nanos,
                stored_at,
                now,
            )
        },
    );
    if state.held_raw != pressed {
        state.held_raw = pressed;
        state.last_raw_change_time = timestamp;
        state.last_raw_change_host_nanos = timestamp_host_nanos;
        state.last_raw_store_time = now;
    }
    let second = debounce_emit_if_due(state, now, windows).map(
        |(debounced_pressed, ts, ts_host_nanos, stored_at)| {
            debounced_edge(
                binding,
                debounced_pressed,
                ts,
                ts_host_nanos,
                stored_at,
                now,
            )
        },
    );
    DebounceEdges { first, second }
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
    if state.held_raw == state.held_reported {
        return None;
    }
    state.last_report_time.checked_add(windows.window)
}

pub(super) fn debounce_input_edge_in_store(
    states: &Mutex<DebounceStore>,
    binding: DebounceBinding,
    pressed: bool,
    timestamp: Instant,
    timestamp_host_nanos: u64,
    windows: DebounceWindows,
) -> DebounceEdges {
    let now = Instant::now();
    let mut states = states.lock().unwrap();
    if let Some(index) = states.find_index(binding) {
        let (edges, prune, old_due_at, new_due_at) = {
            let state = &mut states.entries[index].state;
            let old_due_at = debounce_due_at(*state, windows);
            let edges = debounce_step(
                state,
                binding,
                pressed,
                timestamp,
                timestamp_host_nanos,
                now,
                windows,
            );
            let prune = should_prune_debounce_state(*state, now, windows);
            let new_due_at = if prune {
                None
            } else {
                debounce_due_at(*state, windows)
            };
            (edges, prune, old_due_at, new_due_at)
        };
        if prune {
            states.entries.swap_remove(index);
        }
        states.refresh_next_due_at_after_entry_change(old_due_at, new_due_at, windows);
        edges
    } else {
        let mut state = DebounceState {
            held_raw: false,
            held_reported: false,
            last_raw_change_time: timestamp,
            last_raw_change_host_nanos: timestamp_host_nanos,
            last_raw_store_time: now,
            last_report_time: now.checked_sub(windows.prune_window()).unwrap_or(now),
        };
        let edges = debounce_step(
            &mut state,
            binding,
            pressed,
            timestamp,
            timestamp_host_nanos,
            now,
            windows,
        );
        let prune = should_prune_debounce_state(state, now, windows);
        let new_due_at = if prune {
            None
        } else {
            debounce_due_at(state, windows)
        };
        if !prune {
            states.entries.push(DebounceEntry { binding, state });
        }
        states.refresh_next_due_at_after_entry_change(None, new_due_at, windows);
        edges
    }
}

pub(super) fn emit_due_debounce_edges_from(
    states: &Mutex<DebounceStore>,
    now: Instant,
    windows: DebounceWindows,
    mut emit: impl FnMut(DebouncedEdge),
) -> bool {
    // ITGmania Update() parity: delayed edges are surfaced later, but they still
    // carry the original raw timestamp that caused the debounce holdoff.
    let mut states = states.lock().unwrap();
    if let Some(next_due_at) = states.next_due_at
        && now < next_due_at
    {
        return false;
    }
    let mut flushed = false;
    let mut next_due_at = None;
    let mut i = 0;
    while i < states.entries.len() {
        let mut edge = None;
        let (remove, due_at) = {
            let entry = &mut states.entries[i];
            if let Some((pressed, timestamp, timestamp_host_nanos, stored_at)) =
                debounce_emit_if_due(&mut entry.state, now, windows)
            {
                edge = Some(debounced_edge(
                    entry.binding,
                    pressed,
                    timestamp,
                    timestamp_host_nanos,
                    stored_at,
                    now,
                ));
            }
            let remove = should_prune_debounce_state(entry.state, now, windows);
            let due_at = if remove {
                None
            } else {
                debounce_due_at(entry.state, windows)
            };
            (remove, due_at)
        };
        if let Some(edge) = edge {
            flushed = true;
            emit(edge);
        }
        if remove {
            states.entries.swap_remove(i);
            continue;
        }
        next_due_at = match (next_due_at, due_at) {
            (Some(current), Some(due_at)) => Some(current.min(due_at)),
            (None, due_at) | (due_at, None) => due_at,
        };
        i += 1;
    }
    states.next_due_at = next_due_at;
    flushed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_state(now: Instant, window: Duration) -> DebounceState {
        DebounceState {
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
        binding: DebounceBinding,
        pressed: bool,
        timestamp: Instant,
        timestamp_host_nanos: u64,
        stored_at: Instant,
        emitted_at: Instant,
    ) {
        let edge = edge.expect("expected debounced edge");
        assert_eq!(edge.binding, binding);
        assert_eq!(edge.pressed, pressed);
        assert_eq!(edge.timestamp, timestamp);
        assert_eq!(edge.timestamp_host_nanos, timestamp_host_nanos);
        assert_eq!(edge.stored_at, stored_at);
        assert_eq!(edge.emitted_at, emitted_at);
    }

    #[test]
    fn debounce_keeps_short_tap_and_delays_release() {
        let window = Duration::from_millis(20);
        let windows = DebounceWindows::uniform(window);
        let t0 = Instant::now();
        let t0_host = 100;
        let binding = DebounceBinding::Keyboard(KeyCode::KeyA);
        let mut state = base_state(t0, window);

        let press = debounce_step(&mut state, binding, true, t0, t0_host, t0, windows);
        assert!(press.first.is_none());
        assert_edge(press.second, binding, true, t0, t0_host, t0, t0);

        let release_ts = t0 + Duration::from_millis(1);
        let release_host = 101;
        let release = debounce_step(
            &mut state,
            binding,
            false,
            release_ts,
            release_host,
            release_ts,
            windows,
        );
        assert!(release.first.is_none());
        assert!(release.second.is_none());

        let delayed = debounce_emit_if_due(&mut state, t0 + Duration::from_millis(21), windows);
        assert_eq!(delayed, Some((false, release_ts, release_host, release_ts)));
    }

    #[test]
    fn debounce_cancels_quick_release_repress_chatter() {
        let window = Duration::from_millis(20);
        let windows = DebounceWindows::uniform(window);
        let t0 = Instant::now();
        let t0_host = 100;
        let binding = DebounceBinding::Keyboard(KeyCode::KeyA);
        let mut state = base_state(t0, window);

        let press = debounce_step(&mut state, binding, true, t0, t0_host, t0, windows);
        assert!(press.first.is_none());
        assert_edge(press.second, binding, true, t0, t0_host, t0, t0);

        let release_ts = t0 + Duration::from_millis(1);
        let release = debounce_step(
            &mut state, binding, false, release_ts, 101, release_ts, windows,
        );
        assert!(release.first.is_none());
        assert!(release.second.is_none());

        let repress_ts = t0 + Duration::from_millis(5);
        let repress = debounce_step(
            &mut state, binding, true, repress_ts, 105, repress_ts, windows,
        );
        assert!(repress.first.is_none());
        assert!(repress.second.is_none());

        assert_eq!(
            debounce_emit_if_due(&mut state, t0 + Duration::from_millis(25), windows),
            None
        );
    }

    #[test]
    fn debounce_flushes_due_release_before_new_press() {
        let window = Duration::from_millis(20);
        let windows = DebounceWindows::uniform(window);
        let t0 = Instant::now();
        let t0_host = 100;
        let binding = DebounceBinding::Keyboard(KeyCode::KeyA);
        let mut state = base_state(t0, window);

        let press = debounce_step(&mut state, binding, true, t0, t0_host, t0, windows);
        assert!(press.first.is_none());
        assert_edge(press.second, binding, true, t0, t0_host, t0, t0);

        let release_ts = t0 + Duration::from_millis(1);
        let release_host = 101;
        let release = debounce_step(
            &mut state,
            binding,
            false,
            release_ts,
            release_host,
            release_ts,
            windows,
        );
        assert!(release.first.is_none());
        assert!(release.second.is_none());

        let repress_ts = t0 + Duration::from_millis(30);
        let repress_host = 130;
        let repress = debounce_step(
            &mut state,
            binding,
            true,
            repress_ts,
            repress_host,
            repress_ts,
            windows,
        );
        assert_edge(
            repress.first,
            binding,
            false,
            release_ts,
            release_host,
            release_ts,
            repress_ts,
        );
        assert!(repress.second.is_none());

        assert_eq!(
            debounce_emit_if_due(&mut state, t0 + Duration::from_millis(50), windows),
            Some((true, repress_ts, repress_host, repress_ts))
        );
    }

    #[test]
    fn emit_due_edges_prunes_stale_entries_without_temp_buffers() {
        let window = Duration::from_millis(20);
        let windows = DebounceWindows::uniform(window);
        let t0 = Instant::now();
        let binding_due = DebounceBinding::Keyboard(KeyCode::KeyA);
        let binding_stale = DebounceBinding::Keyboard(KeyCode::KeyB);
        let mut due_state = base_state(t0, window);
        let stale_state = base_state(t0, window);

        let press = debounce_step(&mut due_state, binding_due, true, t0, 100, t0, windows);
        assert!(press.first.is_none());
        assert!(press.second.is_some());

        let release_ts = t0 + Duration::from_millis(1);
        let release = debounce_step(
            &mut due_state,
            binding_due,
            false,
            release_ts,
            101,
            release_ts,
            windows,
        );
        assert!(release.first.is_none());
        assert!(release.second.is_none());
        let mut store = DebounceStore::new();
        store.entries.push(DebounceEntry {
            binding: binding_due,
            state: due_state,
        });
        store.entries.push(DebounceEntry {
            binding: binding_stale,
            state: stale_state,
        });
        store.recalc_next_due_at(windows);
        let states = Mutex::new(store);
        let mut emitted = Vec::new();

        assert!(emit_due_debounce_edges_from(
            &states,
            t0 + Duration::from_millis(21),
            windows,
            |edge| emitted.push(edge),
        ));
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].binding, binding_due);
        assert!(!emitted[0].pressed);
        assert_eq!(emitted[0].timestamp, release_ts);
        assert_eq!(emitted[0].timestamp_host_nanos, 101);

        let guard = states.lock().unwrap();
        assert_eq!(guard.entries.len(), 1);
        assert_eq!(guard.entries[0].binding, binding_due);
        drop(guard);

        emitted.clear();
        assert!(!emit_due_debounce_edges_from(
            &states,
            t0 + Duration::from_millis(41),
            windows,
            |edge| emitted.push(edge),
        ));
        assert!(emitted.is_empty());
        assert!(states.lock().unwrap().entries.is_empty());
    }

    #[test]
    fn next_due_cache_recomputes_when_current_min_moves_later() {
        let window = Duration::from_millis(20);
        let windows = DebounceWindows::uniform(window);
        let t0 = Instant::now();
        let mut store = DebounceStore::new();
        let early_due = t0 + Duration::from_millis(10);
        let late_due = t0 + Duration::from_millis(30);
        store.entries.push(DebounceEntry {
            binding: DebounceBinding::Keyboard(KeyCode::KeyA),
            state: DebounceState {
                held_raw: true,
                held_reported: false,
                last_raw_change_time: t0,
                last_raw_change_host_nanos: 0,
                last_raw_store_time: t0,
                last_report_time: early_due.checked_sub(window).unwrap_or(early_due),
            },
        });
        store.entries.push(DebounceEntry {
            binding: DebounceBinding::Keyboard(KeyCode::KeyB),
            state: DebounceState {
                held_raw: true,
                held_reported: false,
                last_raw_change_time: t0,
                last_raw_change_host_nanos: 0,
                last_raw_store_time: t0,
                last_report_time: late_due.checked_sub(window).unwrap_or(late_due),
            },
        });
        store.recalc_next_due_at(windows);

        store.entries[0].state.last_report_time = t0 + Duration::from_millis(40);
        store.refresh_next_due_at_after_entry_change(
            Some(early_due),
            Some(t0 + Duration::from_millis(60)),
            windows,
        );

        assert_eq!(store.next_due_at, Some(late_due));
    }

    #[test]
    fn emit_due_edges_updates_next_due_without_rescan() {
        let window = Duration::from_millis(20);
        let windows = DebounceWindows::uniform(window);
        let t0 = Instant::now();
        let mut store = DebounceStore::new();
        let first_due = t0 + Duration::from_millis(10);
        let second_due = t0 + Duration::from_millis(30);
        store.entries.push(DebounceEntry {
            binding: DebounceBinding::Keyboard(KeyCode::KeyA),
            state: DebounceState {
                held_raw: true,
                held_reported: false,
                last_raw_change_time: t0,
                last_raw_change_host_nanos: 100,
                last_raw_store_time: t0,
                last_report_time: first_due.checked_sub(window).unwrap_or(first_due),
            },
        });
        store.entries.push(DebounceEntry {
            binding: DebounceBinding::Keyboard(KeyCode::KeyB),
            state: DebounceState {
                held_raw: true,
                held_reported: false,
                last_raw_change_time: t0,
                last_raw_change_host_nanos: 200,
                last_raw_store_time: t0,
                last_report_time: second_due.checked_sub(window).unwrap_or(second_due),
            },
        });
        store.recalc_next_due_at(windows);
        let states = Mutex::new(store);
        let mut emitted = Vec::new();

        assert!(emit_due_debounce_edges_from(
            &states,
            first_due,
            windows,
            |edge| emitted.push(edge)
        ));
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].binding, DebounceBinding::Keyboard(KeyCode::KeyA));

        let guard = states.lock().unwrap();
        assert_eq!(guard.next_due_at, Some(second_due));
        assert_eq!(guard.entries.len(), 2);
    }
}
