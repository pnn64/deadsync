//! Stage chart selection from profile choices and authored Lua reloads.

use crate::{gameplay_play_style_from_profile, gameplay_player_side_from_profile};
use deadsync_chart::{
    ChartData, GameplayChartData, STANDARD_DIFFICULTY_COUNT, SongBackgroundChange, SongData,
};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::stage_charts::{self, GameplayChartEntryPlan, lua_reload_chart_indices};
use deadsync_gameplay::{GameplayInputPlayStyle as Style, GameplaySession};
use deadsync_profile::{PlayStyle, PlayerSide};
use deadsync_song_lua::playback::PreparedGameplaySongLua;
use std::sync::Arc;

/// Resolve primary backgrounds using the final stage's master chart.
#[must_use]
pub fn stage_background_changes(
    song: &SongData,
    master_chart: &GameplayChartData,
    random_movies: bool,
) -> Vec<SongBackgroundChange> {
    let movies = deadsync_simfile::app_runtime::random_movie_paths(song, random_movies);
    deadsync_simfile::app_runtime::gameplay_background_changes(song, master_chart, movies)
}

/// Standard chart slots select their own difficulty. Edits retain the
/// player's preferred standard difficulty for fallback and profile saves.
#[must_use]
pub fn stage_difficulty_indices(
    steps: [usize; MAX_PLAYERS],
    preferred: [usize; MAX_PLAYERS],
) -> [usize; MAX_PLAYERS] {
    std::array::from_fn(|player| {
        if steps[player] < STANDARD_DIFFICULTY_COUNT {
            steps[player]
        } else {
            preferred[player].min(STANDARD_DIFFICULTY_COUNT - 1)
        }
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectMusicJoinContext {
    pub active_side: PlayerSide,
    pub join_side: PlayerSide,
    pub selected_steps: usize,
    pub preferred_difficulty: usize,
    pub p1_profile_preferred: usize,
    pub p2_profile_preferred: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectMusicJoinPlan {
    pub selected_steps: usize,
    pub preferred_difficulty: usize,
    pub p2_selected_steps: usize,
    pub p2_preferred_difficulty: usize,
}

#[must_use]
pub const fn select_music_join_plan(context: SelectMusicJoinContext) -> SelectMusicJoinPlan {
    if matches!(context.active_side, PlayerSide::P2) && matches!(context.join_side, PlayerSide::P1)
    {
        SelectMusicJoinPlan {
            selected_steps: context.p1_profile_preferred,
            preferred_difficulty: context.p1_profile_preferred,
            p2_selected_steps: context.selected_steps,
            p2_preferred_difficulty: context.preferred_difficulty,
        }
    } else {
        SelectMusicJoinPlan {
            selected_steps: context.selected_steps,
            preferred_difficulty: context.preferred_difficulty,
            p2_selected_steps: context.p2_profile_preferred,
            p2_preferred_difficulty: context.p2_profile_preferred,
        }
    }
}

/// Keep the incumbent's chart and start the joining player at the easiest
/// available chart. The new player's saved difficulty remains their preference.
#[must_use]
pub fn select_music_join_charts(
    context: SelectMusicJoinContext,
    song: Option<&SongData>,
    play_style: PlayStyle,
) -> SelectMusicJoinPlan {
    let mut plan = select_music_join_plan(context);
    if let Some(first) = song.and_then(|song| song.best_steps_index(play_style.chart_type(), 0)) {
        match context.join_side {
            PlayerSide::P1 => plan.selected_steps = first,
            PlayerSide::P2 => plan.p2_selected_steps = first,
        }
    }
    plan
}

#[must_use]
pub fn gameplay_chart_entry_plan(
    song: &SongData,
    requested_steps: [usize; MAX_PLAYERS],
    preferred_steps: [usize; MAX_PLAYERS],
    play_style: PlayStyle,
    player_side: PlayerSide,
) -> GameplayChartEntryPlan {
    stage_charts::gameplay_chart_entry_plan(
        song,
        requested_steps,
        preferred_steps,
        gameplay_play_style_from_profile(play_style),
        gameplay_player_side_from_profile(player_side),
    )
}

/// Commit Lua's chart and session changes together once the replacement
/// payloads have loaded successfully.
pub fn apply_song_lua_reload<S>(
    song: &SongData,
    charts: &mut [Arc<ChartData>; MAX_PLAYERS],
    payloads: &mut [Arc<GameplayChartData>; MAX_PLAYERS],
    prepared: &PreparedGameplaySongLua<S>,
    session: &mut GameplaySession,
    global_offset_seconds: f32,
) -> Result<bool, String> {
    let Some(steps) = prepared.loaded_steps() else {
        return Ok(false);
    };
    let mut selected_session = session.clone();
    if let Some(players) = prepared.loaded_players() {
        selected_session.joined_sides = if session.p2_runtime_player() {
            [players[1], players[0]]
        } else {
            players
        };
    }
    if let Some(style) = prepared.loaded_style() {
        selected_session.play_style = match (session.play_style.is_pump(), style) {
            (false, "single") => Style::Single,
            (false, "double") => Style::Double,
            (false, "versus") => Style::Versus,
            (false, "couple") => Style::Couple,
            (false, "routine") => Style::Routine,
            // Compiled Pump scenes retain the game-qualified style key.
            (true, "single" | "pump-single") => Style::PumpSingle,
            (true, "double" | "pump-double") => Style::PumpDouble,
            (true, "versus" | "pump-versus") => Style::PumpVersus,
            (true, "couple" | "pump-couple") => Style::PumpCouple,
            (true, "routine" | "pump-routine") => Style::PumpRoutine,
            _ => return Err(format!("Lua reload selected unsupported style {style}")),
        };
        if selected_session.play_style.player_count()
            != selected_session
                .joined_sides
                .iter()
                .filter(|joined| **joined)
                .count()
        {
            return Err(format!(
                "Lua reload style {style} is incompatible with joined sides"
            ));
        }
    }
    let indices = lua_reload_chart_indices(song, steps, selected_session.play_style)?;
    let loaded =
        deadsync_simfile::app_runtime::load_gameplay_charts(song, &indices, global_offset_seconds)?;
    *payloads = stage_charts::take_player_charts(loaded);
    *charts = std::array::from_fn(|player| Arc::new(song.charts[indices[player]].clone()));
    *session = selected_session;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_chart_types_match_profile_selection_for_each_style() {
        for style in [
            PlayStyle::Single,
            PlayStyle::Versus,
            PlayStyle::Double,
            PlayStyle::Couple,
            PlayStyle::Routine,
            PlayStyle::PumpSingle,
            PlayStyle::PumpVersus,
            PlayStyle::PumpDouble,
            PlayStyle::PumpCouple,
            PlayStyle::PumpRoutine,
        ] {
            assert_eq!(
                gameplay_play_style_from_profile(style).chart_type(),
                style.chart_type()
            );
        }
    }
}
