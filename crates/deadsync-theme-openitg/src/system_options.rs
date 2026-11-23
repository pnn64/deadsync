mod choice_layout;
mod help;

use deadlib_assets::AssetManager;
use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::space::{screen_height, screen_width};
use deadsync_theme::system_options::{SystemOptionsLayout, SystemOptionsPresentation};
use deadsync_theme::views::{
    SystemOptionsPage, SystemOptionsPageKind, SystemOptionsPhase, SystemOptionsRenderer,
    SystemOptionsRow, SystemOptionsServiceRow, SystemOptionsTiming,
};

use crate::presentation::{
    ACCENT, Layout, LinearMotion, TextMetrics, options_label, push_options_cursor,
    push_options_heading, push_options_highlight, push_options_panel, push_red_backdrop,
    push_red_streaks,
};
use crate::resources::MENU_NORMAL_FONT;
use choice_layout::{ChoiceLayout, CursorLabel};
use help::OptionsHelp;

#[derive(Default)]
pub struct OpenItgSystemOptions {
    page_id: Option<usize>,
    first: usize,
    rows: Vec<LinearMotion<2>>,
    choices: Vec<ChoiceLayout>,
    rows_move_until: f64,
    cursor: Option<LinearMotion<3>>,
    cursor_label: CursorLabel,
    thumb: LinearMotion<1>,
    help: OptionsHelp,
}

impl OpenItgSystemOptions {
    pub const SERVICE_ORDER: &'static [SystemOptionsServiceRow] = &[
        SystemOptionsServiceRow::Arcade,
        SystemOptionsServiceRow::System,
        SystemOptionsServiceRow::Graphics,
        SystemOptionsServiceRow::SongPacks,
        SystemOptionsServiceRow::Downloads,
        SystemOptionsServiceRow::TestInput,
        SystemOptionsServiceRow::ConfigureMappings,
        SystemOptionsServiceRow::ReloadSongsCourses,
    ];
    pub const TIMING: SystemOptionsTiming = SystemOptionsTiming {
        service_seconds: 0.5,
        settings_seconds: PANEL_SECONDS,
    };
}

impl SystemOptionsPresentation<Actor> for OpenItgSystemOptions {
    fn layout(&self) -> SystemOptionsLayout {
        SystemOptionsLayout {
            split_graphics: true,
            service_order: Self::SERVICE_ORDER,
            timing: Some(Self::TIMING),
            return_overscan_to_graphics: true,
        }
    }

    /// Start a fresh Options visit while retaining the row allocation.
    fn reset(&mut self) {
        self.page_id = None;
    }

    fn with_renderer(
        &mut self,
        phase: SystemOptionsPhase,
        now_seconds: f64,
        assets: &AssetManager,
        draw: &mut dyn FnMut(SystemOptionsRenderer<'_, Actor>),
    ) {
        let mut page = |actors: &mut Vec<Actor>, page: SystemOptionsPage<'_>| {
            self.push_page(actors, page, assets, now_seconds);
        };
        draw(SystemOptionsRenderer {
            phase,
            background: push_background,
            page: &mut page,
        });
    }
}

impl OpenItgSystemOptions {
    fn retarget_rows(&mut self, page: &SystemOptionsPage<'_>, first: usize, end: usize, now: f64) {
        let progress = page.selected as f32 / page.row_count.max(1) as f32;
        if self.page_id != Some(page.id) || self.rows.len() != page.row_count + 1 {
            self.page_id = Some(page.id);
            self.rows
                .resize(page.row_count + 1, LinearMotion::default());
            self.choices
                .resize_with(page.row_count, ChoiceLayout::default);
            for (index, row) in self.rows.iter_mut().enumerate() {
                *row = LinearMotion::at(row_destination(page, first, end, index));
            }
            self.rows_move_until = now;
            self.cursor = None;
            self.thumb = LinearMotion::at([progress]);
        } else if self.first != first {
            for (index, row) in self.rows.iter_mut().enumerate() {
                row.retarget(row_destination(page, first, end, index), now, ROW_SECONDS);
            }
            self.rows_move_until = now + ROW_SECONDS;
        }
        self.first = first;
        self.thumb.retarget([progress], now, SCROLLBAR_SECONDS);
    }
}

