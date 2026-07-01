use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use deadlib_present::actors::TextAlign;
use deadlib_present::anim::{EffectClock, EffectMode};

mod actions;
mod cmd;
mod compat;
mod crypto;
mod eases;
mod files;
mod host;
mod json;
mod lua_util;
mod mod_windows;
mod multitap;
mod net;
mod noteskin;
mod option_rows;
mod perframe;
mod player_options;
mod runtime;
mod runtime_mod;
mod sl;
mod song_tables;
mod tables;
mod theme_colors;
mod timing;
mod top_screen;
mod values;
mod version;

pub use actions::{SongLuaFunctionActionInput, read_actions_with_function_capture};
pub use cmd::preprocess_lua_cmd_syntax;
pub use compat::{SongLuaCompatCallbacks, install_stdlib_compat};
pub use crypto::create_cryptman_table;
pub use eases::{
    SongLuaFunctionEaseDecision, SongLuaFunctionEaseInput, SongLuaFunctionEaseResult,
    SongLuaReadEasesResult, SongLuaReadEasesStats, read_eases_with_function_capture,
};
pub use files::{
    actor_util_class_registered, actor_util_file_type, call_with_chunk_env, call_with_script_dir,
    call_with_script_path, create_actor_util_table, create_fileman_table,
    create_find_files_function, create_lua_compat_table, entry_file_path, file_path_string,
    fileman_dir_listing, find_compat_files, is_song_lua_audio_path, is_song_lua_image_path,
    is_song_lua_media_path, is_song_lua_simfile_path, is_song_lua_video_path, path_basename,
    register_loader_env, resolve_compat_path, resolve_load_actor_path, resolve_script_path,
    retarget_loader_env, song_dir_string, song_group_name, song_lookup_matches, song_music_path,
    song_named_image_path, song_simfile_path, strip_sprite_hints, theme_path, wildcard_matches,
};
pub use host::{
    SONG_LUA_STARTUP_MESSAGE, SongLuaCompileGlobals, SongLuaDateGlobals, SongLuaGameStateGlobals,
    SongLuaHostState, clone_lua_value, create_arrow_effects_table, create_chunk_env_proxy,
    initial_chunk_environment, install_basic_globals, install_cmd_helpers, install_core_globals,
    install_date_globals, install_ease_table, install_game_state_globals, install_late_globals,
    install_manager_globals, install_message_manager_globals, install_screen_manager_globals,
    install_screen_string_globals, install_screen_utility_globals, install_sound_globals,
    is_compile_global_name, register_loaded_easing_names, restore_compile_globals,
    snapshot_compile_globals,
};
pub use json::{json_to_lua_value, lua_to_json_value};
pub use lua_util::{
    SongLuaActionCaptureScope, actor_active_commands, actor_aft_capture_name, actor_child_at,
    actor_children, actor_command_queue, actor_current_capture_block, actor_debug_label,
    actor_decode_movie, actor_diffuse, actor_direct_children, actor_effect_magnitude, actor_glow,
    actor_halign, actor_is_bitmap_text, actor_is_child_group, actor_named_children,
    actor_runs_startup_commands, actor_shadow_len, actor_table_has_update_functions,
    actor_texture_is_video, actor_texture_path, actor_tree_has_update_functions,
    actor_tween_time_left, actor_type_is, actor_update_text_pre_zoom_flags, actor_valign,
    actor_vertex_colors, actor_wrappers, actor_zoom_axis, banner_sort_order_path,
    begin_action_capture_scope, call_actor_function, call_table_method,
    capture_actor_command_preserving_state, capture_actor_text_attribute,
    capture_actor_vertex_diffuse, capture_block_set_bool, capture_block_set_color,
    capture_block_set_f32, capture_block_set_i32, capture_block_set_size,
    capture_block_set_stretch, capture_block_set_string, capture_block_set_u32,
    capture_block_set_vec2, capture_block_set_vec3, capture_block_set_vec4, capture_block_set_vec5,
    capture_block_set_vertex_colors, capture_block_set_zoom_axes, capture_scope_actor_pointers,
    capture_scope_actor_tables, capture_scope_snapshots, capture_texture_rect,
    classify_function_ease_probe, collect_aft_capture_names,
    collect_tracked_capture_blocks_for_indices, copy_dummy_actor_tags, create_actor_child_group,
    create_bool_array, create_color_constants_table, create_debug_table, create_owned_string_array,
    create_string_array, current_gamestate_player_value, current_gamestate_value,
    current_song_lua_style_name, current_song_value, current_steps_value,
    default_message_command_params, drain_actor_command_queue, finish_actor_tweening,
    flush_actor_capture, hurry_actor_tweening, inherit_actor_dirs, install_actor_metatable,
    install_course_contents_list_children, lua_format_text, lua_text_value,
    make_actor_add_f32_method, make_actor_capture_f32_method, make_actor_chain_method,
    make_actor_finish_tweening_method, make_actor_set_size_method, make_actor_stop_tweening_method,
    make_actor_tween_method, make_actor_wrap_width_method, make_color_table,
    make_vertex_color_table, method_arg, method_arg_offset, normalize_broadcast_params,
    note_zoom_point_hides, populate_course_contents_display, position_scroller_items,
    prepare_capture_scope_actor, probe_actor_pointers, probe_call_names,
    probe_function_ease_target, probe_target_kind, push_note_hide_window, push_sequence_child_once,
    push_unique_actor_child, read_actor_capture_blocks, read_actor_color_field,
    read_actor_multi_vertex_mesh, read_actor_multi_vertex_texture_path,
    read_actor_semantic_state_table, read_bitmap_font, read_bitmap_text_attributes,
    read_child_index, read_color_args, read_color_call, read_color_value, read_graph_display_size,
    read_graph_display_values, read_model_path, read_proxy_target_kind, read_song_lua_sound_paths,
    read_vertex_colors_value, record_probe_method_call, register_song_lua_actor,
    remove_actor_child, remove_all_actor_children, reset_actor_capture, reset_actor_capture_tables,
    reset_tracked_capture_tables, resolve_actor_asset_path, restore_action_capture_scope,
    restore_actor_mutable_state, restore_actors_semantic_state, run_actor_init_commands,
    run_actor_init_commands_for_table, run_actor_named_command, run_actor_named_command_with_drain,
    run_actor_named_command_with_drain_and_params, run_actor_startup_commands,
    run_actor_startup_commands_for_table, run_actor_update_functions,
    run_actor_update_functions_for_table, run_actor_update_functions_with_delta,
    run_added_actor_child_commands, run_command_on_leaves,
    run_named_command_on_children_recursively, run_named_command_on_leaves,
    set_actor_decode_movie_for_texture, set_actor_effect_defaults, set_actor_effect_mode,
    set_actor_sound_file_from_value, set_actor_sprite_state, set_actor_texture_from_path,
    set_actor_texture_from_path_methods, set_actor_texture_from_path_methods_or_fallback,
    set_actor_texture_from_value, set_proxy_target_fields, snapshot_actor_mutable_state,
    snapshot_actor_semantic_state_table, snapshot_actors_semantic_state, song_lua_actor_registry,
    song_lua_screen_center, song_lua_screen_size, table_bool_field, table_f32_field,
    table_i32_field, table_string_field, table_vec2, table_vec3, table_vec4, table_vec5,
    table_vertex_colors, text_attribute_matches, text_attribute_value,
    tracked_indices_for_actor_pointers,
};
pub use mod_windows::read_mod_windows;
pub use multitap::{
    MULTITAP_HIDE_EPSILON_BEATS, MULTITAP_PREVISIBLE_BEATS, MULTITAP_SAMPLE_STEP, MultitapDesc,
    MultitapPhase, apply_multitap_field_state, calc_multitap_phase, multitap_deco_child_state,
    multitap_deco_state, multitap_explosion_command_blocks, multitap_explosion_message_events,
    multitap_explosion_message_name, multitap_explosion_state, multitap_frame_state,
    overlay_delta_pair_from_states, push_multitap_actor_eases, push_multitap_arrow_sample,
    push_multitap_explosion_eases, push_overlay_sample_eases, read_multitap_descs,
};
pub use net::{create_network_table, encode_query_params, query_value_text, url_encode_component};
pub use noteskin::{SongLuaActorFactory, create_noteskin_table};
pub use option_rows::{
    SongLuaNamedOptionRowSpec, SongLuaOperatorOptionRowSpec, SongLuaOptionRowSpec,
    SongLuaOptionValues, THEME_PREF_ROW_NAMES, conf_option_row_spec, create_conf_option_row,
    create_custom_option_row, create_operator_menu_option_rows_table, create_sl_custom_prefs_table,
    create_theme_prefs_rows_table, create_theme_prefs_table, custom_option_default_text,
    custom_option_row_spec, operator_menu_option_row_spec, option_value_text, theme_pref_row_spec,
};
pub use perframe::{
    SONG_LUA_UPDATE_FUNCTION_MAX_SAMPLES, SongLuaPerframeEntry, SongLuaPerframePlayerState,
    SongLuaPerframeSample, active_perframe_entries, actor_perframe_player_state,
    call_perframe_entry, current_perframe_player_states, perframe_boundaries,
    perframe_delta_seconds, perframe_samples, perframe_segment_step, push_perframe_overlay_targets,
    push_perframe_player_target, push_perframe_player_targets, push_perframe_static_targets,
    push_sampled_perframe_targets, read_perframe_entries, relative_player_target,
    tracked_player_tables, unsupported_perframe_info, update_function_end_beat,
    update_function_overlay_eases, update_function_sample_step, update_function_samples,
};
pub use player_options::{
    SONG_LUA_PLAYER_OPTION_CAPABILITIES, SONG_LUA_PLAYER_OPTION_MULTICOL_PREFIXES,
    default_player_option_value, is_player_option_method_name, normalize_player_option_key,
    normalize_player_option_value, parse_player_option_amount, parse_player_speed_option,
    player_option_default_string, player_option_uses_bool, song_lua_speedmod_value,
    split_first_word, strip_player_option_prefix,
};
pub use runtime::{
    compile_song_runtime_delta_values, compile_song_runtime_values, create_song_position_table,
    create_song_runtime_table, note_song_lua_side_effect, read_song_lua_broadcasts,
    record_song_lua_broadcast, set_compile_song_runtime_beat,
    set_compile_song_runtime_delta_values, set_compile_song_runtime_values,
    song_lua_runtime_number, song_lua_side_effect_count,
};
pub use runtime_mod::{
    RuntimeModEaseEntry, RuntimeOverlayCaptureKey, XeroRuntimeModEaseEntry,
    XeroRuntimeOverlayFunctionEntry, extend_runtime_mod_sustains, read_runtime_mod_ease_entry,
    read_runtime_mod_eases, read_xero_runtime_mod_eases_with_overlay_capture,
    read_xero_runtime_mod_entries, record_unsupported_xero_overlay_function_ease,
    runtime_mod_ease_target, runtime_mod_end_value, runtime_mod_entry_players, runtime_mod_key,
    runtime_mod_start_value, runtime_overlay_capture_key, runtime_player_option_ease_target,
};
pub use sl::{create_sl_streams, create_sl_table, init_sl_streams, parse_chart_info};
pub use song_tables::{
    PlayerLuaTables, create_course_table, create_enabled_players_table, create_player_tables,
    create_song_options_table, create_song_table, create_song_util_table, create_songman_table,
    create_steps_table, create_trail_table,
};
pub use tables::{
    create_author_table, create_background_filter_values, create_branch_table,
    create_charman_table, create_credits_table, create_difficulty_table, create_display_bpms_table,
    create_display_table, create_ex_judgment_counts, create_game_table, create_gameman_table,
    create_gameplay_layout, create_hooks_table, create_index_array, create_ini_file_table,
    create_life_record_table, create_memcardman_table, create_network_response_table,
    create_other_player_table, create_player_number_table, create_prefsmgr_table,
    create_profileman_table, create_radar_values_table, create_rage_file_util_table,
    create_range_table, create_screen_system_layer_helpers_table, create_screen_table,
    create_single_value_array, create_song_group_table, create_split_table, create_statsman_table,
    create_string_enum_table, create_style_table, create_theme_table, create_timing_table,
    create_unlockman_table, create_version_parts_table, create_websocket_table,
    deduplicate_lua_table, display_bpms_for_args, format_number_and_suffix, lua_table_to_string,
    map_lua_table, prefsmgr_default_value, rotate_lua_table, scale_value, seconds_to_hhmmss,
    seconds_to_mmss, seconds_to_mmss_ms_ms, seconds_to_mss, seconds_to_mss_ms_ms, set_path_methods,
    set_string_method, stringify_lua_table,
};
pub use theme_colors::{
    DDR_DIFF_COLORS, ITG_DIFF_COLORS, SL_COLORS, SL_DECORATIVE_COLORS, SL_FA_PLUS_COLORS,
    SL_JUDGMENT_COLORS, SONG_LUA_ACTIVE_COLOR_INDEX, blend_color, color_to_hex,
    custom_difficulty_color, install_theme_color_helpers, judgment_line_color, light_color,
    palette_color, parse_color_text, song_lua_difficulty_color, song_lua_difficulty_index,
    song_lua_palette, song_lua_player_color, song_lua_player_dark_color,
    song_lua_player_score_color, stage_color, tone_color,
};
pub use timing::{
    SONG_LUA_TIMING_WINDOW_NAMES, timing_window_arg_index, timing_window_name,
    timing_window_seconds, worst_judgment_from_offsets,
};
pub use top_screen::{
    SONG_LUA_TOP_SCREEN_OPTION_ROWS, option_row_default_text, player_child_proxy_name,
    top_screen_danger_index, top_screen_life_meter_bar_index, top_screen_life_meter_index,
    top_screen_life_meter_name, top_screen_option_row_name, top_screen_option_row_name_at,
    top_screen_player_index, top_screen_player_name, top_screen_score_index, top_screen_score_name,
    top_screen_score_percent_name, top_screen_song_meter_display_index,
    top_screen_step_stats_pane_index, top_screen_steps_display_index, underlay_score_index,
    underlay_score_name,
};
pub use values::{
    SONG_LUA_EASING_NAME_KEY, lua_binary_to_hex, lua_values_equal, player_index_from_value,
    player_number_name, read_boolish, read_easing_name, read_f32, read_i32_value, read_player,
    read_span_mode, read_string, read_u32_value, truthy,
};
pub use version::{
    is_minimum_product_version, is_product_version, song_lua_is_minimum_product_version,
    song_lua_is_product_version, version_args, version_parts,
};

