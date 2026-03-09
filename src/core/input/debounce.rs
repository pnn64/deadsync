use std::collections::HashMap;
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
    pub(super) last_raw_store_time: Instant,
    pub(super) last_report_time: Instant,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct DebouncedEdge {
    pub(super) binding: DebounceBinding,
    pub(super) pressed: bool,
    pub(super) source: InputSource,
    pub(super) timestamp: Instant,
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
    pub(super) press: Duration,
    pub(super) release: Duration,
}

impl DebounceWindows {
    #[cfg_attr(not(test), allow(dead_code))]
    #[inline(always)]
    pub(super) const fn uniform(window: Duration) -> Self {
        // ITGmania InputFilter parity: gameplay debounce is a symmetric window
        // unless the caller explicitly asks for a different release window.
        Self {
            press: window,
            release: window,
        }
    }

    #[inline(always)]
    pub(super) fn prune_window(self) -> Duration {
        if self.press >= self.release {
            self.press
        } else {
            self.release
        }
    }
}

#[inline(always)]
pub(super) fn debounce_emit_if_due(
    state: &mut DebounceState,
    now: Instant,
    windows: DebounceWindows,
) -> Option<(bool, Instant, Instant)> {
    // ITGmania parity: the debounce gate compares against the last reported edge,
    // not just the last raw edge, so chatter inside the window is suppressed.
    if state.held_raw == state.held_reported {
        return None;
    }
    let window = if state.held_raw {
        windows.press
    } else {
        windows.release
    };
    if now.duration_since(state.last_report_time) < window {
        return None;
    }
    state.last_report_time = now;
    state.held_reported = state.held_raw;
    Some((
        state.held_reported,
        state.last_raw_change_time,
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
    stored_at: Instant,
    emitted_at: Instant,
) -> DebouncedEdge {
    DebouncedEdge {
        binding,
        pressed,
        source: debounce_binding_source(binding),
        timestamp,
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
    now: Instant,
    windows: DebounceWindows,
) -> DebounceEdges {
    // ITGmania InputFilter parity: flush any now-due delayed edge before storing
    // the new raw state, so a delayed release can still report just ahead of a
    // later repress instead of being silently lost.
    let first =
        debounce_emit_if_due(state, now, windows).map(|(debounced_pressed, ts, stored_at)| {
            debounced_edge(binding, debounced_pressed, ts, stored_at, now)
        });
    if state.held_raw != pressed {
        state.held_raw = pressed;
        state.last_raw_change_time = timestamp;
        state.last_raw_store_time = now;
    }
    let second =
        debounce_emit_if_due(state, now, windows).map(|(debounced_pressed, ts, stored_at)| {
            debounced_edge(binding, debounced_pressed, ts, stored_at, now)
        });
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

pub(super) fn debounce_input_edge_in_store(
    states: &Mutex<HashMap<DebounceBinding, DebounceState>>,
    binding: DebounceBinding,
    pressed: bool,
    timestamp: Instant,
    windows: DebounceWindows,
) -> DebounceEdges {
    let now = Instant::now();
    let mut states = states.lock().unwrap();
    let (edges, prune) = {
        let state = states.entry(binding).or_insert_with(|| DebounceState {
            held_raw: false,
            held_reported: false,
            last_raw_change_time: timestamp,
            last_raw_store_time: now,
            last_report_time: now.checked_sub(windows.prune_window()).unwrap_or(now),
        });
        // Preserve the original raw edge timestamp separately from store/emission
        // time so gameplay can judge against the real edge time, like ITGmania.
        let edges = debounce_step(state, binding, pressed, timestamp, now, windows);
        (edges, should_prune_debounce_state(*state, now, windows))
    };
    if prune {
        states.remove(&binding);
    }
    edges
}

pub(super) fn collect_due_debounce_edges_from(
    states: &Mutex<HashMap<DebounceBinding, DebounceState>>,
    now: Instant,
    windows: DebounceWindows,
) -> Vec<DebouncedEdge> {
    // ITGmania Update() parity: delayed edges are surfaced later, but they still
    // carry the original raw timestamp that caused the debounce holdoff.
    let mut states = states.lock().unwrap();
    let mut out: Vec<DebouncedEdge> = Vec::with_capacity(states.len().min(8));
    let mut prune: Vec<DebounceBinding> = Vec::new();
    for (&binding, state) in states.iter_mut() {
        if let Some((pressed, timestamp, stored_at)) = debounce_emit_if_due(state, now, windows) {
            out.push(debounced_edge(binding, pressed, timestamp, stored_at, now));
        }
        if should_prune_debounce_state(*state, now, windows) {
            prune.push(binding);
        }
    }
    for binding in prune {
        states.remove(&binding);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_state(now: Instant, window: Duration) -> DebounceState {
        DebounceState {
            held_raw: false,
            held_reported: false,
            last_raw_change_time: now,
            last_raw_store_time: now,
            last_report_time: now.checked_sub(window).unwrap_or(now),
        }
    }

    fn windows(press_ms: u64, release_ms: u64) -> DebounceWindows {
        DebounceWindows {
            press: Duration::from_millis(press_ms),
            release: Duration::from_millis(release_ms),
        }
    }

    fn assert_edge(
        edge: Option<DebouncedEdge>,
        binding: DebounceBinding,
        pressed: bool,
        timestamp: Instant,
        stored_at: Instant,
        emitted_at: Instant,
    ) {
        let edge = edge.expect("expected debounced edge");
        assert_eq!(edge.binding, binding);
        assert_eq!(edge.pressed, pressed);
        assert_eq!(edge.timestamp, timestamp);
        assert_eq!(edge.stored_at, stored_at);
        assert_eq!(edge.emitted_at, emitted_at);
    }

    #[test]
    fn debounce_keeps_short_tap_and_delays_release() {
        let window = Duration::from_millis(20);
        let windows = DebounceWindows::uniform(window);
        let t0 = Instant::now();
        let binding = DebounceBinding::Keyboard(KeyCode::KeyA);
        let mut state = base_state(t0, window);

        let press = debounce_step(&mut state, binding, true, t0, t0, windows);
        assert!(press.first.is_none());
        assert_edge(press.second, binding, true, t0, t0, t0);

        let release_ts = t0 + Duration::from_millis(1);
        let release = debounce_step(&mut state, binding, false, release_ts, release_ts, windows);
        assert!(release.first.is_none());
        assert!(release.second.is_none());

        let delayed = debounce_emit_if_due(&mut state, t0 + Duration::from_millis(21), windows);
        assert_eq!(delayed, Some((false, release_ts, release_ts)));
    }

    #[test]
    fn debounce_cancels_quick_release_repress_chatter() {
        let window = Duration::from_millis(20);
        let windows = DebounceWindows::uniform(window);
        let t0 = Instant::now();
        let binding = DebounceBinding::Keyboard(KeyCode::KeyA);
        let mut state = base_state(t0, window);

        let press = debounce_step(&mut state, binding, true, t0, t0, windows);
        assert!(press.first.is_none());
        assert_edge(press.second, binding, true, t0, t0, t0);

        let release_ts = t0 + Duration::from_millis(1);
        let release = debounce_step(&mut state, binding, false, release_ts, release_ts, windows);
        assert!(release.first.is_none());
        assert!(release.second.is_none());

        let repress_ts = t0 + Duration::from_millis(5);
        let repress = debounce_step(&mut state, binding, true, repress_ts, repress_ts, windows);
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
        let binding = DebounceBinding::Keyboard(KeyCode::KeyA);
        let mut state = base_state(t0, window);

        let press = debounce_step(&mut state, binding, true, t0, t0, windows);
        assert!(press.first.is_none());
        assert_edge(press.second, binding, true, t0, t0, t0);

        let release_ts = t0 + Duration::from_millis(1);
        let release = debounce_step(&mut state, binding, false, release_ts, release_ts, windows);
        assert!(release.first.is_none());
        assert!(release.second.is_none());

        let repress_ts = t0 + Duration::from_millis(30);
        let repress = debounce_step(&mut state, binding, true, repress_ts, repress_ts, windows);
        assert_edge(
            repress.first,
            binding,
            false,
            release_ts,
            release_ts,
            repress_ts,
        );
        assert!(repress.second.is_none());

        assert_eq!(
            debounce_emit_if_due(&mut state, t0 + Duration::from_millis(50), windows),
            Some((true, repress_ts, repress_ts))
        );
    }

    #[test]
    fn debounce_can_use_shorter_release_window() {
        let t0 = Instant::now();
        let binding = DebounceBinding::Keyboard(KeyCode::KeyA);
        let mut state = base_state(t0, Duration::from_millis(20));
        let windows = windows(20, 5);

        let press = debounce_step(&mut state, binding, true, t0, t0, windows);
        assert!(press.first.is_none());
        assert_edge(press.second, binding, true, t0, t0, t0);

        let release_ts = t0 + Duration::from_millis(1);
        let release = debounce_step(&mut state, binding, false, release_ts, release_ts, windows);
        assert!(release.first.is_none());
        assert!(release.second.is_none());

        assert_eq!(
            debounce_emit_if_due(&mut state, t0 + Duration::from_millis(6), windows),
            Some((false, release_ts, release_ts))
        );
    }
}