const ROWS_SHOWN: usize = 8;
const ROW_SECONDS: f64 = 0.15;
const CURSOR_SECONDS: f64 = 0.15;
const SCROLLBAR_SECONDS: f64 = 0.2;
const PANEL_SECONDS: f32 = 0.3;
const SERVICE_HEADER_ENTRY_SECONDS: f32 = 0.3;
const SERVICE_HEADER_EXIT_SECONDS: f32 = 0.2;
const FIRST_ROW_Y: f32 = 127.0;
const ROW_PITCH: f32 = 28.0;
// The native preset occupies slot 1. Its three resolution rows share slot 3.
const BASIC_GRAPHICS_SLOTS: [usize; 4] = [0, 2, 3, 5];
const DONE_Y: f32 = 353.0;
const LABEL_X: f32 = 53.0;
const VALUE_X: f32 = 160.0;
const VALUE_END_X: f32 = 595.0;
const ITEM_GAP: f32 = 14.0;
const INLINE_ZOOM: f32 = 0.59;
const ROW_ZOOM: f32 = 0.62;
const DONE_ZOOM: f32 = 0.69;

const STROKE: [f32; 4] = [0.39, 0.15, 0.16, 1.0];
const MUTED: [f32; 4] = [0.79, 0.72, 0.72, 1.0];
const DIM: [f32; 4] = [0.55, 0.46, 0.48, 1.0];
const AMBER: [f32; 4] = [ACCENT[0], ACCENT[1], ACCENT[2], 1.0];
const WHITE: [f32; 4] = [1.0; 4];

fn row_destination(
    page: &SystemOptionsPage<'_>,
    first: usize,
    end: usize,
    index: usize,
) -> [f32; 2] {
    if index == page.row_count {
        return [DONE_Y, if end == page.row_count { 1.0 } else { 0.0 }];
    }
    if index < first {
        return [FIRST_ROW_Y - ROW_PITCH * 0.5, 0.0];
    }
    if index >= end {
        return [FIRST_ROW_Y + ROW_PITCH * (ROWS_SHOWN as f32 - 0.5), 0.0];
    }
    let slot = index - first;
    let slot = if page.kind == SystemOptionsPageKind::BasicGraphics {
        BASIC_GRAPHICS_SLOTS.get(slot).copied().unwrap_or(slot)
    } else {
        slot
    };
    [ROW_PITCH.mul_add(slot as f32, FIRST_ROW_Y), 1.0]
}

fn push_background(actors: &mut Vec<Actor>) {
    let (width, height) = (screen_width(), screen_height());
    push_red_backdrop(actors, [width, height], 0.0, -30);
    push_red_streaks(actors, [width, height], 1.0, -28);
}

fn push_service_header(actors: &mut Vec<Actor>, layout: Layout, phase: SystemOptionsPhase) {
    let (stretch_x, stretch_y, alpha) = match phase {
        SystemOptionsPhase::Entering(elapsed) if elapsed < SERVICE_HEADER_ENTRY_SECONDS => {
            let t = (elapsed / SERVICE_HEADER_ENTRY_SECONDS).clamp(0.0, 1.0);
            // The original bounce-end tween uses the overshooting sine segment.
            let angle = (std::f32::consts::PI - 1.1).mul_add(1.0 - t, 1.1);
            let progress = angle.sin() / 0.89;
            (progress, (-5.0f32).mul_add(progress, 6.0), 1.0)
        }
        SystemOptionsPhase::Leaving(elapsed) => {
            let t = (elapsed / SERVICE_HEADER_EXIT_SECONDS).clamp(0.0, 1.0);
            let t = t * t;
            (1.0 + t, 1.0 - t, 1.0 - t)
        }
        _ => (1.0, 1.0, 1.0),
    };
    if stretch_x <= 0.0 || alpha <= 0.0 {
        return;
    }
    push_options_heading(
        actors,
        layout,
        "SERVICE OPTIONS",
        [stretch_x, stretch_y],
        alpha,
    );
}

