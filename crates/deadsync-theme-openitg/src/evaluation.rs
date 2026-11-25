mod award;
mod grade;
mod life_graph;

use std::sync::{Arc, LazyLock};

#[cfg(test)]
use deadlib_assets::AssetManager;
use deadlib_assets::{AssetView, present_dsl::cover_uv};
use deadlib_present::actors::{Actor, MeshTexture, SizeSpec, TextContent};
use deadlib_present::anim::bounceend_p;
use deadlib_present::dsl::TextBuilder;
use deadlib_present::render::{BlendMode, CullMode, TexturedMeshVertex};
use deadlib_present::space::{screen_height, screen_width};
use deadsync_theme::views::{EvaluationOverview, EvaluationPlayerOverview, Grade};
use glam::{Mat4, Vec3};

use crate::presentation::{
    HEIGHT, Layout, TextMetrics, WIDTH, italic, push_red_backdrop, push_red_streaks, sprite,
};
use crate::resources::{
    EVALUATION_NUMBER_FONT, GAMEPLAY_SCORE_FONT, MENU_HEADING_FONT, MENU_NORMAL_FONT, TITLE_FONT,
};

const STATS_START_SECONDS: f32 = 3.0;
const STATS_ENTER_SECONDS: f32 = 0.3;
const CAPTION_FADE_SECONDS: f32 = 0.5;
pub const EVALUATION_OVERVIEW_SETTLED_SECONDS: f32 = 4.45;
pub(super) const INPUT_READY_SECONDS: f32 = 3.2;
pub(super) const DEPARTURE_SECONDS: f32 = 0.5;
const DEPARTURE_MOTION_SECONDS: f32 = 0.3;

const GRADE_ENTER_SECONDS: f32 = 0.5;
const GRADE_SETTLE_START: f32 = 2.5;
const BANNER_SIZE: [f32; 2] = [210.0, 66.0];
const BANNER_FRAME_SIZE: [f32; 2] = [222.0, 82.0];
const LIFE_SIZE: [f32; 2] = [271.0, 39.0];
const LIFE_Y: f32 = 239.0;
const COMBO_Y: f32 = 287.0;
const COUNT_FIRST_Y: f32 = 325.0;
const COUNT_PITCH: f32 = 16.0;

const WHITE: [f32; 4] = [1.0; 4];
const P1: [f32; 4] = [251.0 / 255.0, 190.0 / 255.0, 3.0 / 255.0, 1.0];
const P2: [f32; 4] = [86.0 / 255.0, 1.0, 72.0 / 255.0, 1.0];
const JUDGMENTS: [&str; 6] = [
    "Fantastic",
    "Excellent",
    "Great",
    "Decent",
    "Way Off",
    "Miss",
];
const MACHINE_RECORDS: [&str; 10] = [
    "Machine Record #1",
    "Machine Record #2",
    "Machine Record #3",
    "Machine Record #4",
    "Machine Record #5",
    "Machine Record #6",
    "Machine Record #7",
    "Machine Record #8",
    "Machine Record #9",
    "Machine Record #10",
];

static BANNER_MASK: LazyLock<Arc<str>> =
    LazyLock::new(|| "openitg/evaluation-banner-mask.png".into());
static FALLBACK_BANNER: LazyLock<Arc<str>> =
    LazyLock::new(|| "openitg/wheel-fallback-banner.png".into());
static HELP_TEXT: LazyLock<Arc<str>> = LazyLock::new(|| {
    deadlib_present::font::replace_markers("Press &START; to continue")
        .as_ref()
        .into()
});
static BANNER_QUAD: LazyLock<Arc<[TexturedMeshVertex]>> = LazyLock::new(|| {
    [0, 2, 3, 0, 3, 1]
        .map(|corner| {
            let u = (corner % 2) as f32;
            let v = (corner / 2) as f32;
            TexturedMeshVertex {
                pos: [u, v, 0.0],
                uv: [u, v],
                color: WHITE,
                ..Default::default()
            }
        })
        .into()
});

#[derive(Clone, Copy)]
struct Seat {
    panel_x: f32,
    grade_hold_x: f32,
    grade_x: f32,
    percent_x: f32,
    difficulty: [f32; 2],
    outward: f32,
    tint: [f32; 4],
}

impl Seat {
    fn new(p2: bool, solo_double: bool) -> Self {
        let (panel_x, grade_hold_x, grade_x, percent_x, difficulty_x, outward, tint) = if p2 {
            (329.0, 480.0, 376.0, 504.0, 558.0, 1.0, P2)
        } else {
            (39.0, 160.0, 86.0, 220.0, 80.0, -1.0, P1)
        };
        let (shift, difficulty) = if solo_double {
            (-147.0 * outward, [difficulty_x - 75.0 * outward, 90.0])
        } else {
            (0.0, [difficulty_x, 145.0])
        };
        Self {
            panel_x: panel_x + shift,
            grade_hold_x: grade_hold_x + shift,
            grade_x: grade_x + shift,
            percent_x: percent_x + shift,
            difficulty,
            outward,
            tint,
        }
    }
}

#[derive(Default)]
pub struct OpenItgEvaluation {
    life: [Option<life_graph::LifeGraph>; 2],
}

impl OpenItgEvaluation {
    pub fn reset(&mut self) {
        self.life = [None, None];
    }

