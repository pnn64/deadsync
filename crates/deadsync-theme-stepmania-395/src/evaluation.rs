use deadlib_assets::{AssetView, present_dsl::cover_sprite};
use deadlib_present::actors::{Actor, TextAlign, TextContent};
use deadsync_theme::stage::EvaluationOverviewPresentation;
use deadsync_theme::views::{EvaluationOverview, EvaluationPlayerOverview, Grade};

use crate::gameplay::{difficulty_index, stage_artwork};
use crate::presentation::{Layout, push_footer, push_style_icon};
use crate::resources::{JUDGE_FONT, NORMAL_FONT, SCORE_FONT, STAGE_FONT};

#[derive(Default)]
pub(crate) struct StepManiaEvaluation;

fn arrive(elapsed: f32, delay: f32) -> f32 {
    1.0 - (1.0 - ((elapsed - delay) / 0.3).clamp(0.0, 1.0)).powi(2)
}
fn leave(elapsed: Option<f32>, delay: f32) -> f32 {
    elapsed.map_or(0.0, |time| ((time - delay) / 0.3).clamp(0.0, 1.0).powi(2))
}

impl EvaluationOverviewPresentation<Actor> for StepManiaEvaluation {
    fn push_overview(
        &mut self,
        actors: &mut Vec<Actor>,
        _assets: &dyn AssetView,
        page: EvaluationOverview<'_>,
    ) {
        push_page(actors, page, None);
    }
}

fn push_page(
    actors: &mut Vec<Actor>,
    page: EvaluationOverview<'_>,
    summary: Option<(&dyn AssetView, &[std::sync::Arc<str>])>,
) {
    let layout = Layout::current();
    let elapsed = page.elapsed.max(0.0);
    let departure = page.departure_elapsed;
    let banner_y = 100.0 + 480.0 * (1.0 - arrive(elapsed, 0.0) + leave(departure, 0.8));
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
    let header_exit = (departure.unwrap_or_default() / 0.5).clamp(0.0, 1.0);
    crate::play_selection::push_header(
        actors,
        if summary.is_some() {
            "stepmania-395/evaluation-summary-header"
        } else {
            "stepmania-395/evaluation-header"
        },
        elapsed,
        header_exit,
    );
    push_style_icon(actors, page.session_style, elapsed, header_exit);
    if let Some((assets, banners)) = summary {
        for (index, key) in banners.iter().take(5).enumerate() {
            let x = 260.0 + index as f32 * 30.0;
            let y = banner_y - 20.0 + index as f32 * 10.0;
            let key = if assets.has_texture_key(key) {
                key.clone()
            } else {
                "stepmania-395/music-fallback".into()
            };
            let [px, py] = layout.position([x, y]);
            actors.push(cover_sprite(
                key,
                px,
                py,
                124.0,
                39.0,
                layout.scale,
                2 + index as i16 * 2,
            ));
            actors.push(
                layout
                    .sprite(
                        "stepmania-395/evaluation-banner-frame",
                        [x, y],
                        [128.0, 44.0],
                        3 + index as i16 * 2,
                    )
                    .build(0),
            );
        }
    } else {
        let banner = page
            .banner_key
            .cloned()
            .unwrap_or_else(|| "stepmania-395/music-fallback".into());
        let [banner_x, banner_at_y] = layout.position([320.0, banner_y]);
        actors.push(cover_sprite(
            banner,
            banner_x,
            banner_at_y,
            248.0,
            78.0,
            layout.scale,
            2,
        ));
        actors.push(
            layout
                .sprite(
                    "stepmania-395/evaluation-banner-frame",
                    [320.0, banner_y],
                    [256.0, 88.0],
                    3,
                )
                .build(0),
        );
        if let Some(number) = page.stage_number {
            let (key, width) = stage_artwork(number);
            actors.push(
                layout
                    .sprite(key, [320.0, banner_y - 28.0], [width, 16.0], 4)
                    .build(0),
            );
        }
    }

    for index in 0..7 {
        let delay = (index + 1) as f32 * 0.1;
        let y = 160.0
            + index as f32 * 32.0
            + 480.0 * (1.0 - arrive(elapsed, delay) + leave(departure, 0.7 - index as f32 * 0.1));
        let mut label = layout.sprite(
            "stepmania-395/evaluation-judgments",
            [320.0, y],
            [106.0, 30.0],
            3,
        );
        label.customtexturerect([0.0, index as f32 / 8.0, 1.0, (index + 1) as f32 / 8.0]);
        actors.push(label.build(0));
    }
    let score_y = 380.0 + 480.0 * (1.0 - arrive(elapsed, 0.8) + leave(departure, 0.0));
    actors.push(
        layout
            .sprite(
                "stepmania-395/evaluation-score",
                [320.0, score_y],
                [102.0, 22.0],
                3,
            )
            .build(0),
    );
    for player in page.players.iter().flatten() {
        push_player(actors, layout, player, &page, banner_y, summary.is_some());
    }
    push_footer(
        actors,
        "Press &START; to continue",
        (elapsed / 0.5).clamp(0.0, 1.0)
            * (1.0 - departure.unwrap_or_default() / 0.5).clamp(0.0, 1.0),
    );
}

