use crate::scene::{AMBER, MINT, MUTED, PAPER, PITCH, Scene};
use deadlib_assets::AssetManager;
use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::dsl::{SpriteBuilder, TweenSteps};
use deadlib_present::{anim, runtime, space};
use deadsync_theme::Screen;
use deadsync_theme::player_options::{
    PlayerOptionsLayout, PlayerOptionsPage, PlayerOptionsViewport,
};
use deadsync_theme::system_options::{SystemOptionsLayout, SystemOptionsPresentation};
use deadsync_theme::views::{
    SystemOptionsPage, SystemOptionsPageKind, SystemOptionsPhase, SystemOptionsRenderer,
    SystemOptionsTiming,
};
use std::ops::Range;
use std::sync::Arc;

const SEAT_COLORS: [[f32; 3]; 2] = [MINT, AMBER];
const SEAT_LABELS: [&str; 2] = ["PLAYER 1", "PLAYER 2"];
const LEFT: f32 = 46.0;
const RIGHT: f32 = 594.0;
const WIDTH: f32 = RIGHT - LEFT;
const FIRST_ROW: f32 = 115.0;
const LAST_ROW: f32 = 311.0;
const FOOTER_ROW: f32 = 341.0;
const HELP_TOP: f32 = 374.0;
const SETTINGS_TIMING: SystemOptionsTiming = SystemOptionsTiming {
    service_seconds: 0.28,
    settings_seconds: 0.22,
};
const PLAYER_ENTRY_SECONDS: f32 = 0.25;
const PLAYER_EXIT_SECONDS: f32 = 0.20;

pub struct ExtremeSystemOptions;

impl SystemOptionsPresentation<Actor> for ExtremeSystemOptions {
    fn layout(&self) -> SystemOptionsLayout {
        SystemOptionsLayout {
            timing: Some(SETTINGS_TIMING),
            ..SystemOptionsLayout::default()
        }
    }

    fn reset(&mut self) {}

    fn with_renderer(
        &mut self,
        phase: SystemOptionsPhase,
        _: f64,
        _: &AssetManager,
        draw: &mut dyn FnMut(SystemOptionsRenderer<'_, Actor>),
    ) {
        draw(SystemOptionsRenderer {
            phase,
            background: |actors| Scene::new(1.0).cover(actors),
            page: &mut push_system_page,
        });
    }
}

fn push_system_page(actors: &mut Vec<Actor>, page: SystemOptionsPage<'_>) {
    const ROWS: usize = 7;
    let duration = if page.kind == SystemOptionsPageKind::ServiceMenu {
        SETTINGS_TIMING.service_seconds
    } else {
        SETTINGS_TIMING.settings_seconds
    };
    let alpha = match page.phase {
        SystemOptionsPhase::Steady => 1.0,
        SystemOptionsPhase::Entering(elapsed) => elapsed / duration,
        SystemOptionsPhase::Leaving(elapsed) => 1.0 - elapsed / duration,
    };
    let scene = Scene::new(alpha);
    page_heading(actors, scene, page.title);

    let visible = centered_window(page.row_count, page.selected, ROWS);
    for (slot, index) in visible.enumerate() {
        let Some(row) = (page.row)(index) else {
            continue;
        };
        let y = (LAST_ROW - FIRST_ROW).mul_add(slot as f32 / (ROWS - 1) as f32, FIRST_ROW);
        let focused = index == page.selected;
        let selectable = scene.with_alpha(alpha * if row.disabled { 0.50 } else { 1.0 });
        row_panel(actors, selectable, y, 27.0, focused);

        let label_width = if row.choices.is_some() {
            225.0
        } else {
            WIDTH - 30.0
        };
        selectable.text(
            actors,
            [LEFT + 12.0, y],
            [0.72, label_width],
            row.label,
            if focused { PAPER } else { MUTED },
        );
        if let Some(choices) = row.choices.as_ref() {
            let has_choices = row.inline || row.enabled_choices.is_some();
            let accent = row.choice_tint.map_or(MINT, |[r, g, b, _]| [r, g, b]);
            if has_choices {
                draw_choices(
                    actors,
                    selectable,
                    ChoiceStrip {
                        left: 288.0,
                        y,
                        width: 294.0,
                        limit: 4,
                        accent,
                    },
                    choices.len(),
                    row.selected_choice,
                    focused,
                    |index| {
                        let enabled = row
                            .enabled_choices
                            .map_or(index == row.selected_choice, |bits| {
                                index < u64::BITS as usize && bits & (1_u64 << index) != 0
                            });
                        ((&choices[index]).into(), enabled)
                    },
                );
            } else if let Some(value) = choices.get(row.selected_choice) {
                selectable.text(
                    actors,
                    [300.0, y],
                    [0.68, 275.0],
                    value,
                    if focused { accent } else { PAPER },
                );
                if focused && !row.disabled && choices.len() > 1 {
                    selectable.rect(actors, [290.0, y - 8.0], [2.0, 16.0], accent, 1.0, 2);
                }
            }
        }
    }
    scroll_track(
        actors,
        scene,
        607.0,
        page.selected as f32 / page.row_count.max(1) as f32,
        MINT,
    );
    let focused_done = page.selected == page.row_count;
    row_panel(actors, scene, FOOTER_ROW, 27.0, focused_done);
    scene.text(
        actors,
        [LEFT + 12.0, FOOTER_ROW],
        [0.67, WIDTH - 24.0],
        "DONE",
        if focused_done { MINT } else { MUTED },
    );
    let description = page.description.as_ref().or(page.detail.as_ref());
    let detail = page.detail.as_ref().filter(|detail| {
        page.description
            .as_ref()
            .is_some_and(|body| body != *detail)
    });
    help_panel(
        actors,
        scene,
        LEFT,
        WIDTH,
        description,
        detail,
        if page.kind == SystemOptionsPageKind::ServiceMenu {
            "START TO OPEN"
        } else {
            "LEFT / RIGHT TO CHANGE"
        },
    );
}

pub struct ExtremePlayerOptions;

impl PlayerOptionsLayout<Actor, TextContent> for ExtremePlayerOptions {
    fn viewport(&self) -> PlayerOptionsViewport {
        PlayerOptionsViewport {
            slots: 8,
            separate_exit: true,
            row_tween_seconds: 0.14,
        }
    }