    fn graph(
        &mut self,
        slot: usize,
        timeline: &deadsync_theme::views::StageLifeTimeline,
    ) -> &life_graph::LifeGraph {
        let graph = &mut self.life[slot];
        if graph.as_ref().is_none_or(|graph| !graph.matches(timeline)) {
            *graph = Some(life_graph::LifeGraph::new(timeline));
        }
        graph.as_ref().unwrap()
    }
    pub fn push_overview(
        &mut self,
        actors: &mut Vec<Actor>,
        assets: &dyn AssetView,
        page: EvaluationOverview<'_>,
    ) {
        let viewport = [screen_width(), screen_height()];
        let layout = Layout::centered(viewport[0], viewport[1]);
        let departure = page.departure_elapsed.unwrap_or_default();
        let elapsed = page.elapsed.max(0.0) + departure;
        let stats_progress = decelerate(elapsed - STATS_START_SECONDS, STATS_ENTER_SECONDS);
        let solo_double = page.session_style.double && page.players.iter().flatten().count() == 1;
        let double_distance = if solo_double { 2.0 } else { 1.0 };
        let half_width_pixels = viewport[0] * 0.5;
        let (grade_departure, stats_departure, half_width) =
            page.departure_elapsed.map_or((0.0, 0.0, 0.0), |elapsed| {
                // The departure queues behind any unfinished stats entrance tween.
                let stats_wait = (STATS_START_SECONDS + STATS_ENTER_SECONDS - page.elapsed).max(0.0);
                (
                    accelerate(elapsed, DEPARTURE_MOTION_SECONDS),
                    accelerate(elapsed - stats_wait, DEPARTURE_MOTION_SECONDS),
                    half_width_pixels / layout.size([1.0, 1.0])[0],
                )
            });
        let stats_slide = stats_departure * half_width;
        let stats_visible = stats_progress > 0.0 && stats_departure < 1.0;

        push_arcade_backdrop(actors, viewport, elapsed * 0.5);
        push_header(actors, layout, elapsed, departure);
        push_result_reveal(actors, layout, &page, elapsed);
        if stats_visible {
            push_banner(actors, layout, &page, stats_progress, elapsed, stats_slide);
            if let Some(stage_number) = page.stage_number {
                push_round(
                    actors,
                    layout,
                    stage_number,
                    (1.0 - stats_progress + stats_departure) * half_width_pixels,
                );
            }
        }
        if stats_progress > 0.0 {
            push_footer(actors, layout, &page, elapsed, departure);
        }

        for (slot, player) in page.players.iter().enumerate() {
            let Some(player) = player else { continue };
            let seat = Seat::new(player.p2, solo_double);
            if stats_visible {
                let remaining = (1.0 - stats_progress) * seat.outward;
                let departure_shift = stats_slide * seat.outward * double_distance;
                let body_shift = remaining * (WIDTH * 0.5 * double_distance) + departure_shift;
                let difficulty_shift = remaining * (WIDTH * 0.5) + seat.outward * stats_slide;
                push_difficulty(actors, layout, seat, player, difficulty_shift);
                push_player_stats(actors, layout, seat, player, body_shift);
                if !player.disqualified
                    && let Some(rank) = player.machine_rank
                {
                    let record_layout = layout.translated_x(layout.size([departure_shift, 0.0])[0]);
                    push_machine_record(actors, assets, record_layout, seat, player.p2, rank, elapsed);
                }
                award::push_awards(actors, layout, seat.panel_x + body_shift, player, elapsed);
                let graph = self.graph(slot, player.steps_life);
                let life_x = seat.panel_x + body_shift;
                graph.push(actors, layout, [life_x, LIFE_Y], LIFE_SIZE, player.p2);
                if player.grade != Grade::Failed {
                    graph.push_barely(
                        actors,
                        layout,
                        [life_x, LIFE_Y + LIFE_SIZE[1] * 0.5],
                        LIFE_SIZE[0],
                        elapsed,
                    );
                }
                push_course_boundaries(actors, layout, player, seat.panel_x + body_shift);
                push_combo_runs(actors, layout, player, seat.panel_x + body_shift);
            }
            if grade_departure < 1.0 {
                let arrived = decelerate(elapsed, GRADE_ENTER_SECONDS);
                let settle = decelerate(elapsed - GRADE_SETTLE_START, GRADE_ENTER_SECONDS);
                let grade_x = (seat.grade_x - seat.grade_hold_x).mul_add(settle, seat.grade_hold_x);
                let slide = grade_departure * half_width * double_distance;
                let x = (1.0 - arrived).mul_add(seat.outward * 300.0, grade_x) + seat.outward * slide;
                let y = 22.0_f32.mul_add(settle, 180.0);
                let scale = (-1.5_f32).mul_add(settle, 2.5);
                grade::push_grade(actors, layout, [x, y], scale, player.grade, elapsed);
            }
        }
    }
}

fn push_machine_record(
    actors: &mut Vec<Actor>,
    assets: &dyn AssetView,
    layout: Layout,
    seat: Seat,
    p2: bool,
    rank: u32,
    elapsed: f32,
) {
    let Some(&label) = rank
        .checked_sub(1)
        .and_then(|index| MACHINE_RECORDS.get(index as usize))
    else {
        return;
    };
    let x = seat.panel_x + if p2 { 177.0 } else { 183.0 };
    let shade = 0.75 - 0.25 * (elapsed.fract() * std::f32::consts::TAU).cos();
    let mut text = layout.text(
        [x, 227.0],
        0.5,
        TextContent::Static(label),
        0.5,
        [shade, shade, shade, 1.0],
    );
    text.font(MENU_NORMAL_FONT);
    text.shadowlength(layout.size([1.0, 0.0])[0]);
    let mut text = text.build(0);
    let reveal = (elapsed - STATS_START_SECONDS) / STATS_ENTER_SECONDS;
    if reveal < 1.0
        && let Actor::Text { clip, .. } = &mut text
    {
        let width = TextMetrics::new(assets.fonts(), MENU_NORMAL_FONT).width(label) * 0.5;
        let left = if p2 {
            x + width * (0.5 - reveal)
        } else {
            x - width * 0.5
        };
        let [left, top] = layout.position(left, 215.0);
        let [width, height] = layout.size([width * reveal, 24.0]);
        *clip = Some([left, top, width, height]);
    }
    actors.push(text);
}

