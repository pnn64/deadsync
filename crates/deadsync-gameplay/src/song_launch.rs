//! Song selection data used by the shared stage launch controller.

use crate::GameplayInputPlayStyle;
use deadsync_chart::{STANDARD_DIFFICULTY_COUNT, SongData};
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

    /// Resolve the final two-player song choice before opening options or loading
    /// the stage. With no prior selection, incompatible Versus choices follow P2,
    /// the same precedence as changing both sides at once on the wheel.
    #[must_use]
    pub fn resolved_chart_slots(
        &self,
        play_style: GameplayInputPlayStyle,
    ) -> ([usize; MAX_PLAYERS], [usize; MAX_PLAYERS]) {
        let (steps, preferred) = self.chart_slots(play_style);
        if matches!(
            play_style,
            GameplayInputPlayStyle::Versus | GameplayInputPlayStyle::PumpVersus
        ) && steps[0] != steps[1]
        {
            synchronize_song_charts(&self.song, play_style, None, steps, preferred)
        } else {
            (steps, preferred)
        }
    }
}

/// Partner styles share one chart. Versus can use two charts with the same
/// gameplay music. When the slots must match, follow the changed side, giving
/// P2 priority if both changed. Edit slots leave the saved difficulty alone.
#[inline]
#[must_use]
pub fn synchronize_song_charts(
    song: &SongData,
    play_style: GameplayInputPlayStyle,
    previous_steps: Option<[usize; MAX_PLAYERS]>,
    steps: [usize; MAX_PLAYERS],
    mut preferred: [usize; MAX_PLAYERS],
) -> ([usize; MAX_PLAYERS], [usize; MAX_PLAYERS]) {
    use GameplayInputPlayStyle as Style;

    if !play_style.is_versus() || previous_steps == Some(steps) {
        return (steps, preferred);
    }

    if matches!(play_style, Style::Versus | Style::PumpVersus) {
        let chart_type = play_style.chart_type();
        if song.chart_music_path(chart_type, steps[0])
            == song.chart_music_path(chart_type, steps[1])
        {
            return (steps, preferred);
        }
    }

    let source = usize::from(previous_steps.is_none_or(|previous| previous[1] != steps[1]));
    let selected = steps[source];
    if selected < STANDARD_DIFFICULTY_COUNT {
        preferred[1 - source] = selected;
    }
    ([selected; MAX_PLAYERS], preferred)
}