fn push_player(
    actors: &mut Vec<Actor>,
    layout: Layout,
    player: &EvaluationPlayerOverview<'_>,
    page: &EvaluationOverview<'_>,
    banner_y: f32,
    summary: bool,
) {
    let elapsed = page.elapsed.max(0.0);
    let departure = page.departure_elapsed;
    let p2 = player.p2;
    let direction = if p2 { 1.0 } else { -1.0 };
    let base_x = if p2 { 546.0 } else { 94.0 };
    let grade_x = base_x + direction * 320.0 * (1.0 - arrive(elapsed, 0.0) + leave(departure, 0.8));
    let grade_frame = if p2 {
        "stepmania-395/evaluation-grade-p2"
    } else {
        "stepmania-395/evaluation-grade-p1"
    };
    actors.push(
        layout
            .sprite(grade_frame, [grade_x, 100.0], [176.0, 90.0], 1)
            .build(0),
    );
    push_grade(
        actors,
        layout,
        [grade_x, 100.0],
        grade_frame_index(player),
        if departure.is_some() { 5.0 } else { elapsed },
    );

    if !summary {
        let icon_x = if p2 { 426.0 } else { 214.0 };
        let mut icon = layout.sprite(
            "stepmania-395/music-difficulty",
            [icon_x, banner_y - 20.0],
            [32.0, 32.0],
            3,
        );
        let difficulty = difficulty_index(player.difficulty);
        icon.customtexturerect([
            0.0,
            difficulty as f32 / 6.0,
            1.0,
            (difficulty + 1) as f32 / 6.0,
        ]);
        actors.push(icon.build(0));
        let mut mods = layout.text(
            NORMAL_FONT,
            player.mods.into(),
            [if p2 { 440.0 } else { 200.0 }, banner_y + 40.0],
            0.5,
        );
        mods.align(if p2 { 1.0 } else { 0.0 }, 1.0);
        mods.horizalign(if p2 {
            TextAlign::Right
        } else {
            TextAlign::Left
        });
        mods.maxwidth(240.0 * layout.scale);
        mods.z(4);
        actors.push(mods.build(0));
    }

    for index in 0..7 {
        let value = if index < 6 {
            player.judgments[index]
        } else {
            player.holds[0].saturating_add(player.rolls[0])
        };
        let delay = (index + 1) as f32 * 0.1;
        let x = if p2 { 410.0 } else { 230.0 };
        let x = x + direction
            * 320.0
            * (1.0 - arrive(elapsed, delay) + leave(departure, 0.7 - index as f32 * 0.1));
        if let Some(value) = TextContent::inline_format(format_args!("{value:04}")) {
            let mut counter = layout.text(JUDGE_FONT, value, [x, 160.0 + index as f32 * 32.0], 1.0);
            counter.z(4);
            actors.push(counter.build(0));
        }
    }
    let alpha = (elapsed / 0.7).clamp(0.0, 1.0)
        * (1.0 - departure.unwrap_or_default() / 0.7).clamp(0.0, 1.0);
    let bonus_key = match (page.course_summary, p2) {
        (false, false) => "stepmania-395/evaluation-bonus-p1",
        (false, true) => "stepmania-395/evaluation-bonus-p2",
        (true, false) => "stepmania-395/evaluation-survived-p1",
        (true, true) => "stepmania-395/evaluation-survived-p2",
    };
    let mut bonus = layout.sprite(
        bonus_key,
        [if p2 { 544.0 } else { 96.0 }, 256.0],
        [170.0, 190.0],
        1,
    );
    bonus.alpha(alpha);
    actors.push(bonus.build(0));
    if page.course_summary {
        if let Some(value) = TextContent::inline_format(format_args!("{:02}", player.stages_passed))
        {
            let mut survived = layout.text(
                STAGE_FONT,
                value,
                [if p2 { 544.0 } else { 96.0 }, 240.0],
                1.0,
            );
            survived.alpha(alpha);
            survived.z(4);
            actors.push(survived.build(0));
        }
    } else {
        push_bonus_bars(actors, layout, player, page);
    }
    if !page.course_summary
        && let Some(value) = TextContent::inline_format(format_args!("{:04}", player.max_combo))
    {
        let mut combo = layout.text(
            JUDGE_FONT,
            value,
            [if p2 { 580.0 } else { 134.0 }, 334.0],
            1.2,
        );
        combo.diffuse([1.0, 1.0, 0.0, alpha]);
        combo.z(4);
        actors.push(combo.build(0));
    }
    let score_x = if p2 { 480.0 } else { 160.0 };
    let score_x =
        score_x + direction * 320.0 * (1.0 - arrive(elapsed, 0.8) + leave(departure, 0.0));
    if let Some(value) = TextContent::inline_format(format_args!("{:09}", player.numeric_score)) {
        let mut score = layout.text(SCORE_FONT, value, [score_x, 380.0], 1.0);
        score.z(4);
        actors.push(score.build(0));
    }
    if !player.disqualified {
        push_record(actors, layout, player.machine_rank, p2, false, alpha);
        push_record(actors, layout, player.personal_rank, p2, true, alpha);
    }
}

