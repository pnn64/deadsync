//! Player Options presentation for the shared editor.

mod help;

use crate::presentation::{
    Layout, LinearMotion, TextMetrics, options_label, push_options_cursor, push_options_heading,
    push_options_highlight, push_options_panel, push_red_backdrop, push_red_streaks, sprite,
};
use crate::resources::MENU_NORMAL_FONT;
use deadlib_assets::AssetView;
use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::anim::bounceend_p;
use deadlib_present::font::FontMap;
use deadlib_present::space::{screen_height, screen_width};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_theme::player_options::{
    PlayerOptionsInline, PlayerOptionsLayout, PlayerOptionsPage, PlayerOptionsRow,
    PlayerOptionsViewport,
};
use deadsync_theme::{Screen, ThemeTransitions, TransitionState};

pub(super) const TRANSITION_SECONDS: f32 = 0.3;
pub(super) const ENTRY_SECONDS: f32 = 0.5;

const FIRST_ROW_Y: f32 = 127.0;
const ROW_PITCH: f32 = 28.0;
const EXIT_Y: f32 = 353.0;
const LABEL_X: f32 = 53.0;
const INLINE_LEFT: f32 = 160.0;
const INLINE_RIGHT: f32 = 594.0;
const ITEM_ZOOM: f32 = 0.62;
const INLINE_ZOOM: f32 = 0.54;
const INK: [f32; 3] = [1.0; 3];
const MUTED: [f32; 3] = [0.78, 0.72, 0.72];
const PLAYER_HUES: [[f32; 3]; MAX_PLAYERS] = [
    [251.0 / 255.0, 190.0 / 255.0, 3.0 / 255.0],
    [86.0 / 255.0, 1.0, 72.0 / 255.0],
];
const PLAYER_NAMES: [&str; MAX_PLAYERS] = ["P1", "P2"];
const SCROLLBAR_X: f32 = 34.0;
const SCROLLBAR_TOP: f32 = 122.0;
const SCROLLBAR_HEIGHT: f32 = 240.0;
const THUMB_WIDTH: f32 = 10.0;
const THUMB_HEIGHT: f32 = 22.0;
const THUMB_SECONDS: f64 = 0.2;

type Page<'a> = PlayerOptionsPage<'a, TextContent, Actor>;
type Row<'a> = PlayerOptionsRow<'a, TextContent>;

#[derive(Clone, Copy)]
struct Seats {
    active: [bool; MAX_PLAYERS],
    values: [f32; MAX_PLAYERS],
    previews: [f32; MAX_PLAYERS],
}

impl Seats {
    fn new(active: [bool; MAX_PLAYERS]) -> Self {
        if active == [true; MAX_PLAYERS] {
            Self {
                active,
                values: [337.0, 515.0],
                previews: [389.0, 557.0],
            }
        } else {
            Self {
                active,
                values: [395.0; MAX_PLAYERS],
                previews: [459.0; MAX_PLAYERS],
            }
        }
    }
}

/// Retained scroll feedback for the mounted Player Options editor.
#[derive(Default)]
pub struct OpenItgPlayerOptions {
    pane: Option<&'static str>,
    entered_at: f64,
    thumbs: [Option<LinearMotion<1>>; MAX_PLAYERS],
    help: help::PlayerHelp,
}

impl PlayerOptionsLayout<Actor, TextContent> for OpenItgPlayerOptions {
    fn viewport(&self) -> PlayerOptionsViewport {
        Self::VIEWPORT
    }

    fn push_page(
        &mut self,
        actors: &mut Vec<Actor>,
        page: Page<'_>,
        assets: &dyn AssetView,
        now: f64,
        _transition: &TransitionState<Screen>,
    ) {
        self.push_actors(actors, page, assets.fonts(), now);
    }

    fn entry_transition(&self) -> (Vec<Actor>, f32) {
        crate::OpenItgTheme::in_transition(Screen::PlayerOptions, ())
    }

    fn exit_transition(&self, target: Screen) -> (Vec<Actor>, f32) {
        crate::OpenItgTheme::out_transition(Screen::PlayerOptions, target)
    }
}

impl OpenItgPlayerOptions {
    pub const VIEWPORT: PlayerOptionsViewport = PlayerOptionsViewport {
        slots: 8,
        separate_exit: true,
        row_tween_seconds: 0.15,
    };

