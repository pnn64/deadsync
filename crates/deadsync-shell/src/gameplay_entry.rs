use crate::Command;
use deadsync_chart::{ChartData, SongData};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_profile::{PlayStyle, PlayerSide, player_side_index};
use log::warn;
use std::sync::Arc;

pub struct GameplayChartEntryPlan {
    pub charts: [Arc<ChartData>; MAX_PLAYERS],
    pub chart_indices: [usize; MAX_PLAYERS],
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
    if let Some(chart) = song.chart_for_steps_index(chart_type, requested_steps) {
        return (requested_steps, chart_index(song, chart));
    }

    if let Some(fallback_steps) = song.best_steps_index(chart_type, preferred_steps)
        && let Some(chart) = song.chart_for_steps_index(chart_type, fallback_steps)
    {
        warn!(
            "Missing stepchart index {} for '{}'; using fallback index {}",
            requested_steps, song.title, fallback_steps
        );
        return (fallback_steps, chart_index(song, chart));
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

pub fn gameplay_chart_entry_plan(
    song: &Arc<SongData>,
    requested_steps: [usize; MAX_PLAYERS],
    preferred_steps: [usize; MAX_PLAYERS],
    play_style: PlayStyle,
    player_side: PlayerSide,
) -> GameplayChartEntryPlan {
    let chart_type = play_style.chart_type();
    let mut resolved_steps_index = requested_steps;
    let chart_indices = match play_style {
        PlayStyle::Versus => {
            let p1 = resolve_chart(song, chart_type, requested_steps[0], preferred_steps[0]);
            let p2 = resolve_chart(song, chart_type, requested_steps[1], preferred_steps[1]);
            resolved_steps_index = [p1.0, p2.0];
            [p1.1, p2.1]
        }
        PlayStyle::Single | PlayStyle::Double => {
            let side = player_side_index(player_side);
            let resolved = resolve_chart(
                song,
                chart_type,
                requested_steps[side],
                preferred_steps[side],
            );
            resolved_steps_index[side] = resolved.0;
            [resolved.1; MAX_PLAYERS]
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

pub fn gameplay_last_played_commands(
    song: &SongData,
    plan: &GameplayChartEntryPlan,
    preferred_steps: [usize; MAX_PLAYERS],
    play_style: PlayStyle,
    player_side: PlayerSide,
) -> Vec<Command> {
    let mut commands = Vec::with_capacity(MAX_PLAYERS);
    match play_style {
        PlayStyle::Versus => {
            for (idx, side) in [(0, PlayerSide::P1), (1, PlayerSide::P2)] {
                commands.push(Command::UpdateLastPlayed {
                    side,
                    play_style,
                    music_path: song.music_path.clone(),
                    chart_hash: Some(plan.charts[idx].short_hash.clone()),
                    difficulty_index: preferred_steps[idx],
                });
            }
        }
        PlayStyle::Single | PlayStyle::Double => {
            let index = player_side_index(player_side);
            commands.push(Command::UpdateLastPlayed {
                side: player_side,
                play_style,
                music_path: song.music_path.clone(),
                chart_hash: Some(plan.charts[index].short_hash.clone()),
                difficulty_index: preferred_steps[index],
            });
        }
    }
    commands
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_chart::{ArrowStats, StaminaCounts, TechCounts};
    use std::path::PathBuf;

    fn chart(difficulty: &str, hash: &str) -> ChartData {
        ChartData {
            chart_type: "dance-single".to_string(),
            difficulty: difficulty.to_string(),
            description: String::new(),
            chart_name: String::new(),
            meter: 9,
            step_artist: String::new(),
            music_path: None,
            short_hash: hash.to_string(),
            stats: ArrowStats::default(),
            tech_counts: TechCounts::default(),
            mines_nonfake: 0,
            stamina_counts: StaminaCounts::default(),
            total_streams: 0,
            matrix_rating: 0.0,
            max_nps: 0.0,
            sn_detailed_breakdown: String::new(),
            sn_partial_breakdown: String::new(),
            sn_simple_breakdown: String::new(),
            detailed_breakdown: String::new(),
            partial_breakdown: String::new(),
            simple_breakdown: String::new(),
            total_measures: 0,
            measure_nps_vec: Vec::new(),
            measure_seconds_vec: Vec::new(),
            first_second: 0.0,
            has_note_data: true,
            has_chart_attacks: false,
            possible_grade_points: 0,
            holds_total: 0,
            rolls_total: 0,
            mines_total: 0,
            display_bpm: None,
            min_bpm: 120.0,
            max_bpm: 120.0,
        }
    }

    fn song() -> Arc<SongData> {
        Arc::new(SongData {
            simfile_path: PathBuf::from("song.ssc"),
            title: "Song".to_string(),
            subtitle: String::new(),
            translit_title: String::new(),
            translit_subtitle: String::new(),
            artist: String::new(),
            genre: String::new(),
            banner_path: None,
            background_path: None,
            background_changes: Vec::new(),
            background_layer2_changes: Vec::new(),
            foreground_changes: Vec::new(),
            background_lua_changes: Vec::new(),
            foreground_lua_changes: Vec::new(),
            has_lua: false,
            cdtitle_path: None,
            music_path: Some(PathBuf::from("song.ogg")),
            display_bpm: String::new(),
            offset: 0.0,
            sample_start: None,
            sample_length: None,
            min_bpm: 120.0,
            max_bpm: 120.0,
            normalized_bpms: "120.000".to_string(),
            music_length_seconds: 60.0,
            first_second: 0.0,
            total_length_seconds: 60,
            precise_last_second_seconds: 60.0,
            charts: vec![chart("Hard", "hard"), chart("Challenge", "challenge")],
        })
    }

    #[test]
    fn versus_keeps_independent_chart_slots_and_profile_updates() {
        let plan =
            gameplay_chart_entry_plan(&song(), [3, 4], [3, 4], PlayStyle::Versus, PlayerSide::P1);

        assert_eq!(plan.resolved_steps_index, [3, 4]);
        assert_eq!(plan.chart_indices, [0, 1]);
        assert_eq!(plan.charts[0].short_hash, "hard");
        assert_eq!(plan.charts[1].short_hash, "challenge");
        let commands = gameplay_last_played_commands(
            song().as_ref(),
            &plan,
            [3, 4],
            PlayStyle::Versus,
            PlayerSide::P1,
        );
        assert_eq!(commands.len(), 2);
        assert!(matches!(
            &commands[1],
            Command::UpdateLastPlayed {
                side: PlayerSide::P2,
                chart_hash: Some(hash),
                difficulty_index: 4,
                ..
            } if hash == "challenge"
        ));
    }

    #[test]
    fn single_p2_falls_back_and_duplicates_the_active_chart() {
        let plan =
            gameplay_chart_entry_plan(&song(), [3, 0], [3, 4], PlayStyle::Single, PlayerSide::P2);

        assert_eq!(plan.resolved_steps_index, [3, 4]);
        assert_eq!(plan.chart_indices, [1, 1]);
        assert_eq!(plan.charts[0].short_hash, "challenge");
        assert_eq!(plan.charts[1].short_hash, "challenge");
        assert!(Arc::ptr_eq(&plan.charts[0], &plan.charts[1]));
        let commands = gameplay_last_played_commands(
            song().as_ref(),
            &plan,
            [3, 4],
            PlayStyle::Single,
            PlayerSide::P2,
        );
        assert!(matches!(
            &commands[0],
            Command::UpdateLastPlayed {
                side: PlayerSide::P2,
                chart_hash: Some(hash),
                difficulty_index: 4,
                ..
            } if hash == "challenge"
        ));
    }
}
