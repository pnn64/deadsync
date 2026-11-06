//! Keep larger serial sensor configurations legible without changing navigation.
use super::*;
use std::ops::Range;

type Groups = SmallVec<[(usize, Range<usize>); 16]>;

pub(super) fn simple_page(state: &State) -> (Groups, usize, usize) {
    const PER_PAGE: usize = 8;
    let total = state
        .pads
        .iter()
        .map(|pad| pad.buttons.len())
        .sum::<usize>();
    let selected = selected_slot(state).map_or(0, |slot| {
        state.pads[..slot.pad]
            .iter()
            .map(|pad| pad.buttons.len())
            .sum::<usize>()
            + slot.button
    });
    let page = selected / PER_PAGE;
    let start = page * PER_PAGE;
    let end = (start + PER_PAGE).min(total);
    let mut base = 0;
    let mut visible = Groups::new();
    for (index, pad) in state.pads.iter().enumerate() {
        let count = pad.buttons.len();
        if base < end && base + count > start {
            visible.push((index, start.saturating_sub(base)..(end - base).min(count)));
        }
        base += count;
    }
    (visible, page, total.div_ceil(PER_PAGE).max(1))
}

pub(super) fn advanced_page(
    pad: &PadView,
    focused: Option<AdvTarget>,
    available: f32,
) -> (Groups, usize, usize) {
    let available = available.max(ADV_BAR_W);
    let capacity = ((available + ADV_BAR_GAP) / (ADV_BAR_W + ADV_BAR_GAP))
        .floor()
        .max(1.0) as usize;
    let mut pages: SmallVec<[Groups; 1]> = smallvec::smallvec![Groups::new()];
    let mut width = 0.0;
    let mut selected_page = 0;
    for (button_index, button) in pad.buttons.iter().enumerate() {
        for start in (0..button.sensors.len()).step_by(capacity) {
            let end = (start + capacity).min(button.sensors.len());
            let group_w = group_width(end - start);
            // Keep enough space for group labels as well as the sensor bars.
            let gap = if pages.last().unwrap().is_empty() {
                0.0
            } else {
                ADV_GROUP_GAP
            };
            if width + gap + group_w > available && width > 0.0 {
                pages.push(Groups::new());
                width = 0.0;
            }
            if focused.is_some_and(|focus| matches!(focus, AdvTarget::Sensor { button, sensor } if button == button_index && (start..end).contains(&sensor))) {
                selected_page = pages.len() - 1;
            }
            let page = pages.last_mut().unwrap();
            width += if page.is_empty() {
                group_w
            } else {
                ADV_GROUP_GAP + group_w
            };
            page.push((button_index, start..end));
        }
    }
    let count = pages.len();
    (pages.swap_remove(selected_page), selected_page, count)
}

pub(super) fn push_page_indicator(
    actors: &mut Vec<Actor>,
    page: usize,
    pages: usize,
    y: f32,
    zb: f32,
) {
    if pages <= 1 {
        return;
    }
    actors.push(act!(text:
        font("miso"): settext(TextContent::inline_format(format_args!("{} / {}", page + 1, pages)).unwrap()):
        align(0.5, 0.5): xy(screen_center_x(), y): zoom(0.55):
        diffuse(1.0, 1.0, 1.0, 0.8): z(24.0 + zb)
    ));
}

pub(super) fn push_status(actors: &mut Vec<Actor>, pad: &PadView, x: f32, y: f32, zb: f32) {
    if let Some(status) = &pad.status {
        actors.push(act!(text:
            font("miso"): settext(status.clone()): align(0.5, 0.5): xy(x, y): zoom(0.5):
            diffuse(1.0, 0.7, 0.3, 1.0): z(24.0 + zb)
        ));
    }
}
