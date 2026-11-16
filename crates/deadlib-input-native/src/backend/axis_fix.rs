use super::emit_dir_edges;
use deadlib_platform::input::{PadEvent, PadId};
use std::time::Instant;

#[derive(Clone, Copy, Debug)]
pub(super) struct AxisRange {
    minimum: f32,
    maximum: f32,
}

impl AxisRange {
    pub(super) fn new(minimum: i32, maximum: i32) -> Option<Self> {
        let minimum = minimum as f32;
        let maximum = maximum as f32;
        (minimum < maximum).then_some(Self { minimum, maximum })
    }

    /// Match ITGmania's LinuxEvent AxisFix, including its distinct 127/128
    /// states for 0..255 adapters. Normalize before testing the midpoint.
    /// Source: itgmania/itgmania 8a3bf8884acdcb69363f7f0ea592138e398c71e4,
    /// src/arch/InputHandler/InputHandler_Linux_Event.cpp:508-525 and
    /// the multiply-before-divide SCALE macro in src/RageUtil.h.
    #[inline(always)]
    pub(super) fn directions(self, value: i32) -> [bool; 2] {
        let level = (value as f32 - self.minimum) * 2.0 / (self.maximum - self.minimum) - 1.0;
        let both = f64::from(level) > 0.0001 && level < 0.5;
        [level < -0.5 || both, level > 0.5 || both]
    }
}

/// Hats and corrected X/Y axes can describe the same direction. Retain both
/// sources so one returning to neutral cannot release the other held source.
#[derive(Default)]
pub(super) struct DirectionState {
    hat: [bool; 4],
    axis: [bool; 4],
    emitted: [bool; 4],
}

impl DirectionState {
    #[inline(always)]
    pub(super) fn set_hat(&mut self, horizontal: bool, value: i32) {
        let first = if horizontal { 2 } else { 0 };
        self.hat[first] = value < 0;
        self.hat[first + 1] = value > 0;
    }

    #[inline(always)]
    pub(super) fn set_axis(&mut self, horizontal: bool, directions: [bool; 2]) {
        let first = if horizontal { 2 } else { 0 };
        self.axis[first] = directions[0];
        self.axis[first + 1] = directions[1];
    }

    #[inline(always)]
    pub(super) fn emit(
        &mut self,
        emit_pad: &mut impl FnMut(PadEvent),
        id: PadId,
        timestamp: Instant,
        host_nanos: u64,
    ) {
        let want = std::array::from_fn(|i| self.hat[i] || self.axis[i]);
        emit_dir_edges(emit_pad, id, &mut self.emitted, timestamp, host_nanos, want);
    }

    pub(super) fn is_pressed(&self) -> bool {
        self.emitted.iter().any(|&pressed| pressed)
    }

