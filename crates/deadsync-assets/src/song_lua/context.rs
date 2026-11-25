//! Prepare song Lua from the game's resolved chart and profile inputs.

use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::SongLuaCompilePlayStyleLike;
use deadsync_profile_gameplay::{
    gameplay_pack_data, gameplay_turn_option, scroll_effects_from_option,
};
use log::warn;
use std::sync::Arc;

#[must_use]
const fn song_lua_difficulty_from_chart(difficulty: &str) -> deadsync_song_lua::SongLuaDifficulty {
    if difficulty.eq_ignore_ascii_case("beginner") {
        deadsync_song_lua::SongLuaDifficulty::Beginner
    } else if difficulty.eq_ignore_ascii_case("easy") || difficulty.eq_ignore_ascii_case("basic") {
        deadsync_song_lua::SongLuaDifficulty::Easy
    } else if difficulty.eq_ignore_ascii_case("medium")
        || difficulty.eq_ignore_ascii_case("standard")
    {
        deadsync_song_lua::SongLuaDifficulty::Medium
    } else if difficulty.eq_ignore_ascii_case("hard")
        || difficulty.eq_ignore_ascii_case("difficult")
    {
        deadsync_song_lua::SongLuaDifficulty::Hard
    } else if difficulty.eq_ignore_ascii_case("edit") {
        deadsync_song_lua::SongLuaDifficulty::Edit
    } else {
        deadsync_song_lua::SongLuaDifficulty::Challenge
    }
}

#[must_use]
const fn song_lua_speedmod_from_setting(
    speed: deadsync_rules::scroll::ScrollSpeedSetting,
) -> deadsync_song_lua::SongLuaSpeedMod {
    match speed {
        deadsync_rules::scroll::ScrollSpeedSetting::XMod(value) => {
            deadsync_song_lua::SongLuaSpeedMod::X(value)
        }
        deadsync_rules::scroll::ScrollSpeedSetting::CMod(value) => {
            deadsync_song_lua::SongLuaSpeedMod::C(value)
        }
        deadsync_rules::scroll::ScrollSpeedSetting::MMod(value) => {
            deadsync_song_lua::SongLuaSpeedMod::M(value)
        }
    }
}

fn song_lua_compile_player_screen_x(
    num_players: usize,
    player_index: usize,
    profile: &deadsync_profile::Profile,
    viewport: deadsync_gameplay::GameplayViewport,
    play_style: deadsync_gameplay::GameplayInputPlayStyle,
    player_side: deadsync_gameplay::GameplayInputPlayerSide,
    center_1player_notefield: bool,
) -> f32 {
    let style = play_style.as_song_lua_compile_play_style();
    let centered = num_players == 1
        && (style == deadsync_gameplay::SongLuaCompilePlayStyle::Double
            || (style == deadsync_gameplay::SongLuaCompilePlayStyle::Single
                && center_1player_notefield));
    if centered {
        return viewport.center_x();
    }
    let p2 = if num_players == 1 {
        deadsync_gameplay::gameplay_is_single_p2_side(play_style, player_side)
    } else {
        player_index == 1
    };
    let offset_sign = if p2 { 1.0 } else { -1.0 };
    deadsync_gameplay::gameplay_side_field_center_x(viewport.width(), viewport.center_x(), p2)
        + offset_sign * (profile.note_field_offset_x as f32).clamp(0.0, 50.0)
}

/// Resolve native catalog note times using the preferences at catalog loading.
/// Later game-mode preference changes do not alter Song's stored bounds.
#[must_use]
pub fn song_lua_catalog_note_bounds(
    song: &deadsync_chart::SongData,
    offset: f32,
    rate: f32,
    sync_offset: f32,
) -> [f32; 2] {
    let specified_last = if song.last_second_hint > 0.0 {
        song.last_second_hint
    } else {
        -1.0
    };
    let first = song
        .precise_note_first_second_seconds
        .map(|seconds| (seconds - sync_offset) - rate * offset);
    let last = song
        .precise_note_last_second_seconds
        .map(|seconds| (seconds - sync_offset) - rate * offset)
        .unwrap_or(-1.0)
        .max(specified_last);
    // Native applies catalog offsets to actual endpoints, then uses literal
    // zero when first>=last. That fallback must never acquire another offset.
    [first.filter(|first| *first < last).unwrap_or(0.0), last]
}

