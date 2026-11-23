//! Music-wheel presentation over the application's current browsing view.

use crate::presentation::{ACCENT, HEIGHT, INK, Layout, MUTED, WIDTH, quad};
use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::cache::{SharedStrCache, cached_shared_str, shared_str_cache_with_capacity};
use deadlib_present::space::{screen_height, screen_width};
use deadsync_theme::select_music::{
    SELECT_MUSIC_WHEEL_ROWS, SelectCourseItem, SelectCourseView, SelectMusicPrompt,
    SelectMusicView, SelectMusicWheelRow,
};
use std::cell::RefCell;
use std::f32::consts::{PI, TAU};

const WHEEL_CENTER: [f32; 2] = [212.0, 222.0];
const WHEEL_RADIUS: f32 = 200.0;
const WHEEL_CURVE_X: f32 = 24.0;
// The reference retains two additional positions outside the visible rows.
const WHEEL_ANGLE_PER_ROW: f32 = TAU * 0.42 / 17.0;
const BANNER_CENTER: [f32; 2] = [480.0, 112.0];
const BANNER_SIZE: [f32; 2] = [430.0, 130.0];
const DIFFICULTY_CENTER: [f32; 2] = [460.0, 242.0];
const DIFFICULTY_PITCH: f32 = 20.5;
const TEXT_CACHE_LIMIT: usize = 4096;

const fn opaque([red, green, blue]: [f32; 3]) -> [f32; 4] {
    [red, green, blue, 1.0]
}

thread_local! {
    static WHEEL_LABELS: RefCell<SharedStrCache> = RefCell::new(shared_str_cache_with_capacity(256));
}

fn shared_text(text: &str) -> TextContent {
    TextContent::inline_str(text).unwrap_or_else(|| {
        TextContent::Shared(cached_shared_str(&WHEEL_LABELS, text, TEXT_CACHE_LIMIT))
    })
}

fn wheel_row(slot: usize, scroll_offset: f32) -> ([f32; 2], f32) {
    let offset_from_center = slot as f32 - (SELECT_MUSIC_WHEEL_ROWS / 2) as f32 + scroll_offset;
    let (rise, depth) = (offset_from_center * WHEEL_ANGLE_PER_ROW).sin_cos();
    let curve = (1.0 - (offset_from_center / PI).cos()) * WHEEL_CURVE_X;
    (
        [
            WHEEL_CENTER[0] + curve,
            WHEEL_CENTER[1] + WHEEL_RADIUS * rise,
        ],
        depth,
    )
}

/// Draws the browsing wheel, banner, currently available charts, and launch feedback.
pub fn push_select_music_actors(actors: &mut Vec<Actor>, view: SelectMusicView<'_>) {
    let layout = push_backdrop(actors);
    push_banner(actors, layout, &view);
    push_wheel(
        actors,
        layout,
        &view.rows,
        view.wheel_offset,
        view.selection_elapsed,
        "NO SONGS",
    );
    push_difficulties(actors, layout, &view);
    push_footer(actors, layout, &view);
    push_prompt(actors, layout, view.prompt);
}

fn push_backdrop(actors: &mut Vec<Actor>) -> Layout {
    let (width, height) = (screen_width(), screen_height());
    let layout = Layout::centered(width, height);
    actors.push(quad(
        [0.0, 0.0],
        [width, height],
        [0.035, 0.048, 0.072, 1.0],
        -20,
    ));
    actors.push(layout.panel(0.0, 0.0, [WIDTH, HEIGHT], [0.063, 0.087, 0.126, 1.0], -19));
    layout
}

fn push_banner_plate(
    actors: &mut Vec<Actor>,
    layout: Layout,
    texture: Option<&std::sync::Arc<str>>,
) {
    let banner_left = BANNER_CENTER[0] - BANNER_SIZE[0] * 0.5;
    let banner_top = BANNER_CENTER[1] - BANNER_SIZE[1] * 0.5;
    actors.push(layout.panel(
        banner_left,
        banner_top,
        BANNER_SIZE,
        [0.10, 0.16, 0.23, 1.0],
        -7,
    ));
    if let Some(key) = texture {
        actors.push(layout.image(BANNER_CENTER, BANNER_SIZE, key, -6));
    }
    actors.push(layout.panel(
        banner_left,
        banner_top + BANNER_SIZE[1] - 20.0,
        [BANNER_SIZE[0], 20.0],
        [0.04, 0.07, 0.11, 0.88],
        0,
    ));
}