pub const LUA_PLAYERS: usize = 2;
pub const SONG_LUA_DEFAULT_NOTESKIN_NAME: &str = "cel";
pub const SONG_LUA_PRODUCT_FAMILY: &str = "ITGmania";
pub const SONG_LUA_PRODUCT_ID: &str = "ITGmania";
pub const SONG_LUA_PRODUCT_VERSION: &str = "1.2.0";
pub const SONG_LUA_RUNTIME_KEY: &str = "__songlua_compile_song_runtime";
pub const SONG_LUA_RUNTIME_BEAT_KEY: &str = "__songlua_song_beat";
pub const SONG_LUA_RUNTIME_SECONDS_KEY: &str = "__songlua_music_seconds";
pub const SONG_LUA_RUNTIME_DELTA_BEAT_KEY: &str = "__songlua_song_delta_beat";
pub const SONG_LUA_RUNTIME_DELTA_SECONDS_KEY: &str = "__songlua_music_delta_seconds";
pub const SONG_LUA_RUNTIME_BPS_KEY: &str = "__songlua_song_bps";
pub const SONG_LUA_RUNTIME_RATE_KEY: &str = "__songlua_music_rate";
pub const SONG_LUA_SIDE_EFFECT_COUNT_KEY: &str = "__songlua_side_effect_count";
pub const SONG_LUA_BROADCASTS_KEY: &str = "__songlua_broadcast_messages";
pub const SONG_LUA_SOUND_PATHS_KEY: &str = "__songlua_sound_paths";
pub const SONG_LUA_PROBE_METHODS_KEY: &str = "__songlua_probe_methods";
pub const SONG_LUA_PROBE_ACTORS_KEY: &str = "__songlua_probe_actors";
pub const SONG_LUA_PROBE_ACTOR_SET_KEY: &str = "__songlua_probe_actor_set";
pub const SONG_LUA_CAPTURE_ACTORS_KEY: &str = "__songlua_capture_scope_actors";
pub const SONG_LUA_CAPTURE_ACTOR_SET_KEY: &str = "__songlua_capture_scope_actor_set";
pub const SONG_LUA_CAPTURE_SNAPSHOTS_KEY: &str = "__songlua_capture_scope_snapshots";
pub const SONG_LUA_THEME_PATH_PREFIX: &str = "__songlua_theme_path/";
pub const SONG_LUA_THEME_NAME: &str = "Simply Love";
pub const THEME_RECEPTOR_Y_STD: f32 = -125.0;
pub const THEME_RECEPTOR_Y_REV: f32 = 145.0;
pub const SONG_LUA_INITIAL_LIFE: f32 = 0.5;
pub const SONG_LUA_DANGER_LIFE: f32 = 0.2;
pub const SONG_LUA_NOTE_COLUMNS: usize = 4;
pub const SONG_LUA_DOUBLE_NOTE_COLUMNS: usize = 8;
pub const GRAPH_DISPLAY_VALUE_RESOLUTION: usize = 100;
pub const SONG_LUA_SPRITE_STATE_CLEAR: u32 = u32::MAX;
pub const SONG_LUA_EASING_NAMES: &[&str] = &[
    "instant",
    "linear",
    "inQuad",
    "outQuad",
    "inOutQuad",
    "outInQuad",
    "inCubic",
    "outCubic",
    "inOutCubic",
    "outInCubic",
    "inQuart",
    "outQuart",
    "inOutQuart",
    "outInQuart",
    "inQuint",
    "outQuint",
    "inOutQuint",
    "outInQuint",
    "inSine",
    "outSine",
    "inOutSine",
    "outInSine",
    "inExpo",
    "outExpo",
    "inOutExpo",
    "outInExpo",
    "inCirc",
    "outCirc",
    "inOutCirc",
    "outInCirc",
    "inElastic",
    "outElastic",
    "inOutElastic",
    "outInElastic",
    "inBack",
    "outBack",
    "inOutBack",
    "outInBack",
    "inBounce",
    "outBounce",
    "inOutBounce",
    "outInBounce",
];

const SONG_LUA_COLUMN_X: [f32; SONG_LUA_NOTE_COLUMNS] = [-96.0, -32.0, 32.0, 96.0];
const SONG_LUA_DOUBLE_COLUMN_X: [f32; SONG_LUA_DOUBLE_NOTE_COLUMNS] =
    [-224.0, -160.0, -96.0, -32.0, 32.0, 96.0, 160.0, 224.0];
const SONG_LUA_COLUMN_NAMES: [&str; SONG_LUA_NOTE_COLUMNS] = ["Left", "Down", "Up", "Right"];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SongLuaStyleInfo {
    pub name: &'static str,
    pub steps_type: &'static str,
    pub style_type: &'static str,
    pub columns: usize,
    pub width: f32,
    pub x_offsets: &'static [f32],
}

pub fn song_lua_style_info(style_name: &str) -> SongLuaStyleInfo {
    let normalized = style_name
        .trim()
        .to_ascii_lowercase()
        .replace(['_', '-', ' '], "");
    if matches!(
        normalized.as_str(),
        "double" | "dancedouble" | "stepstypedancedouble"
    ) {
        SongLuaStyleInfo {
            name: "double",
            steps_type: "StepsType_Dance_Double",
            style_type: "StyleType_OnePlayerTwoSides",
            columns: SONG_LUA_DOUBLE_NOTE_COLUMNS,
            width: 512.0,
            x_offsets: &SONG_LUA_DOUBLE_COLUMN_X,
        }
    } else if normalized == "versus" {
        SongLuaStyleInfo {
            name: "versus",
            steps_type: "StepsType_Dance_Single",
            style_type: "StyleType_TwoPlayersTwoSides",
            columns: SONG_LUA_NOTE_COLUMNS,
            width: 256.0,
            x_offsets: &SONG_LUA_COLUMN_X,
        }
    } else {
        SongLuaStyleInfo {
            name: "single",
            steps_type: "StepsType_Dance_Single",
            style_type: "StyleType_OnePlayerOneSide",
            columns: SONG_LUA_NOTE_COLUMNS,
            width: 256.0,
            x_offsets: &SONG_LUA_COLUMN_X,
        }
    }
}

#[inline(always)]
pub fn song_lua_style_column_x(style_name: &str, column_index: usize) -> f32 {
    song_lua_style_info(style_name)
        .x_offsets
        .get(column_index)
        .copied()
        .unwrap_or(0.0)
}

#[inline(always)]
pub fn song_lua_style_column_name(column_index: usize) -> &'static str {
    SONG_LUA_COLUMN_NAMES[column_index % SONG_LUA_COLUMN_NAMES.len()]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongLuaDifficulty {
    Beginner,
    Easy,
    Medium,
    Hard,
    Challenge,
    Edit,
}

impl SongLuaDifficulty {
    #[inline(always)]
    pub const fn sm_name(self) -> &'static str {
        match self {
            Self::Beginner => "Difficulty_Beginner",
            Self::Easy => "Difficulty_Easy",
            Self::Medium => "Difficulty_Medium",
            Self::Hard => "Difficulty_Hard",
            Self::Challenge => "Difficulty_Challenge",
            Self::Edit => "Difficulty_Edit",
        }
    }

    #[inline(always)]
    pub const fn meter(self) -> i32 {
        match self {
            Self::Beginner => 1,
            Self::Easy => 4,
            Self::Medium => 7,
            Self::Hard => 10,
            Self::Challenge => 12,
            Self::Edit => 0,
        }
    }

    #[inline(always)]
    pub const fn default_enabled() -> Self {
        Self::Challenge
    }

    #[inline(always)]
    pub fn from_chart_name(difficulty: &str) -> Self {
        if difficulty.eq_ignore_ascii_case("beginner") {
            Self::Beginner
        } else if difficulty.eq_ignore_ascii_case("easy")
            || difficulty.eq_ignore_ascii_case("basic")
        {
            Self::Easy
        } else if difficulty.eq_ignore_ascii_case("medium")
            || difficulty.eq_ignore_ascii_case("standard")
        {
            Self::Medium
        } else if difficulty.eq_ignore_ascii_case("hard")
            || difficulty.eq_ignore_ascii_case("difficult")
        {
            Self::Hard
        } else if difficulty.eq_ignore_ascii_case("edit") {
            Self::Edit
        } else {
            Self::Challenge
        }
    }

    #[inline(always)]
    pub const fn sort_key(self) -> u8 {
        match self {
            Self::Beginner => 0,
            Self::Easy => 1,
            Self::Medium => 2,
            Self::Hard => 3,
            Self::Challenge => 4,
            Self::Edit => 5,
        }
    }
}

pub fn song_lua_difficulty_from_value(value: mlua::Value) -> Option<SongLuaDifficulty> {
    let normalized = read_string(value)?
        .trim()
        .to_ascii_lowercase()
        .replace(['_', '-', ' '], "");
    let raw = normalized.strip_prefix("difficulty").unwrap_or(&normalized);
    match raw {
        "beginner" => Some(SongLuaDifficulty::Beginner),
        "easy" => Some(SongLuaDifficulty::Easy),
        "medium" => Some(SongLuaDifficulty::Medium),
        "hard" => Some(SongLuaDifficulty::Hard),
        "challenge" | "expert" => Some(SongLuaDifficulty::Challenge),
        "edit" => Some(SongLuaDifficulty::Edit),
        _ => None,
    }
}

pub fn song_lua_steps_type_is_dance_single(value: mlua::Value) -> bool {
    let Some(raw) = read_string(value) else {
        return false;
    };
    let normalized = raw.trim().to_ascii_lowercase().replace(['_', '-', ' '], "");
    matches!(
        normalized.as_str(),
        "stepstypedancesingle" | "dancesingle" | "single"
    )
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SongLuaSpeedMod {
    X(f32),
    C(f32),
    M(f32),
    A(f32),
}

impl Default for SongLuaSpeedMod {
    fn default() -> Self {
        Self::X(1.0)
    }
}

pub fn song_lua_speedmod_parts(speedmod: SongLuaSpeedMod) -> (&'static str, f32) {
    match speedmod {
        SongLuaSpeedMod::X(value) => ("X", value),
        SongLuaSpeedMod::C(value) => ("C", value),
        SongLuaSpeedMod::M(value) => ("M", value),
        SongLuaSpeedMod::A(value) => ("A", value),
    }
}

pub fn song_music_rate_value(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        1.0
    }
}

pub fn format_song_options_text(music_rate: f32) -> String {
    let rate = song_music_rate_value(music_rate);
    format!("{rate}xMusic")
}

pub fn display_bpms_text(bpms: [f32; 2], rate: f32) -> String {
    let lower = format_display_bpm(bpms[0], rate);
    if (bpms[0] - bpms[1]).abs() <= f32::EPSILON {
        lower
    } else {
        format!("{lower} - {}", format_display_bpm(bpms[1], rate))
    }
}

fn format_display_bpm(value: f32, rate: f32) -> String {
    let text = if (rate - 1.0).abs() <= f32::EPSILON {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    };
    text.strip_suffix(".0").unwrap_or(&text).to_string()
}