    fn push_page(
        &mut self,
        actors: &mut Vec<Actor>,
        page: PlayerOptionsPage<'_, TextContent, Actor>,
        _: &AssetManager,
        _: f64,
    ) {
        let background = Scene::new(1.0);
        background.cover(actors);
        let scene = background.with_alpha(page.alpha);
        page_heading(actors, scene, page.title);
        let both = page.active == [true; 2];
        let value_area = if both {
            [(260.0, 157.0), (426.0, 157.0)]
        } else {
            [(260.0, 323.0); 2]
        };
        for (seat, &(left, width)) in value_area.iter().enumerate() {
            if page.active[seat] {
                scene.text(
                    actors,
                    [left + 6.0, 88.0],
                    [0.58, width - 12.0],
                    SEAT_LABELS[seat],
                    SEAT_COLORS[seat],
                );
                scroll_track(
                    actors,
                    scene,
                    604.0 + seat as f32 * 5.0,
                    page.scroll_position[seat],
                    SEAT_COLORS[seat],
                );
            }
        }

        let pitch = (LAST_ROW - FIRST_ROW) / page.visible_slots.saturating_sub(1).max(1) as f32;
        for index in 0..page.row_count {
            let Some(row) = (page.row)(index) else {
                continue;
            };
            let row_scene = scene.with_alpha(row.alpha);
            let focused =
                std::array::from_fn(|seat| page.active[seat] && page.selected_rows[seat] == index);
            if row.exit {
                row_panel(actors, row_scene, FOOTER_ROW, 27.0, focused.contains(&true));
                mark_seats(actors, row_scene, FOOTER_ROW, focused);
                let label = row
                    .values
                    .iter()
                    .flatten()
                    .next()
                    .copied()
                    .unwrap_or(row.label);
                row_scene.text(
                    actors,
                    [LEFT + 12.0, FOOTER_ROW],
                    [0.70, WIDTH - 24.0],
                    label.clone(),
                    if focused.contains(&true) {
                        PAPER
                    } else {
                        MUTED
                    },
                );
                continue;
            }

            let y = row.slot.mul_add(pitch, FIRST_ROW);
            let half_extra = row.slots.saturating_sub(1) as f32 * pitch * 0.5;
            if y - half_extra < FIRST_ROW - 0.1 || y + half_extra > LAST_ROW + 0.1 {
                continue;
            }
            row_panel(
                actors,
                row_scene,
                y,
                pitch * row.slots.max(1) as f32 - 5.0,
                focused.contains(&true),
            );
            mark_seats(actors, row_scene, y, focused);
            let label_y = if row.detail.is_some() { y - 5.5 } else { y };
            row_scene.text(
                actors,
                [LEFT + 12.0, label_y],
                [0.68, 189.0],
                row.label.clone(),
                if focused.contains(&true) {
                    PAPER
                } else {
                    MUTED
                },
            );
            if let Some(detail) = row.detail {
                row_scene.text(
                    actors,
                    [LEFT + 12.0, y + 6.0],
                    [0.43, 189.0],
                    detail.clone(),
                    MUTED,
                );
            }

            let mut previews = [0.0; 2];
            for (seat, (left, width)) in value_area.into_iter().enumerate() {
                if !page.active[seat] {
                    continue;
                }
                let accent = SEAT_COLORS[seat];
                previews[seat] = row_scene.xy([left + width * 0.73, y])[0];
                if row.next_row[seat] && focused[seat] {
                    row_scene.text(
                        actors,
                        [left + 6.0, y],
                        [0.57, width - 12.0],
                        "NEXT ROW",
                        accent,
                    );
                } else if let Some(inline) = &row.inline {
                    let chosen = inline.focused[seat].unwrap_or(inline.selected[seat]);
                    draw_choices(
                        actors,
                        row_scene,
                        ChoiceStrip {
                            left,
                            y,
                            width,
                            limit: if both { 2 } else { 4 },
                            accent,
                        },
                        inline.choices.len(),
                        chosen,
                        focused[seat],
                        |index| {
                            let enabled = inline.enabled[seat]
                                .map_or(index == inline.selected[seat], |bits| {
                                    index < u16::BITS as usize && bits & (1_u16 << index) != 0
                                });
                            (inline.choices[index].clone(), enabled)
                        },
                    );
                } else if let Some(value) = row.values[seat] {
                    let width = if row.has_preview {
                        width * 0.44
                    } else {
                        width - 12.0
                    };
                    row_scene.text(
                        actors,
                        [left + 6.0, y],
                        [0.64, width],
                        value.clone(),
                        if focused[seat] { accent } else { PAPER },
                    );
                }
            }
            if row.has_preview {
                (page.draw_preview)(actors, index, previews, row_scene.xy([0.0, y])[1]);
            }
        }
        for seat in 0..page.active.len() {
            if page.active[seat] {
                let (left, width) = if both {
                    (
                        LEFT + seat as f32 * (WIDTH + 8.0) / 2.0,
                        (WIDTH - 8.0) / 2.0,
                    )
                } else {
                    (LEFT, WIDTH)
                };
                help_panel(
                    actors,
                    scene,
                    left,
                    width,
                    (page.help)(seat, 0),
                    (page.help)(seat, 1),
                    "START TO CONTINUE",
                );
            }
        }
    }

