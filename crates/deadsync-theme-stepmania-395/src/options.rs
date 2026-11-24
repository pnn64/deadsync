use deadlib_assets::AssetManager;
use deadlib_present::actors::{Actor, TextAlign, TextContent};
use deadlib_present::font::{FontMap, measure_line_width_logical};
use deadsync_theme::player_options::{
    PlayerOptionsLayout, PlayerOptionsPage, PlayerOptionsViewport,
};
use deadsync_theme::system_options::{SystemOptionsLayout, SystemOptionsPresentation};
use deadsync_theme::views::{
    SystemOptionsPage, SystemOptionsPhase, SystemOptionsRenderer, SystemOptionsTiming,
};
use deadsync_theme::{Screen, TransitionState};

use crate::presentation::{Layout, push_footer};
use crate::resources::{ITEM_FONT, WHEEL_FONT};

const ROW_Y: f32 = 88.0;
const ROW_PITCH: f32 = 34.0;
const SLOTS: usize = 10;

#[derive(Default)]
pub struct StepManiaSystemOptions;

impl SystemOptionsPresentation<Actor> for StepManiaSystemOptions {
    fn layout(&self) -> SystemOptionsLayout {
        SystemOptionsLayout {
            split_graphics: true,
            timing: Some(SystemOptionsTiming {
                service_seconds: 0.5,
                settings_seconds: 0.5,
            }),
            ..Default::default()
        }
    }

    fn reset(&mut self) {}

    fn with_renderer(
        &mut self,
        phase: SystemOptionsPhase,
        _now_seconds: f64,
        assets: &AssetManager,
        draw: &mut dyn FnMut(SystemOptionsRenderer<'_, Actor>),
    ) {
        let mut page = |actors: &mut Vec<Actor>, page: SystemOptionsPage<'_>| {
            push_system_page(actors, page, assets.fonts());
        };
        draw(SystemOptionsRenderer {
            phase,
            background: &push_background,
            page: &mut page,
        });
    }
}

fn push_background(actors: &mut Vec<Actor>) {
    actors.push(
        Layout::current()
            .sprite(
                "stepmania-395/music-background",
                [320.0, 240.0],
                [640.0, 480.0],
                -100,
            )
            .build(0),
    );
}

fn page_layout(phase: SystemOptionsPhase) -> Layout {
    let shift = match phase {
        SystemOptionsPhase::Entering(elapsed) => {
            -640.0 * (1.0 - elapsed / 0.3).clamp(0.0, 1.0).powi(2)
        }
        SystemOptionsPhase::Leaving(elapsed) => 640.0 * (elapsed / 0.3).clamp(0.0, 1.0).powi(2),
        SystemOptionsPhase::Steady => 0.0,
    };
    Layout::current().translated(shift)
}

fn push_panel(actors: &mut Vec<Actor>, layout: Layout) {
    actors.push(
        layout
            .sprite(
                "stepmania-395/options-page",
                [320.0, 240.0],
                [640.0, 480.0],
                -10,
            )
            .build(0),
    );
}

fn push_system_page(actors: &mut Vec<Actor>, page: SystemOptionsPage<'_>, fonts: &FontMap) {
    let layout = page_layout(page.phase);
    push_panel(actors, layout);
    let first = page
        .selected
        .saturating_sub(SLOTS / 2)
        .min((page.row_count + 1).saturating_sub(SLOTS));
    for index in first..(first + SLOTS).min(page.row_count + 1) {
        let y = ROW_Y + (index - first) as f32 * ROW_PITCH;
        if index == page.row_count {
            push_exit(actors, layout, y, [page.selected == index, false], fonts);
            continue;
        }
        let Some(row) = (page.row)(index) else {
            continue;
        };
        let selected = page.selected == index;
        let shade = if row.disabled {
            0.1
        } else if selected {
            1.0
        } else {
            0.5
        };
        let Some(choices) = row.choices.as_deref().filter(|values| !values.is_empty()) else {
            let width = text_width(fonts, ITEM_FONT, &row.label) * 0.5;
            let mut label = layout.text(ITEM_FONT, owned(&row.label), [340.0, y], 0.5);
            label.diffuse([shade, shade, shade, 1.0]);
            label.maxwidth(430.0 * layout.scale);
            if selected {
                push_cursor(actors, layout, [340.0, y], width.min(430.0), 0, false);
            }
            actors.push(label.build(0));
            continue;
        };
        push_label(actors, layout, &row.label, y, selected, 1.0);
        let total = choices
            .iter()
            .map(|value| text_width(fonts, ITEM_FONT, value) * 0.5 + 14.0)
            .sum::<f32>()
            - 14.0;
        if row.inline && total <= 440.0 {
            let mut left = 160.0;
            for (choice, value) in choices.iter().enumerate() {
                let width = text_width(fonts, ITEM_FONT, value) * 0.5;
                let x = left + width * 0.5;
                let enabled = row
                    .enabled_choices
                    .map_or(choice == row.selected_choice, |bits| {
                        choice < 64 && bits & (1 << choice) != 0
                    });
                if enabled {
                    push_cursor(actors, layout, [x, y], width, 0, true);
                }
                if selected && choice == row.selected_choice {
                    push_cursor(actors, layout, [x, y], width, 0, false);
                }
                push_value(
                    actors,
                    layout,
                    value,
                    [x, y],
                    width.max(1.0),
                    if enabled { 1.0 } else { shade },
                    1.0,
                );
                left += width + 14.0;
            }
        } else if let Some(value) = choices.get(row.selected_choice) {
            let width = (text_width(fonts, ITEM_FONT, value) * 0.5).min(420.0);
            push_cursor(actors, layout, [340.0, y], width, 0, true);
            if selected {
                push_cursor(actors, layout, [340.0, y], width, 0, false);
            }
            push_value(actors, layout, value, [340.0, y], 420.0, shade, 1.0);
        }
    }
    if let Some(help) = page.description.as_deref() {
        push_help(actors, Layout::current(), help, 0, true);
    }
    push_footer(
        actors,
        "&UP; &DOWN; change line     &LEFT; &RIGHT; change value",
        1.0,
    );
}