fn push_bonus_bars(
    actors: &mut Vec<Actor>,
    layout: Layout,
    player: &EvaluationPlayerOverview<'_>,
    page: &EvaluationOverview<'_>,
) {
    let p2 = player.p2;
    let alpha = (1.0 - page.departure_elapsed.unwrap_or_default() / 0.7).clamp(0.0, 1.0);
    for (axis, possible) in player.radar_possible.into_iter().enumerate() {
        let possible = possible.clamp(0.0, 1.0);
        let at = [if p2 { 466.0 } else { 176.0 }, 194.0 + axis as f32 * 27.0];
        let direction = if p2 { 1.0 } else { -1.0 };
        let delay = [0.2, 0.3, 0.4, 1.0, 0.6][axis];
        let width = 160.0 * possible;
        let progress = (page.elapsed - delay).clamp(0.0, 1.0);
        let mut bar = layout.sprite(
            if p2 {
                "stepmania-395/evaluation-bar-possible-p2"
            } else {
                "stepmania-395/evaluation-bar-possible-p1"
            },
            [at[0] + direction * width * progress * 0.5, at[1]],
            [width, 16.0],
            2,
        );
        bar.rotationz(if p2 { 0.0 } else { 180.0 });
        bar.zoomx(progress);
        bar.visible(width > 0.0 && progress > 0.0);
        bar.alpha(alpha);
        actors.push(bar.build(0));
        let Some(achievement) = player.radar_achievement else {
            continue;
        };
        let achieved = achievement[axis].clamp(0.0, 1.0);
        let width = width * achieved;
        let progress = (page.elapsed - 0.7 - axis as f32 * 0.1).clamp(0.0, 1.0);
        let mut actual = layout.sprite(
            if p2 {
                "stepmania-395/evaluation-bar-actual-p2"
            } else {
                "stepmania-395/evaluation-bar-actual-p1"
            },
            [at[0] + direction * width * progress * 0.5, at[1]],
            [width, 16.0],
            3,
        );
        actual.rotationz(if p2 { 0.0 } else { 180.0 });
        actual.zoomx(progress);
        actual.visible(width > 0.0 && progress > 0.0);
        actual.alpha(alpha);
        if achieved > 0.99999 {
            actual.effectmode(deadlib_present::anim::EffectMode::GlowShift);
            actual.effectperiod(0.5);
        }
        actors.push(actual.build(0));
    }
}

fn grade_frame_index(player: &EvaluationPlayerOverview<'_>) -> usize {
    if player.grade == Grade::Failed {
        return 7;
    }
    if player.percent >= 100.0 {
        return usize::from(player.judgments[1..].iter().any(|&count| count != 0));
    }
    for (index, threshold) in [93.0, 80.0, 65.0, 45.0].into_iter().enumerate() {
        if player.percent >= threshold {
            return index + 2;
        }
    }
    6
}

fn push_grade(
    actors: &mut Vec<Actor>,
    layout: Layout,
    [x, y]: [f32; 2],
    frame: usize,
    elapsed: f32,
) {
    let t = (elapsed / 5.0).clamp(0.0, 1.0);
    let progress = if t < 0.75 {
        t / 0.75 * 1.125
    } else if t < 0.9 {
        1.125
    } else {
        1.125 - (t - 0.9) / 0.1 * 0.125
    };
    let top = (frame as f32 / 8.0 + (1.0 - progress) * 4.0).rem_euclid(1.0);
    let first = (1.0 - top).min(0.125);
    for (v, amount, start) in [(top, first, 0.0), (0.0, 0.125 - first, first)] {
        if amount <= 1e-6 {
            continue;
        }
        let height = amount * 8.0 * 74.0;
        let mut grade = layout.sprite(
            "stepmania-395/evaluation-grades",
            [x, y - 37.0 + start * 8.0 * 74.0 + height * 0.5],
            [128.0, height],
            3,
        );
        grade.customtexturerect([0.0, v, 1.0, v + amount]);
        actors.push(grade.build(0));
    }
}