    pub fn push_actors(
        &mut self,
        actors: &mut Vec<Actor>,
        page: Page<'_>,
        fonts: &FontMap,
        now: f64,
    ) {
        let new_pane = self.pane != Some(page.title);
        if new_pane {
            self.pane = Some(page.title);
            self.entered_at = now;
            self.thumbs = [None; MAX_PLAYERS];
        }
        let tops = std::array::from_fn(|player| {
            if !page.active[player] || page.row_count == 0 {
                self.thumbs[player] = None;
                return None;
            }
            let target = [page.scroll_position[player].clamp(0.0, 1.0)];
            let thumb = self.thumbs[player].get_or_insert(LinearMotion::at(target));
            thumb.retarget(target, now, THUMB_SECONDS);
            let [fraction] = thumb.sample(now, THUMB_SECONDS);
            Some((SCROLLBAR_HEIGHT - THUMB_HEIGHT).mul_add(fraction, SCROLLBAR_TOP))
        });
        self.help.update(&page, fonts, new_pane, now);
        push_page(
            actors,
            page,
            tops,
            &self.help,
            TextMetrics::new(fonts, MENU_NORMAL_FONT),
            now,
            (now - self.entered_at) as f32,
        );
    }
}

fn push_page(
    actors: &mut Vec<Actor>,
    page: Page<'_>,
    thumbs: [Option<f32>; MAX_PLAYERS],
    help: &help::PlayerHelp,
    metrics: TextMetrics<'_>,
    now: f64,
    elapsed: f32,
) {
    let (width, height) = (screen_width(), screen_height());
    let layout = Layout::centered(width, height);
    let seats = Seats::new(page.active);
    let pitch = ROW_PITCH * (8.0 / page.visible_slots.max(8) as f32);
    let alpha = page.alpha.clamp(0.0, 1.0);

    push_red_backdrop(actors, [width, height], now.rem_euclid(60.0) as f32, -30);
    push_red_streaks(actors, [width, height], 1.0, -28);
    let panel_bottom = help.panel_bottom();
    let remaining = 1.0 - (elapsed / TRANSITION_SECONDS).clamp(0.0, 1.0);
    let panel = layout.translated_x(-width * remaining * remaining);
    push_options_panel(actors, panel, panel_bottom, alpha);
    draw_heading(actors, layout, page.title, elapsed, alpha);
    draw_seats(actors, layout, seats, elapsed, alpha);

    for index in 0..page.row_count {
        let Some(row) = (page.row)(index) else {
            continue;
        };
        if row.alpha <= 0.001 {
            continue;
        }
        let focused = std::array::from_fn(|player| {
            seats.active[player] && page.selected_rows[player] == index
        });
        if row.exit {
            draw_exit(actors, layout, &row, focused, row.alpha);
            continue;
        }

        let y = row.slot.mul_add(pitch, FIRST_ROW_Y);
        if focused.contains(&true) {
            push_options_highlight(
                actors,
                layout,
                y,
                pitch * row.slots.max(1) as f32 + 2.0,
                row.alpha,
            );
        }
        draw_row_label(actors, layout, y, &row, focused.contains(&true));
        if let Some(inline) = row.inline.as_ref() {
            draw_inline(
                actors, layout, y, pitch, &row, inline, seats, focused, metrics,
            );
        } else {
            draw_values(actors, layout, y, &row, seats, focused, metrics);
        }
        if row.has_preview {
            let [p1_x, _] = layout.position(seats.previews[0], y);
            let [p2_x, preview_y] = layout.position(seats.previews[1], y);
            (page.draw_preview)(actors, index, [p1_x, p2_x], preview_y);
        }
    }
    draw_scrollbar(actors, layout, thumbs, alpha);
    help.push(actors, layout, alpha, now);
    if panel_bottom <= 437.0 {
        actors.push(options_label(
            layout,
            [320.0, 446.0],
            0.5,
            500.0,
            "Choose Exit when ready".into(),
            0.62,
            rgba(INK, alpha),
        ));
    }
}

fn draw_heading(
    actors: &mut Vec<Actor>,
    layout: Layout,
    pane: &'static str,
    elapsed: f32,
    alpha: f32,
) {
    let title = if pane == "PLAYER OPTIONS" {
        "PICK MODIFIERS"
    } else {
        pane
    };
    let t = bounceend_p((elapsed / TRANSITION_SECONDS).clamp(0.0, 1.0));
    push_options_heading(actors, layout, title, [t, (6.0 - 5.0 * t).max(0.0)], alpha);
}

