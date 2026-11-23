use std::sync::Arc;

use deadlib_assets::AssetManager;
use deadlib_present::actors::{Actor, TextContent};
use deadsync_theme::stage::EvaluationOverviewPresentation;
use deadsync_theme::views::{EvaluationOverview, EvaluationPlayerOverview, StageLifeTimeline};

use crate::scene::{AMBER, MINT, MUTED, PAPER, PITCH, Scene};
use crate::stage::number_text;

pub(crate) const SETTLED_SECONDS: f32 = 0.62;
pub(crate) const DEPARTURE_SECONDS: f32 = 0.32;

const CELLS: usize = 48;
const PANEL: [f32; 3] = [0.062, 0.125, 0.174];
const ALERT: [f32; 3] = [1.0, 0.44, 0.48];

#[derive(Default)]
pub(crate) struct ExtremeEvaluation {
    graphs: [Option<LifePlot>; 2],
}

impl EvaluationOverviewPresentation<Actor> for ExtremeEvaluation {
    fn push_overview(
        &mut self,
        actors: &mut Vec<Actor>,
        _assets: &AssetManager,
        page: EvaluationOverview<'_>,
    ) {
        let scene = Scene::new(1.0);
        scene.cover(actors);

        let heading = if page.course_summary {
            "COURSE RESULTS"
        } else {
            "STAGE RESULTS"
        };
        scene.text(actors, [28.0, 42.0], [0.65, 315.0], heading, MINT);
        if let Some(round) = page.stage_number {
            right_text(
                scene,
                actors,
                [609.0, 42.0],
                [0.53, 135.0],
                number_text(format_args!("ROUND {round:02}")),
                MUTED,
            );
        }
        let title = if page.course_summary && page.title.is_empty() {
            TextContent::from("UNTITLED COURSE")
        } else {
            TextContent::from(Arc::clone(page.title))
        };
        let title_width = if page.banner_key.is_some() {
            392.0
        } else {
            575.0
        };
        scene.text(actors, [28.0, 76.0], [1.38, title_width], title, PAPER);
        if !page.artist.is_empty() {
            scene.text(
                actors,
                [29.0, 105.0],
                [0.55, title_width],
                page.artist,
                MUTED,
            );
        }
        if let Some(key) = page.banner_key {
            scene.banner(actors, [530.0, 88.0], [161.0, 50.0], key);
            scene.rect(actors, [449.5, 113.0], [161.0, 1.0], MINT, 0.65, 1);
        }
        scene.rect(actors, [28.0, 120.0], [584.0, 1.0], MUTED, 0.31, 0);

        let dual = page.players.iter().flatten().count() > 1;
        for (column, player) in page.players.iter().flatten().enumerate() {
            let delay = 0.13 + column as f32 * 0.08;
            let reveal = ((page.elapsed - delay) / 0.32).clamp(0.0, 1.0);
            let alpha = 1.0 - (1.0 - reveal).powi(2);
            let x = if dual {
                28.0 + column as f32 * 298.0
            } else {
                177.0
            };
            let y = 132.0 + (1.0 - alpha) * 12.0;
            let slot = &mut self.graphs[usize::from(player.p2)];
            if slot
                .as_ref()
                .is_none_or(|plot| !plot.matches(player.steps_life))
            {
                *slot = Some(LifePlot::new(player.steps_life));
            }
            push_player(
                scene.with_alpha(alpha),
                actors,
                [x, y],
                player,
                slot.as_ref().unwrap(),
            );
        }

        if page.elapsed >= SETTLED_SECONDS {
            scene.text(
                actors,
                [28.0, 437.0],
                [0.55, 400.0],
                "START TO CONTINUE",
                MUTED,
            );
        }
    }
}