#[must_use]
pub fn song_lua_compile_context(
    song: &deadsync_chart::SongData,
    charts: &[Arc<deadsync_chart::ChartData>; deadsync_core::input::MAX_PLAYERS],
    num_players: usize,
    player_profiles: &[deadsync_profile::Profile; deadsync_core::input::MAX_PLAYERS],
    scroll_speed: &[deadsync_rules::scroll::ScrollSpeedSetting; deadsync_core::input::MAX_PLAYERS],
    music_rate: f32,
    machine_global_offset_seconds: f32,
    viewport: deadsync_gameplay::GameplayViewport,
    session: &deadsync_gameplay::GameplaySession,
    center_1player_notefield: bool,
) -> deadsync_song_lua::SongLuaCompileContext {
    let play_style = session.play_style;
    let player_side = session.player_side;
    let mut context = deadsync_song_lua::SongLuaCompileContext::new(
        song.simfile_path
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_default(),
        song.title.clone(),
    );
    context.media_probes = super::MEDIA_PROBES;
    context.song_steps = song
        .charts
        .iter()
        .map(deadsync_song_lua::SongLuaStep::from)
        .collect();
    context.song_banner_path.clone_from(&song.banner_path);
    context.player_step_indices = std::array::from_fn(|player| {
        let selected = &charts[player];
        // Edit charts can share both difficulty and description. Gameplay owns
        // cloned charts, so retain their authored identity and note hash when
        // recovering the catalog index for GetCurrentSteps and reloads.
        song.charts.iter().position(|chart| {
            chart.chart_type == selected.chart_type
                && chart.difficulty == selected.difficulty
                && chart.description == selected.description
                && chart.chart_name == selected.chart_name
                && chart.meter == selected.meter
                && chart.step_artist == selected.step_artist
                && chart.music_file == selected.music_file
                && chart.short_hash == selected.short_hash
        })
    });
    context.song_display_bpms = song.display_bpm_pair_or(
        charts.first().map(std::convert::AsRef::as_ref),
        [60.0, 60.0],
    );
    let timing_bpms = deadsync_song_lua::parse_song_timing_bpms(&song.normalized_bpms);
    context.song_timing = song.song_timing.clone();
    if !timing_bpms.is_empty() {
        context.song_timing_bpms = timing_bpms;
    }
    context.song_music_rate = if music_rate.is_finite() && music_rate > 0.0 {
        music_rate
    } else {
        1.0
    };
    context.music_length_seconds = song.music_length_seconds.max(song.precise_last_second());
    // The scan applies native chart eligibility and includes hold tails.
    // Neither audio EOF nor its rounded display length defines these bounds.
    context.song_note_bounds = Some(song_lua_catalog_note_bounds(
        song,
        machine_global_offset_seconds,
        1.0,
        0.0,
    ));
    context.style_name = match play_style {
        deadsync_gameplay::GameplayInputPlayStyle::Single => "single",
        deadsync_gameplay::GameplayInputPlayStyle::Versus => "versus",
        deadsync_gameplay::GameplayInputPlayStyle::Double => "double",
        deadsync_gameplay::GameplayInputPlayStyle::PumpSingle => "pump-single",
        deadsync_gameplay::GameplayInputPlayStyle::PumpVersus => "pump-versus",
        deadsync_gameplay::GameplayInputPlayStyle::PumpDouble => "pump-double",
        deadsync_gameplay::GameplayInputPlayStyle::Couple => "couple",
        deadsync_gameplay::GameplayInputPlayStyle::Routine => "routine",
        deadsync_gameplay::GameplayInputPlayStyle::PumpCouple => "pump-couple",
        deadsync_gameplay::GameplayInputPlayStyle::PumpRoutine => "pump-routine",
    }
    .to_string();
    context.global_offset_seconds = machine_global_offset_seconds;
    context.screen_width = viewport.width();
    context.screen_height = viewport.height();
    context.display_width = context.screen_width;
    context.display_height = context.screen_height;
    context.confusion_offset_available = true;
    context.confusion_available = true;
    context.amod_available = false;
    context.versus_player_screen_x = Some(std::array::from_fn(|player| {
        song_lua_compile_player_screen_x(
            2,
            player,
            &player_profiles[player],
            viewport,
            deadsync_gameplay::GameplayInputPlayStyle::Versus,
            player_side,
            false,
        )
    }));
    let turn_seed = deadsync_gameplay::turn_seed_for_song(song);
    let player_turn_options: [deadsync_gameplay::GameplayTurnOption; MAX_PLAYERS] =
        std::array::from_fn(|player| {
            if player < num_players {
                gameplay_turn_option(player_profiles[player].turn_option)
            } else {
                deadsync_gameplay::GameplayTurnOption::None
            }
        });
    context.players = std::array::from_fn(|player| deadsync_song_lua::SongLuaPlayerContext {
        enabled: player < num_players,
        autoplay: 0.0,
        turn_option: player_turn_options[player],
        column_take_from: deadsync_gameplay::turn_column_mapping(
            player_turn_options[player],
            play_style.cols_per_player(),
            turn_seed,
        ),
        difficulty: if player < num_players {
            song_lua_difficulty_from_chart(&charts[player].difficulty)
        } else {
            deadsync_song_lua::SongLuaDifficulty::default_enabled()
        },
        display_bpms: if player < num_players {
            song.display_bpm_pair_or(Some(charts[player].as_ref()), [60.0, 60.0])
        } else {
            [60.0, 60.0]
        },
        speedmod: if player < num_players {
            song_lua_speedmod_from_setting(scroll_speed[player])
        } else {
            deadsync_song_lua::SongLuaSpeedMod::default()
        },
        mini: if player < num_players {
            player_profiles[player].mini_percent as f32 / 100.0
        } else {
            0.0
        },
        scroll: if player < num_players {
            scroll_effects_from_option(player_profiles[player].scroll_option)
        } else {
            deadsync_gameplay::ScrollEffects::default()
        },
        noteskin_name: if player < num_players {
            player_profiles[player].noteskin.to_string()
        } else {
            deadsync_profile::NoteSkin::default().to_string()
        },
        perspective: if player < num_players {
            let (tilt, skew) = player_profiles[player].perspective.tilt_skew();
            deadsync_gameplay::PerspectiveEffects { tilt, skew }
        } else {
            deadsync_gameplay::PerspectiveEffects::default()
        },
        screen_x: song_lua_compile_player_screen_x(
            num_players,
            player,
            &player_profiles[player],
            viewport,
            play_style,
            player_side,
            center_1player_notefield,
        ),
        screen_y: viewport.center_y(),
        judgment_texture: None,
        column_cues: player < num_players && player_profiles[player].column_cues,
        background_filter_percent: if player < num_players {
            player_profiles[player].background_filter.percent()
        } else {
            0
        },
        hide_lifebar: player >= num_players || player_profiles[player].hide_lifebar,
        hide_danger: player_profiles[player].hide_danger,
        life_meter_type: player_profiles[player].lifemeter_type.to_string(),
        nps_graph_at_top: player < num_players && player_profiles[player].nps_graph_at_top,
        step_statistics_enabled: player < num_players
            && !player_profiles[player].step_statistics.is_empty(),
        timing_windows: player_profiles[player]
            .timing_windows
            .disabled_windows()
            .map(|disabled| !disabled),
    });
    context
}