pub(crate) fn push_arcade_backdrop(
    actors: &mut Vec<Actor>,
    [width, height]: [f32; 2],
    motion: f32,
) {
    push_red_backdrop(actors, [width, height], motion, -30);
    for (y, phase, speed, opacity) in [
        (120.0, 0.2, 0.12, 0.2),
        (166.0, 0.5, 0.08, 0.25),
        (222.0, 0.8, 0.07, 0.3),
        (267.0, 0.9, 0.09, 0.3),
        (323.0, 0.2, 0.13, 0.2),
        (378.0, 0.5, 0.15, 0.1),
    ] {
        let u = (speed * motion + phase) % 0.95;
        let mut streak = sprite(
            "openitg/evaluation-streak.png",
            [width * 0.5, y * height / HEIGHT],
            [width, 10.0 * height / HEIGHT],
            [0.855, 0.475, 0.475, opacity],
            -28,
        );
        streak.customtexturerect([u, 0.0, u + 0.05, 1.0]);
        actors.push(streak.build(0));
    }
    grade::push_background_arrow(actors, [width, height], motion);
    push_red_streaks(actors, [width, height], 1.0, -20);
}

fn push_result_reveal(
    actors: &mut Vec<Actor>,
    layout: Layout,
    page: &EvaluationOverview<'_>,
    elapsed: f32,
) {
    if !(0.25..3.2).contains(&elapsed) {
        return;
    }
    let passed = page
        .players
        .iter()
        .flatten()
        .any(|player| player.grade != Grade::Failed);
    let (text, text_size, glow, glow_size) = if passed {
        (
            "openitg/evaluation-cleared.png",
            [626.0, 188.0],
            "openitg/evaluation-cleared-glow.png",
            [626.0, 110.0],
        )
    } else {
        (
            "openitg/evaluation-failed.png",
            [522.0, 128.0],
            "openitg/evaluation-failed-glow.png",
            [522.0, 86.0],
        )
    };
    let position = layout.position(320.0, 340.0);
    let sweep = ((elapsed - 0.25) / 0.7).clamp(0.0, 1.0);
    let alpha = ((3.2 - elapsed) / 0.3).clamp(0.0, 1.0);
    let mut word = sprite(
        text,
        position,
        layout.size(text_size),
        [1.0, 1.0, 1.0, alpha],
        102,
    );
    word.cropright(1.3 - sweep * 1.6);
    word.faderight(0.1);
    actors.push(word.build(0));
    if sweep < 1.0 {
        let mut edge = sprite(glow, position, layout.size(glow_size), WHITE, 103);
        edge.cropleft(sweep * 1.3 - 0.3);
        edge.cropright(1.0 - sweep * 1.3);
        edge.fadeleft(0.1);
        edge.faderight(0.1);
        actors.push(edge.build(0));
    }
}

fn push_course_boundaries(
    actors: &mut Vec<Actor>,
    layout: Layout,
    player: &EvaluationPlayerOverview<'_>,
    x: f32,
) {
    let duration = player.steps_life.possible_seconds;
    if !duration.is_finite() || duration <= 0.0 {
        return;
    }
    let stages = player.course_graph_stages;
    let mut elapsed = 0.0;
    for stage in &stages[..stages.len().saturating_sub(1)] {
        elapsed += stage.step_seconds.max(0.0);
        if elapsed > 0.0 && elapsed < duration {
            let center = x + LIFE_SIZE[0] * (elapsed / duration);
            actors.push(layout.panel(
                center - 1.0,
                LIFE_Y,
                [2.0, LIFE_SIZE[1]],
                [1.0, 1.0, 1.0, 0.25],
                9,
            ));
        }
    }
}

fn push_combo_runs(
    actors: &mut Vec<Actor>,
    layout: Layout,
    player: &EvaluationPlayerOverview<'_>,
    x: f32,
) {
    let [first, last] = player.graph_seconds;
    let max_count = player
        .combo_runs
        .iter()
        .map(|run| run.count)
        .max()
        .unwrap_or(0);
    if max_count < 5 || last <= first {
        return;
    }
    let width = LIFE_SIZE[0];
    let bar_x = |second: f32| width * ((second - first) / (last - first)).clamp(0.0, 1.0);
    for run in player.combo_runs.iter().filter(|run| run.count >= 5) {
        let start = bar_x(run.start_second);
        let end = bar_x(run.end_second);
        let max_run = run.count == max_count;
        let key = if max_run {
            "openitg/evaluation-combo-max.png"
        } else if player.p2 {
            "openitg/evaluation-combo-p2.png"
        } else {
            "openitg/evaluation-combo-p1.png"
        };
        let center = x + f32::midpoint(start, end);
        actors.push(
            sprite(
                key,
                layout.position(center, COMBO_Y),
                layout.size([(end - start).max(1.0), 18.0]),
                WHITE,
                3,
            )
            .build(0),
        );
        if max_run {
            let midpoint = center.clamp(x + 12.0, x + width - 12.0);
            let mut count = text(
                layout,
                [midpoint, COMBO_Y - 2.0],
                0.5,
                TextContent::inline_u32(run.count),
                0.5,
                WHITE,
            );
            count.shadowlength(2.0);
            actors.push(count.build(0));
        }
    }
}

fn decelerate(elapsed: f32, duration: f32) -> f32 {
    let remaining = 1.0 - (elapsed / duration).clamp(0.0, 1.0);
    remaining.mul_add(-remaining, 1.0)
}

fn accelerate(elapsed: f32, duration: f32) -> f32 {
    let fraction = (elapsed / duration).clamp(0.0, 1.0);
    fraction * fraction
}

pub(super) fn push_departure_overlay(actors: &mut Vec<Actor>, elapsed: f32) {
    let alpha = ((elapsed - 0.2) / 0.1).clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return;
    }
    let layout = Layout::centered(screen_width(), screen_height());
    let mut label = text(
        layout,
        [320.0, 240.0],
        0.5,
        "Loading...".into(),
        1.4,
        [1.0, 0.63, 0.62, alpha],
    );
    label.font(MENU_HEADING_FONT);
    label.shadowlength(layout.size([3.0, 0.0])[0]);
    label.z(1200);
    actors.push(italic(label.build(0), -0.16));
}

