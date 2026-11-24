use deadlib_present::actors::Actor;
use deadsync_theme::stage::{
    EvaluationAudio, EvaluationDeparture, EvaluationFallbackSounds, EvaluationOverviewPresentation,
    StageCardMode, StageCardPresentation, StageCardView, StageLoadingView, StageOutroPresentation,
    StageOutroView, StagePresentation,
};
use deadsync_theme::views::GameplayHud;

use crate::{StepManiaStageCard, gameplay, gameplay_transition::StageOutro};

#[derive(Default)]
pub struct StepManiaStagePresentation {
    evaluation: crate::evaluation::StepManiaEvaluation,
}

impl StagePresentation<Actor> for StepManiaStagePresentation {
    fn push_loading(&self, actors: &mut Vec<Actor>, view: StageLoadingView<'_>) {
        StepManiaStageCard.push_actors(
            actors,
            StageCardView {
                stage_number: view.stage_number,
                mode: if view.course {
                    StageCardMode::Nonstop
                } else {
                    StageCardMode::Regular
                },
                elapsed: view.elapsed,
                exit_elapsed: None,
            },
        );
    }
    fn gameplay_hud(&self) -> Option<GameplayHud<Actor>> {
        Some(gameplay::GAMEPLAY_HUD)
    }
    fn gameplay_entry(&self, _restarting: bool, prepared: bool) -> Option<Vec<Actor>> {
        Some(if prepared {
            Vec::new()
        } else {
            vec![crate::presentation::Layout::current().solid(
                [320.0, 240.0],
                [640.0, 480.0],
                [0.0, 0.0, 0.0, 1.0],
                1200,
            )]
        })
    }
    fn gameplay_entry_seconds(&self, restarting: bool) -> Option<f32> {
        (!restarting).then_some(5.0)
    }
    fn gameplay_outro(
        &self,
        view: StageOutroView<'_>,
    ) -> Option<Box<dyn StageOutroPresentation<Actor>>> {
        (!view.authored_transition).then(|| {
            Box::new(StageOutro {
                failed: view.failed,
            }) as Box<_>
        })
    }
    fn evaluation_overview(&mut self) -> Option<&mut dyn EvaluationOverviewPresentation<Actor>> {
        Some(&mut self.evaluation)
    }
    fn evaluation_departure(&self) -> Option<EvaluationDeparture<Actor>> {
        Some(EvaluationDeparture {
            input_ready_seconds: 1.3,
            duration: 1.3,
            cue: None,
            push_overlay: crate::evaluation::push_departure,
        })
    }
    fn evaluation_audio(&self) -> EvaluationAudio {
        EvaluationAudio {
            music_loop: None,
            fallback_sounds: EvaluationFallbackSounds::None,
        }
    }
    fn evaluation_intro_settled(&self, elapsed: f32, overview: bool) -> bool {
        !overview || elapsed >= 5.0
    }
}