fn draw_seats(actors: &mut Vec<Actor>, layout: Layout, seats: Seats, elapsed: f32, alpha: f32) {
    for (player, (name, icon)) in PLAYER_NAMES
        .into_iter()
        .zip(["openitg/options-seat-p1.png", "openitg/options-seat-p2.png"])
        .enumerate()
    {
        if !seats.active[player] {
            continue;
        }
        let delay = [0.1, 0.2][player];
        let remaining = 1.0 - ((elapsed - delay) / TRANSITION_SECONDS).clamp(0.0, 1.0);
        let layout = layout.translated_x(screen_width() * 0.75 * remaining * remaining);
        let x = seats.values[player];
        actors.push(
            sprite(
                "openitg/options-seat.png",
                layout.position(x, 81.0),
                layout.size([152.0, 34.0]),
                rgba(INK, alpha),
                -22,
            )
            .build(0),
        );
        actors.push(
            sprite(
                icon,
                layout.position(x + 44.0, [77.0, 83.0][player]),
                layout.size([26.0, 8.0]),
                rgba(INK, alpha),
                -21,
            )
            .build(0),
        );
        actors.push(options_label(
            layout,
            [x - 16.0, 79.0],
            0.5,
            90.0,
            name.into(),
            0.52,
            rgba(PLAYER_HUES[player], alpha),
        ));
    }
}

fn draw_scrollbar(
    actors: &mut Vec<Actor>,
    layout: Layout,
    tops: [Option<f32>; MAX_PLAYERS],
    alpha: f32,
) {
    let half = THUMB_WIDTH * 0.5;
    let mut draw = |player: usize, x, top, width| {
        actors.push(layout.panel(
            x,
            top,
            [width, THUMB_HEIGHT],
            rgba(PLAYER_HUES[player], alpha),
            -15,
        ));
    };
    if let [Some(p1), Some(p2)] = tops
        && (p1 - p2).abs() < THUMB_HEIGHT
    {
        // Each player owns their half of the track where the thumbs overlap.
        draw(0, SCROLLBAR_X, p1, half);
        draw(1, SCROLLBAR_X - half, p2, half);
        draw(0, SCROLLBAR_X - half, p1, half);
        draw(1, SCROLLBAR_X, p2, half);
    } else {
        for (player, top) in tops.into_iter().enumerate() {
            if let Some(top) = top {
                draw(player, SCROLLBAR_X - half, top, THUMB_WIDTH);
            }
        }
    }
}

const fn rgba([red, green, blue]: [f32; 3], alpha: f32) -> [f32; 4] {
    [red, green, blue, alpha]
}

fn draw_row_label(actors: &mut Vec<Actor>, layout: Layout, y: f32, row: &Row<'_>, focused: bool) {
    let label_y = if row.detail.is_some() { y - 4.5 } else { y };
    actors.push(options_label(
        layout,
        [LABEL_X, label_y],
        0.0,
        INLINE_LEFT - LABEL_X - 10.0,
        row.label.clone(),
        0.60,
        rgba(if focused { INK } else { MUTED }, row.alpha),
    ));
    if let Some(detail) = row.detail {
        actors.push(options_label(
            layout,
            [LABEL_X, y + 6.0],
            0.0,
            INLINE_LEFT - LABEL_X - 10.0,
            detail.clone(),
            0.42,
            rgba(MUTED, row.alpha),
        ));
    }
}

fn draw_exit(
    actors: &mut Vec<Actor>,
    layout: Layout,
    row: &Row<'_>,
    focused: [bool; MAX_PLAYERS],
    alpha: f32,
) {
    if focused.contains(&true) {
        push_options_highlight(actors, layout, EXIT_Y, 33.0, alpha);
        draw_choice_marks(
            actors,
            layout,
            [320.0, EXIT_Y],
            35.0,
            [false; MAX_PLAYERS],
            focused,
            alpha,
        );
    }
    let label = row
        .values
        .iter()
        .flatten()
        .copied()
        .next()
        .unwrap_or(row.label);
    actors.push(options_label(
        layout,
        [320.0, EXIT_Y],
        0.5,
        400.0,
        label.clone(),
        0.69,
        rgba(INK, alpha),
    ));
}

