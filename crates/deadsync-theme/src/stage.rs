//! Presentation choices for a stage and its results. The host supplies game facts.

use deadlib_assets::AssetManager;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageCardMode {
    Regular,
    Final,
    Event,
    Nonstop,
    Oni,
    Endless,
    Survival,
}

#[derive(Clone, Copy)]
pub struct StageCardTiming {
    pub hold: Duration,
    pub exit: Duration,
}

pub struct StageCardView {
    pub stage_number: usize,
    pub mode: StageCardMode,
    pub elapsed: f32,
    pub exit_elapsed: Option<f32>,
}

pub trait StageCardPresentation<A> {
    fn timing(&self) -> StageCardTiming;
    fn push_actors(&self, actors: &mut Vec<A>, view: StageCardView);
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
    fn push_overview(
        &mut self,
        actors: &mut Vec<A>,
        assets: &AssetManager,
        page: EvaluationOverview<'_>,
    );
}

/// Presentation for an accepted request to leave the result overview.
///
/// The host resolves the destination and owns the clock. The cue and overlay
/// use the same departure timing for individual songs and course pages.
pub struct EvaluationDeparture<A> {
    pub input_ready_seconds: f32,
    pub duration: f32,
    pub cue: Option<UiCue>,
    pub push_overlay: fn(&mut Vec<A>, f32),
}

/// Result-screen audio for one visit, including any cues from the fallback panes.
#[derive(Clone, Copy, Default)]
pub struct EvaluationAudio {
    pub music_loop: Option<&'static str>,
    pub fallback_sounds: EvaluationFallbackSounds,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum EvaluationFallbackSounds {
    #[default]
    All,
    DeferredOnly,
    None,
}

pub trait StagePresentation<A> {
    fn push_loading(&self, actors: &mut Vec<A>, view: StageLoadingView<'_>);

    fn gameplay_hud(&self) -> Option<GameplayHud<A>> {
        None
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

    /// Override the entry actors while retaining the stage's lead-in duration.
    fn gameplay_entry(&self, _restarting: bool, _prepared: bool) -> Option<Vec<A>> {
        None
    }

    /// Override the presentation clock for the stage entry animation.
    fn gameplay_entry_seconds(&self, _restarting: bool) -> Option<f32> {
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

    fn evaluation_departure(&self) -> Option<EvaluationDeparture<A>> {
        None
    }

    fn evaluation_audio(&self) -> EvaluationAudio {
        EvaluationAudio::default()
    }

    fn evaluation_intro_settled(&self, _elapsed: f32, _overview: bool) -> bool {
        true
    }

    fn leave_evaluation(&mut self) {}
}
