use smallvec::SmallVec;
use std::time::{Duration, Instant};

const GENERIC_AXIS_FIRST: u16 = 0x30;
const GENERIC_AXIS_LAST: u16 = 0x38;
const GENERIC_AXIS_COUNT: usize = (GENERIC_AXIS_LAST - GENERIC_AXIS_FIRST + 1) as usize;

#[derive(Clone, Copy)]
struct AxisState {
    code: u32,
    value: i64,
}

#[derive(Default)]
pub struct AxisCache {
    generic_values: [i64; GENERIC_AXIS_COUNT],
    generic_seen: u16,
    other: SmallVec<[AxisState; 8]>,
}

impl AxisCache {
    #[inline(always)]
    fn changed(&mut self, usage_page: u16, usage: u16, code: u32, value: i64) -> bool {
        if usage_page == 0x01 && (GENERIC_AXIS_FIRST..=GENERIC_AXIS_LAST).contains(&usage) {
            let index = (usage - GENERIC_AXIS_FIRST) as usize;
            let bit = 1 << index;
            if self.generic_seen & bit != 0 && self.generic_values[index] == value {
                return false;
            }
            self.generic_seen |= bit;
            self.generic_values[index] = value;
            return true;
        }
        for axis in &mut self.other {
            if axis.code != code {
                continue;
            }
            if axis.value == value {
                return false;
            }
            axis.value = value;
            return true;
        }
        self.other.push(AxisState { code, value });
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadValueKind {
    Directions([bool; 4]),
    Button,
    Axis,
}

#[inline(always)]
pub fn classify_pad_value(
    last_axis: &mut AxisCache,
    usage_page: u16,
    usage: u16,
    code: u32,
    value: i64,
) -> Option<PadValueKind> {
    if usage_page == 0x01 && usage == 0x39 {
        let hat = value as u32;
        return Some(PadValueKind::Directions([
            matches!(hat, 0 | 1 | 7),
            matches!(hat, 3..=5),
            matches!(hat, 5..=7),
            matches!(hat, 1..=3),
        ]));
    }
    if usage_page == 0x09 {
        return Some(PadValueKind::Button);
    }
    last_axis
        .changed(usage_page, usage, code, value)
        .then_some(PadValueKind::Axis)
}

#[derive(Clone, Copy, Debug)]
pub struct HostInstantMap {
    instant: Instant,
    host_nanos: u64,
}

impl HostInstantMap {
    #[inline(always)]
    pub const fn new(instant: Instant, host_nanos: u64) -> Self {
        Self {
            instant,
            host_nanos,
        }
    }

    #[inline(always)]
    pub fn instant(self, target_host_nanos: u64) -> Instant {
        if target_host_nanos >= self.host_nanos {
            self.instant
                .checked_add(Duration::from_nanos(target_host_nanos - self.host_nanos))
                .unwrap_or(self.instant)
        } else {
            self.instant
                .checked_sub(Duration::from_nanos(self.host_nanos - target_host_nanos))
                .unwrap_or(self.instant)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hats_and_buttons_preserve_repeated_raw_values() {
        let mut axes = AxisCache::default();

        for _ in 0..2 {
            assert_eq!(
                classify_pad_value(&mut axes, 0x01, 0x39, 0x0001_0039, 7),
                Some(PadValueKind::Directions([true, false, true, false]))
            );
            assert_eq!(
                classify_pad_value(&mut axes, 0x09, 3, 0x0009_0003, 1),
                Some(PadValueKind::Button)
            );
        }
        assert_eq!(axes.generic_seen, 0);
        assert!(axes.other.is_empty());
    }

    #[test]
    fn axes_emit_first_and_changed_values_per_usage_code() {
        let mut axes = AxisCache::default();

        assert_eq!(
            classify_pad_value(&mut axes, 0x01, 0x30, 0x0001_0030, 12),
            Some(PadValueKind::Axis)
        );
        assert_eq!(
            classify_pad_value(&mut axes, 0x01, 0x30, 0x0001_0030, 12),
            None
        );
        assert_eq!(
            classify_pad_value(&mut axes, 0x01, 0x31, 0x0001_0031, 12),
            Some(PadValueKind::Axis)
        );
        assert_eq!(
            classify_pad_value(&mut axes, 0x01, 0x30, 0x0001_0030, -4),
            Some(PadValueKind::Axis)
        );
    }

    #[test]
    fn nonstandard_axes_preserve_exact_code_identity_past_inline_storage() {
        let mut axes = AxisCache::default();
        for usage in 0..10 {
            let code = 0x0020_0000 | usage;
            assert_eq!(
                classify_pad_value(&mut axes, 0x20, usage as u16, code, i64::from(usage)),
                Some(PadValueKind::Axis)
            );
            assert_eq!(
                classify_pad_value(&mut axes, 0x20, usage as u16, code, i64::from(usage)),
                None
            );
        }
        assert_eq!(axes.other.len(), 10);
    }

    #[test]
    fn host_instant_map_handles_earlier_and_later_events() {
        let instant = Instant::now();
        let map = HostInstantMap::new(instant, 5_000_000);

        assert_eq!(map.instant(4_250_000), instant - Duration::from_micros(750));
        assert_eq!(map.instant(5_800_000), instant + Duration::from_micros(800));
        assert_eq!(map.instant(5_000_000), instant);
    }
}
