//! Resolve the chart metadata and payload indices admitted to a gameplay stage.

use crate::{GameplayInputPlayStyle, GameplayInputPlayerSide, gameplay_player_side_index};
use deadsync_chart::{ChartData, GameplayChartData, SongData};
use deadsync_core::input::MAX_PLAYERS;
use log::warn;
use std::sync::Arc;

pub struct GameplayChartEntryPlan {
    /// Chart metadata in runtime player order. A solo P2 stage still uses slot zero.
    pub charts: [Arc<ChartData>; MAX_PLAYERS],
    /// Indices into the song's chart catalog, in the same order as `charts`.
    pub chart_indices: [usize; MAX_PLAYERS],
    /// Wheel difficulty slots in cabinet side order, including standard fallbacks.
    pub resolved_steps_index: [usize; MAX_PLAYERS],
}

fn chart_index(song: &SongData, chart: &ChartData) -> usize {
    song.charts
        .iter()
        .position(|candidate| std::ptr::eq(candidate, chart))
        .expect("resolved chart must belong to the selected song")
}

fn resolve_chart(
    song: &SongData,
    chart_type: &str,
    requested_steps: usize,
    preferred_steps: usize,
) -> (usize, usize) {
    if let Some((steps, chart)) =
        resolve_indexed_chart(song, chart_type, requested_steps, preferred_steps)
    {
        if steps != requested_steps {
            warn!(
                "Missing stepchart index {} for '{}'; using fallback index {}",
                requested_steps, song.title, steps
            );
        }
        return (steps, chart_index(song, chart));
    }

    let chart_index = song
        .charts
        .iter()
        .position(|chart| chart.chart_type.eq_ignore_ascii_case(chart_type))
        .or_else(|| (!song.charts.is_empty()).then_some(0))
        .expect("player options song must contain at least one chart");
    let chart = &song.charts[chart_index];
    warn!(
        "Missing indexed stepchart for '{}'; using raw chart fallback ({}/{})",
        song.title, chart.chart_type, chart.difficulty
    );
    (requested_steps, chart_index)
}

/// Resolve an indexed chart using the same preferred fallback as stage entry.
pub(crate) fn resolve_indexed_chart<'a>(
    song: &'a SongData,
    chart_type: &str,
    requested_steps: usize,
    preferred_steps: usize,
) -> Option<(usize, &'a ChartData)> {
    if let Some(chart) = song.chart_for_steps_index(chart_type, requested_steps) {
        return Some((requested_steps, chart));
    }
    let steps = song.best_steps_index(chart_type, preferred_steps)?;
    song.chart_for_steps_index(chart_type, steps)
        .map(|chart| (steps, chart))
}

#[must_use]
pub fn gameplay_chart_entry_plan(
    song: &SongData,
    requested_steps: [usize; MAX_PLAYERS],
    preferred_steps: [usize; MAX_PLAYERS],
    play_style: GameplayInputPlayStyle,
    player_side: GameplayInputPlayerSide,
) -> GameplayChartEntryPlan {
    use GameplayInputPlayStyle as Style;

    let chart_type = play_style.chart_type();
    let mut resolved_steps_index = requested_steps;
    let chart_indices = match play_style {
        // Partner styles play one authored chart split between both participants.
        Style::Couple | Style::PumpCouple | Style::Routine | Style::PumpRoutine => {
            let (steps, index) =
                resolve_chart(song, chart_type, requested_steps[0], preferred_steps[0]);
            resolved_steps_index = [steps; MAX_PLAYERS];
            [index; MAX_PLAYERS]
        }
        Style::Versus | Style::PumpVersus => std::array::from_fn(|player| {
            let (steps, index) = resolve_chart(
                song,
                chart_type,
                requested_steps[player],
                preferred_steps[player],
            );
            resolved_steps_index[player] = steps;
            index
        }),
        Style::Single | Style::Double | Style::PumpSingle | Style::PumpDouble => {
            let side = gameplay_player_side_index(player_side);
            let (steps, index) = resolve_chart(
                song,
                chart_type,
                requested_steps[side],
                preferred_steps[side],
            );
            resolved_steps_index[side] = steps;
            [index; MAX_PLAYERS]
        }
    };
    let first = Arc::new(song.charts[chart_indices[0]].clone());
    let second = if chart_indices[0] == chart_indices[1] {
        Arc::clone(&first)
    } else {
        Arc::new(song.charts[chart_indices[1]].clone())
    };

    GameplayChartEntryPlan {
        charts: [first, second],
        chart_indices,
        resolved_steps_index,
    }
}

/// Validate the source chart indices supplied by song Lua. Lua P1 denotes the
/// sole runtime player even when the selected cabinet side is P2.
pub fn lua_reload_chart_indices(
    song: &SongData,
    steps: [Option<usize>; MAX_PLAYERS],
    play_style: GameplayInputPlayStyle,
) -> Result<[usize; MAX_PLAYERS], String> {
    let mut indices = [0; MAX_PLAYERS];
    for player in 0..play_style.player_count() {
        let index = steps[player]
            .ok_or_else(|| format!("Lua reload has no chart for player {}", player + 1))?;
        let chart = song
            .charts
            .get(index)
            .ok_or_else(|| format!("Lua reload selected missing chart {index}"))?;
        if chart.chart_type != play_style.chart_type() {
            return Err(format!(
                "Lua reload selected incompatible chart type {}",
                chart.chart_type
            ));
        }
        indices[player] = index;
    }
    if !play_style.is_versus() {
        indices[1] = indices[0];
    }
    Ok(indices)
}

pub fn take_player_charts(loaded: Vec<GameplayChartData>) -> [Arc<GameplayChartData>; MAX_PLAYERS] {
    let mut loaded = loaded.into_iter();
    std::array::from_fn(|_| {
        Arc::new(
            loaded
                .next()
                .expect("loaded payload contains both player charts"),
        )
    })
}
