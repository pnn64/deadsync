//! Load chart data and assemble the stage's Song Lua context.

use super::PreparedGameplaySongLua;
use deadsync_assets::song_lua::{
    compile_song_lua, compile_song_lua_layers, resolve_song_lua_judgment_textures,
    song_lua_play_context,
};
use deadsync_chart::{ChartData, SongData};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_profile_gameplay::gameplay_pack_data;
use deadsync_simfile::song::NativeStep;
use deadsync_song_lua::SongLuaMediaProbes;
use log::warn;
use std::sync::Arc;

/// Media metadata available to the stage and headless callers using the same assets.
pub const SONG_LUA_MEDIA_PROBES: SongLuaMediaProbes = SongLuaMediaProbes {
    sound_length_seconds: Some(song_lua_sound_length_seconds),
    ..deadsync_assets::song_lua::MEDIA_PROBES
};

fn song_lua_sound_length_seconds(path: &std::path::Path) -> Result<f32, String> {
    deadlib_audio_decode::file_length_seconds(path, Default::default())
}

/// Build production Lua host inputs, chart timings and initial resources.
pub fn song_lua_context(
    media_probes: SongLuaMediaProbes,
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
    let mut context = song_lua_play_context(
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
    context.media_probes = SongLuaMediaProbes {
        movie_dimensions: media_probes.movie_dimensions.or(context.media_probes.movie_dimensions),
        ..media_probes
    };
    context.random_seed = random_seed;
    context.live = live;
    context.theme_combo_font_directory = Some(deadsync_assets::resolve_asset_path("assets/fonts/_combo"));
    // The screen owns the first clocked Update(0), after audio is initialized.
    context.defer_initial_live_update = live;
    context.screen_loading = live;
    let cfg = deadsync_config::runtime::get();
    let inputs = deadsync_simfile::app_runtime::load_song_lua_chart_data(
        song,
        deadsync_simfile::runtime::RuntimeSongConfig::new(
            cfg.fastload,
            cfg.cachesongs,
            config.global_offset_seconds,
        ),
        deadsync_config::runtime::group_is_never_cached,
    )?;
    let sync_offset = if config.machine_pack_ini_offsets {
        deadsync_chart::song::sync_pref_offset(
            gameplay_pack_data(song, None, None).sync_pref,
            config.machine_default_sync_pref,
        )
    } else {
        0.0
    };
    for (mut timing, notes, row_beats) in inputs.charts {
        timing.shift_group_offset_seconds(sync_offset);
        context.song_step_timing.push(timing);
        context.song_step_notes.push(notes.into());
        context.song_step_row_beats.push(row_beats.into());
    }
    let catalog = inputs.native_steps;
    let playable_steps = std::mem::take(&mut context.song_steps);
    let playable_timing = std::mem::take(&mut context.song_step_timing);
    let playable_notes = std::mem::take(&mut context.song_step_notes);
    let playable_rows = std::mem::take(&mut context.song_step_row_beats);
    context.song_step_chart_indices = catalog.iter().map(NativeStep::playable_index).collect();
    context.player_step_indices = context.player_step_indices.map(|selected| {
        selected.and_then(|index| {
            context
                .song_step_chart_indices
                .iter()
                .position(|value| *value == Some(index))
        })
    });
    for step in catalog {
        if let NativeStep::Playable(index) = step {
            context.song_steps.push(playable_steps[index].clone());
            context
                .song_step_timing
                .push(playable_timing[index].clone());
            context.song_step_notes.push(playable_notes[index].clone());
            context
                .song_step_row_beats
                .push(playable_rows[index].clone());
        } else if let NativeStep::Unplayed(step) = step {
            use deadsync_song_lua::SongLuaDifficulty as Difficulty;
            let difficulty = match step.difficulty.to_ascii_lowercase().as_str() {
                "beginner" => Difficulty::Beginner,
                "easy" | "basic" | "light" => Difficulty::Easy,
                "medium" | "another" | "standard" => Difficulty::Medium,
                "hard" | "difficult" | "heavy" => Difficulty::Hard,
                "challenge" | "expert" | "oni" => Difficulty::Challenge,
                _ => Difficulty::Edit,
            };
            let steps_type = step
                .chart_type
                .split('-')
                .map(|part| {
                    let mut chars = part.chars();
                    chars.next().map_or_else(String::new, |first| {
                        format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
                    })
                })
                .collect::<Vec<_>>()
                .join("_");
            context.song_steps.push(deadsync_song_lua::SongLuaStep {
                steps_type: format!("StepsType_{steps_type}"),
                difficulty,
                description: step.description,
                chart_name: step.chart_name,
                credit: step.credit,
                meter: step.meter,
                music_file: step.music_file,
                possible_grade_points: 0,
            });
            context.song_step_timing.push(
                context
                    .song_timing
                    .clone()
                    .unwrap_or_else(|| playable_timing[0].clone()),
            );
            context.song_step_notes.push(Default::default());
            context.song_step_row_beats.push(Default::default());
        }
    }
    resolve_song_lua_judgment_textures(&mut context, player_profiles);
    Ok(context)
}

/// Prepare baked or runtime song playback on the load worker, retaining authored
/// Lua when input callbacks or player statistics require runtime execution even in baked mode.
pub fn prepare_song_lua(
    media_probes: SongLuaMediaProbes,
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
    mode: deadsync_config::options::SongLuaMode,
    bake_progress: Option<deadsync_song_lua::SongLuaBakeProgress>,
) -> PreparedGameplaySongLua {
    if song.background_lua_changes.is_empty() && song.foreground_lua_changes.is_empty() {
        return PreparedGameplaySongLua::default();
    }
    let live = mode == deadsync_config::options::SongLuaMode::Runtime;
    let mut context = match song_lua_context(
        media_probes,
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
        random_seed,
        live,
    ) {
        Ok(context) => context,
        Err(error) => {
            warn!("Cannot load Lua chart timing for '{}': {error}", song.title);
            return PreparedGameplaySongLua::default();
        }
    };
    context.bake_progress = bake_progress.clone().filter(|_| !live);
    let prepared = deadsync_song_lua::playback::prepare_song_lua_with_runtime_fallback(
        song,
        &context,
        compile_song_lua,
        compile_song_lua_layers,
    );
    if !live && let Some(progress) = bake_progress {
        progress.complete();
    }
    prepared
}
