#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ChoiceViewport {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) has_previous: bool,
    pub(super) has_next: bool,
}

impl ChoiceViewport {
    pub(super) const fn contains(self, choice_idx: usize) -> bool {
        choice_idx >= self.start && choice_idx < self.end
    }
}

pub(super) const SCROLL_HINT_WIDTH: f32 = 12.0;
pub(super) const SCROLL_HINT_GAP: f32 = 4.0;
pub(super) const SCROLL_HINT_SLOT: f32 = SCROLL_HINT_WIDTH + SCROLL_HINT_GAP;

pub(super) fn choice_viewport(
    offsets: &[f32],
    widths: &[f32],
    requested_start: usize,
    available_width: f32,
    hint_slot: f32,
) -> ChoiceViewport {
    let count = widths.len();
    if count == 0 || offsets.len() != count {
        return ChoiceViewport {
            start: 0,
            end: count,
            has_previous: false,
            has_next: false,
        };
    }

    let total_width = offsets[count - 1] + widths[count - 1];
    if total_width <= available_width {
        return ChoiceViewport {
            start: 0,
            end: count,
            has_previous: false,
            has_next: false,
        };
    }

    let start = requested_start.min(count - 1);
    let has_previous = start > 0;
    let left_slot = if has_previous { hint_slot } else { 0.0 };
    let mut end = start;
    for idx in start..count {
        let content_width = offsets[idx] - offsets[start] + widths[idx];
        let right_slot = if idx + 1 < count { hint_slot } else { 0.0 };
        if left_slot + content_width + right_slot > available_width {
            break;
        }
        end = idx + 1;
    }
    if end == start {
        end += 1;
    }
    ChoiceViewport {
        start,
        end,
        has_previous,
        has_next: end < count,
    }
}

pub(super) fn choice_start_for_focus(
    offsets: &[f32],
    widths: &[f32],
    requested_start: usize,
    focus_idx: usize,
    available_width: f32,
    hint_slot: f32,
) -> usize {
    let count = widths.len();
    if count == 0 || offsets.len() != count {
        return 0;
    }
    let focus_idx = focus_idx.min(count - 1);
    let mut start = requested_start.min(count - 1);
    let viewport = choice_viewport(offsets, widths, start, available_width, hint_slot);
    if !viewport.has_previous && !viewport.has_next {
        return 0;
    }
    if focus_idx < start {
        start = focus_idx;
    }
    while focus_idx >= choice_viewport(offsets, widths, start, available_width, hint_slot).end
        && start < focus_idx
    {
        start += 1;
    }
    start
}

pub(super) fn choice_position_for_view(
    offsets: &[f32],
    viewport: ChoiceViewport,
    choice_idx: usize,
    hint_slot: f32,
) -> f32 {
    let hint_offset = if viewport.has_previous {
        hint_slot
    } else {
        0.0
    };
    hint_offset + offsets[choice_idx] - offsets[viewport.start]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_fits_when_possible_and_reserves_hint_slots_when_scrolling() {
        let offsets = [0.0, 30.0, 60.0, 90.0];
        let widths = [20.0; 4];

        assert_eq!(
            choice_viewport(&offsets, &widths, 0, 109.0, 16.0),
            ChoiceViewport {
                start: 0,
                end: 3,
                has_previous: false,
                has_next: true,
            }
        );
        assert_eq!(
            choice_viewport(&offsets, &widths, 2, 80.0, 16.0),
            ChoiceViewport {
                start: 2,
                end: 4,
                has_previous: true,
                has_next: false,
            }
        );
        assert_eq!(
            choice_viewport(&offsets, &widths, 3, 200.0, 16.0),
            ChoiceViewport {
                start: 0,
                end: 4,
                has_previous: false,
                has_next: false,
            }
        );
    }

    #[test]
    fn focus_window_moves_until_the_choice_is_visible() {
        let offsets = [0.0, 30.0, 60.0, 90.0, 120.0];
        let widths = [20.0; 5];
        let start = choice_start_for_focus(&offsets, &widths, 0, 4, 80.0, 16.0);
        let viewport = choice_viewport(&offsets, &widths, start, 80.0, 16.0);

        assert!(viewport.contains(4));
        assert!(viewport.has_previous);
        assert!(!viewport.has_next);
    }
}
