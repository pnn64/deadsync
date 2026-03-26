use super::{App, CurrentScreen, GAMEPLAY_INPUT_RING_CAPACITY, TransitionState};
use crate::config;
use crate::core::input::{self, InputEvent};
use log::warn;
use std::cell::UnsafeCell;
use std::error::Error;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::time::Instant;
use winit::event_loop::ActiveEventLoop;

#[derive(Clone, Copy, Debug)]
pub(super) struct GameplayRawKeyEvent {
    pub(super) code: winit::keyboard::KeyCode,
    pub(super) pressed: bool,
    pub(super) timestamp: Instant,
    pub(super) timestamp_host_nanos: u64,
    pub(super) stored_at: Instant,
    pub(super) emitted_at: Instant,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum GameplayQueuedEvent {
    Input(InputEvent),
    RawKey(GameplayRawKeyEvent),
}

impl GameplayQueuedEvent {
    #[inline(always)]
    const fn timestamp(self) -> Instant {
        match self {
            Self::Input(ev) => ev.timestamp,
            Self::RawKey(ev) => ev.timestamp,
        }
    }

    #[inline(always)]
    const fn timestamp_host_nanos(self) -> u64 {
        match self {
            Self::Input(ev) => ev.timestamp_host_nanos,
            Self::RawKey(ev) => ev.timestamp_host_nanos,
        }
    }

    #[inline(always)]
    const fn stored_at(self) -> Instant {
        match self {
            Self::Input(ev) => ev.stored_at,
            Self::RawKey(ev) => ev.stored_at,
        }
    }

    #[inline(always)]
    const fn emitted_at(self) -> Instant {
        match self {
            Self::Input(ev) => ev.emitted_at,
            Self::RawKey(ev) => ev.emitted_at,
        }
    }

    #[inline(always)]
    const fn source(self) -> input::InputSource {
        match self {
            Self::Input(ev) => ev.source,
            Self::RawKey(_) => input::InputSource::Keyboard,
        }
    }
}

pub(super) struct GameplayInputRing {
    enabled: AtomicBool,
    head: AtomicUsize,
    tail: AtomicUsize,
    dropped: AtomicU32,
    slots: [UnsafeCell<MaybeUninit<GameplayQueuedEvent>>; GAMEPLAY_INPUT_RING_CAPACITY],
}

// SAFETY: `GameplayInputRing` is a single-producer/single-consumer ring. Slot
// ownership is synchronized with the `head`/`tail` atomics, so moving the ring
// between threads does not create unsynchronized aliasing of initialized items.
unsafe impl Send for GameplayInputRing {}
// SAFETY: readers and writers coordinate exclusively through atomics and only
// touch disjoint slots at any instant. Shared references are therefore safe as
// long as callers preserve the intended SPSC usage.
unsafe impl Sync for GameplayInputRing {}

impl GameplayInputRing {
    #[inline(always)]
    pub(super) fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            dropped: AtomicU32::new(0),
            slots: std::array::from_fn(|_| UnsafeCell::new(MaybeUninit::uninit())),
        }
    }

    #[inline(always)]
    pub(super) fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    #[inline(always)]
    pub(super) fn swap_enabled(&self, enabled: bool) -> bool {
        self.enabled.swap(enabled, Ordering::Relaxed)
    }

    #[inline(always)]
    pub(super) fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    #[inline(always)]
    pub(super) fn push(&self, ev: GameplayQueuedEvent) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        if tail.wrapping_sub(head) == GAMEPLAY_INPUT_RING_CAPACITY {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let slot = tail % GAMEPLAY_INPUT_RING_CAPACITY;
        // SAFETY: `tail - head < capacity` guarantees this slot is not currently
        // visible to the consumer. The write happens-before publication via the
        // following Release store to `tail`.
        unsafe { (*self.slots[slot].get()).write(ev) };
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
    }

    #[inline(always)]
    pub(super) fn pop(&self) -> Option<GameplayQueuedEvent> {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        if head == tail {
            return None;
        }
        let slot = head % GAMEPLAY_INPUT_RING_CAPACITY;
        // SAFETY: `head != tail` means the producer has already initialized this
        // slot and published it with a Release store to `tail`. The Acquire load
        // above pairs with that store before we read the item out exactly once.
        let ev = unsafe { (*self.slots[slot].get()).assume_init_read() };
        self.head.store(head.wrapping_add(1), Ordering::Release);
        Some(ev)
    }

    #[inline(always)]
    pub(super) fn clear(&self) {
        while self.pop().is_some() {}
    }

    #[inline(always)]
    pub(super) fn take_dropped(&self) -> u32 {
        self.dropped.swap(0, Ordering::Relaxed)
    }
}

