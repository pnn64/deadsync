use std::sync::Arc;
use std::time::{Duration, Instant};

use deadlib_assets::present_dsl::cover_uv;
use deadlib_present::actors::{Actor, SpriteSource, TextAlign, TextContent};
use deadlib_present::anim;
use deadlib_present::dsl::SpriteBuilder;
use deadsync_theme::select_music::{
    SELECT_MUSIC_WHEEL_ROWS, SelectMusicPrompt, SelectMusicSortView, SelectMusicView,
};
use deadsync_theme::selection::{MusicWheelPresentation, WheelMediaFrame};

use crate::presentation::{Layout, push_footer};
use crate::resources::{NORMAL_FONT, TITLE_FONT, WHEEL_FONT};

#[derive(Default)]
pub struct StepManiaMusicWheel {
    pub(crate) entered: Option<Instant>,
    pub(crate) play_entered: Option<f64>,
    pub(crate) play_selected: Option<usize>,
    pub(crate) focus_started: f64,
    confirmed: Option<Instant>,
}

impl MusicWheelPresentation<Actor> for StepManiaMusicWheel {
    fn show_random_entries(&self) -> bool {
        true
    }
    fn banner_crossfade(&self) -> Option<Duration> {
        Some(Duration::from_millis(250))
    }
    fn player_options_exit_seconds(&self) -> Option<f32> {
        Some(0.5)
    }
    fn enter_wheel(&mut self, now: Instant) {
        self.entered = Some(now);
        self.confirmed = None;
    }
    fn confirm_wheel(&mut self, now: Instant) {
        self.confirmed = Some(now);
    }
    fn cancel_wheel_confirmation(&mut self) {
        self.confirmed = None;
    }