fn push_record(
    actors: &mut Vec<Actor>,
    layout: Layout,
    rank: Option<u32>,
    p2: bool,
    personal: bool,
    alpha: f32,
) {
    let Some(rank) = rank.filter(|rank| (1..=10).contains(rank)) else {
        return;
    };
    const MACHINE: [&str; 10] = [
        "stepmania-395/evaluation-machinerecord-1",
        "stepmania-395/evaluation-machinerecord-2",
        "stepmania-395/evaluation-machinerecord-3",
        "stepmania-395/evaluation-machinerecord-4",
        "stepmania-395/evaluation-machinerecord-5",
        "stepmania-395/evaluation-machinerecord-6",
        "stepmania-395/evaluation-machinerecord-7",
        "stepmania-395/evaluation-machinerecord-8",
        "stepmania-395/evaluation-machinerecord-9",
        "stepmania-395/evaluation-machinerecord-10",
    ];
    const PERSONAL: [&str; 10] = [
        "stepmania-395/evaluation-personalrecord-1",
        "stepmania-395/evaluation-personalrecord-2",
        "stepmania-395/evaluation-personalrecord-3",
        "stepmania-395/evaluation-personalrecord-4",
        "stepmania-395/evaluation-personalrecord-5",
        "stepmania-395/evaluation-personalrecord-6",
        "stepmania-395/evaluation-personalrecord-7",
        "stepmania-395/evaluation-personalrecord-8",
        "stepmania-395/evaluation-personalrecord-9",
        "stepmania-395/evaluation-personalrecord-10",
    ];
    let key = if personal { PERSONAL } else { MACHINE }[(rank - 1) as usize];
    let x = match (p2, personal) {
        (false, false) => 70.0,
        (false, true) => 200.0,
        (true, false) => 440.0,
        (true, true) => 570.0,
    };
    let mut image = layout.sprite(key, [x, 398.0], [148.0, 32.0], 4);
    image.alpha(alpha);
    actors.push(image.build(0));
}

pub(crate) fn push_departure(actors: &mut Vec<Actor>, elapsed: f32) {
    let t = ((elapsed - 0.8) / 0.5).clamp(0.0, 1.0);
    let mut sign = Layout::current().sprite(
        "stepmania-395/evaluation-move-on",
        [320.0, 240.0],
        [368.0, 176.0],
        1100,
    );
    sign.zoomy(t);
    sign.alpha(t);
    actors.push(sign.build(0));
}

pub(crate) fn push_session_summary(
    actors: &mut Vec<Actor>,
    assets: &dyn AssetView,
    summary: deadsync_theme::post_game::SessionSummaryPage<'_>,
) {
    use std::sync::Arc;
    let empty: Arc<str> = "".into();
    let life = Default::default();
    let players = std::array::from_fn(|side| {
        summary.results.players[side]
            .as_ref()
            .map(|result| EvaluationPlayerOverview {
                p2: side == 1,
                grade: result.grade,
                percent: result.percent * 100.0,
                numeric_score: result.numeric_score,
                radar_possible: result.radar_possible,
                radar_achievement: result.radar_achievement,
                stages_passed: 0,
                individual_percent: None,
                ex_percent: None,
                hard_ex_percent: None,
                difficulty: "",
                meter: 0,
                mods: &empty,
                judgments: result.judgments,
                row_window_counts: Default::default(),
                score_valid: true,
                dropped_hold_or_roll: false,
                jumps: [0; 2],
                holds: [result.holds, 0],
                mines: [0; 2],
                hands: [0; 2],
                rolls: [result.rolls, 0],
                max_combo: result.combo_total,
                combo_runs: &[],
                graph_seconds: [0.0; 2],
                course_graph_stages: &[],
                steps_life: &life,
                machine_rank: None,
                personal_rank: None,
                disqualified: false,
            })
    });
    push_page(
        actors,
        EvaluationOverview {
            title: &empty,
            artist: &empty,
            banner_key: None,
            course_summary: false,
            stage_number: None,
            session_style: summary.session_style,
            elapsed: summary.elapsed,
            departure_elapsed: summary.departure_elapsed,
            players,
        },
        Some((assets, summary.banners)),
    );
}