#[derive(Default)]
pub struct StepManiaPlayerOptions {
    pane: Option<&'static str>,
    entered: f64,
}

impl PlayerOptionsLayout<Actor, TextContent> for StepManiaPlayerOptions {
    fn viewport(&self) -> PlayerOptionsViewport {
        PlayerOptionsViewport {
            slots: SLOTS as u8,
            separate_exit: false,
            row_tween_seconds: 0.3,
        }
    }

    fn push_page(
        &mut self,
        actors: &mut Vec<Actor>,
        page: PlayerOptionsPage<'_, TextContent, Actor>,
        assets: &AssetManager,
        now: f64,
        transition: &TransitionState<Screen>,
    ) {
        if self.pane != Some(page.title) {
            self.pane = Some(page.title);
            self.entered = now;
        }
        push_background(actors);
        let phase = match transition {
            TransitionState::ActorsFadeOut { elapsed, .. }
            | TransitionState::FadingOut { elapsed, .. } => SystemOptionsPhase::Leaving(*elapsed),
            _ => SystemOptionsPhase::Entering((now - self.entered).max(0.0) as f32),
        };
        let layout = page_layout(phase);
        push_panel(actors, layout);
        let fonts = assets.fonts();
        for index in 0..page.row_count {
            let Some(row) = (page.row)(index) else {
                continue;
            };
            if row.alpha <= 0.001 {
                continue;
            }
            let y = ROW_Y + row.slot * ROW_PITCH;
            let focused =
                std::array::from_fn(|seat| page.active[seat] && page.selected_rows[seat] == index);
            if row.exit {
                push_exit(actors, layout, y, focused, fonts);
                continue;
            }
            push_label(
                actors,
                layout,
                row.label.as_str(),
                y,
                focused.contains(&true),
                row.alpha,
            );
            let preview_centers = [260.0, 420.0];
            if let Some(inline) = row.inline.as_ref().filter(|inline| {
                inline
                    .choices
                    .iter()
                    .map(|value| text_width(fonts, ITEM_FONT, value.as_str()) * 0.5 + 14.0)
                    .sum::<f32>()
                    - 14.0
                    <= 440.0
            }) {
                let mut left = 160.0;
                for (choice, value) in inline.choices.iter().enumerate() {
                    let width = text_width(fonts, ITEM_FONT, value.as_str()) * 0.5;
                    let x = left + width * 0.5;
                    let mut selected = false;
                    for seat in 0..2 {
                        if !page.active[seat] {
                            continue;
                        }
                        let enabled = inline.enabled[seat]
                            .map_or(inline.selected[seat] == choice, |bits| {
                                choice < 16 && bits & (1 << choice) != 0
                            });
                        selected |= enabled;
                        if enabled {
                            push_cursor(actors, layout, [x, y], width, seat, true);
                        }
                        if focused[seat] && inline.focused[seat] == Some(choice) {
                            push_cursor(actors, layout, [x, y], width, seat, false);
                        }
                    }
                    push_value(
                        actors,
                        layout,
                        value.as_str(),
                        [x, y],
                        width.max(1.0),
                        if selected { 1.0 } else { 0.5 },
                        row.alpha,
                    );
                    left += width + 14.0;
                }
            } else {
                for seat in 0..2 {
                    if !page.active[seat] {
                        continue;
                    }
                    let Some(value) = row.values[seat] else {
                        continue;
                    };
                    let x = preview_centers[seat];
                    let limit = if page.active == [true, true] {
                        145.0
                    } else {
                        420.0
                    };
                    let width = (text_width(fonts, ITEM_FONT, value.as_str()) * 0.5).min(limit);
                    push_cursor(actors, layout, [x, y], width, seat, true);
                    if focused[seat] {
                        push_cursor(actors, layout, [x, y], width, seat, false);
                    }
                    push_value(
                        actors,
                        layout,
                        value.as_str(),
                        [x, y],
                        limit,
                        if focused[seat] { 1.0 } else { 0.5 },
                        row.alpha,
                    );
                }
            }
            if row.has_preview {
                let positions = preview_centers.map(|x| layout.position([x + 60.0, y]));
                (page.draw_preview)(
                    actors,
                    index,
                    [positions[0][0], positions[1][0]],
                    positions[0][1],
                );
            }
        }
        for seat in 0..2 {
            if page.active[seat]
                && let Some(help) = (page.help)(seat, page.selected_rows[seat])
            {
                push_help(actors, Layout::current(), help, seat, false);
            }
        }
        push_footer(
            actors,
            "&UP; &DOWN; change line     &LEFT; &RIGHT; change value    START to continue",
            page.alpha,
        );
    }