    fn push_music(
        &self,
        actors: &mut Vec<Actor>,
        view: SelectMusicView<'_>,
        media: WheelMediaFrame<'_>,
    ) {
        let layout = Layout::current();
        let elapsed = self.entered.map_or(0.0, |at| {
            media.now.saturating_duration_since(at).as_secs_f32()
        });
        let fade = if media.exit_elapsed > 0.0 {
            1.0 - (media.exit_elapsed / 0.5).clamp(0.0, 1.0)
        } else {
            (elapsed / 0.5).clamp(0.0, 1.0)
        };
        actors.push(
            layout
                .sprite(
                    "stepmania-395/music-background",
                    [320.0, 240.0],
                    [640.0, 480.0],
                    -100,
                )
                .build(0),
        );
        actors.push(
            layout
                .sprite(
                    "stepmania-395/music-header",
                    [320.0, 22.0],
                    [640.0, 44.0],
                    10,
                )
                .build(0),
        );

        push_wheel(
            actors,
            layout,
            &view.rows,
            view.wheel_offset,
            elapsed,
            media.exit_elapsed,
            false,
        );

        let banner_shift = if media.exit_elapsed > 0.0 {
            -384.0 * anim::bouncebegin_p((media.exit_elapsed / 0.5).clamp(0.0, 1.0))
        } else {
            -384.0 * (1.0 - anim::bounceend_p((elapsed / 0.5).clamp(0.0, 1.0)))
        };
        let banner_at = [148.0 + banner_shift, 172.0];
        let mut mask = layout.sprite(
            "stepmania-395/music-banner-mask",
            banner_at,
            [284.0, 88.0],
            2,
        );
        mask.mask_source();
        actors.push(mask.build(0));
        let current = view.banner_texture.map_or(
            SpriteSource::TextureStatic("stepmania-395/music-fallback"),
            |key| SpriteSource::Texture(Arc::clone(key)),
        );
        push_banner(actors, layout, current, banner_at, 1.0);
        for outgoing in media.outgoing.iter().flatten().rev() {
            let alpha = 1.0 - (outgoing.elapsed_seconds / 0.25).clamp(0.0, 1.0);
            push_banner(
                actors,
                layout,
                SpriteSource::Texture(Arc::clone(outgoing.texture)),
                banner_at,
                alpha,
            );
        }
        actors.push(
            layout
                .sprite(
                    "stepmania-395/music-banner-frame",
                    [160.0 + banner_shift, 162.0],
                    [320.0, 116.0],
                    5,
                )
                .build(0),
        );
        if let Some(song) = view.song {
            let value = TextContent::inline_str(song.display_bpm)
                .unwrap_or_else(|| TextContent::Owned(song.display_bpm.to_owned()));
            let mut bpm = layout.text(NORMAL_FONT, value, [160.0 + banner_shift, 121.0], 1.0);
            bpm.z(6);
            actors.push(bpm.build(0));
        }
        push_sort(actors, layout, view.sort, banner_shift);

        if let Some(stage) = view.stage_number {
            let (stage_key, stage_width) = match stage {
                0 => ("stepmania-395/music-stage-event", 88.0),
                1 => ("stepmania-395/music-stage-1", 48.0),
                2 => ("stepmania-395/music-stage-2", 54.0),
                3 => ("stepmania-395/music-stage-3", 54.0),
                4 => ("stepmania-395/music-stage-4", 54.0),
                5 => ("stepmania-395/music-stage-5", 54.0),
                _ => ("stepmania-395/music-stage-6", 54.0),
            };
            actors.push(
                layout
                    .sprite(
                        stage_key,
                        [40.0 + banner_shift, 121.0],
                        [stage_width, 16.0],
                        6,
                    )
                    .build(0),
            );
        }
        let radar_zoom = if media.exit_elapsed > 0.0 {
            1.0 - (media.exit_elapsed / 0.6).clamp(0.0, 1.0)
        } else {
            0.5 + 0.5 * (elapsed / 0.6).clamp(0.0, 1.0)
        };
        let radar_rotation = if media.exit_elapsed > 0.0 {
            (1.0 - radar_zoom) * 720.0
        } else {
            720.0 * (1.0 - (elapsed / 0.6).clamp(0.0, 1.0))
        };
        let mut radar = layout.sprite(
            "stepmania-395/music-radar",
            [160.0, 334.0],
            [170.0, 166.0],
            4,
        );
        radar.zoom(radar_zoom);
        radar.rotationz(radar_rotation);
        actors.push(radar.build(0));
        if let Some(song) = view.song {
            crate::radar::push_values(
                actors,
                layout,
                view.players.map(|player| {
                    player
                        .chart
                        .filter(|_| player.joined)
                        .map(|chart| chart.radar_values(song.music_length_seconds))
                }),
                radar_zoom,
                radar_rotation,
            );
        }
        for (index, at) in [
            [160.0, 242.0],
            [40.0, 314.0],
            [60.0, 400.0],
            [260.0, 400.0],
            [280.0, 314.0],
        ]
        .into_iter()
        .enumerate()
        {
            let t = ((elapsed - 0.6 - index as f32 * 0.2) / 0.4).clamp(0.0, 1.0);
            let mut label = layout.sprite(
                "stepmania-395/music-radar-labels",
                [at[0] - 100.0 * (1.0 - t * t), at[1]],
                [84.0, 26.0],
                5,
            );
            label.alpha(t * fade);
            label.customtexturerect([0.0, index as f32 / 5.0, 1.0, (index + 1) as f32 / 5.0]);
            actors.push(label.build(0));
        }
        for (index, name) in ["Beginner", "Easy", "Medium", "Hard", "Challenge"]
            .into_iter()
            .enumerate()
        {
            if !view.difficulty_rows.iter().any(|row| {
                row.chart
                    .is_some_and(|chart| chart.difficulty.eq_ignore_ascii_case(name))
            }) {
                continue;
            }
            let mut icon = layout.sprite(
                "stepmania-395/music-difficulties",
                [313.0 + banner_shift, 140.0 + 16.0 * index as f32],
                [16.0, 16.0],
                6,
            );
            icon.customtexturerect([index as f32 / 6.0, 0.0, (index + 1) as f32 / 6.0, 1.0]);
            actors.push(icon.build(0));
        }

        for (seat, player) in view.players.iter().enumerate() {
            let Some(chart) = player.chart.filter(|_| player.joined) else {
                continue;
            };
            let p2 = seat == 1;
            let (frame, icon_x, frame_x, meter_frame, meter_frame_x, meter_x) = if p2 {
                (
                    "stepmania-395/music-difficulty-p2",
                    272.0,
                    260.0,
                    "stepmania-395/music-meter-p2",
                    238.0,
                    228.0,
                )
            } else {
                (
                    "stepmania-395/music-difficulty-p1",
                    48.0,
                    60.0,
                    "stepmania-395/music-meter-p1",
                    82.0,
                    88.0,
                )
            };
            let difficulty = ["Beginner", "Easy", "Medium", "Hard", "Challenge", "Edit"]
                .iter()
                .position(|name| name.eq_ignore_ascii_case(&chart.difficulty))
                .unwrap_or(5);
            let difficulty_color = difficulty_color(&chart.difficulty);
            let mut border = layout.sprite(frame, [frame_x, 248.0], [78.0, 40.0], 6);
            border.alpha(fade);
            actors.push(border.build(0));
            let mut icon = layout.sprite(
                "stepmania-395/music-difficulty",
                [icon_x, 248.0],
                [32.0, 32.0],
                7,
            );
            icon.zoomy(fade);
            icon.customtexturerect([
                0.0,
                difficulty as f32 / 6.0,
                1.0,
                (difficulty + 1) as f32 / 6.0,
            ]);
            actors.push(icon.build(0));
            let mut border = layout.sprite(meter_frame, [meter_frame_x, 432.0], [152.0, 28.0], 6);
            border.alpha(fade);
            actors.push(border.build(0));
            let lit = chart.meter.min(14) as usize;
            let count = lit.max(10);
            for foot in 0..count {
                let x = meter_x + (foot as f32 - (count - 1) as f32 * 0.5) * 10.0;
                let u = if foot < lit { 0.0 } else { 0.5 };
                let mut mark =
                    layout.sprite("stepmania-395/music-feet", [x, 432.0], [10.0, 18.0], 7);
                mark.zoomy(fade);
                mark.diffuse(difficulty_color);
                mark.customtexturerect([u, 0.0, u + 0.5, 1.0]);
                actors.push(mark.build(0));
            }
        }
        let help = match (elapsed / 5.0) as usize % 3 {
            0 => {
                "&LEFT; or &RIGHT; change music     Hold &LEFT; and &RIGHT; then press START to change sort"
            }
            1 => "&UP;&UP; easier difficulty     &DOWN;&DOWN; harder difficulty",
            _ => "&UP;&DOWN;&UP;&DOWN; to change sort",
        };
        push_footer(actors, help, fade);
        self.push_options_prompt(actors, view.prompt, media.now);
    }
}