#[inline(always)]
pub(super) fn gameplay_event_precedes(a: &GameplayQueuedEvent, b: &GameplayQueuedEvent) -> bool {
    if a.timestamp_host_nanos() != 0
        && b.timestamp_host_nanos() != 0
        && a.timestamp_host_nanos() != b.timestamp_host_nanos()
    {
        return a.timestamp_host_nanos() < b.timestamp_host_nanos();
    }
    if a.timestamp() != b.timestamp() {
        return a.timestamp() < b.timestamp();
    }
    if a.emitted_at() != b.emitted_at() {
        return a.emitted_at() < b.emitted_at();
    }
    if a.stored_at() != b.stored_at() {
        return a.stored_at() < b.stored_at();
    }
    matches!(
        (a.source(), b.source()),
        (input::InputSource::Keyboard, input::InputSource::Gamepad)
    )
}

#[inline(always)]
pub(super) fn gameplay_raw_key_event(
    raw_key: &input::RawKeyboardEvent,
) -> Option<GameplayQueuedEvent> {
    use winit::keyboard::KeyCode;

    if raw_key.repeat {
        return None;
    }
    match raw_key.code {
        KeyCode::ShiftLeft
        | KeyCode::ShiftRight
        | KeyCode::ControlLeft
        | KeyCode::ControlRight
        | KeyCode::KeyR
        | KeyCode::F6
        | KeyCode::F7
        | KeyCode::F8
        | KeyCode::F11
        | KeyCode::F12 => {}
        _ => return None,
    }
    Some(GameplayQueuedEvent::RawKey(GameplayRawKeyEvent {
        code: raw_key.code,
        pressed: raw_key.pressed,
        timestamp: raw_key.timestamp,
        timestamp_host_nanos: raw_key.host_nanos,
        stored_at: raw_key.timestamp,
        emitted_at: raw_key.timestamp,
    }))
}

#[inline(always)]
pub(super) fn proxy_gameplay_raw_key(raw_key: &input::RawKeyboardEvent) -> bool {
    use winit::keyboard::KeyCode;

    matches!(
        raw_key.code,
        KeyCode::ShiftLeft
            | KeyCode::ShiftRight
            | KeyCode::ControlLeft
            | KeyCode::ControlRight
            | KeyCode::F3
            | KeyCode::F10
    )
}

impl App {
    pub(super) fn flush_due_input_events(
        &mut self,
        event_loop: &ActiveEventLoop,
    ) -> Result<bool, Box<dyn Error>> {
        if !matches!(self.state.shell.transition, TransitionState::Idle)
            || self.state.screens.current_screen == CurrentScreen::Init
        {
            input::clear_debounce_state();
            return Ok(false);
        }
        let mut flushed = false;
        let mut err: Option<Box<dyn Error>> = None;
        let gameplay_screen = self.state.screens.current_screen == CurrentScreen::Gameplay;
        let start_screen = self.state.screens.current_screen;
        let mut discard_gameplay_batch = false;
        input::drain_debounced_input_events_with(|ev| {
            flushed = true;
            if gameplay_screen {
                if discard_gameplay_batch || err.is_some() {
                    return;
                }
                if let Err(e) =
                    self.route_gameplay_event(event_loop, GameplayQueuedEvent::Input(ev))
                {
                    err = Some(e);
                    return;
                }
                if !self.gameplay_dispatch_continues(start_screen) {
                    discard_gameplay_batch = true;
                }
            } else if err.is_none()
                && let Err(e) = self.route_input_event(event_loop, ev)
            {
                err = Some(e);
            }
        });
        if discard_gameplay_batch {
            self.gameplay_key_ring.clear();
            self.gameplay_pad_ring.clear();
        }
        if let Some(e) = err {
            return Err(e);
        }
        Ok(flushed)
    }

    #[inline(always)]
    pub(super) fn gameplay_dispatch_continues(&self, start_screen: CurrentScreen) -> bool {
        self.state.screens.current_screen == start_screen
            && matches!(self.state.shell.transition, TransitionState::Idle)
    }

