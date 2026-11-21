//! Queued physical edges and held-input repeats for game callback dispatch.

use crate::{InputEvent, PhysicalInput};

type CallbackInput = (InputEvent, Option<PhysicalInput>);

const REPEAT_DELAY_SECONDS: f32 = 0.375;
const REPEAT_INTERVAL_SECONDS: f32 = 0.125;

/// Held physical inputs and their repeat ages, transferable across screen reloads.
///
/// Pending edges and the previous edge used for alias filtering belong to the
/// receiving screen and are deliberately not part of this token.
#[derive(Default)]
pub struct HeldCallbackInputs(Vec<(InputEvent, Option<PhysicalInput>, f32)>);

/// Input ordering and repeat policy shared by authored game callbacks.
///
/// This state does not dispatch callbacks or replay gameplay actions. Collect a
/// frame's repeat batch before dispatching it so callback-side changes cannot
/// alter which inputs repeat in that frame.
#[derive(Default)]
pub struct CallbackInputState {
    pending: Vec<CallbackInput>,
    held: HeldCallbackInputs,
    last_input: Option<InputEvent>,
}

impl CallbackInputState {
    #[inline]
    pub fn queue(&mut self, events: impl IntoIterator<Item = (InputEvent, Option<PhysicalInput>)>) {
        self.pending.extend(events);
    }

    /// Detach the current queue so callbacks can enqueue later physical edges.
    #[inline]
    pub fn take_pending(&mut self) -> Vec<(InputEvent, Option<PhysicalInput>)> {
        std::mem::take(&mut self.pending)
    }

    /// Reuse the drained queue's allocation and preserve newly queued edges.
    #[inline]
    pub fn restore_pending(&mut self, mut pending: Vec<(InputEvent, Option<PhysicalInput>)>) {
        pending.append(&mut self.pending);
        self.pending = pending;
    }

    /// Record an edge before dispatching it, rejecting a duplicate menu alias.
    ///
    /// Alias matching intentionally follows the previous edge's timestamp,
    /// source, slot and action without adding a pressed/released condition.
    #[inline]
    #[must_use]
    pub fn accept(&mut self, event: InputEvent, physical: Option<PhysicalInput>) -> bool {
        if let Some(previous) = self.last_input
            && previous.timestamp == event.timestamp
            && previous.input_slot == event.input_slot
            && previous.source == event.source
            && event.action.primary_from_menu_alias() == Some(previous.action)
        {
            return false;
        }
        self.last_input = Some(event);
        self.held.0.retain(|(held, _, _)| {
            held.source != event.source
                || held.input_slot != event.input_slot
                || held.action != event.action
        });
        if event.pressed {
            self.held.0.push((event, physical, 0.0));
        }
        true
    }

    /// Append at most one repeat per held input, preserving held-input order.
    ///
    /// The caller owns and can reuse the output storage. Complete this snapshot
    /// before invoking callbacks; the original physical edge is retained so its
    /// timestamps and device identity are unchanged.
    #[inline]
    pub fn append_repeats(
        &mut self,
        delta_time: f32,
        output: &mut Vec<(InputEvent, Option<PhysicalInput>)>,
    ) {
        for (event, physical, held) in &mut self.held.0 {
            let old = *held;
            *held += delta_time.max(0.0);
            // Keep the strict delay and original bucket arithmetic, including
            // the frame that starts exactly on the delay boundary.
            if *held > REPEAT_DELAY_SECONDS
                && (old < REPEAT_DELAY_SECONDS
                    || ((old - REPEAT_DELAY_SECONDS) / REPEAT_INTERVAL_SECONDS) as u32
                        != ((*held - REPEAT_DELAY_SECONDS) / REPEAT_INTERVAL_SECONDS) as u32)
            {
                output.push((*event, *physical));
            }
        }
    }

    #[inline]
    pub fn take_held(&mut self) -> HeldCallbackInputs {
        std::mem::take(&mut self.held)
    }

    #[inline]
    pub fn restore_held(&mut self, held: HeldCallbackInputs) {
        self.held = held;
    }

    /// Drop inputs released while a replacement callback screen was prepared.
    #[inline]
    pub fn reconcile_held(&mut self, mut is_down: impl FnMut(&InputEvent) -> bool) {
        self.held.0.retain(|(event, _, _)| is_down(event));
    }

