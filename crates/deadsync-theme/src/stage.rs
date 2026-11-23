//! Presentation choices for a stage and its results. The host supplies game facts.

use std::path::Path;
use std::sync::Arc;

use crate::UiCue;
use crate::views::{EvaluationOverview, GameplayHud, GameplayHudMotion};

pub struct StageLoadingView<'a> {
    pub title: &'a Arc<str>,
    pub stage_number: usize,
    pub course: bool,
    pub color_index: i32,
    pub elapsed: f32,
    pub progress: f32,
}

pub struct StageOutroView<'a> {
    pub failed: bool,
    pub simfile: &'a Path,
    pub authored_transition: bool,
}

pub struct StageMotionView {
    pub chart_outro: Option<f32>,
    pub give_up_outro: Option<f32>,
    pub transition: GameplayHudMotion,
    pub restarting: bool,
    pub hide_intro: bool,
}

pub trait StageOutroPresentation<A> {
    fn duration(&self) -> f32;
    fn cue(&self) -> Option<UiCue>;
    fn covers_stage(&self, elapsed: f32) -> bool;
    fn stop_tail_audio_when_frozen(&self) -> bool;
    fn push_actors(&self, actors: &mut Vec<A>, elapsed: f32);
}

pub trait EvaluationOverviewPresentation<A> {
    fn push_overview(&mut self, actors: &mut Vec<A>, page: EvaluationOverview<'_>);
}

pub trait StagePresentation<A> {
    fn push_loading(&self, actors: &mut Vec<A>, view: StageLoadingView<'_>);

    fn gameplay_hud(&self) -> Option<GameplayHud<A>> {
        None
    }

    fn gameplay_motion(&self, _view: StageMotionView) -> GameplayHudMotion {
        GameplayHudMotion::Settled
    }

    fn gameplay_entry(&self, _restarting: bool, _prepared: bool) -> Option<(Vec<A>, f32)> {
        None
    }

    fn gameplay_outro(
        &self,
        _view: StageOutroView<'_>,
    ) -> Option<Box<dyn StageOutroPresentation<A>>> {
        None
    }

    fn evaluation_overview(&mut self) -> Option<&mut dyn EvaluationOverviewPresentation<A>> {
        None
    }

    fn evaluation_intro_settled(&self, _elapsed: f32, _overview: bool) -> bool {
        true
    }

    fn evaluation_entry_sfx_enabled(&self) -> bool {
        true
    }

    fn leave_evaluation(&mut self) {}
}