fn push_sort(
    actors: &mut Vec<Actor>,
    layout: Layout,
    sort: SelectMusicSortView,
    banner_shift: f32,
) {
    let at = [252.0 + banner_shift, 121.0];
    let artwork = match sort {
        SelectMusicSortView::Title => Some("stepmania-395/music-sort-title"),
        SelectMusicSortView::Artist => Some("stepmania-395/music-sort-artist"),
        SelectMusicSortView::Bpm => Some("stepmania-395/music-sort-bpm"),
        SelectMusicSortView::Group => Some("stepmania-395/music-sort-group"),
        SelectMusicSortView::Popularity { side: None } => {
            Some("stepmania-395/music-sort-popularity")
        }
        SelectMusicSortView::TopGrades { side: None } => Some("stepmania-395/music-sort-grades"),
        _ => None,
    };
    if let Some(key) = artwork {
        actors.push(layout.sprite(key, at, [68.0, 16.0], 6).build(0));
        return;
    }
    // Extra catalog sorts have no artwork in the reference theme.
    let label = match sort {
        SelectMusicSortView::Series => "Series".to_owned(),
        SelectMusicSortView::Genre => "Genre".to_owned(),
        SelectMusicSortView::Length => "Length".to_owned(),
        SelectMusicSortView::Meter => "Meter".to_owned(),
        SelectMusicSortView::Recent { side: None } => "Recent".to_owned(),
        SelectMusicSortView::Popularity { side: Some(side) } => format!("P{} Popularity", side + 1),
        SelectMusicSortView::Recent { side: Some(side) } => format!("P{} Recent", side + 1),
        SelectMusicSortView::TopGrades { side: Some(side) } => format!("P{} Top Grades", side + 1),
        SelectMusicSortView::Favorites { side } => format!("P{} Favorites", side + 1),
        SelectMusicSortView::Playlist => "Playlist".to_owned(),
        _ => return,
    };
    let mut text = layout.text(NORMAL_FONT, TextContent::Owned(label), at, 0.6);
    text.maxwidth(68.0 * layout.scale);
    text.z(6);
    actors.push(text.build(0));
}

