//! Stage chart selection from profile choices and authored Lua reloads.

use crate::{gameplay_play_style_from_profile, gameplay_player_side_from_profile};
use deadsync_chart::{ChartData, GameplayChartData, SongData};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::stage_charts::{self, GameplayChartEntryPlan, lua_reload_chart_indices};
use deadsync_gameplay::{GameplayInputPlayStyle as Style, GameplaySession};
use deadsync_profile::{PlayStyle, PlayerSide};
use deadsync_song_lua::playback::PreparedGameplaySongLua;
use std::sync::Arc;

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