    pub(super) fn release(
        &mut self,
        emit_pad: &mut impl FnMut(PadEvent),
        id: PadId,
        timestamp: Instant,
        host_nanos: u64,
    ) {
        self.hat = [false; 4];
        self.axis = [false; 4];
        self.emit(emit_pad, id, timestamp, host_nanos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_platform::input::PadDir;

    #[test]
    fn psx_adapter_preserves_neutral_and_both_opposites() {
        let range = AxisRange::new(0, 255).unwrap();
        for (value, expected) in [
            (0, [true, false]),
            (127, [false, false]),
            (128, [true, true]),
            (255, [false, true]),
        ] {
            assert_eq!(range.directions(value), expected, "value {value}");
        }
    }

    #[test]
    fn whole_unsigned_byte_range_matches_itgmania_regions() {
        let range = AxisRange::new(0, 255).unwrap();
        for (values, expected) in [
            (0..=63, [true, false]),
            (64..=127, [false, false]),
            (128..=191, [true, true]),
            (192..=255, [false, true]),
        ] {
            for value in values {
                assert_eq!(range.directions(value), expected, "value {value}");
            }
        }
    }

    #[test]
    fn device_range_controls_normalization_and_thresholds_are_strict() {
        for (minimum, maximum) in [(0, 255), (-128, 127), (1_000, 1_255)] {
            let range = AxisRange::new(minimum, maximum).unwrap();
            for (offset, expected) in [
                (0, [true, false]),
                (127, [false, false]),
                (128, [true, true]),
                (255, [false, true]),
            ] {
                assert_eq!(range.directions(minimum + offset), expected);
            }
        }
        let range = AxisRange::new(-100, 100).unwrap();
        assert_eq!(range.directions(-51), [true, false]);
        assert_eq!(range.directions(-50), [false, false]);
        assert_eq!(range.directions(0), [false, false]);
        assert_eq!(range.directions(1), [true, true]);
        assert_eq!(range.directions(49), [true, true]);
        assert_eq!(range.directions(50), [false, false]);
        assert_eq!(range.directions(51), [false, true]);
    }

    #[test]
    fn malformed_or_float_collapsed_ranges_are_rejected() {
        for (minimum, maximum) in [(0, 0), (255, 0), (i32::MAX - 1, i32::MAX)] {
            assert!(AxisRange::new(minimum, maximum).is_none());
        }
    }

    #[test]
    fn overlapping_sources_emit_only_real_edges_and_preserve_event_time() {
        let mut state = DirectionState::default();
        let timestamp = Instant::now();
        let mut edges = Vec::new();
        let mut emit = |event| match event {
            PadEvent::Dir {
                id,
                timestamp: at,
                host_nanos,
                dir,
                pressed,
            } => {
                assert_eq!(id, PadId(7));
                assert_eq!(at, timestamp);
                assert_eq!(host_nanos, 42);
                edges.push((dir, pressed));
            }
            other => panic!("unexpected event: {other:?}"),
        };

        state.set_hat(true, -1);
        state.emit(&mut emit, PadId(7), timestamp, 42);
        state.set_axis(true, [true, true]);
        state.emit(&mut emit, PadId(7), timestamp, 42);
        state.set_axis(false, [true, false]);
        state.emit(&mut emit, PadId(7), timestamp, 42);
        state.set_hat(true, 0);
        state.emit(&mut emit, PadId(7), timestamp, 42);
        state.set_axis(true, [true, true]);
        state.emit(&mut emit, PadId(7), timestamp, 42);
        assert!(state.is_pressed());
        state.release(&mut emit, PadId(7), timestamp, 42);
        state.release(&mut emit, PadId(7), timestamp, 42);
        assert!(!state.is_pressed());
        assert_eq!(
            edges,
            vec![
                (PadDir::Left, true),
                (PadDir::Right, true),
                (PadDir::Up, true),
                (PadDir::Up, false),
                (PadDir::Left, false),
                (PadDir::Right, false),
            ]
        );
    }

    #[test]
    fn axis_release_keeps_a_held_hat_and_does_not_affect_other_axis() {
        let mut state = DirectionState::default();
        let timestamp = Instant::now();
        let mut edges = Vec::new();
        let mut emit = |event| {
            let PadEvent::Dir { dir, pressed, .. } = event else {
                panic!("unexpected non-direction event");
            };
            edges.push((dir, pressed));
        };
        state.set_axis(true, [true, true]);
        state.emit(&mut emit, PadId(0), timestamp, 0);
        state.set_hat(true, -1);
        state.set_axis(false, [false, true]);
        state.emit(&mut emit, PadId(0), timestamp, 0);
        state.set_axis(true, [false, false]);
        state.emit(&mut emit, PadId(0), timestamp, 0);
        state.set_hat(true, 0);
        state.emit(&mut emit, PadId(0), timestamp, 0);
        assert_eq!(
            edges,
            vec![
                (PadDir::Left, true),
                (PadDir::Right, true),
                (PadDir::Down, true),
                (PadDir::Right, false),
                (PadDir::Left, false),
            ]
        );
        assert!(state.is_pressed());
    }
}
