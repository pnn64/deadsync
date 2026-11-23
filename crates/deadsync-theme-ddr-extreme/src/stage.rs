use crate::scene::{AMBER, MINT, MUTED, PAPER, Scene};
use deadlib_assets::AssetManager;
use deadlib_present::actors::{Actor, InlineText, TextContent};
use deadsync_theme::UiCue;
use deadsync_theme::attract::{
    AttractDemoChart, AttractDemoRequest, AttractMedia, AttractPresentation,
};
use deadsync_theme::stage::{
    EvaluationDeparture, EvaluationOverviewPresentation, StageLoadingView, StagePresentation,
};
use deadsync_theme::views::{AttractRecords, EvaluationOverview};
use std::fmt::Write;

#[derive(Default)]
pub struct CircuitStage;

impl StagePresentation<Actor> for CircuitStage {
    fn push_loading(&self, actors: &mut Vec<Actor>, view: StageLoadingView<'_>) {
        let scene = Scene::new(1.0);
        scene.cover(actors);
        scene.motif(actors, [70.0, 166.0], view.elapsed);
        scene.text(actors, [70.0, 222.0], [0.65, 475.0], "STAGE LOADING", MINT);
        scene.text(actors, [70.0, 267.0], [1.7, 495.0], view.title, PAPER);
        scene.rect(actors, [70.0, 341.0], [500.0, 12.0], MUTED, 0.2, 0);
        let filled = 500.0 * view.progress.clamp(0.0, 1.0);
        if filled > 0.0 {
            scene.rect(actors, [70.0, 341.0], [filled, 12.0], AMBER, 1.0, 1);
        }
    }

    fn evaluation_overview(&mut self) -> Option<&mut dyn EvaluationOverviewPresentation<Actor>> {
        Some(self)
    }

    fn evaluation_departure(&self) -> Option<EvaluationDeparture<Actor>> {
        Some(EvaluationDeparture {
            input_ready_seconds: 0.3,
            duration: 0.22,
            cue: Some(UiCue::Confirm),
            push_overlay: |_, _| {},
        })
    }
}

impl EvaluationOverviewPresentation<Actor> for CircuitStage {
    fn push_overview(
        &mut self,
        actors: &mut Vec<Actor>,
        _assets: &AssetManager,
        page: EvaluationOverview<'_>,
    ) {
        let alpha = page
            .departure_elapsed
            .map_or(1.0, |elapsed| 1.0 - elapsed / 0.22);
        let scene = Scene::new(alpha);
        scene.cover(actors);
        scene.text(actors, [48.0, 57.0], [0.78, 520.0], "STAGE RESULTS", MINT);
        if page.course_summary && page.title.is_empty() {
            scene.text(
                actors,
                [48.0, 107.0],
                [1.65, 544.0],
                "UNTITLED COURSE",
                PAPER,
            );
        } else {
            scene.text(actors, [48.0, 107.0], [1.65, 544.0], page.title, PAPER);
        }

        let count = page.players.iter().flatten().count();
        for (column, player) in page.players.iter().flatten().enumerate() {
            let x = if count > 1 {
                48.0 + column as f32 * 279.0
            } else {
                186.0
            };
            let accent = if player.p2 { AMBER } else { MINT };
            scene.rect(actors, [x, 151.0], [265.0, 252.0], accent, 0.09, 0);
            scene.rect(actors, [x, 151.0], [265.0, 3.0], accent, 1.0, 1);
            scene.text(
                actors,
                [x + 15.0, 175.0],
                [0.65, 220.0],
                if player.p2 { "PLAYER 2" } else { "PLAYER 1" },
                accent,
            );
            scene.text(
                actors,
                [x + 15.0, 225.0],
                [2.1, 235.0],
                player.grade.gif_suffix(),
                PAPER,
            );
            scene.text(
                actors,
                [x + 15.0, 283.0],
                [1.35, 235.0],
                number_text(format_args!("{:.2}%", player.percent)),
                accent,
            );
            scene.text(
                actors,
                [x + 15.0, 326.0],
                [0.66, 235.0],
                number_text(format_args!("COMBO  {}", player.max_combo)),
                PAPER,
            );
            scene.text(
                actors,
                [x + 15.0, 366.0],
                [0.66, 235.0],
                number_text(format_args!("HOLDS  {}/{}", player.holds[0], player.holds[1])),
                MUTED,
            );
        }
        scene.text(
            actors,
            [48.0, 434.0],
            [0.56, 520.0],
            "START TO CONTINUE",
            MUTED,
        );
    }
}

fn number_text(args: std::fmt::Arguments<'_>) -> TextContent {
    let mut text = InlineText::default();
    if text.write_fmt(args).is_ok() {
        TextContent::Inline(text)
    } else {
        TextContent::from(args.to_string())
    }
}

#[derive(Default)]
pub struct CircuitAttract {
    elapsed: f32,
    idle: bool,
    records: AttractRecords,
}

impl AttractPresentation<Actor> for CircuitAttract {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn record_limits(&self) -> (usize, usize) {
        (5, 0)
    }

    fn start_idle(&mut self, records: AttractRecords) {
        self.elapsed = 0.0;
        self.idle = true;
        self.records = records;
    }

    fn advance(&mut self, delta: f32) -> bool {
        self.elapsed += delta;
        self.elapsed >= if self.idle { 10.0 } else { 2.5 }
    }

    fn media(&self) -> AttractMedia {
        AttractMedia {
            movie: None,
            music: None,
            loop_music: false,
        }
    }

    fn playback_time(&self) -> f32 {
        self.elapsed
    }

    fn demonstration(&self) -> Option<AttractDemoRequest> {
        None
    }

    fn prepare_demonstration(&mut self, _: Option<AttractDemoChart<'_>>) {}

    fn finish_demonstration(&mut self) {}

    fn push_actors(&self, actors: &mut Vec<Actor>, _: Option<&std::sync::Arc<str>>) {
        let scene = Scene::new(1.0);
        scene.cover(actors);
        if self.idle && !self.records.songs.is_empty() {
            scene.text(actors, [62.0, 73.0], [1.3, 505.0], "CABINET RECORDS", MINT);
            for (index, record) in self.records.songs.iter().enumerate() {
                let y = 142.0 + index as f32 * 51.0;
                scene.rect(actors, [62.0, y - 19.0], [516.0, 38.0], MINT, 0.10, 0);
                scene.text(actors, [74.0, y], [0.77, 334.0], &record.title, PAPER);
                scene.text(
                    actors,
                    [428.0, y],
                    [0.70, 135.0],
                    number_text(format_args!("{:.2}%", record.percent)),
                    AMBER,
                );
            }
        } else {
            scene.motif(actors, [235.0, 188.0], self.elapsed);
            scene.text(actors, [109.0, 263.0], [2.7, 423.0], "PULSE CIRCUIT", PAPER);
        }
        scene.text(actors, [62.0, 430.0], [0.58, 515.0], "PRESS START", MUTED);
    }

    fn push_demo_underlay(&self, _: &mut Vec<Actor>, _: f32) {}
    fn push_demo_overlay(&self, _: &mut Vec<Actor>, _: f32) {}
}
