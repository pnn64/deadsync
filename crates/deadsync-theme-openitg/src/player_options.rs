//! Player Options presentation for the shared editor.

mod help;

use crate::presentation::{ACCENT, HEIGHT, INK, Layout, LinearMotion, MUTED, WIDTH, quad};
use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::font::FontMap;
use deadlib_present::space::{screen_height, screen_width};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_theme::player_options::{
    PlayerOptionsInline, PlayerOptionsPage, PlayerOptionsRow, PlayerOptionsViewport,
};

pub(super) const TRANSITION_SECONDS: f32 = 0.3;

const FIRST_ROW_Y: f32 = 127.0;
const ROW_PITCH: f32 = 28.0;
const EXIT_Y: f32 = 353.0;
const ROW_X: f32 = 42.0;
const ROW_WIDTH: f32 = 562.0;
const LABEL_X: f32 = 53.0;
const INLINE_LEFT: f32 = 210.0;
const INLINE_RIGHT: f32 = 594.0;
const PLAYER_HUES: [[f32; 3]; MAX_PLAYERS] = [ACCENT, [0.48, 0.94, 0.51]];
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
    thumbs: [Option<LinearMotion<1>>; MAX_PLAYERS],
    help: help::PlayerHelp,
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
        push_page(actors, page, tops, &self.help, now);
    }
}

fn push_page(
    actors: &mut Vec<Actor>,
    page: Page<'_>,
    thumbs: [Option<f32>; MAX_PLAYERS],
    help: &help::PlayerHelp,
    now: f64,
) {
    let (width, height) = (screen_width(), screen_height());
    let layout = Layout::centered(width, height);
    let seats = Seats::new(page.active);
    let pitch = ROW_PITCH * (8.0 / page.visible_slots.max(8) as f32);
    let alpha = page.alpha.clamp(0.0, 1.0);

    actors.push(quad(
        [0.0, 0.0],
        [width, height],
        [0.035, 0.048, 0.072, 1.0],
        -20,
    ));
    actors.push(layout.panel(0.0, 0.0, [WIDTH, HEIGHT], [0.063, 0.087, 0.126, 1.0], -19));
    actors.push(layout.label([36.0, 38.0], 0.0, page.title.into(), 1.8, rgba(INK, alpha)));
    actors.push(layout.panel(
        36.0,
        70.0,
        [568.0, 1.0],
        rgba([0.32, 0.40, 0.48], alpha),
        -17,
    ));
    for (player, name) in PLAYER_NAMES.into_iter().enumerate() {
        if seats.active[player] {
            actors.push(layout.label(
                [seats.values[player], 92.0],
                0.5,
                name.into(),
                1.1,
                rgba(PLAYER_HUES[player], alpha),
            ));
        }
    }

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
        let height = pitch * row.slots.max(1) as f32 - 2.0;
        let fill = if focused.contains(&true) {
            [0.14, 0.19, 0.27]
        } else {
            [0.09, 0.13, 0.19]
        };
        actors.push(layout.panel(
            ROW_X,
            y - height * 0.5,
            [ROW_WIDTH, height],
            rgba(fill, row.alpha),
            -18,
        ));
        draw_row_focus(actors, layout, y, height, focused, row.alpha);
        draw_row_label(actors, layout, y, &row, focused.contains(&true));
        if let Some(inline) = row.inline.as_ref() {
            draw_inline(actors, layout, y, pitch, &row, inline, seats, focused);
        } else {
            draw_values(actors, layout, y, &row, seats, focused);
        }
        if row.has_preview {
            let [p1_x, _] = layout.position(seats.previews[0], y);
            let [p2_x, preview_y] = layout.position(seats.previews[1], y);
            (page.draw_preview)(actors, index, [p1_x, p2_x], preview_y);
        }
    }
    draw_scrollbar(actors, layout, thumbs, alpha);
    help.push(actors, layout, alpha, now);
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
    actors.push(layout.bounded_label(
        [LABEL_X, label_y],
        0.0,
        INLINE_LEFT - LABEL_X - 10.0,
        row.label.clone(),
        0.94,
        rgba(if focused { INK } else { MUTED }, row.alpha),
    ));
    if let Some(detail) = row.detail {
        actors.push(layout.bounded_label(
            [LABEL_X, y + 6.0],
            0.0,
            INLINE_LEFT - LABEL_X - 10.0,
            detail.clone(),
            0.66,
            rgba(MUTED, row.alpha),
        ));
    }
}

fn draw_row_focus(
    actors: &mut Vec<Actor>,
    layout: Layout,
    y: f32,
    height: f32,
    focused: [bool; MAX_PLAYERS],
    alpha: f32,
) {
    for (player, x) in [ROW_X, ROW_X + ROW_WIDTH - 3.0].into_iter().enumerate() {
        if focused[player] {
            actors.push(layout.panel(
                x,
                y - height * 0.5,
                [3.0, height],
                rgba(PLAYER_HUES[player], alpha),
                -16,
            ));
        }
    }
}