/// `display_size` is captured on the render thread, never read on the worker.
pub fn song_lua_play_context(
    song: &deadsync_chart::SongData,
    charts: &[Arc<deadsync_chart::ChartData>; MAX_PLAYERS],
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
) -> deadsync_song_lua::SongLuaCompileContext {
    let first_player = usize::from(session.p2_runtime_player());
    let runtime_charts = [charts[first_player].clone(), charts[1].clone()];
    let runtime_profiles = [
        player_profiles[first_player].clone(),
        player_profiles[1].clone(),
    ];
    let runtime_scroll_speed = [scroll_speed[first_player], scroll_speed[1]];

    let mut context = song_lua_compile_context(
        song,
        &runtime_charts,
        session.play_style.player_count(),
        &runtime_profiles,
        &runtime_scroll_speed,
        music_rate,
        config.global_offset_seconds,
        viewport,
        session,
        config.center_1player_notefield,
    );
    let (display_width, display_height) = display_size;
    context.pad_stick_seconds = config.pad_stick_seconds;
    // These timings enter Lua before the core applies the cabinet/group offsets.
    // Match runtime_init's settings without substituting selected chart timing
    // for the global SongPosition maintained by ITGmania.
    let sync_offset = if config.machine_pack_ini_offsets {
        deadsync_chart::song::sync_pref_offset(
            gameplay_pack_data(song, None, None).sync_pref,
            config.machine_default_sync_pref,
        )
    } else {
        0.0
    };
    context.song_note_bounds = Some(song_lua_catalog_note_bounds(
        song,
        config.global_offset_seconds,
        1.0,
        sync_offset,
    ));
    if let Some(timing) = &mut context.song_timing {
        timing.shift_group_offset_seconds(sync_offset);
        timing.set_global_offset_seconds(config.global_offset_seconds);
    }
    context.player_timing = std::array::from_fn(|player| {
        let source = if session.p2_runtime_player() {
            1
        } else {
            player
        };
        context.players[player].enabled.then(|| {
            let mut value = timing[source].clone();
            value.shift_group_offset_seconds(sync_offset);
            let player_offset = if config.machine_allow_per_player_global_offsets {
                runtime_profiles[player]
                    .global_offset_shift_ms
                    .clamp(-100, 100) as f32
                    / 1000.0
            } else {
                0.0
            };
            value.set_global_offset_seconds(config.global_offset_seconds + player_offset);
            value
        })
    });
    context.display_width = display_width as f32;
    context.display_height = display_height as f32;
    context.display_aspect_ratio = display_aspect_ratio;
    for player in &mut context.players {
        player.autoplay = if player.enabled { autoplay } else { 0.0 };
    }
    // `VideoRenderers` is a legacy capability preference used by mod charts to
    // gate OpenGL-era ActorFrameTexture effects. DeadSync implements those
    // semantics above every physical backend, so advertise the compatibility
    // renderer first while retaining the actual backend for diagnostics.
    // Native null captures have no physical display or render targets.
    context.render_to_texture = !video_renderer.eq_ignore_ascii_case("null");
    context.video_renderers = if context.render_to_texture {
        format!("opengl,{video_renderer}")
    } else {
        video_renderer.to_string()
    };
    context
}