    fn entry_transition(&self) -> (Vec<Actor>, f32) {
        (
            vec![page_curtain(PLAYER_ENTRY_SECONDS, true)],
            PLAYER_ENTRY_SECONDS,
        )
    }

    fn exit_transition(&self, _: Screen) -> (Vec<Actor>, f32) {
        (
            vec![page_curtain(PLAYER_EXIT_SECONDS, false)],
            PLAYER_EXIT_SECONDS,
        )
    }
}

fn page_heading(actors: &mut Vec<Actor>, scene: Scene, title: &'static str) {
    scene.text(actors, [LEFT, 54.0], [1.24, WIDTH], title, PAPER);
    scene.rect(actors, [LEFT, 74.0], [WIDTH, 1.0], MINT, 0.75, 0);
}

fn row_panel(actors: &mut Vec<Actor>, scene: Scene, y: f32, height: f32, focused: bool) {
    scene.rect(
        actors,
        [LEFT, y - height * 0.5],
        [WIDTH, height],
        if focused { MINT } else { MUTED },
        if focused { 0.13 } else { 0.07 },
        -1,
    );
}

fn mark_seats(actors: &mut Vec<Actor>, scene: Scene, y: f32, focused: [bool; 2]) {
    for (seat, focused) in focused.into_iter().enumerate() {
        if focused {
            scene.rect(
                actors,
                [LEFT + 4.0 * seat as f32, y - 9.0],
                [3.0, 18.0],
                SEAT_COLORS[seat],
                1.0,
                1,
            );
        }
    }
}

fn scroll_track(actors: &mut Vec<Actor>, scene: Scene, x: f32, fraction: f32, accent: [f32; 3]) {
    let range = FOOTER_ROW - FIRST_ROW + 20.0;
    scene.rect(actors, [x, FIRST_ROW - 10.0], [2.0, range], MUTED, 0.26, 0);
    let thumb_y = fraction
        .clamp(0.0, 1.0)
        .mul_add(range - 16.0, FIRST_ROW - 10.0);
    scene.rect(actors, [x, thumb_y], [2.0, 16.0], accent, 1.0, 1);
}

fn help_panel(
    actors: &mut Vec<Actor>,
    scene: Scene,
    left: f32,
    width: f32,
    description: Option<&Arc<str>>,
    detail: Option<&Arc<str>>,
    hint: &'static str,
) {
    scene.rect(actors, [left, HELP_TOP], [width, 72.0], MUTED, 0.09, 0);
    let x = left + 10.0;
    if let Some(body) = description {
        let height = if detail.is_some() { 30.0 } else { 41.0 };
        scene.paragraph(
            actors,
            [x, HELP_TOP + 7.0],
            [width - 20.0, height],
            0.55,
            body,
            PAPER,
        );
    }
    if let Some(text) = detail {
        scene.text(
            actors,
            [x, HELP_TOP + 46.0],
            [0.48, width - 20.0],
            text,
            MUTED,
        );
    }
    scene.text(
        actors,
        [x, HELP_TOP + 61.0],
        [0.49, width - 20.0],
        hint,
        MINT,
    );
}

struct ChoiceStrip {
    left: f32,
    y: f32,
    width: f32,
    limit: usize,
    accent: [f32; 3],
}

fn draw_choices(
    actors: &mut Vec<Actor>,
    scene: Scene,
    strip: ChoiceStrip,
    count: usize,
    selected: usize,
    focused: bool,
    choice: impl Fn(usize) -> (TextContent, bool),
) {
    let visible = centered_window(count, selected, strip.limit);
    let width = strip.width / visible.len().max(1) as f32;
    for (slot, index) in visible.enumerate() {
        let (label, enabled) = choice(index);
        let x = (slot as f32).mul_add(width, strip.left + 2.0);
        let is_cursor = focused && index == selected;
        if is_cursor {
            scene.rect(
                actors,
                [x, strip.y - 9.0],
                [width - 4.0, 18.0],
                strip.accent,
                0.24,
                1,
            );
        }
        if enabled {
            scene.rect(
                actors,
                [x, strip.y + 7.0],
                [width - 4.0, 2.0],
                strip.accent,
                0.80,
                2,
            );
        }
        scene.text(
            actors,
            [x + 4.0, strip.y - 1.0],
            [0.53, width - 11.0],
            label,
            if enabled || is_cursor { PAPER } else { MUTED },
        );
    }
}

fn centered_window(count: usize, selected: usize, slots: usize) -> Range<usize> {
    let first = selected
        .saturating_sub(slots / 2)
        .min(count.saturating_sub(slots));
    first..(first + slots).min(count)
}

fn page_curtain(duration: f32, arriving: bool) -> Actor {
    const SITE: u64 = runtime::site_base(file!(), line!(), column!());
    let mut actor = SpriteBuilder::solid();
    actor.tweensalt(u64::from(arriving));
    actor.align(0.0, 0.0);
    actor.size(space::screen_width(), space::screen_height());
    let [r, g, b] = PITCH;
    actor.diffuse([r, g, b, f32::from(u8::from(arriving))]);
    actor.z(1100);
    actor.build_tweened(SITE, || {
        let mut steps = TweenSteps::new();
        steps.push(
            anim::linear(duration)
                .alpha(f32::from(u8::from(!arriving)))
                .build(),
        );
        steps
    })
}