fn push_banner(actors: &mut Vec<Actor>, layout: Layout, view: &SelectMusicView<'_>) {
    push_banner_plate(actors, layout, view.banner_texture);
    if let Some(song) = &view.song {
        actors.push(layout.bounded_label(
            [280.0, 168.0],
            0.0,
            330.0,
            shared_text(song.title),
            1.10,
            opaque(INK),
        ));
        if !song.subtitle.is_empty() {
            actors.push(layout.bounded_label(
                [280.0, 187.0],
                0.0,
                330.0,
                shared_text(song.subtitle),
                0.75,
                opaque(MUTED),
            ));
        }
        actors.push(layout.bounded_label(
            [280.0, 202.0],
            0.0,
            180.0,
            shared_text(song.artist),
            0.85,
            opaque(MUTED),
        ));
        if !song.display_bpm.is_empty() {
            actors.push(layout.label([483.0, 206.0], 1.0, "BPM".into(), 0.8, opaque(ACCENT)));
            actors.push(layout.bounded_label(
                [495.0, 206.0],
                0.0,
                125.0,
                shared_text(song.display_bpm),
                0.95,
                opaque(INK),
            ));
        }
    } else if let Some(section) = &view.rows[SELECT_MUSIC_WHEEL_ROWS / 2] {
        actors.push(layout.bounded_label(
            [280.0, 168.0],
            0.0,
            330.0,
            shared_text(section.title),
            1.10,
            opaque(INK),
        ));
    }
}