fn push_player(
    scene: Scene,
    actors: &mut Vec<Actor>,
    [x, y]: [f32; 2],
    player: &EvaluationPlayerOverview<'_>,
    life: &LifePlot,
) {
    let seat = if player.p2 { AMBER } else { MINT };
    let failed = player.grade.gif_suffix() == "F";
    let grade_ink = if failed { ALERT } else { seat };
    scene.rect(actors, [x, y], [286.0, 287.0], PANEL, 1.0, 0);
    scene.rect(actors, [x, y], [286.0, 2.0], seat, 0.94, 1);
    scene.text(
        actors,
        [x + 12.0, y + 20.0],
        [0.65, 106.0],
        if player.p2 { "PLAYER 2" } else { "PLAYER 1" },
        seat,
    );
    right_text(
        scene,
        actors,
        [x + 274.0, y + 20.0],
        [0.52, 151.0],
        number_text(format_args!("{}  {}", player.difficulty, player.meter)),
        PAPER,
    );

    scene.text(
        actors,
        [x + 12.0, y + 53.0],
        [1.49, 167.0],
        number_text(format_args!("{:.2}%", player.percent)),
        PAPER,
    );
    scene.rect(
        actors,
        [x + 192.0, y + 36.0],
        [82.0, 39.0],
        grade_ink,
        0.11,
        1,
    );
    scene.rect(
        actors,
        [x + 192.0, y + 36.0],
        [2.0, 39.0],
        grade_ink,
        0.98,
        2,
    );
    let grade = player.grade.to_sprite_state();
    let grade = if grade <= 4 {
        number_text(format_args!("STAR x{}", 5 - grade))
    } else {
        TextContent::from(player.grade.gif_suffix())
    };
    let mut grade_badge = scene.text_builder([x + 233.0, y + 54.0], 1.25, grade, grade_ink);
    grade_badge.align(0.5, 0.5);
    grade_badge.maxwidth(69.0);
    actors.push(grade_badge.build(0));

    if let Some(ex) = player.ex_percent {
        scene.text(
            actors,
            [x + 13.0, y + 78.0],
            [0.42, 78.0],
            number_text(format_args!("EX {ex:.2}%")),
            seat,
        );
    }
    if let Some(hard_ex) = player.hard_ex_percent {
        scene.text(
            actors,
            [x + 105.0, y + 78.0],
            [0.42, 83.0],
            number_text(format_args!("HEX {hard_ex:.2}%")),
            seat,
        );
    }
    let status = if player.disqualified {
        Some((TextContent::from("DISQUALIFIED"), ALERT))
    } else if !player.score_valid {
        Some((TextContent::from("UNRANKED"), MUTED))
    } else {
        player
            .machine_rank
            .map(|rank| (number_text(format_args!("MACHINE {rank}")), seat))
    };
    if let Some((status, ink)) = status {
        right_text(
            scene,
            actors,
            [x + 274.0, y + 85.0],
            [0.43, 81.0],
            status,
            ink,
        );
    }

    scene.rect(actors, [x + 12.0, y + 98.0], [262.0, 1.0], MUTED, 0.35, 1);
    for (row, (label, count)) in [
        "FANTASTIC",
        "EXCELLENT",
        "GREAT",
        "DECENT",
        "WAY OFF",
        "MISS",
    ]
    .into_iter()
    .zip(player.judgments)
    .enumerate()
    {
        let line = y + 112.0 + row as f32 * 16.0;
        if row % 2 == 0 {
            scene.rect(
                actors,
                [x + 10.0, line - 8.0],
                [129.0, 15.0],
                PITCH,
                0.52,
                1,
            );
        }
        scene.text(actors, [x + 15.0, line], [0.48, 75.0], label, MUTED);
        right_text(
            scene,
            actors,
            [x + 134.0, line],
            [0.54, 39.0],
            number_text(format_args!("{count}")),
            if row == 5 && count != 0 { ALERT } else { PAPER },
        );
    }
    for (row, (label, [matched, total])) in [
        ("HOLDS", player.holds),
        ("ROLLS", player.rolls),
        ("MINES", player.mines),
    ]
    .into_iter()
    .enumerate()
    {
        let line = y + 112.0 + row as f32 * 16.0;
        scene.text(actors, [x + 150.0, line], [0.49, 63.0], label, MUTED);
        right_text(
            scene,
            actors,
            [x + 274.0, line],
            [0.52, 60.0],
            number_text(format_args!("{matched}/{total}")),
            PAPER,
        );
    }
    scene.text(
        actors,
        [x + 150.0, y + 166.0],
        [0.49, 75.0],
        "MAX COMBO",
        MUTED,
    );
    right_text(
        scene,
        actors,
        [x + 274.0, y + 166.0],
        [0.62, 47.0],
        number_text(format_args!("{}", player.max_combo)),
        seat,
    );
    if let Some(percent) = player.individual_percent {
        right_text(
            scene,
            actors,
            [x + 274.0, y + 191.0],
            [0.46, 123.0],
            number_text(format_args!("SOLO {percent:.2}%")),
            MUTED,
        );
    }

    scene.rect(actors, [x + 12.0, y + 203.0], [262.0, 1.0], MUTED, 0.35, 1);
    scene.text(actors, [x + 13.0, y + 217.0], [0.45, 76.0], "LIFE", MUTED);
    life.push(scene, actors, [x + 12.0, y + 229.0], seat);
    if !player.mods.is_empty() {
        right_text(
            scene,
            actors,
            [x + 273.0, y + 217.0],
            [0.40, 198.0],
            player.mods,
            MUTED,
        );
    }
}

