use deadlib_assets::AssetManager;
use deadlib_present::actors::Actor;
use deadsync_theme::stage::{
    EvaluationDeparture, EvaluationOverviewPresentation, StageLoadingView, StageMotionView,
    StageOutroPresentation, StageOutroView, StagePresentation,
};
use deadsync_theme::views::{EvaluationOverview, GameplayHud, GameplayHudMotion};
use deadsync_theme::{Screen, ThemeTransitions, UiCue};

use crate::{
    EVALUATION_OVERVIEW_SETTLED_SECONDS, GAMEPLAY_HUD, GameplayOutro, OpenItgEvaluation,
    OpenItgTheme, push_stage_loading_actors,
};

#[derive(Default)]
pub struct OpenItgStagePresentation {
    evaluation: OpenItgEvaluation,
}

impl StagePresentation<Actor> for OpenItgStagePresentation {
    fn push_loading(&self, actors: &mut Vec<Actor>, view: StageLoadingView<'_>) {
        push_stage_loading_actors(
            actors,
            view.title,
            view.stage_number,
            view.course,
            view.elapsed,
            view.progress,
        );
    }

    fn gameplay_hud(&self) -> Option<GameplayHud<Actor>> {
        Some(GAMEPLAY_HUD)
    }

    fn gameplay_motion(&self, view: StageMotionView) -> GameplayHudMotion {
        if let Some(elapsed_seconds) = view.chart_outro.or(view.give_up_outro) {
            return GameplayHudMotion::Leaving { elapsed_seconds };
        }
        match view.transition {
            GameplayHudMotion::Entering { .. } if view.restarting || view.hide_intro => {
                GameplayHudMotion::Settled
            }
            motion => motion,
        }
    }

    fn gameplay_entry(&self, restarting: bool, prepared: bool) -> Option<(Vec<Actor>, f32)> {
        (!restarting && prepared).then(|| OpenItgTheme::in_transition(Screen::Gameplay, ()))
    }

    fn gameplay_outro(
        &self,
        view: StageOutroView<'_>,
    ) -> Option<Box<dyn StageOutroPresentation<Actor>>> {
        (!view.authored_transition)
            .then(|| Box::new(GameplayOutro::from_result(view.failed, view.simfile)) as Box<_>)
    }

    fn evaluation_overview(&mut self) -> Option<&mut dyn EvaluationOverviewPresentation<Actor>> {
        Some(&mut self.evaluation)
    }

    fn evaluation_departure(&self) -> Option<EvaluationDeparture<Actor>> {
        Some(EvaluationDeparture {
            input_ready_seconds: crate::evaluation::INPUT_READY_SECONDS,
            duration: crate::evaluation::DEPARTURE_SECONDS,
            cue: Some(UiCue::Confirm),
            push_overlay: crate::evaluation::push_departure_overlay,
        })
    }

    fn evaluation_music_loop(&self) -> Option<&'static str> {
        Some("assets/media/openitg/evaluation.ogg")
    }

    fn evaluation_intro_settled(&self, elapsed: f32, overview: bool) -> bool {
        !overview || elapsed >= EVALUATION_OVERVIEW_SETTLED_SECONDS
    }

    fn evaluation_entry_sfx_enabled(&self) -> bool {
        false
    }

    fn leave_evaluation(&mut self) {
        self.evaluation.reset();
    }
}

impl EvaluationOverviewPresentation<Actor> for OpenItgEvaluation {
    fn push_overview(
        &mut self,
        actors: &mut Vec<Actor>,
        assets: &AssetManager,
        page: EvaluationOverview<'_>,
    ) {
        OpenItgEvaluation::push_overview(self, actors, assets, page);
    }
}

impl StageOutroPresentation<Actor> for GameplayOutro {
    fn duration(&self) -> f32 {
        (*self).duration()
    }

    fn cue(&self) -> Option<UiCue> {
        (*self).cue()
    }

    fn covers_stage(&self, elapsed: f32) -> bool {
        (*self).covers_stage(elapsed)
    }

    fn stop_tail_audio_when_frozen(&self) -> bool {
        *self != Self::Cleared
    }

    fn push_actors(&self, actors: &mut Vec<Actor>, elapsed: f32) {
        (*self).push_actors(actors, elapsed);
    }
}