    /// Clear pending edges and detach an owned batch of synthetic releases.
    ///
    /// This consumes the existing held allocation without allocating another
    /// batch. Dispatch the releases, then call [Self::reset_alias_history] to
    /// preserve the original focus-loss ordering. The iterator does not borrow
    /// this state, so dispatch may enqueue or accept new input.
    #[inline]
    pub fn take_releases(
        &mut self,
    ) -> impl ExactSizeIterator<Item = (InputEvent, Option<PhysicalInput>)> + use<> {
        self.pending.clear();
        std::mem::take(&mut self.held.0)
            .into_iter()
            .map(|(mut event, physical, _)| {
                event.pressed = false;
                (event, physical)
            })
    }

    #[inline]
    pub fn reset_alias_history(&mut self) {
        self.last_input = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VirtualAction;
    use deadlib_platform::input::KeyCode;
    use deadsync_core::input::InputSource;
    use std::time::{Duration, Instant};

    fn edge(action: VirtualAction, input_slot: u32, source: InputSource) -> InputEvent {
        let timestamp = Instant::now();
        InputEvent::new(
            action,
            input_slot,
            true,
            source,
            timestamp,
            1_234,
            timestamp + Duration::from_millis(1),
            timestamp + Duration::from_millis(2),
        )
    }

    fn assert_edge_eq(actual: InputEvent, expected: InputEvent) {
        assert_eq!(actual.action, expected.action);
        assert_eq!(actual.input_slot, expected.input_slot);
        assert_eq!(actual.pressed, expected.pressed);
        assert_eq!(actual.source, expected.source);
        assert_eq!(actual.timestamp, expected.timestamp);
        assert_eq!(actual.timestamp_host_nanos, expected.timestamp_host_nanos);
        assert_eq!(actual.stored_at, expected.stored_at);
        assert_eq!(actual.emitted_at, expected.emitted_at);
    }

    fn assert_edges(actual: &[CallbackInput], expected: &[InputEvent]) {
        assert_eq!(actual.len(), expected.len());
        for ((event, _), expected) in actual.iter().zip(expected) {
            assert_edge_eq(*event, *expected);
        }
    }

    #[test]
    fn pending_edges_keep_order_and_reuse_drained_storage() {
        let first = edge(VirtualAction::p1_left, 1, InputSource::Keyboard);
        let second = edge(VirtualAction::p1_right, 2, InputSource::Keyboard);
        let later = edge(VirtualAction::p1_up, 3, InputSource::Keyboard);
        let mut state = CallbackInputState::default();
        state.queue([
            (first, Some(PhysicalInput::Key(KeyCode::ArrowLeft))),
            (second, None),
        ]);
        let mut pending = state.take_pending();
        let allocation = pending.as_ptr();
        let drained: Vec<_> = pending.drain(..).collect();
        assert_edges(&drained, &[first, second]);
        assert!(matches!(
            drained[0].1,
            Some(PhysicalInput::Key(KeyCode::ArrowLeft))
        ));

        state.queue([(later, None)]);
        state.restore_pending(pending);
        let pending = state.take_pending();
        assert_eq!(pending.as_ptr(), allocation);
        assert_edges(&pending, &[later]);
    }

    #[test]
    fn repeats_keep_strict_delay_and_bucket_boundaries() {
        let input = edge(VirtualAction::p1_left, 1, InputSource::Keyboard);
        let mut state = CallbackInputState::default();
        assert!(state.accept(input, None));
        let mut repeats = Vec::new();
        for delta in [0.25, 0.125, 0.0625] {
            state.append_repeats(delta, &mut repeats);
            assert!(repeats.is_empty());
        }
        state.append_repeats(0.0625, &mut repeats);
        assert_edges(&repeats, &[input]);
        repeats.clear();
        state.append_repeats(0.0625, &mut repeats);
        assert!(repeats.is_empty());
        state.append_repeats(0.0625, &mut repeats);
        assert_edges(&repeats, &[input]);
    }

    #[test]
    fn crossing_delay_from_below_emits_a_repeat() {
        let input = edge(VirtualAction::p1_left, 1, InputSource::Keyboard);
        let mut state = CallbackInputState::default();
        assert!(state.accept(input, None));
        let mut repeats = Vec::new();
        state.append_repeats(0.25, &mut repeats);
        assert!(repeats.is_empty());
        state.append_repeats(0.1875, &mut repeats);
        assert_edges(&repeats, &[input]);
    }

    #[test]
    fn long_frames_emit_one_repeat_per_input_and_keep_original_edges() {
        let first = edge(VirtualAction::p1_left, 1, InputSource::Keyboard);
        let second = edge(VirtualAction::p2_right, 2, InputSource::Gamepad);
        let mut state = CallbackInputState::default();
        assert!(state.accept(first, Some(PhysicalInput::Key(KeyCode::ArrowLeft))));
        assert!(state.accept(second, None));
        let mut repeats = Vec::new();
        state.append_repeats(2.0, &mut repeats);
        assert_edges(&repeats, &[first, second]);
        assert!(matches!(
            repeats[0].1,
            Some(PhysicalInput::Key(KeyCode::ArrowLeft))
        ));
        repeats.clear();
        state.append_repeats(2.0, &mut repeats);
        assert_edges(&repeats, &[first, second]);
    }

    #[test]
    fn nonpositive_or_nan_delta_does_not_advance_repeat_age() {
        let input = edge(VirtualAction::p1_left, 1, InputSource::Keyboard);
        let mut state = CallbackInputState::default();
        assert!(state.accept(input, None));
        let mut repeats = Vec::new();
        state.append_repeats(0.25, &mut repeats);
        for delta in [0.0, -1.0, f32::NAN] {
            state.append_repeats(delta, &mut repeats);
            assert!(repeats.is_empty());
        }
        state.append_repeats(0.1875, &mut repeats);
        assert_edges(&repeats, &[input]);
    }

    #[test]
    fn identities_and_represses_preserve_order_and_reset_only_matching_age() {
        let first = edge(VirtualAction::p1_left, 1, InputSource::Keyboard);
        let other_slot = InputEvent {
            input_slot: 2,
            ..first
        };
        let other_source = InputEvent {
            source: InputSource::Gamepad,
            ..first
        };
        let other_action = InputEvent {
            action: VirtualAction::p1_right,
            ..first
        };
        let mut state = CallbackInputState::default();
        for input in [first, other_slot, other_source, other_action] {
            assert!(state.accept(input, None));
        }
        let mut repeats = Vec::new();
        state.append_repeats(0.25, &mut repeats);
        let repressed = InputEvent {
            timestamp: first.timestamp + Duration::from_millis(10),
            ..first
        };
        assert!(state.accept(repressed, None));
        state.append_repeats(0.1875, &mut repeats);
        assert_edges(&repeats, &[other_slot, other_source, other_action]);
        repeats.clear();
        state.append_repeats(0.25, &mut repeats);
        assert_edges(
            &repeats,
            &[other_slot, other_source, other_action, repressed],
        );

        assert!(state.accept(
            InputEvent {
                pressed: false,
                ..repressed
            },
            None
        ));
        repeats.clear();
        state.append_repeats(0.125, &mut repeats);
        assert_edges(&repeats, &[other_slot, other_source, other_action]);
    }

    #[test]
    fn aliases_match_timestamp_slot_source_and_immediately_previous_primary() {
        let primary = edge(VirtualAction::p1_left, 1, InputSource::Keyboard);
        let alias = InputEvent {
            action: VirtualAction::p1_menu_left,
            pressed: false,
            ..primary
        };
        for changed in [
            InputEvent {
                timestamp: alias.timestamp + Duration::from_nanos(1),
                ..alias
            },
            InputEvent {
                input_slot: 2,
                ..alias
            },
            InputEvent {
                source: InputSource::Gamepad,
                ..alias
            },
            InputEvent {
                action: VirtualAction::p1_menu_right,
                ..alias
            },
        ] {
            let mut state = CallbackInputState::default();
            assert!(state.accept(primary, None));
            assert!(state.accept(changed, None));
        }
        let mut state = CallbackInputState::default();
        assert!(state.accept(primary, None));
        assert!(state.accept(
            InputEvent {
                action: VirtualAction::p1_right,
                ..primary
            },
            None
        ));
        assert!(state.accept(alias, None));
    }

    #[test]
    fn ignored_aliases_preserve_history_and_do_not_require_a_release_flag() {
        let primary = edge(VirtualAction::p1_left, 1, InputSource::Keyboard);
        let alias = InputEvent {
            action: VirtualAction::p1_menu_left,
            ..primary
        };
        let mut state = CallbackInputState::default();
        assert!(state.accept(primary, None));
        assert!(!state.accept(alias, Some(PhysicalInput::Key(KeyCode::ArrowLeft))));
        assert!(!state.accept(
            InputEvent {
                pressed: false,
                ..alias
            },
            None
        ));
        let mut repeats = Vec::new();
        state.append_repeats(0.5, &mut repeats);
        assert_edges(&repeats, &[primary]);
    }

    #[test]
    fn held_transfer_preserves_repeat_age_without_transferring_alias_history() {
        let primary = edge(VirtualAction::p1_left, 1, InputSource::Keyboard);
        let mut original = CallbackInputState::default();
        assert!(original.accept(primary, None));
        let mut repeats = Vec::new();
        original.append_repeats(0.25, &mut repeats);
        let held = original.take_held();
        original.append_repeats(0.5, &mut repeats);
        assert!(repeats.is_empty());

        let mut replacement = CallbackInputState::default();
        replacement.restore_held(held);
        replacement.append_repeats(0.1875, &mut repeats);
        assert_edges(&repeats, &[primary]);
        let alias = InputEvent {
            action: VirtualAction::p1_menu_left,
            pressed: false,
            ..primary
        };
        assert!(replacement.accept(alias, None));
        assert!(!original.accept(alias, None));
    }

    #[test]
    fn reconciliation_drops_only_inputs_no_longer_down() {
        let first = edge(VirtualAction::p1_left, 1, InputSource::Keyboard);
        let second = edge(VirtualAction::p1_right, 2, InputSource::Keyboard);
        let mut state = CallbackInputState::default();
        assert!(state.accept(first, None));
        assert!(state.accept(second, None));
        let mut repeats = Vec::new();
        state.append_repeats(0.25, &mut repeats);
        let mut checked = Vec::new();
        state.reconcile_held(|event| {
            checked.push(event.input_slot);
            event.input_slot == 2
        });
        assert_eq!(checked, vec![1, 2]);
        state.append_repeats(0.1875, &mut repeats);
        assert_edges(&repeats, &[second]);
    }

    #[test]
    fn focus_loss_detaches_release_batch_before_alias_history_is_reset() {
        let first = edge(VirtualAction::p1_left, 1, InputSource::Keyboard);
        let second = edge(VirtualAction::p1_right, 2, InputSource::Keyboard);
        let mut state = CallbackInputState::default();
        assert!(state.accept(first, Some(PhysicalInput::Key(KeyCode::ArrowLeft))));
        assert!(state.accept(second, None));
        state.queue([(first, None)]);
        let mut releases = state.take_releases();
        assert_eq!(releases.len(), 2);
        assert!(state.take_pending().is_empty());
        let alias = InputEvent {
            action: VirtualAction::p1_menu_right,
            pressed: false,
            ..second
        };
        assert!(!state.accept(alias, None));
        let (released, physical) = releases.next().unwrap();
        assert_edge_eq(
            released,
            InputEvent {
                pressed: false,
                ..first
            },
        );
        assert!(matches!(
            physical,
            Some(PhysicalInput::Key(KeyCode::ArrowLeft))
        ));
        assert_edge_eq(
            releases.next().unwrap().0,
            InputEvent {
                pressed: false,
                ..second
            },
        );
        assert!(releases.next().is_none());
        state.reset_alias_history();
        assert!(state.accept(alias, None));
        let mut repeats = Vec::new();
        state.append_repeats(0.5, &mut repeats);
        assert!(repeats.is_empty());
    }

    #[test]
    fn repeat_batches_append_and_stay_stable_during_later_state_changes() {
        let first = edge(VirtualAction::p1_left, 1, InputSource::Keyboard);
        let second = edge(VirtualAction::p1_right, 2, InputSource::Keyboard);
        let mut state = CallbackInputState::default();
        assert!(state.accept(first, None));
        let mut repeats = vec![(second, None)];
        state.append_repeats(0.5, &mut repeats);
        assert!(state.accept(
            InputEvent {
                pressed: false,
                ..first
            },
            None
        ));
        assert!(state.accept(second, None));
        assert_edges(&repeats, &[second, first]);
    }
}