fn draw_exit(
    actors: &mut Vec<Actor>,
    layout: Layout,
    row: &Row<'_>,
    focused: [bool; MAX_PLAYERS],
    alpha: f32,
) {
    let fill = if focused.contains(&true) {
        [0.22, 0.30, 0.39]
    } else {
        [0.10, 0.15, 0.22]
    };
    actors.push(layout.panel(
        ROW_X,
        EXIT_Y - 13.0,
        [ROW_WIDTH, 26.0],
        rgba(fill, alpha),
        -18,
    ));
    draw_row_focus(actors, layout, EXIT_Y, 26.0, focused, alpha);
    let label = row
        .values
        .iter()
        .flatten()
        .copied()
        .next()
        .unwrap_or(row.label);
    actors.push(layout.bounded_label(
        [320.0, EXIT_Y],
        0.5,
        400.0,
        label.clone(),
        1.04,
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
        let focus_color = rgba(PLAYER_HUES[player], row.alpha);
        if focused[player] && !row.next_row[player] {
            actors.push(layout.panel(x - width * 0.5, y + 7.0, [width, 2.0], focus_color, -15));
        }
        actors.push(layout.bounded_label(
            [x, y],
            0.5,
            width,
            text.clone(),
            0.95,
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
    let fits = inline
        .choices
        .iter()
        .all(|text| (text.as_str().chars().count() as f32).mul_add(8.0, 8.0) <= width);
    if next_column {
        draw_next_row(actors, layout, y, row, focused);
    }
    if !fits {
        draw_compact_choices(actors, layout, y, row, inline, seats, focused);
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
        actors.push(layout.bounded_label(
            [x, y],
            0.5,
            width - 8.0,
            choice.clone(),
            0.9,
            rgba(tint, row.alpha),
        ));
        draw_choice_marks(
            actors,
            layout,
            [x, y],
            (width - 12.0).min(56.0),
            choice_enabled,
            choice_focused,
            row.alpha,
        );
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
        actors.push(layout.bounded_label(
            [x, y],
            0.5,
            120.0,
            inline.choices[choice].clone(),
            0.92,
            rgba(if focus || choice_on { INK } else { MUTED }, row.alpha),
        ));
        if inline.enabled[player].is_some() {
            let status = if choice_on { "ON" } else { "OFF" };
            actors.push(layout.bounded_label(
                [x + 62.0, y],
                0.0,
                22.0,
                status.into(),
                0.65,
                rgba(PLAYER_HUES[player], row.alpha),
            ));
        }
        draw_choice_marks(
            actors,
            layout,
            [x, y],
            56.0,
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
    actors.push(layout.bounded_label(
        position,
        0.5,
        50.0,
        "NEXT ROW".into(),
        0.8,
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
    for player in 0..MAX_PLAYERS {
        let mark_x = x + if player == 0 { -width * 0.5 } else { 1.0 };
        let mark_width = (width * 0.5 - 1.0).max(1.0);
        if enabled[player] {
            actors.push(layout.panel(
                mark_x,
                y + 7.0,
                [mark_width, 2.0],
                rgba(PLAYER_HUES[player], alpha),
                -14,
            ));
        }
        if focused[player] {
            actors.push(layout.panel(
                mark_x,
                y - 9.0,
                [mark_width, 2.0],
                rgba(PLAYER_HUES[player], alpha),
                -14,
            ));
        }
    }
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

    fn has_mark(actors: &[Actor], position: [f32; 2], tint: [f32; 4]) -> bool {
        actors.iter().any(|actor| {
            matches!(actor,
                Actor::Sprite { offset, tint: color, z: -14, .. }
                    if *offset == position && *color == tint
            )
        })
    }

    #[test]
    fn explanations_keep_their_layout_and_reveal_independently() {
        use deadlib_present::actors::{TextAlign, TextAttributes};
        use deadlib_present::compose::measure_wrapped_text;

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let font = deadlib_assets::parse_font_with_asset_context(
            &root.join("fonts/miso/_miso light.ini"),
            vec![root.clone()],
        )
        .expect("bundled font");
        let mut assets = deadlib_assets::AssetManager::new();
        assets.register_font("miso", font.font);
        let fonts = assets.fonts();
        let font = fonts.get("miso").unwrap();
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
                                w as f32 * scale[0] <= 250.001 && h as f32 * scale[1] <= 86.001
                            );
                            if right {
                                assert!(content.as_str().contains('\n'));
                                assert!(scale[0] < 0.82);
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
                assert!(has_text(&actors, "PLAYER OPTIONS", [36.0 + delta, 38.0]));
                assert!(has_text(&actors, "P2", [395.0 + delta, 92.0]));
                assert!(!has_text(&actors, "P1", [395.0 + delta, 92.0]));
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
            assert!(has_text(&actors, "Normal", [306.0, 127.0]));
            assert!(has_text(&actors, "Reverse", [498.0, 127.0]));
            assert!(has_text(&actors, "Split", [306.0, 155.0]));
            let p1 = rgba(PLAYER_HUES[0], 1.0);
            let p2 = rgba(PLAYER_HUES[1], 1.0);
            assert!(has_mark(&actors, [278.0, 134.0], p1));
            assert!(has_mark(&actors, [278.0, 162.0], p1));
            assert!(has_mark(&actors, [499.0, 134.0], p2));
            assert!(has_mark(&actors, [470.0, 118.0], p1));
            assert!(has_mark(&actors, [307.0, 146.0], p2));
            assert!(!has_mark(&actors, [470.0, 134.0], p1));
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
                [212.0, 118.0],
                rgba(PLAYER_HUES[0], 0.25)
            ));
            assert!(actors.iter().any(|actor| matches!(actor,
                Actor::Text { content, color, .. }
                    if content.as_str() == "DISPLAY OPTIONS" && color[3] == 0.5
            )));
        });
    }
}