fn right_text(
    scene: Scene,
    actors: &mut Vec<Actor>,
    at: [f32; 2],
    [zoom, width]: [f32; 2],
    text: impl Into<TextContent>,
    ink: [f32; 3],
) {
    let mut label = scene.text_builder(at, zoom, text.into(), ink);
    label.align(1.0, 0.5);
    label.maxwidth(width);
    actors.push(label.build(0));
}

struct LifePlot {
    timeline: StageLifeTimeline,
    levels: [f32; CELLS],
    recorded: usize,
}

impl LifePlot {
    fn new(timeline: &StageLifeTimeline) -> Self {
        let duration = timeline.possible_seconds;
        let recorded = if duration.is_finite() && duration > 0.0 && !timeline.samples.is_empty() {
            ((timeline.elapsed_seconds / duration).clamp(0.0, 1.0) * CELLS as f32).ceil() as usize
        } else {
            0
        };
        let mut levels = [0.0; CELLS];
        for (index, level) in levels[..recorded].iter_mut().enumerate() {
            let second =
                (duration * (index as f32 + 0.5) / CELLS as f32).min(timeline.elapsed_seconds);
            *level = timeline.sample(second).clamp(0.0, 1.0);
        }
        Self {
            timeline: timeline.clone(),
            levels,
            recorded,
        }
    }

    fn matches(&self, timeline: &StageLifeTimeline) -> bool {
        Arc::ptr_eq(&self.timeline.samples, &timeline.samples)
            && self.timeline.possible_seconds.to_bits() == timeline.possible_seconds.to_bits()
            && self.timeline.elapsed_seconds.to_bits() == timeline.elapsed_seconds.to_bits()
    }

    fn push(&self, scene: Scene, actors: &mut Vec<Actor>, [x, y]: [f32; 2], ink: [f32; 3]) {
        const WIDTH: f32 = 262.0;
        const HEIGHT: f32 = 43.0;
        const STEP: f32 = WIDTH / CELLS as f32;
        scene.rect(actors, [x, y], [WIDTH, HEIGHT], PITCH, 0.94, 1);
        scene.rect(actors, [x, y + HEIGHT * 0.5], [WIDTH, 1.0], MUTED, 0.19, 2);
        for (index, &level) in self.levels[..self.recorded].iter().enumerate() {
            let height = (level * HEIGHT).max(1.0);
            let x = x + STEP * index as f32;
            scene.rect(
                actors,
                [x, y + HEIGHT - height],
                [STEP - 0.75, height],
                ink,
                0.49,
                3,
            );
            scene.rect(
                actors,
                [x, y + HEIGHT - height],
                [STEP - 0.75, 1.0],
                ink,
                0.92,
                4,
            );
        }
    }
}

pub(crate) fn push_departure(actors: &mut Vec<Actor>, elapsed: f32) {
    let progress = (elapsed / DEPARTURE_SECONDS).clamp(0.0, 1.0);
    Scene::new(progress * progress).fill_screen(actors, PITCH, 1100);
}
