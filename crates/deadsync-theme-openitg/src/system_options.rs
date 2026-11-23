use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::space::{screen_height, screen_width};
use deadsync_theme::views::{SystemOptionsPage, SystemOptionsRenderer, SystemOptionsRow};

use crate::presentation::{ACCENT, INK, Layout, quad};

pub const SYSTEM_OPTIONS: SystemOptionsRenderer<Actor> = SystemOptionsRenderer {
    background: push_background,
    page: push_page,
};

const ROWS_SHOWN: usize = 8;
const FIRST_ROW_Y: f32 = 127.0;
const ROW_PITCH: f32 = 28.0;
const DONE_Y: f32 = 353.0;
const LABEL_X: f32 = 53.0;
const VALUE_X: f32 = 160.0;
const VALUE_END_X: f32 = 595.0;

const WINE: [f32; 4] = [0.109, 0.026, 0.040, 1.0];
const STROKE: [f32; 4] = [0.39, 0.15, 0.16, 1.0];
const ROW: [f32; 4] = [0.17, 0.046, 0.063, 1.0];
const FOCUSED_ROW: [f32; 4] = [0.39, 0.218, 0.101, 1.0];
const MUTED: [f32; 4] = [0.76, 0.66, 0.67, 1.0];
const DIM: [f32; 4] = [0.55, 0.46, 0.48, 1.0];
const AMBER: [f32; 4] = [ACCENT[0], ACCENT[1], ACCENT[2], 1.0];
const WHITE: [f32; 4] = [INK[0], INK[1], INK[2], 1.0];

fn push_background(actors: &mut Vec<Actor>) {
    let (width, height) = (screen_width(), screen_height());
    actors.push(quad(
        [0.0, 0.0],
        [width, height],
        [0.036, 0.015, 0.023, 1.0],
        -23,
    ));
}

fn push_panel(actors: &mut Vec<Actor>, layout: Layout) {
    actors.push(layout.panel(15.0, 90.0, [610.0, 336.0], WINE, -22));
    actors.push(layout.panel(15.0, 90.0, [610.0, 2.0], STROKE, -21));
    actors.push(layout.panel(15.0, 424.0, [610.0, 2.0], STROKE, -21));
    actors.push(layout.panel(15.0, 92.0, [2.0, 332.0], STROKE, -21));
    actors.push(layout.panel(623.0, 92.0, [2.0, 332.0], STROKE, -21));
    actors.push(layout.panel(39.0, 368.0, [562.0, 1.0], STROKE, -20));
}

fn push_page(actors: &mut Vec<Actor>, page: SystemOptionsPage<'_>) {
    let width = screen_width();
    let layout =
        Layout::centered(width, screen_height()).translated_x(page.slide.clamp(-1.0, 1.0) * width);
    push_panel(actors, layout);
    let (title, title_zoom) = if page.main_menu {
        ("SERVICE OPTIONS", 1.9)
    } else {
        (page.title, 1.55)
    };
    actors.push(layout.bounded_label([36.0, 38.0], 0.0, 568.0, title.into(), title_zoom, WHITE));

    let end_offset = page.row_count.saturating_sub(ROWS_SHOWN);
    let first = page.selected.saturating_sub(ROWS_SHOWN / 2).min(end_offset);
    let end = (first + ROWS_SHOWN).min(page.row_count);
    for index in first..end {
        if let Some(row) = (page.row)(index) {
            let y = (index - first) as f32 * ROW_PITCH + FIRST_ROW_Y;
            push_row(actors, layout, y, &row, index == page.selected);
        }
    }
    if end == page.row_count {
        let focused = page.selected == page.row_count;
        push_row_band(actors, layout, DONE_Y, focused);
        actors.push(layout.label(
            [320.0, DONE_Y],
            0.5,
            "DONE".into(),
            1.0,
            if focused { WHITE } else { MUTED },
        ));
    }
    if end_offset > 0 {
        let track_height = 240.0;
        let thumb_height = (track_height * ROWS_SHOWN as f32 / page.row_count as f32).max(16.0);
        let thumb_offset = (track_height - thumb_height) * first as f32 / end_offset as f32;
        let top = 122.0;
        actors.push(layout.panel(33.0, top, [2.0, track_height], STROKE, -15));
        actors.push(layout.panel(33.0, top + thumb_offset, [2.0, thumb_height], AMBER, -14));
    }
    if let Some(text) = page.description {
        let height = if page.detail.is_some() { 22.0 } else { 40.0 };
        actors.push(layout.paragraph([54.0, 381.0], [548.0, height], text.into(), 0.86, WHITE));
    }
    if let Some(text) = page.detail {
        actors.push(layout.paragraph([54.0, 405.0], [548.0, 19.0], text.into(), 0.86, MUTED));
    }
}

