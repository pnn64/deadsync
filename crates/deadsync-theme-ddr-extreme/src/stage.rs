use crate::evaluation::{self, ExtremeEvaluation};
use crate::scene::{AMBER, MINT, MUTED, PAPER, Scene};
use deadlib_present::actors::{Actor, InlineText, TextContent};
use deadsync_theme::UiCue;
use deadsync_theme::attract::{
    AttractDemoChart, AttractDemoRequest, AttractMedia, AttractPresentation,
};
use deadsync_theme::stage::{
    EvaluationAudio, EvaluationDeparture, EvaluationFallbackSounds, EvaluationOverviewPresentation,
    StageLoadingView, StageOutroPresentation, StageOutroView, StagePresentation,
};
use deadsync_theme::views::{AttractRecords, GameplayHud};
use std::fmt::Write;

#[derive(Default)]
pub struct ExtremeStage {
    evaluation: ExtremeEvaluation,
}

impl StagePresentation<Actor> for ExtremeStage {
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

    fn gameplay_hud(&self) -> Option<GameplayHud<Actor>> {
        Some(crate::gameplay::HUD)
    }

    fn gameplay_entry(&self, restarting: bool, prepared: bool) -> Option<Vec<Actor>> {
        (!restarting && prepared).then(Vec::new)
    }

    fn gameplay_outro(
        &self,
        view: StageOutroView<'_>,
    ) -> Option<Box<dyn StageOutroPresentation<Actor>>> {
        (!view.authored_transition).then(|| {
            Box::new(crate::gameplay::Outro {
                failed: view.failed,
            }) as Box<_>
        })
    }

    fn evaluation_overview(&mut self) -> Option<&mut dyn EvaluationOverviewPresentation<Actor>> {
        Some(&mut self.evaluation)
    }

    fn evaluation_departure(&self) -> Option<EvaluationDeparture<Actor>> {
        Some(EvaluationDeparture {
            input_ready_seconds: evaluation::SETTLED_SECONDS,
            duration: evaluation::DEPARTURE_SECONDS,
            cue: Some(UiCue::Confirm),
            push_overlay: evaluation::push_departure,
        })
    }

    fn evaluation_audio(&self) -> EvaluationAudio {
        EvaluationAudio {
            music_loop: Some("assets/media/ddr-extreme/evaluation.ogg"),
            fallback_sounds: EvaluationFallbackSounds::None,
        }
    }

    fn evaluation_intro_settled(&self, elapsed: f32, overview: bool) -> bool {
        !overview || elapsed >= evaluation::SETTLED_SECONDS
    }

    fn leave_evaluation(&mut self) {
        self.evaluation = ExtremeEvaluation::default();
    }
}

pub(crate) fn number_text(args: std::fmt::Arguments<'_>) -> TextContent {
    let mut text = InlineText::default();
    if text.write_fmt(args).is_ok() {
        TextContent::Inline(text)
    } else {
        TextContent::from(args.to_string())
    }
}

#[derive(Default)]
pub struct ExtremeAttract {
    elapsed: f32,
    idle: bool,
    records: AttractRecords,
}

impl AttractPresentation<Actor> for ExtremeAttract {
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
            music: Some("assets/media/ddr-extreme/attract.ogg"),
            loop_music: true,
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
            scene.text(actors, [109.0, 263.0], [2.7, 423.0], "DDR EXTREME", PAPER);
        }
        scene.text(actors, [62.0, 430.0], [0.58, 515.0], "PRESS START", MUTED);
    }

    fn push_demo_underlay(&self, _: &mut Vec<Actor>, _: f32) {}
    fn push_demo_overlay(&self, _: &mut Vec<Actor>, _: f32) {}
}