    #[inline(always)]
    pub(super) fn route_gameplay_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        ev: GameplayQueuedEvent,
    ) -> Result<(), Box<dyn Error>> {
        self.state.shell.note_gameplay_queued_input();
        match ev {
            GameplayQueuedEvent::Input(ev) => self.route_input_event(event_loop, ev),
            GameplayQueuedEvent::RawKey(ev) => {
                self.route_gameplay_raw_key_event(event_loop, ev);
                Ok(())
            }
        }
    }

    fn route_gameplay_raw_key_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        ev: GameplayRawKeyEvent,
    ) {
        if self.state.screens.current_screen != CurrentScreen::Gameplay {
            return;
        }
        let Some(gs) = self.state.screens.gameplay_state.as_mut() else {
            return;
        };
        let allow_commands = self.state.gameplay_offset_save_prompt.is_none();
        let action = crate::game::gameplay::handle_queued_raw_key(
            gs,
            ev.code,
            ev.pressed,
            ev.timestamp,
            allow_commands,
        );
        if matches!(action, crate::game::gameplay::RawKeyAction::Restart)
            && config::get().keyboard_features
            && self.state.session.course_run.is_none()
        {
            self.try_gameplay_restart(event_loop, "Ctrl+R");
        }
    }

    #[inline(always)]
    pub(super) fn sync_gameplay_input_capture(&self) {
        let capture_enabled = self.accepts_live_input();
        let ring_enabled = capture_enabled
            && self.state.screens.current_screen == CurrentScreen::Gameplay
            && matches!(self.state.shell.transition, TransitionState::Idle);
        let key_was_enabled = self.gameplay_key_ring.swap_enabled(ring_enabled);
        let pad_was_enabled = self.gameplay_pad_ring.swap_enabled(ring_enabled);
        input::set_raw_keyboard_capture_enabled(ring_enabled);
        if key_was_enabled != ring_enabled || !ring_enabled {
            self.gameplay_key_ring.clear();
            self.gameplay_key_ring.take_dropped();
        }
        if pad_was_enabled != ring_enabled || !ring_enabled {
            self.gameplay_pad_ring.clear();
            self.gameplay_pad_ring.take_dropped();
        }
    }

    #[inline(always)]
    pub(super) fn clear_gameplay_input_events(&self) {
        self.gameplay_key_ring.set_enabled(false);
        self.gameplay_pad_ring.set_enabled(false);
        input::set_raw_keyboard_capture_enabled(false);
        self.gameplay_key_ring.clear();
        self.gameplay_key_ring.take_dropped();
        self.gameplay_pad_ring.clear();
        self.gameplay_pad_ring.take_dropped();
    }

    pub(super) fn drain_gameplay_input_events(
        &mut self,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn Error>> {
        let dropped_keys = self.gameplay_key_ring.take_dropped();
        if dropped_keys > 0 {
            warn!(
                "Gameplay key input ring overflowed; dropped {dropped_keys} event(s) on screen {:?}",
                self.state.screens.current_screen
            );
        }
        let dropped_pads = self.gameplay_pad_ring.take_dropped();
        if dropped_pads > 0 {
            warn!(
                "Gameplay pad input ring overflowed; dropped {dropped_pads} event(s) on screen {:?}",
                self.state.screens.current_screen
            );
        }
        if self.state.screens.current_screen != CurrentScreen::Gameplay
            || !matches!(self.state.shell.transition, TransitionState::Idle)
        {
            self.gameplay_key_ring.clear();
            self.gameplay_pad_ring.clear();
            return Ok(());
        }
        let start_screen = self.state.screens.current_screen;
        let mut next_key = self.gameplay_key_ring.pop();
        let mut next_pad = self.gameplay_pad_ring.pop();
        loop {
            let ev = match (next_key.as_ref(), next_pad.as_ref()) {
                (Some(key_ev), Some(pad_ev)) => {
                    if gameplay_event_precedes(key_ev, pad_ev) {
                        let ev = next_key.take().unwrap();
                        next_key = self.gameplay_key_ring.pop();
                        ev
                    } else {
                        let ev = next_pad.take().unwrap();
                        next_pad = self.gameplay_pad_ring.pop();
                        ev
                    }
                }
                (Some(_), None) => {
                    let ev = next_key.take().unwrap();
                    next_key = self.gameplay_key_ring.pop();
                    ev
                }
                (None, Some(_)) => {
                    let ev = next_pad.take().unwrap();
                    next_pad = self.gameplay_pad_ring.pop();
                    ev
                }
                (None, None) => break,
            };
            self.route_gameplay_event(event_loop, ev)?;
            if !self.gameplay_dispatch_continues(start_screen) {
                self.gameplay_key_ring.clear();
                self.gameplay_pad_ring.clear();
                break;
            }
        }
        Ok(())
    }
}