pub fn player_short_name(player: usize) -> &'static str {
    match player {
        0 => "P1",
        1 => "P2",
        _ => unreachable!("song lua only exposes two player numbers"),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaPlayerContext {
    pub enabled: bool,
    pub difficulty: SongLuaDifficulty,
    pub speedmod: SongLuaSpeedMod,
    pub display_bpms: [f32; 2],
    pub noteskin_name: String,
    pub screen_x: f32,
    pub screen_y: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct SongLuaActorMultiVertexPoint {
    pub pos: [f32; 2],
    pub color: [f32; 4],
    pub uv: [f32; 2],
}

impl Default for SongLuaPlayerContext {
    fn default() -> Self {
        Self {
            enabled: true,
            difficulty: SongLuaDifficulty::default_enabled(),
            speedmod: SongLuaSpeedMod::default(),
            display_bpms: [60.0, 60.0],
            noteskin_name: SONG_LUA_DEFAULT_NOTESKIN_NAME.to_string(),
            screen_x: 320.0,
            screen_y: 240.0,
        }
    }
}

pub fn easiest_steps_difficulty(
    players: &[SongLuaPlayerContext; LUA_PLAYERS],
) -> Option<SongLuaDifficulty> {
    players
        .iter()
        .filter(|player| player.enabled)
        .map(|player| player.difficulty)
        .min_by_key(|difficulty| difficulty.sort_key())
}

pub fn song_lua_human_player_count(context: &SongLuaCompileContext) -> usize {
    context
        .players
        .iter()
        .filter(|player| player.enabled)
        .count()
}

pub fn graph_display_body_size(human_player_count: usize) -> [f32; 2] {
    [
        if human_player_count == 1 {
            610.0
        } else {
            300.0
        },
        64.0,
    ]
}

pub fn theme_metric_number(group: &str, name: &str) -> Option<f32> {
    theme_metric_number_for_human_players(group, name, LUA_PLAYERS)
}

pub fn theme_metric_number_for_human_players(
    group: &str,
    name: &str,
    human_player_count: usize,
) -> Option<f32> {
    theme_metric_number_for_screen(group, name, human_player_count, 480.0)
}

pub fn theme_metric_number_for_screen(
    group: &str,
    name: &str,
    human_player_count: usize,
    screen_height: f32,
) -> Option<f32> {
    if group.eq_ignore_ascii_case("Player") {
        if name.eq_ignore_ascii_case("ReceptorArrowsYStandard") {
            return Some(THEME_RECEPTOR_Y_STD);
        }
        if name.eq_ignore_ascii_case("ReceptorArrowsYReverse") {
            return Some(THEME_RECEPTOR_Y_REV);
        }
        if name.eq_ignore_ascii_case("DrawDistanceBeforeTargetsPixels") {
            return Some(screen_height.max(1.0) * 1.5);
        }
        if name.eq_ignore_ascii_case("DrawDistanceAfterTargetsPixels") {
            return Some(-130.0);
        }
    }
    if group.eq_ignore_ascii_case("Combo") && name.eq_ignore_ascii_case("ShowComboAt") {
        return Some(4.0);
    }
    if group.eq_ignore_ascii_case("GraphDisplay") {
        if name.eq_ignore_ascii_case("BodyWidth") {
            return Some(graph_display_body_size(human_player_count)[0]);
        }
        if name.eq_ignore_ascii_case("BodyHeight") {
            return Some(graph_display_body_size(human_player_count)[1]);
        }
    }
    if group.eq_ignore_ascii_case("LifeMeterBar") && name.eq_ignore_ascii_case("InitialValue") {
        return Some(SONG_LUA_INITIAL_LIFE);
    }
    if group.eq_ignore_ascii_case("MusicWheel") && name.eq_ignore_ascii_case("NumWheelItems") {
        return Some(15.0);
    }
    if group.eq_ignore_ascii_case("PlayerStageStats")
        && name.eq_ignore_ascii_case("NumGradeTiersUsed")
    {
        return Some(7.0);
    }
    None
}

const SONG_LUA_SCREEN_PLAYER_OPTIONS_LINE_NAMES: &str = "SpeedModType,SpeedMod,Mini,Perspective,NoteSkinSL,NoteSkinVariant,Judgment,ComboFont,HoldJudgment,BackgroundFilter,NoteFieldOffsetX,NoteFieldOffsetY,VisualDelay,MusicRate,Stepchart,ScreenAfterPlayerOptions";
const SONG_LUA_SCREEN_PLAYER_OPTIONS2_LINE_NAMES: &str = "Turn,Scroll,Hide,LifeMeterType,DataVisualizations,TargetScore,ActionOnMissedTarget,GameplayExtras,GameplayExtrasB,GameplayExtrasC,TiltMultiplier,ErrorBar,ErrorBarTrim,ErrorBarOptions,MeasureCounter,MeasureCounterOptions,MeasureLines,TimingWindowOptions,FaPlus,ScreenAfterPlayerOptions2";
const SONG_LUA_SCREEN_PLAYER_OPTIONS3_LINE_NAMES: &str =
    "Insert,Remove,Holds,11,12,13,Attacks,Characters,HideLightType,ScreenAfterPlayerOptions3";
const SONG_LUA_SCREEN_ATTACK_MENU_LINE_NAMES: &str =
    "SpeedModType,SpeedMod,Mini,Perspective,NoteSkin,MusicRate,Assist,ShowBGChangesPlay";
const SONG_LUA_SCREEN_OPTIONS_SERVICE_LINE_NAMES: &str = "SystemOptions,MapControllers,TestInput,InputOptions,GraphicsSoundOptions,VisualOptions,ArcadeOptions,Bookkeeping,AdvancedOptions,MenuTimerOptions,USBProfileOptions,OptionsManageProfiles,ThemeOptions,TournamentModeOptions,GrooveStatsOptions,StepManiaCredits,Reload";
const SONG_LUA_SCREEN_SYSTEM_OPTIONS_LINE_NAMES: &str =
    "Game,Theme,Language,Announcer,DefaultNoteSkin,EditorNoteSkin";
const SONG_LUA_SCREEN_INPUT_OPTIONS_LINE_NAMES: &str =
    "AutoMap,OnlyDedicatedMenu,OptionsNav,Debounce,ThreeKey,AxisFix";
const SONG_LUA_SCREEN_GRAPHICS_SOUND_OPTIONS_LINE_NAMES: &str = "VideoRenderer,DisplayMode,DisplayAspectRatio,DisplayResolution,RefreshRate,FullscreenType,DisplayColorDepth,HighResolutionTextures,MaxTextureResolution";
const SONG_LUA_SCREEN_VISUAL_OPTIONS_LINE_NAMES: &str =
    "AppearanceOptions,Set BG Fit Mode,Overscan Correction,CRT Test Patterns";
const SONG_LUA_SCREEN_APPEARANCE_OPTIONS_LINE_NAMES: &str = "Center1Player,ShowBanners,BGBrightness,RandomBackgroundMode,NumBackgrounds,ShowLyrics,ShowNativeLanguage,ShowDancingCharacters";
const SONG_LUA_SCREEN_ARCADE_OPTIONS_LINE_NAMES: &str = "Event,Coin,CoinsPerCredit,MaxNumCredits,ResetCoinsAtStartup,Premium,SongsPerPlay,Long Time,Marathon Time";
const SONG_LUA_SCREEN_ADVANCED_OPTIONS_LINE_NAMES: &str =
    "DefaultFailType,TimingWindowScale,LifeDifficulty,HiddenSongs,EasterEggs,AllowExtraStage";
const SONG_LUA_SCREEN_THEME_OPTIONS_LINE_NAMES: &str =
    "VisualStyle,MusicWheelSpeed,MusicWheelStyle,AutoStyle,DefaultGameMode,CasualMaxMeter";
const SONG_LUA_SCREEN_MENU_TIMER_OPTIONS_LINE_NAMES: &str =
    "MenuTimer,ScreenSelectMusicMenuTimer,ScreenPlayerOptionsMenuTimer,ScreenEvaluationMenuTimer";
const SONG_LUA_SCREEN_USB_PROFILE_OPTIONS_LINE_NAMES: &str = "MemoryCards,CustomSongs,MaxCount,CustomSongsLoadTimeout,CustomSongsMaxSeconds,CustomSongsMaxMegabytes";
const SONG_LUA_SCREEN_TOURNAMENT_MODE_OPTIONS_LINE_NAMES: &str =
    "EnableTournamentMode,ScoringSystem,StepStats,EnforceNoCmod";
const SONG_LUA_SCREEN_GROOVE_STATS_OPTIONS_LINE_NAMES: &str =
    "EnableGrooveStats,AutoDownloadUnlocks,SeparateUnlocksByPlayer,QRLogin,EnableOnlineLobbies";

pub fn theme_metric_value_for_human_players(
    lua: &mlua::Lua,
    group: &str,
    name: &str,
    human_player_count: usize,
    screen_height: f32,
) -> mlua::Result<mlua::Value> {
    if let Some(value) =
        theme_metric_number_for_screen(group, name, human_player_count, screen_height)
    {
        return Ok(mlua::Value::Number(value as f64));
    }
    if let Some(value) = theme_metric_string(group, name) {
        return Ok(mlua::Value::String(lua.create_string(&value)?));
    }
    if group.eq_ignore_ascii_case("Common") && name.eq_ignore_ascii_case("DefaultNoteSkinName") {
        return Ok(mlua::Value::String(lua.create_string("default")?));
    }
    if name.eq_ignore_ascii_case("Class") {
        return Ok(mlua::Value::String(lua.create_string(group)?));
    }
    if group.eq_ignore_ascii_case("Common") && name.eq_ignore_ascii_case("AutoSetStyle")
        || group.eq_ignore_ascii_case("ScreenHeartEntry")
            && name.eq_ignore_ascii_case("HeartEntryEnabled")
    {
        return Ok(mlua::Value::Boolean(false));
    }
    Ok(mlua::Value::Nil)
}

fn theme_metric_string(group: &str, name: &str) -> Option<String> {
    if name.eq_ignore_ascii_case("LineNames") {
        return theme_line_names(group).map(str::to_string);
    }
    if name.eq_ignore_ascii_case("Fallback") {
        return theme_screen_fallback(group).map(str::to_string);
    }
    if let Some(row) = name.strip_prefix("Line") {
        if let Some(metric) = theme_explicit_line_metric(group, row) {
            return Some(metric.to_string());
        }
        if group.eq_ignore_ascii_case("ScreenOptionsService") {
            return Some(format!("gamecommand;screen,Screen{row};name,{row}"));
        }
        if theme_screen_fallback(group).is_some() && !row.trim().is_empty() {
            return Some(format!("conf,{row}"));
        }
    }
    None
}

fn theme_explicit_line_metric(group: &str, row: &str) -> Option<&'static str> {
    if group.eq_ignore_ascii_case("ScreenGraphicsSoundOptions") {
        return match row {
            "VideoRenderer" => Some("lua,OperatorMenuOptionRows.VideoRenderer()"),
            "DisplayAspectRatio" => Some("lua,ConfAspectRatio()"),
            "DisplayResolution" => Some("lua,ConfDisplayResolution()"),
            "DisplayMode" => Some("lua,ConfDisplayMode()"),
            "FullscreenType" => Some("lua,ConfFullscreenType()"),
            "GlobalOffsetSeconds" => Some("lua,OperatorMenuOptionRows.GlobalOffsetSeconds()"),
            "VisualDelaySeconds" => Some("lua,OperatorMenuOptionRows.VisualDelaySeconds()"),
            _ => None,
        };
    }
    if group.eq_ignore_ascii_case("ScreenSystemOptions") {
        return match row {
            "Theme" => Some("lua,OperatorMenuOptionRows.Theme()"),
            "EditorNoteSkin" => Some("lua,OperatorMenuOptionRows.EditorNoteskin()"),
            _ => None,
        };
    }
    None
}

fn theme_line_names(group: &str) -> Option<&'static str> {
    if group.eq_ignore_ascii_case("ScreenPlayerOptions") {
        Some(SONG_LUA_SCREEN_PLAYER_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenPlayerOptions2") {
        Some(SONG_LUA_SCREEN_PLAYER_OPTIONS2_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenPlayerOptions3") {
        Some(SONG_LUA_SCREEN_PLAYER_OPTIONS3_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenAttackMenu") {
        Some(SONG_LUA_SCREEN_ATTACK_MENU_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenOptionsService") {
        Some(SONG_LUA_SCREEN_OPTIONS_SERVICE_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenSystemOptions") {
        Some(SONG_LUA_SCREEN_SYSTEM_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenInputOptions") {
        Some(SONG_LUA_SCREEN_INPUT_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenGraphicsSoundOptions") {
        Some(SONG_LUA_SCREEN_GRAPHICS_SOUND_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenVisualOptions") {
        Some(SONG_LUA_SCREEN_VISUAL_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenAppearanceOptions") {
        Some(SONG_LUA_SCREEN_APPEARANCE_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenArcadeOptions") {
        Some(SONG_LUA_SCREEN_ARCADE_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenAdvancedOptions") {
        Some(SONG_LUA_SCREEN_ADVANCED_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenThemeOptions") {
        Some(SONG_LUA_SCREEN_THEME_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenMenuTimerOptions") {
        Some(SONG_LUA_SCREEN_MENU_TIMER_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenUSBProfileOptions") {
        Some(SONG_LUA_SCREEN_USB_PROFILE_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenTournamentModeOptions") {
        Some(SONG_LUA_SCREEN_TOURNAMENT_MODE_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenGrooveStatsOptions") {
        Some(SONG_LUA_SCREEN_GROOVE_STATS_OPTIONS_LINE_NAMES)
    } else {
        None
    }
}

fn theme_screen_fallback(group: &str) -> Option<&'static str> {
    let lower = group.to_ascii_lowercase();
    match lower.as_str() {
        "screenoptionsservice" => Some("ScreenOptionsSimple"),
        "screenvisualoptions" => Some("ScreenOptionsServiceSub"),
        "screensystemoptions"
        | "screeninputoptions"
        | "screengraphicssoundoptions"
        | "screenappearanceoptions"
        | "screenarcadeoptions"
        | "screenadvancedoptions"
        | "screenthemeoptions"
        | "screenmenutimeroptions"
        | "screenusbprofileoptions"
        | "screentournamentmodeoptions"
        | "screengroovestatsoptions" => Some("ScreenOptionsServiceChild"),
        _ => None,
    }
}

pub fn theme_metric_bool(value: mlua::Value) -> bool {
    match value {
        mlua::Value::Boolean(value) => value,
        mlua::Value::Integer(value) => value != 0,
        mlua::Value::Number(value) => value != 0.0,
        mlua::Value::String(value) => !value.to_str().is_ok_and(|text| text.is_empty()),
        _ => false,
    }
}

pub fn theme_metric_names(group: &str) -> Vec<String> {
    let mut names = Vec::new();
    if theme_line_names(group).is_some() {
        names.push("LineNames".to_string());
    }
    if theme_screen_fallback(group).is_some() {
        names.push("Fallback".to_string());
    }
    if let Some(lines) = theme_line_names(group) {
        names.extend(
            lines
                .split(',')
                .filter(|line| !line.trim().is_empty())
                .map(|line| format!("Line{}", line.trim())),
        );
    }
    if group.eq_ignore_ascii_case("Player") {
        names.extend(
            [
                "ReceptorArrowsYStandard",
                "ReceptorArrowsYReverse",
                "DrawDistanceBeforeTargetsPixels",
                "DrawDistanceAfterTargetsPixels",
            ]
            .into_iter()
            .map(str::to_string),
        );
    } else if group.eq_ignore_ascii_case("Common") {
        names.extend(
            ["DefaultNoteSkinName", "AutoSetStyle"]
                .into_iter()
                .map(str::to_string),
        );
    } else if group.eq_ignore_ascii_case("Combo") {
        names.push("ShowComboAt".to_string());
    } else if group.eq_ignore_ascii_case("GraphDisplay") {
        names.extend(["BodyWidth", "BodyHeight"].into_iter().map(str::to_string));
    } else if group.eq_ignore_ascii_case("LifeMeterBar") {
        names.push("InitialValue".to_string());
    } else if group.eq_ignore_ascii_case("MusicWheel") {
        names.push("NumWheelItems".to_string());
    } else if group.eq_ignore_ascii_case("PlayerStageStats") {
        names.push("NumGradeTiersUsed".to_string());
    } else if group.eq_ignore_ascii_case("ScreenHeartEntry") {
        names.push("HeartEntryEnabled".to_string());
    }
    names.sort_unstable();
    names.dedup();
    names
}

pub fn theme_string_names(section: &str) -> Vec<String> {
    if section.eq_ignore_ascii_case("Difficulty")
        || section.eq_ignore_ascii_case("CustomDifficulty")
    {
        return [
            SongLuaDifficulty::Beginner,
            SongLuaDifficulty::Easy,
            SongLuaDifficulty::Medium,
            SongLuaDifficulty::Hard,
            SongLuaDifficulty::Challenge,
            SongLuaDifficulty::Edit,
        ]
        .into_iter()
        .map(|difficulty| difficulty.sm_name().to_string())
        .collect();
    }
    if matches!(
        section,
        "OptionTitles"
            | "OptionNames"
            | "ThemePrefs"
            | "SLPlayerOptions"
            | "ScreenSelectPlayMode"
            | "ScreenSelectStyle"
            | "GameButton"
            | "TapNoteScore"
            | "TapNoteScoreFA+"
            | "HoldNoteScore"
            | "Stage"
            | "Months"
    ) {
        return [
            "Yes",
            "No",
            "Cancel",
            "DisplayMode",
            "MusicRate",
            "SpeedMod",
            "NoteSkin",
            "Difficulty_Hard",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
    }
    Vec::new()
}

pub fn theme_string(section: &str, name: &str) -> String {
    if section.eq_ignore_ascii_case("Difficulty")
        || section.eq_ignore_ascii_case("CustomDifficulty")
    {
        return name.trim_start_matches("Difficulty_").to_string();
    }
    if matches!(
        section,
        "OptionTitles"
            | "OptionNames"
            | "ThemePrefs"
            | "SLPlayerOptions"
            | "ScreenSelectPlayMode"
            | "ScreenSelectStyle"
            | "GameButton"
            | "TapNoteScore"
            | "TapNoteScoreFA+"
            | "HoldNoteScore"
            | "Stage"
            | "Months"
    ) {
        return name.replace('_', " ");
    }
    match name {
        "Yes" => "Yes".to_string(),
        "No" => "No".to_string(),
        "Cancel" => "Cancel".to_string(),
        _ => name.to_string(),
    }
}

pub fn theme_has_string(section: &str, name: &str) -> bool {
    section.eq_ignore_ascii_case("Difficulty")
        || section.eq_ignore_ascii_case("CustomDifficulty")
        || matches!(
            section,
            "OptionTitles"
                | "OptionNames"
                | "ThemePrefs"
                | "SLPlayerOptions"
                | "ScreenSelectPlayMode"
                | "ScreenSelectStyle"
                | "GameButton"
                | "TapNoteScore"
                | "TapNoteScoreFA+"
                | "HoldNoteScore"
                | "Stage"
                | "Months"
        )
        || matches!(name, "Yes" | "No" | "Cancel")
}

pub fn song_lua_arch_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "Mac OS X"
    } else if cfg!(target_os = "linux") {
        "Linux"
    } else if cfg!(target_os = "freebsd") {
        "FreeBSD"
    } else {
        "Unknown"
    }
}

pub fn custom_multi_modifier_key(option_name: &str, choice: &str) -> String {
    if option_name.eq_ignore_ascii_case("Hide") {
        format!("Hide{choice}")
    } else {
        choice.to_string()
    }
}

pub fn theme_pref_default(lua: &mlua::Lua, name: &str) -> mlua::Result<mlua::Value> {
    let lower = name.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "casualmaxmeter"
            | "numberofcontinuesallowed"
            | "screenselectmusicmenutimer"
            | "screenselectmusiccasualmenutimer"
            | "screenplayeroptionsmenutimer"
            | "screenevaluationmenutimer"
            | "screenevaluationnonstopmenutimer"
            | "screenevaluationsummarymenutimer"
            | "screennameentrymenutimer"
            | "screengroovestatsloginmenutimer"
            | "simplylovecolor"
            | "nice"
    ) {
        return Ok(mlua::Value::Integer(match lower.as_str() {
            "casualmaxmeter" => 12,
            "simplylovecolor" => 1,
            _ => 0,
        }));
    }
    if matches!(
        lower.as_str(),
        "visualstyle"
            | "lastactiveevent"
            | "musicwheelstyle"
            | "themefont"
            | "defaultgamemode"
            | "autostyle"
            | "songselectbg"
            | "resultsbg"
            | "scoringsystem"
            | "stepstats"
            | "editmodelastseensong"
            | "editmodelastseendifficulty"
            | "editmodelastseenstepstype"
            | "editmodelastseenstyletype"
    ) {
        let value = match lower.as_str() {
            "themefont" => "Common",
            "defaultgamemode" => "Dance",
            "songselectbg" | "resultsbg" => "Off",
            "musicwheelstyle" => "Default",
            "autostyle" => "Default",
            _ => "",
        };
        return Ok(mlua::Value::String(lua.create_string(value)?));
    }
    Ok(mlua::Value::Boolean(matches!(
        lower.as_str(),
        "useimagecache"
    )))
}

pub type SongLuaNoteskinPathResolver = fn(&str, &str, &str) -> Option<PathBuf>;
pub type SongLuaNoteskinMetricResolver = fn(&str, &str, &str) -> Option<String>;
pub type SongLuaNoteskinMetricFResolver = fn(&str, &str, &str) -> Option<f32>;
pub type SongLuaNoteskinMetricBResolver = fn(&str, &str, &str) -> Option<bool>;
pub type SongLuaNoteskinExistsResolver = fn(&str) -> bool;
pub type SongLuaNoteskinNamesResolver = fn() -> Vec<String>;

#[derive(Clone, Copy)]
pub struct SongLuaNoteskinResolver {
    pub resolve_path: SongLuaNoteskinPathResolver,
    pub metric: SongLuaNoteskinMetricResolver,
    pub metric_f: SongLuaNoteskinMetricFResolver,
    pub metric_b: SongLuaNoteskinMetricBResolver,
    pub exists: SongLuaNoteskinExistsResolver,
    pub names: SongLuaNoteskinNamesResolver,
}

fn missing_noteskin_path(_: &str, _: &str, _: &str) -> Option<PathBuf> {
    None
}

fn missing_noteskin_metric(_: &str, _: &str, _: &str) -> Option<String> {
    None
}

fn missing_noteskin_metric_f(_: &str, _: &str, _: &str) -> Option<f32> {
    None
}

fn missing_noteskin_metric_b(_: &str, _: &str, _: &str) -> Option<bool> {
    None
}

fn missing_noteskin_exists(_: &str) -> bool {
    false
}

fn missing_noteskin_names() -> Vec<String> {
    Vec::new()
}

impl Default for SongLuaNoteskinResolver {
    fn default() -> Self {
        Self {
            resolve_path: missing_noteskin_path,
            metric: missing_noteskin_metric,
            metric_f: missing_noteskin_metric_f,
            metric_b: missing_noteskin_metric_b,
            exists: missing_noteskin_exists,
            names: missing_noteskin_names,
        }
    }
}

impl SongLuaNoteskinResolver {
    #[inline(always)]
    pub fn resolve_path(self, skin: &str, button: &str, element: &str) -> Option<PathBuf> {
        (self.resolve_path)(skin, button, element)
    }

    #[inline(always)]
    pub fn path_string(self, skin: &str, button: &str, element: &str) -> String {
        self.resolve_path(skin, button, element)
            .map(|path| file_path_string(path.as_path()))
            .unwrap_or_default()
    }

    #[inline(always)]
    pub fn metric(self, skin: &str, element: &str, value: &str) -> Option<String> {
        (self.metric)(skin, element, value)
    }

    #[inline(always)]
    pub fn metric_f(self, skin: &str, element: &str, value: &str) -> Option<f32> {
        (self.metric_f)(skin, element, value)
    }

    pub fn metric_i(self, skin: &str, element: &str, value: &str) -> i64 {
        let Some(metric) = self.metric(skin, element, value) else {
            return 0;
        };
        let metric = metric.trim();
        metric
            .parse::<i64>()
            .ok()
            .or_else(|| {
                metric
                    .parse::<f64>()
                    .ok()
                    .filter(|value| value.is_finite())
                    .map(|value| value.round().clamp(i64::MIN as f64, i64::MAX as f64) as i64)
            })
            .unwrap_or(0)
    }

    #[inline(always)]
    pub fn metric_b(self, skin: &str, element: &str, value: &str) -> Option<bool> {
        (self.metric_b)(skin, element, value)
    }

    #[inline(always)]
    pub fn exists(self, skin: &str) -> bool {
        (self.exists)(skin)
    }

    #[inline(always)]
    pub fn names(self) -> Vec<String> {
        (self.names)()
    }
}

#[derive(Debug, Clone)]
pub struct SongLuaCompileContext {
    pub song_dir: PathBuf,
    pub main_title: String,
    pub song_display_bpms: [f32; 2],
    pub song_music_rate: f32,
    pub music_length_seconds: f32,
    pub style_name: String,
    pub global_offset_seconds: f32,
    pub screen_width: f32,
    pub screen_height: f32,
    pub players: [SongLuaPlayerContext; LUA_PLAYERS],
    pub confusion_offset_available: bool,
    pub confusion_available: bool,
    pub amod_available: bool,
}

impl SongLuaCompileContext {
    pub fn new(song_dir: impl Into<PathBuf>, main_title: impl Into<String>) -> Self {
        Self {
            song_dir: song_dir.into(),
            main_title: main_title.into(),
            song_display_bpms: [60.0, 60.0],
            song_music_rate: 1.0,
            music_length_seconds: 0.0,
            style_name: "single".to_string(),
            global_offset_seconds: 0.0,
            screen_width: 640.0,
            screen_height: 480.0,
            players: std::array::from_fn(|_| SongLuaPlayerContext::default()),
            confusion_offset_available: true,
            confusion_available: true,
            amod_available: true,
        }
    }
}

pub fn song_lua_default_noteskin_name(context: &SongLuaCompileContext) -> String {
    context
        .players
        .iter()
        .find(|player| player.enabled)
        .map(|player| player.noteskin_name.clone())
        .or_else(|| {
            context
                .players
                .first()
                .map(|player| player.noteskin_name.clone())
        })
        .unwrap_or_else(|| SONG_LUA_DEFAULT_NOTESKIN_NAME.to_string())
}

#[inline(always)]
pub fn song_display_bps(context: &SongLuaCompileContext) -> f32 {
    (context.song_display_bpms[0].max(context.song_display_bpms[1]) / 60.0).max(f32::EPSILON)
}

#[inline(always)]
pub fn song_music_rate(context: &SongLuaCompileContext) -> f32 {
    song_music_rate_value(context.song_music_rate)
}

#[inline(always)]
pub fn song_elapsed_seconds_for_beat(beat: f32, song_bps: f32, music_rate: f32) -> f32 {
    beat / (song_bps.max(f32::EPSILON) * music_rate.max(f32::EPSILON))
}

#[inline(always)]
pub fn mod_window_cmp(left: &SongLuaModWindow, right: &SongLuaModWindow) -> std::cmp::Ordering {
    left.start
        .total_cmp(&right.start)
        .then_with(|| left.limit.total_cmp(&right.limit))
        .then_with(|| left.mods.cmp(&right.mods))
}

#[inline(always)]
pub fn ease_window_cmp(left: &SongLuaEaseWindow, right: &SongLuaEaseWindow) -> std::cmp::Ordering {
    left.start
        .total_cmp(&right.start)
        .then_with(|| left.limit.total_cmp(&right.limit))
}

#[inline(always)]
pub fn message_event_cmp(
    left: &SongLuaMessageEvent,
    right: &SongLuaMessageEvent,
) -> std::cmp::Ordering {
    left.beat.total_cmp(&right.beat)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongLuaTimeUnit {
    Beat,
    Second,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongLuaSpanMode {
    Len,
    End,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SongLuaEaseTarget {
    Mod(String),
    PlayerX,
    PlayerY,
    PlayerZ,
    PlayerRotationX,
    PlayerRotationZ,
    PlayerRotationY,
    PlayerSkewX,
    PlayerSkewY,
    PlayerZoom,
    PlayerZoomX,
    PlayerZoomY,
    PlayerZoomZ,
    Function,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaModWindow {
    pub unit: SongLuaTimeUnit,
    pub start: f32,
    pub limit: f32,
    pub span_mode: SongLuaSpanMode,
    pub mods: String,
    pub player: Option<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaEaseWindow {
    pub unit: SongLuaTimeUnit,
    pub start: f32,
    pub limit: f32,
    pub span_mode: SongLuaSpanMode,
    pub from: f32,
    pub to: f32,
    pub target: SongLuaEaseTarget,
    pub easing: Option<String>,
    pub player: Option<u8>,
    pub sustain: Option<f32>,
    pub opt1: Option<f32>,
    pub opt2: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaMessageEvent {
    pub beat: f32,
    pub message: String,
    pub persists: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SongLuaCompileInfo {
    pub unsupported_perframes: usize,
    pub unsupported_function_eases: usize,
    pub unsupported_function_actions: usize,
    pub unsupported_perframe_captures: Vec<String>,
    pub unsupported_function_ease_captures: Vec<String>,
    pub unsupported_function_action_captures: Vec<String>,
    pub skipped_message_command_captures: Vec<String>,
}

pub fn push_unique_compile_detail(out: &mut Vec<String>, detail: String) {
    if !out.contains(&detail) {
        out.push(detail);
    }
}

pub fn record_unsupported_function_ease_capture(
    info: &mut SongLuaCompileInfo,
    unit: SongLuaTimeUnit,
    start: f32,
    limit: f32,
    span_mode: SongLuaSpanMode,
    from: f32,
    to: f32,
    easing: &Option<String>,
    probe_methods: &[String],
) -> String {
    info.unsupported_function_eases += 1;
    let detail = format!(
        "function ease unit={unit:?} start={start:.3} limit={limit:.3} \
         span={span_mode:?} from={from:.3} to={to:.3} easing={easing:?} \
         probe_methods={probe_methods:?}"
    );
    push_unique_compile_detail(&mut info.unsupported_function_ease_captures, detail.clone());
    detail
}

pub fn record_unsupported_function_action_capture(
    info: &mut SongLuaCompileInfo,
    beat: f32,
    persists: bool,
) -> String {
    info.unsupported_function_actions += 1;
    let detail = format!("function action beat={beat:.3} persists={persists}");
    push_unique_compile_detail(
        &mut info.unsupported_function_action_captures,
        detail.clone(),
    );
    detail
}

pub fn merge_compile_info(out: &mut SongLuaCompileInfo, info: SongLuaCompileInfo) {
    out.unsupported_perframes += info.unsupported_perframes;
    out.unsupported_function_eases += info.unsupported_function_eases;
    out.unsupported_function_actions += info.unsupported_function_actions;
    for detail in info.unsupported_perframe_captures {
        push_unique_compile_detail(&mut out.unsupported_perframe_captures, detail);
    }
    for detail in info.unsupported_function_ease_captures {
        push_unique_compile_detail(&mut out.unsupported_function_ease_captures, detail);
    }
    for detail in info.unsupported_function_action_captures {
        push_unique_compile_detail(&mut out.unsupported_function_action_captures, detail);
    }
    for detail in info.skipped_message_command_captures {
        push_unique_compile_detail(&mut out.skipped_message_command_captures, detail);
    }
}

#[derive(Debug, Clone)]
pub struct CompiledSongLua<OverlayActor> {
    pub entry_path: PathBuf,
    pub screen_width: f32,
    pub screen_height: f32,
    pub beat_mods: Vec<SongLuaModWindow>,
    pub time_mods: Vec<SongLuaModWindow>,
    pub eases: Vec<SongLuaEaseWindow>,
    pub messages: Vec<SongLuaMessageEvent>,
    pub sound_paths: Vec<PathBuf>,
    pub overlays: Vec<OverlayActor>,
    pub overlay_eases: Vec<SongLuaOverlayEase>,
    pub player_actors: [SongLuaCapturedActor; LUA_PLAYERS],
    pub song_foreground: SongLuaCapturedActor,
    pub hidden_players: [bool; LUA_PLAYERS],
    pub note_hides: Vec<SongLuaNoteHideWindow>,
    pub column_offsets: Vec<SongLuaColumnOffsetWindow>,
    pub info: SongLuaCompileInfo,
}

impl<OverlayActor> Default for CompiledSongLua<OverlayActor> {
    fn default() -> Self {
        Self {
            entry_path: PathBuf::new(),
            screen_width: 0.0,
            screen_height: 0.0,
            beat_mods: Vec::new(),
            time_mods: Vec::new(),
            eases: Vec::new(),
            messages: Vec::new(),
            sound_paths: Vec::new(),
            overlays: Vec::new(),
            overlay_eases: Vec::new(),
            player_actors: std::array::from_fn(|_| SongLuaCapturedActor::default()),
            song_foreground: SongLuaCapturedActor::default(),
            hidden_players: [false; LUA_PLAYERS],
            note_hides: Vec::new(),
            column_offsets: Vec::new(),
            info: SongLuaCompileInfo::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongLuaOverlayBlendMode {
    Alpha,
    Add,
    Multiply,
    Subtract,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongLuaTextGlowMode {
    Inner,
    Stroke,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongLuaProxyTarget {
    Player { player_index: usize },
    NoteField { player_index: usize },
    Judgment { player_index: usize },
    Combo { player_index: usize },
    Underlay,
    Overlay,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SongLuaOverlayMeshVertex {
    pub pos: [f32; 2],
    pub color: [f32; 4],
    pub uv: [f32; 2],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SongLuaOverlayModelDraw {
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    pub zoom: [f32; 3],
    pub tint: [f32; 4],
    pub vert_align: f32,
    pub blend_add: bool,
    pub visible: bool,
}

#[derive(Debug, Clone)]
pub struct SongLuaOverlayModelLayer<Vertex> {
    pub texture_key: Arc<str>,
    pub vertices: Arc<[Vertex]>,
    pub model_size: [f32; 2],
    pub uv_scale: [f32; 2],
    pub uv_offset: [f32; 2],
    pub uv_tex_shift: [f32; 2],
    pub uv_velocity: [f32; 2],
    pub uv_cycle_seconds: Option<f32>,
    pub draw: SongLuaOverlayModelDraw,
}

#[derive(Debug, Clone)]
pub enum SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute> {
    Actor,
    ActorFrame,
    ActorFrameTexture,
    ActorProxy {
        target: SongLuaProxyTarget,
    },
    AftSprite {
        capture_name: String,
    },
    Sprite {
        texture_path: PathBuf,
        texture_key: Arc<str>,
    },
    Sound {
        sound_path: PathBuf,
    },
    BitmapText {
        font_name: &'static str,
        font_path: PathBuf,
        text: Arc<str>,
        stroke_color: Option<[f32; 4]>,
        attributes: Arc<[TextAttribute]>,
    },
    ActorMultiVertex {
        vertices: Arc<[SongLuaOverlayMeshVertex]>,
        texture_path: Option<PathBuf>,
        texture_key: Option<Arc<str>>,
    },
    Model {
        layers: Arc<[SongLuaOverlayModelLayer<ModelVertex>]>,
    },
    NoteskinActor {
        slots: Arc<[NoteskinSlot]>,
    },
    SongMeterDisplay {
        stream_width: f32,
        stream_state: SongLuaOverlayState,
        music_length_seconds: f32,
    },
    GraphDisplay {
        size: [f32; 2],
        body_values: Arc<[f32]>,
        body_state: SongLuaOverlayState,
        line_state: SongLuaOverlayState,
    },
    Quad,
}

pub fn parse_overlay_blend_mode(raw: &str) -> Option<SongLuaOverlayBlendMode> {
    if raw.eq_ignore_ascii_case("add") || raw.eq_ignore_ascii_case("blendmode_add") {
        Some(SongLuaOverlayBlendMode::Add)
    } else if raw.eq_ignore_ascii_case("multiply") || raw.eq_ignore_ascii_case("blendmode_multiply")
    {
        Some(SongLuaOverlayBlendMode::Multiply)
    } else if raw.eq_ignore_ascii_case("subtract") || raw.eq_ignore_ascii_case("blendmode_subtract")
    {
        Some(SongLuaOverlayBlendMode::Subtract)
    } else if raw.eq_ignore_ascii_case("alpha")
        || raw.eq_ignore_ascii_case("normal")
        || raw.eq_ignore_ascii_case("blendmode_normal")
    {
        Some(SongLuaOverlayBlendMode::Alpha)
    } else {
        None
    }
}

pub fn parse_overlay_effect_mode(raw: &str) -> Option<EffectMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "none" => Some(EffectMode::None),
        "diffuseramp" => Some(EffectMode::DiffuseRamp),
        "diffuseshift" => Some(EffectMode::DiffuseShift),
        "glowshift" => Some(EffectMode::GlowShift),
        "pulse" => Some(EffectMode::Pulse),
        "bob" => Some(EffectMode::Bob),
        "bounce" => Some(EffectMode::Bounce),
        "wag" => Some(EffectMode::Wag),
        "spin" => Some(EffectMode::Spin),
        _ => None,
    }
}

pub fn parse_overlay_effect_clock(raw: &str) -> Option<EffectClock> {
    let lower = raw
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase();
    match lower.as_str() {
        "beat" | "beatnooffset" | "bgm" => Some(EffectClock::Beat),
        "timer" | "timerglobal" | "music" | "musicnooffset" | "time" | "seconds" => {
            Some(EffectClock::Time)
        }
        _ if lower.contains("beat") => Some(EffectClock::Beat),
        _ if !lower.is_empty() => Some(EffectClock::Time),
        _ => None,
    }
}

pub fn parse_overlay_text_align(raw: &str) -> Option<TextAlign> {
    let lower = raw
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase();
    match lower.as_str() {
        "left" | "horizalign_left" => Some(TextAlign::Left),
        "center" | "middle" | "horizalign_center" | "horizalign_middle" => Some(TextAlign::Center),
        "right" | "horizalign_right" => Some(TextAlign::Right),
        _ => None,
    }
}

pub fn parse_overlay_text_glow_mode(raw: &str) -> Option<SongLuaTextGlowMode> {
    let lower = raw
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase();
    match lower.as_str() {
        "inner" | "textglowmode_inner" => Some(SongLuaTextGlowMode::Inner),
        "stroke" | "textglowmode_stroke" => Some(SongLuaTextGlowMode::Stroke),
        "both" | "textglowmode_both" => Some(SongLuaTextGlowMode::Both),
        _ => None,
    }
}

pub fn input_status_actor_text(actor_type: &str) -> Option<&'static str> {
    if actor_type.eq_ignore_ascii_case("DeviceList") {
        Some("No input devices")
    } else if actor_type.eq_ignore_ascii_case("InputList") {
        Some("No unmapped inputs")
    } else {
        None
    }
}

#[inline(always)]
pub fn effect_clock_label(clock: EffectClock) -> &'static str {
    match clock {
        EffectClock::Time => "time",
        EffectClock::Beat => "beat",
    }
}

#[inline(always)]
pub fn text_glow_mode_label(mode: SongLuaTextGlowMode) -> &'static str {
    match mode {
        SongLuaTextGlowMode::Inner => "inner",
        SongLuaTextGlowMode::Stroke => "stroke",
        SongLuaTextGlowMode::Both => "both",
    }
}

#[inline(always)]
pub fn song_lua_valid_sprite_state_index(index: Option<u32>) -> Option<u32> {
    index.filter(|&value| value != SONG_LUA_SPRITE_STATE_CLEAR)
}

#[inline(always)]
pub fn sprite_sheet_rect(index: u32, cols: u32, rows: u32) -> [f32; 4] {
    let cols = cols.max(1);
    let rows = rows.max(1);
    let col = index % cols;
    let row = (index / cols).min(rows.saturating_sub(1));
    let width = 1.0 / cols as f32;
    let height = 1.0 / rows as f32;
    let left = col as f32 * width;
    let top = row as f32 * height;
    [left, top, left + width, top + height]
}

pub fn song_lua_halign_value(value: &mlua::Value) -> Option<f32> {
    read_f32(value.clone()).or_else(|| {
        read_string(value.clone()).and_then(|raw| {
            match song_lua_align_token(raw.as_str()).as_str() {
                "left" => Some(0.0),
                "center" | "middle" => Some(0.5),
                "right" => Some(1.0),
                _ => None,
            }
        })
    })
}

pub fn song_lua_valign_value(value: &mlua::Value) -> Option<f32> {
    read_f32(value.clone()).or_else(|| {
        read_string(value.clone()).and_then(|raw| {
            match song_lua_align_token(raw.as_str()).as_str() {
                "top" => Some(0.0),
                "center" | "middle" => Some(0.5),
                "bottom" => Some(1.0),
                _ => None,
            }
        })
    })
}

pub fn song_lua_align_token(raw: &str) -> String {
    raw.trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase()
        .trim_start_matches("horizalign_")
        .trim_start_matches("vertalign_")
        .to_string()
}

pub fn song_lua_text_align_value(value: &mlua::Value) -> Option<TextAlign> {
    read_string(value.clone()).and_then(|raw| parse_overlay_text_align(raw.as_str()))
}

pub fn overlay_text_align_label(value: TextAlign) -> &'static str {
    match value {
        TextAlign::Left => "left",
        TextAlign::Center => "center",
        TextAlign::Right => "right",
    }
}

pub fn crop_texture_rect(source: [f32; 2], target: [f32; 2]) -> Option<[f32; 4]> {
    if !source.iter().all(|value| value.is_finite() && *value > 0.0) {
        return None;
    }
    let scale = (target[0] / source[0]).max(target[1] / source[1]);
    if !scale.is_finite() || scale <= f32::EPSILON {
        return None;
    }
    let zoomed = [source[0] * scale, source[1] * scale];
    if zoomed[0] > target[0] + 0.01 {
        let cut = ((zoomed[0] - target[0]) / zoomed[0]).max(0.0) * 0.5;
        return Some([cut, 0.0, 1.0 - cut, 1.0]);
    }
    let cut = ((zoomed[1] - target[1]) / zoomed[1]).max(0.0) * 0.5;
    Some([0.0, cut, 1.0, 1.0 - cut])
}

#[inline(always)]
pub fn song_lua_span_end(start: f32, limit: f32, span_mode: SongLuaSpanMode) -> f32 {
    match span_mode {
        SongLuaSpanMode::Len => start + limit.max(0.0),
        SongLuaSpanMode::End => limit,
    }
}

pub fn rolling_numbers_format(metric: &str) -> &'static str {
    if metric.eq_ignore_ascii_case("RollingNumbersEvaluationB") {
        "%03.0f"
    } else if metric.eq_ignore_ascii_case("RollingNumbersEvaluationA")
        || metric.eq_ignore_ascii_case("RollingNumbersEvaluationNoDecentsWayOffs")
        || metric.eq_ignore_ascii_case("RollingNumbersEvaluation")
    {
        "%04.0f"
    } else {
        "%.0f"
    }
}

pub fn format_rolling_number(format: &str, number: f32) -> String {
    let rounded = number.round().clamp(i64::MIN as f32, i64::MAX as f32) as i64;
    if format.contains("%04") {
        format!("{rounded:04}")
    } else if format.contains("%03") {
        format!("{rounded:03}")
    } else if format.contains("%.2") {
        format!("{number:.2}")
    } else {
        rounded.to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SongLuaOverlayState {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub z_bias: f32,
    pub draw_order: i32,
    pub draw_by_z_position: bool,
    pub halign: f32,
    pub valign: f32,
    pub text_align: TextAlign,
    pub uppercase: bool,
    pub shadow_len: [f32; 2],
    pub shadow_color: [f32; 4],
    pub glow: [f32; 4],
    pub fov: Option<f32>,
    pub vanishpoint: Option<[f32; 2]>,
    pub diffuse: [f32; 4],
    pub vertex_colors: Option<[[f32; 4]; 4]>,
    pub visible: bool,
    pub cropleft: f32,
    pub cropright: f32,
    pub croptop: f32,
    pub cropbottom: f32,
    pub fadeleft: f32,
    pub faderight: f32,
    pub fadetop: f32,
    pub fadebottom: f32,
    pub mask_source: bool,
    pub mask_dest: bool,
    pub depth_test: bool,
    pub zoom: f32,
    pub zoom_x: f32,
    pub zoom_y: f32,
    pub zoom_z: f32,
    pub basezoom: f32,
    pub basezoom_x: f32,
    pub basezoom_y: f32,
    pub basezoom_z: f32,
    pub rot_x_deg: f32,
    pub rot_y_deg: f32,
    pub rot_z_deg: f32,
    pub skew_x: f32,
    pub skew_y: f32,
    pub blend: SongLuaOverlayBlendMode,
    pub vibrate: bool,
    pub effect_magnitude: [f32; 3],
    pub effect_clock: EffectClock,
    pub effect_mode: EffectMode,
    pub effect_color1: [f32; 4],
    pub effect_color2: [f32; 4],
    pub effect_period: f32,
    pub effect_offset: f32,
    pub effect_timing: Option<[f32; 5]>,
    pub rainbow: bool,
    pub rainbow_scroll: bool,
    pub text_jitter: bool,
    pub text_distortion: f32,
    pub text_glow_mode: SongLuaTextGlowMode,
    pub mult_attrs_with_diffuse: bool,
    pub sprite_animate: bool,
    pub sprite_loop: bool,
    pub sprite_playback_rate: f32,
    pub sprite_state_delay: f32,
    pub sprite_state_index: Option<u32>,
    pub decode_movie: bool,
    pub vert_spacing: Option<i32>,
    pub wrap_width_pixels: Option<i32>,
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    pub max_w_pre_zoom: bool,
    pub max_h_pre_zoom: bool,
    pub max_dimension_uses_zoom: bool,
    pub texture_filtering: bool,
    pub texture_wrapping: bool,
    pub texcoord_offset: Option<[f32; 2]>,
    pub custom_texture_rect: Option<[f32; 4]>,
    pub texcoord_velocity: Option<[f32; 2]>,
    pub size: Option<[f32; 2]>,
    pub stretch_rect: Option<[f32; 4]>,
}

impl Default for SongLuaOverlayState {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            z_bias: 0.0,
            draw_order: 0,
            draw_by_z_position: false,
            halign: 0.5,
            valign: 0.5,
            text_align: TextAlign::Center,
            uppercase: false,
            shadow_len: [0.0, 0.0],
            shadow_color: [0.0, 0.0, 0.0, 0.5],
            glow: [0.0, 0.0, 0.0, 0.0],
            fov: None,
            vanishpoint: None,
            diffuse: [1.0, 1.0, 1.0, 1.0],
            vertex_colors: None,
            visible: true,
            cropleft: 0.0,
            cropright: 0.0,
            croptop: 0.0,
            cropbottom: 0.0,
            fadeleft: 0.0,
            faderight: 0.0,
            fadetop: 0.0,
            fadebottom: 0.0,
            mask_source: false,
            mask_dest: false,
            depth_test: false,
            zoom: 1.0,
            zoom_x: 1.0,
            zoom_y: 1.0,
            zoom_z: 1.0,
            basezoom: 1.0,
            basezoom_x: 1.0,
            basezoom_y: 1.0,
            basezoom_z: 1.0,
            rot_x_deg: 0.0,
            rot_y_deg: 0.0,
            rot_z_deg: 0.0,
            skew_x: 0.0,
            skew_y: 0.0,
            blend: SongLuaOverlayBlendMode::Alpha,
            vibrate: false,
            effect_magnitude: [0.0, 0.0, 0.0],
            effect_clock: EffectClock::Time,
            effect_mode: EffectMode::None,
            effect_color1: [1.0, 1.0, 1.0, 1.0],
            effect_color2: [1.0, 1.0, 1.0, 1.0],
            effect_period: 1.0,
            effect_offset: 0.0,
            effect_timing: None,
            rainbow: false,
            rainbow_scroll: false,
            text_jitter: false,
            text_distortion: 0.0,
            text_glow_mode: SongLuaTextGlowMode::Both,
            mult_attrs_with_diffuse: false,
            sprite_animate: false,
            sprite_loop: true,
            sprite_playback_rate: 1.0,
            sprite_state_delay: 0.1,
            sprite_state_index: None,
            decode_movie: false,
            vert_spacing: None,
            wrap_width_pixels: None,
            max_width: None,
            max_height: None,
            max_w_pre_zoom: false,
            max_h_pre_zoom: false,
            max_dimension_uses_zoom: false,
            texture_filtering: true,
            texture_wrapping: false,
            texcoord_offset: None,
            custom_texture_rect: None,
            texcoord_velocity: None,
            size: None,
            stretch_rect: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SongLuaOverlayStateDelta {
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub z: Option<f32>,
    pub z_bias: Option<f32>,
    pub draw_order: Option<i32>,
    pub draw_by_z_position: Option<bool>,
    pub halign: Option<f32>,
    pub valign: Option<f32>,
    pub text_align: Option<TextAlign>,
    pub uppercase: Option<bool>,
    pub shadow_len: Option<[f32; 2]>,
    pub shadow_color: Option<[f32; 4]>,
    pub glow: Option<[f32; 4]>,
    pub fov: Option<f32>,
    pub vanishpoint: Option<[f32; 2]>,
    pub diffuse: Option<[f32; 4]>,
    pub vertex_colors: Option<[[f32; 4]; 4]>,
    pub visible: Option<bool>,
    pub cropleft: Option<f32>,
    pub cropright: Option<f32>,
    pub croptop: Option<f32>,
    pub cropbottom: Option<f32>,
    pub fadeleft: Option<f32>,
    pub faderight: Option<f32>,
    pub fadetop: Option<f32>,
    pub fadebottom: Option<f32>,
    pub mask_source: Option<bool>,
    pub mask_dest: Option<bool>,
    pub depth_test: Option<bool>,
    pub zoom: Option<f32>,
    pub zoom_x: Option<f32>,
    pub zoom_y: Option<f32>,
    pub zoom_z: Option<f32>,
    pub basezoom: Option<f32>,
    pub basezoom_x: Option<f32>,
    pub basezoom_y: Option<f32>,
    pub basezoom_z: Option<f32>,
    pub rot_x_deg: Option<f32>,
    pub rot_y_deg: Option<f32>,
    pub rot_z_deg: Option<f32>,
    pub skew_x: Option<f32>,
    pub skew_y: Option<f32>,
    pub blend: Option<SongLuaOverlayBlendMode>,
    pub vibrate: Option<bool>,
    pub effect_magnitude: Option<[f32; 3]>,
    pub effect_clock: Option<EffectClock>,
    pub effect_mode: Option<EffectMode>,
    pub effect_color1: Option<[f32; 4]>,
    pub effect_color2: Option<[f32; 4]>,
    pub effect_period: Option<f32>,
    pub effect_offset: Option<f32>,
    pub effect_timing: Option<[f32; 5]>,
    pub rainbow: Option<bool>,
    pub rainbow_scroll: Option<bool>,
    pub text_jitter: Option<bool>,
    pub text_distortion: Option<f32>,
    pub text_glow_mode: Option<SongLuaTextGlowMode>,
    pub mult_attrs_with_diffuse: Option<bool>,
    pub sprite_animate: Option<bool>,
    pub sprite_loop: Option<bool>,
    pub sprite_playback_rate: Option<f32>,
    pub sprite_state_delay: Option<f32>,
    pub sprite_state_index: Option<u32>,
    pub vert_spacing: Option<i32>,
    pub wrap_width_pixels: Option<i32>,
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    pub max_w_pre_zoom: Option<bool>,
    pub max_h_pre_zoom: Option<bool>,
    pub max_dimension_uses_zoom: Option<bool>,
    pub texture_filtering: Option<bool>,
    pub texture_wrapping: Option<bool>,
    pub texcoord_offset: Option<[f32; 2]>,
    pub custom_texture_rect: Option<[f32; 4]>,
    pub texcoord_velocity: Option<[f32; 2]>,
    pub size: Option<[f32; 2]>,
    pub stretch_rect: Option<[f32; 4]>,
    pub sound_play: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaOverlayCommandBlock {
    pub start: f32,
    pub duration: f32,
    pub easing: Option<String>,
    pub opt1: Option<f32>,
    pub opt2: Option<f32>,
    pub delta: SongLuaOverlayStateDelta,
}

pub fn overlay_state_after_blocks(
    mut state: SongLuaOverlayState,
    blocks: &[SongLuaOverlayCommandBlock],
    elapsed: f32,
) -> SongLuaOverlayState {
    if !elapsed.is_finite() {
        return state;
    }
    for block in blocks {
        if elapsed < block.start {
            break;
        }
        if block.duration <= f32::EPSILON || elapsed >= block.start + block.duration {
            apply_overlay_delta(&mut state, &block.delta);
            continue;
        }
        let target = overlay_state_with_delta(state, &block.delta);
        return overlay_state_lerp(
            state,
            target,
            ((elapsed - block.start) / block.duration).clamp(0.0, 1.0),
            &block.delta,
        );
    }
    state
}

fn overlay_state_with_delta(
    mut state: SongLuaOverlayState,
    delta: &SongLuaOverlayStateDelta,
) -> SongLuaOverlayState {
    apply_overlay_delta(&mut state, delta);
    state
}

fn apply_overlay_delta(state: &mut SongLuaOverlayState, delta: &SongLuaOverlayStateDelta) {
    if let Some(value) = delta.x {
        state.x = value;
    }
    if let Some(value) = delta.y {
        state.y = value;
    }
    if let Some(value) = delta.z {
        state.z = value;
    }
    if let Some(value) = delta.z_bias {
        state.z_bias = value;
    }
    if let Some(value) = delta.draw_order {
        state.draw_order = value;
    }
    if let Some(value) = delta.draw_by_z_position {
        state.draw_by_z_position = value;
    }
    if let Some(value) = delta.halign {
        state.halign = value;
    }
    if let Some(value) = delta.valign {
        state.valign = value;
    }
    if let Some(value) = delta.text_align {
        state.text_align = value;
    }
    if let Some(value) = delta.uppercase {
        state.uppercase = value;
    }
    if let Some(value) = delta.shadow_len {
        state.shadow_len = value;
    }
    if let Some(value) = delta.shadow_color {
        state.shadow_color = value;
    }
    if let Some(value) = delta.glow {
        state.glow = value;
    }
    if let Some(value) = delta.fov {
        state.fov = Some(value);
    }
    if let Some(value) = delta.vanishpoint {
        state.vanishpoint = Some(value);
    }
    if let Some(value) = delta.diffuse {
        state.diffuse = value;
    }
    if let Some(value) = delta.vertex_colors {
        state.vertex_colors = Some(value);
    }
    if let Some(value) = delta.visible {
        state.visible = value;
    }
    if let Some(value) = delta.cropleft {
        state.cropleft = value;
    }
    if let Some(value) = delta.cropright {
        state.cropright = value;
    }
    if let Some(value) = delta.croptop {
        state.croptop = value;
    }
    if let Some(value) = delta.cropbottom {
        state.cropbottom = value;
    }
    if let Some(value) = delta.fadeleft {
        state.fadeleft = value;
    }
    if let Some(value) = delta.faderight {
        state.faderight = value;
    }
    if let Some(value) = delta.fadetop {
        state.fadetop = value;
    }
    if let Some(value) = delta.fadebottom {
        state.fadebottom = value;
    }
    if let Some(value) = delta.mask_source {
        state.mask_source = value;
    }
    if let Some(value) = delta.mask_dest {
        state.mask_dest = value;
    }
    if let Some(value) = delta.depth_test {
        state.depth_test = value;
    }
    if let Some(value) = delta.zoom {
        state.zoom = value;
    }
    if let Some(value) = delta.zoom_x {
        state.zoom_x = value;
    }
    if let Some(value) = delta.zoom_y {
        state.zoom_y = value;
    }
    if let Some(value) = delta.zoom_z {
        state.zoom_z = value;
    }
    if let Some(value) = delta.basezoom {
        state.basezoom = value;
    }
    if let Some(value) = delta.basezoom_x {
        state.basezoom_x = value;
    }
    if let Some(value) = delta.basezoom_y {
        state.basezoom_y = value;
    }
    if let Some(value) = delta.basezoom_z {
        state.basezoom_z = value;
    }
    if let Some(value) = delta.rot_x_deg {
        state.rot_x_deg = value;
    }
    if let Some(value) = delta.rot_y_deg {
        state.rot_y_deg = value;
    }
    if let Some(value) = delta.rot_z_deg {
        state.rot_z_deg = value;
    }
    if let Some(value) = delta.skew_x {
        state.skew_x = value;
    }
    if let Some(value) = delta.skew_y {
        state.skew_y = value;
    }
    if let Some(value) = delta.blend {
        state.blend = value;
    }
    if let Some(value) = delta.vibrate {
        state.vibrate = value;
    }
    if let Some(value) = delta.effect_magnitude {
        state.effect_magnitude = value;
    }
    if let Some(value) = delta.effect_clock {
        state.effect_clock = value;
    }
    if let Some(value) = delta.effect_mode {
        state.effect_mode = value;
    }
    if let Some(value) = delta.effect_color1 {
        state.effect_color1 = value;
    }
    if let Some(value) = delta.effect_color2 {
        state.effect_color2 = value;
    }
    if let Some(value) = delta.effect_period {
        state.effect_period = value;
    }
    if let Some(value) = delta.effect_offset {
        state.effect_offset = value;
    }
    if let Some(value) = delta.effect_timing {
        state.effect_timing = Some(value);
    }
    if let Some(value) = delta.rainbow {
        state.rainbow = value;
    }
    if let Some(value) = delta.rainbow_scroll {
        state.rainbow_scroll = value;
    }
    if let Some(value) = delta.text_jitter {
        state.text_jitter = value;
    }
    if let Some(value) = delta.text_distortion {
        state.text_distortion = value;
    }
    if let Some(value) = delta.text_glow_mode {
        state.text_glow_mode = value;
    }
    if let Some(value) = delta.mult_attrs_with_diffuse {
        state.mult_attrs_with_diffuse = value;
    }
    if let Some(value) = delta.sprite_animate {
        state.sprite_animate = value;
    }
    if let Some(value) = delta.sprite_loop {
        state.sprite_loop = value;
    }
    if let Some(value) = delta.sprite_playback_rate {
        state.sprite_playback_rate = value;
    }
    if let Some(value) = delta.sprite_state_delay {
        state.sprite_state_delay = value;
    }
    if let Some(value) = delta.sprite_state_index {
        state.sprite_state_index = Some(value);
    }
    if let Some(value) = delta.vert_spacing {
        state.vert_spacing = Some(value);
    }
    if let Some(value) = delta.wrap_width_pixels {
        state.wrap_width_pixels = Some(value);
    }
    if let Some(value) = delta.max_width {
        state.max_width = Some(value);
    }
    if let Some(value) = delta.max_height {
        state.max_height = Some(value);
    }
    if let Some(value) = delta.max_w_pre_zoom {
        state.max_w_pre_zoom = value;
    }
    if let Some(value) = delta.max_h_pre_zoom {
        state.max_h_pre_zoom = value;
    }
    if let Some(value) = delta.max_dimension_uses_zoom {
        state.max_dimension_uses_zoom = value;
    }
    if let Some(value) = delta.texture_filtering {
        state.texture_filtering = value;
    }
    if let Some(value) = delta.texture_wrapping {
        state.texture_wrapping = value;
    }
    if let Some(value) = delta.texcoord_offset {
        state.texcoord_offset = Some(value);
    }
    if let Some(value) = delta.custom_texture_rect {
        state.custom_texture_rect = Some(value);
    }
    if let Some(value) = delta.texcoord_velocity {
        state.texcoord_velocity = Some(value);
    }
    if let Some(value) = delta.size {
        state.size = Some(value);
    }
    if let Some(value) = delta.stretch_rect {
        state.stretch_rect = Some(value);
    }
}

fn overlay_state_lerp(
    mut from: SongLuaOverlayState,
    to: SongLuaOverlayState,
    t: f32,
    delta: &SongLuaOverlayStateDelta,
) -> SongLuaOverlayState {
    if delta.x.is_some() {
        from.x = (to.x - from.x).mul_add(t, from.x);
    }
    if delta.y.is_some() {
        from.y = (to.y - from.y).mul_add(t, from.y);
    }
    if delta.z.is_some() {
        from.z = (to.z - from.z).mul_add(t, from.z);
    }
    if delta.z_bias.is_some() {
        from.z_bias = (to.z_bias - from.z_bias).mul_add(t, from.z_bias);
    }
    if delta.draw_order.is_some() && t >= 1.0 - f32::EPSILON {
        from.draw_order = to.draw_order;
    }
    if delta.draw_by_z_position.is_some() && t >= 1.0 - f32::EPSILON {
        from.draw_by_z_position = to.draw_by_z_position;
    }
    if delta.halign.is_some() {
        from.halign = (to.halign - from.halign).mul_add(t, from.halign);
    }
    if delta.valign.is_some() {
        from.valign = (to.valign - from.valign).mul_add(t, from.valign);
    }
    if delta.text_align.is_some() && t >= 1.0 - f32::EPSILON {
        from.text_align = to.text_align;
    }
    if delta.uppercase.is_some() && t >= 1.0 - f32::EPSILON {
        from.uppercase = to.uppercase;
    }
    if delta.shadow_len.is_some() {
        from.shadow_len = [
            (to.shadow_len[0] - from.shadow_len[0]).mul_add(t, from.shadow_len[0]),
            (to.shadow_len[1] - from.shadow_len[1]).mul_add(t, from.shadow_len[1]),
        ];
    }
    if delta.shadow_color.is_some() {
        for i in 0..4 {
            from.shadow_color[i] =
                (to.shadow_color[i] - from.shadow_color[i]).mul_add(t, from.shadow_color[i]);
        }
    }
    if delta.glow.is_some() {
        for i in 0..4 {
            from.glow[i] = (to.glow[i] - from.glow[i]).mul_add(t, from.glow[i]);
        }
    }
    if delta.fov.is_some()
        && let (Some(from_fov), Some(to_fov)) = (from.fov, to.fov)
    {
        from.fov = Some((to_fov - from_fov).mul_add(t, from_fov));
    }
    if delta.vanishpoint.is_some()
        && let (Some(from_vanish), Some(to_vanish)) = (from.vanishpoint, to.vanishpoint)
    {
        from.vanishpoint = Some([
            (to_vanish[0] - from_vanish[0]).mul_add(t, from_vanish[0]),
            (to_vanish[1] - from_vanish[1]).mul_add(t, from_vanish[1]),
        ]);
    }
    if delta.diffuse.is_some() {
        for i in 0..4 {
            from.diffuse[i] = (to.diffuse[i] - from.diffuse[i]).mul_add(t, from.diffuse[i]);
        }
    }
    if delta.vertex_colors.is_some() {
        let mut from_colors = from.vertex_colors.unwrap_or([[1.0, 1.0, 1.0, 1.0]; 4]);
        let to_colors = to.vertex_colors.unwrap_or([[1.0, 1.0, 1.0, 1.0]; 4]);
        for corner in 0..4 {
            for channel in 0..4 {
                from_colors[corner][channel] = (to_colors[corner][channel]
                    - from_colors[corner][channel])
                    .mul_add(t, from_colors[corner][channel]);
            }
        }
        from.vertex_colors = Some(from_colors);
    }
    if delta.cropleft.is_some() {
        from.cropleft = (to.cropleft - from.cropleft).mul_add(t, from.cropleft);
    }
    if delta.cropright.is_some() {
        from.cropright = (to.cropright - from.cropright).mul_add(t, from.cropright);
    }
    if delta.croptop.is_some() {
        from.croptop = (to.croptop - from.croptop).mul_add(t, from.croptop);
    }
    if delta.cropbottom.is_some() {
        from.cropbottom = (to.cropbottom - from.cropbottom).mul_add(t, from.cropbottom);
    }
    if delta.fadeleft.is_some() {
        from.fadeleft = (to.fadeleft - from.fadeleft).mul_add(t, from.fadeleft);
    }
    if delta.faderight.is_some() {
        from.faderight = (to.faderight - from.faderight).mul_add(t, from.faderight);
    }
    if delta.fadetop.is_some() {
        from.fadetop = (to.fadetop - from.fadetop).mul_add(t, from.fadetop);
    }
    if delta.fadebottom.is_some() {
        from.fadebottom = (to.fadebottom - from.fadebottom).mul_add(t, from.fadebottom);
    }
    if delta.mask_source.is_some() && t >= 1.0 - f32::EPSILON {
        from.mask_source = to.mask_source;
    }
    if delta.mask_dest.is_some() && t >= 1.0 - f32::EPSILON {
        from.mask_dest = to.mask_dest;
    }
    if delta.zoom.is_some() {
        from.zoom = (to.zoom - from.zoom).mul_add(t, from.zoom);
    }
    if delta.zoom_x.is_some() {
        from.zoom_x = (to.zoom_x - from.zoom_x).mul_add(t, from.zoom_x);
    }
    if delta.zoom_y.is_some() {
        from.zoom_y = (to.zoom_y - from.zoom_y).mul_add(t, from.zoom_y);
    }
    if delta.zoom_z.is_some() {
        from.zoom_z = (to.zoom_z - from.zoom_z).mul_add(t, from.zoom_z);
    }
    if delta.basezoom.is_some() {
        from.basezoom = (to.basezoom - from.basezoom).mul_add(t, from.basezoom);
    }
    if delta.basezoom_x.is_some() {
        from.basezoom_x = (to.basezoom_x - from.basezoom_x).mul_add(t, from.basezoom_x);
    }
    if delta.basezoom_y.is_some() {
        from.basezoom_y = (to.basezoom_y - from.basezoom_y).mul_add(t, from.basezoom_y);
    }
    if delta.basezoom_z.is_some() {
        from.basezoom_z = (to.basezoom_z - from.basezoom_z).mul_add(t, from.basezoom_z);
    }
    if delta.rot_x_deg.is_some() {
        from.rot_x_deg = (to.rot_x_deg - from.rot_x_deg).mul_add(t, from.rot_x_deg);
    }
    if delta.rot_y_deg.is_some() {
        from.rot_y_deg = (to.rot_y_deg - from.rot_y_deg).mul_add(t, from.rot_y_deg);
    }
    if delta.rot_z_deg.is_some() {
        from.rot_z_deg = (to.rot_z_deg - from.rot_z_deg).mul_add(t, from.rot_z_deg);
    }
    if delta.skew_x.is_some() {
        from.skew_x = (to.skew_x - from.skew_x).mul_add(t, from.skew_x);
    }
    if delta.skew_y.is_some() {
        from.skew_y = (to.skew_y - from.skew_y).mul_add(t, from.skew_y);
    }
    if delta.effect_magnitude.is_some() {
        for i in 0..3 {
            from.effect_magnitude[i] = (to.effect_magnitude[i] - from.effect_magnitude[i])
                .mul_add(t, from.effect_magnitude[i]);
        }
    }
    if delta.effect_color1.is_some() {
        for i in 0..4 {
            from.effect_color1[i] =
                (to.effect_color1[i] - from.effect_color1[i]).mul_add(t, from.effect_color1[i]);
        }
    }
    if delta.effect_color2.is_some() {
        for i in 0..4 {
            from.effect_color2[i] =
                (to.effect_color2[i] - from.effect_color2[i]).mul_add(t, from.effect_color2[i]);
        }
    }
    if delta.effect_period.is_some() {
        from.effect_period = (to.effect_period - from.effect_period).mul_add(t, from.effect_period);
    }
    if delta.effect_offset.is_some() {
        from.effect_offset = (to.effect_offset - from.effect_offset).mul_add(t, from.effect_offset);
    }
    if delta.effect_timing.is_some()
        && let (Some(from_timing), Some(to_timing)) = (from.effect_timing, to.effect_timing)
    {
        from.effect_timing = Some([
            (to_timing[0] - from_timing[0]).mul_add(t, from_timing[0]),
            (to_timing[1] - from_timing[1]).mul_add(t, from_timing[1]),
            (to_timing[2] - from_timing[2]).mul_add(t, from_timing[2]),
            (to_timing[3] - from_timing[3]).mul_add(t, from_timing[3]),
            (to_timing[4] - from_timing[4]).mul_add(t, from_timing[4]),
        ]);
    }
    if delta.sprite_playback_rate.is_some() {
        from.sprite_playback_rate = (to.sprite_playback_rate - from.sprite_playback_rate)
            .mul_add(t, from.sprite_playback_rate);
    }
    if delta.sprite_state_delay.is_some() {
        from.sprite_state_delay =
            (to.sprite_state_delay - from.sprite_state_delay).mul_add(t, from.sprite_state_delay);
    }
    if delta.sprite_state_index.is_some() && t >= 1.0 - f32::EPSILON {
        from.sprite_state_index = to.sprite_state_index;
    }
    if delta.vert_spacing.is_some() && t >= 1.0 - f32::EPSILON {
        from.vert_spacing = to.vert_spacing;
    }
    if delta.wrap_width_pixels.is_some() && t >= 1.0 - f32::EPSILON {
        from.wrap_width_pixels = to.wrap_width_pixels;
    }
    if delta.max_width.is_some()
        && let (Some(from_width), Some(to_width)) = (from.max_width, to.max_width)
    {
        from.max_width = Some((to_width - from_width).mul_add(t, from_width));
    }
    if delta.max_height.is_some()
        && let (Some(from_height), Some(to_height)) = (from.max_height, to.max_height)
    {
        from.max_height = Some((to_height - from_height).mul_add(t, from_height));
    }
    if delta.max_w_pre_zoom.is_some() && t >= 1.0 - f32::EPSILON {
        from.max_w_pre_zoom = to.max_w_pre_zoom;
    }
    if delta.max_h_pre_zoom.is_some() && t >= 1.0 - f32::EPSILON {
        from.max_h_pre_zoom = to.max_h_pre_zoom;
    }
    if delta.max_dimension_uses_zoom.is_some() && t >= 1.0 - f32::EPSILON {
        from.max_dimension_uses_zoom = to.max_dimension_uses_zoom;
    }
    if delta.texcoord_offset.is_some()
        && let (Some(from_offset), Some(to_offset)) = (from.texcoord_offset, to.texcoord_offset)
    {
        from.texcoord_offset = Some([
            (to_offset[0] - from_offset[0]).mul_add(t, from_offset[0]),
            (to_offset[1] - from_offset[1]).mul_add(t, from_offset[1]),
        ]);
    }
    if delta.custom_texture_rect.is_some()
        && let (Some(from_rect), Some(to_rect)) = (from.custom_texture_rect, to.custom_texture_rect)
    {
        from.custom_texture_rect = Some([
            (to_rect[0] - from_rect[0]).mul_add(t, from_rect[0]),
            (to_rect[1] - from_rect[1]).mul_add(t, from_rect[1]),
            (to_rect[2] - from_rect[2]).mul_add(t, from_rect[2]),
            (to_rect[3] - from_rect[3]).mul_add(t, from_rect[3]),
        ]);
    }
    if delta.texcoord_velocity.is_some()
        && let (Some(from_vel), Some(to_vel)) = (from.texcoord_velocity, to.texcoord_velocity)
    {
        from.texcoord_velocity = Some([
            (to_vel[0] - from_vel[0]).mul_add(t, from_vel[0]),
            (to_vel[1] - from_vel[1]).mul_add(t, from_vel[1]),
        ]);
    }
    if delta.size.is_some()
        && let (Some(from_size), Some(to_size)) = (from.size, to.size)
    {
        from.size = Some([
            (to_size[0] - from_size[0]).mul_add(t, from_size[0]),
            (to_size[1] - from_size[1]).mul_add(t, from_size[1]),
        ]);
    }
    if delta.stretch_rect.is_some()
        && let (Some(from_rect), Some(to_rect)) = (from.stretch_rect, to.stretch_rect)
    {
        from.stretch_rect = Some([
            (to_rect[0] - from_rect[0]).mul_add(t, from_rect[0]),
            (to_rect[1] - from_rect[1]).mul_add(t, from_rect[1]),
            (to_rect[2] - from_rect[2]).mul_add(t, from_rect[2]),
            (to_rect[3] - from_rect[3]).mul_add(t, from_rect[3]),
        ]);
    }
    if delta.visible.is_some() && t >= 1.0 - f32::EPSILON {
        from.visible = to.visible;
    }
    if delta.blend.is_some() && t >= 1.0 - f32::EPSILON {
        from.blend = to.blend;
    }
    if delta.vibrate.is_some() && t >= 1.0 - f32::EPSILON {
        from.vibrate = to.vibrate;
    }
    if delta.effect_clock.is_some() && t >= 1.0 - f32::EPSILON {
        from.effect_clock = to.effect_clock;
    }
    if delta.effect_mode.is_some() && t >= 1.0 - f32::EPSILON {
        from.effect_mode = to.effect_mode;
    }
    if delta.rainbow.is_some() && t >= 1.0 - f32::EPSILON {
        from.rainbow = to.rainbow;
    }
    if delta.rainbow_scroll.is_some() && t >= 1.0 - f32::EPSILON {
        from.rainbow_scroll = to.rainbow_scroll;
    }
    if delta.text_jitter.is_some() && t >= 1.0 - f32::EPSILON {
        from.text_jitter = to.text_jitter;
    }
    if delta.text_distortion.is_some() {
        from.text_distortion =
            (to.text_distortion - from.text_distortion).mul_add(t, from.text_distortion);
    }
    if delta.text_glow_mode.is_some() && t >= 1.0 - f32::EPSILON {
        from.text_glow_mode = to.text_glow_mode;
    }
    if delta.mult_attrs_with_diffuse.is_some() && t >= 1.0 - f32::EPSILON {
        from.mult_attrs_with_diffuse = to.mult_attrs_with_diffuse;
    }
    if delta.sprite_animate.is_some() && t >= 1.0 - f32::EPSILON {
        from.sprite_animate = to.sprite_animate;
    }
    if delta.sprite_loop.is_some() && t >= 1.0 - f32::EPSILON {
        from.sprite_loop = to.sprite_loop;
    }
    if delta.texture_wrapping.is_some() && t >= 1.0 - f32::EPSILON {
        from.texture_wrapping = to.texture_wrapping;
    }
    if delta.texture_filtering.is_some() && t >= 1.0 - f32::EPSILON {
        from.texture_filtering = to.texture_filtering;
    }
    if delta.depth_test.is_some() && t >= 1.0 - f32::EPSILON {
        from.depth_test = to.depth_test;
    }
    from
}

fn overlay_delta_is_empty(delta: &SongLuaOverlayStateDelta) -> bool {
    delta.x.is_none()
        && delta.y.is_none()
        && delta.z.is_none()
        && delta.z_bias.is_none()
        && delta.draw_order.is_none()
        && delta.draw_by_z_position.is_none()
        && delta.halign.is_none()
        && delta.valign.is_none()
        && delta.text_align.is_none()
        && delta.uppercase.is_none()
        && delta.shadow_len.is_none()
        && delta.shadow_color.is_none()
        && delta.glow.is_none()
        && delta.fov.is_none()
        && delta.vanishpoint.is_none()
        && delta.diffuse.is_none()
        && delta.vertex_colors.is_none()
        && delta.visible.is_none()
        && delta.cropleft.is_none()
        && delta.cropright.is_none()
        && delta.croptop.is_none()
        && delta.cropbottom.is_none()
        && delta.fadeleft.is_none()
        && delta.faderight.is_none()
        && delta.fadetop.is_none()
        && delta.fadebottom.is_none()
        && delta.mask_source.is_none()
        && delta.mask_dest.is_none()
        && delta.depth_test.is_none()
        && delta.zoom.is_none()
        && delta.zoom_x.is_none()
        && delta.zoom_y.is_none()
        && delta.zoom_z.is_none()
        && delta.basezoom.is_none()
        && delta.basezoom_x.is_none()
        && delta.basezoom_y.is_none()
        && delta.basezoom_z.is_none()
        && delta.rot_x_deg.is_none()
        && delta.rot_y_deg.is_none()
        && delta.rot_z_deg.is_none()
        && delta.skew_x.is_none()
        && delta.skew_y.is_none()
        && delta.blend.is_none()
        && delta.vibrate.is_none()
        && delta.effect_magnitude.is_none()
        && delta.effect_clock.is_none()
        && delta.effect_mode.is_none()
        && delta.effect_color1.is_none()
        && delta.effect_color2.is_none()
        && delta.effect_period.is_none()
        && delta.effect_offset.is_none()
        && delta.effect_timing.is_none()
        && delta.rainbow.is_none()
        && delta.rainbow_scroll.is_none()
        && delta.text_jitter.is_none()
        && delta.text_distortion.is_none()
        && delta.text_glow_mode.is_none()
        && delta.mult_attrs_with_diffuse.is_none()
        && delta.sprite_animate.is_none()
        && delta.sprite_loop.is_none()
        && delta.sprite_playback_rate.is_none()
        && delta.sprite_state_delay.is_none()
        && delta.sprite_state_index.is_none()
        && delta.vert_spacing.is_none()
        && delta.wrap_width_pixels.is_none()
        && delta.max_width.is_none()
        && delta.max_height.is_none()
        && delta.max_w_pre_zoom.is_none()
        && delta.max_h_pre_zoom.is_none()
        && delta.max_dimension_uses_zoom.is_none()
        && delta.texture_filtering.is_none()
        && delta.texture_wrapping.is_none()
        && delta.texcoord_offset.is_none()
        && delta.custom_texture_rect.is_none()
        && delta.texcoord_velocity.is_none()
        && delta.size.is_none()
        && delta.stretch_rect.is_none()
        && delta.sound_play.is_none()
}

fn merge_overlay_delta(into: &mut SongLuaOverlayStateDelta, from: &SongLuaOverlayStateDelta) {
    if from.x.is_some() {
        into.x = from.x;
    }
    if from.y.is_some() {
        into.y = from.y;
    }
    if from.z.is_some() {
        into.z = from.z;
    }
    if from.z_bias.is_some() {
        into.z_bias = from.z_bias;
    }
    if from.draw_order.is_some() {
        into.draw_order = from.draw_order;
    }
    if from.draw_by_z_position.is_some() {
        into.draw_by_z_position = from.draw_by_z_position;
    }
    if from.halign.is_some() {
        into.halign = from.halign;
    }
    if from.valign.is_some() {
        into.valign = from.valign;
    }
    if from.text_align.is_some() {
        into.text_align = from.text_align;
    }
    if from.uppercase.is_some() {
        into.uppercase = from.uppercase;
    }
    if from.shadow_len.is_some() {
        into.shadow_len = from.shadow_len;
    }
    if from.shadow_color.is_some() {
        into.shadow_color = from.shadow_color;
    }
    if from.glow.is_some() {
        into.glow = from.glow;
    }
    if from.fov.is_some() {
        into.fov = from.fov;
    }
    if from.vanishpoint.is_some() {
        into.vanishpoint = from.vanishpoint;
    }
    if from.diffuse.is_some() {
        into.diffuse = from.diffuse;
    }
    if from.visible.is_some() {
        into.visible = from.visible;
    }
    if from.cropleft.is_some() {
        into.cropleft = from.cropleft;
    }
    if from.cropright.is_some() {
        into.cropright = from.cropright;
    }
    if from.croptop.is_some() {
        into.croptop = from.croptop;
    }
    if from.cropbottom.is_some() {
        into.cropbottom = from.cropbottom;
    }
    if from.fadeleft.is_some() {
        into.fadeleft = from.fadeleft;
    }
    if from.faderight.is_some() {
        into.faderight = from.faderight;
    }
    if from.fadetop.is_some() {
        into.fadetop = from.fadetop;
    }
    if from.fadebottom.is_some() {
        into.fadebottom = from.fadebottom;
    }
    if from.mask_source.is_some() {
        into.mask_source = from.mask_source;
    }
    if from.mask_dest.is_some() {
        into.mask_dest = from.mask_dest;
    }
    if from.depth_test.is_some() {
        into.depth_test = from.depth_test;
    }
    if from.halign.is_some() {
        into.halign = from.halign;
    }
    if from.valign.is_some() {
        into.valign = from.valign;
    }
    if from.text_align.is_some() {
        into.text_align = from.text_align;
    }
    if from.shadow_len.is_some() {
        into.shadow_len = from.shadow_len;
    }
    if from.shadow_color.is_some() {
        into.shadow_color = from.shadow_color;
    }
    if from.glow.is_some() {
        into.glow = from.glow;
    }
    if from.vertex_colors.is_some() {
        into.vertex_colors = from.vertex_colors;
    }
    if from.zoom.is_some() {
        into.zoom = from.zoom;
    }
    if from.zoom_x.is_some() {
        into.zoom_x = from.zoom_x;
    }
    if from.zoom_y.is_some() {
        into.zoom_y = from.zoom_y;
    }
    if from.zoom_z.is_some() {
        into.zoom_z = from.zoom_z;
    }
    if from.basezoom.is_some() {
        into.basezoom = from.basezoom;
    }
    if from.basezoom_x.is_some() {
        into.basezoom_x = from.basezoom_x;
    }
    if from.basezoom_y.is_some() {
        into.basezoom_y = from.basezoom_y;
    }
    if from.basezoom_z.is_some() {
        into.basezoom_z = from.basezoom_z;
    }
    if from.rot_x_deg.is_some() {
        into.rot_x_deg = from.rot_x_deg;
    }
    if from.rot_y_deg.is_some() {
        into.rot_y_deg = from.rot_y_deg;
    }
    if from.rot_z_deg.is_some() {
        into.rot_z_deg = from.rot_z_deg;
    }
    if from.skew_x.is_some() {
        into.skew_x = from.skew_x;
    }
    if from.skew_y.is_some() {
        into.skew_y = from.skew_y;
    }
    if from.blend.is_some() {
        into.blend = from.blend;
    }
    if from.vibrate.is_some() {
        into.vibrate = from.vibrate;
    }
    if from.effect_magnitude.is_some() {
        into.effect_magnitude = from.effect_magnitude;
    }
    if from.effect_clock.is_some() {
        into.effect_clock = from.effect_clock;
    }
    if from.effect_mode.is_some() {
        into.effect_mode = from.effect_mode;
    }
    if from.effect_color1.is_some() {
        into.effect_color1 = from.effect_color1;
    }
    if from.effect_color2.is_some() {
        into.effect_color2 = from.effect_color2;
    }
    if from.effect_period.is_some() {
        into.effect_period = from.effect_period;
    }
    if from.effect_offset.is_some() {
        into.effect_offset = from.effect_offset;
    }
    if from.effect_timing.is_some() {
        into.effect_timing = from.effect_timing;
    }
    if from.rainbow.is_some() {
        into.rainbow = from.rainbow;
    }
    if from.rainbow_scroll.is_some() {
        into.rainbow_scroll = from.rainbow_scroll;
    }
    if from.text_jitter.is_some() {
        into.text_jitter = from.text_jitter;
    }
    if from.text_distortion.is_some() {
        into.text_distortion = from.text_distortion;
    }
    if from.text_glow_mode.is_some() {
        into.text_glow_mode = from.text_glow_mode;
    }
    if from.mult_attrs_with_diffuse.is_some() {
        into.mult_attrs_with_diffuse = from.mult_attrs_with_diffuse;
    }
    if from.sprite_animate.is_some() {
        into.sprite_animate = from.sprite_animate;
    }
    if from.sprite_loop.is_some() {
        into.sprite_loop = from.sprite_loop;
    }
    if from.sprite_playback_rate.is_some() {
        into.sprite_playback_rate = from.sprite_playback_rate;
    }
    if from.sprite_state_delay.is_some() {
        into.sprite_state_delay = from.sprite_state_delay;
    }
    if from.sprite_state_index.is_some() {
        into.sprite_state_index = from.sprite_state_index;
    }
    if from.vert_spacing.is_some() {
        into.vert_spacing = from.vert_spacing;
    }
    if from.wrap_width_pixels.is_some() {
        into.wrap_width_pixels = from.wrap_width_pixels;
    }
    if from.max_width.is_some() {
        into.max_width = from.max_width;
    }
    if from.max_height.is_some() {
        into.max_height = from.max_height;
    }
    if from.max_w_pre_zoom.is_some() {
        into.max_w_pre_zoom = from.max_w_pre_zoom;
    }
    if from.max_h_pre_zoom.is_some() {
        into.max_h_pre_zoom = from.max_h_pre_zoom;
    }
    if from.max_dimension_uses_zoom.is_some() {
        into.max_dimension_uses_zoom = from.max_dimension_uses_zoom;
    }
    if from.texture_filtering.is_some() {
        into.texture_filtering = from.texture_filtering;
    }
    if from.texture_wrapping.is_some() {
        into.texture_wrapping = from.texture_wrapping;
    }
    if from.texcoord_offset.is_some() {
        into.texcoord_offset = from.texcoord_offset;
    }
    if from.custom_texture_rect.is_some() {
        into.custom_texture_rect = from.custom_texture_rect;
    }
    if from.texcoord_velocity.is_some() {
        into.texcoord_velocity = from.texcoord_velocity;
    }
    if from.size.is_some() {
        into.size = from.size;
    }
    if from.stretch_rect.is_some() {
        into.stretch_rect = from.stretch_rect;
    }
    if from.sound_play.is_some() {
        into.sound_play = from.sound_play;
    }
}

pub fn overlay_delta_from_blocks(
    blocks: &[SongLuaOverlayCommandBlock],
) -> Option<SongLuaOverlayStateDelta> {
    let mut delta = SongLuaOverlayStateDelta::default();
    for block in blocks {
        merge_overlay_delta(&mut delta, &block.delta);
    }
    (!overlay_delta_is_empty(&delta)).then_some(delta)
}

pub fn overlay_delta_intersection(
    from: &SongLuaOverlayStateDelta,
    to: &SongLuaOverlayStateDelta,
) -> Option<(SongLuaOverlayStateDelta, SongLuaOverlayStateDelta)> {
    let mut out_from = SongLuaOverlayStateDelta::default();
    let mut out_to = SongLuaOverlayStateDelta::default();
    macro_rules! copy_pair {
        ($field:ident) => {
            if let (Some(from_value), Some(to_value)) = (from.$field, to.$field) {
                out_from.$field = Some(from_value);
                out_to.$field = Some(to_value);
            }
        };
    }
    copy_pair!(x);
    copy_pair!(y);
    copy_pair!(z);
    copy_pair!(z_bias);
    copy_pair!(draw_order);
    copy_pair!(draw_by_z_position);
    copy_pair!(halign);
    copy_pair!(valign);
    copy_pair!(text_align);
    copy_pair!(uppercase);
    copy_pair!(shadow_len);
    copy_pair!(shadow_color);
    copy_pair!(glow);
    copy_pair!(fov);
    copy_pair!(vanishpoint);
    copy_pair!(diffuse);
    copy_pair!(vertex_colors);
    copy_pair!(visible);
    copy_pair!(cropleft);
    copy_pair!(cropright);
    copy_pair!(croptop);
    copy_pair!(cropbottom);
    copy_pair!(fadeleft);
    copy_pair!(faderight);
    copy_pair!(fadetop);
    copy_pair!(fadebottom);
    copy_pair!(mask_source);
    copy_pair!(mask_dest);
    copy_pair!(depth_test);
    copy_pair!(zoom);
    copy_pair!(zoom_x);
    copy_pair!(zoom_y);
    copy_pair!(zoom_z);
    copy_pair!(basezoom);
    copy_pair!(basezoom_x);
    copy_pair!(basezoom_y);
    copy_pair!(basezoom_z);
    copy_pair!(rot_x_deg);
    copy_pair!(rot_y_deg);
    copy_pair!(rot_z_deg);
    copy_pair!(skew_x);
    copy_pair!(skew_y);
    copy_pair!(blend);
    copy_pair!(vibrate);
    copy_pair!(effect_magnitude);
    copy_pair!(effect_clock);
    copy_pair!(effect_mode);
    copy_pair!(effect_color1);
    copy_pair!(effect_color2);
    copy_pair!(effect_period);
    copy_pair!(effect_offset);
    copy_pair!(effect_timing);
    copy_pair!(rainbow);
    copy_pair!(rainbow_scroll);
    copy_pair!(text_jitter);
    copy_pair!(text_distortion);
    copy_pair!(text_glow_mode);
    copy_pair!(mult_attrs_with_diffuse);
    copy_pair!(sprite_animate);
    copy_pair!(sprite_loop);
    copy_pair!(sprite_playback_rate);
    copy_pair!(sprite_state_delay);
    copy_pair!(sprite_state_index);
    copy_pair!(vert_spacing);
    copy_pair!(wrap_width_pixels);
    copy_pair!(max_width);
    copy_pair!(max_height);
    copy_pair!(max_w_pre_zoom);
    copy_pair!(max_h_pre_zoom);
    copy_pair!(max_dimension_uses_zoom);
    copy_pair!(texture_filtering);
    copy_pair!(texture_wrapping);
    copy_pair!(texcoord_offset);
    copy_pair!(custom_texture_rect);
    copy_pair!(texcoord_velocity);
    copy_pair!(size);
    copy_pair!(stretch_rect);
    (!overlay_delta_is_empty(&out_from)).then_some((out_from, out_to))
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaOverlayMessageCommand {
    pub message: String,
    pub blocks: Vec<SongLuaOverlayCommandBlock>,
}

#[derive(Debug, Clone)]
pub struct SongLuaOverlayActor<Kind> {
    pub kind: Kind,
    pub name: Option<String>,
    pub parent_index: Option<usize>,
    pub initial_state: SongLuaOverlayState,
    pub message_commands: Vec<SongLuaOverlayMessageCommand>,
}

pub fn named_overlay_indices_by_name<'a>(
    len: usize,
    mut name_at: impl FnMut(usize) -> Option<&'a str>,
) -> HashMap<String, usize> {
    let mut out = HashMap::new();
    for index in 0..len {
        if let Some(name) = name_at(index) {
            out.insert(name.to_string(), index);
        }
    }
    out
}

pub fn overlay_descendants_by_parent(
    len: usize,
    root_index: usize,
    mut parent_at: impl FnMut(usize) -> Option<usize>,
) -> Vec<usize> {
    let mut out = Vec::new();
    for index in 0..len {
        let mut parent = parent_at(index);
        while let Some(parent_index) = parent {
            if parent_index == root_index {
                out.push(index);
                break;
            }
            parent = parent_at(parent_index);
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaOverlayEase {
    pub overlay_index: usize,
    pub unit: SongLuaTimeUnit,
    pub start: f32,
    pub limit: f32,
    pub span_mode: SongLuaSpanMode,
    pub from: SongLuaOverlayStateDelta,
    pub to: SongLuaOverlayStateDelta,
    pub easing: Option<String>,
    pub sustain: Option<f32>,
    pub opt1: Option<f32>,
    pub opt2: Option<f32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SongLuaCapturedActor {
    pub initial_state: SongLuaOverlayState,
    pub message_commands: Vec<SongLuaOverlayMessageCommand>,
}

#[derive(Clone, Copy)]
pub enum SongLuaTrackedActorTarget {
    Player(usize),
    SongForeground,
}

pub struct SongLuaTrackedActor {
    pub table: mlua::Table,
    pub actor: SongLuaCapturedActor,
    pub target: SongLuaTrackedActorTarget,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SongLuaNoteHideWindow {
    pub player: usize,
    pub column: usize,
    pub start_beat: f32,
    pub end_beat: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaColumnOffsetWindow {
    pub unit: SongLuaTimeUnit,
    pub start: f32,
    pub limit: f32,
    pub span_mode: SongLuaSpanMode,
    pub player: usize,
    pub column: usize,
    pub from_y: f32,
    pub to_y: f32,
    pub easing: Option<String>,
    pub sustain: Option<f32>,
    pub opt1: Option<f32>,
    pub opt2: Option<f32>,
}

#[cfg(test)]
mod tests {
    use mlua::{Lua, Value};

    use super::{
        SONG_LUA_INITIAL_LIFE, SongLuaCompileContext, SongLuaDifficulty, SongLuaPlayerContext,
        THEME_RECEPTOR_Y_REV, THEME_RECEPTOR_Y_STD, custom_multi_modifier_key,
        easiest_steps_difficulty, graph_display_body_size, song_lua_arch_name,
        song_lua_difficulty_from_value, song_lua_human_player_count,
        song_lua_steps_type_is_dance_single, theme_has_string, theme_metric_number,
        theme_metric_number_for_screen, theme_pref_default, theme_string, theme_string_names,
    };

    #[test]
    fn difficulty_from_value_accepts_stepmania_names() {
        let lua = Lua::new();
        let value = Value::String(lua.create_string("Difficulty_Challenge").unwrap());

        assert_eq!(
            song_lua_difficulty_from_value(value),
            Some(SongLuaDifficulty::Challenge)
        );
        assert_eq!(
            song_lua_difficulty_from_value(Value::String(lua.create_string("expert").unwrap())),
            Some(SongLuaDifficulty::Challenge)
        );
        assert_eq!(
            song_lua_difficulty_from_value(Value::String(lua.create_string("unknown").unwrap())),
            None
        );
    }

    #[test]
    fn steps_type_policy_accepts_dance_single_aliases() {
        let lua = Lua::new();

        assert!(song_lua_steps_type_is_dance_single(Value::String(
            lua.create_string("StepsType_Dance_Single").unwrap()
        )));
        assert!(song_lua_steps_type_is_dance_single(Value::String(
            lua.create_string("single").unwrap()
        )));
        assert!(!song_lua_steps_type_is_dance_single(Value::String(
            lua.create_string("StepsType_Dance_Double").unwrap()
        )));
    }

    #[test]
    fn easiest_steps_difficulty_ignores_disabled_players() {
        let mut players = std::array::from_fn(|_| SongLuaPlayerContext::default());
        players[0].difficulty = SongLuaDifficulty::Beginner;
        players[0].enabled = false;
        players[1].difficulty = SongLuaDifficulty::Hard;

        assert_eq!(
            easiest_steps_difficulty(&players),
            Some(SongLuaDifficulty::Hard)
        );
    }

    #[test]
    fn human_player_count_counts_enabled_context_players() {
        let mut context = SongLuaCompileContext::new("songs/pack/song", "Song");
        context.players[1].enabled = false;

        assert_eq!(song_lua_human_player_count(&context), 1);
    }

    #[test]
    fn graph_display_body_size_matches_player_count() {
        assert_eq!(graph_display_body_size(1), [610.0, 64.0]);
        assert_eq!(graph_display_body_size(2), [300.0, 64.0]);
        assert_eq!(graph_display_body_size(0), [300.0, 64.0]);
    }

    #[test]
    fn theme_metric_number_exposes_numeric_compat_values() {
        assert_eq!(
            theme_metric_number("Player", "ReceptorArrowsYStandard"),
            Some(THEME_RECEPTOR_Y_STD)
        );
        assert_eq!(
            theme_metric_number("Player", "ReceptorArrowsYReverse"),
            Some(THEME_RECEPTOR_Y_REV)
        );
        assert_eq!(theme_metric_number("Combo", "ShowComboAt"), Some(4.0));
        assert_eq!(
            theme_metric_number("LifeMeterBar", "InitialValue"),
            Some(SONG_LUA_INITIAL_LIFE)
        );
        assert_eq!(
            theme_metric_number("MusicWheel", "NumWheelItems"),
            Some(15.0)
        );
        assert_eq!(
            theme_metric_number("PlayerStageStats", "NumGradeTiersUsed"),
            Some(7.0)
        );
    }

    #[test]
    fn theme_metric_number_uses_screen_and_player_count() {
        assert_eq!(
            theme_metric_number_for_screen("Player", "DrawDistanceBeforeTargetsPixels", 1, 720.0),
            Some(1080.0)
        );
        assert_eq!(
            theme_metric_number_for_screen("GraphDisplay", "BodyWidth", 1, 480.0),
            Some(610.0)
        );
        assert_eq!(
            theme_metric_number_for_screen("GraphDisplay", "BodyWidth", 2, 480.0),
            Some(300.0)
        );
    }

    #[test]
    fn theme_string_names_include_difficulty_and_common_groups() {
        assert_eq!(
            theme_string_names("Difficulty"),
            vec![
                "Difficulty_Beginner".to_string(),
                "Difficulty_Easy".to_string(),
                "Difficulty_Medium".to_string(),
                "Difficulty_Hard".to_string(),
                "Difficulty_Challenge".to_string(),
                "Difficulty_Edit".to_string(),
            ]
        );

        let option_names = theme_string_names("OptionNames");
        assert!(option_names.contains(&"MusicRate".to_string()));
        assert!(option_names.contains(&"Difficulty_Hard".to_string()));
        assert!(theme_string_names("Unknown").is_empty());
    }

    #[test]
    fn theme_string_formats_compat_values() {
        assert_eq!(
            theme_string("Difficulty", "Difficulty_Challenge"),
            "Challenge"
        );
        assert_eq!(theme_string("OptionNames", "Music_Rate"), "Music Rate");
        assert_eq!(theme_string("", "Cancel"), "Cancel");
        assert_eq!(theme_string("", "CustomValue"), "CustomValue");
    }

    #[test]
    fn theme_has_string_matches_compat_groups_and_common_values() {
        assert!(theme_has_string("CustomDifficulty", "Difficulty_Edit"));
        assert!(theme_has_string("OptionTitles", "MusicRate"));
        assert!(theme_has_string("", "Yes"));
        assert!(!theme_has_string("Unknown", "Maybe"));
    }

    #[test]
    fn arch_name_reports_stepmania_style_platform_name() {
        assert!(matches!(
            song_lua_arch_name(),
            "Windows" | "Mac OS X" | "Linux" | "FreeBSD" | "Unknown"
        ));
    }

    #[test]
    fn custom_multi_modifier_key_prefixes_hide_choices() {
        assert_eq!(custom_multi_modifier_key("Hide", "Targets"), "HideTargets");
        assert_eq!(
            custom_multi_modifier_key("hide", "ComboExplosions"),
            "HideComboExplosions"
        );
        assert_eq!(
            custom_multi_modifier_key("GameplayExtras", "ColumnCues"),
            "ColumnCues"
        );
    }

    #[test]
    fn theme_pref_default_returns_compat_defaults() {
        let lua = Lua::new();

        assert_eq!(
            theme_pref_default(&lua, "CasualMaxMeter").unwrap(),
            Value::Integer(12)
        );
        assert_eq!(
            theme_pref_default(&lua, "SimplyLoveColor").unwrap(),
            Value::Integer(1)
        );
        assert!(matches!(
            theme_pref_default(&lua, "ThemeFont").unwrap(),
            Value::String(value) if value.to_str().unwrap() == "Common"
        ));
        assert!(matches!(
            theme_pref_default(&lua, "SongSelectBG").unwrap(),
            Value::String(value) if value.to_str().unwrap() == "Off"
        ));
        assert_eq!(
            theme_pref_default(&lua, "UseImageCache").unwrap(),
            Value::Boolean(true)
        );
        assert_eq!(
            theme_pref_default(&lua, "UnknownPreference").unwrap(),
            Value::Boolean(false)
        );
    }
}