    fn entry_transition(&self) -> (Vec<Actor>, f32) {
        (Vec::new(), 0.5)
    }
    fn exit_transition(&self, _target: Screen) -> (Vec<Actor>, f32) {
        (Vec::new(), 0.5)
    }
}

fn push_label(
    actors: &mut Vec<Actor>,
    layout: Layout,
    value: &str,
    y: f32,
    focused: bool,
    alpha: f32,
) {
    let value = match value {
        "Default Scroll Speed" => "Speed",
        "Default Scroll Direction" => "Scroll",
        "Default Background Filter" => "Filter",
        "Default NoteSkin" => "NoteSkins",
        _ => value,
    };
    let mut label = layout.text(WHEEL_FONT, owned(value), [140.0, y], 0.7);
    label.horizalign(TextAlign::Right);
    label.align(1.0, 0.5);
    label.maxwidth(90.0 * layout.scale);
    let shade = if focused { 1.0 } else { 0.5 };
    label.diffuse([shade, shade, shade, alpha]);
    actors.push(label.build(0));
}

fn push_value(
    actors: &mut Vec<Actor>,
    layout: Layout,
    value: &str,
    at: [f32; 2],
    width: f32,
    shade: f32,
    alpha: f32,
) {
    let mut text = layout.text(ITEM_FONT, owned(value), at, 0.5);
    text.maxwidth(width * layout.scale);
    text.diffuse([shade, shade, shade, alpha]);
    actors.push(text.build(0));
}

fn push_exit(actors: &mut Vec<Actor>, layout: Layout, y: f32, focused: [bool; 2], fonts: &FontMap) {
    let width = text_width(fonts, ITEM_FONT, "EXIT") * 0.5;
    for (seat, active) in focused.into_iter().enumerate() {
        if active {
            push_cursor(actors, layout, [340.0, y], width, seat, false);
        }
    }
    push_value(
        actors,
        layout,
        "EXIT",
        [340.0, y],
        440.0,
        if focused.contains(&true) { 1.0 } else { 0.5 },
        1.0,
    );
}

fn push_cursor(
    actors: &mut Vec<Actor>,
    layout: Layout,
    [x, y]: [f32; 2],
    width: f32,
    seat: usize,
    underline: bool,
) {
    let width = (width * 0.5).ceil() * 2.0;
    let key = if underline {
        "stepmania-395/options-underline"
    } else {
        "stepmania-395/options-cursor"
    };
    for (piece, center, size) in [
        (0, x - width * 0.5 - 7.0, 14.0),
        (1, x, width),
        (2, x + width * 0.5 + 7.0, 14.0),
    ] {
        let mut sprite = layout.sprite(
            key,
            [center, y],
            [size, 36.0],
            if underline { 3 } else { 0 },
        );
        sprite.customtexturerect([
            piece as f32 / 3.0,
            seat as f32 / 2.0,
            (piece + 1) as f32 / 3.0,
            (seat + 1) as f32 / 2.0,
        ]);
        actors.push(sprite.build(0));
    }
}

fn push_help(actors: &mut Vec<Actor>, layout: Layout, value: &str, seat: usize, together: bool) {
    let mut help = layout.text(
        ITEM_FONT,
        owned(value),
        [if seat == 0 { 70.0 } else { 570.0 }, 412.0],
        0.5,
    );
    help.horizalign(if seat == 0 {
        TextAlign::Left
    } else {
        TextAlign::Right
    });
    help.align(if seat == 0 { 0.0 } else { 1.0 }, 0.5);
    help.wrapwidthpixels(if together { 1000.0 } else { 500.0 });
    help.z(5);
    actors.push(help.build(0));
}

fn text_width(fonts: &FontMap, name: &'static str, value: &str) -> f32 {
    fonts.get(name).map_or(0.0, |font| {
        measure_line_width_logical(font, value, fonts) as f32
    })
}

fn owned(value: &str) -> TextContent {
    TextContent::inline_str(value).unwrap_or_else(|| TextContent::Owned(value.to_owned()))
}