impl OpenItgSystemOptions {
    fn push_page(
        &mut self,
        actors: &mut Vec<Actor>,
        page: SystemOptionsPage<'_>,
        assets: &AssetManager,
        now: f64,
    ) {
        let width = screen_width();
        let origin = Layout::centered(width, screen_height());
        let slide = match page.phase {
            SystemOptionsPhase::Steady => 0.0,
            SystemOptionsPhase::Entering(elapsed) => {
                -(1.0 - (elapsed / PANEL_SECONDS).clamp(0.0, 1.0)).powi(2)
            }
            SystemOptionsPhase::Leaving(elapsed) => {
                (elapsed / PANEL_SECONDS).clamp(0.0, 1.0).powi(2)
            }
        };
        if slide.abs() >= 1.0 {
            return;
        }
        let layout = origin.translated_x(slide * width);
        self.help
            .update(assets, &page, self.page_id != Some(page.id), now);
        let bottom = self.help.panel_bottom();
        push_options_panel(actors, layout, bottom, 1.0);
        if page.kind == SystemOptionsPageKind::ServiceMenu {
            push_service_header(actors, origin, page.phase);
        } else {
            push_options_heading(actors, layout, page.title, [1.0; 2], 1.0);
        }

        let end_offset = page.row_count.saturating_sub(ROWS_SHOWN);
        let first = page.selected.saturating_sub(ROWS_SHOWN / 2).min(end_offset);
        let end = (first + ROWS_SHOWN).min(page.row_count);
        self.retarget_rows(&page, first, end, now);
        let focus_y = self.rows[page.selected].target[0];
        push_options_highlight(
            actors,
            layout,
            focus_y,
            if focus_y == DONE_Y { 33.0 } else { 30.0 },
            1.0,
        );
        let moving = now < self.rows_move_until;
        let metrics = TextMetrics::new(assets.fonts(), MENU_NORMAL_FONT);
        let range = if moving {
            0..page.row_count
        } else {
            first..end
        };
        let mut cursor_target = None;
        for index in range {
            let (y, alpha) = if moving {
                let [y, alpha] = self.rows[index].sample(now, ROW_SECONDS);
                (y, alpha)
            } else {
                (self.rows[index].target[0], 1.0)
            };
            let focused = index == page.selected;
            if alpha <= 0.0 && !focused {
                continue;
            }
            if let Some(row) = (page.row)(index) {
                let row_start = actors.len();
                if let Some([center, width]) = push_row(
                    actors,
                    layout,
                    y,
                    &row,
                    focused,
                    &mut self.choices[index],
                    &mut self.cursor_label,
                    metrics,
                ) {
                    cursor_target = Some([center.round(), focus_y, marker_width(width)]);
                }
                if alpha < 1.0 {
                    for actor in &mut actors[row_start..] {
                        actor.mul_alpha(alpha);
                    }
                }
            }
        }
        let focused_done = page.selected == page.row_count;
        let [_, done_alpha] = self.rows[page.row_count].sample(now, ROW_SECONDS);
        if done_alpha > 0.0 || focused_done {
            let done_start = actors.len();
            actors.push(options_label(
                layout,
                [320.0, DONE_Y],
                0.5,
                180.0,
                "DONE".into(),
                DONE_ZOOM,
                if focused_done { WHITE } else { MUTED },
            ));
            if done_alpha < 1.0 {
                for actor in &mut actors[done_start..] {
                    actor.mul_alpha(done_alpha);
                }
            }
            if focused_done {
                cursor_target = Some([
                    320.0,
                    DONE_Y,
                    marker_width(metrics.width("DONE") * DONE_ZOOM),
                ]);
            }
        }
        if let Some(target) = cursor_target {
            let cursor = self.cursor.get_or_insert(LinearMotion::at(target));
            cursor.retarget(target, now, CURSOR_SECONDS);
            let [center, y, width] = cursor.sample(now, CURSOR_SECONDS);
            push_options_cursor(actors, layout, [center, y], width, 0, true, 1.0);
        } else {
            self.cursor = None;
        }
        if end_offset > 0 {
            let track_height = 240.0;
            let thumb_height = 16.0;
            let [percent] = self.thumb.sample(now, SCROLLBAR_SECONDS);
            let thumb_offset = (track_height - thumb_height) * percent;
            let top = 122.0;
            actors.push(layout.panel(33.0, top, [2.0, track_height], STROKE, -15));
            actors.push(layout.panel(33.0, top + thumb_offset, [2.0, thumb_height], AMBER, -14));
        }
        self.help.push(actors, layout, [WHITE, MUTED], now);
        if bottom <= 437.0 {
            let hint = if page.kind == SystemOptionsPageKind::ServiceMenu {
                "Press Start to open the selected option"
            } else {
                "Use Left or Right to change a setting"
            };
            actors.push(options_label(
                layout,
                [320.0, 446.0],
                0.5,
                550.0,
                hint.into(),
                0.62,
                WHITE,
            ));
        }
    }
}