fn text(value: &str) -> TextContent {
    TextContent::inline_str(value).unwrap_or_else(|| TextContent::Owned(value.to_owned()))
}

fn push_banner(
    actors: &mut Vec<Actor>,
    layout: Layout,
    source: SpriteSource,
    at: [f32; 2],
    alpha: f32,
) {
    let uv = source
        .texture_key()
        .and_then(|key| cover_uv(key, 284.0, 88.0));
    let [x, y] = layout.position(at);
    let mut banner = SpriteBuilder::with_source(source);
    banner.xy(x, y);
    banner.size(284.0 * layout.scale, 88.0 * layout.scale);
    banner.alpha(alpha);
    banner.mask_dest();
    banner.z(3);
    if let Some(uv) = uv {
        banner.customtexturerect(uv);
    }
    actors.push(banner.build(0));
}

pub(crate) fn push_wheel(
    actors: &mut Vec<Actor>,
    layout: Layout,
    rows: &[Option<deadsync_theme::select_music::SelectMusicWheelRow<'_>>; SELECT_MUSIC_WHEEL_ROWS],
    offset: f32,
    elapsed: f32,
    exit: f32,
    course: bool,
) {
    for (index, row) in rows.iter().enumerate() {
        let Some(row) = row else { continue };
        let relative = index as f32 - (SELECT_MUSIC_WHEEL_ROWS / 2) as f32 + offset;
        if relative.abs() > 6.5 {
            continue;
        }
        let delay = relative.abs() * 0.04;
        let enter = ((elapsed - delay) / 0.2).clamp(0.0, 1.0);
        let shift = if exit > 0.0 {
            let t = ((exit - delay) / 0.2).clamp(0.0, 1.0);
            320.0 * (1.0 - (1.0 - t).powi(2))
        } else {
            320.0 * (1.0 - enter * enter)
        };
        let x = 480.0 + ((1.0 - (relative / std::f32::consts::PI).cos()) * 85.0).round() + shift;
        let y = 244.0 + (relative * 46.0).round();
        let section = row.song_count.is_some() || row.action.is_some();
        let key = if section {
            "stepmania-395/music-section"
        } else {
            "stepmania-395/music-song"
        };
        actors.push(layout.sprite(key, [x, y], [324.0, 46.0], 0).build(0));
        if section {
            let value = if row.action.is_some() {
                "RANDOM"
            } else {
                row.title
            };
            let mut label = layout.text(TITLE_FONT, text(value), [x - 30.0, y], 1.0);
            label.maxwidth(200.0 * layout.scale);
            actors.push(label.build(0));
        } else if course {
            let mut title = layout.text(TITLE_FONT, text(row.title), [x - 130.0, y], 1.3);
            title.align(0.0, 0.5);
            title.horizalign(TextAlign::Left);
            title.maxwidth(200.0 * layout.scale);
            actors.push(title.build(0));
        } else {
            let three_lines = !row.subtitle.is_empty();
            let line_offset = if three_lines { 10.0 } else { 8.0 };
            let mut title = layout.text(
                WHEEL_FONT,
                text(row.title),
                [x - 120.0, y - line_offset],
                1.0,
            );
            title.horizalign(TextAlign::Left);
            title.align(0.0, 0.5);
            title.maxwidth(180.0 * layout.scale);
            actors.push(title.build(0));
            if three_lines {
                let mut subtitle = layout.text(WHEEL_FONT, text(row.subtitle), [x - 120.0, y], 0.5);
                subtitle.horizalign(TextAlign::Left);
                subtitle.align(0.0, 0.5);
                subtitle.maxwidth(180.0 * layout.scale);
                actors.push(subtitle.build(0));
            }
            let mut artist = layout.text(
                WHEEL_FONT,
                TextContent::Owned(format!("/{}", row.artist)),
                [x - 120.0, y + line_offset],
                0.6,
            );
            artist.horizalign(TextAlign::Left);
            artist.align(0.0, 0.5);
            artist.maxwidth(159.6 * layout.scale);
            actors.push(artist.build(0));
        }
    }
    let reveal = ((elapsed - 0.05) / 0.4).clamp(0.0, 1.0);
    let highlight_shift = if exit > 0.0 {
        let t = (exit / 0.2).clamp(0.0, 1.0);
        320.0 * (1.0 - (1.0 - t).powi(2))
    } else {
        320.0 * (1.0 - reveal * reveal)
    };
    let mut highlight = layout.sprite(
        "stepmania-395/music-highlight",
        [480.0 + highlight_shift, 244.0],
        [324.0, 46.0],
        2,
    );
    highlight.glow([
        1.0,
        1.0,
        1.0,
        0.6 + 0.4 * (elapsed * std::f32::consts::TAU / 2.0).cos(),
    ]);
    actors.push(highlight.build(0));
}

impl StepManiaMusicWheel {
    pub(crate) fn push_options_prompt(
        &self,
        actors: &mut Vec<Actor>,
        prompt: SelectMusicPrompt,
        now: Instant,
    ) {
        let layout = Layout::current();
        if prompt != SelectMusicPrompt::Hidden {
            let time = self
                .confirmed
                .map_or(0.0, |at| now.saturating_duration_since(at).as_secs_f32());
            let zoom =
                (time / 0.25).clamp(0.0, 1.0) * (1.0 - ((time - 1.25) / 0.25).clamp(0.0, 1.0));
            let top = if prompt == SelectMusicPrompt::EnteringOptions {
                0.5
            } else {
                0.0
            };
            let mut prompt = layout.sprite(
                "stepmania-395/music-options",
                [320.0, 240.0],
                [480.0, 32.0],
                20,
            );
            prompt.zoomy(zoom);
            prompt.customtexturerect([0.0, top, 1.0, top + 0.5]);
            actors.push(prompt.build(0));
        }
    }
}

pub(crate) fn difficulty_color(name: &str) -> [f32; 4] {
    let index = ["Beginner", "Easy", "Medium", "Hard", "Challenge", "Edit"]
        .iter()
        .position(|value| value.eq_ignore_ascii_case(name))
        .unwrap_or(5);
    [
        [0.0, 0.9, 1.0, 1.0],
        [0.9, 0.9, 0.0, 1.0],
        [1.0, 0.1, 0.1, 1.0],
        [0.2, 1.0, 0.2, 1.0],
        [0.2, 0.6, 1.0, 1.0],
        [0.8, 0.8, 0.8, 1.0],
    ][index]
}