fn push_row_band(actors: &mut Vec<Actor>, layout: Layout, y: f32, focused: bool) {
    let fill = if focused { FOCUSED_ROW } else { ROW };
    actors.push(layout.panel(35.0, y - 12.0, [561.0, 24.0], fill, -17));
    if focused {
        actors.push(layout.panel(35.0, y - 12.0, [3.0, 24.0], AMBER, -16));
    }
}

fn push_row(
    actors: &mut Vec<Actor>,
    layout: Layout,
    y: f32,
    row: &SystemOptionsRow,
    focused: bool,
) {
    push_row_band(actors, layout, y, focused);
    let color = if row.disabled {
        DIM
    } else if focused {
        WHITE
    } else {
        MUTED
    };
    let choices = row.choices.as_deref().filter(|choices| !choices.is_empty());
    let (label_x, label_width) = if choices.is_some() {
        (LABEL_X, VALUE_X - LABEL_X - 13.0)
    } else {
        (VALUE_X, VALUE_END_X - VALUE_X)
    };
    actors.push(layout.bounded_label(
        [label_x, y],
        0.0,
        label_width,
        TextContent::Shared(row.label.clone()),
        0.97,
        color,
    ));
    if let Some(choices) = choices {
        let selected = row.selected_choice.min(choices.len() - 1);
        let value_limit = if row.disabled { 534.0 } else { VALUE_END_X };
        let cell_width = (value_limit - VALUE_X) / choices.len() as f32;
        let fits_inline = row.inline
            && choices.len() > 1
            && row.choice_tint.is_none()
            && choices
                .iter()
                .all(|choice| (choice.chars().count() as f32).mul_add(9.0, 12.0) <= cell_width);
        if fits_inline {
            for (index, choice) in choices.iter().enumerate() {
                let x = (index as f32).mul_add(cell_width, VALUE_X);
                let enabled = match row.enabled_choices {
                    Some(mask) => choice_enabled(mask, index),
                    None => index == selected,
                };
                let color = if row.disabled {
                    DIM
                } else if enabled {
                    WHITE
                } else {
                    MUTED
                };
                actors.push(layout.bounded_label(
                    [x, y],
                    0.0,
                    cell_width - 10.0,
                    TextContent::Shared(choice.clone()),
                    0.9,
                    color,
                ));
                if enabled || (focused && index == selected && !row.disabled) {
                    let mark = if focused && index == selected {
                        AMBER
                    } else {
                        STROKE
                    };
                    actors.push(layout.panel(x, y + 8.0, [15.0, 2.0], mark, -15));
                }
            }
        } else {
            let mut value_x = VALUE_X;
            if let Some(tint) = row.choice_tint {
                actors.push(layout.panel(VALUE_X, y - 6.0, [14.0, 12.0], tint, -15));
                value_x += 22.0;
            }
            actors.push(layout.bounded_label(
                [value_x, y],
                0.0,
                value_limit
                    - value_x
                    - if row.enabled_choices.is_some() {
                        56.0
                    } else {
                        14.0
                    },
                TextContent::Shared(choices[selected].clone()),
                0.97,
                color,
            ));
            if let Some(mask) = row.enabled_choices {
                let (status, tint) = if choice_enabled(mask, selected) {
                    ("ON", AMBER)
                } else {
                    ("OFF", MUTED)
                };
                actors.push(layout.label([value_limit - 16.0, y], 1.0, status.into(), 0.8, tint));
            }
            if focused && !row.disabled && choices.len() > 1 {
                actors.push(layout.label([VALUE_X - 10.0, y], 0.5, "<".into(), 0.8, AMBER));
                actors.push(layout.label([VALUE_END_X, y], 0.5, ">".into(), 0.8, AMBER));
            }
        }
    } else if focused && !row.disabled {
        actors.push(layout.label([VALUE_X - 17.0, y], 0.5, ">".into(), 0.8, AMBER));
    }
    if row.disabled {
        actors.push(layout.label([582.0, y], 1.0, "LOCKED".into(), 0.73, DIM));
    }
}