fn push_row(
    actors: &mut Vec<Actor>,
    layout: Layout,
    y: f32,
    row: &SystemOptionsRow,
    focused: bool,
    choice_layout: &mut ChoiceLayout,
    cursor_label: &mut CursorLabel,
    metrics: TextMetrics<'_>,
) -> Option<[f32; 2]> {
    let color = if row.disabled {
        DIM
    } else if focused {
        WHITE
    } else {
        MUTED
    };
    let choices = row.choices.as_ref().filter(|choices| !choices.is_empty());
    let (label_x, label_width) = if choices.is_some() {
        (LABEL_X, VALUE_X - LABEL_X - 13.0)
    } else {
        (VALUE_X, VALUE_END_X - VALUE_X)
    };
    actors.push(options_label(
        layout,
        [label_x, y],
        0.0,
        label_width,
        TextContent::Shared(row.label.clone()),
        ROW_ZOOM,
        color,
    ));
    let show_cursor = focused && !row.disabled;
    let mut cursor = None;
    if let Some(choices) = choices {
        let selected = row.selected_choice.min(choices.len() - 1);
        let value_limit = if row.disabled { 534.0 } else { VALUE_END_X };
        let fits_inline = if row.inline && choices.len() > 1 && row.choice_tint.is_none() {
            choice_layout.update(choices, metrics);
            choice_layout.total_width() * INLINE_ZOOM + ITEM_GAP * (choices.len() - 1) as f32
                <= value_limit - VALUE_X
        } else {
            false
        };
        if fits_inline {
            let mut x = VALUE_X;
            for (index, (choice, width)) in choices.iter().zip(choice_layout.widths()).enumerate() {
                let width = width * INLINE_ZOOM;
                let center = x + width * 0.5;
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
                actors.push(options_label(
                    layout,
                    [center, y],
                    0.5,
                    width + 2.0,
                    TextContent::Shared(choice.clone()),
                    INLINE_ZOOM,
                    color,
                ));
                if show_cursor && index == selected {
                    cursor = Some([center, width]);
                } else if enabled {
                    push_options_cursor(
                        actors,
                        layout,
                        [center, y],
                        marker_width(width),
                        0,
                        false,
                        1.0,
                    );
                }
                x += width + ITEM_GAP;
            }
        } else {
            let mut value_x = VALUE_X;
            if let Some(tint) = row.choice_tint {
                actors.push(layout.panel(VALUE_X, y - 6.0, [14.0, 12.0], tint, -15));
                value_x += 22.0;
            }
            let width_limit = value_limit
                - value_x
                - if row.enabled_choices.is_some() {
                    56.0
                } else {
                    14.0
                };
            actors.push(options_label(
                layout,
                [value_x, y],
                0.0,
                width_limit,
                TextContent::Shared(choices[selected].clone()),
                ROW_ZOOM,
                color,
            ));
            if show_cursor {
                let width =
                    (cursor_label.width(&choices[selected], metrics) * ROW_ZOOM).min(width_limit);
                cursor = Some([value_x + width * 0.5, width]);
            }
            if let Some(mask) = row.enabled_choices {
                let (status, tint) = if choice_enabled(mask, selected) {
                    ("ON", AMBER)
                } else {
                    ("OFF", MUTED)
                };
                actors.push(options_label(
                    layout,
                    [value_limit - 16.0, y],
                    1.0,
                    39.0,
                    status.into(),
                    0.52,
                    tint,
                ));
            }
            if focused && !row.disabled && choices.len() > 1 {
                actors.push(options_label(
                    layout,
                    [VALUE_X - 10.0, y],
                    0.5,
                    13.0,
                    "<".into(),
                    0.53,
                    WHITE,
                ));
                actors.push(options_label(
                    layout,
                    [VALUE_END_X, y],
                    0.5,
                    13.0,
                    ">".into(),
                    0.53,
                    AMBER,
                ));
            }
        }
    } else if show_cursor {
        let width = (cursor_label.width(&row.label, metrics) * ROW_ZOOM).min(label_width);
        cursor = Some([label_x + width * 0.5, width]);
    }
    if row.disabled {
        actors.push(options_label(
            layout,
            [582.0, y],
            1.0,
            80.0,
            "LOCKED".into(),
            0.52,
            DIM,
        ));
    }
    cursor
}