/// Resolve the initial judgment texture for each player in a stage's Lua context.
pub fn resolve_song_lua_judgment_textures(
    context: &mut deadsync_song_lua::SongLuaCompileContext,
    player_profiles: &[deadsync_profile::Profile; MAX_PLAYERS],
) {
    for (player, profile) in context.players.iter_mut().zip(player_profiles) {
        let Some(choice) = crate::textures::resolve_texture_choice_entry(
            profile.judgment_graphic.texture_key(),
            crate::textures::judgment_texture_choices(),
        ) else {
            continue;
        };
        let path = crate::textures::texture_key_source_path(
            &choice.key,
            &choice.key,
            crate::resolve_asset_path,
        );
        match std::fs::canonicalize(&path) {
            Ok(path) => player.judgment_texture = Some(path),
            Err(error) => warn!(
                "Cannot load initial Lua judgment {}: {error}",
                path.display()
            ),
        }
    }
}

/// Unset in tools and tests, which then bake every time.
static SONG_LUA_BAKE_CACHE: std::sync::OnceLock<deadsync_song_lua::bake_cache::BakeCache> =
    std::sync::OnceLock::new();

/// Keep completed song-Lua bakes under `directory` for later sessions.
/// `DEADSYNC_SONG_LUA_BAKE_CACHE=0` leaves the cache off, to compare a cached
/// load against a fresh bake without changing the Song Lua mode.
pub fn init_song_lua_bake_cache(directory: std::path::PathBuf) {
    if std::env::var_os("DEADSYNC_SONG_LUA_BAKE_CACHE").is_some_and(|value| value == "0") {
        log::info!("Song Lua bake cache disabled by DEADSYNC_SONG_LUA_BAKE_CACHE=0");
        return;
    }
    let _ = SONG_LUA_BAKE_CACHE.set(deadsync_song_lua::bake_cache::BakeCache::new(
        directory,
        super::SONG_LUA_BAKE_HOST_BUILD,
    ));
}

/// The bake cache, once [`init_song_lua_bake_cache`] has installed one.
#[must_use]
pub fn song_lua_bake_cache() -> Option<&'static deadsync_song_lua::bake_cache::BakeCache> {
    SONG_LUA_BAKE_CACHE.get()
}

#[cfg(test)]
mod tests;