fn draw_values(
    actors: &mut Vec<Actor>,
    layout: Layout,
    y: f32,
    row: &Row<'_>,
    seats: Seats,
    focused: [bool; MAX_PLAYERS],
    metrics: TextMetrics<'_>,
) {
    for player in 0..MAX_PLAYERS {
        if !seats.active[player] {
            continue;
        }
        let Some(text) = row.values[player] else {
            continue;
        };
        let content_x = seats.values[player];
        let (x, width) = if row.has_preview {
            (content_x - 33.0, 82.0)
        } else {
            (content_x, 150.0)
        };
        if focused[player] && !row.next_row[player] {
            push_options_cursor(
                actors,
                layout,
                [x, y],
                marker_width(metrics, text, width, ITEM_ZOOM),
                player,
                true,
                row.alpha,
            );
        }
        actors.push(options_label(
            layout,
            [x, y],
            0.5,
            width,
            text.clone(),
            ITEM_ZOOM,
            rgba(if focused[player] { INK } else { MUTED }, row.alpha),
        ));
    }
    if row
        .next_row
        .iter()
        .zip(focused)
        .any(|(&next, focused)| next && focused)
    {
        draw_next_row(actors, layout, y, row, focused);
    }
}

fn draw_inline(
    actors: &mut Vec<Actor>,
    layout: Layout,
    y: f32,
    pitch: f32,
    row: &Row<'_>,
    inline: &PlayerOptionsInline<'_, TextContent>,
    seats: Seats,
    focused: [bool; MAX_PLAYERS],
    metrics: TextMetrics<'_>,
) {
    let next_column = row
        .next_row
        .iter()
        .zip(focused)
        .any(|(&next, focused)| next && focused);
    let first = INLINE_LEFT + if next_column { 72.0 } else { 0.0 };
    let count = inline.choices.len();
    if count == 0 {
        if next_column {
            draw_next_row(actors, layout, y, row, focused);
        }
        return;
    }
    let columns = count.div_ceil(row.slots.max(1));
    let width = (INLINE_RIGHT - first) / columns as f32;
    let widest = inline
        .choices
        .iter()
        .map(|text| metrics.width(text.as_str()))
        .fold(1.0, f32::max);
    let zoom = INLINE_ZOOM.min((width - 10.0) / widest);
    if next_column {
        draw_next_row(actors, layout, y, row, focused);
    }
    if zoom < 0.43 {
        draw_compact_choices(actors, layout, y, row, inline, seats, focused, metrics);
        return;
    }

    let lines = count.div_ceil(columns);
    let start_y = y - ((lines - 1) as f32 * pitch * 0.5);
    for (index, choice) in inline.choices.iter().enumerate() {
        let x = (index % columns) as f32 * width + first + width * 0.5;
        let y = (index / columns) as f32 * pitch + start_y;
        let choice_focused = std::array::from_fn(|player| {
            focused[player]
                && !row.next_row[player]
                && inline.focused[player].unwrap_or(inline.selected[player]) == index
        });
        let choice_enabled =
            std::array::from_fn(|player| seats.active[player] && enabled(inline, player, index));
        let tint = if choice_focused.contains(&true) || choice_enabled.contains(&true) {
            INK
        } else {
            MUTED
        };
        actors.push(options_label(
            layout,
            [x, y],
            0.5,
            width - 8.0,
            choice.clone(),
            zoom,
            rgba(tint, row.alpha),
        ));
        if choice_enabled.contains(&true) || choice_focused.contains(&true) {
            draw_choice_marks(
                actors,
                layout,
                [x, y],
                marker_width(metrics, choice, width - 8.0, zoom),
                choice_enabled,
                choice_focused,
                row.alpha,
            );
        }
    }
}