fn push_wheel(
    actors: &mut Vec<Actor>,
    layout: Layout,
    rows: &[Option<SelectMusicWheelRow<'_>>; SELECT_MUSIC_WHEEL_ROWS],
    wheel_offset: f32,
    selection_elapsed: f32,
    empty_text: &'static str,
) {
    let phase = selection_elapsed.rem_euclid(1.0);
    let pulse = 1.0 - (2.0 * phase - 1.0).abs();
    for (index, row) in rows.iter().enumerate() {
        let Some(row) = row else { continue };
        let ([x, y], depth) = wheel_row(index, wheel_offset);
        let focused = index == SELECT_MUSIC_WHEEL_ROWS / 2;
        let section = row.song_count.is_some();
        let row_height = 22.0 * depth.clamp(0.45, 1.0);
        let panel_tint = if focused {
            [0.23, 0.30, 0.39, 1.0]
        } else if section {
            [0.14, 0.22, 0.31, 1.0]
        } else {
            [0.09, 0.14, 0.21, 1.0]
        };
        actors.push(layout.panel(
            x - 171.0,
            y - row_height * 0.5,
            [200.0, row_height],
            panel_tint,
            3,
        ));
        if focused {
            actors.push(layout.panel(
                x - 171.0,
                y - row_height * 0.5,
                [3.0, row_height],
                [ACCENT[0], ACCENT[1], ACCENT[2], 0.70 + 0.30 * pulse],
                4,
            ));
        }
        let title_color = if focused {
            opaque(INK)
        } else if section {
            opaque(ACCENT)
        } else {
            opaque(MUTED)
        };
        let title_x = if section { x - 146.0 } else { x - 159.0 };
        if section {
            let indicator = if row.expanded { "-" } else { "+" };
            actors.push(layout.label([x - 158.0, y], 0.5, indicator.into(), 0.70, title_color));
        }
        let available_width = if section { 135.0 } else { 182.0 };
        actors.push(layout.bounded_label(
            [title_x, y],
            0.0,
            available_width,
            shared_text(row.title),
            0.70 + 0.17 * depth,
            title_color,
        ));
        if let Some(count) = row.song_count {
            actors.push(layout.label(
                [x + 21.0, y],
                1.0,
                TextContent::inline_u32(count.min(u32::MAX as usize) as u32),
                0.65,
                opaque(MUTED),
            ));
        }
    }
    if rows.iter().all(Option::is_none) {
        actors.push(layout.label(
            [83.0, WHEEL_CENTER[1]],
            0.0,
            empty_text.into(),
            1.0,
            opaque(MUTED),
        ));
    }
}

fn push_difficulties(actors: &mut Vec<Actor>, layout: Layout, view: &SelectMusicView<'_>) {
    if view.song.is_none() {
        return;
    }
    let [x, base_y] = DIFFICULTY_CENTER;
    for (index, row) in view.difficulty_rows.iter().enumerate() {
        let y = base_y + index as f32 * DIFFICULTY_PITCH;
        let selected = view.players.iter().any(|player| {
            player.joined && player.chart.is_some() && player.step_index == row.step_index
        });
        let panel_tint = if selected {
            [0.19, 0.30, 0.39, 1.0]
        } else {
            [0.10, 0.16, 0.23, 1.0]
        };
        actors.push(layout.panel(x - 113.0, y - 8.5, [249.0, 17.0], panel_tint, 2));
        let Some(chart) = row.chart else { continue };
        let level =
            if chart.difficulty.eq_ignore_ascii_case("edit") && !chart.description.is_empty() {
                chart.description.as_str()
            } else {
                chart.difficulty.as_str()
            };
        actors.push(layout.bounded_label(
            [x - 69.0, y],
            1.0,
            115.0,
            shared_text(level),
            0.73,
            opaque(INK),
        ));
        actors.push(layout.label(
            [x - 22.0, y],
            0.5,
            TextContent::inline_u32(chart.meter),
            0.89,
            opaque(ACCENT),
        ));
        actors.push(layout.panel(x + 3.0, y - 2.5, [85.0, 5.0], [0.15, 0.23, 0.34, 1.0], 3));
        let fill = 85.0 * (chart.meter as f32 / 12.0).clamp(0.0, 1.0);
        if fill > 0.0 {
            actors.push(layout.panel(x + 3.0, y - 2.5, [fill, 5.0], opaque(ACCENT), 4));
        }
        for (player, side) in view.players.iter().enumerate() {
            if side.joined && side.chart.is_some() && side.step_index == row.step_index {
                let (offset, label) = if player == 0 {
                    (103.0, "P1")
                } else {
                    (126.0, "P2")
                };
                actors.push(layout.label([x + offset, y], 0.5, label.into(), 0.72, opaque(ACCENT)));
            }
        }
    }
}

fn push_footer(actors: &mut Vec<Actor>, layout: Layout, view: &SelectMusicView<'_>) {
    actors.push(layout.label([36.0, 32.0], 0.0, "SELECT MUSIC".into(), 1.55, opaque(INK)));
    actors.push(layout.panel(36.0, 425.0, [568.0, 1.0], [0.32, 0.40, 0.48, 1.0], 2));
    if let Some(stage) = view.stage_number {
        let label = TextContent::inline_format(format_args!("STAGE {stage}"))
            .unwrap_or_else(|| "STAGE".into());
        actors.push(layout.label([36.0, 447.0], 0.0, label, 0.9, opaque(MUTED)));
    }
    if (view.music_rate - 1.0).abs() >= 0.005 {
        let label = TextContent::inline_format(format_args!("{:.2}x", view.music_rate))
            .unwrap_or_else(|| "RATE".into());
        actors.push(layout.label([604.0, 447.0], 1.0, label, 0.9, opaque(ACCENT)));
    }
    let [first, second] = &view.players;
    for (side, x) in [(first, 368.0), (second, 517.0)] {
        if side.joined && !side.name.is_empty() {
            actors.push(layout.bounded_label(
                [x, 389.0],
                0.5,
                135.0,
                shared_text(side.name),
                0.80,
                opaque(INK),
            ));
        }
    }
}

fn push_prompt(actors: &mut Vec<Actor>, layout: Layout, prompt: SelectMusicPrompt) {
    let label = match prompt {
        SelectMusicPrompt::Hidden => return,
        SelectMusicPrompt::OfferOptions => "PRESS START FOR PLAYER OPTIONS",
        SelectMusicPrompt::EnteringOptions => "ENTERING PLAYER OPTIONS",
    };
    actors.push(quad(
        [0.0, 0.0],
        [screen_width(), screen_height()],
        [0.02, 0.03, 0.05, 0.70],
        1400,
    ));
    actors.push(layout.panel(112.0, 212.0, [416.0, 56.0], [0.11, 0.16, 0.23, 1.0], 1401));
    let mut heading = layout.bounded_label(
        [WIDTH * 0.5, HEIGHT * 0.5],
        0.5,
        384.0,
        label.into(),
        1.0,
        opaque(INK),
    );
    if let Actor::Text { z, .. } = &mut heading {
        *z = 1402;
    }
    actors.push(heading);
}

/// Present the course variant of the music wheel using its current rating and records.
pub fn push_select_course_actors(actors: &mut Vec<Actor>, view: SelectCourseView<'_>) {
    let layout = push_backdrop(actors);
    push_banner_plate(actors, layout, view.banner_texture);
    push_wheel(
        actors,
        layout,
        &view.rows,
        view.wheel_offset,
        view.selection_elapsed,
        "NO COURSES",
    );
    actors.push(layout.label([36.0, 32.0], 0.0, "PICK A COURSE".into(), 1.55, opaque(INK)));
    actors.push(layout.panel(36.0, 425.0, [568.0, 1.0], [0.32, 0.40, 0.48, 1.0], 2));
    if let Some(course) = &view.item {
        push_course_details(actors, layout, course);
    }
    for (player, name) in view.player_names.into_iter().enumerate() {
        if let Some(name) = name.filter(|name| !name.is_empty()) {
            let x = 343.0 + player as f32 * 125.0;
            actors.push(layout.bounded_label(
                [x, 447.0],
                0.0,
                112.0,
                shared_text(name),
                0.79,
                opaque(INK),
            ));
        }
    }
    if (view.music_rate - 1.0).abs() >= 0.005 {
        let label = TextContent::inline_format(format_args!("{:.2}x", view.music_rate))
            .unwrap_or_else(|| "RATE".into());
        actors.push(layout.label([604.0, 447.0], 1.0, label, 0.9, opaque(ACCENT)));
    }
    push_prompt(actors, layout, view.prompt);
}

fn push_course_details(actors: &mut Vec<Actor>, layout: Layout, course: &SelectCourseItem<'_>) {
    actors.push(layout.bounded_label(
        [280.0, 168.0],
        0.0,
        330.0,
        shared_text(course.title),
        1.10,
        opaque(INK),
    ));
    if !course.author.is_empty() {
        actors.push(layout.bounded_label(
            [280.0, 189.0],
            0.0,
            325.0,
            shared_text(course.author),
            0.83,
            opaque(MUTED),
        ));
    }
    for (x, label, value, width) in [
        (280.0, "SONGS", course.songs, 88.0),
        (380.0, "BPM", course.bpm, 137.0),
        (529.0, "TIME", course.length, 98.0),
    ] {
        let text = TextContent::inline_format(format_args!("{label} {value}"))
            .unwrap_or_else(|| shared_text(value));
        actors.push(layout.bounded_label([x, 211.0], 0.0, width, text, 0.83, opaque(INK)));
    }
    if let Some(rating) = course.rating {
        let fill = if course.rating_focused {
            [0.33, 0.27, 0.16, 1.0]
        } else {
            [0.19, 0.30, 0.39, 1.0]
        };
        actors.push(layout.panel(280.0, 226.0, [324.0, 22.0], fill, 2));
        actors.push(layout.bounded_label(
            [442.0, 237.0],
            0.5,
            238.0,
            shared_text(rating),
            0.94,
            opaque(ACCENT),
        ));
        if course.can_change_rating {
            for (x, arrow) in [(297.0, "<"), (587.0, ">")] {
                actors.push(layout.label([x, 237.0], 0.5, arrow.into(), 0.90, opaque(ACCENT)));
            }
        }
    }
    for (index, track) in course.tracks.iter().enumerate() {
        let Some(track) = track else { continue };
        let y = 261.0 + index as f32 * 20.0;
        actors.push(layout.panel(280.0, y - 9.0, [324.0, 18.0], [0.10, 0.16, 0.23, 1.0], 2));
        actors.push(layout.bounded_label(
            [289.0, y],
            0.0,
            250.0,
            track.title.into(),
            0.81,
            opaque(INK),
        ));
        actors.push(layout.bounded_label(
            [591.0, y],
            1.0,
            45.0,
            track.meter.into(),
            0.81,
            opaque(ACCENT),
        ));
    }
    if !course.description.is_empty() {
        actors.push(layout.paragraph(
            [280.0, 379.0],
            [324.0, 41.0],
            course.description.into(),
            0.77,
            opaque(MUTED),
        ));
    }
    for (x, label, record) in [
        (36.0, "MACHINE", course.machine_record),
        (185.0, "PERSONAL", course.player_record),
    ] {
        if let Some([name, score]) = record {
            let value = TextContent::inline_format(format_args!("{label} {name} {score}"))
                .unwrap_or_else(|| shared_text(score));
            actors.push(layout.bounded_label([x, 447.0], 0.0, 142.0, value, 0.78, opaque(MUTED)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::actors::SizeSpec;
    use deadlib_present::space::{Metrics, with_current_metrics};
    use deadsync_theme::select_music::{
        SelectMusicDifficultyRow, SelectMusicPlayerView, SelectMusicSongView, SelectMusicWheelRow,
    };
    use std::sync::Arc;

    fn view(banner: &Arc<str>, prompt: SelectMusicPrompt) -> SelectMusicView<'_> {
        let mut rows = [None; SELECT_MUSIC_WHEEL_ROWS];
        rows[SELECT_MUSIC_WHEEL_ROWS / 2] = Some(SelectMusicWheelRow {
            title: "Track",
            song_count: None,
            expanded: false,
        });
        SelectMusicView {
            rows,
            wheel_offset: 0.0,
            selection_elapsed: 0.0,
            song: Some(SelectMusicSongView {
                title: "Track",
                subtitle: "",
                artist: "Artist",
                display_bpm: "150",
            }),
            banner_texture: Some(banner),
            players: [SelectMusicPlayerView {
                joined: false,
                name: "",
                chart: None,
                step_index: 0,
            }; 2],
            difficulty_rows: std::array::from_fn(|index| SelectMusicDifficultyRow {
                step_index: index,
                chart: None,
            }),
            stage_number: Some(1),
            music_rate: 1.0,
            prompt,
        }
    }

    #[test]
    fn wheel_arc_has_native_endpoints_and_retains_the_scroll_offset() {
        let (top, _) = wheel_row(0, 0.0);
        let (middle, depth) = wheel_row(SELECT_MUSIC_WHEEL_ROWS / 2, 0.0);
        let (bottom, _) = wheel_row(SELECT_MUSIC_WHEEL_ROWS - 1, 0.0);
        assert!((top[0] - 250.665).abs() < 0.01);
        assert!((top[1] - 44.988).abs() < 0.01);
        assert_eq!(middle, [212.0, 222.0]);
        assert_eq!(depth, 1.0);
        assert!((bottom[1] - 399.012).abs() < 0.01);
        assert_eq!(
            wheel_row(SELECT_MUSIC_WHEEL_ROWS / 2, 1.0),
            wheel_row(SELECT_MUSIC_WHEEL_ROWS / 2 + 1, 0.0)
        );
    }

    #[test]
    fn native_and_wide_frames_keep_banner_artist_difficulties_and_prompt_anchors() {
        let banner = Arc::<str>::from("selection-banner");
        for (width, delta) in [(640.0, 0.0), (854.0, 107.0)] {
            with_current_metrics(Metrics::centered(width, 480.0), || {
                let mut actors = Vec::new();
                push_select_music_actors(
                    &mut actors,
                    view(&banner, SelectMusicPrompt::EnteringOptions),
                );
                assert!(actors.iter().any(|actor| matches!(actor,
                    Actor::Sprite { align, offset, size: [SizeSpec::Px(w), SizeSpec::Px(h)], .. }
                        if *align == [0.5, 0.5] && *offset == [480.0 + delta, 112.0] && *w == 430.0 && *h == 130.0
                )));
                assert!(actors.iter().any(|actor| matches!(actor,
                    Actor::Text { content, offset, .. }
                        if content.as_str() == "Artist" && *offset == [280.0 + delta, 202.0]
                )));
                assert!(actors.iter().any(|actor| matches!(actor,
                    Actor::Sprite { offset, size: [SizeSpec::Px(w), SizeSpec::Px(h)], .. }
                        if *offset == [347.0 + delta, 315.5] && *w == 249.0 && *h == 17.0
                )));
                assert!(actors.iter().any(|actor| matches!(actor,
                    Actor::Text { content, offset, z, .. }
                        if content.as_str() == "ENTERING PLAYER OPTIONS" && *offset == [320.0 + delta, 240.0] && *z > 1401
                )));
            });
        }
    }
}