fn push_header(actors: &mut Vec<Actor>, layout: Layout, elapsed: f32, departure: f32) {
    let exit = accelerate(departure, 0.2);
    if elapsed <= 2.0 || exit >= 1.0 {
        return;
    }
    let reveal = bounceend_p(((elapsed - 2.0) / 0.3).min(1.0));
    let layout = layout.translated_x(-layout.position(0.0, 0.0)[0]);
    let mut headline = text(
        layout,
        [36.0, 38.0],
        0.0,
        "CHECK YOUR STATS".into(),
        1.0,
        [1.0, 1.0, 1.0, 1.0 - exit],
    );
    let [zoom_x, zoom_y] = layout.size([
        reveal * (1.0 + exit),
        (6.0 - 5.0 * reveal).max(0.6) * (1.0 - exit),
    ]);
    headline.font(MENU_HEADING_FONT);
    headline.zoomx(zoom_x);
    headline.zoomy(zoom_y);
    headline.shadowlength(layout.size([4.0, 0.0])[0]);
    actors.push(italic(headline.build(0), -0.16));
}

fn push_round(actors: &mut Vec<Actor>, layout: Layout, round: usize, shift: f32) {
    let layout = layout.translated_x(-layout.position(0.0, 0.0)[0] - shift);
    let left = 24.0;
    if let Some((key, size)) = crate::gameplay::round_artwork(round) {
        let size = size.map(|axis| axis * 0.16);
        actors.push(
            sprite(
                key,
                layout.position(left + size[0] * 0.5, 72.0),
                layout.size(size),
                WHITE,
                1,
            )
            .build(0),
        );
    } else {
        let mut label = text(
            layout,
            [left, 72.0],
            0.0,
            value(format_args!("ROUND {round}")),
            0.55,
            WHITE,
        );
        label.font(TITLE_FONT);
        actors.push(label.build(0));
    }
}

fn push_footer(
    actors: &mut Vec<Actor>,
    layout: Layout,
    page: &EvaluationOverview<'_>,
    elapsed: f32,
    departure: f32,
) {
    let alpha = ((elapsed - STATS_START_SECONDS) / 0.5).clamp(0.0, 1.0);
    if departure < DEPARTURE_MOTION_SECONDS {
        let mut help = text(
            layout,
            [320.0, 445.0],
            0.5,
            (&*HELP_TEXT).into(),
            0.7,
            WHITE,
        );
        help.font(TITLE_FONT);
        if departure > 0.0 {
            let zoom_y = 0.7 * (1.0 - departure / DEPARTURE_MOTION_SECONDS);
            help.zoomy(layout.size([0.0, zoom_y])[1]);
        }
        help.diffuse([
            1.0,
            1.0,
            1.0,
            alpha * (0.7 + 0.3 * (elapsed * std::f32::consts::TAU).cos()),
        ]);
        actors.push(help.build(0));
    }
    for (p2, x) in [(false, 42.0), (true, 598.0)] {
        if !page.players.iter().flatten().any(|player| player.p2 == p2) {
            let edge = layout.position(0.0, 0.0)[0];
            let layout = layout.translated_x(if p2 { edge } else { -edge });
            let mut label = text(
                layout,
                [x, 464.0],
                if p2 { 1.0 } else { 0.0 },
                "NOT PRESENT".into(),
                0.5,
                [1.0, 1.0, 1.0, alpha],
            );
            label.font(TITLE_FONT);
            actors.push(label.build(0));
        }
    }
}

fn push_banner(
    actors: &mut Vec<Actor>,
    layout: Layout,
    page: &EvaluationOverview<'_>,
    progress: f32,
    elapsed: f32,
    rise: f32,
) {
    let y = 114.0 - (1.0 - progress) * (WIDTH * 0.5) - rise;
    let key = page.banner_key.unwrap_or(&FALLBACK_BANNER);
    let crop = cover_uv(key, BANNER_SIZE[0], BANNER_SIZE[1]).unwrap_or([0.0, 0.0, 1.0, 1.0]);
    actors.push(banner_mesh(
        layout.position(320.0, y + 2.0),
        layout.size(BANNER_FRAME_SIZE),
        &BANNER_MASK,
        [0.0, 0.0, 1.0, 1.0],
        true,
    ));
    actors.push(banner_mesh(
        layout.position(320.0, y),
        layout.size(BANNER_SIZE),
        key,
        crop,
        false,
    ));
    actors.push(
        sprite(
            "openitg/evaluation-banner-frame.png",
            layout.position(320.0, y + 2.0),
            layout.size(BANNER_FRAME_SIZE),
            WHITE,
            7,
        )
        .build(0),
    );

    let alpha = ((elapsed - STATS_START_SECONDS - STATS_ENTER_SECONDS) / CAPTION_FADE_SECONDS)
        .clamp(0.0, 1.0);
    if page.banner_key.is_none() && alpha > 0.0 {
        let mut title = text(
            layout,
            [320.0, 65.0 - rise],
            0.5,
            if page.course_summary && page.title.is_empty() {
                "Untitled Course".into()
            } else {
                page.title.into()
            },
            0.55,
            [1.0, 1.0, 1.0, alpha],
        );
        title.maxwidth(layout.size([272.0, 0.0])[0]);
        actors.push(title.build(0));
    }
    if page.course_summary {
        actors.push(
            text(
                layout,
                [36.0, y - 38.0],
                0.0,
                "COURSE RESULTS".into(),
                0.55,
                WHITE,
            )
            .build(0),
        );
    }
}

