//! Song selection data used by the shared stage launch controller.

use crate::GameplayInputPlayStyle;
use deadsync_chart::SongData;
use deadsync_core::input::MAX_PLAYERS;
use std::sync::Arc;

pub use crate::stage_launch::{
    OPTIONS_ENTRY_SECONDS, OPTIONS_WINDOW_SECONDS, StageLaunchPrompt as SongLaunchPrompt,
    StageLaunchTarget as SongLaunchTarget,
};

pub type SongLaunch = crate::stage_launch::StageLaunch<SongSelection>;
pub type SongLaunchRequest = crate::stage_launch::StageLaunchRequest<SongSelection>;

/// The selected song and chart slots in wheel order. Solo play uses the primary slot.
#[derive(Clone, Debug)]
pub struct SongSelection {
    pub song: Arc<SongData>,
    pub steps: [usize; MAX_PLAYERS],
    pub preferred: [usize; MAX_PLAYERS],
}

impl SongSelection {
    pub const fn chart_slots(
        &self,
        play_style: GameplayInputPlayStyle,
    ) -> ([usize; MAX_PLAYERS], [usize; MAX_PLAYERS]) {
        if play_style.player_count() == 1 {
            (
                [self.steps[0]; MAX_PLAYERS],
                [self.preferred[0]; MAX_PLAYERS],
            )
        } else {
            (self.steps, self.preferred)
        }
    }
}