fn draw_compact_choices(
    actors: &mut Vec<Actor>,
    layout: Layout,
    y: f32,
    row: &Row<'_>,
    inline: &PlayerOptionsInline<'_, TextContent>,
    seats: Seats,
    focused: [bool; MAX_PLAYERS],
    metrics: TextMetrics<'_>,
) {
    for player in 0..MAX_PLAYERS {
        if !seats.active[player] {
            continue;
        }
        let focus = focused[player] && !row.next_row[player];
        let choice = (if focus { inline.focused[player] } else { None })
            .unwrap_or(inline.selected[player])
            .min(inline.choices.len() - 1);
        let choice_on = enabled(inline, player, choice);
        let x = seats.values[player];
        actors.push(options_label(
            layout,
            [x, y],
            0.5,
            120.0,
            inline.choices[choice].clone(),
            ITEM_ZOOM,
            rgba(if focus || choice_on { INK } else { MUTED }, row.alpha),
        ));
        if inline.enabled[player].is_some() {
            let status = if choice_on { "ON" } else { "OFF" };
            actors.push(options_label(
                layout,
                [x + 62.0, y],
                0.0,
                22.0,
                status.into(),
                0.43,
                rgba(PLAYER_HUES[player], row.alpha),
            ));
        }
        draw_choice_marks(
            actors,
            layout,
            [x, y],
            marker_width(metrics, &inline.choices[choice], 120.0, ITEM_ZOOM),
            std::array::from_fn(|side| side == player && choice_on),
            std::array::from_fn(|side| side == player && focus),
            row.alpha,
        );
    }
}

fn enabled(inline: &PlayerOptionsInline<'_, TextContent>, player: usize, choice: usize) -> bool {
    match inline.enabled[player] {
        Some(mask) => choice < u16::BITS as usize && mask & (1_u16 << choice) != 0,
        None => inline.selected[player] == choice,
    }
}

fn draw_next_row(
    actors: &mut Vec<Actor>,
    layout: Layout,
    y: f32,
    row: &Row<'_>,
    focused: [bool; MAX_PLAYERS],
) {
    let position = [235.0, y];
    actors.push(options_label(
        layout,
        position,
        0.5,
        50.0,
        "NEXT ROW".into(),
        0.50,
        rgba(INK, row.alpha),
    ));
    draw_choice_marks(
        actors,
        layout,
        position,
        46.0,
        [false; MAX_PLAYERS],
        std::array::from_fn(|player| focused[player] && row.next_row[player]),
        row.alpha,
    );
}

fn draw_choice_marks(
    actors: &mut Vec<Actor>,
    layout: Layout,
    [x, y]: [f32; 2],
    width: f32,
    enabled: [bool; MAX_PLAYERS],
    focused: [bool; MAX_PLAYERS],
    alpha: f32,
) {
    let overlapping = focused == [true; MAX_PLAYERS];
    for player in 0..MAX_PLAYERS {
        if focused[player] {
            let shift = if overlapping {
                [-4.0, 4.0][player]
            } else {
                0.0
            };
            push_options_cursor(actors, layout, [x, y + shift], width, player, true, alpha);
        }
        if enabled[player] {
            push_options_cursor(actors, layout, [x, y], width, player, false, alpha);
        }
    }
}