fn banner_mesh(
    offset: [f32; 2],
    [width, height]: [f32; 2],
    key: &Arc<str>,
    [u0, v0, u1, v1]: [f32; 4],
    mask: bool,
) -> Actor {
    Actor::TexturedMesh {
        environment: None,
        align: [0.5; 2],
        offset,
        world_z: if mask { 0.125 } else { 0.0 },
        size: [SizeSpec::Px(width), SizeSpec::Px(height)],
        local_transform: Mat4::from_scale(Vec3::new(width, height, 1.0)),
        texture: MeshTexture::Key(Arc::clone(key)),
        tint: WHITE,
        vertex_color_encoding: Default::default(),
        glow: [0.0; 4],
        vertices: Arc::clone(&BANNER_QUAD),
        geom_cache_key: BANNER_QUAD.as_ptr() as u64,
        uv_scale: [u1 - u0, v1 - v0],
        uv_offset: [u0, v0],
        uv_tex_shift: [0.0; 2],
        depth_test: !mask,
        depth_write: mask,
        depth_test_on_fail: false,
        clear_depth: mask,
        clear_depth_after: !mask,
        cull_mode: CullMode::None,
        visible: true,
        blend: if mask {
            BlendMode::NoEffect
        } else {
            BlendMode::Alpha
        },
        z: if mask { 5 } else { 6 },
    }
}

fn push_difficulty(
    actors: &mut Vec<Actor>,
    layout: Layout,
    seat: Seat,
    player: &EvaluationPlayerOverview<'_>,
    shift: f32,
) {
    let [x, y] = seat.difficulty;
    let x = x + shift;
    let (row, _, _) = crate::select_music::difficulty_style(player.difficulty);
    let label = ["Novice", "Easy", "Medium", "Hard", "Expert", "Edit"][row];
    let direction = -seat.outward;
    let mut tab = sprite(
        "openitg/gameplay-difficulty.png",
        layout.position(x, y),
        layout.size([114.0, 40.0]),
        WHITE,
        2,
    );
    tab.customtexturerect([0.0, row as f32 / 6.0, 1.0, (row + 1) as f32 / 6.0]);
    tab.zoomx(direction);
    actors.push(tab.build(0));

    let mut name = text(
        layout,
        [x - direction * 14.0, y],
        0.5,
        label.into(),
        0.5,
        WHITE,
    );
    name.maxwidth(layout.size([61.0, 0.0])[0]);
    name.shadowlength(2.0);
    actors.push(name.build(0));
    let mut meter = text(
        layout,
        [x + direction * 30.0, y],
        0.5,
        TextContent::inline_u32(player.meter),
        0.5,
        WHITE,
    );
    meter.shadowlength(2.0);
    actors.push(meter.build(0));
}

fn push_player_stats(
    actors: &mut Vec<Actor>,
    layout: Layout,
    seat: Seat,
    player: &EvaluationPlayerOverview<'_>,
    shift: f32,
) {
    let x = seat.panel_x + shift;
    let percent_x = seat.percent_x + shift;
    actors.push(
        sprite(
            "openitg/evaluation-grade-frame.png",
            layout.position(x + 136.0, 294.0),
            layout.size([286.0, 260.0]),
            WHITE,
            1,
        )
        .build(0),
    );
    if !player.mods.is_empty() {
        let options_x = percent_x - if player.p2 { 2.0 } else { 0.0 };
        let mut mods = text(
            layout,
            [options_x, 183.0],
            0.5,
            player.mods.into(),
            0.5,
            WHITE,
        );
        mods.wrapwidthpixels(386.0);
        mods.shadowlength(2.0);
        actors.push(mods.build(0));
    }
    let mut percent = text(
        layout,
        [percent_x, 212.0],
        0.5,
        value(format_args!("{:5.2}%", player.percent)),
        1.0,
        seat.tint,
    );
    percent.font(GAMEPLAY_SCORE_FONT);
    percent.maxwidth(layout.size([156.0, 0.0])[0]);
    actors.push(percent.build(0));

    let accompanying = [
        ("EX", player.ex_percent),
        ("H.EX", player.hard_ex_percent),
        ("OWN", player.individual_percent),
    ];
    let count = accompanying
        .iter()
        .filter(|(_, percent)| percent.is_some())
        .count();
    let detail_y = if player
        .machine_rank
        .is_some_and(|rank| (1..=10).contains(&rank))
    {
        432.0
    } else {
        230.0
    };
    for (index, (name, percent)) in accompanying
        .into_iter()
        .filter_map(|(name, percent)| percent.map(|percent| (name, percent)))
        .enumerate()
    {
        let dx = (index as f32 - (count - 1) as f32 * 0.5) * 55.0;
        let mut detail = text(
            layout,
            [percent_x + dx, detail_y],
            0.5,
            value(format_args!("{name} {percent:.2}%")),
            0.38,
            WHITE,
        );
        detail.maxwidth(layout.size([52.0, 0.0])[0]);
        actors.push(detail.build(0));
    }

    if player.disqualified {
        actors.push(
            text(
                layout,
                [percent_x + 2.0, 228.0],
                0.5,
                "DISQUALIFIED".into(),
                0.5,
                seat.tint,
            )
            .build(0),
        );
    }
    push_counts(actors, layout, x, seat.tint, player);
}