fn choice_enabled(mask: u64, index: usize) -> bool {
    index < u64::BITS as usize && (mask & (1_u64 << index)) != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::actors::SizeSpec;
    use deadlib_present::space::{Metrics, with_current_metrics};
    use std::cell::RefCell;
    use std::sync::Arc;

    fn action(label: &str) -> SystemOptionsRow {
        SystemOptionsRow {
            label: Arc::from(label),
            choices: None,
            selected_choice: 0,
            inline: false,
            disabled: false,
            enabled_choices: None,
            choice_tint: None,
        }
    }

    fn page<'a>(
        row: &'a dyn Fn(usize) -> Option<SystemOptionsRow>,
        row_count: usize,
        selected: usize,
    ) -> SystemOptionsPage<'a> {
        SystemOptionsPage {
            title: "OPTIONS",
            main_menu: true,
            selected,
            row_count,
            row,
            description: Some(Arc::from("Configure the selected setting.")),
            detail: None,
            slide: 0.0,
        }
    }

    fn has_text(actors: &[Actor], text: &str, position: [f32; 2]) -> bool {
        actors.iter().any(|actor| {
            matches!(actor,
                Actor::Text { content, offset, .. }
                    if content.as_str() == text && *offset == position
            )
        })
    }

    #[test]
    fn retains_the_native_panel_and_rows_inside_a_wide_frame() {
        for (width, delta) in [(640.0, 0.0), (854.0, 107.0)] {
            with_current_metrics(Metrics::centered(width, 480.0), || {
                let mut actors = Vec::new();
                push_background(&mut actors);
                push_page(&mut actors, page(&|_| Some(action("Graphics")), 1, 0));
                assert!(actors.iter().any(|actor| matches!(actor,
                    Actor::Sprite { offset, size: [SizeSpec::Px(w), SizeSpec::Px(h)], .. }
                        if *offset == [15.0 + delta, 90.0] && *w == 610.0 && *h == 336.0
                )));
                assert!(has_text(&actors, "SERVICE OPTIONS", [36.0 + delta, 38.0]));
                assert!(has_text(&actors, "Graphics", [160.0 + delta, 127.0]));
                assert!(has_text(&actors, "DONE", [320.0 + delta, 353.0]));
                assert!(has_text(
                    &actors,
                    "Configure the selected setting.",
                    [54.0 + delta, 381.0],
                ));
                actors.clear();
                push_page(
                    &mut actors,
                    SystemOptionsPage {
                        title: "GRAPHICS OPTIONS",
                        main_menu: false,
                        slide: -0.25,
                        ..page(&|_| Some(action("Renderer")), 1, 0)
                    },
                );
                assert!(has_text(
                    &actors,
                    "GRAPHICS OPTIONS",
                    [36.0 + delta - width * 0.25, 38.0],
                ));
            });
        }
    }

    #[test]
    fn visits_only_the_visible_window_and_shows_done_at_the_end() {
        with_current_metrics(Metrics::centered(640.0, 480.0), || {
            let mut actors = Vec::new();
            let visited = RefCell::new(Vec::new());
            let row = |index| {
                visited.borrow_mut().push(index);
                Some(action("Option"))
            };
            push_page(&mut actors, page(&row, 18, 8));
            assert_eq!(*visited.borrow(), (4..12).collect::<Vec<_>>());
            assert!(!has_text(&actors, "DONE", [320.0, 353.0]));
            assert!(actors.iter().any(|actor| matches!(actor,
                Actor::Sprite { offset, tint, .. } if *offset == [35.0, 227.0] && *tint == AMBER
            )));
            actors.clear();
            visited.borrow_mut().clear();
            push_page(&mut actors, page(&row, 18, 18));
            assert_eq!(*visited.borrow(), (10..18).collect::<Vec<_>>());
            assert!(has_text(&actors, "DONE", [320.0, 353.0]));
        });
    }

    #[test]
    fn prepared_values_keep_their_shared_text_and_disabled_toggles_stay_visible() {
        with_current_metrics(Metrics::centered(640.0, 480.0), || {
            let mut actors = Vec::new();
            let value = Arc::<str>::from("38 ms");
            let row = |_| {
                let mut row = action("Offset");
                row.disabled = true;
                row.choices = Some(Arc::from([value.clone()]));
                row.enabled_choices = Some(1);
                Some(row)
            };
            push_page(&mut actors, page(&row, 1, 0));
            assert!(has_text(&actors, "Offset", [53.0, 127.0]));
            assert!(has_text(&actors, "LOCKED", [582.0, 127.0]));
            assert!(has_text(&actors, "ON", [518.0, 127.0]));
            assert!(actors.iter().any(|actor| matches!(actor,
                Actor::Text { content: TextContent::Shared(text), color, offset, .. }
                    if Arc::ptr_eq(text, &value) && *color == DIM && *offset == [160.0, 127.0]
            )));
        });
    }
}
