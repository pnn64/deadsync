use crate::Command;
use deadsync_chart::{ChartData, GameplayChartData, SongData};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_profile::{PlayStyle, PlayerSide, player_side_index};
use log::warn;
use std::sync::Arc;

pub(crate) struct GameplayChartEntryPlan {
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

pub(crate) fn gameplay_chart_entry_plan(
    song: &Arc<SongData>,
    requested_steps: [usize; MAX_PLAYERS],
    preferred_steps: [usize; MAX_PLAYERS],
    play_style: PlayStyle,
    player_side: PlayerSide,
) -> GameplayChartEntryPlan {
    let chart_type = play_style.chart_type();
    let mut resolved_steps_index = requested_steps;
    let chart_indices = match play_style {
        PlayStyle::Versus | PlayStyle::PumpVersus => {
            let p1 = resolve_chart(song, chart_type, requested_steps[0], preferred_steps[0]);
            let p2 = resolve_chart(song, chart_type, requested_steps[1], preferred_steps[1]);
            resolved_steps_index = [p1.0, p2.0];
            [p1.1, p2.1]
        }
        PlayStyle::Single | PlayStyle::Double | PlayStyle::PumpSingle | PlayStyle::PumpDouble => {
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

pub(crate) fn take_player_charts(
    loaded: Vec<GameplayChartData>,
) -> [Arc<GameplayChartData>; MAX_PLAYERS] {
    let mut loaded = loaded.into_iter();
    std::array::from_fn(|_| {
        Arc::new(
            loaded
                .next()
                .expect("loaded payload contains both player charts"),
        )
    })
}

pub(crate) fn gameplay_last_played_commands(
    song: &SongData,
    plan: &GameplayChartEntryPlan,
    preferred_steps: [usize; MAX_PLAYERS],
    play_style: PlayStyle,
    player_side: PlayerSide,
) -> Vec<Command> {
    let mut commands = Vec::with_capacity(MAX_PLAYERS);
    match play_style {
        PlayStyle::Versus | PlayStyle::PumpVersus => {
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
        PlayStyle::Single | PlayStyle::Double | PlayStyle::PumpSingle | PlayStyle::PumpDouble => {
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

pub type PreparedGameplaySongLua =
    deadsync_song_lua::playback::PreparedGameplaySongLua<deadsync_assets::noteskin::SpriteSlot>;

fn reload_chart_indices(
    song: &SongData,
    steps: [Option<usize>; MAX_PLAYERS],
    session: &deadsync_gameplay::GameplaySession,
) -> Result<[usize; MAX_PLAYERS], String> {
    let mut indices = [0; MAX_PLAYERS];
    for player in 0..session.play_style.player_count() {
        let index = steps[player].ok_or_else(|| format!("Lua reload has no chart for player {}", player + 1))?;
        let chart = song.charts.get(index)
            .ok_or_else(|| format!("Lua reload selected missing chart {index}"))?;
        let prefix = if session.play_style.is_pump() { "pump" } else { "dance" };
        let suffix = if session.play_style.is_double() { "double" } else { "single" };
        if chart.chart_type != format!("{prefix}-{suffix}") {
            return Err(format!("Lua reload selected incompatible chart type {}", chart.chart_type));
        }
        indices[player] = index;
    }
    if !session.play_style.is_versus() {
        // Lua P1 is the sole runtime player, including a cabinet P2 session.
        indices[1] = indices[0];
    }
    Ok(indices)
}

/// Replace metadata and note payloads together before gameplay owns them.
pub fn apply_song_lua_reload(
    song: &SongData,
    charts: &mut [Arc<ChartData>; MAX_PLAYERS],
    payloads: &mut [Arc<GameplayChartData>; MAX_PLAYERS],
    prepared: &PreparedGameplaySongLua,
    session: &mut deadsync_gameplay::GameplaySession,
    global_offset_seconds: f32,
) -> Result<bool, String> {
    let Some(steps) = prepared.loaded_steps() else { return Ok(false); };
    let mut selected_session = session.clone();
    if let Some(style) = prepared.loaded_style() {
        use deadsync_gameplay::GameplayInputPlayStyle as Style;
        selected_session.play_style = match (session.play_style.is_pump(), style) {
            (false, "single") => Style::Single,
            (false, "double") => Style::Double,
            (false, "versus") => Style::Versus,
            (true, "single") => Style::PumpSingle,
            (true, "double") => Style::PumpDouble,
            (true, "versus") => Style::PumpVersus,
            _ => return Err(format!("Lua reload selected unsupported style {style}")),
        };
        // GameState::SetCurrentStyle preserves joined sides and rejects styles
        // that require a different number of them.
        if selected_session.play_style.player_count() != session.joined_sides.iter().filter(|joined| **joined).count() {
            return Err(format!("Lua reload style {style} is incompatible with joined sides"));
        }
    }
    let indices = reload_chart_indices(song, steps, &selected_session)?;
    let loaded = deadsync_simfile::app_runtime::load_gameplay_charts(song, &indices, global_offset_seconds)?;
    // Update only after both players loaded successfully. ChartData carries
    // the selected chart's music path, which gameplay uses to start audio.
    *payloads = take_player_charts(loaded);
    *charts = std::array::from_fn(|player| Arc::new(song.charts[indices[player]].clone()));
    *session = selected_session;
    Ok(true)
}

/// Build production Lua host inputs, chart timings and initial resources.
pub fn song_lua_context(
    song: &SongData,
    charts: &[Arc<ChartData>; MAX_PLAYERS],
    timing: [&deadsync_rules::timing::TimingData; MAX_PLAYERS],
    player_profiles: &[deadsync_profile::Profile; MAX_PLAYERS],
    scroll_speed: &[deadsync_rules::scroll::ScrollSpeedSetting; MAX_PLAYERS],
    music_rate: f32,
    viewport: deadsync_gameplay::GameplayViewport,
    display_size: (u32, u32),
    display_aspect_ratio: f32,
    autoplay: f32,
    session: &deadsync_gameplay::GameplaySession,
    config: &deadsync_gameplay::GameplayConfig,
    video_renderer: &str,
    random_seed: u32,
    live: bool,
) -> Result<deadsync_song_lua::SongLuaCompileContext, String> {
    let mut context = deadsync_profile_gameplay::song_lua_play_context(
        song,
        charts,
        timing,
        player_profiles,
        scroll_speed,
        music_rate,
        viewport,
        display_size,
        display_aspect_ratio,
        autoplay,
        session,
        config,
        video_renderer,
    );
    context.random_seed = random_seed;
    context.live = live;
    // The screen owns the first clocked Update(0), after audio is initialized.
    context.defer_initial_live_update = live;
    context.screen_loading = live;
    let inputs = deadsync_simfile::app_runtime::load_chart_timing_and_notes(
        song,
        config.global_offset_seconds,
    )?;
    let sync_offset = if config.machine_pack_ini_offsets {
        deadsync_chart::song::sync_pref_offset(
            deadsync_profile_gameplay::gameplay_pack_data(song, None, None).sync_pref,
            config.machine_default_sync_pref,
        )
    } else {
        0.0
    };
    for (mut timing, notes, row_beats) in inputs {
        timing.shift_group_offset_seconds(sync_offset);
        context.song_step_timing.push(timing);
        context.song_step_notes.push(notes.into());
        context.song_step_row_beats.push(row_beats.into());
    }
    let catalog = deadsync_simfile::song::load_native_step_metadata(song)?;
    let playable_steps = std::mem::take(&mut context.song_steps);
    let playable_timing = std::mem::take(&mut context.song_step_timing);
    let playable_notes = std::mem::take(&mut context.song_step_notes);
    let playable_rows = std::mem::take(&mut context.song_step_row_beats);
    context.song_step_chart_indices = catalog.iter().map(|step| step.playable_index).collect();
    context.player_step_indices = context.player_step_indices.map(|selected|
        selected.and_then(|index| context.song_step_chart_indices.iter().position(|value| *value == Some(index))));
    for step in catalog {
        if let Some(index) = step.playable_index {
            context.song_steps.push(playable_steps[index].clone());
            context.song_step_timing.push(playable_timing[index].clone());
            context.song_step_notes.push(playable_notes[index].clone());
            context.song_step_row_beats.push(playable_rows[index].clone());
        } else {
            use deadsync_song_lua::SongLuaDifficulty as Difficulty;
            let difficulty = match step.difficulty.to_ascii_lowercase().as_str() {
                "beginner" => Difficulty::Beginner, "easy" | "basic" | "light" => Difficulty::Easy,
                "medium" | "another" | "standard" => Difficulty::Medium,
                "hard" | "difficult" | "heavy" => Difficulty::Hard,
                "challenge" | "expert" | "oni" => Difficulty::Challenge, _ => Difficulty::Edit,
            };
            let steps_type = step.chart_type.split('-').map(|part| {
                let mut chars = part.chars();
                chars.next().map_or_else(String::new, |first| format!("{}{}", first.to_ascii_uppercase(), chars.as_str()))
            }).collect::<Vec<_>>().join("_");
            context.song_steps.push(deadsync_song_lua::SongLuaStep {
                steps_type: format!("StepsType_{steps_type}"), difficulty,
                description: step.description, chart_name: step.chart_name,
                credit: step.credit, meter: step.meter, music_file: step.music_file,
                possible_grade_points: 0,
            });
            context.song_step_timing.push(context.song_timing.clone().unwrap_or_else(|| playable_timing[0].clone()));
            context.song_step_notes.push(Default::default());
            context.song_step_row_beats.push(Default::default());
        }
    }
    for (player, profile) in context.players.iter_mut().zip(player_profiles) {
        let Some(choice) = deadsync_assets::textures::resolve_texture_choice_entry(
            profile.judgment_graphic.texture_key(),
            deadsync_assets::textures::judgment_texture_choices(),
        ) else {
            continue;
        };
        let path = deadsync_assets::textures::texture_key_source_path(
            &choice.key,
            &choice.key,
            deadsync_assets::resolve_asset_path,
        );
        match std::fs::canonicalize(&path) {
            Ok(path) => player.judgment_texture = Some(path),
            Err(error) => warn!(
                "Cannot load initial Lua judgment {}: {error}",
                path.display()
            ),
        }
    }
    Ok(context)
}

/// Compile song Lua on the load worker into Rust playback data. Normal gameplay
/// never retains the compiler VM; live scenes are reserved for diagnostic tools.
pub fn prepare_song_lua(
    song: &SongData,
    charts: &[Arc<ChartData>; MAX_PLAYERS],
    timing: [&deadsync_rules::timing::TimingData; MAX_PLAYERS],
    player_profiles: &[deadsync_profile::Profile; MAX_PLAYERS],
    scroll_speed: &[deadsync_rules::scroll::ScrollSpeedSetting; MAX_PLAYERS],
    music_rate: f32,
    viewport: deadsync_gameplay::GameplayViewport,
    display_size: (u32, u32),
    display_aspect_ratio: f32,
    autoplay: f32,
    session: &deadsync_gameplay::GameplaySession,
    config: &deadsync_gameplay::GameplayConfig,
    video_renderer: &str,
    random_seed: u32,
) -> PreparedGameplaySongLua {
    if song.background_lua_changes.is_empty() && song.foreground_lua_changes.is_empty() {
        return PreparedGameplaySongLua::default();
    }
    let context = match song_lua_context(song, charts, timing, player_profiles, scroll_speed,
        music_rate, viewport, display_size, display_aspect_ratio, autoplay, session,
        config, video_renderer, random_seed, false) {
        Ok(context) => context,
        Err(error) => {
            warn!("Cannot load Lua chart timing for '{}': {error}", song.title);
            return PreparedGameplaySongLua::default();
        }
    };
    deadsync_song_lua::playback::prepare_song_lua(
        song, &context, deadsync_assets::song_lua::compile_song_lua,
        deadsync_assets::song_lua::compile_song_lua_layers,
    )
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
            music_file: String::new(),
            music_path: None,
            short_hash: hash.to_string(),
            stats: ArrowStats::default(),
            tech_counts: TechCounts::default(),
            mines_nonfake: 0,
            stamina_counts: StaminaCounts::default(),
            total_streams: 0,
            matrix_rating: 0.0,
            matrix_profile: Box::default(),
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
            translit_artist: String::new(),
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
            song_timing: None,
            music_length_seconds: 60.0,
            first_second: 0.0,
            total_length_seconds: 60,
            precise_last_second_seconds: 60.0,
            precise_note_last_second_seconds: Some(60.0),
            last_second_hint: 0.0,
            charts: vec![chart("Hard", "hard"), chart("Challenge", "challenge")],
        })
    }

    #[test]
    fn reload_maps_lua_p1_to_single_p2_payloads() {
        let session = deadsync_gameplay::GameplaySession {
            player_side: deadsync_gameplay::GameplayInputPlayerSide::P2,
            joined_sides: [false, true],
            ..Default::default()
        };
        assert_eq!(reload_chart_indices(&song(), [Some(1), None], &session), Ok([1, 1]));
    }

    #[test]
    fn reload_keeps_versus_selections_and_rejects_missing_steps() {
        let session = deadsync_gameplay::GameplaySession {
            play_style: deadsync_gameplay::GameplayInputPlayStyle::Versus,
            joined_sides: [true, true],
            ..Default::default()
        };
        assert_eq!(reload_chart_indices(&song(), [Some(1), Some(0)], &session), Ok([1, 0]));
        assert!(reload_chart_indices(&song(), [Some(1), None], &session).is_err());
        assert!(reload_chart_indices(&song(), [Some(1), Some(7)], &session).is_err());
        let mut incompatible = song().as_ref().clone();
        incompatible.charts[1].chart_type = "dance-double".into();
        assert!(reload_chart_indices(&incompatible, [Some(1), Some(0)], &session).is_err());
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

#[cfg(test)]
#[path = "../../../tests/perf/owned_charts.rs"]
mod owned_pipeline_tests;