fn push_counts(
    actors: &mut Vec<Actor>,
    layout: Layout,
    x: f32,
    tint: [f32; 4],
    player: &EvaluationPlayerOverview<'_>,
) {
    let judge_x = x + if player.p2 { 99.0 } else { 97.0 };
    for (index, (label, count)) in JUDGMENTS.into_iter().zip(player.judgments).enumerate() {
        let y = (index as f32).mul_add(COUNT_PITCH, COUNT_FIRST_Y);
        actors.push(text(layout, [x + 8.0, y], 0.0, label.into(), 0.5, WHITE).build(0));
        actors.push(number(
            layout,
            [judge_x, y],
            0.5,
            value(format_args!("{count:4}")),
            tint,
        ));
    }
    for (index, (label, [hit, total])) in [
        ("Jumps", player.jumps),
        ("Holds", player.holds),
        ("Mines", player.mines),
        ("Hands", player.hands),
        ("Rolls", player.rolls),
    ]
    .into_iter()
    .enumerate()
    {
        let y = (index as f32).mul_add(COUNT_PITCH, COUNT_FIRST_Y);
        actors.push(text(layout, [x + 140.0, y], 0.0, label.into(), 0.5, WHITE).build(0));
        actors.push(number(
            layout,
            [x + 263.0, y],
            1.0,
            value(format_args!("{hit:3}/{total:3}")),
            tint,
        ));
    }
    let y = 5.0_f32.mul_add(COUNT_PITCH, COUNT_FIRST_Y);
    let mut combo = text(layout, [x + 140.0, y], 0.0, "Max Combo".into(), 0.5, WHITE);
    combo.maxwidth(layout.size([65.0, 0.0])[0]);
    actors.push(combo.build(0));
    actors.push(number(
        layout,
        [x + 263.0, y],
        1.0,
        value(format_args!("{:5}", player.max_combo)),
        tint,
    ));
}

fn text(
    layout: Layout,
    position: [f32; 2],
    alignment: f32,
    content: TextContent,
    zoom: f32,
    rgba: [f32; 4],
) -> TextBuilder {
    let mut label = layout.text(position, alignment, content, zoom, rgba);
    label.font(MENU_NORMAL_FONT);
    label
}

fn number(
    layout: Layout,
    position: [f32; 2],
    alignment: f32,
    content: TextContent,
    color: [f32; 4],
) -> Actor {
    let mut label = text(layout, position, alignment, content, 0.5, color);
    label.font(EVALUATION_NUMBER_FONT);
    label.build(0)
}

