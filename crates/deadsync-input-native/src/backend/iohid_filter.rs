#[derive(Clone, Copy)]
pub(super) struct AxisState {
    code: u32,
    value: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PadValueKind {
    Directions([bool; 4]),
    Button,
    Axis,
}

#[inline(always)]
pub(super) fn classify_pad_value(
    last_axis: &mut Vec<AxisState>,
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
    for axis in last_axis.iter_mut() {
        if axis.code != code {
            continue;
        }
        if axis.value == value {
            return None;
        }
        axis.value = value;
        return Some(PadValueKind::Axis);
    }
    last_axis.push(AxisState { code, value });
    Some(PadValueKind::Axis)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hats_and_buttons_preserve_repeated_raw_values() {
        let mut axes = Vec::new();

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
        assert!(axes.is_empty());
    }

    #[test]
    fn axes_emit_first_and_changed_values_per_usage_code() {
        let mut axes = Vec::new();

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
}