fn marker_width(metrics: TextMetrics<'_>, choice: &TextContent, max_width: f32, zoom: f32) -> f32 {
    (metrics.width(choice.as_str()) * zoom)
        .min(max_width)
        .max(8.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::space::{Metrics, with_current_metrics};
    use std::cell::Cell;
    use std::sync::Arc;

    fn row(label: &TextContent) -> Row<'_> {
        Row {
            label,
            detail: None,
            slot: 0.0,
            slots: 1,
            alpha: 1.0,
            exit: false,
            values: [None; MAX_PLAYERS],
            next_row: [false; MAX_PLAYERS],
            inline: None,
            has_preview: false,
        }
    }

    fn has_text(actors: &[Actor], text: &str, position: [f32; 2]) -> bool {
        actors.iter().any(|actor| matches!(actor,
            Actor::Text { content, offset, .. } if content.as_str() == text && *offset == position
        ))
    }

    fn has_mark(
        actors: &[Actor],
        position: [f32; 2],
        key: &str,
        player: usize,
        alpha: f32,
    ) -> bool {
        use deadlib_present::actors::SpriteSource;
        let v = player as f32 * 0.5;
        actors.iter().any(|actor| {
            matches!(actor,
                Actor::Sprite { source: SpriteSource::TextureStatic(texture), offset, tint, uv_rect, .. }
                    if *texture == key && *offset == position && tint[3] == alpha
                        && *uv_rect == Some([1.0 / 3.0, v, 2.0 / 3.0, v + 0.5])
            )
        })
    }

    #[test]
    fn explanations_keep_their_layout_and_reveal_independently() {
        use deadlib_present::actors::{TextAlign, TextAttributes};
        use deadlib_present::compose::measure_wrapped_text;

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let font = deadlib_assets::parse_font_with_asset_context(
            &root.join("fonts/openitg/_eurostile normal.ini"),
            vec![root.clone()],
        )
        .expect("bundled font");
        let mut assets = deadlib_assets::AssetManager::<()>::new();
        assets.register_font(MENU_NORMAL_FONT, font.font);
        let fonts = assets.fonts();
        let font = fonts.get(MENU_NORMAL_FONT).unwrap();
        let first = [
            Arc::<str>::from("Choose an option."),
            Arc::<str>::from("Choose an option."),
        ];
        let second = [
            Arc::<str>::from(
                "A long line with many words that will wrap in the right column. ".repeat(5),
            ),
            Arc::<str>::from("A final translated line."),
            Arc::<str>::from("A different final line."),
        ];
        for (width, delta) in [(640.0, 0.0), (854.0, 107.0)] {
            with_current_metrics(Metrics::centered(width, 480.0), || {
                let mut presentation = OpenItgPlayerOptions::default();
                let mut actors = Vec::new();
                let mut frame = |title, selection: [usize; 2], now| {
                    actors.clear();
                    presentation.push_actors(
                        &mut actors,
                        PlayerOptionsPage {
                            title,
                            active: [true, true],
                            selected_rows: selection,
                            scroll_position: [0.0; 2],
                            row_count: 2,
                            visible_slots: 8,
                            alpha: 1.0,
                            row: &|_| None,
                            help: &|player, line| match (player, line) {
                                (0, 0) => first.get(selection[0]),
                                (1, 0) => Some(&second[0]),
                                (1, 1) => second.get(selection[1] + 1),
                                _ => None,
                            },
                            draw_preview: &|_, _, _, _| {},
                        },
                        fonts,
                        now,
                    );
                    let mut reveal = [0.0; 2];
                    let mut seen = 0;
                    for actor in &actors {
                        if let Actor::Text {
                            offset,
                            align,
                            align_text,
                            content,
                            scale,
                            max_height,
                            wrap_width_pixels,
                            attributes,
                            ..
                        } = actor
                            && offset[1] == 381.0
                        {
                            let right = seen == 1;
                            assert_eq!(offset[0], if right { 568.0 } else { 68.0 } + delta);
                            assert_eq!(*align, [if right { 1.0 } else { 0.0 }, 0.0]);
                            assert_eq!(
                                *align_text,
                                if right {
                                    TextAlign::Right
                                } else {
                                    TextAlign::Left
                                }
                            );
                            assert_eq!(scale[0], scale[1]);
                            assert!(max_height.is_none());
                            let [w, h] = measure_wrapped_text(
                                font,
                                fonts,
                                content.as_str(),
                                wrap_width_pixels.unwrap(),
                            );
                            assert!(
                                w as f32 * scale[0] <= 250.001 && h as f32 * scale[1] <= 70.001
                            );
                            if right {
                                assert!(content.as_str().contains('\n'));
                                assert!(scale[0] < 0.54);
                            }
                            reveal[seen] = match attributes {
                                TextAttributes::GlyphReveal(fraction) => *fraction,
                                _ => 1.0,
                            };
                            seen += 1;
                        }
                    }
                    assert_eq!(seen, 2);
                    reveal
                };

                assert_eq!(frame("PLAYER OPTIONS", [0, 0], 0.0), [0.0, 0.0]);
                assert_eq!(frame("PLAYER OPTIONS", [1, 0], 0.25), [0.5, 0.5]);
                assert_eq!(frame("PLAYER OPTIONS", [1, 1], 0.375), [0.75, 0.0]);
                assert_eq!(frame("PLAYER OPTIONS", [1, 1], 0.875), [1.0, 1.0]);
                assert_eq!(frame("DISPLAY OPTIONS", [1, 1], 1.0), [0.0, 0.0]);
            });
        }
    }

    #[test]
    fn overlapping_thumbs_keep_both_seats_visible_and_retarget_independently() {
        use deadlib_present::actors::SizeSpec;

        with_current_metrics(Metrics::centered(854.0, 480.0), || {
            let mut presentation = OpenItgPlayerOptions::default();
            let mut actors = Vec::new();
            let mut frame = |title, active, scroll_position, now| {
                actors.clear();
                presentation.push_actors(
                    &mut actors,
                    PlayerOptionsPage {
                        title,
                        active,
                        selected_rows: [0; MAX_PLAYERS],
                        scroll_position,
                        row_count: 16,
                        visible_slots: 8,
                        alpha: 1.0,
                        row: &|_| None,
                        help: &|_, _| None,
                        draw_preview: &|_, _, _, _| {},
                    },
                    &FontMap::default(),
                    now,
                );
                actors
                    .iter()
                    .filter_map(|actor| match actor {
                        Actor::Sprite {
                            offset,
                            size: [SizeSpec::Px(width), SizeSpec::Px(22.0)],
                            tint,
                            ..
                        } => Some((*offset, *width, *tint)),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            };

            let p1 = rgba(PLAYER_HUES[0], 1.0);
            let p2 = rgba(PLAYER_HUES[1], 1.0);
            let both = [true, true];
            let overlap = frame("PLAYER OPTIONS", both, [0.0, 0.0], 0.0);
            assert_eq!(
                overlap[2..],
                [([136.0, 122.0], 5.0, p1), ([141.0, 122.0], 5.0, p2)]
            );
            assert_eq!(frame("PLAYER OPTIONS", both, [0.0, 1.0], 1.0), overlap);
            let halfway = frame("PLAYER OPTIONS", both, [0.0, 1.0], 1.1);
            assert_eq!(
                halfway,
                [([136.0, 122.0], 10.0, p1), ([136.0, 231.0], 10.0, p2)]
            );
            assert_eq!(frame("PLAYER OPTIONS", both, [0.0, 0.25], 1.1), halfway);
            assert_eq!(
                frame("PLAYER OPTIONS", both, [0.0, 0.25], 1.4)[1],
                ([136.0, 176.5], 10.0, p2)
            );
            assert_eq!(
                frame("DISPLAY OPTIONS", [false, true], [0.0, 1.0], 1.41),
                [([136.0, 340.0], 10.0, p2)]
            );
        });
    }

    #[test]
    fn p2_solo_wide_layout_retains_prepared_values_exit_help_and_preview_anchor() {
        let speed = TextContent::from("Scroll Speed");
        let speed_value = TextContent::inline_str("2.50x").unwrap();
        let skin = TextContent::from("Noteskin");
        let skin_value = TextContent::from("Leaf");
        let exit = TextContent::from("Exit");
        let help = Arc::<str>::from("Choose the noteskin for this player.");
        for (width, delta) in [(640.0, 0.0), (854.0, 107.0)] {
            with_current_metrics(Metrics::centered(width, 480.0), || {
                let mut actors = Vec::new();
                let preview = Cell::new(None);
                OpenItgPlayerOptions::default().push_actors(
                    &mut actors,
                    PlayerOptionsPage {
                        title: "PLAYER OPTIONS",
                        active: [false, true],
                        selected_rows: [0, 2],
                        scroll_position: [0.0, 2.0 / 3.0],
                        row_count: 4,
                        visible_slots: 10,
                        alpha: 1.0,
                        row: &|index| match index {
                            0 => Some(Row {
                                values: [None, Some(&speed_value)],
                                ..row(&speed)
                            }),
                            1 => None,
                            2 => Some(Row {
                                slot: 5.0,
                                values: [None, Some(&skin_value)],
                                has_preview: true,
                                ..row(&skin)
                            }),
                            3 => Some(Row {
                                slot: 9.0,
                                exit: true,
                                ..row(&exit)
                            }),
                            _ => None,
                        },
                        help: &|player, line| (player == 1 && line == 0).then_some(&help),
                        draw_preview: &|_, index, xs, y| preview.set(Some((index, xs, y))),
                    },
                    &FontMap::default(),
                    0.0,
                );
                assert!(has_text(&actors, "PICK MODIFIERS", [36.0 + delta, 38.0]));
                let arriving_seat = [379.0 + delta + width * 0.75, 79.0];
                assert!(has_text(&actors, "P2", arriving_seat));
                assert!(!has_text(&actors, "P1", arriving_seat));
                assert!(has_text(&actors, "Scroll Speed", [53.0 + delta, 127.0]));
                assert!(has_text(&actors, "2.50x", [395.0 + delta, 127.0]));
                assert!(has_text(&actors, "Leaf", [362.0 + delta, 239.0]));
                assert!(has_text(&actors, "Exit", [320.0 + delta, 353.0]));
                assert!(has_text(&actors, &help, [568.0 + delta, 381.0]));
                assert_eq!(preview.get(), Some((2, [459.0 + delta; 2], 239.0)));
            });
        }
    }

    #[test]
    fn versus_inline_focus_and_enabled_masks_are_independent() {
        let scroll = TextContent::from("Scroll");
        let choices = ["Normal", "Reverse", "Split"].map(TextContent::from);
        with_current_metrics(Metrics::centered(640.0, 480.0), || {
            let mut actors = Vec::new();
            OpenItgPlayerOptions::default().push_actors(
                &mut actors,
                PlayerOptionsPage {
                    title: "ADVANCED OPTIONS",
                    active: [true, true],
                    selected_rows: [0, 0],
                    scroll_position: [0.0, 0.0],
                    row_count: 1,
                    visible_slots: 8,
                    alpha: 1.0,
                    row: &|_| {
                        Some(Row {
                            slot: 0.5,
                            slots: 2,
                            inline: Some(PlayerOptionsInline {
                                choices: &choices,
                                focused: [Some(1), Some(2)],
                                selected: [1, 0],
                                enabled: [Some(0b101), Some(0b010)],
                            }),
                            ..row(&scroll)
                        })
                    },
                    help: &|_, _| None,
                    draw_preview: &|_, _, _, _| {
                        panic!("inline text row does not request a preview")
                    },
                },
                &FontMap::default(),
                0.0,
            );
            assert!(has_text(&actors, "Normal", [268.5, 127.0]));
            assert!(has_text(&actors, "Reverse", [485.5, 127.0]));
            assert!(has_text(&actors, "Split", [268.5, 155.0]));
            let underline = "openitg/options-underline.png";
            let cursor = "openitg/options-cursor.png";
            assert!(has_mark(&actors, [268.5, 127.0], underline, 0, 1.0));
            assert!(has_mark(&actors, [268.5, 155.0], underline, 0, 1.0));
            assert!(has_mark(&actors, [485.5, 127.0], underline, 1, 1.0));
            assert!(has_mark(&actors, [485.5, 127.0], cursor, 0, 1.0));
            assert!(has_mark(&actors, [268.5, 155.0], cursor, 1, 1.0));
            assert!(!has_mark(&actors, [485.5, 127.0], underline, 0, 1.0));
        });
    }

    #[test]
    fn arcade_next_row_retains_values_during_the_pane_fade() {
        let rate = TextContent::from("Music Rate");
        let bpm = TextContent::from("180 BPM");
        let rate_value = TextContent::from("1.50x");
        with_current_metrics(Metrics::centered(640.0, 480.0), || {
            let mut actors = Vec::new();
            OpenItgPlayerOptions::default().push_actors(
                &mut actors,
                PlayerOptionsPage {
                    title: "DISPLAY OPTIONS",
                    active: [true, true],
                    selected_rows: [0, 0],
                    scroll_position: [0.0, 0.0],
                    row_count: 1,
                    visible_slots: 10,
                    alpha: 0.5,
                    row: &|_| {
                        Some(Row {
                            detail: Some(&bpm),
                            alpha: 0.25,
                            values: [Some(&rate_value), None],
                            next_row: [true, false],
                            ..row(&rate)
                        })
                    },
                    help: &|_, _| None,
                    draw_preview: &|_, _, _, _| {},
                },
                &FontMap::default(),
                0.0,
            );
            assert!(has_text(&actors, "Music Rate", [53.0, 122.5]));
            assert!(has_text(&actors, "180 BPM", [53.0, 133.0]));
            assert!(has_text(&actors, "1.50x", [337.0, 127.0]));
            assert!(has_text(&actors, "NEXT ROW", [235.0, 127.0]));
            assert!(has_mark(
                &actors,
                [235.0, 127.0],
                "openitg/options-cursor.png",
                0,
                0.25
            ));
            assert!(actors.iter().any(|actor| matches!(actor,
                Actor::Text { content, color, .. }
                    if content.as_str() == "DISPLAY OPTIONS" && color[3] == 0.5
            )));
        });
    }
}