fn value(args: std::fmt::Arguments<'_>) -> TextContent {
    TextContent::inline_format(args).unwrap_or_else(|| TextContent::Owned(args.to_string()))
}
#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::actors::SpriteSource;
    use deadlib_present::space::{Metrics, with_current_metrics};
    use std::sync::Arc;

    static TEST_LIFE: LazyLock<deadsync_theme::views::StageLifeTimeline> =
        LazyLock::new(|| deadsync_theme::views::StageLifeTimeline {
            samples: Arc::from([(0.0, 0.5), (80.0, 0.8)]),
            possible_seconds: 120.0,
            elapsed_seconds: 120.0,
            ..Default::default()
        });

    fn has_life_graph(actors: &[Actor], p2: bool, x: f32) -> bool {
        let key = if p2 {
            "openitg/evaluation-life-ffc-p2.png"
        } else {
            "openitg/evaluation-life-ffc-p1.png"
        };
        actors.iter().any(|actor| matches!(actor,
            Actor::TexturedMesh { texture, offset, size: [SizeSpec::Px(271.0), SizeSpec::Px(39.0)], z: 8, .. }
                if texture.texture_key() == Some(key) && (offset[0] - x).abs() < 0.001 && offset[1] == 239.0
        ))
    }

    fn player(mods: &Arc<str>) -> EvaluationPlayerOverview<'_> {
        EvaluationPlayerOverview {
            p2: false,
            grade: Grade::Tier08,
            percent: 97.65,
            numeric_score: 0,
            radar_possible: [0.0; 5],
            radar_achievement: None,
            stages_passed: 1,
            individual_percent: None,
            ex_percent: None,
            hard_ex_percent: None,
            difficulty: "Expert",
            meter: 11,
            mods,
            judgments: [963, 104, 27, 4, 2, 8],
            row_window_counts: Default::default(),
            score_valid: true,
            dropped_hold_or_roll: false,
            jumps: [18, 20],
            holds: [72, 79],
            mines: [32, 36],
            hands: [15, 16],
            rolls: [7, 9],
            max_combo: 611,
            combo_runs: &[],
            graph_seconds: [0.0, 120.0],
            course_graph_stages: &[],
            steps_life: &TEST_LIFE,
            machine_rank: Some(3),
            personal_rank: None,
            disqualified: false,
        }
    }

    fn has_text(actors: &[Actor], text: &str, position: [f32; 2]) -> bool {
        actors.iter().any(|actor| matches!(actor,
            Actor::Text { content, offset, .. } if content.as_str() == text && *offset == position
        ))
    }

    fn has_texture(actors: &[Actor], texture: &str) -> bool {
        actors.iter().any(|actor| {
            matches!(actor,
                Actor::Sprite { source: SpriteSource::TextureStatic(key), .. } if *key == texture
            )
        })
    }

    #[test]
    fn departure_moves_the_results_and_keeps_the_backdrop_at_both_widths() {
        let title = Arc::from("Fixture");
        let mods = Arc::from("C500");
        let assets = AssetManager::<()>::new();
        for width in [640.0, 854.0] {
            with_current_metrics(Metrics::centered(width, 480.0), || {
                let mut presentation = OpenItgEvaluation::default();
                let mut actors = Vec::new();
                let mut frame = |actors: &mut Vec<Actor>, elapsed| {
                    actors.clear();
                    presentation.push_overview(actors, &assets, EvaluationOverview {
                        title: &title,
                        artist: &title,
                        banner_key: None,
                        course_summary: false,
                        stage_number: Some(1),
                        session_style: Default::default(),
                        elapsed: EVALUATION_OVERVIEW_SETTLED_SECONDS,
                        departure_elapsed: Some(elapsed),
                        players: [Some(player(&mods)), Some(EvaluationPlayerOverview {
                            p2: true,
                            ..player(&mods)
                        })],
                    });
                    push_departure_overlay(actors, elapsed);
                };
                let edge = (width - WIDTH) * 0.5;
                let half_motion = width * 0.125;
                frame(&mut actors, 0.15);
                assert!(has_life_graph(&actors, false, 39.0 + edge - half_motion));
                assert!(has_life_graph(&actors, true, 329.0 + edge + half_motion));
                assert!(actors.iter().any(|actor| matches!(actor, Actor::TexturedMesh {
                    offset, z: 6, ..
                } if (offset[1] - (114.0 - half_motion)).abs() < 0.001)));
                assert!(!has_text(&actors, "Loading...", [width * 0.5, 240.0]));
                let life = actors.iter().find_map(|actor| match actor {
                    Actor::TexturedMesh { vertices, z: 8, .. } => Some(Arc::clone(vertices)),
                    _ => None,
                }).unwrap();

                frame(&mut actors, 0.25);
                assert!(actors.iter().any(|actor| matches!(actor, Actor::Text {
                    content, color, z: 1200, ..
                } if content.as_str() == "Loading..." && (color[3] - 0.5).abs() < 0.001)));
                assert!(actors.iter().any(|actor| matches!(actor, Actor::TexturedMesh {
                    vertices, z: 8, ..
                } if Arc::ptr_eq(&life, vertices))));

                frame(&mut actors, 0.40);
                assert!(!actors.iter().any(|actor| matches!(actor,
                    Actor::TexturedMesh { z: 6 | 8, .. }
                )));
                assert!(has_texture(&actors, "openitg/evaluation-streak.png"));
                assert!(has_text(&actors, "Loading...", [width * 0.5, 240.0]));
            });
        }
    }

    #[test]
    fn native_and_wide_overviews_place_the_banner_and_both_canonical_life_graphs() {
        use deadsync_rules::timing::WindowCounts;
        use deadsync_theme::views::ComboRun;
        let title = Arc::from("DeadSync Stage");
        let artist = Arc::from("Artist");
        let mods = Arc::from("C500, Overhead");
        let banner = Arc::from("stage-banner");
        let combo_runs = [
            ComboRun {
                start_second: 3.0,
                end_second: 7.0,
                count: 4,
                carried: 500,
            },
            ComboRun {
                start_second: 15.0,
                end_second: 45.0,
                count: 9,
                carried: 0,
            },
            ComboRun {
                start_second: 75.0,
                end_second: 105.0,
                count: 9,
                carried: 0,
            },
        ];
        for (width, delta) in [(640.0, 0.0), (854.0, 107.0)] {
            with_current_metrics(Metrics::centered(width, 480.0), || {
                let mut actors = Vec::new();
                OpenItgEvaluation::default().push_overview(
                    &mut actors,
                    &AssetManager::<()>::new(),
                    EvaluationOverview {
                        title: &title,
                        artist: &artist,
                        banner_key: Some(&banner),
                        course_summary: false,
                        stage_number: Some(1),
                        session_style: Default::default(),
                        elapsed: EVALUATION_OVERVIEW_SETTLED_SECONDS,
                        departure_elapsed: None,
                        players: [
                            Some(EvaluationPlayerOverview {
                                combo_runs: &combo_runs,
                                row_window_counts: WindowCounts {
                                    w2: 2,
                                    w3: 1,
                                    ..Default::default()
                                },
                                ..player(&mods)
                            }),
                            Some(EvaluationPlayerOverview {
                                p2: true,
                                grade: Grade::Failed,
                                percent: 65.0,
                                disqualified: true,
                                max_combo: 8158,
                                machine_rank: None,
                                ..player(&mods)
                            }),
                        ],
                    },
                );
                assert!(actors.iter().any(|actor| matches!(actor,
                    Actor::TexturedMesh { offset, size: [SizeSpec::Px(210.0), SizeSpec::Px(66.0)],
                        texture, depth_test: true, .. }
                        if *offset == [320.0 + delta, 114.0] && texture.texture_key() == Some("stage-banner")
                )));
                assert!(has_life_graph(&actors, false, 39.0 + delta));
                assert!(has_life_graph(&actors, true, 329.0 + delta));
                assert!(has_texture(&actors, "openitg/gameplay-stage-1.png"));
                assert!(has_text(&actors, "C500, Overhead", [220.0 + delta, 183.0]));
                assert!(has_text(&actors, "C500, Overhead", [502.0 + delta, 183.0]));
                assert!(has_text(&actors, "97.65%", [220.0 + delta, 212.0]));
                assert!(has_text(&actors, "65.00%", [504.0 + delta, 212.0]));
                assert!(has_text(
                    &actors,
                    "Machine Record #3",
                    [222.0 + delta, 227.0]
                ));
                assert!(!has_text(
                    &actors,
                    "Machine Record #3",
                    [506.0 + delta, 227.0]
                ));
                assert!(has_text(&actors, "One Great", [310.0 + delta, 305.0]));
                assert!(!has_text(
                    &actors,
                    "8000 Peak Combo",
                    [330.0 + delta, 305.0]
                ));
                assert!(actors.iter().any(|actor| matches!(actor,
                    Actor::TexturedMesh { offset, texture, .. }
                        if *offset == [280.0 + delta, 258.0]
                            && texture.texture_key() == Some("openitg/evaluation-award-flag-green.png")
                )));
                for (key, x) in [
                    ("openitg/evaluation-red.png", 86.0),
                    ("openitg/evaluation-purple.png", 376.0),
                ] {
                    assert!(actors.iter().any(
                        |actor| matches!(actor, Actor::TexturedMesh { texture, offset, .. }
                            if texture.texture_key() == Some(key) && *offset == [x + delta, 202.0]
                        )
                    ));
                }
                assert!(has_text(&actors, "DISQUALIFIED", [506.0 + delta, 228.0]));
                assert!(has_text(&actors, " 18/ 20", [302.0 + delta, 325.0]));
                assert!(has_text(&actors, " 18/ 20", [592.0 + delta, 325.0]));
                assert!(has_text(&actors, "9", [106.75 + delta, 285.0]));
                assert!(has_text(&actors, "9", [242.25 + delta, 285.0]));
                assert_eq!(actors.iter().filter(|actor| matches!(actor,
                    Actor::Sprite { source: SpriteSource::TextureStatic("openitg/evaluation-combo-max.png"),
                        offset, size: [_, SizeSpec::Px(18.0)], .. } if offset[1] == COMBO_Y
                )).count(), 2);
                assert!(has_text(&actors, &HELP_TEXT, [320.0 + delta, 445.0]));
                assert!(has_texture(&actors, "openitg/evaluation-banner-frame.png"));
                assert!(has_texture(&actors, "openitg/evaluation-grade-frame.png"));
            });
        }
    }

    #[test]
    fn grade_enters_holds_and_settles_before_the_stats_slide() {
        let title = Arc::from("Stage");
        let empty = Arc::from("");
        with_current_metrics(Metrics::centered(640.0, 480.0), || {
            for (elapsed, grade_xy) in [
                (0.0, [-140.0, 180.0]),
                (0.5, [160.0, 180.0]),
                (2.49, [160.0, 180.0]),
                (2.75, [104.5, 196.5]),
                (3.0, [86.0, 202.0]),
            ] {
                let mut actors = Vec::new();
                OpenItgEvaluation::default().push_overview(
                    &mut actors,
                    &AssetManager::<()>::new(),
                    EvaluationOverview {
                        title: &title,
                        artist: &empty,
                        banner_key: None,
                        course_summary: false,
                        stage_number: Some(1),
                        session_style: Default::default(),
                        elapsed,
                        departure_elapsed: None,
                        players: [Some(player(&empty)), None],
                    },
                );
                assert!(actors.iter().any(|actor| matches!(actor,
                    Actor::TexturedMesh { texture, offset, .. }
                        if texture.texture_key() == Some("openitg/evaluation-red.png") && *offset == grade_xy
                )));
                assert_eq!(
                    has_text(&actors, "CHECK YOUR STATS", [36.0, 38.0]),
                    elapsed > 2.0
                );
                assert_eq!(
                    has_texture(&actors, "openitg/evaluation-cleared.png"),
                    elapsed > 0.25
                );
                assert!(!has_text(&actors, "97.65%", [220.0, 212.0]));
                assert!(!has_text(&actors, "Machine Record #3", [222.0, 227.0]));
                assert!(!has_texture(&actors, "openitg/gameplay-stage-1.png"));
                assert!(
                    !actors
                        .iter()
                        .any(|actor| matches!(actor, Actor::TexturedMesh { z: 8, .. }))
                );
            }
            let mut actors = Vec::new();
            OpenItgEvaluation::default().push_overview(
                &mut actors,
                &AssetManager::<()>::new(),
                EvaluationOverview {
                    title: &title,
                    artist: &empty,
                    banner_key: None,
                    course_summary: false,
                    stage_number: Some(1),
                    session_style: Default::default(),
                    elapsed: 3.15,
                    departure_elapsed: None,
                    players: [Some(player(&empty)), None],
                },
            );
            assert!(has_life_graph(&actors, false, -41.0));
            assert!(actors.iter().any(|actor| matches!(actor,
                Actor::Text { content, clip: Some(clip), .. }
                    if content.as_str() == "Machine Record #3" && clip[0] < 222.0 && clip[2] > 0.0
            )));
            assert!(actors.iter().any(|actor| matches!(actor,
                Actor::Sprite { source: SpriteSource::TextureStatic("openitg/gameplay-stage-1.png"), offset, .. }
                    if (offset[0] - (-56.0 + 498.0 * 0.08)).abs() < 0.001
                        && offset[1] == 72.0
            )));
        });
    }

    #[test]
    fn p2_solo_double_centers_the_panel_and_keeps_runtime_slot_and_prepared_stats() {
        let title = Arc::from("Course Name");
        let mods = Arc::from("2.30x");
        with_current_metrics(Metrics::centered(640.0, 480.0), || {
            let mut actors = Vec::new();
            OpenItgEvaluation::default().push_overview(
                &mut actors,
                &AssetManager::<()>::new(),
                EvaluationOverview {
                    title: &title,
                    artist: &Arc::from(""),
                    banner_key: None,
                    course_summary: true,
                    stage_number: None,
                    session_style: deadsync_theme::select_music::SessionStyleView { double: true, ..Default::default() },
                    elapsed: 5.0,
                    departure_elapsed: None,
                    players: [
                        Some(EvaluationPlayerOverview {
                            p2: true,
                            grade: Grade::Quint,
                            individual_percent: Some(93.44),
                            ex_percent: Some(91.25),
                            hard_ex_percent: Some(89.64),
                            ..player(&mods)
                        }),
                        None,
                    ],
                },
            );
            assert!(has_life_graph(&actors, true, 182.0));
            assert!(has_text(&actors, "COURSE RESULTS", [36.0, 76.0]));
            assert!(has_text(&actors, "Course Name", [320.0, 65.0]));
            assert!(has_text(&actors, "Expert", [497.0, 90.0]));
            assert_eq!(
                actors
                    .iter()
                    .filter(|actor| matches!(actor, Actor::TexturedMesh { texture, .. }
                        if texture.texture_key() == Some("openitg/evaluation-gold.png")
                    ))
                    .count(),
                5
            );
            assert!(has_text(&actors, "EX 91.25%", [302.0, 432.0]));
            assert!(has_text(&actors, "H.EX 89.64%", [357.0, 432.0]));
            assert!(has_text(&actors, "OWN 93.44%", [412.0, 432.0]));
            assert!(has_text(&actors, " 963", [281.0, 325.0]));
            assert!(has_text(&actors, " 72/ 79", [445.0, 341.0]));
            assert!(has_text(&actors, "  611", [445.0, 405.0]));
            assert!(has_text(&actors, "Machine Record #3", [359.0, 227.0]));
        });
    }
}