fn marker_width(text_width: f32) -> f32 {
    let pixels = text_width.round() as u32;
    (pixels + (pixels & 1) + 4) as f32
}

fn choice_enabled(mask: u64, index: usize) -> bool {
    index < u64::BITS as usize && (mask & (1_u64 << index)) != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::actors::TextAttributes;
    use deadlib_present::actors::{SizeSpec, SpriteSource};
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
            id: 0,
            title: "OPTIONS",
            kind: SystemOptionsPageKind::ServiceMenu,
            selected,
            row_count,
            row,
            description: Some(Arc::from("Configure the selected setting.")),
            detail: None,
            phase: SystemOptionsPhase::Steady,
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

    fn cursor_center(actor: &Actor) -> Option<([f32; 2], f32)> {
        match actor {
            Actor::Sprite {
                source: SpriteSource::TextureStatic("openitg/options-cursor.png"),
                offset,
                size: [SizeSpec::Px(width), SizeSpec::Px(26.0)],
                uv_rect: Some(uv),
                ..
            } if *uv == [1.0 / 3.0, 0.0, 2.0 / 3.0, 0.5] => Some((*offset, *width)),
            _ => None,
        }
    }

    fn push_page(actors: &mut Vec<Actor>, page: SystemOptionsPage<'_>) {
        OpenItgSystemOptions::default().push_page(actors, page, &AssetManager::new(), 0.0);
    }

    #[test]
    fn retains_the_native_panel_and_rows_inside_a_wide_frame() {
        for (width, delta) in [(640.0, 0.0), (854.0, 107.0)] {
            with_current_metrics(Metrics::centered(width, 480.0), || {
                let mut actors = Vec::new();
                push_background(&mut actors);
                push_page(&mut actors, page(&|_| Some(action("Graphics")), 1, 0));
                assert!(actors.iter().any(|actor| matches!(actor,
                    Actor::Sprite { source: SpriteSource::TextureStatic("openitg/options-frame.png"), offset, size: [SizeSpec::Px(w), SizeSpec::Px(h)], .. }
                        if *offset == [320.0 + delta, 258.0] && *w == 610.0 && *h == 336.0
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
                        kind: SystemOptionsPageKind::Settings,
                        phase: SystemOptionsPhase::Entering(PANEL_SECONDS / 2.0),
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
    fn service_header_scales_in_place_while_the_panel_slides() {
        with_current_metrics(Metrics::centered(640.0, 480.0), || {
            let mut actors = Vec::new();
            let row = |_| Some(action("Graphics"));
            push_page(
                &mut actors,
                SystemOptionsPage {
                    phase: SystemOptionsPhase::Entering(0.21),
                    ..page(&row, 1, 0)
                },
            );
            assert!(actors.iter().any(|actor| matches!(actor,
                Actor::Text { content, offset, scale, .. }
                    if content.as_str() == "SERVICE OPTIONS"
                        && *offset == [36.0, 38.0] && scale[0] > 1.0 && scale[1] < 1.0
            )));
            assert!(actors.iter().any(|actor| matches!(actor,
                Actor::Sprite { source: SpriteSource::TextureStatic("openitg/options-frame.png"), offset, .. }
                    if (offset[0] - (320.0 - 57.6)).abs() < 0.01 && offset[1] == 258.0
            )));

            actors.clear();
            push_page(
                &mut actors,
                SystemOptionsPage {
                    phase: SystemOptionsPhase::Leaving(0.1),
                    ..page(&row, 1, 0)
                },
            );
            assert!(actors.iter().any(|actor| matches!(actor,
                Actor::Text { content, offset, color, scale, .. }
                    if content.as_str() == "SERVICE OPTIONS" && *offset == [36.0, 38.0]
                        && color[3] == 0.75 && (scale[0] - 1.25).abs() < 0.001
                        && (scale[1] - 0.75).abs() < 0.001
            )));
            actors.clear();
            push_page(
                &mut actors,
                SystemOptionsPage {
                    phase: SystemOptionsPhase::Leaving(0.25),
                    ..page(&row, 1, 0)
                },
            );
            assert!(!has_text(&actors, "SERVICE OPTIONS", [36.0, 38.0]));
            assert!(!actors.is_empty());
        });
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
                Actor::Sprite { source: SpriteSource::TextureStatic("openitg/options-highlight.png"), offset, .. }
                    if *offset == [320.0, 240.0]
            )));
            actors.clear();
            visited.borrow_mut().clear();
            push_page(&mut actors, page(&row, 18, 18));
            assert_eq!(*visited.borrow(), (10..18).collect::<Vec<_>>());
            assert!(has_text(&actors, "DONE", [320.0, 353.0]));
        });
    }

    #[test]
    fn fast_repeats_retarget_rows_cursor_and_thumb_from_the_current_frame() {
        with_current_metrics(Metrics::centered(640.0, 480.0), || {
            let labels = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
            let row = |index: usize| Some(action(labels[index]));
            let mut visual = OpenItgSystemOptions::default();
            let assets = AssetManager::new();
            let mut actors = Vec::new();
            let mut frame = |selected: usize, now: f64| {
                actors.clear();
                visual.push_page(
                    &mut actors,
                    page(&row, labels.len(), selected),
                    &assets,
                    now,
                );
                let y = actors.iter().find_map(|actor| match actor {
                    Actor::Text {
                        content, offset, ..
                    } if content.as_str() == "1" => Some(offset[1]),
                    _ => None,
                });
                let cursor = actors.iter().find_map(cursor_center).unwrap().0[1];
                let thumb = actors.iter().find_map(|actor| match actor {
                    Actor::Sprite { offset, tint, .. } if offset[0] == 33.0 && *tint == AMBER => {
                        Some(offset[1])
                    }
                    _ => None,
                });
                [y.unwrap_or(-1.0), cursor, thumb.unwrap()]
            };

            frame(3, 1.0);
            assert_eq!(frame(5, 1.02)[..2], [155.0, 211.0]);
            let moving = frame(5, 1.095);
            assert!((moving[0] - 141.0).abs() < 0.01);
            assert!((moving[1] - 225.0).abs() < 0.01);
            assert!((moving[2] - 206.0).abs() < 0.01);
            assert_eq!(frame(6, 1.095), moving);
            let settled = frame(6, 1.4);
            assert_eq!(settled[..2], [-1.0, 239.0]);
            assert!((settled[2] - 256.4).abs() < 0.01);
        });
    }

    #[test]
    fn choice_cursor_moves_horizontally_and_a_new_page_starts_settled() {
        with_current_metrics(Metrics::centered(640.0, 480.0), || {
            let choices: Arc<[Arc<str>]> = Arc::from(["Off".into(), "Low".into(), "High".into()]);
            let mut visual = OpenItgSystemOptions::default();
            let assets = AssetManager::new();
            let mut actors = Vec::new();
            let mut cursor_geometry = |id, choice, now| {
                let row = |_| {
                    Some(SystemOptionsRow {
                        choices: Some(Arc::clone(&choices)),
                        selected_choice: choice,
                        inline: true,
                        ..action("Quality")
                    })
                };
                actors.clear();
                visual.push_page(
                    &mut actors,
                    SystemOptionsPage {
                        id,
                        ..page(&row, 1, 0)
                    },
                    &assets,
                    now,
                );
                actors
                    .iter()
                    .find_map(cursor_center)
                    .map(|([x, _], width)| [x, width])
                    .unwrap()
            };

            assert_eq!(cursor_geometry(1, 0, 0.0), [167.0, 18.0]);
            assert_eq!(cursor_geometry(1, 2, 0.1), [167.0, 18.0]);
            assert_eq!(cursor_geometry(1, 2, 0.175), [196.5, 21.0]);
            assert_eq!(cursor_geometry(2, 2, 0.18), [226.0, 24.0]);
        });
    }

    #[test]
    fn inline_choices_and_the_marker_follow_bitmap_text_widths() {
        let assets_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let font = deadlib_assets::parse_font_with_asset_context(
            &assets_root.join("fonts/openitg/_eurostile normal.ini"),
            vec![assets_root.clone()],
        )
        .expect("bundled font");
        let mut assets = AssetManager::new();
        assets.register_font(MENU_NORMAL_FONT, font.font);
        let metrics = TextMetrics::new(assets.fonts(), MENU_NORMAL_FONT);
        let narrow_width = metrics.width("iii") * INLINE_ZOOM;
        let wide_width = metrics.width("WWW") * INLINE_ZOOM;
        assert!(narrow_width < wide_width);
        let narrow_center = VALUE_X + narrow_width * 0.5;
        let wide_center = VALUE_X + narrow_width + ITEM_GAP + wide_width * 0.5;
        let choices = Arc::from([Arc::from("iii"), Arc::from("WWW")]);
        let row = |_| {
            Some(SystemOptionsRow {
                choices: Some(Arc::clone(&choices)),
                selected_choice: 1,
                inline: true,
                ..action("Quality")
            })
        };
        for (width, delta) in [(640.0, 0.0), (854.0, 107.0)] {
            with_current_metrics(Metrics::centered(width, 480.0), || {
                let mut actors = Vec::new();
                OpenItgSystemOptions::default().push_page(
                    &mut actors,
                    page(&row, 1, 0),
                    &assets,
                    0.0,
                );
                assert!(has_text(
                    &actors,
                    "iii",
                    [narrow_center + delta, FIRST_ROW_Y]
                ));
                assert!(has_text(&actors, "WWW", [wide_center + delta, FIRST_ROW_Y]));
                let marker = marker_width(wide_width);
                assert_eq!(
                    actors.iter().find_map(cursor_center),
                    Some(([wide_center.round() + delta, FIRST_ROW_Y], marker)),
                );
            });
        }
    }

    #[test]
    fn explanation_reveal_follows_text_changes_and_page_entry_time() {
        with_current_metrics(Metrics::centered(640.0, 480.0), || {
            let mut visual = OpenItgSystemOptions::default();
            let assets = AssetManager::new();
            let mut actors = Vec::new();
            let row = |_| Some(action("Input"));
            let mut frame = |text: &str, selected, phase, now| {
                actors.clear();
                visual.push_page(
                    &mut actors,
                    SystemOptionsPage {
                        description: Some(Arc::from(text)),
                        phase,
                        ..page(&row, 2, selected)
                    },
                    &assets,
                    now,
                );
                actors
                    .iter()
                    .find_map(|actor| match actor {
                        Actor::Text {
                            content,
                            attributes,
                            offset,
                            scale,
                            ..
                        } if offset[1] == 381.0 && content.as_str() == text => {
                            assert_eq!(scale[0], scale[1]);
                            Some(match attributes {
                                TextAttributes::GlyphReveal(fraction) => *fraction,
                                _ => 1.0,
                            })
                        }
                        _ => None,
                    })
                    .expect("explanation actor")
            };

            let arrived = frame(
                "Test the inputs.",
                0,
                SystemOptionsPhase::Entering(0.125),
                1.0,
            );
            assert_eq!(arrived, 0.25);
            assert_eq!(
                frame("Test the inputs.", 1, SystemOptionsPhase::Steady, 1.125),
                0.5
            );
            assert_eq!(
                frame("Map the inputs.", 0, SystemOptionsPhase::Steady, 1.25),
                0.0
            );
            assert_eq!(
                frame("Map the inputs.", 0, SystemOptionsPhase::Steady, 1.75),
                1.0
            );
        });
    }

    #[test]
    fn basic_graphics_uses_the_native_row_slots_at_both_widths() {
        let rows = [
            ("Display Mode", 127.0),
            ("Aspect Ratio", 183.0),
            ("Resolution", 211.0),
            ("Advanced Graphic Options", 267.0),
        ];
        let row = |index: usize| rows.get(index).map(|(label, _)| action(label));
        for (width, delta) in [(640.0, 0.0), (854.0, 107.0)] {
            with_current_metrics(Metrics::centered(width, 480.0), || {
                let mut actors = Vec::new();
                push_page(
                    &mut actors,
                    SystemOptionsPage {
                        title: "GRAPHIC OPTIONS",
                        kind: SystemOptionsPageKind::BasicGraphics,
                        ..page(&row, rows.len(), 3)
                    },
                );
                for (label, y) in rows {
                    assert!(has_text(&actors, label, [160.0 + delta, y]));
                }
                assert!(has_text(&actors, "DONE", [320.0 + delta, 353.0]));
            });
        }
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
