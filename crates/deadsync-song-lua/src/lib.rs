use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use deadlib_present::actors::TextAlign;
use deadlib_present::anim::{EffectClock, EffectMode};

mod actions;
mod cmd;
mod compat;
mod compile;
mod compile_timing;
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

pub use actions::{
    SongLuaFunctionActionInput, SongLuaFunctionActionPlan, function_action_plan,
    read_actions_with_function_capture,
};
pub use cmd::preprocess_lua_cmd_syntax;
pub use compat::{SongLuaCompatCallbacks, install_default_stdlib_compat, install_stdlib_compat};
pub use compile::{compile_song_lua_with_actors, compile_song_lua_with_default_host};
pub use compile_timing::{
    SongLuaCompileTimer, log_song_lua_compile_timing, song_lua_compile_stage_summary,
};
pub use crypto::create_cryptman_table;
pub use deadlib_assets::parse_sprite_sheet_dims;
pub use eases::{
    SongLuaFunctionEaseDecision, SongLuaFunctionEaseInput, SongLuaFunctionEaseResult,
    SongLuaReadEasesResult, SongLuaReadEasesStats, read_eases_for_overlay_actors,
    read_eases_with_function_capture,
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
    initial_chunk_environment, install_basic_globals, install_cmd_helpers, install_compile_host,
    install_core_globals, install_date_globals, install_ease_table, install_game_state_globals,
    install_late_globals, install_manager_globals, install_message_manager_globals,
    install_screen_manager_globals, install_screen_string_globals, install_screen_utility_globals,
    install_sound_globals, is_compile_global_name, register_loaded_easing_names,
    restore_compile_globals, snapshot_compile_globals, song_lua_local_date_globals,
};
pub use json::{json_to_lua_value, lua_to_json_value};
pub use lua_util::{
    SongLuaActionCaptureScope, SongLuaCapturedMessageCommands, SongLuaFunctionActionCapture,
    SongLuaNoteColumnHandlerSnapshot, SongLuaNoteFieldColumnSnapshot, SongLuaNoteskinTapActorModel,
    SongLuaOverlayCompileActor, TopScreenLuaTables, actor_active_commands, actor_aft_capture_name,
    actor_base_size, actor_base_size_with_image, actor_child_at, actor_children,
    actor_command_queue, actor_crop_source_size, actor_current_capture_block, actor_debug_label,
    actor_decode_movie, actor_diffuse, actor_direct_children, actor_effect_magnitude, actor_glow,
    actor_halign, actor_image_frame_size, actor_image_texture_size, actor_indices_for_pointers,
    actor_is_bitmap_text, actor_is_child_group, actor_named_children, actor_overlay_initial_state,
    actor_pointers_touch_actor, actor_runs_startup_commands, actor_shadow_len,
    actor_sprite_frame_count, actor_sprite_sheet_dims, actor_table_has_update_functions,
    actor_texture_is_video, actor_texture_path, actor_texture_rect,
    actor_tree_has_update_functions, actor_tween_time_left, actor_type_is,
    actor_update_text_pre_zoom_flags, actor_valign, actor_vertex_colors, actor_wrappers,
    actor_zoom_axis, add_actor_child_from_path, banner_sort_order_path, begin_action_capture_scope,
    broadcast_song_lua_message, call_actor_function, call_table_method,
    can_create_named_child_actor, capture_actor_command_preserving_state,
    capture_actor_message_commands, capture_actor_text_attribute, capture_actor_vertex_diffuse,
    capture_block_set_bool, capture_block_set_color, capture_block_set_f32, capture_block_set_i32,
    capture_block_set_size, capture_block_set_stretch, capture_block_set_string,
    capture_block_set_u32, capture_block_set_vec2, capture_block_set_vec3, capture_block_set_vec4,
    capture_block_set_vec5, capture_block_set_vertex_colors, capture_block_set_zoom_axes,
    capture_function_action_blocks, capture_graph_display_values,
    capture_indexed_actor_function_blocks, capture_overlay_compile_actor_function_action_blocks,
    capture_overlay_compile_actor_function_eases, capture_overlay_function_eases,
    capture_scope_actor_pointers, capture_scope_actor_tables, capture_scope_snapshots,
    capture_texture_rect, classify_function_ease_probe, collect_aft_capture_names,
    collect_indexed_actor_capture_blocks, collect_tracked_capture_blocks_for_indices,
    compile_note_column_pos_function_ease, compile_overlay_compile_actor_function_action,
    copy_dummy_actor_tags, create_actor_child_group, create_actorframe_class_table,
    create_bool_array, create_color_constants_table, create_debug_table, create_dummy_actor,
    create_life_meter_table, create_loader_function, create_media_actor, create_music_wheel_table,
    create_named_actor, create_named_child_actor, create_named_text_actor,
    create_note_column_actor, create_note_column_spline_handler, create_note_field_actor,
    create_option_row_table, create_owned_string_array, create_score_display_percent_actor,
    create_score_percent_text_actor, create_screen_timer_actor, create_sprite_class_table,
    create_string_array, create_texture_proxy, create_theme_path_actor,
    create_top_screen_player_actor, create_top_screen_score_actor,
    create_top_screen_song_meter_display_actor, create_top_screen_table,
    create_top_screen_theme_actor, create_underlay_theme_actor, crop_actor_to,
    crop_actor_to_source_size, current_gamestate_player_value, current_gamestate_value,
    current_song_lua_style_name, current_song_value, current_steps_value,
    default_message_command_params, drain_actor_command_queue, execute_script_file,
    finish_actor_tweening, flush_actor_capture, function_ease_actor_indices,
    function_named_upvalue_tables, hurry_actor_tweening, inherit_actor_dirs,
    install_actor_basic_getter_methods, install_actor_child_command_methods,
    install_actor_child_query_methods, install_actor_command_methods,
    install_actor_crop_shadow_methods, install_actor_display_state_methods,
    install_actor_effect_methods, install_actor_effect_time_getter_methods,
    install_actor_extra_transform_methods, install_actor_image_coord_methods,
    install_actor_metatable, install_actor_methods, install_actor_parent_methods,
    install_actor_path_child_methods, install_actor_render_compat_methods,
    install_actor_runtime_child_methods, install_actor_scale_size_methods,
    install_actor_size_getter_methods, install_actor_sprite_animation_methods,
    install_actor_tap_note_methods, install_actor_texture_load_methods,
    install_actor_texture_proxy_getter_methods, install_actor_transform_getter_methods,
    install_actor_transform_methods, install_actor_visual_text_methods,
    install_actor_wrapper_query_methods, install_course_contents_list_children,
    install_def_globals, install_file_loader_globals, install_song_meter_display_children,
    install_texture_proxy_methods, install_top_screen_theme_children,
    install_underlay_theme_children, load_actor_path, load_script_file, lua_format_text,
    lua_text_value, make_actor_add_f32_method, make_actor_capture_f32_method,
    make_actor_chain_method, make_actor_finish_tweening_method, make_actor_set_size_method,
    make_actor_stop_tweening_method, make_actor_tween_method, make_actor_wrap_width_method,
    make_color_table, make_vertex_color_table, method_arg, method_arg_offset,
    nested_function_named_upvalue_tables, normalize_broadcast_params, note_column_pos_offset_y,
    note_field_column_actors, note_field_tables, note_zoom_point_hides, offset_actor_texture_rect,
    overlay_compile_actor_tables_for_indices, overlay_model_layers_from_slots,
    populate_course_contents_display, position_scroller_items, prepare_capture_scope_actor,
    probe_actor_pointers, probe_call_names, probe_function_ease_target, probe_target_kind,
    push_note_hide_window, push_sequence_child_once, push_unique_actor_child,
    read_actor_capture_blocks, read_actor_color_field, read_actor_model_layers,
    read_actor_multi_vertex_mesh, read_actor_multi_vertex_texture_path,
    read_actor_semantic_state_table, read_bitmap_font, read_bitmap_text_attributes,
    read_child_index, read_color_args, read_color_call, read_color_value,
    read_global_function_nested_tables, read_graph_display_body_state,
    read_graph_display_line_state, read_graph_display_size, read_graph_display_values,
    read_model_path, read_note_column_pos_samples, read_note_column_pos_samples_for_fields,
    read_note_column_zoom_hides, read_note_column_zoom_hides_for_actor,
    read_noteskin_tap_actor_model, read_noteskin_tap_actor_slots,
    read_overlay_compile_actor_actions, read_overlay_compile_actors, read_proxy_target_kind,
    read_song_lua_sound_paths, read_song_meter_display_state, read_tracked_compile_actors,
    read_update_function_nested_tables, read_update_function_overlay_compile_actor_actions,
    read_update_function_tables, read_vertex_colors_value, record_probe_method_call,
    register_song_lua_actor, remove_actor_child, remove_all_actor_children, reset_actor_capture,
    reset_actor_capture_tables, reset_indexed_actor_capture_tables,
    reset_overlay_compile_actor_capture_tables, reset_tracked_capture_tables,
    resolve_actor_asset_path, restore_action_capture_scope, restore_actor_mutable_state,
    restore_actors_semantic_state, restore_note_column_handlers, restore_note_field_columns,
    rolling_numbers_text, run_actor_draw_functions, run_actor_draw_functions_for_table,
    run_actor_init_commands, run_actor_init_commands_for_table, run_actor_named_command,
    run_actor_named_command_with_drain, run_actor_named_command_with_drain_and_params,
    run_actor_startup_commands, run_actor_startup_commands_for_table, run_actor_update_functions,
    run_actor_update_functions_for_table, run_actor_update_functions_with_delta,
    run_added_actor_child_commands, run_command_on_leaves,
    run_named_command_on_children_recursively, run_named_command_on_leaves,
    scale_actor_to_rect_with_base_size, set_actor_decode_movie_for_texture,
    set_actor_effect_defaults, set_actor_effect_mode, set_actor_seconds_into_animation,
    set_actor_sound_file_from_value, set_actor_sprite_state, set_actor_texture_from_path,
    set_actor_texture_from_path_methods, set_actor_texture_from_path_methods_or_fallback,
    set_actor_texture_from_value, set_proxy_target_fields, set_rolling_numbers_metric,
    snapshot_actor_mutable_state, snapshot_actor_semantic_state_table,
    snapshot_actors_semantic_state, snapshot_note_column_handlers, snapshot_note_field_columns,
    song_lua_actor_registry, song_lua_screen_center, song_lua_screen_size, table_bool_field,
    table_f32_field, table_i32_field, table_string_field, table_vec2, table_vec3, table_vec4,
    table_vec5, table_vertex_colors, text_attribute_matches, text_attribute_value,
    texture_source_size, top_screen_steps_text, tracked_indices_for_actor_pointers,
    tracked_song_lua_actor,
};
pub use mod_windows::read_mod_windows;
pub use multitap::{
    MULTITAP_HIDE_EPSILON_BEATS, MULTITAP_PREVISIBLE_BEATS, MULTITAP_SAMPLE_STEP, MultitapDesc,
    MultitapPhase, apply_multitap_field_state, calc_multitap_phase,
    compile_multitap_update_overlays_for_actors, multitap_deco_child_state, multitap_deco_state,
    multitap_explosion_command_blocks, multitap_explosion_message_events,
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
    call_perframe_entry, call_update_functions_at, compile_perframes,
    compile_update_function_overlays, current_overlay_compile_actor_states,
    current_perframe_player_states, perframe_boundaries, perframe_delta_seconds, perframe_samples,
    perframe_segment_step, push_perframe_overlay_targets, push_perframe_player_target,
    push_perframe_player_targets, push_perframe_static_targets, push_sampled_perframe_targets,
    read_perframe_entries, relative_player_target, tracked_player_tables,
    unsupported_perframe_info, update_function_end_beat, update_function_overlay_eases,
    update_function_sample_step, update_function_samples,
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
    read_runtime_mod_eases, read_xero_runtime_mod_eases_for_overlay_actors,
    read_xero_runtime_mod_eases_with_overlay_capture, read_xero_runtime_mod_entries,
    record_unsupported_xero_overlay_function_ease, runtime_mod_ease_target, runtime_mod_end_value,
    runtime_mod_entry_players, runtime_mod_key, runtime_mod_start_value,
    runtime_overlay_capture_key, runtime_player_option_ease_target,
};
pub use sl::{create_sl_streams, create_sl_table, init_sl_streams, parse_chart_info};
pub use song_tables::{
    PlayerLuaTables, create_course_table, create_enabled_players_table, create_player_tables,
    create_song_options_table, create_song_table, create_song_util_table, create_songman_table,
    create_steps_table, create_trail_table,
};
#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub use tables::prefsmgr_default_value_legacy_for_bench;
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
    SONG_LUA_TOP_SCREEN_OPTION_ROWS, TOP_SCREEN_THEME_CHILD_NAMES, UNDERLAY_THEME_CHILD_NAMES,
    option_row_default_text, player_child_proxy_name, top_screen_danger_index,
    top_screen_life_meter_bar_index, top_screen_life_meter_index, top_screen_life_meter_name,
    top_screen_option_row_name, top_screen_option_row_name_at, top_screen_player_index,
    top_screen_player_name, top_screen_score_index, top_screen_score_name,
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

#[inline(always)]
pub fn overlay_ease_cmp(
    left: &SongLuaOverlayEase,
    right: &SongLuaOverlayEase,
) -> std::cmp::Ordering {
    left.start
        .total_cmp(&right.start)
        .then_with(|| left.limit.total_cmp(&right.limit))
        .then_with(|| left.overlay_index.cmp(&right.overlay_index))
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

pub fn push_startup_message_if_listened<'a>(
    messages: &mut Vec<SongLuaMessageEvent>,
    command_lists: impl IntoIterator<Item = &'a [SongLuaOverlayMessageCommand]>,
) {
    if message_command_lists_have_listener(command_lists, SONG_LUA_STARTUP_MESSAGE) {
        messages.push(SongLuaMessageEvent {
            beat: 0.0,
            message: SONG_LUA_STARTUP_MESSAGE.to_string(),
            persists: false,
        });
    }
}

pub fn sort_compiled_song_lua<OverlayActor>(compiled: &mut CompiledSongLua<OverlayActor>) {
    compiled.beat_mods.sort_by(mod_window_cmp);
    compiled.time_mods.sort_by(mod_window_cmp);
    compiled.eases.sort_by(ease_window_cmp);
    compiled.overlay_eases.sort_by(overlay_ease_cmp);
    compiled.messages.sort_by(message_event_cmp);
}

pub fn runtime_static_overlay_index_by_path<'a>(
    len: usize,
    mut path_at: impl FnMut(usize) -> Option<&'a Path>,
) -> Option<usize> {
    (0..len).position(|index| {
        path_at(index).is_some_and(|path| {
            path.file_name().is_some_and(|name| {
                name.to_string_lossy()
                    .eq_ignore_ascii_case("_static 4x1.png")
            })
        })
    })
}

pub fn runtime_static_overlay_index_for_actors<NoteskinSlot, ModelVertex, TextAttribute>(
    overlays: &[SongLuaOverlayCompileActor<
        SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>,
    >],
) -> Option<usize> {
    runtime_static_overlay_index_by_path(overlays.len(), |index| {
        let SongLuaOverlayKind::Sprite { texture_path, .. } = &overlays[index].actor.kind else {
            return None;
        };
        Some(texture_path.as_path())
    })
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

impl SongLuaOverlayModelDraw {
    pub const fn new(
        pos: [f32; 3],
        rot: [f32; 3],
        zoom: [f32; 3],
        tint: [f32; 4],
        vert_align: f32,
        blend_add: bool,
        visible: bool,
    ) -> Self {
        Self {
            pos,
            rot,
            zoom,
            tint,
            vert_align,
            blend_add,
            visible,
        }
    }
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

impl<Vertex> SongLuaOverlayModelLayer<Vertex> {
    pub fn new(
        texture_key: Arc<str>,
        vertices: Arc<[Vertex]>,
        model_size: [f32; 2],
        uv_scale: [f32; 2],
        uv_offset: [f32; 2],
        uv_tex_shift: [f32; 2],
        uv_velocity: [f32; 2],
        uv_cycle_seconds: Option<f32>,
        draw: SongLuaOverlayModelDraw,
    ) -> Self {
        Self {
            texture_key,
            vertices,
            model_size,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            uv_velocity,
            uv_cycle_seconds,
            draw,
        }
    }
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

pub fn sprite_texture_rect(
    custom_rect: Option<[f32; 4]>,
    state_index: Option<u32>,
    sheet_dims: Option<(u32, u32)>,
) -> [f32; 4] {
    if let Some(rect) = custom_rect {
        return rect;
    }
    if let Some(state_index) = song_lua_valid_sprite_state_index(state_index)
        && let Some((cols, rows)) = sheet_dims
    {
        return sprite_sheet_rect(state_index, cols, rows);
    }
    [0.0, 0.0, 1.0, 1.0]
}

pub fn sprite_texture_rect_with_offset(
    custom_rect: Option<[f32; 4]>,
    state_index: Option<u32>,
    sheet_dims: Option<(u32, u32)>,
    texcoord_offset: Option<[f32; 2]>,
) -> Option<[f32; 4]> {
    let mut rect = custom_rect.or_else(|| {
        let state_index = song_lua_valid_sprite_state_index(state_index)?;
        let (cols, rows) = sheet_dims?;
        Some(sprite_sheet_rect(state_index, cols, rows))
    });
    if rect.is_none() && texcoord_offset.is_some() {
        rect = Some([0.0, 0.0, 1.0, 1.0]);
    }
    if let (Some(base), Some(offset)) = (rect, texcoord_offset) {
        return Some(offset_texture_rect(base, offset));
    }
    rect
}

pub fn sprite_frame_count(sheet_dims: Option<(u32, u32)>) -> u32 {
    let Some((cols, rows)) = sheet_dims else {
        return 1;
    };
    cols.max(1).saturating_mul(rows.max(1)).max(1)
}

pub fn sprite_image_frame_size(
    texture_size: Option<(f32, f32)>,
    animate: bool,
    state_index: Option<u32>,
    sheet_dims: Option<(u32, u32)>,
) -> Option<(f32, f32)> {
    let (mut width, mut height) = texture_size?;
    if animate || song_lua_valid_sprite_state_index(state_index).is_some() {
        if let Some((cols, rows)) = sheet_dims {
            width /= cols.max(1) as f32;
            height /= rows.max(1) as f32;
        }
    }
    Some((width, height))
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SongLuaScaleToRectPlan {
    pub pos: [f32; 2],
    pub zoom: f32,
    pub flip_x: bool,
    pub flip_y: bool,
}

pub fn scale_to_rect_plan(
    rect: [f32; 4],
    base_size: [f32; 2],
    align: [f32; 2],
    cover: bool,
) -> Option<SongLuaScaleToRectPlan> {
    let width = rect[2] - rect[0];
    let height = rect[3] - rect[1];
    if base_size[0].abs() <= f32::EPSILON || base_size[1].abs() <= f32::EPSILON {
        return None;
    }
    let zoom_x = (width / base_size[0]).abs();
    let zoom_y = (height / base_size[1]).abs();
    let zoom = if cover {
        zoom_x.max(zoom_y)
    } else {
        zoom_x.min(zoom_y)
    };
    if !zoom.is_finite() {
        return None;
    }
    Some(SongLuaScaleToRectPlan {
        pos: [rect[0] + width * align[0], rect[1] + height * align[1]],
        zoom,
        flip_x: width < 0.0,
        flip_y: height < 0.0,
    })
}

#[inline(always)]
pub fn offset_texture_rect(rect: [f32; 4], offset: [f32; 2]) -> [f32; 4] {
    [
        rect[0] + offset[0],
        rect[1] + offset[1],
        rect[2] + offset[0],
        rect[3] + offset[1],
    ]
}

pub fn texture_pixel_offset_rect(
    rect: [f32; 4],
    texture_size: [f32; 2],
    offset: [f32; 2],
) -> Option<[f32; 4]> {
    if texture_size[0] <= f32::EPSILON || texture_size[1] <= f32::EPSILON {
        return None;
    }
    Some(offset_texture_rect(
        rect,
        [offset[0] / texture_size[0], offset[1] / texture_size[1]],
    ))
}

pub fn sprite_animation_state_at(seconds: f32, delay: f32, frame_count: u32) -> u32 {
    let delay = delay.max(0.0);
    let frame_count = frame_count.max(1);
    if delay <= f32::EPSILON {
        0
    } else {
        ((seconds.max(0.0) / delay).floor() as u32) % frame_count
    }
}

pub fn sprite_animation_state_from(
    start: u32,
    seconds: f32,
    playback_rate: f32,
    delay: f32,
    frame_count: u32,
    loops: bool,
) -> u32 {
    let frame_count = frame_count.max(1);
    if delay <= f32::EPSILON || frame_count <= 1 {
        return start.min(frame_count - 1);
    }
    let steps = (seconds * playback_rate / delay).floor() as i64;
    let frame = i64::from(start) + steps;
    let frame_count = i64::from(frame_count);
    if loops {
        frame.rem_euclid(frame_count) as u32
    } else {
        frame.clamp(0, frame_count - 1) as u32
    }
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

pub fn overlay_state_uses_repeat_sampler(state: &SongLuaOverlayState) -> bool {
    state.texture_wrapping
        || state
            .texcoord_offset
            .is_some_and(|[u, v]| u.abs() > f32::EPSILON || v.abs() > f32::EPSILON)
        || state
            .custom_texture_rect
            .is_some_and(|[u0, v0, u1, v1]| u0 < 0.0 || v0 < 0.0 || u1 > 1.0 || v1 > 1.0)
        || state.texcoord_velocity.is_some()
}

pub fn overlay_state_uses_nearest_sampler(state: &SongLuaOverlayState) -> bool {
    !state.texture_filtering
}

pub fn overlay_state_axis_scale(state: SongLuaOverlayState) -> [f32; 2] {
    let basezoom_x = if (state.basezoom_x - 1.0).abs() <= f32::EPSILON {
        state.basezoom
    } else {
        state.basezoom_x
    };
    let basezoom_y = if (state.basezoom_y - 1.0).abs() <= f32::EPSILON {
        state.basezoom
    } else {
        state.basezoom_y
    };
    let zoom_x = if (state.zoom_x - 1.0).abs() <= f32::EPSILON {
        state.zoom
    } else {
        state.zoom_x
    };
    let zoom_y = if (state.zoom_y - 1.0).abs() <= f32::EPSILON {
        state.zoom
    } else {
        state.zoom_y
    };
    [basezoom_x * zoom_x, basezoom_y * zoom_y]
}

pub fn overlay_state_z_scale(state: SongLuaOverlayState) -> f32 {
    let basezoom_z = if (state.basezoom_z - 1.0).abs() <= f32::EPSILON {
        state.basezoom
    } else {
        state.basezoom_z
    };
    let zoom_z = if (state.zoom_z - 1.0).abs() <= f32::EPSILON {
        state.zoom
    } else {
        state.zoom_z
    };
    basezoom_z * zoom_z
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

pub fn overlay_delta_uses_repeat_sampler(delta: &SongLuaOverlayStateDelta) -> bool {
    delta.texture_wrapping == Some(true)
        || delta
            .texcoord_offset
            .is_some_and(|[u, v]| u.abs() > f32::EPSILON || v.abs() > f32::EPSILON)
        || delta
            .custom_texture_rect
            .is_some_and(|[u0, v0, u1, v1]| u0 < 0.0 || v0 < 0.0 || u1 > 1.0 || v1 > 1.0)
        || delta.texcoord_velocity.is_some()
}

pub fn overlay_delta_uses_nearest_sampler(delta: &SongLuaOverlayStateDelta) -> bool {
    delta.texture_filtering == Some(false)
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

#[derive(Debug, Clone)]
pub struct SongLuaOverlayEaseBuildParams {
    pub unit: SongLuaTimeUnit,
    pub start: f32,
    pub limit: f32,
    pub span_mode: SongLuaSpanMode,
    pub easing: Option<String>,
    pub sustain: Option<f32>,
    pub opt1: Option<f32>,
    pub opt2: Option<f32>,
}

pub fn overlay_eases_from_captures(
    overlay_count: usize,
    from_blocks: &[(usize, Vec<SongLuaOverlayCommandBlock>)],
    to_blocks: &[(usize, Vec<SongLuaOverlayCommandBlock>)],
    params: SongLuaOverlayEaseBuildParams,
) -> Vec<SongLuaOverlayEase> {
    let mut from_deltas = HashMap::new();
    for (overlay_index, blocks) in from_blocks {
        if let Some(delta) = overlay_delta_from_blocks(blocks) {
            from_deltas.insert(*overlay_index, delta);
        }
    }
    let mut to_deltas = HashMap::new();
    for (overlay_index, blocks) in to_blocks {
        if let Some(delta) = overlay_delta_from_blocks(blocks) {
            to_deltas.insert(*overlay_index, delta);
        }
    }

    let mut out = Vec::new();
    for overlay_index in 0..overlay_count {
        let Some((from_delta, to_delta)) = from_deltas
            .get(&overlay_index)
            .zip(to_deltas.get(&overlay_index))
            .and_then(|(from_delta, to_delta)| overlay_delta_intersection(from_delta, to_delta))
        else {
            continue;
        };
        out.push(SongLuaOverlayEase {
            overlay_index,
            unit: params.unit,
            start: params.start,
            limit: params.limit,
            span_mode: params.span_mode,
            from: from_delta,
            to: to_delta,
            easing: params.easing.clone(),
            sustain: params.sustain,
            opt1: params.opt1,
            opt2: params.opt2,
        });
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaOverlayMessageCommand {
    pub message: String,
    pub blocks: Vec<SongLuaOverlayCommandBlock>,
}

pub fn message_command_lists_have_listener<'a>(
    command_lists: impl IntoIterator<Item = &'a [SongLuaOverlayMessageCommand]>,
    message: &str,
) -> bool {
    command_lists
        .into_iter()
        .any(|commands| commands.iter().any(|command| command.message == message))
}

#[derive(Debug, Clone)]
pub struct SongLuaOverlayActor<Kind> {
    pub kind: Kind,
    pub name: Option<String>,
    pub parent_index: Option<usize>,
    pub initial_state: SongLuaOverlayState,
    pub message_commands: Vec<SongLuaOverlayMessageCommand>,
}

pub fn overlay_actor_uses_repeat_sampler<Kind>(actor: &SongLuaOverlayActor<Kind>) -> bool {
    overlay_state_uses_repeat_sampler(&actor.initial_state)
        || actor
            .message_commands
            .iter()
            .flat_map(|command| command.blocks.iter())
            .any(|block| overlay_delta_uses_repeat_sampler(&block.delta))
}

pub fn overlay_actor_uses_nearest_sampler<Kind>(actor: &SongLuaOverlayActor<Kind>) -> bool {
    overlay_state_uses_nearest_sampler(&actor.initial_state)
        || actor
            .message_commands
            .iter()
            .flat_map(|command| command.blocks.iter())
            .any(|block| overlay_delta_uses_nearest_sampler(&block.delta))
}

pub fn push_song_lua_video_paths<'a, NoteskinSlot, ModelVertex, TextAttribute>(
    overlays: &'a [SongLuaOverlayActor<
        SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>,
    >],
    seen: &mut HashSet<&'a str>,
    paths: &mut Vec<PathBuf>,
) {
    for overlay in overlays {
        let SongLuaOverlayKind::Sprite {
            texture_path,
            texture_key,
        } = &overlay.kind
        else {
            continue;
        };
        if !is_song_lua_video_path(texture_path) {
            continue;
        }
        if !overlay.initial_state.decode_movie {
            continue;
        }
        if seen.insert(texture_key.as_ref()) {
            paths.push(texture_path.clone());
        }
    }
}

pub fn song_lua_video_paths<NoteskinSlot, ModelVertex, TextAttribute>(
    overlays: &[SongLuaOverlayActor<
        SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>,
    >],
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    push_song_lua_video_paths(overlays, &mut seen, &mut paths);
    paths
}

pub fn push_unique_song_lua_sound_paths(
    source: &[PathBuf],
    seen: &mut HashSet<String>,
    paths: &mut Vec<PathBuf>,
) {
    for path in source {
        if seen.insert(path.to_string_lossy().into_owned()) {
            paths.push(path.clone());
        }
    }
}

pub fn push_song_lua_overlay_sound_paths<NoteskinSlot, ModelVertex, TextAttribute>(
    overlays: &[SongLuaOverlayActor<
        SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>,
    >],
    seen: &mut HashSet<String>,
    paths: &mut Vec<PathBuf>,
) {
    for overlay in overlays {
        let SongLuaOverlayKind::Sound { sound_path } = &overlay.kind else {
            continue;
        };
        if seen.insert(sound_path.to_string_lossy().into_owned()) {
            paths.push(sound_path.clone());
        }
    }
}

pub fn compiled_song_lua_sound_paths<'a, OverlayActor: 'a>(
    compiled: impl IntoIterator<Item = &'a CompiledSongLua<OverlayActor>>,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    for compiled in compiled {
        push_unique_song_lua_sound_paths(&compiled.sound_paths, &mut seen, &mut paths);
    }
    paths
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

pub fn overlay_actor_tree_has_visual<NoteskinSlot, ModelVertex, TextAttribute>(
    overlays: &[SongLuaOverlayCompileActor<
        SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>,
    >],
    root_index: usize,
) -> bool {
    overlay_actor_has_visual(&overlays[root_index].actor)
        || overlay_descendants_by_parent(overlays.len(), root_index, |index| {
            overlays
                .get(index)
                .and_then(|overlay| overlay.actor.parent_index)
        })
        .into_iter()
        .any(|index| overlay_actor_has_visual(&overlays[index].actor))
}

pub fn ensure_overlay_arrow_visual<NoteskinSlot, ModelVertex, TextAttr>(
    lua: &mlua::Lua,
    overlays: &mut Vec<
        SongLuaOverlayCompileActor<SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttr>>,
    >,
    arrow_index: usize,
    noteskin: &str,
    create_dummy_actor: fn(&mlua::Lua, &'static str) -> mlua::Result<mlua::Table>,
    visual_spec: impl FnOnce(
        &str,
    ) -> Option<(
        SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttr>,
        SongLuaOverlayState,
    )>,
) -> Result<(), String> {
    if overlay_actor_tree_has_visual(overlays, arrow_index) {
        return Ok(());
    }
    let Some((kind, initial_state)) = visual_spec(noteskin) else {
        return Ok(());
    };
    overlays.push(SongLuaOverlayCompileActor {
        table: create_dummy_actor(lua, "Model").map_err(|err| err.to_string())?,
        actor: SongLuaOverlayActor {
            kind,
            name: None,
            parent_index: Some(arrow_index),
            initial_state,
            message_commands: Vec::new(),
        },
    });
    Ok(())
}

fn overlay_actor_has_visual<NoteskinSlot, ModelVertex, TextAttribute>(
    actor: &SongLuaOverlayActor<SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>>,
) -> bool {
    matches!(
        actor.kind,
        SongLuaOverlayKind::Sprite { .. }
            | SongLuaOverlayKind::Model { .. }
            | SongLuaOverlayKind::NoteskinActor { .. }
    )
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

pub fn note_hide_window_from_indices(
    player: usize,
    column: usize,
    beats_per_t: f32,
    start_index: usize,
    end_index: usize,
) -> Option<SongLuaNoteHideWindow> {
    if start_index == 0 || end_index < start_index {
        return None;
    }
    let start_beat = (start_index - 1) as f32 * beats_per_t;
    let end_beat = (end_index - 1) as f32 * beats_per_t;
    if !start_beat.is_finite() || !end_beat.is_finite() || end_beat < start_beat {
        return None;
    }
    Some(SongLuaNoteHideWindow {
        player,
        column,
        start_beat,
        end_beat,
    })
}

pub fn note_hide_windows_from_flags(
    player: usize,
    column: usize,
    beats_per_t: f32,
    hidden: &[bool],
) -> Vec<SongLuaNoteHideWindow> {
    if !beats_per_t.is_finite() || beats_per_t <= 0.0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut run_start = None::<usize>;
    for (offset, hidden) in hidden.iter().copied().enumerate() {
        let index = offset + 1;
        match (run_start, hidden) {
            (None, true) => run_start = Some(index),
            (Some(start), false) => {
                if let Some(window) =
                    note_hide_window_from_indices(player, column, beats_per_t, start, index - 1)
                {
                    out.push(window);
                }
                run_start = None;
            }
            _ => {}
        }
    }
    if let Some(start) = run_start
        && let Some(window) =
            note_hide_window_from_indices(player, column, beats_per_t, start, hidden.len())
    {
        out.push(window);
    }
    out
}

pub fn note_column_zoom_hide_beats_per_t(
    mode: &str,
    subtract_song_beat: bool,
    beats_per_t: f32,
) -> Option<f32> {
    if !mode.eq_ignore_ascii_case("NoteColumnSplineMode_Offset") || subtract_song_beat {
        return None;
    }
    (beats_per_t.is_finite() && beats_per_t > 0.0).then_some(beats_per_t)
}

pub fn sort_note_hide_windows(windows: &mut [SongLuaNoteHideWindow]) {
    windows.sort_by(|left, right| {
        left.player
            .cmp(&right.player)
            .then_with(|| left.column.cmp(&right.column))
            .then_with(|| left.start_beat.total_cmp(&right.start_beat))
            .then_with(|| left.end_beat.total_cmp(&right.end_beat))
    });
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SongLuaColumnOffsetSample {
    pub player: usize,
    pub column: usize,
    pub y: f32,
}

pub fn note_column_pos_offset_y_from_points(mode: &str, points: &[[f32; 2]]) -> Option<f32> {
    const EPS: f32 = 0.001;
    if mode.eq_ignore_ascii_case("NoteColumnSplineMode_Disabled") {
        return Some(0.0);
    }
    if !mode.eq_ignore_ascii_case("NoteColumnSplineMode_Offset") {
        return None;
    }
    if points.is_empty() {
        return Some(0.0);
    }
    let mut y = None::<f32>;
    for [x, point_y] in points.iter().copied() {
        if !x.is_finite() || !point_y.is_finite() || x.abs() > EPS {
            return None;
        }
        if let Some(y) = y {
            if (point_y - y).abs() > EPS {
                return None;
            }
        } else {
            y = Some(point_y);
        }
    }
    y
}

#[derive(Debug, Clone)]
pub struct SongLuaColumnOffsetBuildParams {
    pub unit: SongLuaTimeUnit,
    pub start: f32,
    pub limit: f32,
    pub span_mode: SongLuaSpanMode,
    pub easing: Option<String>,
    pub sustain: Option<f32>,
    pub opt1: Option<f32>,
    pub opt2: Option<f32>,
}

pub fn column_offset_windows_from_samples(
    from_samples: &[SongLuaColumnOffsetSample],
    to_samples: &[SongLuaColumnOffsetSample],
    params: SongLuaColumnOffsetBuildParams,
) -> Vec<SongLuaColumnOffsetWindow> {
    let mut keys = Vec::<(usize, usize)>::new();
    for sample in from_samples.iter().chain(to_samples.iter()) {
        let key = (sample.player, sample.column);
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys.sort_unstable();

    let mut out = Vec::new();
    for (player, column) in keys {
        let from_y = column_offset_sample_y(from_samples, player, column);
        let to_y = column_offset_sample_y(to_samples, player, column);
        if from_y.abs() <= f32::EPSILON && to_y.abs() <= f32::EPSILON {
            continue;
        }
        out.push(SongLuaColumnOffsetWindow {
            unit: params.unit,
            start: params.start,
            limit: params.limit,
            span_mode: params.span_mode,
            player,
            column,
            from_y,
            to_y,
            easing: params.easing.clone(),
            sustain: params.sustain,
            opt1: params.opt1,
            opt2: params.opt2,
        });
    }
    out
}

fn column_offset_sample_y(
    samples: &[SongLuaColumnOffsetSample],
    player: usize,
    column: usize,
) -> f32 {
    samples
        .iter()
        .find(|sample| sample.player == player && sample.column == column)
        .map_or(0.0, |sample| sample.y)
}

#[cfg(test)]
mod tests {
    use chrono::{Datelike, Local};
    use deadlib_present::actors::{TextAlign, TextAttribute};
    use deadlib_present::anim::{EffectClock, EffectMode};
    use mlua::{Function, Lua, Table, Value};

    use super::{
        CompiledSongLua, GRAPH_DISPLAY_VALUE_RESOLUTION, MultitapPhase, SONG_LUA_INITIAL_LIFE,
        SONG_LUA_RUNTIME_KEY, SONG_LUA_SPRITE_STATE_CLEAR, SONG_LUA_STARTUP_MESSAGE,
        SongLuaColumnOffsetBuildParams, SongLuaColumnOffsetSample, SongLuaCompileContext,
        SongLuaDifficulty, SongLuaEaseTarget, SongLuaEaseWindow, SongLuaMessageEvent,
        SongLuaModWindow, SongLuaNoteHideWindow, SongLuaNoteskinResolver, SongLuaOverlayActor,
        SongLuaOverlayBlendMode, SongLuaOverlayCommandBlock, SongLuaOverlayCompileActor,
        SongLuaOverlayEase, SongLuaOverlayEaseBuildParams, SongLuaOverlayKind,
        SongLuaOverlayMessageCommand, SongLuaOverlayModelDraw, SongLuaOverlayModelLayer,
        SongLuaOverlayState, SongLuaOverlayStateDelta, SongLuaPlayerContext, SongLuaProxyTarget,
        SongLuaSpanMode, SongLuaSpeedMod, SongLuaTextGlowMode, SongLuaTimeUnit,
        THEME_RECEPTOR_Y_REV, THEME_RECEPTOR_Y_STD, TOP_SCREEN_THEME_CHILD_NAMES,
        UNDERLAY_THEME_CHILD_NAMES, actor_indices_for_pointers, actor_overlay_initial_state,
        actor_pointers_touch_actor, add_actor_child_from_path as add_lua_actor_child_from_path,
        capture_actor_message_commands, capture_block_set_bool, capture_block_set_f32,
        capture_function_action_blocks, capture_indexed_actor_function_blocks,
        capture_overlay_function_eases, collect_indexed_actor_capture_blocks,
        column_offset_windows_from_samples, compile_song_lua_with_actors,
        compile_song_runtime_values, compiled_song_lua_sound_paths, create_debug_table,
        create_dummy_actor as create_lua_dummy_actor,
        create_named_child_actor as create_lua_named_child_actor, create_song_runtime_table,
        custom_multi_modifier_key, easiest_steps_difficulty, ensure_overlay_arrow_visual,
        file_path_string, function_ease_actor_indices, function_named_upvalue_tables,
        graph_display_body_size, install_actor_methods as install_lua_actor_methods,
        message_command_lists_have_listener, multitap_deco_state,
        nested_function_named_upvalue_tables, note_column_pos_offset_y_from_points,
        note_column_zoom_hide_beats_per_t,
        note_field_column_actors as create_note_field_column_actors, note_hide_window_from_indices,
        note_hide_windows_from_flags, note_song_lua_side_effect, offset_texture_rect,
        overlay_eases_from_captures, overlay_state_axis_scale, overlay_state_z_scale,
        parse_overlay_blend_mode, parse_overlay_effect_clock, parse_overlay_effect_mode,
        push_multitap_arrow_sample, push_overlay_sample_eases, push_song_lua_overlay_sound_paths,
        push_song_lua_video_paths, push_startup_message_if_listened,
        read_global_function_nested_tables, read_graph_display_body_state,
        read_graph_display_line_state, read_song_meter_display_state,
        read_update_function_nested_tables, read_update_function_tables, record_song_lua_broadcast,
        reset_actor_capture, reset_indexed_actor_capture_tables,
        runtime_static_overlay_index_by_path, scale_to_rect_plan, set_compile_song_runtime_values,
        song_lua_arch_name, song_lua_difficulty_from_value, song_lua_human_player_count,
        song_lua_steps_type_is_dance_single, song_lua_video_paths, sort_compiled_song_lua,
        sort_note_hide_windows, sprite_animation_state_at, sprite_animation_state_from,
        sprite_frame_count, sprite_image_frame_size, sprite_texture_rect,
        sprite_texture_rect_with_offset, texture_pixel_offset_rect, theme_has_string,
        theme_metric_number, theme_metric_number_for_screen, theme_pref_default, theme_string,
        theme_string_names,
    };
    use std::collections::HashSet;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    type TestOverlayKind = SongLuaOverlayKind<(), (), TextAttribute>;
    type TestOverlayActor = SongLuaOverlayActor<TestOverlayKind>;
    type TestCompiledSongLua = CompiledSongLua<TestOverlayActor>;

    fn test_sprite_overlay(path: PathBuf, decode_movie: bool) -> TestOverlayActor {
        TestOverlayActor {
            kind: TestOverlayKind::Sprite {
                texture_key: Arc::from(path.to_string_lossy().into_owned()),
                texture_path: path,
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState {
                decode_movie,
                ..SongLuaOverlayState::default()
            },
            message_commands: Vec::new(),
        }
    }

    fn test_sound_overlay(path: PathBuf) -> TestOverlayActor {
        TestOverlayActor {
            kind: TestOverlayKind::Sound { sound_path: path },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        }
    }

    fn deadsync_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn song_lua_fixture(name: &str) -> (PathBuf, PathBuf) {
        let root = deadsync_root().join("tests/fixtures/song_lua");
        let entry = root.join(name);
        assert!(
            entry.is_file(),
            "missing song Lua fixture: {}",
            entry.display()
        );
        (root, entry)
    }

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "deadsync-song-lua-crate-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn song_lua_video_paths_filter_and_dedupe_video_sprites() {
        let movie = PathBuf::from("badapple.AVI");
        let overlays = vec![
            test_sprite_overlay(movie.clone(), true),
            test_sprite_overlay(movie.clone(), true),
            test_sprite_overlay(PathBuf::from("panel.png"), true),
            TestOverlayActor {
                kind: TestOverlayKind::Quad,
                name: None,
                parent_index: None,
                initial_state: SongLuaOverlayState::default(),
                message_commands: Vec::new(),
            },
        ];

        assert_eq!(song_lua_video_paths(&overlays), vec![movie]);
    }

    #[test]
    fn song_lua_video_paths_skip_disabled_video_decode() {
        let movie = PathBuf::from("badapple.AVI");
        let overlays = vec![test_sprite_overlay(movie, false)];

        assert!(song_lua_video_paths(&overlays).is_empty());
    }

    #[test]
    fn push_song_lua_video_paths_reuses_seen_keys() {
        let movie = PathBuf::from("badapple.AVI");
        let overlays = vec![test_sprite_overlay(movie.clone(), true)];
        let mut paths = vec![movie.clone()];
        let mut seen = HashSet::from(["badapple.AVI"]);

        push_song_lua_video_paths(&overlays, &mut seen, &mut paths);

        assert_eq!(paths, vec![movie]);
    }

    #[test]
    fn push_song_lua_overlay_sound_paths_filters_and_dedupes_sounds() {
        let sound = PathBuf::from("sound.ogg");
        let overlays = vec![
            test_sound_overlay(sound.clone()),
            test_sound_overlay(sound.clone()),
            test_sprite_overlay(PathBuf::from("movie.avi"), true),
        ];
        let mut paths = Vec::new();
        let mut seen = HashSet::new();

        push_song_lua_overlay_sound_paths(&overlays, &mut seen, &mut paths);

        assert_eq!(paths, vec![sound]);
    }

    #[test]
    fn compiled_song_lua_sound_paths_preserves_first_seen_order() {
        let mut first = TestCompiledSongLua::default();
        first.sound_paths = vec![PathBuf::from("a.ogg"), PathBuf::from("b.ogg")];
        let mut second = TestCompiledSongLua::default();
        second.sound_paths = vec![PathBuf::from("b.ogg"), PathBuf::from("c.ogg")];

        assert_eq!(
            compiled_song_lua_sound_paths([&first, &second]),
            vec![
                PathBuf::from("a.ogg"),
                PathBuf::from("b.ogg"),
                PathBuf::from("c.ogg")
            ]
        );
    }

    fn test_compile_song_lua(
        entry_path: &Path,
        context: &SongLuaCompileContext,
    ) -> Result<TestCompiledSongLua, String> {
        compile_song_lua_with_actors(
            entry_path,
            context,
            SongLuaNoteskinResolver::default(),
            test_create_dummy_actor,
            test_create_named_child_actor,
            test_install_actor_methods,
            test_read_model_slots,
            test_model_layer_from_slot,
            |_context, _noteskin| None,
        )
    }

    fn test_read_model_slots(_: &Path) -> Result<Arc<[()]>, String> {
        Ok(Arc::from(Vec::<()>::new().into_boxed_slice()))
    }

    fn test_model_layer_from_slot(_: &()) -> Option<SongLuaOverlayModelLayer<()>> {
        None
    }

    fn test_create_dummy_actor(lua: &Lua, actor_type: &'static str) -> mlua::Result<Table> {
        create_lua_dummy_actor(lua, actor_type, test_install_actor_methods)
    }

    fn test_create_named_child_actor(lua: &Lua, parent: &Table, name: &str) -> mlua::Result<Table> {
        create_lua_named_child_actor(
            lua,
            parent,
            name,
            test_create_dummy_actor,
            test_create_named_child_actor,
        )
    }

    fn test_note_field_column_actors(lua: &Lua, note_field: &Table) -> mlua::Result<Table> {
        create_note_field_column_actors(lua, note_field, test_create_dummy_actor)
    }

    fn test_install_actor_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
        install_lua_actor_methods(
            lua,
            actor,
            test_add_actor_child_from_path,
            test_note_field_column_actors,
            test_create_named_child_actor,
            test_create_dummy_actor,
        )
    }

    fn test_add_actor_child_from_path(lua: &Lua, actor: &Table, path: &str) -> mlua::Result<()> {
        add_lua_actor_child_from_path(lua, actor, path, test_create_dummy_actor)
    }

    fn mod_window(start: f32, limit: f32, mods: &str) -> SongLuaModWindow {
        SongLuaModWindow {
            unit: SongLuaTimeUnit::Beat,
            start,
            limit,
            span_mode: SongLuaSpanMode::Len,
            mods: mods.to_string(),
            player: None,
        }
    }

    fn ease_window(start: f32, limit: f32) -> SongLuaEaseWindow {
        SongLuaEaseWindow {
            unit: SongLuaTimeUnit::Beat,
            start,
            limit,
            span_mode: SongLuaSpanMode::Len,
            from: 0.0,
            to: 1.0,
            target: SongLuaEaseTarget::PlayerX,
            easing: None,
            player: None,
            sustain: None,
            opt1: None,
            opt2: None,
        }
    }

    fn overlay_ease(overlay_index: usize, start: f32, limit: f32) -> SongLuaOverlayEase {
        SongLuaOverlayEase {
            overlay_index,
            unit: SongLuaTimeUnit::Beat,
            start,
            limit,
            span_mode: SongLuaSpanMode::Len,
            from: SongLuaOverlayStateDelta::default(),
            to: SongLuaOverlayStateDelta::default(),
            easing: None,
            sustain: None,
            opt1: None,
            opt2: None,
        }
    }

    #[test]
    fn compile_song_lua_reads_mod_tables() {
        let song_dir = test_dir("direct");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
mods = {
    {1, 2, "*100 no invert", "len", 2},
}
mod_time = {
    {0, 5, "*100 no dark", "len"},
}
mods_ease = {
    {4, 1, 0, 100, "flip", "len", ease.outQuad, 1},
    {6, 1, 0, 1, function(value) end, "len"},
}
mod_actions = {
    {12, "ShowDDRFail", true},
    {13, function() end},
}
mod_perframes = {
    {16, 20, function() end},
}
return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled =
            test_compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "Test Song"))
                .unwrap();
        assert_eq!(compiled.beat_mods.len(), 1);
        assert_eq!(compiled.beat_mods[0].unit, SongLuaTimeUnit::Beat);
        assert_eq!(compiled.beat_mods[0].span_mode, SongLuaSpanMode::Len);
        assert_eq!(compiled.beat_mods[0].player, Some(2));
        assert_eq!(compiled.time_mods.len(), 1);
        assert_eq!(compiled.eases.len(), 1);
        assert_eq!(
            compiled.eases[0].target,
            SongLuaEaseTarget::Mod("flip".to_string())
        );
        assert_eq!(compiled.eases[0].easing.as_deref(), Some("outQuad"));
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "ShowDDRFail");
        assert_eq!(compiled.info.unsupported_function_eases, 1);
        assert_eq!(compiled.info.unsupported_function_ease_captures.len(), 1);
        assert!(
            compiled.info.unsupported_function_ease_captures[0]
                .contains("function ease unit=Beat start=6.000")
        );
        assert_eq!(compiled.info.unsupported_function_actions, 1);
        assert_eq!(compiled.info.unsupported_function_action_captures.len(), 1);
        assert!(
            compiled.info.unsupported_function_action_captures[0]
                .contains("function action beat=13.000 persists=false")
        );
        assert_eq!(compiled.info.unsupported_perframes, 1);
        assert_eq!(compiled.info.unsupported_perframe_captures.len(), 1);
        assert!(
            compiled.info.unsupported_perframe_captures[0]
                .contains("perframe start=16.000 end=20.000")
        );
    }

    #[test]
    fn compile_song_lua_reads_local_update_mod_time() {
        let song_dir = test_dir("local-update-mod-time");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local mod_time = {
    {0, 5, "*100 no dark", "len"},
}

return Def.ActorFrame{
    Def.ActorFrame{
        OnCommand=function(self)
            self:SetUpdateFunction(function()
                if mod_time[1] then end
            end)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Local Mod Time"),
        )
        .unwrap();
        assert_eq!(compiled.time_mods.len(), 1);
        assert_eq!(compiled.time_mods[0].unit, SongLuaTimeUnit::Second);
        assert_eq!(compiled.time_mods[0].mods, "*100 no dark");
    }

    #[test]
    fn compile_song_lua_samples_player_perframes_into_eases() {
        let song_dir = test_dir("perframe-player");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
mod_perframes = {
    {4, 5, function(beat)
        local p = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        if p then
            p:x(320 + (beat - 4) * 40)
            p:y(240 - (beat - 4) * 30)
            p:z((beat - 4) * -120)
            p:rotationx((beat - 4) * 45)
            p:rotationz((beat - 4) * 90)
            p:skewx((beat - 4) * 0.5)
            p:skewy((beat - 4) * 0.25)
            p:zoom(1 + (beat - 4) * 0.25)
        end
    end},
}
return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Perframe Player"),
        )
        .unwrap();
        assert_eq!(compiled.info.unsupported_perframes, 0);
        assert!(compiled.eases.iter().any(|window| {
            matches!(window.target, SongLuaEaseTarget::PlayerX) && window.player == Some(1)
        }));
        assert!(compiled.eases.iter().any(|window| {
            matches!(window.target, SongLuaEaseTarget::PlayerY) && window.player == Some(1)
        }));
        assert!(compiled.eases.iter().any(|window| {
            matches!(window.target, SongLuaEaseTarget::PlayerZ) && window.player == Some(1)
        }));
        assert!(compiled.eases.iter().any(|window| {
            matches!(window.target, SongLuaEaseTarget::PlayerRotationX) && window.player == Some(1)
        }));
        assert!(compiled.eases.iter().any(|window| {
            matches!(window.target, SongLuaEaseTarget::PlayerRotationZ) && window.player == Some(1)
        }));
        assert!(compiled.eases.iter().any(|window| {
            matches!(window.target, SongLuaEaseTarget::PlayerSkewX) && window.player == Some(1)
        }));
        assert!(compiled.eases.iter().any(|window| {
            matches!(window.target, SongLuaEaseTarget::PlayerSkewY) && window.player == Some(1)
        }));
        assert!(compiled.eases.iter().any(|window| {
            matches!(window.target, SongLuaEaseTarget::PlayerZoomX) && window.player == Some(1)
        }));
        assert!(compiled.eases.iter().any(|window| {
            matches!(window.target, SongLuaEaseTarget::PlayerZoomY) && window.player == Some(1)
        }));
        assert!(compiled.eases.iter().any(|window| {
            matches!(window.target, SongLuaEaseTarget::PlayerZoomZ) && window.player == Some(1)
        }));
    }

    #[test]
    fn compile_song_lua_exposes_song_time_to_perframes() {
        let song_dir = test_dir("perframe-song-time");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
mod_perframes = {
    {4, 5, function()
        local p = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        if p then
            local beat = math.floor(GAMESTATE:GetSongBeat())
            local seconds = math.floor(GAMESTATE:GetCurMusicSeconds())
            local pos = math.floor(GAMESTATE:GetSongPosition():GetSongBeat())
            local since = math.floor(GetTimeSinceStart())
            p:rotationz(beat + seconds + pos + since)
        end
    end},
}
return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Perframe Song Time");
        context.song_display_bpms = [120.0, 120.0];
        context.song_music_rate = 2.0;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.info.unsupported_perframes, 0);
        let windows = compiled
            .eases
            .iter()
            .filter(|window| {
                matches!(window.target, SongLuaEaseTarget::PlayerRotationZ)
                    && window.player == Some(1)
            })
            .collect::<Vec<_>>();
        assert!(!windows.is_empty());
        assert!(
            windows
                .iter()
                .all(|window| window.from == 10.0 && window.to == 10.0)
        );
    }

    #[test]
    fn compile_song_lua_exposes_effect_delta_to_perframes() {
        let song_dir = test_dir("perframe-effect-delta");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
mod_perframes = {
    {4, 5, function()
        local p = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        if p then
            p:effectclock("beat")
            p:x(p:GetEffectDelta() * 100)
            p:effectclock("timer")
            p:y(p:GetEffectDelta() * 1000)
        end
    end},
}
return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Perframe Effect Delta");
        context.song_display_bpms = [120.0, 120.0];
        context.song_music_rate = 2.0;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.info.unsupported_perframes, 0);
        let x_windows = compiled
            .eases
            .iter()
            .filter(|window| {
                matches!(window.target, SongLuaEaseTarget::PlayerX) && window.player == Some(1)
            })
            .collect::<Vec<_>>();
        let y_windows = compiled
            .eases
            .iter()
            .filter(|window| {
                matches!(window.target, SongLuaEaseTarget::PlayerY) && window.player == Some(1)
            })
            .collect::<Vec<_>>();
        assert!(
            x_windows
                .iter()
                .any(|window| window.from > 0.0 || window.to > 0.0)
        );
        assert!(
            y_windows
                .iter()
                .any(|window| window.from > 0.0 || window.to > 0.0)
        );
    }

    #[test]
    fn compile_song_lua_accepts_side_effect_only_perframes() {
        let song_dir = test_dir("perframe-side-effects");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
mod_perframes = {
    {4, 5, function()
        SCREENMAN:SystemMessage("perframe")
        SCREENMAN:GetTopScreen():StartTransitioningScreen("SM_DoNextScreen")
    end},
}
return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Perframe Side Effects"),
        )
        .unwrap();
        assert_eq!(compiled.info.unsupported_perframes, 0);
        assert!(compiled.eases.is_empty());
        assert!(compiled.overlay_eases.is_empty());
    }

    #[test]
    fn compile_song_lua_runs_actor_init_commands() {
        let song_dir = test_dir("init-command");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    InitCommand=function(self)
        prefix_globals = {
            mods = {
                {2, 1, "*100 no dark", "len", 1},
            },
            ease = {
                {8, 2, 0, 100, "flip", "len", ease.inOutQuad, 2},
            },
            actions = {
                {12, "ShowDDRFail", true},
            },
        }
    end,
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Init Command Song"),
        )
        .unwrap();
        assert_eq!(compiled.beat_mods.len(), 1);
        assert_eq!(compiled.beat_mods[0].player, Some(1));
        assert_eq!(compiled.eases.len(), 1);
        assert_eq!(
            compiled.eases[0].target,
            SongLuaEaseTarget::Mod("flip".to_string())
        );
        assert_eq!(compiled.eases[0].player, Some(2));
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "ShowDDRFail");
    }

    #[test]
    fn compile_song_lua_names_callable_table_easings() {
        let song_dir = test_dir("callable-table-easings");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
xero = {
    outElastic = setmetatable({}, {
        __call = function(self, t)
            return t
        end,
    }),
}

mods_ease = {
    {1, 1, 0, 100, "tiny", "len", xero.outElastic, 1},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Callable Table Easings"),
        )
        .unwrap();
        assert_eq!(compiled.eases.len(), 1);
        assert_eq!(compiled.eases[0].easing.as_deref(), Some("outElastic"));
    }

    #[test]
    fn compile_song_lua_runs_actor_startup_commands_with_stub_methods() {
        let song_dir = test_dir("startup-command");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
prefix_globals = {}

return Def.ActorFrame{
    OnCommand=function(self)
        prefix_globals.actions = {
            {4, "StartupReady", true},
        }
    end,
    Def.Actor{
        OnCommand=function(self)
            self:sleep(9e9)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Startup Command Song"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "StartupReady");
    }

    #[test]
    fn compile_song_lua_captures_def_actor_message_commands() {
        let song_dir = test_dir("def-actor-message-command");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
mod_actions = {
    {2, "WorkerPulse", true},
}

return Def.ActorFrame{
    Def.Actor{
        Name="Worker",
        InitCommand=function(self)
            self:aux(2)
        end,
        OnCommand=function(self)
            self:SetUpdateFunction(function(actor)
                actor:aux(actor:getaux() + 3)
            end)
        end,
        WorkerPulseMessageCommand=function(self)
            self:x(self:getaux())
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Def Actor Message Command"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "WorkerPulse");
        assert_eq!(compiled.overlays.len(), 1);
        assert!(matches!(
            compiled.overlays[0].kind,
            SongLuaOverlayKind::Actor
        ));
        assert_eq!(compiled.overlays[0].name.as_deref(), Some("Worker"));
        assert_eq!(compiled.overlays[0].message_commands.len(), 1);
        assert_eq!(
            compiled.overlays[0].message_commands[0].message,
            "WorkerPulse"
        );
        assert_eq!(
            compiled.overlays[0].message_commands[0].blocks[0].delta.x,
            Some(5.0)
        );
    }

    #[test]
    fn compile_song_lua_exposes_product_globals() {
        let song_dir = test_dir("product-globals");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local version = ProductVersion()
local product = ProductID()
local family = ProductFamily()

if version ~= "1.2.0" then
    error("unexpected ProductVersion: " .. tostring(version))
end
if product ~= "ITGmania" then
    error("unexpected ProductID: " .. tostring(product))
end
if family ~= "ITGmania" then
    error("unexpected ProductFamily: " .. tostring(family))
end

mod_actions = {
    {4, product .. ":" .. family .. ":" .. version, true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Product Globals"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "ITGmania:ITGmania:1.2.0");
    }

    #[test]
    fn compile_song_lua_exposes_enabled_player_globals() {
        let song_dir = test_dir("player-globals");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local enabled = GAMESTATE:GetEnabledPlayers()
local human = GAMESTATE:GetHumanPlayers()

if PLAYER_1 ~= "PlayerNumber_P1" then
    error("unexpected PLAYER_1: " .. tostring(PLAYER_1))
end
if PLAYER_2 ~= "PlayerNumber_P2" then
    error("unexpected PLAYER_2: " .. tostring(PLAYER_2))
end
if #enabled ~= 1 or enabled[1] ~= PLAYER_1 then
    error("unexpected enabled players")
end
if #human ~= 1 or human[1] ~= PLAYER_1 then
    error("unexpected human players")
end
if not GAMESTATE:IsHumanPlayer(PLAYER_1) then
    error("PLAYER_1 should be human")
end
if GAMESTATE:IsHumanPlayer(PLAYER_2) then
    error("PLAYER_2 should be disabled")
end

mod_actions = {
    {4, enabled[1], true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Player Globals");
        context.players = [
            SongLuaPlayerContext {
                enabled: true,
                ..SongLuaPlayerContext::default()
            },
            SongLuaPlayerContext {
                enabled: false,
                ..SongLuaPlayerContext::default()
            },
        ];

        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "PlayerNumber_P1");
    }

    #[test]
    fn compile_song_lua_exposes_player_noteskin_name() {
        let song_dir = test_dir("player-noteskin");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local po = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions("ModsLevel_Song")
if string.lower(po:NoteSkin()) ~= "cyber" then
    error("unexpected NoteSkin getter: " .. tostring(po:NoteSkin()))
end
po:NoteSkin("lambda")
if po:NoteSkin() ~= "lambda" then
    error("unexpected NoteSkin setter: " .. tostring(po:NoteSkin()))
end
mod_actions = {
    {4, po:NoteSkin(), true},
}
return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Player Noteskin");
        context.players = [
            SongLuaPlayerContext {
                enabled: true,
                noteskin_name: "cyber".to_string(),
                ..SongLuaPlayerContext::default()
            },
            SongLuaPlayerContext {
                enabled: false,
                ..SongLuaPlayerContext::default()
            },
        ];

        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "lambda");
    }

    #[test]
    fn compile_song_lua_supports_bitmap_text_ctor() {
        let song_dir = test_dir("bitmap-text");
        let entry = song_dir.join("default.lua");
        fs::write(song_dir.join("_komika axis 42px.ini"), b"placeholder").unwrap();
        fs::write(
            &entry,
            r##"
return Def.ActorFrame{
    Def.BitmapText{
        Name="Countdown",
        Font="_komika axis 42px.ini",
        Text="",
        OnCommand=function(self)
            self:visible(false)
                :z(10)
                :strokecolor(color("#000000"))
                :settext(3)
                :finishtweening()
        end,
    },
}
"##,
        )
        .unwrap();

        let compiled =
            test_compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "BitmapText"))
                .unwrap();
        assert_eq!(compiled.overlays.len(), 1);
        assert!(!compiled.overlays[0].initial_state.visible);
        assert!(matches!(
            compiled.overlays[0].kind,
            SongLuaOverlayKind::BitmapText {
                ref font_path,
                ref text,
                stroke_color: Some([0.0, 0.0, 0.0, 1.0]),
                ..
            } if font_path.ends_with("_komika axis 42px.ini") && text.as_ref() == "3"
        ));
    }

    #[test]
    fn compile_song_lua_supports_bitmap_text_get_text() {
        let song_dir = test_dir("bitmap-text-get-text");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.BitmapText{
        Font="Common Normal",
        Text="Alpha",
        OnCommand=function(self)
            local before = self:GetText()
            self:settext(3)
            mod_actions = {
                {1, before .. ":" .. self:GetText(), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "BitmapText GetText"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "Alpha:3");
        assert!(matches!(
            compiled.overlays[0].kind,
            SongLuaOverlayKind::BitmapText { ref text, .. } if text.as_ref() == "3"
        ));
    }

    #[test]
    fn compile_song_lua_supports_bitmap_text_settextf() {
        let song_dir = test_dir("bitmap-text-settextf");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.BitmapText{
        Font="Common Normal",
        Text="",
        OnCommand=function(self)
            self:settextf("Stage %02d - %s", 4, "Final")
            mod_actions = {
                {1, self:GetText(), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "BitmapText SetTextF"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "Stage 04 - Final");
        assert!(matches!(
            compiled.overlays[0].kind,
            SongLuaOverlayKind::BitmapText { ref text, .. } if text.as_ref() == "Stage 04 - Final"
        ));
    }

    #[test]
    fn compile_song_lua_supports_rolling_numbers_shape() {
        let song_dir = test_dir("rolling-numbers-shape");
        let entry = song_dir.join("default.lua");
        fs::write(song_dir.join("_numbers.ini"), b"placeholder").unwrap();
        fs::write(
            &entry,
            r#"
local counts = GetExJudgmentCounts(PLAYER_1)
assert(counts.W0 == 0 and counts.totalHolds == 0)

return Def.ActorFrame{
    Def.RollingNumbers{
        Font="_numbers.ini",
        InitCommand=function(self)
            assert(self:Load("RollingNumbersEvaluationA") == self)
            assert(self:targetnumber(12) == self)
            mod_actions = {{
                1,
                string.format("%s:%d", self:GetText(), self:GetTargetNumber()),
                true,
            }}
        end,
    },
    Def.RollingNumbers{
        Font="_numbers.ini",
        InitCommand=function(self)
            assert(self:Load("RollingNumbersEvaluationB") == self)
            assert(self:SetTargetNumber(7) == self)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Rolling Numbers Shape"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "0012:12");
        assert_eq!(compiled.overlays.len(), 2);
        assert!(matches!(
            compiled.overlays[0].kind,
            SongLuaOverlayKind::BitmapText { ref text, .. } if text.as_ref() == "0012"
        ));
        assert!(matches!(
            compiled.overlays[1].kind,
            SongLuaOverlayKind::BitmapText { ref text, .. } if text.as_ref() == "007"
        ));
    }

    #[test]
    fn compile_song_lua_supports_graph_display_shape() {
        let song_dir = test_dir("graph-display-shape");
        let entry = song_dir.join("default.lua");
        fs::write(
                &entry,
                r#"
return Def.ActorFrame{
    Def.GraphDisplay{
        Name="GraphDisplay",
        InitCommand=function(self)
            self:vertalign(top)
            assert(self:Load("GraphDisplay2") == self)
            assert(self:Set(
                STATSMAN:GetCurStageStats(),
                STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1)
            ) == self)
            self:SetWidth(120)
            local body = self:GetChild("")
            body[2]:visible(false)
            local line = self:GetChild("Line")
            line:addy(1)
            mod_actions = {{
                1,
                string.format("%d:%s:%.0f:%.0f", #body, tostring(body[2]:GetVisible()), line:GetY(), self:GetWidth()),
                true,
            }}
        end,
    },
}
"#,
            )
            .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Graph Display Shape"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "2:false:1:120");
        assert_eq!(compiled.overlays.len(), 1);
        let SongLuaOverlayKind::GraphDisplay {
            size,
            body_values,
            body_state,
            line_state,
        } = &compiled.overlays[0].kind
        else {
            panic!("expected GraphDisplay overlay");
        };
        assert_eq!(*size, [120.0, 64.0]);
        assert_eq!(body_values.len(), GRAPH_DISPLAY_VALUE_RESOLUTION);
        assert!(
            body_values
                .iter()
                .all(|value| *value == SONG_LUA_INITIAL_LIFE)
        );
        assert!(!body_state.visible);
        assert_eq!(line_state.y, 1.0);
    }

    #[test]
    fn compile_song_lua_uses_single_player_graph_display_width() {
        let song_dir = test_dir("graph-display-single-player-width");
        let entry = song_dir.join("default.lua");
        fs::write(
                &entry,
                r#"
local metric_width = THEME:GetMetricF("GraphDisplay", "BodyWidth")
local metric_height = THEME:GetMetricI("GraphDisplay", "BodyHeight")

return Def.ActorFrame{
    Def.GraphDisplay{
        InitCommand=function(self)
            mod_actions = {{
                1,
                string.format("%.0f:%d:%.0f:%.0f", metric_width, metric_height, self:GetWidth(), self:GetHeight()),
                true,
            }}
        end,
    },
}
"#,
            )
            .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Graph Display Single Player");
        context.players[1].enabled = false;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();

        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "610:64:610:64");
        assert_eq!(compiled.overlays.len(), 1);
        let SongLuaOverlayKind::GraphDisplay {
            size, body_values, ..
        } = &compiled.overlays[0].kind
        else {
            panic!("expected GraphDisplay overlay");
        };
        assert_eq!(*size, [610.0, 64.0]);
        assert_eq!(body_values.len(), GRAPH_DISPLAY_VALUE_RESOLUTION);
    }

    #[test]
    fn compile_song_lua_supports_song_meter_display_shape() {
        let song_dir = test_dir("song-meter-display-shape");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
                r#"
return Def.ActorFrame{
    Def.SongMeterDisplay{
        StreamWidth=96,
        Stream=Def.Quad{
            InitCommand=function(self)
                self:zoomy(18):diffuse(GetCurrentColor(true))
            end,
        },
        InitCommand=function(self)
            assert(self:GetStreamWidth() == 96)
            assert(self:SetStreamWidth(144) == self)
            local stream = self:GetChild("Stream")
            stream:visible(false)
            mod_actions = {{
                1,
                string.format("%s:%s:%d", stream:GetName(), tostring(stream:GetVisible()), self:GetStreamWidth()),
                true,
            }}
        end,
    },
}
"#,
            )
            .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Song Meter Display Shape"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "Stream:false:144");
        assert_eq!(compiled.overlays.len(), 1);
        let SongLuaOverlayKind::SongMeterDisplay {
            stream_width,
            stream_state,
            music_length_seconds,
        } = &compiled.overlays[0].kind
        else {
            panic!("expected SongMeterDisplay overlay");
        };
        assert_eq!(*stream_width, 144.0);
        assert!(!stream_state.visible);
        assert_eq!(stream_state.zoom_y, 18.0);
        assert_eq!(*music_length_seconds, 0.0);
    }

    #[test]
    fn compile_song_lua_supports_course_contents_list_shape() {
        let song_dir = test_dir("course-contents-list-shape");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local function transform(self, offsetFromCenter, itemIndex, numItems)
    self:y(offsetFromCenter * 23)
end

local function update(ccl, dt)
    if ccl:GetCurrentItem() <= 0 and ccl:GetTweenTimeLeft() == 0 then
        ccl:SetDestinationItem(math.max(0, ccl:GetNumItems() - 1))
    end
end

return Def.ActorFrame{
    Def.CourseContentsList{
        MaxSongs=1000,
        NumItemsToDraw=8,
        InitCommand=function(self)
            self:SetUpdateFunction(update)
        end,
        OnCommand=function(self)
            self:playcommand("Set")
        end,
        SetCommand=function(self)
            assert(self:SetFromGameState() == self)
            assert(self:SetTransformFromHeight(23) == self)
            assert(self:SetTransformFromWidth(100) == self)
            assert(self:SetSecondsPerItem(0.25) == self)
            assert(self:SetNumSubdivisions(2) == self)
            assert(self:ScrollThroughAllItems() == self)
            assert(self:ScrollWithPadding(0, 0) == self)
            assert(self:SetFastCatchup(true) == self)
            assert(self:SetWrap(false) == self)
            assert(self:SetMask(300, 80) == self)
            assert(self:SetNumItemsToDraw(8) == self)
            assert(self:SetCurrentAndDestinationItem(0) == self)
            assert(self:SetTransformFromFunction(transform) == self)
            assert(self:PositionItems() == self)
            assert(self:SetLoop(false) == self)
            assert(self:SetPauseCountdownSeconds(0) == self)
            assert(self:SetSecondsPauseBetweenItems(0.5) == self)

            local display = self:GetChild("Display")
            mod_actions = {{
                1,
                string.format(
                    "%d:%.0f:%.0f:%s:%.1f:%.0f:%.2f:%.2f",
                    self:GetNumItems(),
                    self:GetCurrentItem(),
                    self:GetDestinationItem(),
                    tostring(display ~= nil),
                    self:GetSecondsPauseBetweenItems(),
                    display:GetY(),
                    self:GetSecondsToDestination(),
                    self:GetFullScrollLengthSeconds()
                ),
                true,
            }}
        end,
        Display=Def.ActorFrame{
            Name="Display",
            SetCommand=function(self)
                self:finishtweening()
            end,
            SetSongCommand=function(self, params)
                self:zoom(0.875)
            end,
            Def.BitmapText{
                Font="Common Normal",
                SetSongCommand=function(self, params)
                    self:settext(params.Song:GetDisplayFullTitle() .. ":" .. params.Meter)
                end,
            },
        },
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Course Contents List Shape"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "1:0:0:true:0.5:0:0.00:0.25");
        assert_eq!(compiled.info.unsupported_function_actions, 0);
        assert_eq!(compiled.overlays.len(), 3);
        assert_eq!(compiled.overlays[1].parent_index, Some(0));
        assert_eq!(compiled.overlays[1].initial_state.zoom_x, 0.875);
        assert!(matches!(
            compiled.overlays[2].kind,
            SongLuaOverlayKind::BitmapText {
                font_name: "miso",
                ref text,
                ..
            } if text.as_ref() == "Course Contents List Shape:12"
        ));
    }

    #[test]
    fn compile_song_lua_supports_input_device_list_shapes() {
        let song_dir = test_dir("input-device-list-shapes");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.DeviceList{
        Font=THEME:GetPathF("", "Common Normal"),
        InitCommand=function(self)
            self:xy(_screen.cx, _screen.h - 60):zoom(0.8)
            assert(self:GetText() == "No input devices")
        end,
    },
    Def.InputList{
        Font="Common Normal",
        InitCommand=function(self)
            assert(self:GetText() == "No unmapped inputs")
            self:xy(_screen.cx - 250, 50):horizalign(left):vertalign(top):vertspacing(0)
            mod_actions = {{
                1,
                string.format(
                    "%s:%s:%s",
                    tostring(Def.DeviceList ~= nil),
                    self:GetName(),
                    self:GetText()
                ),
                true,
            }}
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Input Device List Shapes"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "true::No unmapped inputs");
        assert_eq!(compiled.overlays.len(), 2);
        assert!(matches!(
            compiled.overlays[0].kind,
            SongLuaOverlayKind::BitmapText {
                font_name: "miso",
                ref text,
                ..
            } if text.as_ref() == "No input devices"
        ));
        assert!(matches!(
            compiled.overlays[1].kind,
            SongLuaOverlayKind::BitmapText {
                font_name: "miso",
                ref text,
                ..
            } if text.as_ref() == "No unmapped inputs"
        ));
    }

    #[test]
    fn compile_song_lua_supports_model_base_rotation_shape() {
        let song_dir = test_dir("model-base-rotation-shape");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Model{
        Meshes="ring_model.txt",
        Materials="ring_model.txt",
        Bones="ring_model.txt",
        InitCommand=function(self)
            self:diffuse(1, 1, 1, 0.8)
                :baserotationx(-60)
                :baserotationy(20)
                :baserotationz(50)
                :SetTextureFiltering(true)
            mod_actions = {{
                1,
                string.format(
                    "%s:%s:%s:%.0f:%.0f:%.0f",
                    self.Meshes,
                    self.Materials,
                    self.Bones,
                    self:GetRotationX(),
                    self:GetRotationY(),
                    self:GetRotationZ()
                ),
                true,
            }}
        end,
        OnCommand=function(self)
            self:zoom(0.75):xy(SCREEN_CENTER_X, SCREEN_CENTER_Y):z(-100)
                :spin():effectmagnitude(0, 0, 20)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Model Base Rotation Shape"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "ring_model.txt:ring_model.txt:ring_model.txt:-60:20:50"
        );
        assert!(compiled.overlays.is_empty());
    }

    #[test]
    fn compile_song_lua_supports_bitmap_text_style_shims() {
        let song_dir = test_dir("bitmap-text-style-shims");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.BitmapText{
        Font="Common Normal",
        Text="STYLE",
        OnCommand=function(self)
            self:_wrapwidthpixels(88)
                :AddAttribute(0, { Length=1, Diffuse=Color.White })
                :ClearAttributes()
                :rainbowscroll(true)
                :jitter(true)
                :distort(0.5)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "BitmapText Style Shims"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 1);
        assert_eq!(
            compiled.overlays[0].initial_state.wrap_width_pixels,
            Some(88)
        );
        assert!(compiled.overlays[0].initial_state.rainbow_scroll);
        assert!(compiled.overlays[0].initial_state.text_jitter);
        assert_eq!(compiled.overlays[0].initial_state.text_distortion, 0.5);
    }

    #[test]
    fn compile_song_lua_exposes_hooks_and_noteskin_variant_helpers() {
        let song_dir = test_dir("hooks-noteskin-variant-helpers");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local arch = HOOKS:GetArchName()
assert(type(arch) == "string" and arch ~= "")
assert(HOOKS:GetClipboard() == "")
assert(NOTESKIN:HasVariants("default") == false)
assert(NOTESKIN:IsNoteSkinVariant("default") == false)
assert(#NOTESKIN:GetVariantNamesForNoteSkin("default") == 0)

mod_actions = {
    {1, function()
        assert(HOOKS:SetClipboard("theme helper") == false)
        assert(HOOKS:OpenURL("https://example.invalid") == false)
        assert(HOOKS:OpenFile("Save/ThemePrefs.ini") == false)
        assert(HOOKS:RestartProgram() == false)
    end, true},
    {2, arch:lower():match("windows") and "windows" or "not-windows", true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Hooks Noteskin Variants"),
        )
        .unwrap();
        assert_eq!(compiled.info.unsupported_function_actions, 0);
        assert_eq!(compiled.messages.len(), 1);
        assert!(matches!(
            compiled.messages[0].message.as_str(),
            "windows" | "not-windows"
        ));
    }

    #[test]
    fn compile_song_lua_runs_concat_noteskin_sprite_oncommand() {
        let song_dir = test_dir("noteskin-concat-oncommand");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
mod_actions = {}

return Def.ActorFrame{
    NOTESKIN:LoadActorForNoteSkin("Down", "Tap Explosion Bright W1", "cyber")..{
        Name="ConcatNoteskin",
        OnCommand=function(self)
            mod_actions = {
                {4, self:GetName(), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Noteskin Concat"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "ConcatNoteskin");
    }

    #[test]
    fn compile_song_lua_exposes_color_helpers() {
        let song_dir = test_dir("color-helpers");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r##"
local c1 = color("#00000080")
local c2 = color("1,0.5,0.25")
local c3 = color(0.25, 0.5, 0.75, 1)
local mix = lerp_color(0.5, c1, c3)

local function approx(a, b)
    return math.abs(a - b) < 0.001
end

if not approx(c1[4], 128 / 255) then
    error("unexpected hex alpha: " .. tostring(c1[4]))
end
if c2[4] ~= 1 then
    error("numeric string alpha default mismatch")
end
if not approx(mix[1], 0.125) or not approx(mix[2], 0.25) or not approx(mix[3], 0.375) then
    error("unexpected lerp color")
end
if Color.White[1] ~= 1 or Color.White[2] ~= 1 or Color.White[3] ~= 1 or Color.White[4] ~= 1 then
    error("unexpected Color.White")
end
if not approx(Color.Blue[3], 239 / 255) or Color.Blue[1] ~= 0 then
    error("unexpected Color.Blue")
end

return Def.ActorFrame{}
"##,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Color Helpers"),
        )
        .unwrap();
        assert!(compiled.overlays.is_empty());
    }

    #[test]
    fn compile_song_lua_supports_bitmaptext_skew_methods() {
        let song_dir = test_dir("bitmaptext-overlay-skew");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.BitmapText{
        Font="Common Normal",
        Text="SKEW",
        OnCommand=function(self)
            self:skewx(0.15):skewy(-0.35)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "BitmapText Overlay Skew"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 1);
        let state = compiled.overlays[0].initial_state;
        assert!((state.skew_x - 0.15).abs() <= 0.000_1);
        assert!((state.skew_y + 0.35).abs() <= 0.000_1);
    }

    #[test]
    fn compile_song_lua_captures_bitmap_text_attributes() {
        let song_dir = test_dir("bitmap-text-attributes");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.BitmapText{
        Font="Common Normal",
        Text="ATTR",
        OnCommand=function(self)
            self:AddAttribute(1, {
                Length=2,
                Diffuse={0.2, 0.4, 0.6, 0.8},
                Glow={0.7, 0.3, 0.9, 0.5},
            })
        end,
    },
    Def.BitmapText{
        Font="Common Normal",
        Text="GRAD",
        OnCommand=function(self)
            self:AddAttribute(0, {
                Length=-1,
                Diffuses={
                    {1, 0, 0, 1},
                    {0, 1, 0, 1},
                    {0, 0, 1, 1},
                    {1, 1, 0, 1},
                },
            })
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "BitmapText Attributes"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 2);
        let SongLuaOverlayKind::BitmapText { attributes, .. } = &compiled.overlays[0].kind else {
            panic!("expected BitmapText overlay");
        };
        assert_eq!(attributes.len(), 1);
        assert_eq!(attributes[0].start, 1);
        assert_eq!(attributes[0].length, 2);
        assert_eq!(attributes[0].color, [0.2, 0.4, 0.6, 0.8]);
        assert_eq!(attributes[0].glow, Some([0.7, 0.3, 0.9, 0.5]));
        let SongLuaOverlayKind::BitmapText { attributes, .. } = &compiled.overlays[1].kind else {
            panic!("expected BitmapText overlay");
        };
        assert_eq!(attributes.len(), 1);
        assert_eq!(attributes[0].start, 0);
        assert_eq!(attributes[0].length, usize::MAX);
        assert_eq!(attributes[0].color, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(
            attributes[0].vertex_colors,
            Some([
                [1.0, 0.0, 0.0, 1.0],
                [0.0, 1.0, 0.0, 1.0],
                [0.0, 0.0, 1.0, 1.0],
                [1.0, 1.0, 0.0, 1.0],
            ])
        );
    }

    #[test]
    fn compile_song_lua_exposes_theme_color_helpers() {
        let song_dir = test_dir("theme-color-helpers");
        let entry = song_dir.join("default.lua");
        fs::write(
                &entry,
                r##"
local wrap = GetHexColor(13)
local itg = GetHexColor(1, false, "ITG")
local p2 = PlayerColor(PLAYER_2)
local hard = DifficultyColor("Difficulty_Hard")
local edit = DifficultyColor("Difficulty_Edit")
local dark = PlayerDarkColor(PLAYER_2)
local custom = CustomDifficultyToColor("Difficulty_Medium")
local stage = StageToColor("Stage_Final")
local judge = JudgmentLineToColor("JudgmentLine_W1")
local light = LightenColor(color("#202020"))
local blend = BlendColors(Color.Red, Color.Blue)
local alpha = Color.Alpha(Color.White, 0.25)
local named = Color("Black")
local stroke = JudgmentLineToStrokeColor("JudgmentLine_W1")
local step = StepsOrTrailToColor({ GetDifficulty=function() return "Difficulty_Hard" end })
local hex = ColorToHex(color("#00000080"))
local has_alpha = HasAlpha(color("#00000080"))

mod_actions = {
    {
        1,
        string.format(
            "%.3f:%.3f:%.3f:%.3f:%.3f:%.3f:%.3f:%.3f:%.3f:%.3f:%.3f:%.3f:%.3f:%.3f:%.3f:%.3f:%s:%.3f",
            wrap[1],
            wrap[2],
            itg[1],
            p2[3],
            hard[2],
            edit[1],
            dark[1],
            custom[1],
            stage[2],
            judge[1],
            light[1],
            blend[1],
            alpha[4],
            named[1],
            stroke[1],
            step[1],
            hex,
            has_alpha
        ),
        true,
    },
}

return Def.ActorFrame{}
"##,
            )
            .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Theme Color Helpers"),
        )
        .unwrap();

        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "1.000:0.365:0.799:0.000:0.490:0.706:0.290:0.996:0.027:0.749:0.157:0.465:0.250:0.000:0.375:1.000:00000080:0.502"
        );
    }

    #[test]
    fn compile_song_lua_exposes_simply_love_namespace_helpers() {
        let song_dir = test_dir("simply-love-namespace-helpers");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r##"
SL.Global.ActiveColorIndex = 2
local original = { nested = { 1 } }
local copied = DeepCopy(original)
copied.nested[1] = 7

mod_actions = {
    {
        1,
        string.format(
            "%.0f:%.1f:%s:%.0f:%.1f:%d:%d:%d:%.3f:%.2f:%s:%s:%s:%s",
            SL.Global.ActiveColorIndex,
            SL.Global.ActiveModifiers.MusicRate,
            SL.P1.ActiveModifiers.SpeedModType,
            SL.P1.ActiveModifiers.SpeedMod,
            SL_WideScale(10, 20),
            FindInTable(SL.Colors[12], SL.Colors),
            original.nested[1],
            copied.nested[1],
            SL.JudgmentColors["FA+"][7][1],
            round(1.234, 2),
            tostring(IsServiceAllowed(SL.GrooveStats.GetScores)),
            tostring(IsUsingWideScreen()),
            tostring(DarkUI()),
            tostring(SL.P1.ActiveModifiers.TimingWindows[4])
        ),
        true,
    },
}

return Def.ActorFrame{}
"##,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Simply Love Namespace Helpers");
        context.song_music_rate = 1.5;
        context.screen_width = 854.0;
        context.screen_height = 480.0;
        context.players[0].speedmod = SongLuaSpeedMod::C(650.0);
        let compiled = test_compile_song_lua(&entry, &context).unwrap();

        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "2:1.5:C:650:20.0:12:1:7:1.000:1.23:false:true:false:false"
        );
    }

    #[test]
    fn compile_song_lua_exposes_lua51_stdlib_aliases() {
        let song_dir = test_dir("lua51-stdlib-aliases");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local values = {10, 20, 30}
mod_actions = {
    {1, string.format("%d:%d", math.mod(5, 2), table.getn(values)), true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Lua51 Stdlib Aliases"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "1:3");
    }

    #[test]
    fn compile_song_lua_exposes_ivalues_helper() {
        let song_dir = test_dir("ivalues-helper");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local sum = 0
for value in ivalues({10, 20, 30}) do
    sum = sum + value
end
mod_actions = {
    {1, tostring(sum), true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled =
            test_compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "IValues"))
                .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "60");
    }

    #[test]
    fn compile_song_lua_accepts_diffusecolor_alias() {
        let song_dir = test_dir("diffusecolor-alias");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:diffusecolor(0.85, 0.92, 0.99, 0.7)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "DiffuseColor Alias"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 1);
        assert_eq!(
            compiled.overlays[0].initial_state.diffuse,
            [0.85, 0.92, 0.99, 0.7]
        );
    }

    #[test]
    fn compile_song_lua_exposes_theme_player_metrics() {
        let song_dir = test_dir("theme-metrics");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local standard = THEME:GetMetric("Player", "ReceptorArrowsYStandard")
local reverse = THEME:GetMetricF("Player", "ReceptorArrowsYReverse")
local missing = THEME:GetMetric("Player", "NoSuchMetric")

if standard ~= -125 then
    error("unexpected ReceptorArrowsYStandard: " .. tostring(standard))
end
if reverse ~= 145 then
    error("unexpected ReceptorArrowsYReverse: " .. tostring(reverse))
end
if missing ~= nil then
    error("unexpected metric fallback: " .. tostring(missing))
end

mod_actions = {
    {4, "theme-metrics-ok", true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Theme Metrics"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "theme-metrics-ok");
    }

    #[test]
    fn compile_song_lua_exposes_player_draw_distance_metrics() {
        let song_dir = test_dir("player-draw-distance-metrics");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local before = THEME:GetMetric("Player", "DrawDistanceBeforeTargetsPixels")
local before_f = THEME:GetMetricF("Player", "DrawDistanceBeforeTargetsPixels")
local before_i = THEME:GetMetricI("Player", "DrawDistanceBeforeTargetsPixels")
local after = THEME:GetMetric("Player", "DrawDistanceAfterTargetsPixels")

if not THEME:HasMetric("Player", "DrawDistanceBeforeTargetsPixels") then
    error("missing DrawDistanceBeforeTargetsPixels")
end
if not THEME:HasMetric("Player", "DrawDistanceAfterTargetsPixels") then
    error("missing DrawDistanceAfterTargetsPixels")
end

mod_actions = {
    {1, string.format("%.0f:%.0f:%d:%.0f", before, before_f, before_i, after), true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Draw Distance Metrics");
        context.screen_height = 720.0;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "1080:1080:1080:-130");
    }

    #[test]
    fn compile_song_lua_exposes_theme_singleton_compat() {
        let song_dir = test_dir("theme-singletons");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local profile = PROFILEMAN:GetProfile(PLAYER_1)
ThemePrefs.Set("RainbowMode", true)
ThemePrefs.Save()
GAMESTATE:InsertCoin(-GAMESTATE:GetCoinsNeededToJoin())

mod_actions = {
    {
        1,
        string.format(
            "%s:%s:%s:%s:%d:%d:%s:%s:%s:%s:%s:%s:%s:%d",
            tostring(GAMESTATE:IsCourseMode()),
            tostring(GAMESTATE:IsEventMode()),
            GAMESTATE:GetMasterPlayerNumber(),
            GAMESTATE:GetCurrentGame():GetName(),
            GAMESTATE:GetNumSidesJoined(),
            GAMESTATE:GetNumStagesLeft(),
            GAMESTATE:GetCoinMode(),
            GAMESTATE:GetPremium(),
            THEME:GetString("Difficulty", "Difficulty_Challenge"),
            tostring(THEME:HasString("OptionTitles", "Yes")),
            ThemePrefs.Get("ThemeFont"),
            tostring(ThemePrefs.Get("UseImageCache")),
            profile:GetDisplayName(),
            PROFILEMAN:IsPersistentProfile(PLAYER_1) and 1 or 0
        ),
        true,
    },
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Theme Singletons");
        context.players[1].enabled = false;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();

        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "false:false:PlayerNumber_P1:dance:1:1:CoinMode_Free:Premium_Off:Challenge:true:Common:true:Player 1:0"
        );
    }

    #[test]
    fn compile_song_lua_exposes_theme_enum_and_songutil_helpers() {
        let song_dir = test_dir("theme-enum-songutil");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local song = GAMESTATE:GetCurrentSong()
local playable = SongUtil.GetPlayableSteps(song)
local typed = SongUtil:GetPlayableStepsByStepsType(song, "StepsType_Dance_Single")
local player = PlayerNumber:Reverse()[PLAYER_2]
local difficulty = Difficulty:Reverse()["Difficulty_Hard"]
local other = OtherPlayer[PLAYER_1]

GAMESTATE:ApplyGameCommand("mod,1.0xmusic")

mod_actions = {
    {
        1,
        string.format(
            "%d:%d:%s:%d:%d:%s:%s",
            player,
            difficulty,
            other,
            #playable,
            #typed,
            FormatPercentScore(0.93456),
            ScreenSystemLayerHelpers.GetCreditsMessage(PLAYER_1)
        ),
        true,
    },
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Theme Enum SongUtil"),
        )
        .unwrap();

        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "1:3:PlayerNumber_P2:6:6:93.46%:Free Play"
        );
    }

    #[test]
    fn compile_song_lua_initializes_capture_before_startup_tweens() {
        let song_dir = test_dir("startup-capture");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.ActorFrame{
        InitCommand=function(self)
            self:visible(false)
        end,
        OnCommand=function(self)
            self:accelerate(0.8):diffusealpha(1):xy(320, 240)
        end,
    },
}
"#,
        )
        .unwrap();

        test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Startup Capture Song"),
        )
        .unwrap();
    }

    #[test]
    fn compile_song_lua_runs_set_update_function_once() {
        let song_dir = test_dir("set-update-function");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.ActorFrame{
        OnCommand=function(self)
            self:SetUpdateFunction(function()
                mods = {
                    {4, 1, "*100 no dark", "len"},
                }
            end)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "SetUpdateFunction Song"),
        )
        .unwrap();
        assert_eq!(compiled.beat_mods.len(), 1);
        assert_eq!(compiled.beat_mods[0].start, 4.0);
    }

    #[test]
    fn compile_song_lua_passes_update_delta_seconds() {
        let song_dir = test_dir("set-update-function-delta");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    SetUpdateRateCommand=function(self)
        self:SetUpdateRate(3)
    end,
    InitCommand=function(self)
        self:playcommand("SetUpdateRate")
    end,
    Def.Quad{
        OnCommand=function(self)
            self:SetUpdateFunction(function(actor, dt)
                actor:x(dt * 60):y(actor:GetParent():GetUpdateRate())
                mod_actions = {{
                    1,
                    string.format("%.0f:%.0f", actor:GetX(), actor:GetY()),
                    true,
                }}
            end)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "SetUpdateFunction Delta Song"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "3:3");
        assert_eq!(compiled.overlays.len(), 1);
        assert_eq!(compiled.overlays[0].initial_state.x, 3.0);
        assert_eq!(compiled.overlays[0].initial_state.y, 3.0);
    }

    #[test]
    fn compile_song_lua_drains_update_function_queuecommands() {
        let song_dir = test_dir("set-update-function-queuecommand");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        Name="Panel",
        OnCommand=function(self)
            self:SetUpdateFunction(function(actor)
                actor:queuecommand("Pulse")
            end)
        end,
        PulseCommand=function(self)
            self:x(12)
            mod_actions = {
                {1, "update-queued", true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "SetUpdateFunction QueueCommand Song"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "update-queued");
        assert_eq!(compiled.overlays.len(), 1);
        assert_eq!(compiled.overlays[0].initial_state.x, 12.0);
    }

    #[test]
    fn compile_song_lua_samples_update_function_overlay_motion() {
        let song_dir = test_dir("set-update-function-overlay-motion");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local target
return Def.ActorFrame{
    Def.Quad{
        InitCommand=function(self)
            target = self
            self:visible(false):zoomto(16, 16)
        end,
    },
    Def.ActorFrame{
        OnCommand=function(self)
            self:SetUpdateFunction(function()
                local beat = GAMESTATE:GetSongBeat()
                target:visible(beat >= 2 and beat <= 4)
                target:x(beat * 10)
                target:rotationz(beat * 15)
            end)
        end,
    },
}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "SetUpdateFunction Overlay Motion");
        context.music_length_seconds = 6.0;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.overlays.len(), 1);
        assert!(compiled.overlay_eases.iter().any(|ease| {
            ease.overlay_index == 0 && ease.from.x.is_some() && ease.to.x.is_some()
        }));
        assert!(compiled.overlay_eases.iter().any(|ease| {
            ease.overlay_index == 0 && ease.from.rot_z_deg.is_some() && ease.to.rot_z_deg.is_some()
        }));
        assert!(compiled.overlay_eases.iter().any(|ease| {
            ease.overlay_index == 0 && ease.from.visible.is_some() && ease.to.visible.is_some()
        }));
    }

    #[test]
    fn compile_song_lua_clears_update_function_with_nil() {
        let song_dir = test_dir("set-update-function-clear");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        Name="Panel",
        OnCommand=function(self)
            self:SetUpdateFunction(function(actor)
                actor:x(99)
                mod_actions = {
                    {1, "should-not-run", true},
                }
            end)
            self:SetUpdateFunction(nil)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "SetUpdateFunction Clear Song"),
        )
        .unwrap();
        assert!(compiled.messages.is_empty());
        assert_eq!(compiled.overlays.len(), 1);
        assert_eq!(compiled.overlays[0].initial_state.x, 0.0);
    }

    #[test]
    fn compile_song_lua_extracts_local_update_mod_actions() {
        let song_dir = test_dir("local-update-mod-actions");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local mod_actions = {
    {2, function()
        local p = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        if p then
            p:linear(1):x(SCREEN_CENTER_X + 24):z(3):zoom(0.5):rotationz(20)
        end
    end, true},
}
local curaction = 1
local mod_firstSeenBeat = 0

local domods = function()
    local beatupdate = GAMESTATE:GetSongBeat()
    if beatupdate > mod_firstSeenBeat + 0.1 then
        while curaction <= table.getn(mod_actions) and beatupdate >= mod_actions[curaction][1] do
            if type(mod_actions[curaction][2]) == "function" then
                mod_actions[curaction][2]()
            end
            curaction = curaction + 1
        end
    end
end

return Def.ActorFrame{
    InitCommand=function(self)
        table.sort(mod_actions, function(a, b) return a[1] < b[1] end)
    end,
    OnCommand=function(self)
        self:SetUpdateFunction(domods)
    end,
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Local Update Mod Actions"),
        )
        .unwrap();
        assert_eq!(compiled.info.unsupported_function_actions, 0);
        assert_eq!(compiled.player_actors[0].message_commands.len(), 1);
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].beat, 2.0);
        let block = &compiled.player_actors[0].message_commands[0].blocks[0];
        assert_eq!(block.delta.x, Some(344.0));
        assert_eq!(block.delta.z, Some(3.0));
        assert_eq!(block.delta.zoom, Some(0.5));
        assert_eq!(block.delta.rot_z_deg, Some(20.0));
    }

    #[test]
    fn compile_song_lua_guards_recursive_update_commands() {
        let song_dir = test_dir("recursive-update");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local runs = 0

return Def.ActorFrame{
    Def.ActorFrame{
        OnCommand=function(self)
            self:queuecommand("Update")
        end,
        UpdateCommand=function(self)
            runs = runs + 1
            mod_actions = {
                {runs, "LoopSafe", true},
            }
            self:sleep(1/60)
            self:queuecommand("Update")
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Recursive Update Song"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].beat, 1.0);
        assert_eq!(compiled.messages[0].message, "LoopSafe");
    }

    #[test]
    fn compile_song_lua_classifies_player_transform_function_eases() {
        let song_dir = test_dir("function-ease");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local target = nil
prefix_globals = {}

return Def.ActorFrame{
    InitCommand=function(self)
        prefix_globals.ease = {
            {3, 1, 320, 360, function(x) if target then target:x(x) end end, "len", ease.outQuad},
            {4, 1, 240, 210, function(x) if target then target:y(x) end end, "len", ease.outQuad},
            {5, 1, 0, -120, function(x) if target then target:z(x) end end, "len", ease.outQuad},
            {6, 2, 0, 20, function(x) if target then target:rotationx(x) end end, "len", ease.outQuad},
            {8, 2, 0, 10, function(x) if target then target:rotationz(x) end end, "len", ease.inOutQuad},
            {12, 1, 0, 0.15, function(x) if target then target:skewx(x) end end, "len", ease.outQuad},
            {13, 1, 0, 0.2, function(x) if target then target:skewy(x) end end, "len", ease.outQuad},
            {14, 1, 1, 0.75, function(x) if target then target:zoom(x) end end, "len", ease.outQuad},
            {15, 1, 1, 1.25, function(x) if target then target:zoomz(x) end end, "len", ease.outQuad},
        }
    end,
    Def.ActorFrame{
        OnCommand=function(self)
            self:queuecommand("BindTarget")
        end,
        BindTargetCommand=function(self)
            target = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Function Ease Song"),
        )
        .unwrap();
        assert_eq!(compiled.eases.len(), 9);
        assert_eq!(compiled.info.unsupported_function_eases, 0);
        assert!(matches!(
            compiled.eases[0].target,
            SongLuaEaseTarget::PlayerX
        ));
        assert!(matches!(
            compiled.eases[1].target,
            SongLuaEaseTarget::PlayerY
        ));
        assert!(matches!(
            compiled.eases[2].target,
            SongLuaEaseTarget::PlayerZ
        ));
        assert!(matches!(
            compiled.eases[3].target,
            SongLuaEaseTarget::PlayerRotationX
        ));
        assert!(matches!(
            compiled.eases[4].target,
            SongLuaEaseTarget::PlayerRotationZ
        ));
        assert!(matches!(
            compiled.eases[5].target,
            SongLuaEaseTarget::PlayerSkewX
        ));
        assert!(matches!(
            compiled.eases[6].target,
            SongLuaEaseTarget::PlayerSkewY
        ));
        assert!(matches!(
            compiled.eases[7].target,
            SongLuaEaseTarget::PlayerZoom
        ));
        assert!(matches!(
            compiled.eases[8].target,
            SongLuaEaseTarget::PlayerZoomZ
        ));
    }

    #[test]
    fn compile_song_lua_extracts_overlay_message_tweens() {
        let song_dir = test_dir("overlay");
        let entry = song_dir.join("default.lua");
        let overlay_dir = song_dir.join("gfx");
        fs::create_dir_all(&overlay_dir).unwrap();
        fs::write(
            overlay_dir.join("door.png"),
            b"not-an-image-but-good-enough-for-parser",
        )
        .unwrap();
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Sprite{
        Name="door",
        Texture="gfx/door.png",
        OnCommand=function(self)
            self:diffusealpha(0)
            self:xy(SCREEN_CENTER_X, SCREEN_CENTER_Y)
            self:stretchto(0, 0, SCREEN_WIDTH, SCREEN_HEIGHT)
            self:cropright(0.5)
        end,
        SlideDoorMessageCommand=function(self)
            self:x(0)
            self:diffusealpha(1)
            self:linear(0.3)
            self:x(SCREEN_CENTER_X)
        end,
    }
}
"#,
        )
        .unwrap();

        let compiled =
            test_compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "Overlay"))
                .unwrap();
        assert_eq!(compiled.overlays.len(), 1);
        let overlay = &compiled.overlays[0];
        assert_eq!(overlay.parent_index, None);
        assert!(matches!(
            overlay.kind,
            SongLuaOverlayKind::Sprite { ref texture_path, .. }
                if texture_path.ends_with("gfx/door.png")
        ));
        assert_eq!(overlay.initial_state.diffuse[3], 0.0);
        assert_eq!(overlay.initial_state.x, 320.0);
        assert_eq!(overlay.initial_state.y, 240.0);
        assert_eq!(overlay.initial_state.cropright, 0.5);
        assert_eq!(
            overlay.initial_state.stretch_rect,
            Some([0.0, 0.0, 640.0, 480.0])
        );
        assert_eq!(overlay.message_commands.len(), 1);
        assert_eq!(overlay.message_commands[0].message, "SlideDoor");
        assert_eq!(overlay.message_commands[0].blocks.len(), 2);
        assert_eq!(overlay.message_commands[0].blocks[0].delta.x, Some(0.0));
        assert_eq!(
            overlay.message_commands[0].blocks[0].delta.diffuse.unwrap()[3],
            1.0
        );
        assert_eq!(overlay.message_commands[0].blocks[1].duration, 0.3);
        assert_eq!(overlay.message_commands[0].blocks[1].delta.x, Some(320.0));
    }

    #[test]
    fn compile_song_lua_supports_spring_bounce_and_stoptweening_commands() {
        let song_dir = test_dir("overlay-spring-bounce");
        let entry = song_dir.join("default.lua");
        let overlay_dir = song_dir.join("gfx");
        fs::create_dir_all(&overlay_dir).unwrap();
        fs::write(
            overlay_dir.join("door.png"),
            b"not-an-image-but-good-enough-for-parser",
        )
        .unwrap();
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Sprite{
        Name="door",
        Texture="gfx/door.png",
        BounceDoorMessageCommand=function(self)
            self:stoptweening()
            self:bouncebegin(0.2):diffusealpha(0.5)
            self:bounceend(0.25):diffusealpha(1)
            self:spring(0.5):x(SCREEN_CENTER_X)
        end,
    }
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Overlay Spring Bounce"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 1);
        let overlay = &compiled.overlays[0];
        assert_eq!(overlay.message_commands.len(), 1);
        assert_eq!(overlay.message_commands[0].message, "BounceDoor");
        assert_eq!(overlay.message_commands[0].blocks.len(), 3);
        assert_eq!(
            overlay.message_commands[0].blocks[0].easing.as_deref(),
            Some("inBounce")
        );
        assert_eq!(overlay.message_commands[0].blocks[0].duration, 0.2);
        assert_eq!(
            overlay.message_commands[0].blocks[0].delta.diffuse.unwrap()[3],
            0.5
        );
        assert_eq!(
            overlay.message_commands[0].blocks[1].easing.as_deref(),
            Some("outBounce")
        );
        assert_eq!(overlay.message_commands[0].blocks[1].start, 0.2);
        assert_eq!(overlay.message_commands[0].blocks[1].duration, 0.25);
        assert_eq!(
            overlay.message_commands[0].blocks[1].delta.diffuse.unwrap()[3],
            1.0
        );
        assert_eq!(
            overlay.message_commands[0].blocks[2].easing.as_deref(),
            Some("outElastic")
        );
        assert_eq!(overlay.message_commands[0].blocks[2].start, 0.45);
        assert_eq!(overlay.message_commands[0].blocks[2].duration, 0.5);
        assert_eq!(overlay.message_commands[0].blocks[2].delta.x, Some(320.0));
    }

    #[test]
    fn compile_song_lua_tracks_tween_time_left_during_capture() {
        let song_dir = test_dir("tween-time-left");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            local before = self:GetTweenTimeLeft()
            self:sleep(0.5)
            local after_sleep = self:GetTweenTimeLeft()
            self:linear(0.25):x(10)
            local after_linear = self:GetTweenTimeLeft()
            self:stoptweening()
            local after_stop = self:GetTweenTimeLeft()
            self:bounceend(0.125):diffusealpha(0.5)
            local after_bounce = self:GetTweenTimeLeft()
            mod_actions = {{
                1,
                string.format(
                    "%.2f:%.2f:%.2f:%.2f:%.3f",
                    before,
                    after_sleep,
                    after_linear,
                    after_stop,
                    after_bounce
                ),
                true,
            }}
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Tween Time Left"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "0.00:0.50:0.75:0.00:0.125");
        assert_eq!(compiled.overlays.len(), 1);
        assert_eq!(compiled.overlays[0].initial_state.x, 10.0);
        assert_eq!(compiled.overlays[0].initial_state.diffuse[3], 0.5);
    }

    #[test]
    fn compile_song_lua_hurrytweening_scales_capture_timeline() {
        let song_dir = test_dir("hurrytweening-timeline");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        Name="Panel",
        HurryMessageCommand=function(self)
            self:sleep(1):linear(1):x(20)
            self:linear(2):y(40)
            self:hurrytweening(2)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Hurrytweening Timeline"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 1);
        assert_eq!(compiled.overlays[0].message_commands.len(), 1);
        let command = &compiled.overlays[0].message_commands[0];
        assert_eq!(command.message, "Hurry");
        assert_eq!(command.blocks.len(), 2);
        assert_eq!(command.blocks[0].start, 0.5);
        assert_eq!(command.blocks[0].duration, 0.5);
        assert_eq!(command.blocks[0].delta.x, Some(20.0));
        assert_eq!(command.blocks[1].start, 1.0);
        assert_eq!(command.blocks[1].duration, 1.0);
        assert_eq!(command.blocks[1].delta.y, Some(40.0));
    }

    #[test]
    fn compile_song_lua_finishtweening_collapses_queued_tweens() {
        let song_dir = test_dir("finishtweening-collapse");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        Name="Panel",
        FinishMessageCommand=function(self)
            self:linear(1):x(10):diffusealpha(0.5)
            self:sleep(0.5):decelerate(1):y(20)
            self:finishtweening()
            self:linear(0.25):zoom(2)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Finishtweening Collapse"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 1);
        assert_eq!(compiled.overlays[0].message_commands.len(), 1);
        let command = &compiled.overlays[0].message_commands[0];
        assert_eq!(command.message, "Finish");
        assert_eq!(command.blocks.len(), 2);
        assert_eq!(command.blocks[0].start, 0.0);
        assert_eq!(command.blocks[0].duration, 0.0);
        assert_eq!(command.blocks[0].easing, None);
        assert_eq!(command.blocks[0].delta.x, Some(10.0));
        assert_eq!(command.blocks[0].delta.y, Some(20.0));
        assert_eq!(command.blocks[0].delta.diffuse.unwrap()[3], 0.5);
        assert_eq!(command.blocks[1].start, 0.0);
        assert_eq!(command.blocks[1].duration, 0.25);
        assert_eq!(command.blocks[1].easing.as_deref(), Some("linear"));
        assert_eq!(command.blocks[1].delta.zoom, Some(2.0));
    }

    #[test]
    fn compile_song_lua_stoptweening_clears_queued_tweens() {
        let song_dir = test_dir("stoptweening-clear");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        Name="Panel",
        StopMessageCommand=function(self)
            self:linear(1):x(10)
            self:sleep(0.5):decelerate(1):y(20)
            self:stoptweening()
            self:linear(0.25):zoom(2)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Stoptweening Clear"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 1);
        assert_eq!(compiled.overlays[0].message_commands.len(), 1);
        let command = &compiled.overlays[0].message_commands[0];
        assert_eq!(command.message, "Stop");
        assert_eq!(command.blocks.len(), 1);
        assert_eq!(command.blocks[0].start, 0.0);
        assert_eq!(command.blocks[0].duration, 0.25);
        assert_eq!(command.blocks[0].easing.as_deref(), Some("linear"));
        assert_eq!(command.blocks[0].delta.x, None);
        assert_eq!(command.blocks[0].delta.y, None);
        assert_eq!(command.blocks[0].delta.zoom, Some(2.0));
    }

    #[test]
    fn compile_song_lua_exposes_named_children_and_duplicate_groups() {
        let song_dir = test_dir("actor-children");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    OnCommand=function(self)
        local count = 0
        local children = self:GetChildren()
        for _name, _child in pairs(children) do
            count = count + 1
        end
        local panel = children.Panel
        local lines = self:GetChild("Line")
        mod_actions = {
            {
                1,
                string.format("%d:%s:%d", count, panel and panel:GetName() or "nil", type(lines) == "table" and #lines or 0),
                true,
            },
        }
    end,
    Def.ActorFrame{ Name="Panel" },
    Def.Quad{ Name="Line" },
    Def.Quad{ Name="Line" },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor Children"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "2:Panel:2");
    }

    #[test]
    fn compile_song_lua_skips_failing_overlay_message_commands() {
        let song_dir = test_dir("overlay-message-error");
        let entry = song_dir.join("default.lua");
        let overlay_dir = song_dir.join("gfx");
        fs::create_dir_all(&overlay_dir).unwrap();
        fs::write(
            overlay_dir.join("door.png"),
            b"not-an-image-but-good-enough-for-parser",
        )
        .unwrap();
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Sprite{
        Name="door",
        Texture="gfx/door.png",
        BreakMeMessageCommand=function(self)
            local broken = nil
            broken:GetName()
        end,
    }
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Overlay Message Error"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 1);
        assert!(compiled.overlays[0].message_commands.is_empty());
        assert_eq!(compiled.info.skipped_message_command_captures.len(), 1);
        assert!(
            compiled.info.skipped_message_command_captures[0].contains("BreakMeMessageCommand")
        );
    }

    #[test]
    fn compile_song_lua_captures_message_commands_with_default_params() {
        let song_dir = test_dir("message-command-default-params");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        LifeChangedMessageCommand=function(self, params)
            if params.Player == PLAYER_1 then
                self:playcommand("ChangeSize", {CropAmount=(1 - params.LifeMeter:GetLife())})
            end
        end,
        ChangeSizeCommand=function(self, params)
            self:smooth(0.2)
            self:croptop(params.CropAmount)
        end,
    },
    Def.BitmapText{
        Font="Common Normal",
        Text="",
        ExCountsChangedMessageCommand=function(self, params)
            if params.Player == PLAYER_1 then
                self:x(params.ActualPossible)
            end
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Message Command Default Params"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 2);
        assert_eq!(compiled.overlays[0].message_commands.len(), 1);
        assert_eq!(
            compiled.overlays[0].message_commands[0].message,
            "LifeChanged"
        );
        assert_eq!(compiled.overlays[0].message_commands[0].blocks.len(), 1);
        assert_eq!(
            compiled.overlays[0].message_commands[0].blocks[0].duration,
            0.2
        );
        assert_eq!(
            compiled.overlays[0].message_commands[0].blocks[0]
                .delta
                .croptop,
            Some(0.5)
        );
        assert_eq!(compiled.overlays[1].message_commands.len(), 1);
        assert_eq!(
            compiled.overlays[1].message_commands[0].blocks[0].delta.x,
            Some(1.0)
        );
    }

    #[test]
    fn compile_song_lua_runs_messageman_broadcast_during_startup() {
        let song_dir = test_dir("broadcast-startup");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    OnCommand=function(self)
        assert(MESSAGEMAN:SetLogging(true) == MESSAGEMAN)
        assert(MESSAGEMAN.SetLogging(MESSAGEMAN, false) == MESSAGEMAN)
        assert(MESSAGEMAN:Broadcast("ProxyStart") == MESSAGEMAN)
    end,
    Def.Quad{
        InitCommand=function(self)
            self:visible(false)
            self:zoomto(12, 18)
        end,
        ProxyStartMessageCommand=function(self)
            self:visible(true)
            self:x(42)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Broadcast Startup"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 1);
        assert_eq!(compiled.overlays[0].initial_state.x, 42.0);
        assert!(compiled.overlays[0].initial_state.visible);
        assert_eq!(compiled.overlays[0].initial_state.size, Some([12.0, 18.0]));
    }

    #[test]
    fn compile_song_lua_passes_messageman_broadcast_params() {
        let song_dir = test_dir("broadcast-params");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    OnCommand=function(self)
        MESSAGEMAN:Broadcast("Judgment", {
            Player=PLAYER_1,
            TapNoteScore="TapNoteScore_W1",
            FirstTrack=3,
        })
    end,
    Def.Quad{
        InitCommand=function(self)
            self:visible(false)
        end,
        JudgmentMessageCommand=function(self, params)
            if params.Player == PLAYER_1 and params.TapNoteScore == "TapNoteScore_W1" then
                self:visible(true)
                self:x(params.FirstTrack * 10)
            end
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Broadcast Params"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 1);
        assert!(compiled.overlays[0].initial_state.visible);
        assert_eq!(compiled.overlays[0].initial_state.x, 30.0);
    }

    #[test]
    fn compile_song_lua_shapes_judgment_broadcast_tap_notes() {
        let song_dir = test_dir("broadcast-judgment-tap-notes");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    OnCommand=function(self)
        MESSAGEMAN:Broadcast("Judgment", {
            Player=PLAYER_1,
            TapNoteScore="TapNoteScore_Miss",
            TapNoteOffset=-0.02,
            Notes={
                [2]={ TapNoteType="TapNoteType_HoldHead", Held=true },
            },
        })
    end,
    Def.BitmapText{
        Font="Common Normal",
        Text="",
        JudgmentMessageCommand=function(self, params)
            for col,tapnote in pairs(params.Notes) do
                local result = tapnote:GetTapNoteResult()
                self:settext(table.concat({
                    tostring(col),
                    ToEnumShortString(tapnote:GetTapNoteType()),
                    tostring(result:GetHeld()),
                    result:GetTapNoteScore(),
                    string.format("%.2f", result:GetTapNoteOffset()),
                    tapnote:GetPlayerNumber(),
                    tostring(TapNoteType:Reverse()[tapnote:GetTapNoteType()] ~= nil),
                }, "|"))
            end
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Broadcast Judgment Tap Notes"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 1);
        assert!(matches!(
            compiled.overlays[0].kind,
            SongLuaOverlayKind::BitmapText { ref text, .. }
                if text.as_ref()
                    == "2|HoldHead|true|TapNoteScore_Miss|-0.02|PlayerNumber_P1|true"
        ));
    }

    #[test]
    fn compile_song_lua_respects_context_screen_dimensions() {
        let song_dir = test_dir("overlay-screen-dims");
        let entry = song_dir.join("default.lua");
        let overlay_dir = song_dir.join("gfx");
        fs::create_dir_all(&overlay_dir).unwrap();
        fs::write(
            overlay_dir.join("panel.png"),
            b"not-an-image-but-good-enough-for-parser",
        )
        .unwrap();
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="gfx/panel.png",
        OnCommand=function(self)
            self:xy(SCREEN_CENTER_X, SCREEN_CENTER_Y)
            self:stretchto(0, 0, SCREEN_WIDTH, SCREEN_HEIGHT)
        end,
    }
}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Overlay");
        context.screen_width = 854.0;
        context.screen_height = 480.0;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        let overlay = &compiled.overlays[0];

        assert_eq!(compiled.screen_width, 854.0);
        assert_eq!(compiled.screen_height, 480.0);
        assert_eq!(overlay.initial_state.x, 427.0);
        assert_eq!(overlay.initial_state.y, 240.0);
        assert_eq!(
            overlay.initial_state.stretch_rect,
            Some([0.0, 0.0, 854.0, 480.0])
        );
    }

    #[test]
    fn compile_song_lua_exposes_display_compat_globals() {
        let song_dir = test_dir("display-compat");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
mod_actions = {
    {
        1,
        string.format(
            "%d:%d:%s:%s",
            DISPLAY:GetDisplayWidth(),
            DISPLAY:GetDisplayHeight(),
            tostring(DISPLAY.SupportsRenderToTexture ~= nil),
            tostring(DISPLAY:SupportsRenderToTexture())
        ),
        true,
    },
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Display Compat");
        context.screen_width = 854.0;
        context.screen_height = 480.0;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();

        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "854:480:true:true");
    }

    #[test]
    fn compile_song_lua_exposes_display_specs_shape() {
        let song_dir = test_dir("display-specs-shape");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local specs = DISPLAY:GetDisplaySpecs()
local spec = specs[1]
local mode = spec:GetCurrentMode()
local modes = spec:GetSupportedModes()
mod_actions = {
    {
        1,
        string.format(
            "%d:%s:%s:%s:%d:%d:%d:%d:%s:%s:%s",
            #specs,
            spec:GetId(),
            spec:GetName(),
            tostring(spec:IsVirtual()),
            mode:GetWidth(),
            mode:GetHeight(),
            mode:GetRefreshRate(),
            #modes,
            tostring(modes[1] == mode),
            tostring(tostring(specs):find("DisplaySpecs") ~= nil),
            tostring(DISPLAY:SupportsFullscreenBorderlessWindow())
        ),
        true,
    },
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Display Specs Shape");
        context.screen_width = 1366.0;
        context.screen_height = 768.0;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();

        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "1:Default:Default Display:false:1366:768:60:1:true:true:false"
        );
    }

    #[test]
    fn compile_song_lua_exposes_date_compat_globals() {
        let song_dir = test_dir("date-compat");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
mod_actions = {
    {
        1,
        string.format(
            "%d:%d:%d:%d:%d:%d",
            Year(),
            MonthOfYear(),
            DayOfMonth(),
            Hour(),
            Minute(),
            Second()
        ),
        true,
    },
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Date Compat"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        let parts = compiled.messages[0]
            .message
            .split(':')
            .map(|value| value.parse::<i32>().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(parts.len(), 6);
        let now = Local::now();
        assert_eq!(parts[0], now.year());
        assert_eq!(parts[1], now.month0() as i32);
        assert_eq!(parts[2], now.day() as i32);
        assert!((0..=23).contains(&parts[3]));
        assert!((0..=59).contains(&parts[4]));
        assert!((0..=59).contains(&parts[5]));
    }

    #[test]
    fn compile_song_lua_exposes_charman_compat_helpers() {
        let song_dir = test_dir("charman-compat");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local characters = CHARMAN:GetAllCharacters()
assert(type(characters) == "table")
assert(#characters == 0)
assert(CHARMAN:GetCharacterCount() == 0)
assert(CHARMAN:GetCharacter("unused") == nil)
assert(CHARMAN:GetDefaultCharacter() == nil)
assert(CHARMAN:GetRandomCharacter() == nil)

mod_actions = {
    {1, string.format("%d:%d:%s", #characters, CHARMAN:GetCharacterCount(), tostring(CHARMAN:GetRandomCharacter())), true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Character Manager Compat"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "0:0:nil");
    }

    #[test]
    fn compile_song_lua_exposes_course_trail_and_song_position_helpers() {
        let song_dir = test_dir("course-trail-position-helpers");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local course = GAMESTATE:GetCurrentCourse()
local trail = GAMESTATE:GetCurrentTrail(PLAYER_1)
local entries = trail:GetTrailEntries()
local entry = trail:GetTrailEntry(0)
local pos = GAMESTATE:GetSongPosition()
local player_pos = GAMESTATE:GetPlayerState(PLAYER_1):GetSongPosition()

assert(course:GetDisplayFullTitle() == "Course Trail Position")
assert(course:GetCourseDir():match("compat%-course%.crs$") ~= nil)
assert(course:GetCourseType() == "CourseType_Nonstop")
assert(course:GetEstimatedNumStages() == 1)
assert(course:AllSongsAreFixed())
assert(course:IsAutogen() == false)
assert(course:IsEndless() == false)
assert(#course:GetCourseEntries() == 1)
assert(course:GetAllTrails()[1] == trail)
assert(course:GetTrail("StepsType_Dance_Single") == trail)

assert(#entries == 1)
assert(entries[1] == entry)
assert(entry:GetSong() == GAMESTATE:GetCurrentSong())
assert(entry:GetSteps() == GAMESTATE:GetCurrentSteps(PLAYER_1))
assert(entry:GetCourseEntryType() == "CourseEntryType_Fixed")
assert(entry:IsSecret() == false)
assert(trail:GetStepsType() == "StepsType_Dance_Single")
assert(trail:GetDisplayBpms()[1] == 120)
assert(pos:GetMusicSeconds() == pos:GetMusicSecondsVisible())
assert(pos:GetSongBeat() == pos:GetSongBeatVisible())
assert(pos:GetCurBPS() > 0)
assert(player_pos:GetCurBPS() == pos:GetCurBPS())

mod_actions = {
    {1, table.concat({
        course:GetDisplayFullTitle(),
        tostring(#course:GetCourseEntries()),
        tostring(#entries),
        tostring(entry:GetSteps():GetMeter()),
        tostring(player_pos:GetMusicSecondsVisible()),
    }, "|"), true},
    {2, function()
        GAMESTATE:SetCurrentTrail(PLAYER_1, trail)
    end, true},
}

return Def.ActorFrame{
    OnCommand=function(self)
        self:LoadFromSong(GAMESTATE:GetCurrentSong())
        self:LoadFromCourse(course)
    end,
}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Course Trail Position");
        context.song_display_bpms = [120.0, 180.0];
        context.players[0].display_bpms = [120.0, 180.0];
        context.players[0].difficulty = SongLuaDifficulty::Hard;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.info.unsupported_function_actions, 0);
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "Course Trail Position|1|1|10|0"
        );
    }

    #[test]
    fn compile_song_lua_exposes_song_and_steps_display_bpms() {
        let song_dir = test_dir("display-bpms");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local song_bpms = GAMESTATE:GetCurrentSong():GetDisplayBpms()
local step_bpms = GAMESTATE:GetCurrentSteps(PLAYER_1):GetDisplayBpms()
mod_actions = {
    {
        1,
        string.format(
            "%s:%d:%d:%d:%d",
            GAMESTATE:GetCurrentSong():GetDisplayMainTitle(),
            song_bpms[1],
            song_bpms[2],
            step_bpms[1],
            step_bpms[2]
        ),
        true,
    },
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Display BPMs");
        context.song_display_bpms = [120.0, 180.0];
        context.players[0].display_bpms = [150.0, 200.0];
        let compiled = test_compile_song_lua(&entry, &context).unwrap();

        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "Display BPMs:120:180:150:200");
    }

    #[test]
    fn compile_song_lua_exposes_timing_bpm_segments() {
        let song_dir = test_dir("timing-bpm-segments");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local song_timing = GAMESTATE:GetCurrentSong():GetTimingData()
local steps_timing = GAMESTATE:GetCurrentSteps(PLAYER_1):GetTimingData()
local song_bpms = song_timing:GetBPMs()
local step_bpms = steps_timing:GetBPMs()

mod_actions = {{
    1,
    string.format(
        "%s:%d:%d:%s:%d:%d",
        tostring(song_timing:HasBPMChanges()),
        #song_bpms,
        song_bpms[2],
        tostring(steps_timing:HasBPMChanges()),
        #step_bpms,
        step_bpms[1]
    ),
    true,
}}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Timing BPM Segments");
        context.song_display_bpms = [120.0, 180.0];
        context.players[0].display_bpms = [150.0, 150.0];
        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "true:2:180:false:1:150");
    }

    #[test]
    fn compile_song_lua_exposes_song_steps_type_selectors() {
        let song_dir = test_dir("song-steps-type-selectors");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local song = GAMESTATE:GetCurrentSong()
local steps_type = GAMESTATE:GetCurrentStyle():GetStepsType()
local all_steps = song:GetAllSteps()
local single_steps = song:GetStepsByStepsType(steps_type)
local pump_steps = song:GetStepsByStepsType("StepsType_Pump_Single")
local hard_steps = song:GetOneSteps(steps_type, "Difficulty_Hard")
local edit_steps = song:GetOneSteps("dance-single", "Edit")

mod_actions = {{
    1,
    string.format(
        "%d:%d:%d:%s:%d:%s:%s:%s:%s:%s",
        #all_steps,
        #single_steps,
        #pump_steps,
        hard_steps:GetDifficulty(),
        hard_steps:GetMeter(),
        edit_steps:GetDifficulty(),
        tostring(song:HasStepsType(steps_type)),
        tostring(song:HasStepsTypeAndDifficulty(steps_type, "Expert")),
        tostring(song:HasEdits(steps_type)),
        tostring(song:GetOneSteps("StepsType_Pump_Single", "Hard") == nil)
    ),
    true,
}}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Song Steps Type Selectors"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "6:6:0:Difficulty_Hard:10:Difficulty_Edit:true:true:true:true"
        );
    }

    #[test]
    fn compile_song_lua_exposes_song_options_object_music_rate() {
        let song_dir = test_dir("song-options-object");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local so = GAMESTATE:GetSongOptionsObject("ModsLevel_Song")
local before = so:MusicRate()
so:MusicRate(0.75)
mod_actions = {
    {1, string.format("%.2f:%.2f", before, so:MusicRate()), true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Song Options Object");
        context.song_music_rate = 1.5;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "1.50:0.75");
    }

    #[test]
    fn compile_song_lua_exposes_song_options_string_music_rate() {
        let song_dir = test_dir("song-options-string");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
mod_actions = {
    {1, GAMESTATE:GetSongOptions("ModsLevel_Song"), true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Song Options String");
        context.song_music_rate = 1.25;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "1.25xMusic");
    }

    #[test]
    fn compile_song_lua_exposes_save_your_tears_compat_helpers() {
        let song_dir = test_dir("save-your-tears-compat");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    OnCommand=function(self)
        local steps = GAMESTATE:GetCurrentSong():GetStepsByStepsType("StepsType_Dance_Single")
        GAMESTATE:SetCurrentSteps(PLAYER_1, steps[2])
        SCREENMAN:SetNewScreen("ScreenGameplay")
        local ps = GAMESTATE:GetPlayerState(PLAYER_1)
        ps:SetPlayerOptions("ModsLevel_Song", "1x, Overhead")
        mod_actions = {
            {1, string.format("%d:%s", #steps, ps:GetPlayerOptionsString("ModsLevel_Song")), true},
        }
    end,
    Def.Sound{
        File="thunder.ogg",
        OnCommand=function(self)
            self:play():pause():stop():playforplayer(PLAYER_1):load("rain.ogg"):volume(0.5)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Save Your Tears Compat"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "6:1x, Overhead");
    }

    #[test]
    fn compile_song_lua_extracts_sound_actor_assets() {
        let song_dir = test_dir("sound-actor-assets");
        fs::write(song_dir.join("hit.ogg"), b"not decoded during compile").unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
mod_actions = {
    {1, "Ding", true},
}
return Def.ActorFrame{
    Def.Sound{
        Name="HitSound",
        File="hit.ogg",
        OnCommand=function(self)
            self:play()
        end,
        DingMessageCommand=function(self)
            self:sleep(0.25):playforplayer(PLAYER_1)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Sound Actor Assets"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 1);
        let SongLuaOverlayKind::Sound { sound_path } = &compiled.overlays[0].kind else {
            panic!("expected sound overlay");
        };
        assert_eq!(sound_path, &song_dir.join("hit.ogg"));
        assert!(
            compiled
                .messages
                .iter()
                .any(|event| event.message == SONG_LUA_STARTUP_MESSAGE && event.beat == 0.0)
        );
        let startup = compiled.overlays[0]
            .message_commands
            .iter()
            .find(|command| command.message == SONG_LUA_STARTUP_MESSAGE)
            .expect("expected startup sound command");
        assert_eq!(startup.blocks[0].delta.sound_play, Some(true));
        let ding = compiled.overlays[0]
            .message_commands
            .iter()
            .find(|command| command.message == "Ding")
            .expect("expected Ding sound command");
        assert_eq!(ding.blocks[0].start, 0.25);
        assert_eq!(ding.blocks[0].delta.sound_play, Some(true));
    }

    #[test]
    fn compile_song_lua_extracts_sound_load_assets() {
        let song_dir = test_dir("sound-load-assets");
        for name in ["initial.ogg", "lower.ogg", "upper.ogg"] {
            fs::write(song_dir.join(name), b"not decoded during compile").unwrap();
        }
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Sound{
        Name="LowerLoad",
        File="initial.ogg",
        OnCommand=function(self)
            self:load("lower.ogg")
        end,
    },
    Def.Sound{
        Name="UpperLoad",
        OnCommand=function(self)
            self:Load("upper.ogg")
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Sound Load Assets"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 2);
        let SongLuaOverlayKind::Sound { sound_path } = &compiled.overlays[0].kind else {
            panic!("expected lower sound overlay");
        };
        assert_eq!(sound_path, &song_dir.join("lower.ogg"));
        let SongLuaOverlayKind::Sound { sound_path } = &compiled.overlays[1].kind else {
            panic!("expected upper sound overlay");
        };
        assert_eq!(sound_path, &song_dir.join("upper.ogg"));
    }

    #[test]
    fn compile_song_lua_extracts_sound_singleton_assets() {
        let song_dir = test_dir("sound-singleton-assets");
        fs::write(song_dir.join("effect.ogg"), b"not decoded during compile").unwrap();
        fs::write(song_dir.join("music.wav"), b"not decoded during compile").unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
SOUND:PlayOnce("effect.ogg")
SOUND:PlayOnce("missing.ogg")
SOUND:PlayMusicPart("music.wav", 0, 1, 0, 0)
assert(SOUND:PlayOnce("effect.ogg") == SOUND)
assert(SOUND:PlayMusicPart("music.wav", 0, 1, 0, 0, false, true, true) == SOUND)
assert(SOUND:DimMusic(0.5, 1.0) == SOUND)
assert(SOUND:StopMusic() == SOUND)
assert(SOUND:PlayAnnouncer("common start") == SOUND)
assert(SOUND:GetPlayerBalance(PLAYER_1) == 0)
assert(SOUND:IsTimingDelayed() == false)

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Sound Singleton Assets"),
        )
        .unwrap();
        assert_eq!(
            compiled.sound_paths,
            vec![song_dir.join("effect.ogg"), song_dir.join("music.wav")]
        );
    }

    #[test]
    fn compile_song_lua_set_current_steps_updates_selected_steps() {
        let song_dir = test_dir("set-current-steps");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    OnCommand=function(self)
        local song_steps = GAMESTATE:GetCurrentSong():GetStepsByStepsType("StepsType_Dance_Single")
        local before = ToEnumShortString(GAMESTATE:GetCurrentSteps(PLAYER_1):GetDifficulty())
        GAMESTATE:SetCurrentSteps(PLAYER_1, song_steps[2])
        local after = GAMESTATE:GetCurrentSteps(PLAYER_1)
        local bpms = after:GetDisplayBpms()
        mod_actions = {
            {
                1,
                string.format(
                    "%s:%s:%d:%d:%s",
                    before,
                    ToEnumShortString(after:GetDifficulty()),
                    bpms[1],
                    bpms[2],
                    ToEnumShortString(GAMESTATE:GetCurrentSteps(PLAYER_2):GetDifficulty())
                ),
                true,
            },
        }
    end,
}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Set Current Steps");
        context.song_display_bpms = [120.0, 180.0];
        context.players[0].difficulty = SongLuaDifficulty::Challenge;
        context.players[0].display_bpms = [200.0, 240.0];
        context.players[1].difficulty = SongLuaDifficulty::Hard;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();

        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "Challenge:Easy:120:180:Hard");
    }

    #[test]
    fn compile_song_lua_supports_get_column_actors_alias() {
        let song_dir = test_dir("column-actors-alias");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    OnCommand=function(self)
        local nf = SCREENMAN:GetTopScreen():GetChild("PlayerP1"):GetChild("NoteField")
        mod_actions = {
            {1, tostring(#nf:get_column_actors()), true},
        }
    end,
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Column Actors Alias"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "4");
    }

    #[test]
    fn compile_song_lua_accepts_screen_transition_and_sm_helpers() {
        let song_dir = test_dir("screen-transition-sm");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    OnCommand=function(self)
        SM("hello")
        SCREENMAN:GetTopScreen():StartTransitioningScreen("SM_DoNextScreen")
        mod_actions = {
            {1, "ok", true},
        }
    end,
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Screen Transition"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "ok");
    }

    #[test]
    fn compile_song_lua_exposes_common_prefsmgr_preferences() {
        let song_dir = test_dir("prefsmgr-preferences");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
mod_actions = {
    {
        1,
        string.format(
            "%.4f:%d:%d:%s:%.2f:%.2f",
            PREFSMAN:GetPreference("DisplayAspectRatio"),
            PREFSMAN:GetPreference("DisplayWidth"),
            PREFSMAN:GetPreference("DisplayHeight"),
            tostring(string.find(string.lower(PREFSMAN:GetPreference("VideoRenderers")), "opengl") ~= nil),
            PREFSMAN:GetPreference("BGBrightness"),
            PREFSMAN:GetPreference("GlobalOffsetSeconds")
        ),
        true,
    },
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "PrefsMgr Preferences");
        context.screen_width = 1280.0;
        context.screen_height = 720.0;
        context.global_offset_seconds = 0.02;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "1.7778:1280:720:true:1.00:0.02"
        );
    }

    #[test]
    fn compile_song_lua_exposes_after_dark_runtime_helpers() {
        let song_dir = test_dir("after-dark-runtime-helpers");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local leaf = nil

return Def.ActorFrame{
    OnCommand=function(self)
        local spline = SCREENMAN:GetTopScreen():GetChild("PlayerP1"):GetChild("NoteField"):GetColumnActors()[1]:GetPosHandler():GetSpline()
        local polygonal = spline:SetPolygonal(true) ~= nil
        self:runcommandsonleaves(function(actor)
            actor:visible(false)
        end)
        mod_actions = {
            {1, string.format(
                "%s:%.2f:%s:%s",
                GAMESTATE:GetCurrentStyle():GetName(),
                GAMESTATE:GetSongBPS(),
                tostring(leaf:GetVisible()),
                tostring(polygonal)
            ), true},
        }
    end,
    Def.ActorFrame{
        Def.Quad{
            InitCommand=function(self)
                leaf = self
            end,
        },
    },
}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "After Dark Helpers");
        context.song_display_bpms = [120.0, 180.0];
        context.style_name = "double".to_string();
        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "double:3.00:false:true");
    }

    #[test]
    fn compile_song_lua_exposes_scale_helper() {
        let song_dir = test_dir("scale-helper");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local WideScale = function(AR4_3, AR16_9)
    local w = 480 * PREFSMAN:GetPreference("DisplayAspectRatio")
    return scale(w, 640, 854, AR4_3, AR16_9)
end

mod_actions = {
    {1, string.format("%.2f", WideScale(100, 200)), true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Scale Helper");
        context.screen_width = 1280.0;
        context.screen_height = 720.0;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "199.69");
    }

    #[test]
    fn compile_song_lua_exposes_difficulty_enum_globals() {
        let song_dir = test_dir("difficulty-enum");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
mod_actions = {
    {
        1,
        string.format(
            "%s:%s:%s:%s",
            ToEnumShortString(Difficulty[1]),
            ToEnumShortString(Difficulty[#Difficulty]),
            ToEnumShortString(GAMESTATE:GetCurrentSteps(PLAYER_1):GetDifficulty()),
            Difficulty[4]
        ),
        true,
    },
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Difficulty Enum");
        context.players[0].difficulty = SongLuaDifficulty::Hard;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();

        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "Beginner:Edit:Hard:Difficulty_Hard"
        );
    }

    #[test]
    fn compile_song_lua_exposes_gamestate_easiest_steps_difficulty() {
        let song_dir = test_dir("easiest-steps-difficulty");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
mod_actions = {
    {1, ToEnumShortString(GAMESTATE:GetEasiestStepsDifficulty()), true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Easiest Steps Difficulty");
        context.players[0].difficulty = SongLuaDifficulty::Hard;
        context.players[1].difficulty = SongLuaDifficulty::Medium;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();

        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "Medium");
    }

    #[test]
    fn compile_song_lua_supports_center_methods() {
        let song_dir = test_dir("actor-center-methods");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:CenterX()
            self:CenterY()
            self:Center()
            mod_actions = {
                {1, string.format("%.0f:%.0f", self:GetX(), self:GetY()), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Actor Center Methods");
        context.screen_width = 1280.0;
        context.screen_height = 720.0;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "640:360");
    }

    #[test]
    fn compile_song_lua_supports_hibernate_chain_method() {
        let song_dir = test_dir("actor-hibernate");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:hibernate(0):diffusealpha(0.25):sleep(1)
            mod_actions = {
                {1, string.format("%.2f", self:GetDiffuseAlpha()), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor Hibernate"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "0.25");
    }

    #[test]
    fn compile_song_lua_captures_hibernate_visibility_window() {
        let song_dir = test_dir("actor-hibernate-visibility");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        PulseMessageCommand=function(self)
            self:hibernate(0.5):diffusealpha(0.25)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor Hibernate Visibility"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 1);
        assert_eq!(compiled.overlays[0].message_commands.len(), 1);
        let command = &compiled.overlays[0].message_commands[0];
        assert_eq!(command.message, "Pulse");
        assert_eq!(command.blocks.len(), 2);
        assert_eq!(command.blocks[0].start, 0.0);
        assert_eq!(command.blocks[0].duration, 0.0);
        assert_eq!(command.blocks[0].delta.visible, Some(false));
        assert_eq!(command.blocks[1].start, 0.5);
        assert_eq!(command.blocks[1].duration, 0.0);
        assert_eq!(command.blocks[1].delta.visible, Some(true));
        assert_eq!(command.blocks[1].delta.diffuse.unwrap()[3], 0.25);
    }

    #[test]
    fn compile_song_lua_supports_fullscreen_method() {
        let song_dir = test_dir("actor-fullscreen");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:FullScreen():Center()
            mod_actions = {
                {1, string.format("%.0f:%.0f:%.0f:%.0f", self:GetX(), self:GetY(), self:GetWidth(), self:GetHeight()), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Actor FullScreen");
        context.screen_width = 1280.0;
        context.screen_height = 720.0;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "640:360:1280:720");
    }

    #[test]
    fn compile_song_lua_supports_additive_transform_methods() {
        let song_dir = test_dir("actor-additive-transforms");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:x(10):addx(5)
            self:y(20):addy(-3)
            self:z(4):addz(6)
            self:rotationx(15):addrotationx(5)
            self:rotationy(25):addrotationy(10)
            self:rotationz(45):addrotationz(90)
            mod_actions = {
                {1, string.format(
                    "%.0f:%.0f:%.0f:%.0f:%.0f:%.0f",
                    self:GetX(),
                    self:GetY(),
                    self:GetZ(),
                    self:GetRotationX(),
                    self:GetRotationY(),
                    self:GetRotationZ()
                ), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor Additive Transforms"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "15:17:10:20:35:135");
    }

    #[test]
    fn compile_song_lua_reads_sprite_image_dimensions() {
        let song_dir = test_dir("sprite-dimensions");
        let image_path = song_dir.join("panel.png");
        image::RgbaImage::new(10, 20).save(&image_path).unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            mod_actions = {
                {1, string.format("%.0f:%.0f", self:GetWidth(), self:GetHeight()), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Sprite Dimensions"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "10:20");
    }

    #[test]
    fn compile_song_lua_setstate_uses_sprite_sheet_cell_size() {
        let song_dir = test_dir("sprite-setstate");
        let image_path = song_dir.join("panel 4x3.png");
        image::RgbaImage::new(40, 30).save(&image_path).unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="panel 4x3.png",
        OnCommand=function(self)
            self:setstate(5)
            mod_actions = {
                {1, string.format("%.0f:%.0f", self:GetWidth(), self:GetHeight()), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Sprite SetState"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "10:10");
        assert_eq!(compiled.overlays.len(), 1);
        assert_eq!(
            compiled.overlays[0].initial_state.sprite_state_index,
            Some(5)
        );
        assert_eq!(compiled.overlays[0].initial_state.custom_texture_rect, None);
    }

    #[test]
    fn compile_song_lua_tracks_sprite_animation_state() {
        let song_dir = test_dir("sprite-animate");
        let image_path = song_dir.join("panel 4x3.png");
        image::RgbaImage::new(40, 30).save(&image_path).unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="panel 4x3.png",
        OnCommand=function(self)
            self:setstate(1):animate(true):SetAllStateDelays(0.5)
            mod_actions = {
                {1, string.format("%.0f:%.0f", self:GetWidth(), self:GetHeight()), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Sprite Animate"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "10:10");
        assert_eq!(compiled.overlays.len(), 1);
        let state = compiled.overlays[0].initial_state;
        assert!(state.sprite_animate);
        assert!(state.sprite_loop);
        assert_eq!(state.sprite_playback_rate, 1.0);
        assert_eq!(state.sprite_state_delay, 0.5);
        assert_eq!(state.sprite_state_index, Some(1));
        assert_eq!(state.custom_texture_rect, None);
    }

    #[test]
    fn compile_song_lua_loadactor_exposes_texture_proxy_methods() {
        let song_dir = test_dir("loadactor-texture-proxy");
        let image_path = song_dir.join("panel.png");
        image::RgbaImage::new(12, 34).save(&image_path).unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local loaded = nil

return Def.ActorFrame{
    LoadActor("panel.png")..{
        OnCommand=function(self)
            loaded = self
        end,
    },
    Def.Sprite{
        OnCommand=function(self)
            self:SetTexture(loaded:GetTexture())
            local texture = self:GetTexture()
            mod_actions = {
                {1, string.format(
                    "%s:%.0f:%.0f",
                    tostring(texture:GetPath():match("panel%.png$") ~= nil),
                    texture:GetSourceWidth(),
                    texture:GetSourceHeight()
                ), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "LoadActor Texture Proxy"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "true:12:34");
    }

    #[test]
    fn compile_song_lua_loadactor_resolves_extensionless_image() {
        let song_dir = test_dir("loadactor-image-no-ext");
        let lua_dir = song_dir.join("lua");
        fs::create_dir_all(&lua_dir).unwrap();
        let image_path = lua_dir.join("panel.png");
        image::RgbaImage::new(12, 34).save(&image_path).unwrap();
        let entry = lua_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local loaded = nil

return Def.ActorFrame{
    LoadActor("panel")..{
        OnCommand=function(self)
            loaded = self
        end,
    },
    Def.Sprite{
        OnCommand=function(self)
            local texture = loaded:GetTexture()
            mod_actions = {
                {1, string.format(
                    "%s:%.0f:%.0f",
                    tostring(texture:GetPath():match("panel%.png$") ~= nil),
                    texture:GetSourceWidth(),
                    texture:GetSourceHeight()
                ), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "LoadActor NoExt Image"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "true:12:34");
    }

    #[test]
    fn compile_song_lua_loadactor_resolves_extensionless_script() {
        let song_dir = test_dir("loadactor-script-no-ext");
        let lua_dir = song_dir.join("lua");
        fs::create_dir_all(&lua_dir).unwrap();
        fs::write(
            lua_dir.join("child.lua"),
            r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:SetSize(12, 34)
            mod_actions = {
                {1, string.format("%.0f:%.0f", self:GetWidth(), self:GetHeight()), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();
        let entry = lua_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    LoadActor("child"),
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "LoadActor NoExt Script"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "12:34");
    }

    #[test]
    fn compile_song_lua_loadactor_treats_binary_video_as_media() {
        let song_dir = test_dir("loadactor-video-media");
        let video_path = song_dir.join("clip.mp4");
        fs::write(&video_path, [0xff_u8, 0xd8, 0x00, 0x81]).unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    LoadActor("clip.mp4")..{
        OnCommand=function(self)
            local texture = self:GetTexture()
            mod_actions = {
                {1, string.format(
                    "%s:%s",
                    tostring(texture:GetPath():match("clip%.mp4$") ~= nil),
                    tostring(texture:GetSourceWidth() > 0 and texture:GetSourceHeight() > 0)
                ), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "LoadActor Video Media"),
        )
        .unwrap();
        assert!(compiled.messages.iter().any(|event| {
            event.message != SONG_LUA_STARTUP_MESSAGE && event.message == "true:true"
        }));
    }

    #[test]
    fn compile_song_lua_supports_sprite_decode_movie_methods() {
        let song_dir = test_dir("sprite-decode-movie");
        fs::write(song_dir.join("clip.mp4"), [0xff_u8, 0xd8, 0x00, 0x81]).unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    LoadActor("clip.mp4")..{
        OnCommand=function(self)
            local before = self:GetDecodeMovie()
            self:SetDecodeMovie(false)
            mod_actions = {
                {1, string.format("%s:%s", tostring(before), tostring(self:GetDecodeMovie())), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Sprite Decode Movie"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "true:false");
        assert_eq!(compiled.overlays.len(), 1);
        assert!(!compiled.overlays[0].initial_state.decode_movie);
    }

    #[test]
    fn compile_song_lua_loadactor_treats_binary_audio_as_media() {
        let song_dir = test_dir("loadactor-audio-media");
        let audio_path = song_dir.join("clip.ogg");
        fs::write(&audio_path, [0xff_u8, 0xd8, 0x00, 0x81]).unwrap();
        fs::write(song_dir.join("other.ogg"), b"not decoded during compile").unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    LoadActor("clip.ogg")..{
        OnCommand=function(self)
            self:play():pause():stop():load("other.ogg"):volume(0.5)
            mod_actions = {
                {1, string.format(
                    "%s:%s",
                    tostring(self.File == "other.ogg"),
                    tostring(self:GetTexture() == nil)
                ), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "LoadActor Audio Media"),
        )
        .unwrap();
        assert!(compiled.messages.iter().any(|event| {
            event.message != SONG_LUA_STARTUP_MESSAGE && event.message == "true:true"
        }));
        assert_eq!(compiled.overlays.len(), 1);
        let SongLuaOverlayKind::Sound { sound_path } = &compiled.overlays[0].kind else {
            panic!("expected sound overlay");
        };
        assert_eq!(sound_path, &song_dir.join("other.ogg"));
    }

    #[test]
    fn compile_song_lua_supports_animate_loop_rate_chain_methods() {
        let song_dir = test_dir("actor-animate-loop-rate");
        let image_path = song_dir.join("panel 4x3.png");
        image::RgbaImage::new(40, 30).save(&image_path).unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="panel 4x3.png",
        OnCommand=function(self)
            local texture = self:GetTexture()
            texture:loop(false):rate(1.5)
            self:setstate(2):position(0):play():pause():play():diffusealpha(0.2)
            mod_actions = {
                {1, string.format("%.2f:%d:%d", self:GetDiffuseAlpha(), self:GetNumStates(), texture:GetNumFrames()), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor Animate Loop Rate"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "0.20:12:12");
        assert_eq!(compiled.overlays.len(), 1);
        let state = compiled.overlays[0].initial_state;
        assert!(state.sprite_animate);
        assert!(!state.sprite_loop);
        assert_eq!(state.sprite_playback_rate, 1.5);
        assert_eq!(state.sprite_state_index, Some(0));
    }

    #[test]
    fn compile_song_lua_supports_sprite_load_and_text_compat_methods() {
        let song_dir = test_dir("sprite-load-text-compat");
        image::RgbaImage::new(10, 20)
            .save(song_dir.join("first.png"))
            .unwrap();
        image::RgbaImage::new(30, 40)
            .save(song_dir.join("second.png"))
            .unwrap();
        image::RgbaImage::new(50, 10)
            .save(song_dir.join("banner.png"))
            .unwrap();
        image::RgbaImage::new(90, 30)
            .save(song_dir.join("cached-banner.png"))
            .unwrap();
        image::RgbaImage::new(40, 40)
            .save(song_dir.join("sheet 2x2.png"))
            .unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
mod_actions = {}

return Def.ActorFrame{
    Def.Sprite{
        Texture="first.png",
        OnCommand=function(self)
            self:Load("second.png")
            self:LoadBanner("banner.png")
            self:LoadBackground("second.png")
            self:LoadFromCached("Banner", "sheet 2x2.png")
            self:SetAllStateDelays(0.25):SetSecondsIntoAnimation(0.6):SetEffectMode("Normal")
            local texture = self:GetTexture()
            mod_actions[#mod_actions + 1] = {
                1,
                string.format(
                    "%s:%s:%d:%.2f:%.0f:%.0f",
                    tostring(Sprite.LoadFromCached ~= nil),
                    tostring(texture:GetPath():match("sheet 2x2%.png$") ~= nil),
                    self:GetState(),
                    self:GetAnimationLengthSeconds(),
                    self:GetWidth(),
                    self:GetHeight()
                ),
                true,
            }
        end,
    },
    Def.Banner{
        OnCommand=function(self)
            self:LoadFromCachedBanner("cached-banner.png")
            local texture = self:GetTexture()
            mod_actions[#mod_actions + 1] = {
                1,
                string.format(
                    "%s:%s:%.0f:%.0f",
                    tostring(Sprite.LoadFromCachedBanner ~= nil),
                    tostring(texture:GetPath():match("cached%-banner%.png$") ~= nil),
                    self:GetWidth(),
                    self:GetHeight()
                ),
                true,
            }
        end,
    },
    Def.BitmapText{
        Font="Common Normal",
        Text="TEXT",
        OnCommand=function(self)
            self:strokecolor(color("0.2,0.3,0.4,0.5"))
                :max_dimension_use_zoom(true)
                :textglowmode("Stroke")
                :set_mult_attrs_with_diffuse(true)
            local stroke = self:getstrokecolor()
            mod_actions[#mod_actions + 1] = {
                1,
                string.format(
                    "%.1f:%.1f:%.1f:%.1f:%s",
                    stroke[1],
                    stroke[2],
                    stroke[3],
                    stroke[4],
                    tostring(self:get_mult_attrs_with_diffuse())
                ),
                true,
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Sprite Load Text Compat"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 3);
        assert_eq!(compiled.messages[0].message, "true:true:2:1.00:20:20");
        assert_eq!(compiled.messages[1].message, "true:true:90:30");
        assert_eq!(compiled.messages[2].message, "0.2:0.3:0.4:0.5:true");
        assert_eq!(compiled.overlays.len(), 3);
        assert!(matches!(
            compiled.overlays[0].kind,
            SongLuaOverlayKind::Sprite { ref texture_path, .. }
                if texture_path.ends_with("sheet 2x2.png")
        ));
        assert_eq!(
            compiled.overlays[0].initial_state.sprite_state_index,
            Some(2)
        );
        assert!(matches!(
            compiled.overlays[1].kind,
            SongLuaOverlayKind::Sprite { ref texture_path, .. }
                if texture_path.ends_with("cached-banner.png")
        ));
        assert!(matches!(
            compiled.overlays[2].kind,
            SongLuaOverlayKind::BitmapText {
                stroke_color: Some([0.2, 0.3, 0.4, 0.5]),
                ..
            }
        ));
        assert_eq!(
            compiled.overlays[2].initial_state.text_glow_mode,
            SongLuaTextGlowMode::Stroke
        );
        assert!(compiled.overlays[2].initial_state.mult_attrs_with_diffuse);
    }

    #[test]
    fn compile_song_lua_supports_banner_cached_load_aliases() {
        let song_dir = test_dir("banner-cached-load-aliases");
        image::RgbaImage::new(64, 24)
            .save(song_dir.join("rank-banner.png"))
            .unwrap();
        image::RgbaImage::new(120, 80)
            .save(song_dir.join("background.png"))
            .unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
mod_actions = {}

return Def.ActorFrame{
    Def.Banner{
        InitCommand=function(self)
            self:LoadFromCachedBanner("rank-banner.png")
            mod_actions[#mod_actions + 1] = {
                1,
                string.format("%s:%s:%d:%d", self:GetName(), tostring(Sprite.LoadFromCachedBanner ~= nil), self:GetWidth(), self:GetHeight()),
                true,
            }
        end,
    },
    Def.Sprite{
        InitCommand=function(self)
            Sprite.LoadFromCachedBackground(self, "background.png")
            mod_actions[#mod_actions + 1] = {
                1,
                string.format("%s:%d:%d", tostring(Sprite.LoadFromCachedBackground ~= nil), self:GetWidth(), self:GetHeight()),
                true,
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Banner Cached Load Aliases"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 2);
        assert_eq!(compiled.messages[0].message, ":true:64:24");
        assert_eq!(compiled.messages[1].message, "true:120:80");
        assert_eq!(compiled.overlays.len(), 2);
        assert!(matches!(
            compiled.overlays[0].kind,
            SongLuaOverlayKind::Sprite { ref texture_path, .. }
                if texture_path.ends_with("rank-banner.png")
        ));
        assert!(matches!(
            compiled.overlays[1].kind,
            SongLuaOverlayKind::Sprite { ref texture_path, .. }
                if texture_path.ends_with("background.png")
        ));
    }

    #[test]
    fn compile_song_lua_supports_song_and_course_sprite_loads() {
        let song_dir = test_dir("song-course-sprite-loads");
        image::RgbaImage::new(48, 16)
            .save(song_dir.join("banner.png"))
            .unwrap();
        image::RgbaImage::new(80, 60)
            .save(song_dir.join("background.png"))
            .unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
mod_actions = {}

local function note(name, sprite)
    local texture = sprite:GetTexture()
    mod_actions[#mod_actions + 1] = {
        1,
        string.format(
            "%s:%s:%d:%d:%s",
            name,
            texture:GetPath():match(name == "background" and "background%.png$" or "banner%.png$") ~= nil,
            sprite:GetWidth(),
            sprite:GetHeight(),
            tostring(Sprite.LoadFromSong ~= nil and Sprite.LoadFromCourse ~= nil and Sprite.LoadFromSongGroup ~= nil)
        ),
        true,
    }
end

return Def.ActorFrame{
    Def.Banner{
        OnCommand=function(self)
            self:LoadFromSong(GAMESTATE:GetCurrentSong())
            note("song", self)
        end,
    },
    Def.Sprite{
        OnCommand=function(self)
            self:LoadFromSongBackground(GAMESTATE:GetCurrentSong())
            note("background", self)
        end,
    },
    Def.Banner{
        OnCommand=function(self)
            self:LoadFromCourse(GAMESTATE:GetCurrentCourse())
            note("course", self)
        end,
    },
    Def.Banner{
        OnCommand=function(self)
            self:LoadFromSongGroup(GAMESTATE:GetCurrentSong():GetGroupName())
            note("group", self)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Song Course Sprite Loads"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 4);
        assert_eq!(compiled.messages[0].message, "song:true:48:16:true");
        assert_eq!(compiled.messages[1].message, "background:true:80:60:true");
        assert_eq!(compiled.messages[2].message, "course:true:48:16:true");
        assert_eq!(compiled.messages[3].message, "group:true:48:16:true");
        assert_eq!(compiled.overlays.len(), 4);
        assert!(matches!(
            compiled.overlays[0].kind,
            SongLuaOverlayKind::Sprite { ref texture_path, .. }
                if texture_path.ends_with("banner.png")
        ));
        assert!(matches!(
            compiled.overlays[1].kind,
            SongLuaOverlayKind::Sprite { ref texture_path, .. }
                if texture_path.ends_with("background.png")
        ));
        assert!(matches!(
            compiled.overlays[2].kind,
            SongLuaOverlayKind::Sprite { ref texture_path, .. }
                if texture_path.ends_with("banner.png")
        ));
        assert!(matches!(
            compiled.overlays[3].kind,
            SongLuaOverlayKind::Sprite { ref texture_path, .. }
                if texture_path.ends_with("banner.png")
        ));
    }

    #[test]
    fn compile_song_lua_supports_banner_luna_methods() {
        let song_dir = test_dir("banner-luna-methods");
        for (file_name, width, height) in [
            ("icon.png", 24, 24),
            ("card.png", 32, 20),
            ("unlock-banner.png", 64, 18),
            ("unlock-bg.png", 96, 54),
        ] {
            image::RgbaImage::new(width, height)
                .save(song_dir.join(file_name))
                .unwrap();
        }
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
mod_actions = {}

local character = {}
function character:GetIconPath() return "icon.png" end
function character:GetCardPath() return "card.png" end

local unlock = {}
function unlock:GetBannerFile() return "unlock-banner.png" end
function unlock:GetBackgroundFile() return "unlock-bg.png" end

local function note(name, sprite, pattern)
    local texture = sprite:GetTexture()
    mod_actions[#mod_actions + 1] = {
        1,
        string.format(
            "%s:%s:%s:%.2f:%d:%d",
            name,
            tostring(texture:GetPath():match(pattern) ~= nil),
            tostring(sprite:GetScrolling()),
            sprite:GetPercentScrolling(),
            sprite:GetWidth(),
            sprite:GetHeight()
        ),
        true,
    }
end

return Def.ActorFrame{
    Def.Banner{
        OnCommand=function(self)
            self:SetScrolling(true, 0.25)
            self:LoadFromSortOrder("SortOrder_Recent_P1")
            local texture = self:GetTexture()
            mod_actions[#mod_actions + 1] = {
                1,
                string.format(
                    "sort:%s:%.2f:%s:%s",
                    tostring(self:GetScrolling()),
                    self:GetPercentScrolling(),
                    tostring(texture:GetPath():match("__songlua_theme_path/G/Banner/Recent$") ~= nil),
                    tostring(Sprite.LoadFromSortOrder ~= nil and Sprite.SetScrolling ~= nil and Sprite.GetScrolling ~= nil and Sprite.GetPercentScrolling ~= nil)
                ),
                true,
            }
        end,
    },
    Def.Banner{
        OnCommand=function(self)
            self:LoadIconFromCharacter(character)
            note("icon", self, "icon%.png$")
        end,
    },
    Def.Banner{
        OnCommand=function(self)
            self:LoadCardFromCharacter(character)
            note("card", self, "card%.png$")
        end,
    },
    Def.Banner{
        OnCommand=function(self)
            self:LoadBannerFromUnlockEntry(unlock)
            note("unlock-banner", self, "unlock%-banner%.png$")
        end,
    },
    Def.Banner{
        OnCommand=function(self)
            self:LoadBackgroundFromUnlockEntry(unlock)
            note("unlock-bg", self, "unlock%-bg%.png$")
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Banner Luna Methods"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 5);
        assert_eq!(compiled.messages[0].message, "sort:false:0.00:true:true");
        assert_eq!(compiled.messages[1].message, "icon:true:false:0.00:24:24");
        assert_eq!(compiled.messages[2].message, "card:true:false:0.00:32:20");
        assert_eq!(
            compiled.messages[3].message,
            "unlock-banner:true:false:0.00:64:18"
        );
        assert_eq!(
            compiled.messages[4].message,
            "unlock-bg:true:false:0.00:96:54"
        );
        assert_eq!(compiled.overlays.len(), 4);
        assert!(matches!(
            compiled.overlays[0].kind,
            SongLuaOverlayKind::Sprite { ref texture_path, .. }
                if texture_path.ends_with("icon.png")
        ));
        assert!(matches!(
            compiled.overlays[1].kind,
            SongLuaOverlayKind::Sprite { ref texture_path, .. }
                if texture_path.ends_with("card.png")
        ));
        assert!(matches!(
            compiled.overlays[2].kind,
            SongLuaOverlayKind::Sprite { ref texture_path, .. }
                if texture_path.ends_with("unlock-banner.png")
        ));
        assert!(matches!(
            compiled.overlays[3].kind,
            SongLuaOverlayKind::Sprite { ref texture_path, .. }
                if texture_path.ends_with("unlock-bg.png")
        ));
    }

    #[test]
    fn compile_song_lua_supports_texture_translate_and_wrapping() {
        let song_dir = test_dir("actor-texture-translate-wrap");
        let image_path = song_dir.join("panel.png");
        image::RgbaImage::new(40, 30).save(&image_path).unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:texturetranslate(0.25, -0.5):texturewrapping(true)
            mod_actions = {
                {1, string.format("%.0f:%.0f", self:GetWidth(), self:GetHeight()), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor Texture Translate Wrap"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "40:30");
        assert_eq!(compiled.overlays.len(), 1);
        let state = compiled.overlays[0].initial_state;
        assert!(state.texture_wrapping);
        assert_eq!(state.texcoord_offset, Some([0.25, -0.5]));
        assert_eq!(state.custom_texture_rect, None);
    }

    #[test]
    fn compile_song_lua_supports_sprite_texture_coord_helpers() {
        let song_dir = test_dir("sprite-texture-coord-helpers");
        let image_path = song_dir.join("panel.png");
        image::RgbaImage::new(100, 80).save(&image_path).unwrap();
        let sheet_path = song_dir.join("panel 2x2.png");
        image::RgbaImage::new(100, 80).save(&sheet_path).unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:setstate(1):SetCustomImageRect(0.25, 0.5, 0.75, 1)
        end,
    },
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:customtexturerect(0, 0, 1, 1):stretchtexcoords(0.25, -0.5)
        end,
    },
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:addimagecoords(25, 20)
        end,
    },
    Def.Sprite{
        Texture="panel 2x2.png",
        OnCommand=function(self)
            self:setstate(1):addimagecoords(25, 20)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Sprite Texture Coord Helpers"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 4);
        assert_eq!(
            compiled.overlays[0].initial_state.sprite_state_index,
            Some(u32::MAX)
        );
        assert_eq!(
            compiled.overlays[0].initial_state.custom_texture_rect,
            Some([0.25, 0.5, 0.75, 1.0])
        );
        assert_eq!(
            compiled.overlays[1].initial_state.custom_texture_rect,
            Some([0.25, -0.5, 1.25, 0.5])
        );
        assert_eq!(
            compiled.overlays[2].initial_state.custom_texture_rect,
            Some([0.25, 0.25, 1.25, 1.25])
        );
        assert_eq!(
            compiled.overlays[3].initial_state.custom_texture_rect,
            Some([0.75, 0.25, 1.25, 0.75])
        );
    }

    #[test]
    fn compile_song_lua_supports_sprite_fade_edges() {
        let song_dir = test_dir("actor-fade-edges");
        let image_path = song_dir.join("panel.png");
        image::RgbaImage::new(40, 30).save(&image_path).unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:fadeleft(0.1):faderight(0.2):fadetop(0.3):fadebottom(0.4)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor Fade Edges"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 1);
        let state = compiled.overlays[0].initial_state;
        assert_eq!(state.fadeleft, 0.1);
        assert_eq!(state.faderight, 0.2);
        assert_eq!(state.fadetop, 0.3);
        assert_eq!(state.fadebottom, 0.4);
    }

    #[test]
    fn compile_song_lua_supports_overlay_skew_methods() {
        let song_dir = test_dir("actor-overlay-skew");
        let image_path = song_dir.join("panel.png");
        image::RgbaImage::new(40, 30).save(&image_path).unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:skewx(0.25):skewy(-0.5)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor Overlay Skew"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 1);
        let state = compiled.overlays[0].initial_state;
        assert!((state.skew_x - 0.25).abs() <= 0.000_1);
        assert!((state.skew_y + 0.5).abs() <= 0.000_1);
    }

    #[test]
    fn compile_song_lua_supports_mask_methods() {
        let song_dir = test_dir("actor-mask-methods");
        let image_path = song_dir.join("panel.png");
        image::RgbaImage::new(40, 30).save(&image_path).unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        Name="Source",
        OnCommand=function(self)
            self:zoomto(100, 100):MaskSource()
        end,
    },
    Def.BitmapText{
        Font="Common Normal",
        Text="MASK",
        OnCommand=function(self)
            self:MaskDest()
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor Mask Methods"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 2);
        assert!(compiled.overlays[0].initial_state.mask_source);
        assert!(compiled.overlays[1].initial_state.mask_dest);
    }

    #[test]
    fn compile_song_lua_supports_alignment_methods() {
        let song_dir = test_dir("actor-alignment-methods");
        let image_path = song_dir.join("panel.png");
        image::RgbaImage::new(40, 30).save(&image_path).unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:halign(0):valign(1)
        end,
    },
    Def.BitmapText{
        Font="Common Normal",
        Text="ALIGN",
        OnCommand=function(self)
            self:halign(1):valign(0):horizalign("right")
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor Alignment Methods"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 2);
        let sprite = compiled.overlays[0].initial_state;
        assert_eq!(sprite.halign, 0.0);
        assert_eq!(sprite.valign, 1.0);

        let text = compiled.overlays[1].initial_state;
        assert_eq!(text.halign, 1.0);
        assert_eq!(text.valign, 0.0);
        assert_eq!(text.text_align, TextAlign::Right);
    }

    #[test]
    fn compile_song_lua_supports_stepmania_alignment_enums() {
        let song_dir = test_dir("actor-alignment-enums");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:horizalign(HorizAlign_Left):vertalign(bottom)
        end,
    },
    Def.BitmapText{
        Font="Common Normal",
        Text="ENUM",
        OnCommand=function(self)
            self:horizalign("HorizAlign_Right"):vertalign("VertAlign_Top")
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor Alignment Enums"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 2);
        let quad = compiled.overlays[0].initial_state;
        assert_eq!(quad.halign, 0.0);
        assert_eq!(quad.valign, 1.0);

        let text = compiled.overlays[1].initial_state;
        assert_eq!(text.halign, 1.0);
        assert_eq!(text.valign, 0.0);
        assert_eq!(text.text_align, TextAlign::Right);
    }

    #[test]
    fn compile_song_lua_supports_shadow_methods() {
        let song_dir = test_dir("actor-shadow-methods");
        let image_path = song_dir.join("panel.png");
        image::RgbaImage::new(40, 30).save(&image_path).unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:shadowlength(5):shadowcolor(0.1, 0.2, 0.3, 0.4)
        end,
    },
    Def.BitmapText{
        Font="Common Normal",
        Text="SHADOW",
        OnCommand=function(self)
            self:shadowlengthx(3):shadowlengthy(4)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor Shadow Methods"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 2);

        let sprite = compiled.overlays[0].initial_state;
        assert_eq!(sprite.shadow_len, [5.0, -5.0]);
        assert_eq!(sprite.shadow_color, [0.1, 0.2, 0.3, 0.4]);

        let text = compiled.overlays[1].initial_state;
        assert_eq!(text.shadow_len, [3.0, -4.0]);
        assert_eq!(text.shadow_color, [0.0, 0.0, 0.0, 0.5]);
    }

    #[test]
    fn compile_song_lua_supports_glow_and_glowshift_methods() {
        let song_dir = test_dir("actor-glow-methods");
        let image_path = song_dir.join("panel.png");
        image::RgbaImage::new(40, 30).save(&image_path).unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:glow(0.1, 0.2, 0.3, 0.4)
        end,
    },
    Def.BitmapText{
        Font="Common Normal",
        Text="GLOW",
        OnCommand=function(self)
            self:glowshift()
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor Glow Methods"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 2);

        let sprite = compiled.overlays[0].initial_state;
        assert_eq!(sprite.glow, [0.1, 0.2, 0.3, 0.4]);

        let text = compiled.overlays[1].initial_state;
        assert_eq!(
            text.effect_mode,
            deadlib_present::anim::EffectMode::GlowShift
        );
        assert_eq!(text.effect_color1, [1.0, 1.0, 1.0, 0.2]);
        assert_eq!(text.effect_color2, [1.0, 1.0, 1.0, 0.8]);
    }

    #[test]
    fn compile_song_lua_accepts_vertex_diffuse_style_shims() {
        let song_dir = test_dir("actor-vertex-diffuse-shims");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r##"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:diffuseleftedge(0, 0, 0, 0.25)
                :diffuserightedge({1, 1, 1, 0.5})
                :diffusetopedge(color("#11223344"))
                :diffusebottomedge(0.8, 0.7, 0.6, 1)
                :diffuseupperleft(1, 0, 0, 1)
                :diffuseupperright(0, 1, 0, 1)
                :diffuselowerleft(0, 0, 1, 1)
                :diffuselowerright(1, 1, 0, 1)
            mod_actions = {
                {1, "ok", true},
            }
        end,
    },
}
"##,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor Vertex Diffuse Shims"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "ok");
        assert_eq!(compiled.overlays.len(), 1);
        let colors = compiled.overlays[0].initial_state.vertex_colors.unwrap();
        assert_eq!(colors[0], [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(colors[1], [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(colors[2], [1.0, 1.0, 0.0, 1.0]);
        assert_eq!(colors[3], [0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn compile_song_lua_supports_actor_multi_vertex_shape() {
        let song_dir = test_dir("actor-multi-vertex-shape");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r##"
local verts = {
    {{0, 0, 0}, color("#ff0000")},
    {{10, 0, 0}, {0, 1, 0, 1}},
    {{10, 10, 0}, {0, 0, 1, 1}},
    {{0, 10, 0}, {1, 1, 1, 1}},
}

return Def.ActorFrame{
    Def.ActorMultiVertex{
        InitCommand=function(self)
            self:SetDrawState{Mode="DrawMode_Quads"}
                :SetNumVertices(#verts)
                :SetVertices(verts)
                :SetLineWidth(3)
            mod_actions = {{
                1,
                string.format("%s:%d:%d", self:GetDrawState().Mode, self:GetNumVertices(), self:GetLineWidth()),
                true,
            }}
        end,
    },
}
"##,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor Multi Vertex Shape"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "DrawMode_Quads:4:3");
        assert_eq!(compiled.overlays.len(), 1);
        let SongLuaOverlayKind::ActorMultiVertex {
            vertices,
            texture_path,
            ..
        } = &compiled.overlays[0].kind
        else {
            panic!("expected ActorMultiVertex overlay");
        };
        assert!(texture_path.is_none());
        assert_eq!(vertices.len(), 6);
        assert_eq!(vertices[0].pos, [0.0, 0.0]);
        assert_eq!(vertices[0].color, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(vertices[5].pos, [0.0, 0.0]);
    }

    #[test]
    fn compile_song_lua_triangulates_actor_multi_vertex_line_strip() {
        let song_dir = test_dir("actor-multi-vertex-line-strip");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.ActorMultiVertex{
        InitCommand=function(self)
            self:SetDrawState{Mode="DrawMode_LineStrip"}
                :SetLineWidth(4)
                :SetVertices{
                    {{0, 0, 0}, {1, 0, 0, 1}},
                    {{10, 0, 0}, {0, 1, 0, 1}},
                    {{10, 10, 0}, {0, 0, 1, 1}},
                }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor Multi Vertex Line Strip"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 1);
        let SongLuaOverlayKind::ActorMultiVertex { vertices, .. } = &compiled.overlays[0].kind
        else {
            panic!("expected ActorMultiVertex overlay");
        };
        assert_eq!(vertices.len(), 12);
        let assert_pos = |actual: [f32; 2], expected: [f32; 2]| {
            assert!(
                actual
                    .iter()
                    .zip(expected.iter())
                    .all(|(a, b)| (a - b).abs() <= 0.000_1),
                "expected {expected:?}, got {actual:?}"
            );
        };
        assert_pos(vertices[0].pos, [0.0, 2.0]);
        assert_pos(vertices[1].pos, [8.0, 2.0]);
        assert_pos(vertices[2].pos, [12.0, -2.0]);
        assert_pos(vertices[5].pos, [0.0, -2.0]);
        assert_pos(vertices[6].pos, [8.0, 2.0]);
    }

    #[test]
    fn compile_song_lua_captures_textured_actor_multi_vertex_uvs() {
        let song_dir = test_dir("actor-multi-vertex-texture");
        let entry = song_dir.join("default.lua");
        let texture_path = song_dir.join("panel.png");
        image::RgbaImage::new(16, 16).save(&texture_path).unwrap();
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.ActorMultiVertex{
        InitCommand=function(self)
            self:SetTexture("panel.png")
                :SetDrawState{Mode="DrawMode_Triangles"}
                :SetVertices{
                    {{0, 0, 0}, {1, 1, 1, 1}, {0, 0}},
                    {{16, 0, 0}, {1, 1, 1, 1}, {1, 0}},
                    {{0, 16, 0}, {1, 1, 1, 1}, {0, 1}},
                }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor Multi Vertex Texture"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 1);
        let SongLuaOverlayKind::ActorMultiVertex {
            vertices,
            texture_path: Some(actual_texture),
            ..
        } = &compiled.overlays[0].kind
        else {
            panic!("expected textured ActorMultiVertex overlay");
        };
        assert_eq!(actual_texture, &texture_path);
        assert_eq!(vertices.len(), 3);
        assert_eq!(vertices[1].uv, [1.0, 0.0]);
        assert_eq!(vertices[2].uv, [0.0, 1.0]);
    }

    #[test]
    fn compile_song_lua_supports_diffuse_and_glow_blink_methods() {
        let song_dir = test_dir("actor-blink-effects");
        let image_path = song_dir.join("panel.png");
        image::RgbaImage::new(40, 30).save(&image_path).unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:diffuseblink():effectperiod(0.25):effectcolor1(0,0,0,1):effectcolor2(1,1,1,1)
        end,
    },
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:glowblink():effectclock("beatnooffset")
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor Blink Effects"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 2);

        let diffuse = compiled.overlays[0].initial_state;
        assert_eq!(diffuse.effect_mode, EffectMode::DiffuseShift);
        assert_eq!(diffuse.effect_period, 0.25);
        assert_eq!(diffuse.effect_color1, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(diffuse.effect_color2, [1.0, 1.0, 1.0, 1.0]);

        let glow = compiled.overlays[1].initial_state;
        assert_eq!(glow.effect_mode, EffectMode::GlowShift);
        assert_eq!(glow.effect_clock, EffectClock::Beat);
        assert_eq!(glow.effect_color1, [1.0, 1.0, 1.0, 0.2]);
        assert_eq!(glow.effect_color2, [1.0, 1.0, 1.0, 0.8]);
    }

    #[test]
    fn compile_song_lua_supports_overlay_multiply_and_subtract_blend() {
        let song_dir = test_dir("overlay-extra-blends");
        let image_path = song_dir.join("panel.png");
        image::RgbaImage::new(40, 30).save(&image_path).unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:blend("multiply")
        end,
    },
    Def.Quad{
        OnCommand=function(self)
            self:blend("subtract")
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Overlay Extra Blends"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 2);
        assert_eq!(
            compiled.overlays[0].initial_state.blend,
            SongLuaOverlayBlendMode::Multiply
        );
        assert_eq!(
            compiled.overlays[1].initial_state.blend,
            SongLuaOverlayBlendMode::Subtract
        );
    }

    #[test]
    fn compile_song_lua_supports_bitmaptext_layout_methods() {
        let song_dir = test_dir("bitmaptext-layout-methods");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.BitmapText{
        Font="Common Normal",
        Text="WRAP",
        OnCommand=function(self)
            self:wrapwidthpixels(64):maxwidth(80):maxheight(40):zoom(2)
        end,
    },
    Def.BitmapText{
        Font="Common Normal",
        Text="POST",
        OnCommand=function(self)
            self:zoom(2):maxwidth(90):maxheight(50)
        end,
    },
    Def.BitmapText{
        Font="Common Normal",
        Text="USEZOOM",
        OnCommand=function(self)
            self:maxwidth(70):maxheight(30):zoom(2):max_dimension_use_zoom(true)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "BitmapText Layout Methods"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 3);

        let pre_zoom = compiled.overlays[0].initial_state;
        assert_eq!(pre_zoom.wrap_width_pixels, Some(64));
        assert_eq!(pre_zoom.max_width, Some(80.0));
        assert_eq!(pre_zoom.max_height, Some(40.0));
        assert!(pre_zoom.max_w_pre_zoom);
        assert!(pre_zoom.max_h_pre_zoom);

        let post_zoom = compiled.overlays[1].initial_state;
        assert_eq!(post_zoom.max_width, Some(90.0));
        assert_eq!(post_zoom.max_height, Some(50.0));
        assert!(!post_zoom.max_w_pre_zoom);
        assert!(!post_zoom.max_h_pre_zoom);

        let use_zoom = compiled.overlays[2].initial_state;
        assert_eq!(use_zoom.max_width, Some(70.0));
        assert_eq!(use_zoom.max_height, Some(30.0));
        assert!(use_zoom.max_w_pre_zoom);
        assert!(use_zoom.max_h_pre_zoom);
        assert!(use_zoom.max_dimension_uses_zoom);
    }

    #[test]
    fn compile_song_lua_supports_bitmaptext_uppercase_and_vertspacing() {
        let song_dir = test_dir("bitmaptext-uppercase-vertspacing");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.BitmapText{
        Font="Common Normal",
        Text="Mixed Case",
        OnCommand=function(self)
            self:uppercase(true):vertspacing(18)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "BitmapText Uppercase VertSpacing"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 1);

        let text = compiled.overlays[0].initial_state;
        assert!(text.uppercase);
        assert_eq!(text.vert_spacing, Some(18));
    }

    #[test]
    fn compile_song_lua_supports_bitmaptext_fit_methods() {
        let song_dir = test_dir("bitmaptext-fit-methods");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.BitmapText{
        Font="Common Normal",
        Text="FIT",
        OnCommand=function(self)
            self:zoomtowidth(120):zoomtoheight(30)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "BitmapText Fit Methods"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 1);
        assert_eq!(compiled.overlays[0].initial_state.size, Some([120.0, 30.0]));
    }

    #[test]
    fn compile_song_lua_supports_actor_set_size_methods() {
        let song_dir = test_dir("actor-set-size");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:SetSize(10, 20)
            self:SetWidth(30)
            self:SetHeight(40)
            mod_actions = {
                {1, string.format("%.0f:%.0f", self:GetWidth(), self:GetHeight()), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor Set Size"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "30:40");
    }

    #[test]
    fn compile_song_lua_supports_align_and_setsize_aliases() {
        let song_dir = test_dir("actor-align-setsize");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:setsize(12, 34):align(0, 1)
            mod_actions = {
                {1, string.format("%.0f:%.0f", self:GetWidth(), self:GetHeight()), true},
            }
        end,
    },
    Def.BitmapText{
        Font="Common Normal",
        Text="ALIGN",
        OnCommand=function(self)
            self:align(1, 0.5)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor Align SetSize"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "12:34");
        assert_eq!(compiled.overlays.len(), 2);

        let quad = compiled.overlays[0].initial_state;
        assert_eq!(quad.size, Some([12.0, 34.0]));
        assert_eq!(quad.halign, 0.0);
        assert_eq!(quad.valign, 1.0);

        let text = compiled.overlays[1].initial_state;
        assert_eq!(text.halign, 1.0);
        assert_eq!(text.valign, 0.5);
    }

    #[test]
    fn compile_song_lua_supports_scale_to_clipped_size() {
        let song_dir = test_dir("scale-to-clipped-size");
        let image_path = song_dir.join("panel.png");
        image::RgbaImage::new(120, 60).save(&image_path).unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:scaletoclipped(90, 36)
            mod_actions = {
                {1, string.format("%.0f:%.0f", self:GetWidth(), self:GetHeight()), true},
            }
        end,
    },
    Def.Quad{
        OnCommand=function(self)
            self:ScaleToClipped(10, 20)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Scale To Clipped Size"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "90:36");
        assert_eq!(compiled.overlays.len(), 2);
        assert_eq!(compiled.overlays[0].initial_state.size, Some([90.0, 36.0]));
        assert_eq!(compiled.overlays[1].initial_state.size, Some([10.0, 20.0]));
    }

    #[test]
    fn compile_song_lua_supports_scale_to_fit_and_cover() {
        let song_dir = test_dir("scale-to-fit-cover");
        let image_path = song_dir.join("panel.png");
        image::RgbaImage::new(200, 100).save(&image_path).unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:scaletofit(100, 100, 300, 220)
        end,
    },
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:scaletocover(100, 100, 300, 220)
        end,
    },
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:halign(0):valign(1):scaletofit(100, 100, 300, 220)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Scale To Fit Cover"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 3);

        let fit = compiled.overlays[0].initial_state;
        assert_eq!(fit.x, 200.0);
        assert_eq!(fit.y, 160.0);
        assert_eq!(fit.zoom, 1.0);
        assert_eq!(fit.zoom_x, 1.0);
        assert_eq!(fit.zoom_y, 1.0);

        let cover = compiled.overlays[1].initial_state;
        assert_eq!(cover.x, 200.0);
        assert_eq!(cover.y, 160.0);
        assert!((cover.zoom - 1.2).abs() <= 0.000_1);
        assert!((cover.zoom_x - 1.2).abs() <= 0.000_1);
        assert!((cover.zoom_y - 1.2).abs() <= 0.000_1);

        let aligned = compiled.overlays[2].initial_state;
        assert_eq!(aligned.x, 100.0);
        assert_eq!(aligned.y, 220.0);
        assert_eq!(aligned.zoom, 1.0);
    }

    #[test]
    fn compile_song_lua_supports_sprite_crop_to() {
        let song_dir = test_dir("sprite-crop-to");
        image::RgbaImage::new(200, 100)
            .save(song_dir.join("wide.png"))
            .unwrap();
        image::RgbaImage::new(100, 200)
            .save(song_dir.join("tall.png"))
            .unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="wide.png",
        OnCommand=function(self)
            self:zoom(2):CropTo(100, 100)
            mod_actions = {
                {1, string.format("%.0f:%.0f:%.0f", self:GetWidth(), self:GetHeight(), self:GetZoomedWidth()), true},
            }
        end,
    },
    Def.Sprite{
        Texture="tall.png",
        OnCommand=function(self)
            self:CropTo(100, 100)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Sprite CropTo"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "100:100:100");
        assert_eq!(compiled.overlays.len(), 2);

        let wide = compiled.overlays[0].initial_state;
        assert_eq!(wide.size, Some([100.0, 100.0]));
        assert_eq!(wide.zoom, 1.0);
        assert_eq!(wide.zoom_x, 1.0);
        assert_eq!(wide.zoom_y, 1.0);
        assert_eq!(wide.sprite_state_index, Some(u32::MAX));
        assert_eq!(wide.custom_texture_rect, Some([0.25, 0.0, 0.75, 1.0]));

        let tall = compiled.overlays[1].initial_state;
        assert_eq!(tall.size, Some([100.0, 100.0]));
        assert_eq!(tall.custom_texture_rect, Some([0.0, 0.25, 1.0, 0.75]));
    }

    #[test]
    fn compile_song_lua_supports_basezoom_axis_methods() {
        let song_dir = test_dir("basezoom-axis");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:basezoom(2)
            self:basezoomx(3)
            self:basezoomy(4)
            self:basezoomz(5)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "BaseZoom Axis"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 1);
        assert_eq!(compiled.overlays[0].initial_state.basezoom, 2.0);
        assert_eq!(compiled.overlays[0].initial_state.basezoom_x, 3.0);
        assert_eq!(compiled.overlays[0].initial_state.basezoom_y, 4.0);
        assert_eq!(compiled.overlays[0].initial_state.basezoom_z, 5.0);
    }

    #[test]
    fn compile_song_lua_exposes_zoomed_actor_size() {
        let song_dir = test_dir("zoomed-actor-size");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:SetSize(40, 20)
            self:zoomx(2)
            self:zoomy(3)
            self:basezoomx(0.5)
            self:basezoomy(2)
            mod_actions = {
                {1, string.format("%.0f:%.0f", self:GetZoomedWidth(), self:GetZoomedHeight()), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Zoomed Actor Size"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "40:120");
    }

    #[test]
    fn compile_song_lua_supports_actor_state_getters() {
        let song_dir = test_dir("actor-state-getters");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local target

mod_actions = {
    {4, function()
        if target then
            target:x(target:GetSecsIntoEffect())
            target:y(target:GetEffectDelta())
        end
    end, true},
}

return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            target = self
            self:xy(10, 20):z(3):basezoom(2):basezoomx(3):basezoomy(4):basezoomz(5)
            self:diffuse(0.2, 0.4, 0.6, 0.8):glow(0.1, 0.2, 0.3, 0.4)
            self:halign(0):valign(1):effectmagnitude(8, 4, 2):effectclock("beat"):visible(false)
            local d = self:GetDiffuse()
            local g = self:GetGlow()
            local mx, my, mz = self:geteffectmagnitude()
            mod_actions[#mod_actions + 1] = {
                1,
                string.format(
                    "%.0f:%.0f:%.0f:%.0f:%.0f:%.0f:%.0f:%.0f:%.0f:%.1f:%.1f:%.1f:%.1f:%s:%.0f:%.0f:%.0f:%.0f:%.0f",
                    self:GetDestX(),
                    self:GetDestY(),
                    self:GetDestZ(),
                    self:GetBaseZoomX(),
                    self:GetBaseZoomY(),
                    self:GetBaseZoomZ(),
                    self:GetHAlign(),
                    self:GetVAlign(),
                    self:GetAlpha() * 10,
                    d[1],
                    d[3],
                    g[1],
                    g[4],
                    tostring(self:GetVisible()),
                    mx,
                    my,
                    mz,
                    self:GetSecsIntoEffect(),
                    self:GetEffectDelta()
                ),
                true
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor State Getters"),
        )
        .unwrap();
        assert_eq!(compiled.info.unsupported_function_actions, 0);
        assert_eq!(compiled.messages.len(), 2);
        assert_eq!(
            compiled.messages[0].message,
            "10:20:3:3:4:5:0:1:8:0.2:0.6:0.1:0.4:false:8:4:2:0:0"
        );
        assert_eq!(compiled.overlays.len(), 1);
        assert_eq!(compiled.overlays[0].initial_state.basezoom_z, 5.0);
        assert_eq!(compiled.overlays[0].message_commands.len(), 1);
        let block = &compiled.overlays[0].message_commands[0].blocks[0];
        assert_eq!(block.delta.x, Some(4.0));
        assert_eq!(block.delta.y, Some(0.0));
    }

    #[test]
    fn compile_song_lua_accepts_basezoomz_method() {
        let song_dir = test_dir("basezoom-z");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.ActorFrame{
        OnCommand=function(self)
            self:basezoomz(5)
            mod_actions = {
                {1, "ok", true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled =
            test_compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "BaseZoom Z"))
                .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "ok");
        assert_eq!(compiled.overlays.len(), 1);
        assert_eq!(compiled.overlays[0].initial_state.basezoom_z, 5.0);
    }

    #[test]
    fn compile_song_lua_exposes_screen_globals() {
        let song_dir = test_dir("screen-globals");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
mod_actions = {
    {
        1,
        string.format(
            "%.0f:%.0f:%.0f:%.0f",
            _screen.w,
            _screen.h,
            _screen.cx,
            _screen.cy
        ),
        true,
    },
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Screen Globals");
        context.screen_width = 800.0;
        context.screen_height = 600.0;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "800:600:400:300");
    }

    #[test]
    fn compile_song_lua_supports_zoom_to_width_and_height() {
        let song_dir = test_dir("zoomto-width-height");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:SetSize(10, 20)
            self:zoomtowidth(30)
            self:zoomtoheight(40)
            mod_actions = {
                {1, string.format("%.0f:%.0f", self:GetWidth(), self:GetHeight()), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Zoomto Width Height"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "30:40");
    }

    #[test]
    fn compile_song_lua_zoom_sets_axis_state() {
        let song_dir = test_dir("zoom-axis-state");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:zoom(2)
            self:zoomx(3)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Zoom Axis State"),
        )
        .unwrap();
        let overlay = &compiled.overlays[0];
        assert_eq!(overlay.initial_state.zoom, 2.0);
        assert_eq!(overlay.initial_state.zoom_x, 3.0);
        assert_eq!(overlay.initial_state.zoom_y, 2.0);
        assert_eq!(overlay.initial_state.zoom_z, 2.0);
    }

    #[test]
    fn compile_song_lua_exposes_debug_getinfo_source() {
        let song_dir = test_dir("debug-getinfo");
        let lua_dir = song_dir.join("lua");
        fs::create_dir_all(&lua_dir).unwrap();
        fs::write(
            lua_dir.join("child.lua"),
            r#"
local info = debug.getinfo(1)
mod_actions = {
    {1, info.source, true},
}
return Def.ActorFrame{}
"#,
        )
        .unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return assert(loadfile(GAMESTATE:GetCurrentSong():GetSongDir() .. "lua/child.lua"))()
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Debug Getinfo"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            format!("@{}", file_path_string(&lua_dir.join("child.lua")))
        );
    }

    #[test]
    fn compile_song_lua_exposes_math_round_compat() {
        let song_dir = test_dir("math-round");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
mod_actions = {
    {1, string.format("%d:%d:%d", math.round(1.49), math.round(1.5), math.round(-1.5)), true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled =
            test_compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "Math Round"))
                .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "1:2:-2");
    }

    #[test]
    fn compile_song_lua_supports_xero_chunk_env_switching() {
        let song_dir = test_dir("xero-chunk-env");
        let template_dir = song_dir.join("template");
        fs::create_dir_all(&template_dir).unwrap();
        fs::write(
            template_dir.join("std.lua"),
            r#"
local xero = setmetatable(xero, xero)
xero.__index = _G

function xero:__call(f)
    setfenv(f or 2, self)
    return f
end

xero()

local stringbuilder_mt = {
    __index = {
        build = table.concat,
    },
    __call = function(self, value)
        table.insert(self, tostring(value))
        return self
    end,
}

function stringbuilder()
    return setmetatable({}, stringbuilder_mt)
end

return Def.Actor{}
"#,
        )
        .unwrap();
        fs::write(
            template_dir.join("template.lua"),
            r#"
xero()

local sb = stringbuilder()
sb("ok")
mod_actions = {
    {1, sb:build(), true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();
        let entry = template_dir.join("main.lua");
        fs::write(
            &entry,
            r#"
_G.xero = {}

return Def.ActorFrame{
    assert(loadfile(GAMESTATE:GetCurrentSong():GetSongDir()..'template/std.lua'))(),
    assert(loadfile(GAMESTATE:GetCurrentSong():GetSongDir()..'template/template.lua'))(),
}
"#,
        )
        .unwrap();

        let compiled =
            test_compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "Xero")).unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "ok");
    }

    #[test]
    fn compile_song_lua_supports_xero_require_env_switching() {
        let song_dir = test_dir("xero-require-env");
        let template_dir = song_dir.join("template");
        let lua_dir = song_dir.join("lua");
        fs::create_dir_all(&template_dir).unwrap();
        fs::create_dir_all(&lua_dir).unwrap();
        fs::write(
            template_dir.join("std.lua"),
            r#"
setmetatable(xero, {
    __index = _G,
    __call = function(self, f)
        setfenv(f or 2, self)
        return f
    end,
})

xero.package = {
    loaded = {},
    loaders = {
        function(modname)
            local loader, err = loadfile(xero.dir .. "lua/" .. modname .. ".lua")
            if loader then return xero(loader) end
            return err
        end,
    },
}

function xero.require(modname)
    local loaded = xero.package.loaded
    if not loaded[modname] then
        for _, loader in ipairs(xero.package.loaders) do
            local chunk = loader(modname)
            if type(chunk) == "function" then
                loaded[modname] = chunk() or true
                break
            end
        end
    end
    return loaded[modname]
end

xero()
return Def.Actor{}
"#,
        )
        .unwrap();
        fs::write(
            template_dir.join("template.lua"),
            r#"
xero()
xero.P = {"ok"}
xero.require("mods")
return Def.ActorFrame{}
"#,
        )
        .unwrap();
        fs::write(
            lua_dir.join("mods.lua"),
            r#"
mod_actions = {
    {1, P[1], true},
}
"#,
        )
        .unwrap();
        let entry = template_dir.join("main.lua");
        fs::write(
            &entry,
            r#"
_G.xero = {
    dir = GAMESTATE:GetCurrentSong():GetSongDir(),
}

return Def.ActorFrame{
    assert(loadfile(xero.dir .. "template/std.lua"))(),
    assert(loadfile(xero.dir .. "template/template.lua"))(),
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Xero Require"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "ok");
    }

    #[test]
    fn compile_song_lua_returns_empty_fileman_listing_for_missing_dir() {
        let song_dir = test_dir("fileman-empty-listing");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local listing = FILEMAN:GetDirListing(GAMESTATE:GetCurrentSong():GetSongDir() .. "plugins/")
mod_actions = {
    {1, string.format("%s:%d", type(listing), #listing), true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled =
            test_compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "Fileman"))
                .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "table:0");
    }

    #[test]
    fn compile_song_lua_exposes_actorframe_class_methods() {
        let song_dir = test_dir("actorframe-class");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local child = Def.ActorFrame{
    Name="child",
    PingCommand=function(self) self:aux(self:getaux() + 1) end,
    Def.Quad{
        Name="leaf",
        PingCommand=function(self) self:aux(self:getaux() + 10) end,
    },
}
local removable = Def.ActorFrame{Name="remove"}
local disposable = Def.ActorFrame{Def.Quad{Name="gone"}}
local root = Def.ActorFrame{
    Name="root",
    child,
    removable,
    InitCommand=function(self)
        ActorFrame.SetFOV(self, 75)
        ActorFrame.fov(self, 80)
        ActorFrame.SetUpdateRate(self, 3)
        ActorFrame.vanishpoint(self, 120, 80)
        ActorFrame.SetDrawFunction(self, function(actor) actor:y(8) end)
        ActorFrame.GetDrawFunction(self)(self)
        local update_ok = ActorFrame.SetUpdateFunction(self, function(actor) actor:aux(11) end) == self
        ActorFrame.SetDrawByZPosition(self, true)
        ActorFrame.SortByDrawOrder(self)
        ActorFrame.SetAmbientLightColor(self, color("1,1,1,1"))
        ActorFrame.SetDiffuseLightColor(self, color("1,1,1,1"))
        ActorFrame.SetSpecularLightColor(self, color("1,1,1,1"))
        ActorFrame.SetLightDirection(self, {0, 0, 1})
        ActorFrame.AddChildFromPath(self, "missing.lua")
        local propagate_ok = ActorFrame.propagate(self, true) == self
        ActorFrame.propagate(self, false)
        ActorFrame.playcommandonchildren(self, "Ping")
        ActorFrame.playcommandonleaves(self, "Ping")
        ActorFrame.RunCommandsOnChildren(self, function(actor, params) actor:aux(actor:getaux() + params.direct) end, {direct=100})
        ActorFrame.runcommandsonleaves(self, function(actor) actor:aux(actor:getaux() + 1000) end)
        local picked = ActorFrame.GetChildAt(self, 0)
        local picked_method = self:GetChildAt(0)
        local second = ActorFrame.GetChildAt(self, 1)
        local named = ActorFrame.GetChild(self, "child")
        local children = ActorFrame.GetChildren(self)
        local count_before = ActorFrame.GetNumChildren(self)
        ActorFrame.RemoveChild(self, "remove")
        local count_after = ActorFrame.GetNumChildren(self)
        ActorFrame.RemoveAllChildren(disposable)
        mod_actions = {
            {1, string.format(
                "%s:%s:%s:%s:%d:%d:%.0f:%.0f:%.0f:%.0f:%s:%s:%s",
                tostring(ActorFrame.fardistz(self, 500) == self),
                picked and picked:GetName() or "nil",
                second and second:GetName() or "nil",
                tostring(picked_method == child and named == child and children["child"] == child),
                count_before,
                count_after,
                self:GetUpdateRate(),
                self:GetDestY(),
                child:getaux(),
                child:GetChild("leaf"):getaux(),
                tostring(update_ok),
                tostring(propagate_ok),
                tostring(ActorFrame.GetNumChildren(disposable) == 0)
            ), true},
        }
    end,
}

return root
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "ActorFrame Class"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "true:child:remove:true:2:1:3:8:101:1010:true:true:true"
        );
        let root = compiled
            .overlays
            .iter()
            .find(|overlay| overlay.name.as_deref() == Some("root"))
            .unwrap();
        assert_eq!(root.initial_state.fov, Some(80.0));
        assert_eq!(root.initial_state.vanishpoint, Some([120.0, 80.0]));
        assert!(root.initial_state.draw_by_z_position);
    }

    #[test]
    fn compile_song_lua_supports_actorframe_child_methods() {
        let song_dir = test_dir("actorframe-child-methods");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local function draw_fn(self)
    self:aux(7)
end

return Def.ActorFrame{
    Name="Root",
    Def.Quad{
        Name="Keep",
        PingCommand=function(self) self:aux(self:getaux() + 1) end,
    },
    Def.Quad{
        Name="RemoveMe",
        PingCommand=function(self) self:aux(99) end,
    },
    Def.ActorFrame{
        Name="Branch",
        Def.Quad{
            Name="Leaf",
            PingCommand=function(self) self:aux(self:getaux() + 3) end,
        },
    },
    OnCommand=function(self)
        self:SetFOV(75):SetUpdateRate(2):SetDrawFunction(draw_fn)
        self:SetDrawByZPosition(true):SortByDrawOrder():propagate(false)
        self:SetAmbientLightColor(color("1,1,1,1")):SetDiffuseLightColor(color("1,1,1,1"))
        self:SetSpecularLightColor(color("1,1,1,1")):SetLightDirection({0, 0, 1})
        self:playcommandonchildren("Ping")
        self:playcommandonleaves("Ping")
        local children = self:GetChildren()
        local keep = children["Keep"]
        local branch = children["Branch"]
        local leaf = branch:GetChild("Leaf")
        local before_remove = children["RemoveMe"] ~= nil
        self:RemoveChild("RemoveMe")
        local after_remove = self:GetChildren()["RemoveMe"] == nil
        mod_actions = {
            {1, string.format(
                "%.0f:%.0f:%.0f:%.0f:%s:%s:%s",
                keep:getaux(),
                branch:getaux(),
                leaf:getaux(),
                self:GetUpdateRate(),
                tostring(before_remove),
                tostring(after_remove),
                tostring(self:GetDrawFunction() ~= nil)
            ), true},
        }
    end,
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "ActorFrame Child Methods"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "2:0:3:2:true:true:true");
        assert!(
            compiled
                .overlays
                .iter()
                .all(|overlay| overlay.name.as_deref() != Some("RemoveMe"))
        );
        let root = compiled
            .overlays
            .iter()
            .find(|overlay| overlay.name.as_deref() == Some("Root"))
            .unwrap();
        assert_eq!(root.initial_state.fov, Some(75.0));
        assert!(root.initial_state.draw_by_z_position);
    }

    #[test]
    fn compile_song_lua_captures_draw_by_z_position_commands() {
        let song_dir = test_dir("draw-by-z-position-command");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Name="Root",
    Def.Quad{Name="Back", z=30},
    Def.Quad{Name="Front", z=-30},
    FlipMessageCommand=function(self)
        self:SetDrawByZPosition(true)
    end,
}
"#,
        )
        .unwrap();

        let compiled =
            test_compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "Draw By Z"))
                .unwrap();
        let root = compiled
            .overlays
            .iter()
            .find(|overlay| overlay.name.as_deref() == Some("Root"))
            .unwrap();
        assert!(!root.initial_state.draw_by_z_position);
        assert_eq!(root.message_commands.len(), 1);
        assert_eq!(root.message_commands[0].message, "Flip");
        assert_eq!(root.message_commands[0].blocks.len(), 1);
        assert_eq!(
            root.message_commands[0].blocks[0].delta.draw_by_z_position,
            Some(true)
        );
    }

    #[test]
    fn compile_song_lua_supports_add_child_from_path() {
        let song_dir = test_dir("add-child-from-path");
        let entry = song_dir.join("default.lua");
        fs::write(
            song_dir.join("child.lua"),
            r#"
return Def.Quad{
    Name="Loaded",
    InitCommand=function(self) self:x(42) end,
    OnCommand=function(self) self:y(24) end,
}
"#,
        )
        .unwrap();
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Name="Root",
    InitCommand=function(self)
        self:AddChildFromPath("child")
        local loaded = self:GetChild("Loaded")
        mod_actions = {{1, loaded and loaded:GetName() or "nil", true}}
    end,
}
"#,
        )
        .unwrap();

        let compiled =
            test_compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "Add Child"))
                .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "Loaded");
        let root_index = compiled
            .overlays
            .iter()
            .position(|overlay| overlay.name.as_deref() == Some("Root"))
            .unwrap();
        let loaded = compiled
            .overlays
            .iter()
            .find(|overlay| overlay.name.as_deref() == Some("Loaded"))
            .unwrap();
        assert_eq!(loaded.parent_index, Some(root_index));
        assert_eq!(loaded.initial_state.x, 42.0);
        assert_eq!(loaded.initial_state.y, 24.0);
    }

    #[test]
    fn compile_song_lua_runs_late_add_child_from_path_commands() {
        let song_dir = test_dir("late-add-child-from-path");
        let entry = song_dir.join("default.lua");
        fs::write(
            song_dir.join("child.lua"),
            r#"
return Def.Quad{
    Name="Loaded",
    InitCommand=function(self) self:x(42) end,
    OnCommand=function(self) self:y(24) end,
}
"#,
        )
        .unwrap();
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Name="Root",
    OnCommand=function(self)
        self:AddChildFromPath("child")
        self:queuecommand("Report")
    end,
    ReportCommand=function(self)
        local loaded = self:GetChild("Loaded")
        mod_actions = {{
            1,
            loaded and string.format("%.0f:%.0f", loaded:GetX(), loaded:GetY()) or "nil",
            true,
        }}
    end,
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Late Add Child"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "42:24");
        let root_index = compiled
            .overlays
            .iter()
            .position(|overlay| overlay.name.as_deref() == Some("Root"))
            .unwrap();
        let loaded = compiled
            .overlays
            .iter()
            .find(|overlay| overlay.name.as_deref() == Some("Loaded"))
            .unwrap();
        assert_eq!(loaded.parent_index, Some(root_index));
        assert_eq!(loaded.initial_state.x, 42.0);
        assert_eq!(loaded.initial_state.y, 24.0);
    }

    #[test]
    fn compile_song_lua_passes_playcommand_params() {
        let song_dir = test_dir("playcommand-params");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Name="Root",
    SetCommand=function(self, params)
        self:aux(params.root)
    end,
    Def.Quad{
        Name="Child",
        SetCommand=function(self, params)
            self:aux(params.child)
        end,
    },
    Def.ActorFrame{
        Name="Branch",
        Def.Quad{
            Name="Leaf",
            LeafCommand=function(self, params)
                self:aux(params.leaf)
            end,
        },
    },
    OnCommand=function(self)
        self:playcommand("Set", {root=4})
        self:playcommandonchildren("Set", {child=7})
        self:playcommandonleaves("Leaf", {leaf=9})
        local children = self:GetChildren()
        mod_actions = {
            {1, string.format(
                "%.0f:%.0f:%.0f",
                self:getaux(),
                children["Child"]:getaux(),
                children["Branch"]:GetChild("Leaf"):getaux()
            ), true},
        }
    end,
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "PlayCommand Params"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "4:7:9");
    }

    #[test]
    fn compile_song_lua_getchildren_scans_unnamed_actorframes() {
        let song_dir = test_dir("getchildren-unnamed-actorframes");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local function scan(actor, skip)
    if tostring(actor):find("table") and not skip then
        for _, child in pairs(actor) do
            scan(child)
        end
        return
    end
    if actor.GetChildren then
        for _, child in pairs(actor:GetChildren()) do
            scan(child)
        end
    end
    if actor.GetName and actor:GetName() == "TargetLeaf" then
        prefix_globals.found_leaf = true
    end
end

prefix_globals = {}

return Def.ActorFrame{
    OnCommand=function(self)
        scan(self, true)
        mod_actions = {{1, tostring(prefix_globals.found_leaf == true), true}}
    end,
    Def.ActorFrame{},
    Def.ActorFrame{
        Def.ActorFrame{
            Def.Quad{
                Name="TargetLeaf",
            },
        },
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "GetChildren Unnamed ActorFrames"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "true");
    }

    #[test]
    fn compile_song_lua_supports_propagate_command_helpers() {
        let song_dir = test_dir("propagate-command");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Name="Root",
    PingCommand=function(self) self:aux(100) end,
    Def.ActorFrame{
        Name="Branch",
        PingCommand=function(self, params) self:aux(params.branch) end,
        Def.Quad{
            Name="Leaf",
            PingCommand=function(self, params) self:aux(params.leaf) end,
        },
    },
    Def.Quad{
        Name="Direct",
        PingCommand=function(self, params) self:aux(params.direct) end,
    },
    OnCommand=function(self)
        self:propagatecommand("Ping", {branch=1, leaf=2, direct=4})
        local branch = self:GetChild("Branch")
        local leaf = branch:GetChild("Leaf")
        local direct = self:GetChild("Direct")
        local after_propagatecommand = string.format(
            "%.0f:%.0f:%.0f:%.0f",
            self:getaux(),
            branch:getaux(),
            leaf:getaux(),
            direct:getaux()
        )
        self:propagate(true):playcommand("Ping", {branch=8, leaf=16, direct=32}):propagate(false)
        mod_actions = {
            {1, after_propagatecommand .. "|" .. string.format(
                "%.0f:%.0f:%.0f:%.0f",
                self:getaux(),
                branch:getaux(),
                leaf:getaux(),
                direct:getaux()
            ), true},
        }
    end,
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Propagate Command"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "0:1:2:4|0:8:16:32");
    }

    #[test]
    fn compile_song_lua_accepts_skewy_probe_calls() {
        let song_dir = test_dir("skewy-probe");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local target = nil

mods_ease = {
    {1, 1, 0, 0.25, function(x)
        if target then
            target:skewy(x)
        end
    end, "len", ease.outQuad},
}

return Def.ActorFrame{
    Def.ActorFrame{
        OnCommand=function(self)
            target = self
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "SkewY Probe"),
        )
        .unwrap();
        assert_eq!(compiled.info.unsupported_function_eases, 0);
        assert_eq!(compiled.eases.len(), 1);
        assert!(matches!(
            compiled.eases[0].target,
            SongLuaEaseTarget::PlayerSkewY
        ));
    }

    #[test]
    fn compile_song_lua_accepts_set_draw_function() {
        let song_dir = test_dir("set-draw-function");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local function draw_fn(self)
    self:visible(true)
end

return Def.ActorFrame{
    OnCommand=function(self)
        self:SetDrawFunction(draw_fn)
        self:queuecommand("Ready")
    end,
    ReadyCommand=function(self)
        mod_actions = {
            {1, tostring(self ~= nil), true},
        }
    end,
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Set Draw Function"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "true");
        assert_eq!(compiled.overlays.len(), 1);
        assert!(compiled.overlays[0].initial_state.visible);
    }

    #[test]
    fn compile_song_lua_accepts_theme_actor_compat_methods() {
        let song_dir = test_dir("theme-actor-compat-methods");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        ExpandForDoubleCommand=function(self)
            self:aux(2)
        end,
        OnCommand=function(self)
            local command = self:GetCommand("ExpandForDouble")
            local missing = self:GetCommand("MissingCommand")
            if command then command(self) end
            self:rainbow():jitter(true):distort(0.5):undistort():hurrytweening(2)
            mod_actions = {
                {1, string.format("%s:%s:%.0f:%.0f", tostring(command ~= nil), tostring(missing == nil), self:getaux(), self:GetTweenTimeLeft()), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Theme Actor Compat Methods"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "true:true:2:0");
        assert_eq!(compiled.overlays.len(), 1);
        assert!(compiled.overlays[0].initial_state.rainbow);
    }

    #[test]
    fn compile_song_lua_supports_aux_and_actor_compat_shims() {
        let song_dir = test_dir("actor-aux-compat-shims");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            local before = self:getaux()
            self:aux(before + 0.25)
            self:SetTextureFiltering(false):zbuffer(false):ztest(false):ztestmode("WriteOnFail"):draworder(100)
            self:zwrite(true):zbias(2):backfacecull(true):cullmode("CullMode_Back")
            self:aux(self:getaux() + 0.75)
            mod_actions = {
                {1, string.format("%.2f", self:getaux()), true},
            }
        end,
    },
    Def.Quad{
        OnCommand=function(self)
            self:ztest(false):ztestmode("WriteOnFail")
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor Aux Compat Shims"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "1.00");
        assert_eq!(compiled.overlays.len(), 2);
        assert_eq!(compiled.overlays[0].initial_state.draw_order, 100);
        assert_eq!(compiled.overlays[0].initial_state.z_bias, 2.0);
        assert!(compiled.overlays[0].initial_state.depth_test);
        assert!(!compiled.overlays[0].initial_state.texture_filtering);
        assert!(compiled.overlays[1].initial_state.depth_test);
    }

    #[test]
    fn compile_song_lua_captures_actor_draw_order() {
        let song_dir = test_dir("actor-draw-order");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        Name="Late",
        InitCommand=function(self)
            self:draworder(100)
        end,
    },
    Def.Quad{
        Name="Early",
        InitCommand=function(self)
            self:draworder(-10)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor Draw Order"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 2);
        let late = compiled
            .overlays
            .iter()
            .find(|overlay| overlay.name.as_deref() == Some("Late"))
            .unwrap();
        let early = compiled
            .overlays
            .iter()
            .find(|overlay| overlay.name.as_deref() == Some("Early"))
            .unwrap();
        assert_eq!(late.initial_state.draw_order, 100);
        assert_eq!(early.initial_state.draw_order, -10);
    }

    #[test]
    fn compile_song_lua_ignores_unsupported_draw_function_errors() {
        let song_dir = test_dir("set-draw-function-error");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local function draw_fn(self)
    self:MissingDrawMethod()
end

return Def.ActorFrame{
    OnCommand=function(self)
        self:SetDrawFunction(draw_fn)
        mod_actions = {
            {1, "draw-ok", true},
        }
    end,
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Set Draw Function Error"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "draw-ok");
    }

    #[test]
    fn compile_song_lua_defers_queuecommand_until_after_oncommand() {
        let song_dir = test_dir("queuecommand-order");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local child_ready = false

return Def.ActorFrame{
    OnCommand=function(self)
        self:queuecommand("BeginUpdate")
    end,
    BeginUpdateCommand=function(self)
        mod_actions = {
            {1, tostring(child_ready), true},
        }
    end,
    Def.ActorFrame{
        OnCommand=function(self)
            child_ready = true
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Queuecommand Order"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "true");
    }

    #[test]
    fn compile_song_lua_exposes_top_screen_player_positions() {
        let song_dir = test_dir("overlay-player-position");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            local player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
            self:x(player:GetX()):y(player:GetY())
            self:zoomto(48, 64)
        end,
    }
}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Overlay Player Position");
        context.players = [
            SongLuaPlayerContext {
                enabled: true,
                screen_x: 123.0,
                screen_y: 234.0,
                ..SongLuaPlayerContext::default()
            },
            SongLuaPlayerContext {
                enabled: false,
                ..SongLuaPlayerContext::default()
            },
        ];

        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        let overlay = &compiled.overlays[0];

        assert_eq!(overlay.initial_state.x, 123.0);
        assert_eq!(overlay.initial_state.y, 234.0);
        assert_eq!(overlay.initial_state.size, Some([48.0, 64.0]));
    }

    #[test]
    fn compile_song_lua_captures_direct_player_startup_state() {
        let song_dir = test_dir("player-startup-state");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    OnCommand=function(self)
        local p = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        p:x(111):y(222):z(3)
        p:rotationx(10):rotationy(20):rotationz(30)
        p:zoom(0.75):zoomx(0.5):zoomy(1.25)
    end,
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Player Startup State"),
        )
        .unwrap();
        let player = &compiled.player_actors[0];
        assert_eq!(player.initial_state.x, 111.0);
        assert_eq!(player.initial_state.y, 222.0);
        assert_eq!(player.initial_state.z, 3.0);
        assert_eq!(player.initial_state.rot_x_deg, 10.0);
        assert_eq!(player.initial_state.rot_y_deg, 20.0);
        assert_eq!(player.initial_state.rot_z_deg, 30.0);
        assert_eq!(player.initial_state.zoom, 0.75);
        assert_eq!(player.initial_state.zoom_x, 0.5);
        assert_eq!(player.initial_state.zoom_y, 1.25);
    }

    #[test]
    fn compile_song_lua_supports_notefield_column_api() {
        let song_dir = test_dir("notefield-column-api");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    InitCommand=function(self)
        self:SetUpdateFunction(function(actor)
            local ps = GAMESTATE:GetPlayerState(PLAYER_1)
            local pp = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
            local nf = pp:GetChild("NoteField")
            local cols = nf:GetColumnActors()
            if type(cols) ~= "table" or #cols ~= 4 then
                error("expected four note columns")
            end
            nf:SetDidTapNoteCallback(function() end)
            local zh = cols[1]:GetZoomHandler()
            zh:SetSplineMode("NoteColumnSplineMode_Offset")
                :SetSubtractSongBeat(false)
                :SetReceptorT(0.0)
                :SetBeatsPerT(1/48)
            local spline = zh:GetSpline()
            spline:SetSize(2)
            spline:SetPoint(1, {0, 0, 0})
            spline:SetPoint(2, {-1, -1, -1})
            spline:Solve()
            local po = ps:GetPlayerOptions("ModsLevel_Song")
            if po:Mirror() ~= false or po:Left() ~= false or po:Right() ~= false then
                error("unexpected lane permutation")
            end
            if po:Skew() ~= 0 or po:Tilt() ~= 0 then
                error("unexpected skew or tilt")
            end
            if po:GetReversePercentForColumn(0) ~= 0 then
                error("unexpected reverse percent")
            end
            mod_actions = {
                {4, string.format("%.0f:%.0f", ArrowEffects.GetXPos(ps, 1, 0), ArrowEffects.GetYPos(ps, 1, 0)), true},
            }
        end)
    end,
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "NoteField Column API"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "-96:-125");
        assert_eq!(compiled.note_hides.len(), 1);
        assert_eq!(compiled.note_hides[0].player, 0);
        assert_eq!(compiled.note_hides[0].column, 0);
    }

    #[test]
    fn compile_song_lua_captures_column_position_function_eases() {
        let song_dir = test_dir("column-position-function-eases");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local function bounce_col(v)
    local nf = SCREENMAN:GetTopScreen():GetChild("PlayerP1"):GetChild("NoteField")
    local ca = nf:GetColumnActors()[2]
    local ph = ca:GetPosHandler()
    ph:SetSplineMode("NoteColumnSplineMode_Offset")
    ph:SetBeatsPerT(10)
    local spline = ph:GetSpline()
    spline:SetSize(2)
    spline:SetPoint(1, {0, v, 0})
    spline:SetPoint(2, {0, v, 0.001})
    spline:Solve()
end

mods_ease = {
    {4, 0.5, 33.75, 0, bounce_col, "len", ease.outSine},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Column Position Function Eases"),
        )
        .unwrap();

        assert_eq!(compiled.info.unsupported_function_eases, 0);
        assert!(compiled.eases.is_empty());
        assert_eq!(compiled.column_offsets.len(), 1);
        let window = &compiled.column_offsets[0];
        assert_eq!(window.player, 0);
        assert_eq!(window.column, 1);
        assert_eq!(window.unit, SongLuaTimeUnit::Beat);
        assert_eq!(window.span_mode, SongLuaSpanMode::Len);
        assert_eq!(window.start, 4.0);
        assert_eq!(window.limit, 0.5);
        assert!((window.from_y - 33.75).abs() <= 0.001);
        assert!(window.to_y.abs() <= 0.001);
        assert_eq!(window.easing.as_deref(), Some("outSine"));
    }

    #[test]
    fn compile_song_lua_supports_double_style_notefield_columns() {
        let song_dir = test_dir("double-style-notefield-columns");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    InitCommand=function(self)
        self:SetUpdateFunction(function(actor)
            local style = GAMESTATE:GetCurrentStyle()
            local nf = SCREENMAN:GetTopScreen():GetChild("PlayerP1"):GetChild("NoteField")
            local cols = nf:GetColumnActors()
            local col8 = style:GetColumnInfo(PLAYER_1, 8)
            mod_actions = {
                {
                    1,
                    string.format(
                        "%s:%s:%s:%d:%.0f:%d:%.0f:%d:%.0f",
                        style:GetName(),
                        style:GetStepsType(),
                        style:GetStyleType(),
                        style:ColumnsPerPlayer(),
                        style:GetWidth(PLAYER_1),
                        #cols,
                        cols[8]:GetX(),
                        col8.Track,
                        col8.XOffset
                    ),
                    true
                },
            }
        end)
    end,
}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Double Style NoteField Columns");
        context.style_name = "double".to_string();
        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "double:StepsType_Dance_Double:StyleType_OnePlayerTwoSides:8:512:8:224:7:224"
        );
    }

    #[test]
    fn compile_song_lua_player_options_getters_return_scalars() {
        let song_dir = test_dir("player-options-getters");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    InitCommand=function(self)
        local po = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions("ModsLevel_Song")
        if po:Reverse() ~= 0 then
            error("expected reverse getter to default to 0")
        end
        if po:Mini() ~= 0 then
            error("expected mini getter to default to 0")
        end
        po:Reverse(1, 1)
        po:Mini(0.25, 1)
        po:Mirror(true)
        mod_actions = {
            {1, string.format("%.2f:%.2f:%s", po:Reverse(), po:Mini(), tostring(po:Mirror())), true},
        }
    end,
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Player Options Getters"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "1.00:0.25:true");
    }

    #[test]
    fn compile_song_lua_custom_mod_vars_do_not_probe_as_player_options() {
        let song_dir = test_dir("custom-mod-vars-player-options");
        let entry = song_dir.join("default.lua");
        fs::write(
        &entry,
        r#"
local po = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions("ModsLevel_Song")
local activeMods = { ConstellationBg = { value = 1 } }

local function getVar(name)
    local value = 0
    if not po[name] then
        local activeMod = activeMods[name]
        if activeMod and activeMod.value then
            value = activeMod.value
        end
    end
    return value
end

return Def.ActorFrame{
    InitCommand=function(self)
        if type(po["MoveX16"]) ~= "function" then
            error("expected real multicol PlayerOptions method")
        end
        if po["ConstellationBg"] ~= nil then
            error("custom mod variable should not be a PlayerOptions method")
        end
    end,
    Def.ActorFrame{
        Name="ConstellationFrame",
        ConstellationBgShowMessageCommand=function(self)
            if getVar("ConstellationBg") ~= 1 then
                error("custom mod variable masked by PlayerOptions")
            end
            local decoy = Def.ActorFrame{ Name="Decoy" }
            local constellationFrame = decoy:GetChild("ConstellationFrame")
            local starfieldFrame = constellationFrame and constellationFrame:GetChild("StarfieldFrame")
            local starFrame = starfieldFrame and starfieldFrame:GetChild("StarFrame")
            if starFrame ~= nil then
                error("missing actor children should not synthesize a StarFrame")
            end
            self:x(12)
        end,
    },
    Def.ActorFrame{
        InitCommand=function(self)
            self:SetUpdateFunction(function()
                MESSAGEMAN:Broadcast("ConstellationBgShow")
            end)
        end,
    },
}
"#,
    )
    .unwrap();

        test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Custom Mod Vars"),
        )
        .unwrap();
    }

    #[test]
    fn compile_song_lua_player_options_speed_setters_chain() {
        let song_dir = test_dir("player-options-speed-setters");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    InitCommand=function(self)
        local po = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions("ModsLevel_Song")
        local initial = string.format("%.2f:%s:%s", po:XMod(), tostring(po:CMod()), tostring(po:NoMines()))
        po:XMod(3.5, 9e9, true):Overhead(true, 9e9):Mini(0.15, 9e9, true)
        local after_x = string.format("%.2f:%s:%.2f", po:XMod(), tostring(po:Overhead()), po:Mini())
        po:CMod(650, 1)
        local after_c = string.format("%s:%.0f:%s", tostring(po:XMod()), po:CMod(), tostring(po:MMod()))
        po:CMod(nil, 1):MMod(700, 1)
        local after_m = string.format("%s:%s:%.0f", tostring(po:XMod()), tostring(po:CMod()), po:MMod())
        mod_actions = {
            {1, table.concat({initial, after_x, after_c, after_m}, "|"), true},
        }
    end,
}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Player Options Speed Setters");
        context.players[0].speedmod = SongLuaSpeedMod::X(2.25);
        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "2.25:nil:false|3.50:true:0.15|nil:650:nil|nil:nil:700"
        );
    }

    #[test]
    fn compile_song_lua_player_options_from_string_parses_common_mods() {
        let song_dir = test_dir("player-options-from-string");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    InitCommand=function(self)
        local ps = GAMESTATE:GetPlayerState(PLAYER_1)
        local po = ps:GetPlayerOptions("ModsLevel_Song")
        po:FromString("*4 350% Drunk, 50% Mini, Overhead, NoMines, 3.5x")
        local parsed_x = string.format(
            "%.1f:%.1f:%.2f:%s:%s",
            po:XMod(),
            po:Drunk(),
            po:Mini(),
            tostring(po:Overhead()),
            tostring(po:NoMines())
        )
        po:FromString("C650, 0% Overhead")
        local parsed_c = string.format("%s:%.0f:%s", tostring(po:XMod()), po:CMod(), tostring(po:Overhead()))
        ps:SetPlayerOptions("ModsLevel_Song", "M700, Shuffle, 25% Tiny")
        local parsed_set = string.format(
            "%s:%.0f:%s:%.2f:%s",
            tostring(po:CMod()),
            po:MMod(),
            tostring(po:Shuffle()),
            po:Tiny(),
            ps:GetPlayerOptionsString("ModsLevel_Song")
        )
        mod_actions = {
            {1, table.concat({parsed_x, parsed_c, parsed_set}, "|"), true},
        }
    end,
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Player Options FromString"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "3.5:3.5:0.50:true:true|nil:650:false|nil:700:true:0.25:M700, Shuffle, 25% Tiny"
        );
    }

    #[test]
    fn compile_song_lua_player_options_exposes_modchart_gates() {
        let song_dir = test_dir("player-options-modchart-gates");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    InitCommand=function(self)
        local required = {
            "CAMod",
            "DrawSize",
            "TanDrunk",
            "LifeSetting",
            "DrainSetting",
            "HideLightSetting",
            "ModTimerSetting",
            "WavePeriod",
            "PulseInner",
            "BounceZ",
            "TanDigitalZPeriod",
            "MoveX16",
            "ConfusionOffset16",
            "MinTNSToHideNotes",
            "VisualDelay",
            "UsingReverse",
            "GetStepAttacks",
            "IsEasierForSongAndSteps",
            "IsEasierForCourseAndTrail",
        }
        for _, name in ipairs(required) do
            if not PlayerOptions[name] then
                error("expected PlayerOptions capability gate: " .. name)
            end
        end
        local po = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions("ModsLevel_Song")
        po:CAMod(640, 9e9, true):DrawSize(0.25, 9e9):DizzyHolds(true):StealthPastReceptors(true)
        local direct = string.format(
            "%s:%.0f:%.2f:%s:%s",
            tostring(po:CMod()),
            po:CAMod(),
            po:DrawSize(),
            tostring(po:DizzyHolds()),
            tostring(po:StealthPastReceptors())
        )
        po:LifeSetting("LifeType_Battery")
            :DrainSetting("DrainType_NoRecover")
            :HideLightSetting("HideLightType_HideAllLights")
            :ModTimerSetting("ModTimerType_Beat")
            :FailSetting("FailType_Off")
            :MinTNSToHideNotes("TapNoteScore_W3")
            :WavePeriod(2.5)
            :PulseInner(0.25)
            :BounceZ(3)
            :TanDigitalZPeriod(4)
            :MoveX16(0.75)
            :Reverse(1)
            :VisualDelay(0.12)
            :BatteryLives(4)
            :Passmark(0.2)
        local surface = string.format(
            "%s:%s:%s:%s:%s:%s:%.1f:%.2f:%.0f:%.0f:%.2f:%.0f:%s:%.0f:%.0f:%s:%s:%.1f",
            po:LifeSetting(),
            po:DrainSetting(),
            po:HideLightSetting(),
            po:ModTimerSetting(),
            po:FailSetting(),
            po:MinTNSToHideNotes(),
            po:WavePeriod(),
            po:PulseInner(),
            po:BounceZ(),
            po:TanDigitalZPeriod(),
            po:MoveX16(),
            po:BatteryLives(),
            tostring(po:UsingReverse()),
            po:GetReversePercentForColumn(0),
            po:GetStepAttacks(),
            tostring(po:IsEasierForSongAndSteps(GAMESTATE:GetCurrentSong(), GAMESTATE:GetCurrentSteps(PLAYER_1), PLAYER_1)),
            tostring(po:IsEasierForCourseAndTrail(GAMESTATE:GetCurrentCourse(), GAMESTATE:GetCurrentTrail(PLAYER_1))),
            po:Passmark()
        )
        po:FromString("*9999 DizzyHolds, *9999 StealthPastReceptors, CA720")
        local parsed = string.format("%s:%.0f:%s", tostring(po:CMod()), po:CAMod(), tostring(po:DizzyHolds()))
        mod_actions = {
            {1, direct .. "|" .. parsed .. "|" .. surface, true},
        }
    end,
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Player Options Modchart Gates"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "nil:640:0.25:true:true|nil:720:true|LifeType_Battery:DrainType_NoRecover:HideLightType_HideAllLights:ModTimerType_Beat:FailType_Off:TapNoteScore_W3:2.5:0.25:3:4:0.75:4:true:1:1:false:false:0.2"
        );
    }

    #[test]
    fn compile_song_lua_supports_player_option_timing_windows() {
        let song_dir = test_dir("player-options-timing-windows");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    InitCommand=function(self)
        local ps = GAMESTATE:GetPlayerState(PLAYER_1)
        local po = ps:GetPlayerOptions("ModsLevel_Preferred")
        po:DisableTimingWindow("TimingWindow_W5")
            :DisableTimingWindow("W3")
            :DisableTimingWindow(2)
            :DisableTimingWindow("TimingWindow_W5")
        local before = po:GetDisabledTimingWindows()
        po:ResetDisabledTimingWindows()
        po:DisableTimingWindow("TimingWindow_W4")
        local after = po:GetDisabledTimingWindows()
        mod_actions = {
            {
                1,
                string.format(
                    "%d:%s:%s:%s:%d:%s:%s",
                    #before,
                    before[1],
                    before[2],
                    before[3],
                    #after,
                    after[1],
                    ps:GetPlayerController()
                ),
                true,
            },
            {
                2,
                function()
                    po:ResetDisabledTimingWindows()
                    po:DisableTimingWindow("TimingWindow_W1")
                end,
                true,
            },
        }
    end,
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Player Option Timing Windows"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "3:TimingWindow_W2:TimingWindow_W3:TimingWindow_W5:1:TimingWindow_W4:PlayerController_Human"
        );
        assert_eq!(compiled.info.unsupported_function_actions, 0);
    }

    #[test]
    fn compile_song_lua_exposes_life_meter_and_health_state_helpers() {
        let song_dir = test_dir("life-meter-health-state");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    InitCommand=function(self)
        local ps = GAMESTATE:GetPlayerState(PLAYER_1)
        local top = SCREENMAN:GetTopScreen()
        local current_options = ps:GetCurrentPlayerOptions()
        local requested_options = ps:GetPlayerOptions("ModsLevel_Preferred")
        current_options:NoteSkin("metal")
        if requested_options:NoteSkin() ~= "metal" then
            error("expected current and requested player options to share state")
        end
        local life = top:GetLifeMeter(ps:GetPlayerNumber())
        local child_life = top:GetChild("Life"..ToEnumShortString(ps:GetPlayerNumber()))
        local generic_life = top:GetChild("LifeMeter")
        mod_actions = {
            {
                1,
                string.format(
                    "%s:%s:%s:%.1f:%.1f:%s:%s:%s:%s:%d:%d",
                    ps:GetPlayerNumber(),
                    ps:GetHealthState(),
                    ps:GetPlayerController(),
                    life:GetLife(),
                    child_life:GetLife(),
                    tostring(life:IsFailing()),
                    tostring(life:IsInDanger()),
                    tostring(life:IsHot()),
                    tostring(generic_life ~= nil),
                    HealthState:Reverse()[ps:GetHealthState()],
                    PlayerController:Reverse()[ps:GetPlayerController()]
                ),
                true,
            },
        }
    end,
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Life Meter Health State"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "PlayerNumber_P1:HealthState_Alive:PlayerController_Human:0.5:0.5:false:false:false:true:1:0"
        );
    }

    #[test]
    fn compile_song_lua_exposes_top_screen_score_percent_children() {
        let song_dir = test_dir("top-screen-score-percent-children");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    InitCommand=function(self)
        local top = SCREENMAN:GetTopScreen()
        local p1 = top:GetChild("ScoreP1"):GetChild("ScoreDisplayPercentage Percent"):GetChild("PercentP1")
        local p2 = top:GetChild("ScoreP2"):GetChild("ScoreDisplayPercentage Percent"):GetChild("PercentP2")
        local score1 = tonumber(string.sub(p1:GetText(), 1, -2))
        local score2 = tonumber(string.sub(p2:GetText(), 1, -2))
        mod_actions = {
            {
                1,
                string.format(
                    "%s:%s:%.0f:%.0f:%s:%s:%s",
                    top:GetChild("ScoreP1"):GetName(),
                    top:GetChild("ScoreP2"):GetName(),
                    score1,
                    score2,
                    p1:GetText(),
                    p2:GetName(),
                    tostring(p1:GetParent():GetName())
                ),
                true,
            },
        }
    end,
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Score Percent Children"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "ScoreP1:ScoreP2:0:0:0.00%:PercentP2:ScoreDisplayPercentage Percent"
        );
        assert_eq!(compiled.info.unsupported_function_actions, 0);
    }

    #[test]
    fn compile_song_lua_exposes_top_screen_theme_actor_shapes() {
        let song_dir = test_dir("top-screen-theme-actor-shapes");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    InitCommand=function(self)
        local top = SCREENMAN:GetTopScreen()
        local bpm = top:GetChild("BPMDisplay")
        local title = top:GetChild("SongTitle")
        local steps = top:GetChild("StepsDisplayP1")
        local underlay = top:GetChild("Underlay")
        local p1_score = underlay:GetChild("P1Score")
        local song_meter_title = underlay:GetChild("SongMeter"):GetChild("SongTitle")
        local screen_meter = top:GetChild("SongMeterDisplayP1")
        local stream = screen_meter:GetChild("Stream")
        local overlay = top:GetChild("Overlay")
        mod_actions = {
            {
                1,
                string.format(
                    "%s:%s:%s:%s:%s:%s:%s:%s:%s:%s",
                    bpm:GetName(),
                    bpm:GetText(),
                    title:GetText(),
                    steps:GetText(),
                    p1_score:GetName(),
                    p1_score:GetText(),
                    song_meter_title:GetText(),
                    screen_meter:GetName(),
                    stream:GetName(),
                    overlay:GetName()
                ),
                true,
            },
        }
    end,
}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Theme Actor Shapes");
        context.song_display_bpms = [120.0, 180.0];
        context.players[0].difficulty = SongLuaDifficulty::Hard;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "BPMDisplay:120 - 180:Theme Actor Shapes:Difficulty_Hard:P1Score:0.00%:Theme Actor Shapes:SongMeterDisplayP1:Stream:Overlay"
        );
        assert_eq!(compiled.info.unsupported_function_actions, 0);
    }

    #[test]
    fn compile_song_lua_enumerates_top_screen_theme_children() {
        let song_dir = test_dir("top-screen-theme-child-enumeration");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    InitCommand=function(self)
        local top = SCREENMAN:GetTopScreen()
        local children = top:GetChildren()
        local wanted = {
            "PlayerP1",
            "PlayerP2",
            "Underlay",
            "Overlay",
            "BPMDisplay",
            "SongForeground",
            "SongBackground",
            "ScoreP1",
            "ScoreP2",
            "SongTitle",
            "SongMeterDisplayP1",
            "StepsDisplayP1",
        }
        for _, name in ipairs(wanted) do
            assert(children[name], name)
        end
        local underlay_children = children.Underlay:GetChildren()
        assert(underlay_children.P1Score:GetText() == "0.00%")
        assert(underlay_children.SongMeter:GetChild("SongTitle"):GetText() == "Enumeration")
        mod_actions = {
            {
                1,
                string.format(
                    "%s:%s:%s:%s:%s:%s",
                    tostring(top:GetNumChildren() >= 20),
                    children.BPMDisplay:GetText(),
                    children.StepsDisplayP1:GetText(),
                    underlay_children.P1Score:GetName(),
                    underlay_children.SongMeter:GetChild("SongTitle"):GetText(),
                    tostring(children.PlayerP1 == top:GetChild("PlayerP1"))
                ),
                true,
            },
        }
    end,
}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Enumeration");
        context.song_display_bpms = [150.0, 150.0];
        context.players[0].difficulty = SongLuaDifficulty::Challenge;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "true:150:Difficulty_Challenge:P1Score:Enumeration:true"
        );
        assert_eq!(compiled.info.unsupported_function_actions, 0);
    }

    #[test]
    fn compile_song_lua_labels_actors_for_tostring_scans() {
        let song_dir = test_dir("actor-tostring-scans");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    InitCommand=function(self)
        local top = SCREENMAN:GetTopScreen()
        local children = top:GetChildren()
        local player = tostring(children.PlayerP1)
        local underlay = tostring(children.Underlay)
        local steps = tostring(children.StepsDisplayP1)
        local score = tostring(children.ScoreP1:GetChild("ScoreDisplayPercentage Percent"))
        mod_actions = {
            {
                1,
                string.format(
                    "%s:%s:%s:%s",
                    tostring(player:find("Player") ~= nil),
                    tostring(underlay:find("ActorFrame") ~= nil),
                    tostring(steps:find("StepsDisplayP1") ~= nil),
                    tostring(score:find("PercentageDisplay") ~= nil)
                ),
                true,
            },
        }
    end,
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor Tostring Scans"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "true:true:true:true");
        assert_eq!(compiled.info.unsupported_function_actions, 0);
    }

    #[test]
    fn compile_song_lua_exposes_lowercase_getrotation() {
        let song_dir = test_dir("actor-lowercase-getrotation");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    InitCommand=function(self)
        local player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        player:rotationx(10):rotationy(20):rotationz(30)
        player:addrotationx(5):addrotationy(6):addrotationz(7)
        local rx, ry, rz = player:getrotation()
        mod_actions = {
            {
                1,
                string.format("%.0f:%.0f:%.0f", rx, ry, rz),
                true,
            },
        }
    end,
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Actor Lowercase Getrotation"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "15:26:37");
        assert_eq!(compiled.info.unsupported_function_actions, 0);
    }

    #[test]
    fn compile_song_lua_exposes_top_screen_edit_state() {
        let song_dir = test_dir("top-screen-edit-state");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
assert(EditState:Reverse()["EditState_Playing"] == 3)

return Def.ActorFrame{
    OnCommand=function(self)
        local top = SCREENMAN:GetTopScreen()
        mod_actions = {
            {1, top:GetEditState(), true},
        }
    end,
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Top Screen Edit State"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "EditState_Playing");
        assert_eq!(compiled.info.unsupported_function_actions, 0);
    }

    #[test]
    fn compile_song_lua_supports_gameplay_layout_and_note_field_shims() {
        let song_dir = test_dir("gameplay-layout-note-field-shims");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    InitCommand=function(self)
        local mods = SL.P1.ActiveModifiers
        local opts = GAMESTATE:GetPlayerState(PLAYER_1):GetCurrentPlayerOptions()
        local layout = GetGameplayLayout(PLAYER_1, opts:Reverse() ~= 0)
        local notefield = GetPlayerAF("P1"):GetChild("NoteField")
        local set_bars = notefield:SetBeatBars(true)
        local set_alpha = notefield:SetBeatBarsAlpha(0.75, 0.5, 0.25, 0)
        local alpha = notefield.__songlua_beat_bars_alpha
        mod_actions = {
            {
                1,
                string.format(
                    "%s:%s:%s:%s:%s:%s:%s:%.0f:%.0f:%s:%s:%s:%.2f:%.2f",
                    mods.ErrorBar,
                    mods.MeasureCounter,
                    mods.MeasureLines,
                    tostring(mods.ColumnCues),
                    mods.Spacing,
                    tostring(mods.MeasureCounterUp),
                    tostring(mods.SubtractiveScoring),
                    layout.Combo.y,
                    layout.SubtractiveScoring.y,
                    tostring(set_bars == notefield),
                    tostring(set_alpha == notefield),
                    tostring(notefield.__songlua_beat_bars),
                    alpha[1],
                    alpha[3]
                ),
                true,
            },
        }
    end,
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Gameplay Layout Note Field Shims"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "None:None:Off:false:0%:true:false:270:182:true:true:true:0.75:0.25"
        );
    }

    #[test]
    fn compile_song_lua_supports_nameless_player_group_and_tap_note_shim() {
        let song_dir = test_dir("nameless-player-group-tap-note-shim");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    InitCommand=function(self)
        local top = SCREENMAN:GetTopScreen()
        local group = top:GetChild("")
        local player = group[1]
        local direct = top:GetChild("PlayerP1")
        local nf = player:GetChild("NoteField")
        local seen = {}
        nf:set_did_tap_note_callback(function(col, score, bright)
            seen = {col, score, bright}
        end)
        local ret = nf:did_tap_note(2, "TapNoteScore_W1", true)
        mod_actions = {
            {
                1,
                string.format(
                    "%d:%s:%s:%d:%s:%s:%s:%d:%s:%s",
                    #group,
                    player:GetName(),
                    tostring(player == direct),
                    seen[1],
                    seen[2],
                    tostring(seen[3]),
                    tostring(ret == nf),
                    nf.__songlua_last_tap_note_column,
                    nf.__songlua_last_tap_note_score,
                    tostring(nf.__songlua_last_tap_note_bright)
                ),
                true,
            },
        }
    end,
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Nameless Player Group Tap Note Shim"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "2:PlayerP1:true:2:TapNoteScore_W1:true:true:2:TapNoteScore_W1:true"
        );
        assert_eq!(compiled.info.unsupported_function_actions, 0);
    }

    #[test]
    fn compile_song_lua_extracts_actorframe_overlay_hierarchy() {
        let song_dir = test_dir("overlay-hierarchy");
        let entry = song_dir.join("default.lua");
        let overlay_dir = song_dir.join("gfx");
        fs::create_dir_all(&overlay_dir).unwrap();
        fs::write(
            overlay_dir.join("grid.png"),
            b"not-an-image-but-good-enough-for-parser",
        )
        .unwrap();
        fs::write(
            &entry,
            r#"
local wrapper = nil

mod_actions = {
    {8, function()
        if wrapper then
            wrapper:visible(true)
            wrapper:zoom(2)
        end
    end, true},
}

return Def.ActorFrame{
    Def.ActorFrame{
        InitCommand=function(self)
            wrapper = self
            self:visible(false)
        end,
        OnCommand=function(self)
            self:xy(SCREEN_CENTER_X, SCREEN_CENTER_Y)
        end,
        Def.Sprite{
            Texture="gfx/grid.png",
            OnCommand=function(self)
                self:xy(10, 20)
            end,
        },
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Overlay Hierarchy"),
        )
        .unwrap();
        assert_eq!(compiled.info.unsupported_function_actions, 0);
        assert_eq!(compiled.overlays.len(), 2);
        assert!(matches!(
            compiled.overlays[0].kind,
            SongLuaOverlayKind::ActorFrame
        ));
        assert_eq!(compiled.overlays[0].parent_index, None);
        assert_eq!(compiled.overlays[0].initial_state.x, 320.0);
        assert_eq!(compiled.overlays[0].initial_state.y, 240.0);
        assert!(!compiled.overlays[0].initial_state.visible);
        assert_eq!(compiled.overlays[0].message_commands.len(), 1);
        assert_eq!(
            compiled.overlays[0].message_commands[0].blocks[0]
                .delta
                .zoom,
            Some(2.0)
        );
        assert_eq!(
            compiled.overlays[0].message_commands[0].blocks[0]
                .delta
                .visible,
            Some(true)
        );
        assert!(matches!(
            compiled.overlays[1].kind,
            SongLuaOverlayKind::Sprite { ref texture_path, .. }
                if texture_path.ends_with("gfx/grid.png")
        ));
        assert_eq!(compiled.overlays[1].parent_index, Some(0));
        assert_eq!(compiled.overlays[1].initial_state.x, 10.0);
        assert_eq!(compiled.overlays[1].initial_state.y, 20.0);
    }

    #[test]
    fn compile_song_lua_captures_player_and_song_foreground_actions() {
        let song_dir = test_dir("player-foreground-actions");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
mod_actions = {
    {0, function()
        local p = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        local fg = SCREENMAN:GetTopScreen():GetChild("SongForeground")
        if p then
            p:linear(1):x(SCREEN_CENTER_X + 40):z(5):zoom(0.6):rotationz(15)
        end
        if fg then
            fg:z(4)
        end
    end, true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Player Foreground Actions"),
        )
        .unwrap();
        assert_eq!(compiled.info.unsupported_function_actions, 0);
        assert_eq!(compiled.player_actors[0].message_commands.len(), 1);
        assert_eq!(compiled.song_foreground.message_commands.len(), 1);
        let player_block = &compiled.player_actors[0].message_commands[0].blocks[0];
        assert_eq!(player_block.delta.x, Some(360.0));
        assert_eq!(player_block.delta.z, Some(5.0));
        assert_eq!(player_block.delta.zoom, Some(0.6));
        assert_eq!(player_block.delta.rot_z_deg, Some(15.0));
        let fg_block = &compiled.song_foreground.message_commands[0].blocks[0];
        assert_eq!(fg_block.delta.z, Some(4.0));
    }

    #[test]
    fn compile_song_lua_captures_function_actions_via_broadcast() {
        let song_dir = test_dir("broadcast-function-action");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
mod_actions = {
    {2, function()
        MESSAGEMAN:Broadcast("Flash")
    end, true},
}

return Def.ActorFrame{
    Def.Quad{
        FlashMessageCommand=function(self)
            self:linear(0.5)
            self:x(96)
            self:diffusealpha(0.5)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Broadcast Function Action"),
        )
        .unwrap();
        assert_eq!(compiled.info.unsupported_function_actions, 0);
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.overlays.len(), 1);
        assert_eq!(compiled.overlays[0].message_commands.len(), 1);
        let block = &compiled.overlays[0].message_commands[0].blocks[0];
        assert_eq!(block.duration, 0.5);
        assert_eq!(block.delta.x, Some(96.0));
        assert_eq!(block.delta.diffuse.unwrap()[3], 0.5);
    }

    #[test]
    fn compile_song_lua_accepts_side_effect_only_function_actions() {
        let song_dir = test_dir("function-action-side-effects");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
mod_actions = {
    {1, function() SCREENMAN:SystemMessage("hello") end, true},
    {2, function() SM("hello") end, true},
    {3, function() SCREENMAN:SetNewScreen("ScreenGameplay") end, true},
    {4, function() SCREENMAN:GetTopScreen():StartTransitioningScreen("SM_DoNextScreen") end, true},
    {5, function() MESSAGEMAN:Broadcast("NoListeners") end, true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Function Action Side Effects"),
        )
        .unwrap();
        assert_eq!(compiled.info.unsupported_function_actions, 0);
        assert!(compiled.messages.is_empty());
        assert!(compiled.overlays.is_empty());
    }

    #[test]
    fn compile_song_lua_accepts_offline_theme_io_network_helpers() {
        let song_dir = test_dir("offline-theme-io-network");
        let plugin_dir = song_dir.join("plugins");
        fs::create_dir_all(plugin_dir.join("nested")).unwrap();
        fs::write(plugin_dir.join("alpha.lua"), "payload").unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local function has_value(values, wanted)
    for _, value in ipairs(values) do
        if value == wanted then return true end
    end
    return false
end

local theme_info = IniFile.ReadFile(THEME:GetCurrentThemeDirectory() .. "ThemeInfo.ini").ThemeInfo
assert(theme_info.DisplayName == "Simply Love")
local missing_ini = IniFile.ReadFile("Save/Missing.ini")
assert(type(missing_ini) == "table" and next(missing_ini) == nil)
assert(IniFile.WriteFile("Save/ThemePrefs.ini", {SimplyLove={DarkUI=true}}) == true)
local plugins = GAMESTATE:GetCurrentSong():GetSongDir() .. "plugins/"
local plugin_files = FILEMAN:GetDirListing(plugins)
local plugin_dirs = FILEMAN:GetDirListing(plugins, true, false)
local plugin_lua_paths = FILEMAN:GetDirListing(plugins .. "*.lua", false, true)
assert(#plugin_files == 2 and has_value(plugin_files, "alpha.lua") and has_value(plugin_files, "nested"))
assert(#plugin_dirs == 1 and plugin_dirs[1] == "nested")
assert(#plugin_lua_paths == 1 and plugin_lua_paths[1]:match("alpha%.lua$"))
assert(FILEMAN:DoesFileExist(plugins .. "alpha.lua") == true)
assert(FILEMAN:DoesFileExist(plugins .. "nested") == true)
assert(FILEMAN:DoesFileExist(plugins .. "missing.lua") == false)
assert(FILEMAN:GetFileSizeBytes(plugins .. "alpha.lua") == 7)
assert(FILEMAN:GetFileSizeBytes(plugins .. "missing.lua") == 0)
assert(FILEMAN:GetHashForFile(plugins .. "alpha.lua") == 0)
local encoded = JsonEncode({a=1, b="two words", nested={true,false}})
local decoded = JsonDecode(encoded)
assert(decoded.a == 1)
assert(BinaryToHex(CRYPTMAN:SHA1String("chart")) == string.rep("0", 40))
assert(BinaryToHex(CRYPTMAN:SHA1File("scores.json")) == string.rep("0", 40))
assert(CRYPTMAN:GenerateRandomUUID() == "00000000-0000-4000-8000-000000000000")
assert(NETWORK:IsUrlAllowed("https://example.invalid") == false)
assert(NETWORK:EncodeQueryParameters({b="two words", a=1}) == "a=1&b=two%20words")
local request = NETWORK:HttpRequest{url="https://example.invalid"}
assert(request.body == "" and request.status == 0 and request.code == 0 and request.error == "offline")
assert(type(request.headers) == "table")
assert(request:IsFinished() == true)
assert(request:GetResponse() == request)
local ws = NETWORK:WebSocket{url="wss://example.invalid"}
assert(ws.is_open == false and ws:IsOpen() == false)
assert(ws:Send(JsonEncode({uuid=CRYPTMAN:GenerateRandomUUID()})) == nil)
assert(ws:Close() == nil)
local file = RageFileUtil:CreateRageFile()
assert(file:Open("Save/Offline.json", 2))
assert(file:Write(encoded))
assert(file:Read() == "")
assert(file:Close() == nil)
assert(file:destroy() == nil)
local dot_file = RageFileUtil.CreateRageFile()
assert(dot_file:Read() == "")
assert(FILEMAN:Unzip("archive.zip", "Songs/Pack") == false)
assert(GetTimingWindow(2) > GetTimingWindow(1))
assert(GetWorstJudgment({{0, GetTimingWindow(3)}}) == 3)
local ex, points, possible = CalculateExScore(PLAYER_1)
assert(ex == 0 and points == 0 and possible == 0)

mod_actions = {
    {1, function()
        assert(NETWORK:HttpRequest{url="https://example.invalid", body=JsonEncode(decoded)}:Cancel() == nil)
        GAMESTATE:JoinPlayer(PLAYER_1)
        assert(CRYPTMAN:SignFileToFile("Save/Offline.json", "Save/Offline.sig") == false)
        assert(FILEMAN:Copy(plugins .. "alpha.lua", "Save/alpha.lua") == false)
        assert(FILEMAN:CreateDir("Save") == true)
        assert(FILEMAN:Remove("Save/Offline.json") == true)
        assert(FILEMAN:FlushDirCache() == nil)
        assert(IsHumanPlayer(PLAYER_1) == GAMESTATE:IsSideJoined(PLAYER_1))
    end, true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Offline Theme Helpers"),
        )
        .unwrap();
        assert_eq!(compiled.info.unsupported_function_actions, 0);
        assert!(compiled.messages.is_empty());
        assert!(compiled.overlays.is_empty());
    }

    #[test]
    fn compile_song_lua_exposes_sha256_crypt_helpers() {
        let song_dir = test_dir("sha256-crypt-helpers");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local key = CRYPTMAN:SHA256String("player-scores")
local file_key = CRYPTMAN:SHA256File("scores.json")

mod_actions = {
    {
        1,
        string.format(
            "%d:%d:%s:%s",
            #key,
            #file_key,
            BinaryToHex(key),
            BinaryToHex(file_key)
        ),
        true,
    },
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "SHA256 Crypt Helpers"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            format!("32:32:{}:{}", "0".repeat(64), "0".repeat(64))
        );
    }

    #[test]
    fn compile_song_lua_accepts_lua_file_and_profile_helpers() {
        let song_dir = test_dir("lua-file-profile-helpers");
        fs::write(song_dir.join("favorites.txt"), "Group/Song\n").unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
assert(lua.ReadFile("favorites.txt") == "Group/Song\n")
assert(lua.ReadFile("missing.txt") == nil)
Warn("compile warning")
lua.ReportScriptError("compile error report")

local profile = PROFILEMAN:GetProfile(PLAYER_1)
assert(PROFILEMAN:GetProfile(PLAYER_1) == profile)
assert(PROFILEMAN:GetProfile(ProfileSlot[1]) == profile)
assert(PROFILEMAN:GetProfile(ProfileSlot[3]) == PROFILEMAN:GetMachineProfile())
assert(PROFILEMAN:GetProfileDir(ProfileSlot[3]) == "/Save/MachineProfile/")
assert(PROFILEMAN:LocalProfileIDToDir("abc") == "/Save/LocalProfiles/abc/")
assert(PROFILEMAN:GetLocalProfile("missing") == nil)
assert(PROFILEMAN:GetLocalProfileIndexFromID("missing") == -1)
assert(#PROFILEMAN:GetLocalProfileIDs() == 0)
assert(#PROFILEMAN:GetLocalProfileDisplayNames() == 0)
assert(PROFILEMAN:IsSongNew(GAMESTATE:GetCurrentSong()) == false)
assert(PROFILEMAN:ProfileWasLoadedFromMemoryCard(PLAYER_1) == false)
assert(PROFILEMAN:LastLoadWasTamperedOrCorrupt(PLAYER_1) == false)
assert(PROFILEMAN:ProfileFromMemoryCardIsNew(PLAYER_1) == false)
assert(PROFILEMAN:GetSongNumTimesPlayed(GAMESTATE:GetCurrentSong(), ProfileSlot[1]) == 0)
assert(PROFILEMAN:SaveProfile(PLAYER_1) == false)
assert(PROFILEMAN:SaveLocalProfile("missing") == false)
assert(PROFILEMAN:SetStatsPrefix("Stats") == PROFILEMAN)
assert(PROFILEMAN:GetStatsPrefix() == "Stats")
assert(profile:GetType() == "ProfileType_Normal")
assert(profile:GetPriority() == 0)
assert(profile:SetDisplayName("AAA") == profile)
assert(profile:GetDisplayName() == "AAA")
assert(profile:GetCaloriesBurnedToday() == 0)
assert(profile:GetNumTotalSongsPlayed() == 0)
assert(profile:GetTotalNumSongsPlayed() == 0)
assert(profile:GetTotalSessions() == 0)
assert(profile:GetIgnoreStepCountCalories() == false)
assert(profile:CalculateCaloriesFromHeartRate(120, 60) == 0)
assert(profile:SetWeightPounds(180) == profile)
assert(profile:GetWeightPounds() == 180)
assert(profile:SetVoomax(42.5) == profile)
assert(profile:GetVoomax() == 42.5)
assert(profile:SetBirthYear(2000) == profile)
assert(profile:GetBirthYear() == 2000)
assert(profile:SetIgnoreStepCountCalories(true) == profile)
assert(profile:GetIgnoreStepCountCalories() == true)
assert(profile:SetIsMale(false) == profile)
assert(profile:GetIsMale() == false)
assert(profile:SetGoalType("GoalType_Calories") == profile)
assert(profile:GetGoalType() == "GoalType_Calories")
assert(profile:SetGoalCalories(120) == profile)
assert(profile:GetGoalCalories() == 120)
assert(profile:SetGoalSeconds(90) == profile)
assert(profile:GetGoalSeconds() == 90)
assert(profile:AddCaloriesToDailyTotal(5) == profile)
assert(profile:GetCaloriesBurnedToday() == 5)
assert(profile:GetTotalCaloriesBurned() == 5)
assert(profile:GetDisplayTotalCaloriesBurned() == "5 Cal")
assert(profile:SetLastUsedHighScoreName("AAA") == profile)
assert(profile:GetLastUsedHighScoreName() == "AAA")
assert(profile:GetAllUsedHighScoreNames()[1] == "AAA")
assert(profile:GetCategoryHighScoreList("StepsType_Dance_Single", "RankingCategory_a"):GetHighScores()[1] ~= nil)
assert(profile:GetCharacter() == nil)
assert(profile:SetCharacter("default") == profile)
assert(profile:GetCharacter() == "default")
assert(profile:IsCodeUnlocked("code") == false)
assert(profile:GetSongsActual("StepsType_Dance_Single", "Difficulty_Medium") == 0)
assert(profile:GetCoursesActual("StepsType_Dance_Single", "Difficulty_Medium") == 0)
assert(profile:GetSongsPossible("StepsType_Dance_Single", "Difficulty_Medium") == 0)
assert(profile:GetCoursesPossible("StepsType_Dance_Single", "Difficulty_Medium") == 0)
assert(profile:GetSongsPercentComplete("StepsType_Dance_Single", "Difficulty_Medium") == 0)
assert(profile:GetCoursesPercentComplete("StepsType_Dance_Single", "Difficulty_Medium") == 0)
assert(profile:GetTotalStepsWithTopGrade("StepsType_Dance_Single", "Difficulty_Medium", "Grade_Tier07") == 0)
assert(profile:GetTotalTrailsWithTopGrade("StepsType_Dance_Single", "Difficulty_Medium", "Grade_Tier07") == 0)
assert(profile:GetTotalSessionSeconds() == 0)
assert(profile:GetTotalGameplaySeconds() == 0)
assert(profile:GetSongsAndCoursesPercentCompleteAllDifficulties("StepsType_Dance_Single") == 0)
assert(profile:GetMostPopularSong() == nil)
assert(profile:GetMostPopularCourse() == nil)
assert(profile:GetSongNumTimesPlayed(GAMESTATE:GetCurrentSong()) == 0)
assert(profile:HasPassedAnyStepsInSong(GAMESTATE:GetCurrentSong()) == false)
assert(profile:GetNumToasties() == 0)
assert(profile:GetTotalTapsAndHolds() == 0)
assert(profile:GetTotalJumps() == 0)
assert(profile:GetTotalHolds() == 0)
assert(profile:GetTotalRolls() == 0)
assert(profile:GetTotalMines() == 0)
assert(profile:GetTotalHands() == 0)
assert(profile:GetTotalLifts() == 0)
assert(profile:GetTotalDancePoints() == 0)
assert(profile:GetLastPlayedSong() == nil)
assert(profile:GetLastPlayedCourse() == nil)
assert(#profile:get_songs() == 0)
profile:GetUserTable().note = "x"
assert(profile:GetUserTable().note == "x")
assert(PROFILEMAN:GetLocalProfileFromIndex(0):GetDisplayName() == "Local Profile")

mod_actions = {
    {1, function()
        lua.WriteFile("favorites.txt", "Group/Song\n")
        lua.ReportScriptError("action report")
        Warn("action warning")
        local p = PROFILEMAN:GetProfile(PLAYER_1)
        p:SetLastUsedHighScoreName("BBB")
        p:AddCaloriesToDailyTotal(p:CalculateCaloriesFromHeartRate(90, 30))
    end, true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Lua File Profile Helpers"),
        )
        .unwrap();
        assert_eq!(compiled.info.unsupported_function_actions, 0);
        assert!(compiled.messages.is_empty());
        assert!(compiled.overlays.is_empty());
    }

    #[test]
    fn compile_song_lua_accepts_stage_stat_and_high_score_helpers() {
        let song_dir = test_dir("stage-stat-high-score-helpers");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local pss = STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1)
local accum = STATSMAN:GetAccumPlayedStageStats()
local final = STATSMAN:GetFinalEvalStageStats()
assert(STATSMAN:GetPlayedStageStats(2) == nil)
assert(STATSMAN:GetStagesPlayed() == 1)
assert(STATSMAN:GetFinalGrade(PLAYER_1) == "Grade_Tier07")
assert(STATSMAN:GetBestGrade() == "Grade_Tier07")
assert(STATSMAN:GetWorstGrade() == "Grade_Tier07")
assert(STATSMAN:GetBestFinalGrade() == "Grade_Tier07")
assert(accum:GetPlayerStageStats(PLAYER_1) ~= nil)
assert(final:GetPlayerStageStats(PLAYER_1) ~= nil)
assert(STATSMAN:GetCurStageStats():GetMultiPlayerStageStats(0) ~= nil)
assert(#STATSMAN:GetCurStageStats():GetPlayedSongs() == 1)
assert(#STATSMAN:GetCurStageStats():GetPossibleSongs() == 1)
assert(STATSMAN:GetCurStageStats():GetGameplaySeconds() >= 0)
assert(STATSMAN:GetCurStageStats():GetStepsSeconds() >= 0)
assert(STATSMAN:GetCurStageStats():GetStage() == "Stage_1st")
assert(STATSMAN:GetCurStageStats():GetStageIndex() == 0)
assert(STATSMAN:GetCurStageStats():OnePassed() == true)
assert(STATSMAN:GetCurStageStats():PlayerHasHighScore(PLAYER_1) == false)
assert(STATSMAN:GetCurStageStats():GetEarnedExtraStage() == false)
local steps = pss:GetPlayedSteps()[1]
assert(steps:GetMeter() >= 0)
assert(steps:GetDifficulty() ~= nil)
assert(#pss:GetPossibleSteps() == 1)
assert(#pss:GetComboList() == 0)
assert(pss:GetMachineHighScoreIndex() == -1)
assert(pss:GetPersonalHighScoreIndex() == -1)
assert(pss:IsDisqualified() == false)
assert(pss:FullComboOfScore(0) == false)
assert(pss:FullCombo() == false)
assert(pss:MaxCombo() == 0)
assert(pss:GetCurrentPossibleDancePoints() == 1)
assert(pss:GetCurrentCombo() == 0)
assert(pss:GetCurrentMissCombo() == 0)
assert(pss:GetCurrentScoreMultiplier() == 1)
assert(pss:GetCurMaxScore() == 0)
assert(pss:GetCaloriesBurned() == 0)
assert(pss:GetNumControllerSteps() == 0)
assert(pss:GetSurvivalSeconds() == 0)
assert(pss:GetAliveSeconds() == 0)
assert(pss:GetLessonScoreActual() == 0)
assert(pss:GetLessonScoreNeeded() == 0)
assert(pss:GetStageAward() == "StageAward_None")
assert(pss:GetPeakComboAward() == "PeakComboAward_None")
assert(pss:GetPercentageOfTaps("TapNoteScore_W1") == 0)
assert(pss:GetBestFullComboTapNoteScore() == "TapNoteScore_None")
assert(pss:GetSongsPassed() == 0)
assert(pss:GetSongsPlayed() == 0)

local highscore = pss:GetHighScore()
assert(highscore:GetHoldNoteScore("HoldNoteScore_Held") == 0)
assert(highscore:GetMaxCombo() == 0)
assert(highscore:GetSurvivalSeconds() == 0)
assert(highscore:GetStageAward() == "StageAward_None")
assert(highscore:GetPeakComboAward() == "PeakComboAward_None")
assert(highscore:IsFillInMarker() == false)
assert(highscore:GetRadarValues():GetValue("RadarCategory_TapsAndHolds") == 0)
local machine_list = PROFILEMAN:GetMachineProfile():GetHighScoreList(GAMESTATE:GetCurrentSong(), steps)
assert(machine_list:GetRankOfName("Machine") == 1)
assert(machine_list:GetRankOfName("Missing") == 0)
assert(machine_list:GetHighestScoreOfName("Machine"):GetName() == "Machine")
assert(machine_list:GetHighestScoreOfName("Missing") == nil)
assert(STATSMAN:GetCurStageStats():GaveUp() == false)
STATSMAN:Reset()

mod_actions = {
    {1, function()
        local stats = STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1)
        stats:SetScore(12)
        stats:SetCurMaxScore(24)
        stats:SetDancePointLimits(8, 10)
        stats:FailPlayer()
        assert(stats:GetFailed() == true)
        assert(stats:GetScore() == 12)
        assert(stats:GetCurMaxScore() == 24)
        assert(stats:GetActualDancePoints() == 8)
        assert(stats:GetPossibleDancePoints() == 10)
        assert(stats:GetPercentDancePoints() == 0.8)
    end, true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Stage Stat High Score Helpers"),
        )
        .unwrap();
        assert_eq!(compiled.info.unsupported_function_actions, 0);
        assert!(compiled.messages.is_empty());
        assert!(compiled.overlays.is_empty());
    }

    #[test]
    fn compile_song_lua_accepts_theme_pref_and_gamestate_control_helpers() {
        let song_dir = test_dir("theme-pref-gamestate-control-helpers");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
assert(PREFSMAN:PreferenceExists("EventMode"))
assert(PREFSMAN:GetPreference("EventMode") == false)
assert(PREFSMAN:GetPreference("MaxHighScoresPerListForMachine") == 10)
assert(PREFSMAN:GetPreference("LongVerSongSeconds") == 150)
assert(PREFSMAN:GetPreference("EditorNoteSkinP1") == "default")
assert(PREFSMAN:PreferenceExists("MissingPreference") == false)
assert(PREFSMAN:GetPreference("MissingPreference") == nil)
assert(PREFSMAN:SetPreference("EventMode", true) == PREFSMAN)
assert(PREFSMAN:GetPreference("EventMode") == true)
assert(PREFSMAN:SetPreferenceToDefault("EventMode") == PREFSMAN)
assert(PREFSMAN:GetPreference("EventMode") == false)
assert(PREFSMAN:SetPreference("ShowNativeLanguage", true) == PREFSMAN)
assert(PREFSMAN:GetPreference("ShowNativeLanguage") == true)
assert(PREFSMAN:SetPreference("MissingPreference", "ignored") == PREFSMAN)
assert(PREFSMAN:GetPreference("MissingPreference") == nil)
assert(ThemePrefs.Get("EditModeLastSeenSong") == "")

assert(THEME:GetCurLanguage() == "en")
assert(THEME:GetSelectableThemeNames()[1] == "Simply Love")
assert(GAMESTATE:GetPlayMode() == "PlayMode_Regular")
assert(GAMESTATE:GetCurrentStageIndex() == 0)
assert(GAMESTATE:GetCourseSongIndex() == 0)
assert(GAMESTATE:GetPlayerFailType(PLAYER_1) == "FailType_Immediate")

local style = GAMESTATE:GetCurrentStyle()
assert(style:ColumnsPerPlayer() == 4)
assert(style:GetStepsType() == "StepsType_Dance_Single")
assert(style:GetStyleType() == "StyleType_OnePlayerOneSide")
assert(style:GetWidth() == 256)
assert(style:GetColumnInfo(PLAYER_1, 1).Name == "Left")
assert(StepsType:Reverse()["StepsType_Dance_Single"] ~= nil)
assert(StyleType:Reverse()["StyleType_OnePlayerOneSide"] ~= nil)
assert(GAMEMAN:GetStylesForGame(GAMESTATE:GetCurrentGame():GetName())[1]:GetName() == "single")

local song = GAMESTATE:GetCurrentSong()
GAMESTATE:SetCurrentSong(song)
assert(GAMESTATE:GetCurrentSong() == song)
GAMESTATE:SetCurrentStyle("single")
assert(GAMESTATE:GetCurrentStyle():GetName() == "single")

mod_actions = {
    {1, function()
        assert(PREFSMAN:SetPreferenceToDefault("EventMode") == PREFSMAN)
        assert(PREFSMAN:SavePreferences() == PREFSMAN)
        THEME:ReloadMetrics()
        THEME:SetTheme("Simply Love")
        GAMESTATE:AddStageToPlayer(PLAYER_1)
        GAMESTATE:ResetPlayerOptions(PLAYER_1)
        GAMESTATE:SetPreferredDifficulty(PLAYER_1, "Difficulty_Hard")
        GAMESTATE:UnjoinPlayer(PLAYER_2)
        GAMESTATE:SetCurrentTrail(PLAYER_1, nil)
        GAMESTATE:SetCurrentSteps(PLAYER_1, GAMESTATE:GetCurrentSteps(PLAYER_1))
    end, true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Theme Pref GameState Control Helpers"),
        )
        .unwrap();
        assert_eq!(compiled.info.unsupported_function_actions, 0);
        assert!(compiled.messages.is_empty());
        assert!(compiled.overlays.is_empty());
    }

    #[test]
    fn compile_song_lua_exposes_theme_support_bpm_profile_helpers() {
        let song_dir = test_dir("theme-support-bpm-profile-helpers");
        fs::create_dir_all(song_dir.join("audio")).unwrap();
        fs::write(song_dir.join("audio/pass.ogg"), "").unwrap();
        fs::write(song_dir.join("audio/skip.wav"), "").unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local parts = GetVersionParts("1.2.0-git")
assert(parts[1] == 1 and parts[2] == 2 and parts[3] == 0)
local product = GetProductVersion()
assert(product[1] == 1 and product[2] == 2 and product[3] == 0)
assert(IsProductVersion(1, 2))
assert(IsMinimumProductVersion(1, 2, 0))
assert(IsITGmania())
assert(StepManiaVersionIsSupported())
assert(MinimumVersionString() == "1.2.0")
assert(CurrentGameIsSupported())
assert(GetThemeVersion() == ProductVersion())
assert(GetAuthor() ~= "")
assert(SupportsRenderToTexture())

local bpms = GetDisplayBPMs(PLAYER_1)
assert(bpms[1] == 120 and bpms[2] == 180)
assert(StringifyDisplayBPMs(PLAYER_1) == "120 - 180")

local song, steps = GetSongAndSteps(PLAYER_1)
assert(song == GAMESTATE:GetCurrentSong())
assert(steps == GAMESTATE:GetCurrentSteps(PLAYER_1))
assert(#getAuthorTable(steps) == 0)
assert(totalLengthSongOrCourse(PLAYER_1) == 123)
assert(currentTimeSongOrCourse(PLAYER_1) == 0)
assert(SecondsToHMMSS(3661) == "01:01:01")
assert(GetPlayerAvatarPath(PLAYER_1) == nil)
assert(GetAvatarPath("", "") == nil)

local files = findFiles("audio", "ogg")
assert(#files == 1 and files[1]:match("pass%.ogg$"))
assert(cleanGSub("a.b", ".", "-") == "a-b")
assert(force_to_range(1, 10, 5) == 5)
assert(wrapped_index(3, 2, 4) == 1)
assert(table.concat(table.rotate_left({1,2,3}, 1), ",") == "2,3,1")
assert(table.concat(table.rotate_right({1,2,3}, 1), ",") == "3,1,2")
assert(TableToString({1, 2}, "Demo"):match("^Demo = "))

mod_actions = {
    {1, function()
        LoadGuest(PLAYER_1)
        LoadProfileCustom(PROFILEMAN:GetProfile(PLAYER_1), PROFILEMAN:GetProfileDir(PLAYER_1))
        SaveProfileCustom(PROFILEMAN:GetProfile(PLAYER_1), PROFILEMAN:GetProfileDir(PLAYER_1))
        local parsed = ParseChartInfo(steps, "P1")
        assert(parsed.PeakNPS == 0)
        assert(#parsed.NotesPerMeasure == 0)
    end, true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let mut context =
            SongLuaCompileContext::new(&song_dir, "Theme Support BPM Profile Helpers");
        context.song_display_bpms = [120.0, 180.0];
        context.music_length_seconds = 123.0;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.info.unsupported_function_actions, 0);
        assert!(compiled.messages.is_empty());
        assert!(compiled.overlays.is_empty());
    }

    #[test]
    fn compile_song_lua_exposes_gameman_style_list() {
        let song_dir = test_dir("gameman-style-list");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local explicit = GAMEMAN:GetStylesForGame("dance")
local current = GAMEMAN:GetStylesForGame(GAMESTATE:GetCurrentGame():GetName())
assert(#explicit == 1 and #current == 1)
assert(explicit[1]:GetName() == "single")
assert(current[1]:GetName() == "single")
assert(explicit[1]:GetStepsType() == "StepsType_Dance_Single")
assert(explicit[1]:GetStyleType() == "StyleType_OnePlayerOneSide")
assert(explicit[1]:ColumnsPerPlayer() == 4)
assert(explicit[1]:GetWidth() == 256)

local col = explicit[1]:GetColumnInfo(PLAYER_1, 4)
mod_actions = {
    {
        1,
        string.format(
            "%s:%s:%s:%d:%.0f:%s:%d:%.0f",
            GAMESTATE:GetCurrentGame():GetName(),
            explicit[1]:GetName(),
            explicit[1]:GetStepsType(),
            explicit[1]:ColumnsPerPlayer(),
            explicit[1]:GetWidth(),
            col.Name,
            col.Track,
            col.XOffset
        ),
        true,
    },
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Gameman Style List"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "dance:single:StepsType_Dance_Single:4:256:Right:3:96"
        );
    }

    #[test]
    fn compile_song_lua_exposes_theme_menu_manager_helpers() {
        let song_dir = test_dir("theme-menu-manager-helpers");
        fs::write(song_dir.join("logo.png"), "").unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local top = SCREENMAN:GetTopScreen()
local wheel = top:GetMusicWheel()
assert(wheel:GetSelectedType() == "WheelItemDataType_Song")
assert(wheel:Move(1) == wheel)
assert(wheel:Move(-1) == wheel)
assert(MEMCARDMAN:GetCardState(PLAYER_1) == "MemoryCardState_none")
assert(MEMCARDMAN:GetName(PLAYER_1) == "")
assert(UNLOCKMAN:IsSongLocked(GAMESTATE:GetCurrentSong()) == 0)
assert(UNLOCKMAN:IsCourseLocked(GAMESTATE:GetCurrentCourse()) == 0)

local song = GAMESTATE:GetCurrentSong()
assert(song:GetFirstSecond() == 0)
assert(song:GetLastSecond() == 90)
assert(song:GetFirstBeat() == 0)
assert(song:GetLastBeat() == 180)
assert(song:GetOrTryAtLeastToGetSimfileAuthor() == "")

local resolved = ActorUtil.ResolvePath("logo.png", 1, true)
assert(ActorUtil.GetFileType(resolved) == "FileType_Bitmap")
assert(ActorUtil.GetFileType("sound.ogg") == "FileType_Sound")
assert(ActorUtil.GetFileType("clip.mp4") == "FileType_Movie")
assert(ActorUtil.GetFileType("notes.ssc") == "FileType_Text")
assert(ActorUtil.IsRegisteredClass("ActorFrame") == true)
assert(ActorUtil.IsRegisteredClass("MissingActorClass") == false)
local gamecommand = Var("GameCommand")
assert(gamecommand:GetIndex() == 0 and gamecommand:GetText() == "")
assert(gamecommand:GetName() == "" and gamecommand:GetScreen() == "")
assert(gamecommand:GetProfileID() == "")
assert(gamecommand:GetAnnouncer() == "")
assert(gamecommand:GetPreferredModifiers() == "")
assert(gamecommand:GetStageModifiers() == "")
assert(gamecommand:GetMultiPlayer() == -1)
assert(gamecommand:GetStyle():GetName() == GAMESTATE:GetCurrentStyle():GetName())
assert(gamecommand:GetSong() == GAMESTATE:GetCurrentSong())
assert(gamecommand:GetSteps() == GAMESTATE:GetCurrentSteps(PLAYER_1))
assert(gamecommand:GetCourse() == GAMESTATE:GetCurrentCourse())
assert(gamecommand:GetTrail() == GAMESTATE:GetCurrentTrail(PLAYER_1))
assert(gamecommand:GetCharacter() == nil)
assert(gamecommand:GetSongGroup() == song:GetGroupName())
assert(gamecommand:GetUrl() == nil)
assert(gamecommand:GetDifficulty() == "Difficulty_Invalid")
assert(gamecommand:GetCourseDifficulty() == "Difficulty_Invalid")
assert(gamecommand:GetPlayMode() == "PlayMode_Invalid")
assert(gamecommand:GetSortOrder() == "SortOrder_Invalid")
assert(Var("LoadingScreen") == "LoadingScreen")

local ps = GAMESTATE:GetPlayerState(PLAYER_1)
ps:SetPlayerOptions("ModsLevel_Preferred", "1x, Overhead, 50% Mini")
local options = ps:GetPlayerOptionsArray("ModsLevel_Preferred")
assert(#options == 3 and options[2] == "Overhead")
assert(GetPlayerOptionsString(PLAYER_1) == "1x, Overhead, 50% Mini")

mod_actions = {
    {1, function()
        top:Continue()
        top:GetOptionRow(1):GetChoiceInRowWithFocus(PLAYER_1)
        local metric_actor = Def.Actor{Name="MetricActor"}
        assert(ActorUtil.LoadAllCommands(metric_actor, "ScreenSystemLayer") == nil)
        assert(ActorUtil.LoadAllCommandsFromName(metric_actor, "ScreenSystemLayer", "Actor") == nil)
        assert(ActorUtil.LoadAllCommandsAndSetXY(metric_actor, Var("LoadingScreen")) == nil)
        assert(MEMCARDMAN:MountCard(PLAYER_1) == false)
        assert(MEMCARDMAN:UnmountCard(PLAYER_1) == false)
        wheel:Move(0)
    end, true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Theme Menu Manager Helpers");
        context.song_display_bpms = [120.0, 120.0];
        context.music_length_seconds = 90.0;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.info.unsupported_function_actions, 0);
        assert!(compiled.messages.is_empty());
        assert!(compiled.overlays.is_empty());
    }

    #[test]
    fn compile_song_lua_exposes_empty_unlockman_shape() {
        let song_dir = test_dir("empty-unlockman-shape");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local songs = UNLOCKMAN:GetSongsUnlockedByEntryID("missing")
local step_songs, step_difficulties = UNLOCKMAN:GetStepsUnlockedByEntryID("missing")
assert(UNLOCKMAN:GetNumUnlocks() == 0)
assert(UNLOCKMAN:GetNumUnlocked() == 0)
assert(UNLOCKMAN:GetPoints("UnlockRequirement_ArcadePoints") == 0)
assert(UNLOCKMAN:GetPointsForProfile(PROFILEMAN:GetMachineProfile(), "UnlockRequirement_SongPoints") == 0)
assert(UNLOCKMAN:GetPointsUntilNextUnlock("UnlockRequirement_DancePoints") == 0)
assert(UNLOCKMAN:AnyUnlocksToCelebrate() == false)
assert(UNLOCKMAN:GetUnlockEntryIndexToCelebrate() == -1)
assert(UNLOCKMAN:FindEntryID("missing") == nil)
assert(UNLOCKMAN:GetUnlockEntry(0) == nil)
assert(#songs == 0 and #step_songs == 0 and #step_difficulties == 0)
assert(UNLOCKMAN:IsSongLocked(GAMESTATE:GetCurrentSong()) == 0)
assert(UNLOCKMAN:IsCourseLocked(GAMESTATE:GetCurrentCourse()) == 0)
assert(UNLOCKMAN:IsStepsLocked(GAMESTATE:GetCurrentSong(), GAMESTATE:GetCurrentSteps(PLAYER_1)) == 0)

mod_actions = {
    {1, function()
        assert(UNLOCKMAN:PreferUnlockEntryID("missing") == UNLOCKMAN)
        assert(UNLOCKMAN:UnlockEntryID("missing") == UNLOCKMAN)
        assert(UNLOCKMAN:UnlockEntryIndex(0) == UNLOCKMAN)
        assert(UNLOCKMAN:LockEntryID("missing") == UNLOCKMAN)
        assert(UNLOCKMAN:LockEntryIndex(0) == UNLOCKMAN)
    end, true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Empty Unlockman Shape"),
        )
        .unwrap();
        assert_eq!(compiled.info.unsupported_function_actions, 0);
        assert!(compiled.messages.is_empty());
        assert!(compiled.overlays.is_empty());
    }

    #[test]
    fn compile_song_lua_exposes_song_time_to_function_actions() {
        let song_dir = test_dir("function-action-song-time");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
mod_actions = {
    {4, function()
        local p = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        local beat = GAMESTATE:GetSongBeat()
        local seconds = GAMESTATE:GetCurMusicSeconds()
        local pos = GAMESTATE:GetSongPosition():GetSongBeat()
        if p then
            p:x(beat)
            p:y(seconds * 100)
            p:rotationz(pos)
        end
    end, true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Function Action Song Time");
        context.song_display_bpms = [120.0, 120.0];
        context.song_music_rate = 2.0;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.info.unsupported_function_actions, 0);
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.player_actors[0].message_commands.len(), 1);
        let block = &compiled.player_actors[0].message_commands[0].blocks[0];
        assert_eq!(block.delta.x, Some(4.0));
        assert_eq!(block.delta.y, Some(100.0));
        assert_eq!(block.delta.rot_z_deg, Some(4.0));
    }

    #[test]
    fn compile_song_lua_extracts_actorproxy_targets() {
        let song_dir = test_dir("overlay-proxy");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local proxy = nil

mod_actions = {
    {8, function()
        if proxy then
            proxy:visible(true)
        end
    end, true},
}

return Def.ActorFrame{
    Def.ActorProxy{
        Name="p1_proxy",
        OnCommand=function(self)
            proxy = self
            self:queuecommand("Bind")
        end,
        BindCommand=function(self)
            local p = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
            local nf = p and p:GetChild("NoteField") or nil
            if nf and nf:GetNumWrapperStates() == 0 then
                nf:AddWrapperState()
            end
            local wrapper = nf and nf:GetWrapperState(1) or nil
            if wrapper then
                self:SetTarget(wrapper)
            end
            self:visible(false)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Overlay Proxy"),
        )
        .unwrap();
        assert_eq!(compiled.info.unsupported_function_actions, 0);
        assert_eq!(compiled.overlays.len(), 1);
        assert!(matches!(
            compiled.overlays[0].kind,
            SongLuaOverlayKind::ActorProxy {
                target: SongLuaProxyTarget::NoteField { player_index: 0 }
            }
        ));
        assert!(!compiled.overlays[0].initial_state.visible);
        assert_eq!(compiled.overlays[0].message_commands.len(), 1);
        assert_eq!(
            compiled.overlays[0].message_commands[0].blocks[0]
                .delta
                .visible,
            Some(true)
        );
    }

    #[test]
    fn compile_song_lua_extracts_player_judgment_and_combo_proxy_targets() {
        let song_dir = test_dir("player-judgment-combo-proxy");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.ActorProxy{
        Name="judgment_proxy",
        OnCommand=function(self)
            local player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
            local target = player:GetChild("Judgment")
            self:SetTarget(target)
            target:visible(false)
        end,
    },
    Def.ActorProxy{
        Name="combo_proxy",
        OnCommand=function(self)
            local player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
            local target = player:GetChild("Combo")
            if target:GetNumWrapperStates() == 0 then
                target:AddWrapperState()
            end
            target:GetWrapperState(1):addy(9999)
            self:SetTarget(target)
            target:visible(false)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Judgment Combo Proxy"),
        )
        .unwrap();
        assert!(compiled.overlays.iter().any(|overlay| {
            matches!(
                overlay.kind,
                SongLuaOverlayKind::ActorProxy {
                    target: SongLuaProxyTarget::Judgment { player_index: 0 }
                }
            )
        }));
        assert!(compiled.overlays.iter().any(|overlay| {
            matches!(
                overlay.kind,
                SongLuaOverlayKind::ActorProxy {
                    target: SongLuaProxyTarget::Combo { player_index: 0 }
                }
            )
        }));
    }

    #[test]
    fn compile_song_lua_runs_cmd_queuecommand_builders() {
        let song_dir = test_dir("overlay-proxy-cmd");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.ActorProxy{
        Name="p1_proxy",
        OnCommand=cmd(queuecommand, "Bind"),
        BindCommand=function(self)
            local p = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
            if p then
                self:SetTarget(p)
            end
            self:visible(false)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Overlay Proxy Cmd"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 1);
        assert!(matches!(
            compiled.overlays[0].kind,
            SongLuaOverlayKind::ActorProxy {
                target: SongLuaProxyTarget::Player { player_index: 0 }
            }
        ));
        assert!(!compiled.overlays[0].initial_state.visible);
    }

    #[test]
    fn compile_song_lua_runs_legacy_cmd_keyword() {
        let song_dir = test_dir("legacy-cmd-keyword");
        let entry = song_dir.join("default.lua");
        fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{
        Name="LegacyCmd",
        OnCommand=cmd(x,SCREEN_CENTER_X;y,SCREEN_CENTER_Y;diffusealpha,0;scaletocover,0,0,SCREEN_WIDTH,SCREEN_HEIGHT),
    },
}
"#,
    )
    .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Legacy Cmd Keyword"),
        )
        .unwrap();
        let actor = compiled
            .overlays
            .iter()
            .find(|actor| actor.name.as_deref() == Some("LegacyCmd"))
            .unwrap();
        assert_eq!(actor.initial_state.x, 320.0);
        assert_eq!(actor.initial_state.y, 240.0);
        assert_eq!(actor.initial_state.diffuse[3], 0.0);
    }

    #[test]
    fn compile_song_lua_extracts_actorframetexture_capture_sprite_and_hidden_player() {
        let song_dir = test_dir("overlay-aft");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local capture = nil

return Def.ActorFrame{
    OnCommand=function(self)
        local p = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        if p then
            p:visible(false)
        end
    end,
    Def.ActorFrameTexture{
        Name="CaptureAFT",
        InitCommand=function(self)
            capture = self
        end,
        Def.ActorProxy{
            Name="ProxyP1",
            OnCommand=function(self)
                local p = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
                if p then
                    local nf = p:GetChild("NoteField")
                    if nf and nf:GetNumWrapperStates() == 0 then
                        nf:AddWrapperState()
                    end
                    self:SetTarget(nf and nf:GetWrapperState(1) or nf)
                end
                self:visible(true)
            end,
        },
    },
    Def.Sprite{
        Name="AFTSpriteR",
        OnCommand=function(self)
            if capture then
                self:SetTexture(capture:GetTexture())
            end
            self:diffuse(1, 0, 0, 1)
            self:blend("add")
            self:vibrate()
            self:effectmagnitude(8, 4, 0)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Overlay AFT"),
        )
        .unwrap();
        assert!(compiled.hidden_players[0]);
        assert_eq!(compiled.overlays.len(), 3);
        assert!(matches!(
            compiled.overlays[0].kind,
            SongLuaOverlayKind::ActorFrameTexture
        ));
        assert!(matches!(
            compiled.overlays[1].kind,
            SongLuaOverlayKind::ActorProxy {
                target: SongLuaProxyTarget::NoteField { player_index: 0 }
            }
        ));
        assert!(matches!(
            compiled.overlays[2].kind,
            SongLuaOverlayKind::AftSprite { ref capture_name }
                if capture_name == "CaptureAFT"
        ));
        assert_eq!(
            compiled.overlays[2].initial_state.blend,
            SongLuaOverlayBlendMode::Add
        );
        assert!(compiled.overlays[2].initial_state.vibrate);
        assert_eq!(
            compiled.overlays[2].initial_state.effect_magnitude,
            [8.0, 4.0, 0.0]
        );
    }

    #[test]
    fn compile_song_lua_supports_named_actorframetexture_sprites() {
        let song_dir = test_dir("overlay-aft-texture-name");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.ActorFrameTexture{
        Name="CaptureAFT",
        InitCommand=function(self)
            self:SetTextureName("ScreenTex")
            self:SetWidth(640)
            self:SetHeight(480)
            self:EnableAlphaBuffer(false)
            self:Create()
        end,
    },
    Def.Sprite{
        Texture="ScreenTex",
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Named AFT Sprite"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 2);
        assert!(matches!(
            compiled.overlays[0].kind,
            SongLuaOverlayKind::ActorFrameTexture
        ));
        assert!(matches!(
            compiled.overlays[1].kind,
            SongLuaOverlayKind::AftSprite { ref capture_name }
                if capture_name == "ScreenTex"
        ));
    }

    #[test]
    fn compile_song_lua_accepts_actorframetexture_draw_call() {
        let song_dir = test_dir("overlay-aft-draw");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.ActorFrameTexture{
        Name="ScreenTex",
        InitCommand=function(self)
            self:Create()
        end,
        OnCommand=function(self)
            self:visible(true)
            self:Draw()
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled =
            test_compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "AFT Draw"))
                .unwrap();
        assert_eq!(compiled.overlays.len(), 1);
        assert!(matches!(
            compiled.overlays[0].kind,
            SongLuaOverlayKind::ActorFrameTexture
        ));
        assert!(compiled.overlays[0].initial_state.visible);
    }

    #[test]
    fn compile_song_lua_extracts_overlay_function_actions_and_eases() {
        let song_dir = test_dir("overlay-functions");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local target = nil

mod_actions = {
    {8, function()
        if target then
            target:visible(true)
            target:diffusealpha(1)
        end
    end, true},
}

mods_ease = {
    {4, 2, 0, 320, function(a)
        if target then
            target:x(a)
            target:zoomx(1 + (a / 320))
            target:cropbottom(a / 640)
        end
    end, "len", ease.outQuad},
}

return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            target = self
            self:visible(false)
            self:diffusealpha(0)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Overlay Functions"),
        )
        .unwrap();
        assert_eq!(compiled.info.unsupported_function_actions, 0);
        assert_eq!(compiled.info.unsupported_function_eases, 0);
        assert_eq!(compiled.messages.len(), 1);
        assert!(
            compiled.messages[0]
                .message
                .starts_with("__songlua_overlay_fn_action_")
        );
        assert_eq!(compiled.overlays.len(), 1);
        assert_eq!(compiled.overlays[0].message_commands.len(), 1);
        assert_eq!(compiled.overlays[0].message_commands[0].blocks.len(), 1);
        assert_eq!(
            compiled.overlays[0].message_commands[0].blocks[0]
                .delta
                .visible,
            Some(true)
        );
        assert_eq!(
            compiled.overlays[0].message_commands[0].blocks[0]
                .delta
                .diffuse
                .unwrap()[3],
            1.0
        );
        assert_eq!(compiled.overlay_eases.len(), 1);
        let ease = &compiled.overlay_eases[0];
        assert_eq!(ease.overlay_index, 0);
        assert_eq!(ease.easing.as_deref(), Some("outQuad"));
        assert_eq!(ease.from.x, Some(0.0));
        assert_eq!(ease.to.x, Some(320.0));
        assert_eq!(ease.from.zoom_x, Some(1.0));
        assert_eq!(ease.to.zoom_x, Some(2.0));
        assert_eq!(ease.from.cropbottom, Some(0.0));
        assert_eq!(ease.to.cropbottom, Some(0.5));
    }

    #[test]
    fn compile_song_lua_keeps_overlay_rotation_eases_out_of_player_transforms() {
        let song_dir = test_dir("overlay-rotation-ease");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local target = nil

mods_ease = {
    {4, 2, 0, 45, function(a)
        if target then
            target:rotationz(a)
        end
    end, "len", ease.outQuad},
}

return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            target = self
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Overlay Rotation Ease"),
        )
        .unwrap();
        assert_eq!(compiled.info.unsupported_function_eases, 0);
        assert!(compiled.eases.is_empty());
        assert_eq!(compiled.overlay_eases.len(), 1);
        assert_eq!(compiled.overlay_eases[0].overlay_index, 0);
        assert_eq!(compiled.overlay_eases[0].from.rot_z_deg, Some(0.0));
        assert_eq!(compiled.overlay_eases[0].to.rot_z_deg, Some(45.0));
    }

    #[test]
    fn compile_song_lua_extracts_xero_aux_overlay_node_eases() {
        let song_dir = test_dir("xero-aux-overlay-node");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
xero = {}
local eases = {}
local nodes = {}
local target = nil
local wagger = nil

local function schedule_ease(self)
    table.insert(eases, self)
end

local function node(self)
    table.insert(nodes, {{self[1]}, {}, self[2]})
end

local function definemod(self)
    node(self)
end

local function export(fn, name)
    local function inner(self)
        fn(self)
        return inner
    end
    xero[name] = inner
end

export(schedule_ease, 'ease')
export(definemod, 'definemod')

local function update(self)
    if eases[0] then self:x(0) end
end

return Def.ActorFrame{
    Def.Quad{
        Name='Flash',
        OnCommand=function(self)
            target = self
            self:diffuse(1,1,1,1):diffusealpha(0)
            xero.definemod {'flashalpha', function(a) target:diffusealpha(a) end}
            xero.ease {4, 1, ease.outQuad, 1, 'flashalpha'}
        end,
    },
    Def.Quad{
        Name='Wagger',
        OnCommand=function(self)
            wagger = self
            xero.definemod {'wagy', function(a)
                wagger:wag():effectmagnitude(0,a,0):effectperiod(1):effectclock('bgm')
            end}
            xero.ease {8, 1, ease.linear, 20, 'wagy'}
        end,
    },
    Def.Quad{
        InitCommand=function(self)
            self:SetUpdateFunction(update)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Xero Aux Overlay Node"),
        )
        .unwrap();
        let flash_index = compiled
            .overlays
            .iter()
            .position(|overlay| overlay.name.as_deref() == Some("Flash"))
            .expect("test flash actor should compile");
        let wagger_index = compiled
            .overlays
            .iter()
            .position(|overlay| overlay.name.as_deref() == Some("Wagger"))
            .expect("test wag actor should compile");
        assert!(compiled.eases.iter().all(
            |ease| !matches!(ease.target, SongLuaEaseTarget::Mod(ref name) if name == "flashalpha")
        ));
        assert!(compiled.overlay_eases.iter().any(|ease| {
            ease.overlay_index == flash_index
                && (ease.start - 4.0).abs() <= 0.001
                && (ease.limit - 1.0).abs() <= 0.001
                && ease.easing.as_deref() == Some("outQuad")
                && ease
                    .from
                    .diffuse
                    .is_some_and(|color| (color[3] - 0.0).abs() <= 0.001)
                && ease
                    .to
                    .diffuse
                    .is_some_and(|color| (color[3] - 1.0).abs() <= 0.001)
        }));
        assert!(compiled.overlay_eases.iter().any(|ease| {
            ease.overlay_index == wagger_index
                && (ease.start - 8.0).abs() <= 0.001
                && (ease.limit - 1.0).abs() <= 0.001
                && ease.easing.as_deref() == Some("linear")
                && ease.to.effect_mode == Some(EffectMode::Wag)
                && ease
                    .to
                    .effect_magnitude
                    .is_some_and(|value| value == [0.0, 20.0, 0.0])
                && ease.to.effect_clock == Some(EffectClock::Beat)
                && ease.to.effect_period.is_some_and(|value| value == 1.0)
        }));
    }

    #[test]
    fn compile_song_lua_reads_table_color_calls_for_overlays() {
        let song_dir = test_dir("overlay-table-colors");
        let entry = song_dir.join("default.lua");
        let overlay_dir = song_dir.join("gfx");
        fs::create_dir_all(&overlay_dir).unwrap();
        fs::write(
            overlay_dir.join("grid.png"),
            b"not-an-image-but-good-enough-for-parser",
        )
        .unwrap();
        fs::write(
            &entry,
            r#"
local function rgb(r, g, b, a)
    return {r / 255, g / 255, b / 255, a or 1}
end

return Def.ActorFrame{
    Def.Sprite{
        Texture="gfx/grid.png",
        OnCommand=function(self)
            self:diffuse(rgb(30, 30, 35, 0.5))
            self:diffuseshift()
            self:effectcolor1(rgb(30, 30, 35, 1))
            self:effectcolor2(rgb(70, 70, 70, 1))
            self:effectperiod(5)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Overlay Table Colors"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 1);
        let state = compiled.overlays[0].initial_state;
        assert_eq!(
            state.diffuse,
            [30.0 / 255.0, 30.0 / 255.0, 35.0 / 255.0, 0.5]
        );
        assert_eq!(state.effect_mode, EffectMode::DiffuseShift);
        assert_eq!(
            state.effect_color1,
            [30.0 / 255.0, 30.0 / 255.0, 35.0 / 255.0, 1.0]
        );
        assert_eq!(
            state.effect_color2,
            [70.0 / 255.0, 70.0 / 255.0, 70.0 / 255.0, 1.0]
        );
        assert_eq!(state.effect_period, 5.0);
    }

    #[test]
    fn compile_song_lua_captures_effect_defaults_and_clocks_for_overlays() {
        let song_dir = test_dir("overlay-effect-defaults");
        let entry = song_dir.join("default.lua");
        let overlay_dir = song_dir.join("gfx");
        fs::create_dir_all(&overlay_dir).unwrap();
        fs::write(
            overlay_dir.join("grid.png"),
            b"not-an-image-but-good-enough-for-parser",
        )
        .unwrap();
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="gfx/grid.png",
        OnCommand=function(self)
            self:diffuseramp()
            self:effectclock("beat")
            self:effectoffset(0.25)
        end,
    },
    Def.Quad{
        OnCommand=function(self)
            self:bounce()
        end,
    },
    Def.Quad{
        OnCommand=function(self)
            self:bob()
        end,
    },
    Def.Quad{
        OnCommand=function(self)
            self:pulse()
        end,
    },
    Def.Quad{
        OnCommand=function(self)
            self:wag()
        end,
    },
    Def.Quad{
        OnCommand=function(self)
            self:spin()
        end,
    },
    Def.Quad{
        OnCommand=function(self)
            self:vibrate()
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Overlay Effect Defaults"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 7);

        let ramp = compiled.overlays[0].initial_state;
        assert_eq!(ramp.effect_mode, EffectMode::DiffuseRamp);
        assert_eq!(ramp.effect_clock, EffectClock::Beat);
        assert_eq!(ramp.effect_color1, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(ramp.effect_color2, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(ramp.effect_offset, 0.25);

        let bounce = compiled.overlays[1].initial_state;
        assert_eq!(bounce.effect_mode, EffectMode::Bounce);
        assert_eq!(bounce.effect_period, 2.0);
        assert_eq!(bounce.effect_magnitude, [0.0, 20.0, 0.0]);

        let bob = compiled.overlays[2].initial_state;
        assert_eq!(bob.effect_mode, EffectMode::Bob);
        assert_eq!(bob.effect_period, 2.0);
        assert_eq!(bob.effect_magnitude, [0.0, 20.0, 0.0]);

        let pulse = compiled.overlays[3].initial_state;
        assert_eq!(pulse.effect_mode, EffectMode::Pulse);
        assert_eq!(pulse.effect_period, 2.0);
        assert_eq!(pulse.effect_magnitude, [0.5, 1.0, 1.0]);

        let wag = compiled.overlays[4].initial_state;
        assert_eq!(wag.effect_mode, EffectMode::Wag);
        assert_eq!(wag.effect_period, 2.0);
        assert_eq!(wag.effect_magnitude, [0.0, 0.0, 20.0]);

        let spin = compiled.overlays[5].initial_state;
        assert_eq!(spin.effect_mode, EffectMode::Spin);
        assert_eq!(spin.effect_magnitude, [0.0, 0.0, 180.0]);

        let vibrate = compiled.overlays[6].initial_state;
        assert!(vibrate.vibrate);
        assert_eq!(vibrate.effect_magnitude, [10.0, 10.0, 10.0]);
    }

    #[test]
    fn compile_song_lua_supports_overlay_effect_timing() {
        let song_dir = test_dir("overlay-effect-timing");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:bob()
            self:effecttiming(0.25, 0.5, 0.75, 1.25)
        end,
    },
    Def.Quad{
        OnCommand=function(self)
            self:bounce()
            self:effecttiming(0.25, 0.5, 0.75, 1.25, 1.5)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Overlay Effect Timing"),
        )
        .unwrap();
        assert_eq!(compiled.overlays.len(), 2);

        let bob = compiled.overlays[0].initial_state;
        assert_eq!(bob.effect_mode, EffectMode::Bob);
        assert_eq!(bob.effect_period, 2.75);
        assert_eq!(bob.effect_timing, Some([0.25, 0.5, 0.75, 0.0, 1.25]));

        let bounce = compiled.overlays[1].initial_state;
        assert_eq!(bounce.effect_mode, EffectMode::Bounce);
        assert_eq!(bounce.effect_period, 4.25);
        assert_eq!(bounce.effect_timing, Some([0.25, 0.5, 0.75, 1.5, 1.25]));
    }

    #[test]
    fn compile_song_lua_captures_actorframe_perspective_state() {
        let song_dir = test_dir("overlay-perspective");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.ActorFrame{
        Name="PerspectiveRoot",
        OnCommand=function(self)
            self:fov(120)
            self:vanishpoint(400, 120)
        end,
        Def.Quad{},
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Overlay Perspective"),
        )
        .unwrap();
        let perspective = compiled
            .overlays
            .iter()
            .find(|overlay| overlay.name.as_deref() == Some("PerspectiveRoot"))
            .expect("expected actorframe overlay with perspective state");
        assert!(matches!(perspective.kind, SongLuaOverlayKind::ActorFrame));
        assert_eq!(perspective.initial_state.fov, Some(120.0));
        assert_eq!(perspective.initial_state.vanishpoint, Some([400.0, 120.0]));
    }

    #[test]
    fn compile_song_lua_preserves_overlay_color_for_diffusealpha_eases() {
        let song_dir = test_dir("overlay-diffusealpha-color");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local target = nil

mods_ease = {
    {4, 2, 0, 1, function(a)
        if target then
            target:diffusealpha(a)
        end
    end, "len", ease.outQuad},
}

return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            target = self
            self:diffuse(0, 0, 0, 0)
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Overlay Diffusealpha Color"),
        )
        .unwrap();
        assert_eq!(compiled.info.unsupported_function_eases, 0);
        assert_eq!(compiled.overlays.len(), 1);
        assert_eq!(
            compiled.overlays[0].initial_state.diffuse,
            [0.0, 0.0, 0.0, 0.0]
        );
        assert_eq!(compiled.overlay_eases.len(), 1);
        assert_eq!(
            compiled.overlay_eases[0].from.diffuse,
            Some([0.0, 0.0, 0.0, 0.0])
        );
        assert_eq!(
            compiled.overlay_eases[0].to.diffuse,
            Some([0.0, 0.0, 0.0, 1.0])
        );
    }

    #[test]
    fn compile_song_lua_exposes_theme_branch_and_path_helpers() {
        let song_dir = test_dir("theme-branch-helpers");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local path = "/Songs/Group/Song/"
local parts = split("/", path)
local method_parts = ("Group/Song"):split("/")
local player_af = GetPlayerAF(ToEnumShortString(PLAYER_1))

mod_actions = {
    {1, table.concat({
        parts[1],
        parts[3],
        parts[4],
        method_parts[2],
        Basename(path),
        ProfileSlot[PlayerNumber:Reverse()[PLAYER_1] + 1],
        GameController:Reverse()["GameController_2"],
        GetDefaultFailType(),
        GetComboThreshold("Maintain"),
        tostring(IsAutoplay(PLAYER_1)),
        tostring(IsW0Judgment({Player=PLAYER_1}, PLAYER_1)),
        tostring(IsW010Judgment({Player=PLAYER_1}, PLAYER_1)),
        string.format("%.0f", GetNotefieldWidth()),
        string.format("%.0f", GetNotefieldX(PLAYER_1)),
        tostring(player_af ~= nil),
        Branch.AfterSelectMusic(),
        Branch.GameplayScreen(),
        SelectMusicOrCourse(),
    }, "|"), true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Theme Helpers");
        context.players[0].screen_x = 123.0;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "|Group|Song|Song|Song|ProfileSlot_Player1|1|FailType_Immediate|TapNoteScore_W3|false|false|false|256|123|true|ScreenGameplay|ScreenGameplay|ScreenSelectMusic"
        );
    }

    #[test]
    fn compile_song_lua_exposes_theme_utility_and_sort_helpers() {
        let song_dir = test_dir("theme-utility-helpers");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local values = range(1, 5, 2)
local doubled = map(function(value) return value * 2 end, values)
local labels = stringify(doubled, "%g")
local unique = deduplicate({"a", "a", "b"})
local wheel = SCREENMAN:GetTopScreen():GetMusicWheel()
local before = GAMESTATE:GetSortOrder()
wheel:ChangeSort("SortOrder_Preferred")
local after = GAMESTATE:GetSortOrder()

mod_actions = {
    {1, table.concat({
        tostring(#values),
        labels[2],
        tostring(#unique),
        ToEnumShortString(before),
        ToEnumShortString(after),
        tostring(SortOrder:Reverse()[after] ~= nil),
        tostring(TapNoteScore:Reverse()["TapNoteScore_W3"] ~= nil),
        tostring(HoldNoteScore:Reverse()["HoldNoteScore_Held"] ~= nil),
        THEME:GetString("TapNoteScore", "W1"),
        THEME:GetString("ScreenEvaluation", "Hands"),
    }, "|"), true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Theme Utilities"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "3|6|2|Group|Preferred|true|true|true|W1|Hands"
        );
        assert_eq!(compiled.info.unsupported_function_actions, 0);
    }

    #[test]
    fn compile_song_lua_exposes_theme_asset_option_helpers() {
        let song_dir = test_dir("theme-asset-option-helpers");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local filters = BackgroundFilterValues()
local columns = GetColumnMapping(PLAYER_1)
local credits = GetCredits()
local fallback = GetFallbackBanner()

mod_actions = {
    {1, table.concat({
        tostring(filters.Dark),
        tostring(NumJudgmentsAvailable()),
        tostring(DetermineTimingWindow(0.03)),
        tostring(credits.Credits),
        tostring(credits.CoinsPerCredit),
        StripSpriteHints("Love 2x6 (doubleres).png"),
        GetJudgmentGraphics()[1],
        GetHoldJudgments()[1],
        GetHeldMissGraphics()[1],
        GetComboFonts()[1],
        tostring(#columns),
        tostring(columns[4]),
        tostring(#GetStepsCredit(PLAYER_1)),
        tostring(IsSpooky()),
        tostring(IsGameAndMenuButton("Left")),
        GetPlayerOptionsString(PLAYER_1),
        tostring(TotalCourseLength(PLAYER_1)),
        tostring(TotalCourseLengthPlayed(PLAYER_1)),
        fallback:sub(1, 21),
    }, "|"), true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Theme Asset Helpers"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "50|5|2|0|1|Love|None|None|None|None|4|4|0|false|false||0|0|__songlua_theme_path"
        );
        assert_eq!(compiled.info.unsupported_function_actions, 0);
    }

    #[test]
    fn compile_song_lua_exposes_theme_option_row_helpers() {
        let song_dir = test_dir("theme-option-row-helpers");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local speed = CustomOptionRow("SpeedModType")
assert(speed.Name == "SpeedModType")
assert(speed.LayoutType == "ShowOneInRow")
assert(speed.SelectType == "SelectOne")
assert(speed.HideOnDisable == false)
assert(speed.Choices[1] == "X")
assert(speed.Values == nil)
assert(speed.EnabledForPlayers()[1] == PLAYER_1)
assert(speed.ReloadRowMessages[1] == nil)

local list = speed:LoadSelections({})
assert(list[1] == true)
speed:SaveSelections({false, true, false}, PLAYER_2)
assert(SL.P2.ActiveModifiers.SpeedModType == "C")

local bg = CustomOptionRow("BackgroundFilter")
assert(bg.Values[1] == "Off")
local hide = CustomOptionRow("Hide")
assert(hide.SelectType == "SelectMultiple")
hide:SaveSelections({true, false, true}, PLAYER_1)
assert(SL.P1.ActiveModifiers.HideTargets == true)
assert(SL.P1.ActiveModifiers.HideSongBG == false)
assert(SL.P1.ActiveModifiers.HideCombo == true)
local variant = CustomOptionRow("NoteSkinVariant")
assert(variant.HideOnDisable == true)
assert(variant.ReloadRowMessages[1] == "RefreshActorProxy")
assert(CustomOptionRow("MissingRow") == false)

local pref = ThemePrefsRows.GetRow("AllowThemeVideos")
assert(pref.Name == "AllowThemeVideos")
assert(pref.Values[1] == true)
assert(pref:LoadSelections({})[1] == true)

local visual = ThemePrefsRows.GetRow("VisualStyle")
assert(visual.Choices[1] == "Hearts")
visual:SaveSelections({false, true}, PLAYER_1)
assert(ThemePrefs.Get("VisualStyle") == "Arrows")
ThemePrefsRows.GetRow("RainbowMode"):SaveSelections({true, false}, PLAYER_1)
assert(ThemePrefs.Get("RainbowMode") == true)
ThemePrefs.InitAll({})
ThemePrefsRows.InitAll({})

local op = OperatorMenuOptionRows.Theme()
assert(op.Name == "Theme")
assert(op.Choices[1] == THEME:GetCurThemeName())
local marathon = OperatorMenuOptionRows.LongAndMarathonTime("Marathon")
assert(marathon.Name == "Marathon Time")
assert(marathon.Values[2] == 450)
marathon:SaveSelections({false, true}, PLAYER_1)
assert(PREFSMAN:GetPreference("MarathonVerSongSeconds") == 450)
local wheel = OperatorMenuOptionRows.MusicWheelSpeed()
assert(wheel.Values[3] == 15)
wheel:SaveSelections({false, false, true}, PLAYER_1)
assert(PREFSMAN:GetPreference("MusicWheelSwitchSpeed") == 15)
local offset = OperatorMenuOptionRows.GlobalOffsetSeconds()
assert(offset.Values[4] == 0.5)
offset:SaveSelections({false, false, false, true}, PLAYER_1)
assert(PREFSMAN:GetPreference("GlobalOffsetSeconds") == 0.5)
local memory = OperatorMenuOptionRows.MemoryCards()
assert(memory.Values[1] == false and memory.Values[2] == true)
memory:SaveSelections({false, true}, PLAYER_1)
assert(PREFSMAN:GetPreference("MemoryCards") == true)
local fallback = OperatorMenuOptionRows.UnknownThing()
assert(fallback.Name == "UnknownThing")

mod_actions = {
    {1, function()
        CustomOptionRow("Mini"):SaveSelections({true, false}, PLAYER_1)
        assert(SL.P1.ActiveModifiers.Mini == "-100%")
        ThemePrefsRows.GetRow("VisualStyle"):SaveSelections({true, false}, PLAYER_1)
        assert(ThemePrefs.Get("VisualStyle") == "Hearts")
        OperatorMenuOptionRows.CustomSongsLoadTimeout():SaveSelections({true}, PLAYER_1)
        assert(PREFSMAN:GetPreference("CustomSongsLoadTimeout") == 3)
        OperatorMenuOptionRows.UnknownThing():SaveSelections({true}, PLAYER_1)
    end, true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Theme Option Row Helpers"),
        )
        .unwrap();
        assert_eq!(compiled.info.unsupported_function_actions, 0);
        assert!(compiled.messages.is_empty());
        assert!(compiled.overlays.is_empty());
    }

    #[test]
    fn compile_song_lua_exposes_sl_custom_prefs_helpers() {
        let song_dir = test_dir("sl-custom-prefs-helpers");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local prefs = SL_CustomPrefs.Get()
assert(prefs.VisualStyle.Default ~= nil)
assert(prefs.VisualStyle.Choices[1] ~= nil)
assert(prefs.VisualStyle.Values[1] ~= nil)
assert(prefs.AllowThemeVideos.Values[1] == true)
assert(prefs.NumberOfContinuesAllowed.Values[1] == 0)
assert(prefs.QRLogin.Values[1] == "Always")
assert(ThemePrefs.Get("EditModeLastSeenSong") == "")
assert(ThemePrefs.Get("RainbowMode") == false)
ThemePrefs.Set("RainbowMode", true)
assert(ThemePrefs.Get("RainbowMode") == true)
ThemePrefs.Set("RainbowMode", nil)
assert(ThemePrefs.Get("RainbowMode") == false)
ThemePrefs.InitAll({CustomCompilePref={Default="initialized"}})
assert(ThemePrefs.Get("CustomCompilePref") == "initialized")
ThemePrefs.Set("CustomCompilePref", "override")
ThemePrefs.InitAll({CustomCompilePref={Default="ignored"}})
assert(ThemePrefs.Get("CustomCompilePref") == "override")

mod_actions = {
    {1, function()
        ThemePrefs.Set("RuntimePref", "value")
        assert(ThemePrefs.Get("RuntimePref") == "value")
        ThemePrefs.Save()
        SL_CustomPrefs.Validate()
        SL_CustomPrefs.Init()
    end, true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "SL Custom Prefs Helpers"),
        )
        .unwrap();
        assert_eq!(compiled.info.unsupported_function_actions, 0);
        assert!(compiled.messages.is_empty());
        assert!(compiled.overlays.is_empty());
    }

    #[test]
    fn compile_song_lua_exposes_top_screen_option_row_shape() {
        let song_dir = test_dir("top-screen-option-row-shape");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local top = SCREENMAN:GetTopScreen()

local function FindOptionRowIndex(ScreenOptions, Name)
    if not ScreenOptions or not ScreenOptions.GetNumRows then return end
    for i=0, ScreenOptions:GetNumRows()-1 do
        if ScreenOptions:GetOptionRow(i):GetName() == Name then
            return i
        end
    end
end

local lines = THEME:GetMetric("ScreenPlayerOptions", "LineNames")
assert(lines:find("MusicRate"))

local speed = FindOptionRowIndex(top, "SpeedMod")
local mini = FindOptionRowIndex(top, "Mini")
local perspective = FindOptionRowIndex(top, "Perspective")
local variant = FindOptionRowIndex(top, "NoteSkinVariant")
local rate = FindOptionRowIndex(top, "MusicRate")
assert(speed and mini and perspective and variant and rate)
assert(top:GetNumRows() > rate)
assert(top:GetOptionRow(speed):GetName() == "SpeedMod")
assert(top:GetOptionRow(perspective):GetChoiceInRowWithFocus(PLAYER_1) == 1)

local speed_bmt = top:GetOptionRow(speed):GetChild(""):GetChild("Item")[PlayerNumber:Reverse()[PLAYER_1]+1]
assert(speed_bmt:GetText() == "1")
speed_bmt:settext("C400")
assert(speed_bmt:GetText() == "C400")

local mini_text = top:GetOptionRow(mini):GetChild(""):GetChild("Item")[1]:GetText():gsub("%%", "")
assert(mini_text == "0")

local title = top:GetOptionRow(rate):GetChild(""):GetChild("Title")
title:settext("Rate")
assert(title:GetText() == "Rate")

top:SetOptionRowIndex(PLAYER_1, rate)
assert(top:GetCurrentRowIndex(PLAYER_1) == rate)

mod_actions = {
    {1, function()
        top:RedrawOptions()
        top:GetOptionRow(variant):GetChild(""):GetChild("Item")[2]:settext("variant")
    end, true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Top Screen Option Row Shape"),
        )
        .unwrap();
        assert_eq!(compiled.info.unsupported_function_actions, 0);
        assert!(compiled.messages.is_empty());
        assert!(compiled.overlays.is_empty());
    }

    #[test]
    fn compile_song_lua_exposes_service_option_metrics() {
        let song_dir = test_dir("service-option-metrics");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local names = THEME:GetMetric("ScreenOptionsService", "LineNames")
assert(names:find("SystemOptions"))
assert(THEME:HasMetric("ScreenSystemOptions", "LineNames"))
assert(THEME:GetMetric("ScreenSystemOptions", "Fallback") == "ScreenOptionsServiceChild")
assert(THEME:GetMetric("ScreenVisualOptions", "Fallback") == "ScreenOptionsServiceSub")
assert(THEME:GetMetric("ScreenSystemOptions", "LineGame") == "conf,Game")
assert(THEME:GetMetric("ScreenSystemOptions", "LineTheme") == "lua,OperatorMenuOptionRows.Theme()")
assert(THEME:GetMetric("ScreenGraphicsSoundOptions", "LineDisplayMode") == "lua,ConfDisplayMode()")
assert(THEME:GetMetric("ScreenGraphicsSoundOptions", "LineFullscreenType") == "lua,ConfFullscreenType()")
assert(THEME:HasString("OptionTitles", "DisplayMode"))
assert(THEME:GetString("OptionTitles", "DisplayMode") == "DisplayMode")

local child_count = 0
local row_count = 0
for childscreen_name in names:gmatch("([^,]+)") do
    local screen = "Screen"..childscreen_name
    if THEME:HasMetric(screen, "LineNames") then
        child_count = child_count + 1
        for optrow_name in THEME:GetMetric(screen, "LineNames"):gmatch("([^,]+)") do
            local line = THEME:GetMetric(screen, "Line"..optrow_name)
            assert(type(line) == "string" and line ~= "")
            row_count = row_count + 1
            if row_count > 8 then break end
        end
    end
end
assert(child_count >= 4)
assert(row_count > 8)

local screen = SCREENMAN:GetTopScreen()
SL.Global.PrevScreenOptionsServiceRow[screen:GetName()] = 3
screen:SetOptionRowIndex(GAMESTATE:GetMasterPlayerNumber(), SL.Global.PrevScreenOptionsServiceRow[screen:GetName()])
assert(screen:GetCurrentRowIndex(GAMESTATE:GetMasterPlayerNumber()) == 3)

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Service Option Metrics"),
        )
        .unwrap();
        assert_eq!(compiled.info.unsupported_function_actions, 0);
        assert!(compiled.messages.is_empty());
        assert!(compiled.overlays.is_empty());
    }

    #[test]
    fn compile_song_lua_exposes_conf_option_row_helpers() {
        let song_dir = test_dir("conf-option-row-helpers");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local aspect = ConfAspectRatio()
assert(aspect.Name == "DisplayAspectRatio")
assert(aspect.Choices[1] == "16:9")
assert(aspect.Values[1] > 1.7)
assert(aspect.OneChoiceForAllPlayers == true)
assert(aspect:LoadSelections({})[1] == true)

local resolution = ConfDisplayResolution()
assert(resolution.Name == "DisplayResolution")
assert(resolution.Choices[1]:find("x"))

local mode = ConfDisplayMode()
assert(mode.Name == "DisplayMode")
assert(mode.Choices[1] == "Windowed")

local rate = ConfRefreshRate()
assert(rate.Name == "RefreshRate")
assert(rate.Choices[1] == "60")
assert(rate.Values[2] == 120)

local fullscreen = ConfFullscreenType()
assert(fullscreen.Name == "FullscreenType")
assert(fullscreen.Choices[1] == "Borderless")

mod_actions = {
    {1, function()
        aspect:SaveSelections({true}, PLAYER_1)
        assert(PREFSMAN:GetPreference("DisplayAspectRatio") > 1.7)
        resolution:SaveSelections({true}, PLAYER_1)
        assert(PREFSMAN:GetPreference("DisplayResolution") == "1920x1080")
        mode:SaveSelections({true}, PLAYER_1)
        assert(PREFSMAN:GetPreference("DisplayMode") == "Windowed")
        rate:SaveSelections({true}, PLAYER_1)
        assert(PREFSMAN:GetPreference("RefreshRate") == 60)
        fullscreen:SaveSelections({true}, PLAYER_1)
        assert(PREFSMAN:GetPreference("FullscreenType") == "Borderless")
    end, true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Conf Option Row Helpers"),
        )
        .unwrap();
        assert_eq!(compiled.info.unsupported_function_actions, 0);
        assert!(compiled.messages.is_empty());
        assert!(compiled.overlays.is_empty());
    }

    #[test]
    fn compile_song_lua_exposes_theme_manager_compat() {
        let song_dir = test_dir("theme-manager-compat");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local env = GAMESTATE:Env()
env.P1PeakNPS = 123

local function has_value(values, wanted)
    if not values then return false end
    for _, value in ipairs(values) do
        if value == wanted then return true end
    end
    return false
end

local bg_path, bg_group, bg_name = THEME:GetPathInfoB("ScreenGameplay", "Overlay")
assert(bg_path == THEME:GetPathB("ScreenGameplay", "Overlay"))
assert(bg_group == "ScreenGameplay" and bg_name == "Overlay")
assert(THEME:GetNumSelectableThemes() == 1)
assert(THEME:GetSelectableThemeNames()[1] == "Simply Love")
assert(THEME:get_theme_fallback_list()[1] == "Simply Love")
assert(THEME:GetThemeAuthor() == "")
assert(THEME:DoesThemeExist("Simply Love") == true)
assert(THEME:IsThemeSelectable("Simply Love") == true)
assert(THEME:DoesThemeExist("Missing Theme") == false)
assert(THEME:IsThemeSelectable("Missing Theme") == false)
assert(THEME:DoesLanguageExist("en") == true)
assert(THEME:DoesLanguageExist("english") == true)
assert(THEME:DoesLanguageExist("zz") == false)
assert(has_value(THEME:GetMetricNamesInGroup("ScreenSystemOptions"), "LineTheme"))
assert(has_value(THEME:GetMetricNamesInGroup("Player"), "DrawDistanceBeforeTargetsPixels"))
assert(THEME:GetMetricNamesInGroup("MissingGroup") == nil)
assert(has_value(THEME:GetStringNamesInGroup("OptionTitles"), "DisplayMode"))
assert(has_value(THEME:GetStringNamesInGroup("Difficulty"), "Difficulty_Hard"))
assert(THEME:GetStringNamesInGroup("MissingStrings") == nil)
assert(THEME:RunLuaScripts("Scripts") == nil)

PREFSMAN:SetPreference("Theme", "Ignored")
PROFILEMAN:SetStatsPrefix("Stats")
PROFILEMAN:SaveMachineProfile()
GAMESTATE:SaveProfiles()
assert(SONGMAN:SetPreferredSongs("Favorites.txt", true) == SONGMAN)
assert(SONGMAN:SetPreferredCourses("Courses.txt", true) == SONGMAN)

local song = GAMESTATE:GetCurrentSong()
local steps = GAMESTATE:GetCurrentSteps(PLAYER_1)
local course = GAMESTATE:GetCurrentCourse()
local all = SONGMAN:GetAllSongs()
local courses = SONGMAN:GetAllCourses()
local groups = SONGMAN:GetSongGroupNames()
local course_groups = SONGMAN:GetCourseGroupNames()
local found = SONGMAN:FindSong(song:GetSongDir())
local found_course = SONGMAN:FindCourse(course:GetCourseDir())
local extra_song, extra_steps = SONGMAN:GetExtraStageInfo(false, GAMESTATE:GetCurrentStyle())
local pref_songs = SONGMAN:GetPreferredSortSongs()
local pref_courses = SONGMAN:GetPreferredSortCourses("CourseType_Nonstop")
local preferred_section = SONGMAN:SongToPreferredSortSectionName(song)
local pss = STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1)
local played = STATSMAN:GetPlayedStageStats(1):GetPlayerStageStats(PLAYER_2)
local highscore = pss:GetHighScore()
local machine_scores = PROFILEMAN:GetMachineProfile():GetHighScoreList(song, steps):GetHighScores()

assert(SONGMAN:GetRandomSong() == song)
assert(SONGMAN:GetRandomCourse() == course)
assert(SONGMAN:GetSongFromSteps(steps) == song)
assert(found_course == course)
assert(SONGMAN:DoesCourseGroupExist(course_groups[1]))
assert(#SONGMAN:GetSongsInGroup(groups[1]) == 1)
assert(#SONGMAN:GetCoursesInGroup(course_groups[1]) == 1)
assert(SONGMAN:ShortenGroupName(groups[1]) == groups[1])
assert(SONGMAN:GetSongRank(song) == 1)
assert(extra_song == song and extra_steps == steps)
assert(#SONGMAN:GetPopularSongs() == 1)
assert(#SONGMAN:GetPopularCourses("CourseType_Nonstop") == 1)
assert(#SONGMAN:GetPreferredSortSongsBySectionName(preferred_section) == 1)
assert(SONGMAN:GetSongColor(song)[4] == 1)
assert(SONGMAN:GetSongGroupColor(groups[1])[4] == 1)
assert(SONGMAN:GetCourseColor(course)[4] == 1)
assert(SONGMAN:GetSongGroupBannerPath(groups[1]) ~= nil)
assert(SONGMAN:GetCourseGroupBannerPath(course_groups[1]) ~= nil)
assert(not SONGMAN:WasLoadedFromAdditionalSongs())
assert(not SONGMAN:WasLoadedFromAdditionalCourses())
assert(SONGMAN:GetNumLockedSongs() == 0)
assert(SONGMAN:GetNumUnlockedSongs() == 1)
assert(SONGMAN:GetNumSelectableAndUnlockedSongs() == 1)
assert(SONGMAN:GetNumAdditionalSongs() == 0)
assert(SONGMAN:GetNumCourses() == 1)
assert(SONGMAN:GetNumAdditionalCourses() == 0)
assert(SONGMAN:GetNumCourseGroups() == 1)

mod_actions = {
    {
        1,
        string.format(
            "%d:%s:%s:%.0f:%s:%d:%d:%d:%d:%d:%s:%s:%.0f:%d:%d:%s:%s:%d",
            GAMESTATE:GetNumPlayersEnabled(),
            THEME:GetCurThemeName(),
            THEME:GetThemeDisplayName(),
            GetTimeSinceStart(),
            tostring(HolidayCheer()),
            #pref_songs,
            #pref_courses,
            #all,
            #courses,
            #groups,
            found:GetDisplayMainTitle(),
            tostring(SONGMAN:DoesSongGroupExist(groups[1])),
            SONGMAN:GetGroup(song):GetSyncOffset(),
            pss:GetPossibleDancePoints(),
            played:GetActualDancePoints(),
            pss:GetGrade(),
            highscore:GetName(),
            #machine_scores
        ),
        true,
    },
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Theme Manager Compat"),
        )
        .unwrap();

        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "2:Simply Love:Simply Love:0:false:1:1:1:1:1:Theme Manager Compat:true:0:1:0:Grade_Tier07:Player:1"
        );
        assert_eq!(compiled.info.unsupported_function_actions, 0);
    }

    #[test]
    fn compile_song_lua_exposes_fallback_theme_utility_helpers() {
        let song_dir = test_dir("theme-utility-helpers");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    LoadFont("Common Normal")..{
        Text=table.concat({
            SecondsToMSS(125),
            SecondsToMMSS(65),
            SecondsToMSSMsMs(65.5),
            SecondsToMMSSMsMs(65.5),
            SecondsToHHMMSS(3661),
            FormatNumberAndSuffix(1),
            FormatNumberAndSuffix(2),
            FormatNumberAndSuffix(3),
            FormatNumberAndSuffix(11),
            FormatNumberAndSuffix(113),
        }, "|"),
        OnCommand=function(self)
            mod_actions = {
                {
                    1,
                    string.format(
                        "%.3f:%.1f:%.0f:%s",
                        GetScreenAspectRatio(),
                        WideScale(100, 200),
                        clamp(5, 0, 3),
                        self:GetText()
                    ),
                    true,
                },
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Theme Utility Helpers");
        context.screen_width = 854.0;
        context.screen_height = 480.0;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();

        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "1.779:200.0:3:2:05|01:05|1:05.50|01:05.50|01:01:01|1st|2nd|3rd|11th|113th"
        );
    }

    #[test]
    fn compile_song_lua_exposes_theme_process_compat_helpers() {
        let song_dir = test_dir("theme-process-helpers");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local top = SCREENMAN:GetTopScreen()
local sound = LoadActor(THEME:GetPathS("", "Common invalid.ogg"))
sound:play():stop()

SOUND:PlayOnce(THEME:GetPathS("", "_unlock.ogg"))
SOUND:DimMusic(0.5, 1.0)
SOUND:PlayMusicPart("sample.ogg", 0, 5)
SOUND:StopMusic()
top:SetNextScreenName("ScreenEvaluationStage")
top:AddInputCallback(function() end):PauseGame(true):RemoveInputCallback(function() end)
top:StartTransitioningScreen("SM_GoToNextScreen")

mod_actions = {
    {
        1,
        string.format(
            "%s:%s:%s:%s:%s:%s:%d:%.0f:%s:%s:%d",
            top:GetName(),
            top:GetNextScreenName(),
            THEME:GetMetric(top:GetName(), "Class"),
            THEME:GetMetric("Common", "DefaultNoteSkinName"),
            tostring(THEME:HasMetric("Player", "ReceptorArrowsYStandard")),
            tostring(THEME:GetMetricB("ScreenHeartEntry", "HeartEntryEnabled")),
            THEME:GetMetricI("MusicWheel", "NumWheelItems"),
            THEME:GetMetricF("GraphDisplay", "BodyWidth"),
            ScreenString("Cancel"),
            string.sub(THEME:GetPathG("Combo", "100Milestone"), 1, 20),
            top:GetCurrentRowIndex(PLAYER_1)
        ),
        true,
    },
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Theme Process Helpers"),
        )
        .unwrap();

        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "ScreenGameplay:ScreenEvaluationStage:ScreenGameplay:default:true:false:15:300:Cancel:__songlua_theme_path:0"
        );
        assert_eq!(compiled.info.unsupported_function_actions, 0);
    }

    #[test]
    fn compile_song_lua_exposes_screen_process_shims() {
        let song_dir = test_dir("screen-process-shims");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
SCREENMAN:set_input_redirected(PLAYER_1, true)
SCREENMAN:AddNewScreenToTop("ScreenTextEntry")

local top = SCREENMAN:GetTopScreen()
top:Load({ Question="Search" })
    :SetPrevScreenName("ScreenSelectMusic")
    :SetNextScreenName("ScreenGameplay")
    :PostScreenMessage("SM_BeginFailed", 0)
    :SetProfileIndex(PLAYER_1, -1)
    :PauseGame(true)

local wheel = top:GetMusicWheel()
wheel:SetOpenSection(""):ChangeSort("SortOrder_Preferred")

mod_actions = {
    {
        1,
        string.format(
            "%s:%s:%s:%s:%s:%s:%.0f:%s",
            top:GetName(),
            top:GetPrevScreenName(),
            top:GetNextScreenName(),
            tostring(top:IsPaused()),
            tostring(top:AllAreOnLastRow()),
            tostring(wheel:IsLocked()),
            top:GetChild("Timer"):GetSeconds(),
            tostring(top:GetNextCourseSong() == GAMESTATE:GetCurrentSong())
        ),
        true,
    },
}

top:Cancel():Finish():begin_backing_out()

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Screen Process Shims"),
        )
        .unwrap();

        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "ScreenTextEntry:ScreenSelectMusic:ScreenGameplay:true:false:false:0:true"
        );
    }

    #[test]
    fn compile_song_lua_resolves_next_course_song_background() {
        let song_dir = test_dir("next-course-song-background");
        image::RgbaImage::new(96, 54)
            .save(song_dir.join("background.png"))
            .unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
mod_actions = {}

return Def.ActorFrame{
    Def.Sprite{
        OnCommand=function(self)
            local song = SCREENMAN:GetTopScreen():GetNextCourseSong()
            self:LoadFromSongBackground(song)
            local texture = self:GetTexture()
            mod_actions[#mod_actions + 1] = {
                1,
                string.format(
                    "%s:%s:%d:%d",
                    tostring(song == GAMESTATE:GetCurrentSong()),
                    tostring(texture:GetPath():match("background%.png$") ~= nil),
                    self:GetWidth(),
                    self:GetHeight()
                ),
                true,
            }
        end,
    },
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Next Course Song Background"),
        )
        .unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, "true:true:96:54");
        assert_eq!(compiled.overlays.len(), 1);
        assert!(matches!(
            compiled.overlays[0].kind,
            SongLuaOverlayKind::Sprite { ref texture_path, .. }
                if texture_path.ends_with("background.png")
        ));
    }

    #[test]
    fn compile_song_lua_exposes_song_and_steps_metadata() {
        let root_dir = test_dir("song-steps-metadata");
        let song_dir = root_dir.join("Pack A").join("Song A");
        fs::create_dir_all(&song_dir).unwrap();
        fs::write(song_dir.join("chart.ssc"), "").unwrap();
        fs::write(song_dir.join("music.ogg"), "").unwrap();
        image::RgbaImage::new(100, 40)
            .save(song_dir.join("banner.png"))
            .unwrap();
        image::RgbImage::new(320, 240)
            .save(song_dir.join("background.jpg"))
            .unwrap();
        image::RgbaImage::new(120, 120)
            .save(song_dir.join("jacket.png"))
            .unwrap();
        image::RgbaImage::new(80, 80)
            .save(song_dir.join("cdtitle.png"))
            .unwrap();
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local song = GAMESTATE:GetCurrentSong()
local steps = GAMESTATE:GetCurrentSteps(PLAYER_1)
local song_bpms = song:GetTimingData():GetActualBPM()
local steps_timing = steps:GetTimingData()
local radar = steps:GetRadarValues(PLAYER_1)
mod_actions = {
    {
        1,
        string.format(
            "%s|%s|%s|%s|%s|%s|%s|%s|%s|%.0f|%s|%s|%s|%d|%d|%.1f|%.0f|%.0f|%.0f|%.0f",
            song:GetDisplayFullTitle(),
            song:GetTranslitMainTitle(),
            song:GetDisplaySubTitle(),
            song:GetGroupName(),
            tostring(song:HasMusic()),
            tostring(song:HasBanner()),
            tostring(song:HasBackground()),
            tostring(song:HasJacket()),
            tostring(song:HasCDImage()),
            song:GetStageCost(),
            tostring(song:GetMusicPath():match("music%.ogg$") ~= nil),
            tostring(song:GetBannerPath():match("banner%.png$") ~= nil),
            tostring(steps:GetFilename():match("chart%.ssc$") ~= nil),
            #song:GetAllSteps(),
            steps:GetMeter(),
            song:MusicLengthSeconds(),
            radar:GetValue("RadarCategory_Notes"),
            song_bpms[1],
            song_bpms[2],
            steps_timing:GetBPMAtBeat(0)
        ),
        true,
    },
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

        let mut context = SongLuaCompileContext::new(&song_dir, "Song Metadata");
        context.song_display_bpms = [90.0, 180.0];
        context.players[0].difficulty = SongLuaDifficulty::Hard;
        context.players[0].display_bpms = [150.0, 210.0];
        context.music_length_seconds = 123.4;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();

        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(
            compiled.messages[0].message,
            "Song Metadata|Song Metadata||Pack A|true|true|true|true|true|1|true|true|true|6|10|123.4|0|90|180|150"
        );
    }

    fn generated_runtime_mod_lua() -> &'static str {
        r#"
mods = {
    {0, 9999, "*1000 no beat, *1000 no drunk, *1000 no tipsy, *1000 no invert, *1000 no flip, *1000 no dizzy", "end"},
}
mod_time = {
    {0.00, 999, "*1 0 Dark1, *1 0 Dark2, *1 0 Dark3, *1 0 Dark4, *1 0 PulseOuter, *1 0 PulseOffset, *1 0 Wave, *1 0 Bumpy3, *1 0 BumpyPeriod, *1 0 Stealth, *1 0 Blind, *1 0 Sudden, *1 0 Tipsy, *1 0 Drunk, *1 0 Dark", "len"},
}
mods_ease = {}

local l = "len"
local function me(...)
    table.insert(mods_ease, {...})
end

me(4, 0.75, 250, 0, "Bumpy1", l, ease.outQuad)
me(4, 0.75, -125, 0, "BumpyPeriod", l, ease.outQuad)
me(4, 0.75, 75, 0, "Wave", l, ease.outElastic)
me(8, 0.75, 250, 0, "Bumpy2", l, ease.outQuad)
me(12, 0.75, 250, 0, "Bumpy3", l, ease.outQuad)
me(16, 0.75, 250, 0, "Bumpy4", l, ease.outQuad)
me(20, 1.5, 50, 1, "hidden", l, ease.outInQuad)
me(24, 0.5, 25, 0, "beat", l, ease.outBounce)

return Def.ActorFrame{}
"#
    }

    #[test]
    fn compile_song_lua_samples_overlay_perframes_into_overlay_eases() {
        let song_dir = test_dir("perframe-overlay");
        let entry = song_dir.join("default.lua");
        fs::write(
            &entry,
            r#"
local target
mod_perframes = {
    {8, 9, function(beat)
        if target then
            target:x((beat - 8) * 120)
            target:diffusealpha(1 - (beat - 8))
        end
    end},
}
return Def.ActorFrame{
    Def.Quad{
        InitCommand=function(self)
            target = self
            self:zoomto(16, 16)
        end
    }
}
"#,
        )
        .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Perframe Overlay"),
        )
        .unwrap();
        assert_eq!(compiled.info.unsupported_perframes, 0);
        assert!(compiled.overlay_eases.iter().any(|ease| {
            ease.overlay_index == 0 && ease.from.x.is_some() && ease.to.x.is_some()
        }));
        assert!(compiled.overlay_eases.iter().any(|ease| {
            ease.overlay_index == 0 && ease.from.diffuse.is_some() && ease.to.diffuse.is_some()
        }));
    }

    #[test]
    fn multitap_sample_eases_step_visibility_edges() {
        let baseline = SongLuaOverlayState {
            visible: false,
            ..SongLuaOverlayState::default()
        };
        let hidden = baseline;
        let visible_a = SongLuaOverlayState {
            visible: true,
            x: 100.0,
            y: -20.0,
            rot_z_deg: 90.0,
            ..SongLuaOverlayState::default()
        };
        let visible_b = SongLuaOverlayState {
            visible: true,
            x: 120.0,
            y: -10.0,
            rot_z_deg: 90.0,
            ..SongLuaOverlayState::default()
        };
        let samples = [
            (0.0, hidden),
            (0.125, visible_a),
            (0.25, visible_b),
            (0.375, hidden),
        ];
        let mut eases = Vec::new();

        push_overlay_sample_eases(&mut eases, 7, baseline, &samples);

        assert!(eases.iter().any(|ease| {
            ease.overlay_index == 7
                && ease.start == 0.125
                && ease.limit == 0.0
                && ease.to.visible == Some(true)
                && ease.to.x == Some(100.0)
                && ease.to.y == Some(-20.0)
        }));
        assert!(eases.iter().any(|ease| {
            ease.overlay_index == 7
                && ease.start == 0.125
                && ease.limit == 0.125
                && ease.from.x == Some(100.0)
                && ease.to.x == Some(120.0)
                && ease.to.visible == Some(true)
        }));
        let hide = eases
            .iter()
            .find(|ease| {
                ease.overlay_index == 7
                    && ease.start == 0.375
                    && ease.limit == 0.0
                    && ease.to.visible == Some(false)
            })
            .expect("visibility should step off instead of tweening to the baseline");
        assert_eq!(hide.to.x, None);
        assert_eq!(hide.to.y, None);
        assert!(!eases.iter().any(|ease| {
            ease.to.visible == Some(false)
                && ease.limit > 0.0
                && (ease.to.x.is_some() || ease.to.y.is_some())
        }));
    }

    #[test]
    fn multitap_deco_state_rotates_yinyang_during_bounce() {
        let state = multitap_deco_state(
            SongLuaOverlayState::default(),
            SongLuaNoteskinResolver::default(),
            "ddr-note",
            MultitapPhase {
                pos: 0.0,
                squish: 0.0,
                lin: 0.75,
                qtc: 1,
                visible: true,
            },
        );

        assert!(state.visible);
        assert_eq!(state.rot_z_deg, 135.0);
    }

    #[test]
    fn multitap_arrow_sampler_does_not_emit_inactive_baseline() {
        let baseline = SongLuaOverlayState {
            visible: true,
            rot_z_deg: 0.0,
            ..SongLuaOverlayState::default()
        };
        let mut samples = Vec::new();

        push_multitap_arrow_sample(
            &mut samples,
            10.0,
            baseline,
            SongLuaNoteskinResolver::default(),
            "ddr-note",
            1,
            MultitapPhase {
                pos: 0.0,
                squish: 0.0,
                lin: 1.0,
                qtc: 1,
                visible: true,
            },
        );
        push_multitap_arrow_sample(
            &mut samples,
            10.001,
            baseline,
            SongLuaNoteskinResolver::default(),
            "ddr-note",
            1,
            MultitapPhase {
                pos: 0.0,
                squish: 0.0,
                lin: 0.0,
                qtc: 0,
                visible: false,
            },
        );

        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].1.visible, true);
        assert_eq!(samples[0].1.rot_z_deg, 90.0);

        let mut eases = Vec::new();
        push_overlay_sample_eases(&mut eases, 3, baseline, &samples);

        assert!(!eases.iter().any(|ease| {
            ease.overlay_index == 3
                && ease.limit > 0.0
                && ease.from.rot_z_deg == Some(90.0)
                && ease.to.rot_z_deg == Some(0.0)
        }));
        assert!(
            !eases
                .iter()
                .any(|ease| ease.overlay_index == 3 && ease.to.visible == Some(false))
        );
    }

    #[test]
    fn compile_song_lua_loadfile_accepts_existing_song_dir_relative_path() {
        let root = PathBuf::from("target")
            .join(format!("song-lua-relative-loadfile-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("lua")).unwrap();
        fs::write(
            root.join("lua/default.lua"),
            r#"
local path = GAMESTATE:GetCurrentSong():GetSongDir() .. "lua/"
loadfile(path .. "helper.lua")()
return Def.ActorFrame {}
"#,
        )
        .unwrap();
        fs::write(
            root.join("lua/helper.lua"),
            "relative_loadfile_helper_ran = true\n",
        )
        .unwrap();

        let entry = root.join("lua/default.lua");
        test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&root, "Relative Loadfile"),
        )
        .expect("loadfile should accept an existing relative path from GetSongDir");
    }

    #[test]
    fn compile_song_lua_supports_generated_runtime_modchart() {
        let root = test_dir("generated-runtime-modchart");
        let entry = root.join("default.lua");
        fs::write(&entry, generated_runtime_mod_lua()).unwrap();

        let mut context = SongLuaCompileContext::new(&root, "Generated Runtime Modchart");
        context.players = [
            SongLuaPlayerContext {
                enabled: true,
                difficulty: SongLuaDifficulty::Challenge,
                speedmod: SongLuaSpeedMod::X(2.0),
                ..SongLuaPlayerContext::default()
            },
            SongLuaPlayerContext {
                enabled: true,
                difficulty: SongLuaDifficulty::Challenge,
                speedmod: SongLuaSpeedMod::C(516.0),
                ..SongLuaPlayerContext::default()
            },
        ];

        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert!(!compiled.beat_mods.is_empty());
        assert!(!compiled.time_mods.is_empty());
        assert!(compiled.eases.len() >= 8);
        assert!(compiled.eases.iter().any(
            |ease| matches!(ease.target, SongLuaEaseTarget::Mod(ref name) if name == "Bumpy1")
        ));
        assert!(compiled.eases.iter().any(
            |ease| matches!(ease.target, SongLuaEaseTarget::Mod(ref name) if name == "Bumpy4")
        ));
        assert!(compiled.eases.iter().any(
            |ease| matches!(ease.target, SongLuaEaseTarget::Mod(ref name) if name == "hidden")
        ));
        assert!(
            compiled.eases.iter().any(
                |ease| matches!(ease.target, SongLuaEaseTarget::Mod(ref name) if name == "beat")
            )
        );
    }

    #[test]
    fn compile_song_lua_supports_player_function_ease_fixture() {
        let (root, entry) = song_lua_fixture("player-eases.lua");

        let mut context = SongLuaCompileContext::new(&root, "Player Ease Fixture");
        context.players = [
            SongLuaPlayerContext {
                enabled: true,
                difficulty: SongLuaDifficulty::Challenge,
                speedmod: SongLuaSpeedMod::X(2.0),
                ..SongLuaPlayerContext::default()
            },
            SongLuaPlayerContext {
                enabled: true,
                difficulty: SongLuaDifficulty::Challenge,
                speedmod: SongLuaSpeedMod::C(516.0),
                ..SongLuaPlayerContext::default()
            },
        ];

        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.messages.len(), 2);
        assert_eq!(compiled.overlays.len(), 3);
        assert_eq!(compiled.eases.len(), 2);
        assert!(compiled.eases.iter().all(|ease| ease.easing.is_some()));
        assert!(
            compiled
                .eases
                .iter()
                .any(|ease| ease.easing.as_deref() == Some("outCirc"))
        );
        assert!(
            compiled
                .eases
                .iter()
                .any(|ease| ease.easing.as_deref() == Some("outExpo"))
        );
        assert_eq!(compiled.info.unsupported_function_eases, 0);
        assert!(
            compiled
                .eases
                .iter()
                .any(|ease| matches!(ease.target, SongLuaEaseTarget::PlayerRotationZ))
        );
        assert!(
            compiled
                .eases
                .iter()
                .any(|ease| matches!(ease.target, SongLuaEaseTarget::PlayerSkewX))
        );
    }

    #[test]
    fn compile_song_lua_supports_named_mod_ease_fixture() {
        let (root, entry) = song_lua_fixture("mod-eases.lua");

        let mut context = SongLuaCompileContext::new(&root, "Named Mod Ease Fixture");
        context.players = [
            SongLuaPlayerContext {
                enabled: true,
                difficulty: SongLuaDifficulty::Challenge,
                speedmod: SongLuaSpeedMod::X(2.0),
                ..SongLuaPlayerContext::default()
            },
            SongLuaPlayerContext {
                enabled: false,
                difficulty: SongLuaDifficulty::Easy,
                speedmod: SongLuaSpeedMod::X(1.0),
                ..SongLuaPlayerContext::default()
            },
        ];

        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert!(!compiled.time_mods.is_empty());
        assert_eq!(compiled.eases.len(), 11);
        assert!(compiled.eases.iter().all(|ease| ease.easing.is_some()));
        assert!(
            compiled
                .eases
                .iter()
                .any(|ease| ease.easing.as_deref() == Some("outCirc"))
        );
        assert!(
            compiled
                .eases
                .iter()
                .any(|ease| ease.easing.as_deref() == Some("inCirc"))
        );
        assert!(
            compiled.eases.iter().any(
                |ease| matches!(ease.target, SongLuaEaseTarget::Mod(ref name) if name == "tiny")
            )
        );
    }

    #[test]
    fn compile_song_lua_preserves_runtime_mod_targets_from_fixture() {
        let (root, entry) = song_lua_fixture("mod-eases.lua");

        let mut context = SongLuaCompileContext::new(&root, "Runtime Mod Target Fixture");
        context.players = [
            SongLuaPlayerContext {
                enabled: true,
                difficulty: SongLuaDifficulty::Hard,
                speedmod: SongLuaSpeedMod::X(2.0),
                ..SongLuaPlayerContext::default()
            },
            SongLuaPlayerContext {
                enabled: false,
                difficulty: SongLuaDifficulty::Hard,
                speedmod: SongLuaSpeedMod::X(1.0),
                ..SongLuaPlayerContext::default()
            },
        ];

        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.eases.len(), 11);
        assert_eq!(compiled.overlays.len(), 1);
        assert!(compiled.eases.iter().all(|ease| ease.easing.is_some()));
        for target in [
            "tiny",
            "drunk",
            "tipsy",
            "brake",
            "beat",
            "stealth",
            "movey1",
            "confusionoffset1",
        ] {
            assert!(
                compiled.eases.iter().any(
                    |ease| matches!(ease.target, SongLuaEaseTarget::Mod(ref name) if name == target)
                ),
                "missing fixture runtime mod target {target}"
            );
        }
        assert_eq!(compiled.info.unsupported_function_eases, 0);
        assert_eq!(compiled.info.unsupported_function_actions, 0);
    }

    #[test]
    fn compile_song_lua_captures_double_column_bounce_fixture() {
        let (root, entry) = song_lua_fixture("column-bounces.lua");

        let mut context = SongLuaCompileContext::new(&root, "Column Bounce Fixture");
        context.style_name = "double".to_string();
        context.players = [
            SongLuaPlayerContext {
                enabled: true,
                difficulty: SongLuaDifficulty::Challenge,
                speedmod: SongLuaSpeedMod::X(2.0),
                ..SongLuaPlayerContext::default()
            },
            SongLuaPlayerContext {
                enabled: false,
                difficulty: SongLuaDifficulty::Challenge,
                speedmod: SongLuaSpeedMod::X(1.0),
                ..SongLuaPlayerContext::default()
            },
        ];

        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert!(!compiled.column_offsets.is_empty());
        assert!(compiled.column_offsets.iter().any(|window| {
            window.player == 0
                && window.column == 7
                && (window.from_y - 33.75).abs() <= 0.001
                && window.to_y.abs() <= 0.001
                && window.easing.as_deref() == Some("outSine")
        }));
        assert!(
            compiled
                .column_offsets
                .iter()
                .any(|window| window.player == 0
                    && window.column == 6
                    && window.from_y.abs() <= 0.001
                    && (window.to_y - 33.75).abs() <= 0.001)
        );
    }

    #[test]
    fn compile_song_lua_supports_basic_mod_overlay_fixture() {
        let (root, entry) = song_lua_fixture("basic.lua");

        let mut context = SongLuaCompileContext::new(&root, "Basic Mod Overlay Fixture");
        context.players = [
            SongLuaPlayerContext {
                enabled: true,
                difficulty: SongLuaDifficulty::Challenge,
                speedmod: SongLuaSpeedMod::X(2.0),
                ..SongLuaPlayerContext::default()
            },
            SongLuaPlayerContext {
                enabled: false,
                difficulty: SongLuaDifficulty::Challenge,
                speedmod: SongLuaSpeedMod::X(1.0),
                ..SongLuaPlayerContext::default()
            },
        ];

        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert!(!compiled.beat_mods.is_empty());
        assert!(!compiled.overlays.is_empty());
    }

    #[test]
    fn compile_song_lua_supports_elastic_mod_ease_fixture() {
        let (root, entry) = song_lua_fixture("mod-eases.lua");

        let mut context = SongLuaCompileContext::new(&root, "Elastic Mod Ease Fixture");
        context.players = [
            SongLuaPlayerContext {
                enabled: true,
                difficulty: SongLuaDifficulty::Challenge,
                speedmod: SongLuaSpeedMod::X(2.0),
                ..SongLuaPlayerContext::default()
            },
            SongLuaPlayerContext {
                enabled: false,
                difficulty: SongLuaDifficulty::Easy,
                speedmod: SongLuaSpeedMod::X(1.0),
                ..SongLuaPlayerContext::default()
            },
        ];

        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert!(!compiled.time_mods.is_empty());
        assert_eq!(compiled.eases.len(), 11);
        assert!(compiled.eases.iter().all(|ease| ease.easing.is_some()));
        assert!(
            compiled
                .eases
                .iter()
                .any(|ease| ease.easing.as_deref() == Some("outElastic"))
        );
        assert!(compiled.eases.iter().any(
            |ease| matches!(ease.target, SongLuaEaseTarget::Mod(ref name) if name == "Bumpy1")
        ));
    }

    #[test]
    fn compile_song_lua_supports_overlay_fixture() {
        let (root, entry) = song_lua_fixture("basic.lua");

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&root, "Overlay Fixture"),
        )
        .unwrap();
        assert!(!compiled.overlays.is_empty());
    }

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
    fn actor_indices_for_pointers_matches_known_actor_tables() {
        let actor_ptrs = HashSet::from([20_usize, 40]);

        assert_eq!(
            actor_indices_for_pointers(5, |index| (index + 1) * 10, &actor_ptrs),
            vec![1, 3]
        );
    }

    #[test]
    fn actor_pointers_touch_actor_requires_probe_hit() {
        assert!(actor_pointers_touch_actor(
            4,
            |index| (index + 1) * 10,
            &[20, 99]
        ));
        assert!(!actor_pointers_touch_actor(
            4,
            |index| (index + 1) * 10,
            &[99]
        ));
        assert!(!actor_pointers_touch_actor(
            4,
            |index| (index + 1) * 10,
            &[]
        ));
    }

    #[test]
    fn function_ease_actor_indices_falls_back_to_all_when_unprobed() {
        assert_eq!(
            function_ease_actor_indices(3, |index| (index + 1) * 10, &[]),
            vec![0, 1, 2]
        );
        assert_eq!(
            function_ease_actor_indices(3, |index| (index + 1) * 10, &[999]),
            vec![0, 1, 2]
        );
        assert_eq!(
            function_ease_actor_indices(3, |index| (index + 1) * 10, &[30]),
            vec![2]
        );
    }

    #[test]
    fn function_named_upvalue_tables_dedups_matching_tables() {
        let lua = Lua::new();
        let debug = create_debug_table(&lua).unwrap();
        let getupvalue = debug.get::<Function>("getupvalue").unwrap();
        let function = lua
            .load(
                r#"
                local target = { name = "target" }
                local duplicate = target
                local ignored = { name = "ignored" }
                return function()
                    return target, duplicate, ignored
                end
                "#,
            )
            .eval::<Function>()
            .unwrap();
        let mut seen = HashSet::new();

        let out = function_named_upvalue_tables(
            &getupvalue,
            &function,
            &["target", "duplicate"],
            &mut seen,
        )
        .unwrap();

        assert_eq!(out.len(), 1);
        assert_eq!(out[0].get::<String>("name").unwrap(), "target");
    }

    #[test]
    fn nested_function_named_upvalue_tables_recurses_once_per_function() {
        let lua = Lua::new();
        let debug = create_debug_table(&lua).unwrap();
        let getupvalue = debug.get::<Function>("getupvalue").unwrap();
        let function = lua
            .load(
                r#"
                local target = { name = "target" }
                local function inner()
                    return target
                end
                return function()
                    return inner
                end
                "#,
            )
            .eval::<Function>()
            .unwrap();
        let mut seen_tables = HashSet::new();
        let mut seen_functions = HashSet::new();

        let out = nested_function_named_upvalue_tables(
            &getupvalue,
            &function,
            &["target"],
            &mut seen_tables,
            &mut seen_functions,
        )
        .unwrap();

        assert_eq!(out.len(), 1);
        assert_eq!(out[0].get::<String>("name").unwrap(), "target");
        let second = nested_function_named_upvalue_tables(
            &getupvalue,
            &function,
            &["target"],
            &mut seen_tables,
            &mut seen_functions,
        )
        .unwrap();
        assert!(second.is_empty());
    }

    #[test]
    fn actor_overlay_initial_state_reads_actor_state_fields() {
        let lua = Lua::new();
        let actor = lua.create_table().unwrap();
        let diffuse = lua.create_table().unwrap();
        for (index, value) in [0.1_f32, 0.2, 0.3, 0.4].into_iter().enumerate() {
            diffuse.raw_set(index + 1, value).unwrap();
        }
        let size = lua.create_table().unwrap();
        size.raw_set(1, 64.0_f32).unwrap();
        size.raw_set(2, 32.0_f32).unwrap();
        actor.set("__songlua_visible", false).unwrap();
        actor.set("__songlua_state_x", 12.0_f32).unwrap();
        actor.set("__songlua_state_y", -8.0_f32).unwrap();
        actor.set("__songlua_state_diffuse", diffuse).unwrap();
        actor.set("__songlua_state_size", size).unwrap();
        actor.set("__songlua_state_blend", "BlendMode_Add").unwrap();
        actor.set("__songlua_state_effect_clock", "beat").unwrap();
        actor
            .set("__songlua_state_effect_mode", "glowshift")
            .unwrap();

        let state = actor_overlay_initial_state(&actor).unwrap();

        assert!(!state.visible);
        assert_eq!(state.x, 12.0);
        assert_eq!(state.y, -8.0);
        assert_eq!(state.diffuse, [0.1, 0.2, 0.3, 0.4]);
        assert_eq!(state.size, Some([64.0, 32.0]));
        assert_eq!(
            state.blend,
            parse_overlay_blend_mode("BlendMode_Add").unwrap()
        );
        assert_eq!(
            state.effect_clock,
            parse_overlay_effect_clock("beat").unwrap()
        );
        assert_eq!(
            state.effect_mode,
            parse_overlay_effect_mode("glowshift").unwrap()
        );
    }

    #[test]
    fn display_child_state_readers_use_named_child_state() {
        let lua = Lua::new();
        let graph = lua.create_table().unwrap();
        let graph_children = lua.create_table().unwrap();
        let line = lua.create_table().unwrap();
        line.set("__songlua_state_x", 5.0_f32).unwrap();
        graph_children.set("Line", line).unwrap();
        let body_group = lua.create_table().unwrap();
        let first_body = lua.create_table().unwrap();
        first_body.set("__songlua_state_y", 1.0_f32).unwrap();
        let second_body = lua.create_table().unwrap();
        second_body.set("__songlua_state_y", 2.0_f32).unwrap();
        body_group.raw_set(1, first_body).unwrap();
        body_group.raw_set(2, second_body).unwrap();
        graph_children.set("", body_group).unwrap();
        graph.set("__songlua_children", graph_children).unwrap();

        let line_state = read_graph_display_line_state(&lua, &graph).unwrap();
        let body_state = read_graph_display_body_state(&lua, &graph).unwrap();

        assert_eq!(line_state.x, 5.0);
        assert_eq!(body_state.y, 2.0);

        let song_meter = lua.create_table().unwrap();
        song_meter.set("__songlua_stream_width", 128.0_f32).unwrap();
        let song_meter_children = lua.create_table().unwrap();
        let stream = lua.create_table().unwrap();
        stream.set("__songlua_state_zoom", 1.5_f32).unwrap();
        song_meter_children.set("Stream", stream).unwrap();
        song_meter
            .set("__songlua_children", song_meter_children)
            .unwrap();

        let Some((stream_width, stream_state)) =
            read_song_meter_display_state(&lua, &song_meter).unwrap()
        else {
            panic!("expected song meter stream state");
        };

        assert_eq!(stream_width, 128.0);
        assert_eq!(stream_state.zoom, 1.5);
    }

    #[test]
    fn update_function_table_scans_find_direct_nested_and_global_upvalues() {
        let lua = Lua::new();
        lua.globals()
            .set("debug", create_debug_table(&lua).unwrap())
            .unwrap();
        let root = lua
            .load(
                r#"
                local mods = { name = "mods" }
                local nodes = { name = "nodes" }
                local function inner()
                    return nodes
                end
                return {
                    __songlua_update_function = function()
                        return mods
                    end,
                    {
                        __songlua_update_function = function()
                            return inner
                        end
                    }
                }
                "#,
            )
            .eval::<Table>()
            .unwrap();

        let direct =
            read_update_function_tables(&lua, &Value::Table(root.clone()), &["mods"]).unwrap();
        let nested =
            read_update_function_nested_tables(&lua, &Value::Table(root), &["nodes"]).unwrap();

        assert_eq!(direct.len(), 1);
        assert_eq!(direct[0].get::<String>("name").unwrap(), "mods");
        assert_eq!(nested.len(), 1);
        assert_eq!(nested[0].get::<String>("name").unwrap(), "nodes");

        let source = lua
            .load(
                r#"
                local nodes = { name = "global" }
                local function inner()
                    return nodes
                end
                return {
                    Run = function()
                        return inner
                    end
                }
                "#,
            )
            .eval::<Table>()
            .unwrap();
        lua.globals().set("global_updates", source).unwrap();

        let global =
            read_global_function_nested_tables(&lua, "global_updates", &["Run"], &["nodes"])
                .unwrap();

        assert_eq!(global.len(), 1);
        assert_eq!(global[0].get::<String>("name").unwrap(), "global");
    }

    #[test]
    fn top_screen_theme_child_names_include_compat_children() {
        assert!(TOP_SCREEN_THEME_CHILD_NAMES.contains(&"BPMDisplay"));
        assert!(TOP_SCREEN_THEME_CHILD_NAMES.contains(&"StepsDisplayP2"));
        assert!(UNDERLAY_THEME_CHILD_NAMES.contains(&"StepStatsPaneP1"));
        assert!(UNDERLAY_THEME_CHILD_NAMES.contains(&"SongMeter"));
    }

    #[test]
    fn indexed_actor_capture_blocks_preserve_source_indices() {
        let lua = Lua::new();
        let first = lua.create_table().unwrap();
        let second = lua.create_table().unwrap();
        reset_actor_capture(&lua, &first).unwrap();
        reset_actor_capture(&lua, &second).unwrap();
        capture_block_set_f32(&lua, &second, "x", 12.0).unwrap();
        let actors = vec![(3, first), (7, second)];

        let blocks = collect_indexed_actor_capture_blocks(&actors).unwrap();

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].0, 7);
        assert_eq!(blocks[0].1[0].delta.x, Some(12.0));
        reset_indexed_actor_capture_tables(&lua, &actors).unwrap();
        assert!(
            collect_indexed_actor_capture_blocks(&actors)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn capture_indexed_actor_function_blocks_restores_state_and_runtime() {
        let lua = Lua::new();
        let actor = lua.create_table().unwrap();
        actor.set("__songlua_visible", true).unwrap();
        reset_actor_capture(&lua, &actor).unwrap();
        let runtime = create_song_runtime_table(&lua, &SongLuaCompileContext::new("", "")).unwrap();
        lua.globals().set(SONG_LUA_RUNTIME_KEY, runtime).unwrap();
        set_compile_song_runtime_values(&lua, 2.0, 3.0).unwrap();

        let captured_actor = actor.clone();
        let function = lua
            .create_function(move |lua, value: f32| {
                let (beat, seconds) = compile_song_runtime_values(lua)?;
                capture_block_set_f32(lua, &captured_actor, "x", value)?;
                capture_block_set_f32(lua, &captured_actor, "y", beat + seconds)?;
                captured_actor.set("__songlua_visible", false)?;
                Ok(())
            })
            .unwrap();
        let actors = vec![(5, actor.clone())];

        let blocks =
            capture_indexed_actor_function_blocks(&lua, &actors, &function, Some(12.0), Some(9.0))
                .unwrap();

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].0, 5);
        assert_eq!(blocks[0].1[0].delta.x, Some(12.0));
        assert_eq!(blocks[0].1[0].delta.y, Some(18.0));
        assert_eq!(actor.get::<bool>("__songlua_visible").unwrap(), true);
        assert_eq!(compile_song_runtime_values(&lua).unwrap(), (2.0, 3.0));
    }

    #[test]
    fn capture_overlay_function_eases_builds_probed_overlay_ease() {
        let lua = Lua::new();
        let first = lua.create_table().unwrap();
        let second = lua.create_table().unwrap();
        second.set("__songlua_visible", true).unwrap();
        let runtime = create_song_runtime_table(&lua, &SongLuaCompileContext::new("", "")).unwrap();
        lua.globals().set(SONG_LUA_RUNTIME_KEY, runtime).unwrap();
        set_compile_song_runtime_values(&lua, 2.0, 3.0).unwrap();
        let probed_ptr = second.to_pointer() as usize;

        let captured_actor = second.clone();
        let function = lua
            .create_function(move |lua, value: f32| {
                capture_block_set_f32(lua, &captured_actor, "x", value)?;
                captured_actor.set("__songlua_visible", false)?;
                Ok(())
            })
            .unwrap();
        let overlays = vec![(0, first), (1, second.clone())];

        let eases = capture_overlay_function_eases(
            &lua,
            &overlays,
            &function,
            SongLuaTimeUnit::Beat,
            4.0,
            2.0,
            SongLuaSpanMode::Len,
            2.0,
            6.0,
            None,
            None,
            None,
            None,
            &[probed_ptr],
        )
        .unwrap();

        assert_eq!(eases.len(), 1);
        assert_eq!(eases[0].overlay_index, 1);
        assert_eq!(eases[0].from.x, Some(2.0));
        assert_eq!(eases[0].to.x, Some(6.0));
        assert_eq!(second.get::<bool>("__songlua_visible").unwrap(), true);
        assert_eq!(compile_song_runtime_values(&lua).unwrap(), (2.0, 3.0));
    }

    #[test]
    fn capture_function_action_blocks_collects_captures_and_broadcasts() {
        let lua = Lua::new();
        let actor = lua.create_table().unwrap();
        actor.set("__songlua_visible", true).unwrap();
        let runtime = create_song_runtime_table(&lua, &SongLuaCompileContext::new("", "")).unwrap();
        lua.globals().set(SONG_LUA_RUNTIME_KEY, runtime).unwrap();
        set_compile_song_runtime_values(&lua, 2.0, 3.0).unwrap();

        let captured_actor = actor.clone();
        let function = lua
            .create_function(move |lua, ()| {
                capture_block_set_f32(lua, &captured_actor, "x", 5.0)?;
                captured_actor.set("__songlua_visible", false)?;
                record_song_lua_broadcast(lua, "Hit", false)?;
                note_song_lua_side_effect(lua)?;
                Ok(())
            })
            .unwrap();
        let overlays = vec![(4, actor.clone())];

        let capture = capture_function_action_blocks(&lua, &overlays, &[], &function, 8.0).unwrap();

        assert_eq!(capture.overlay_blocks.len(), 1);
        assert_eq!(capture.overlay_blocks[0].0, 4);
        assert_eq!(capture.overlay_blocks[0].1[0].delta.x, Some(5.0));
        assert!(capture.tracked_blocks.is_empty());
        assert_eq!(capture.broadcasts, vec![("Hit".to_string(), false)]);
        assert!(capture.saw_side_effect);
        assert_eq!(actor.get::<bool>("__songlua_visible").unwrap(), true);
        assert_eq!(compile_song_runtime_values(&lua).unwrap(), (2.0, 3.0));
    }

    #[test]
    fn capture_actor_message_commands_reads_blocks_and_skips_failures() {
        let lua = Lua::new();
        let actor = lua.create_table().unwrap();
        actor.set("Name", "Sample").unwrap();
        reset_actor_capture(&lua, &actor).unwrap();
        actor
            .set(
                "HitMessageCommand",
                lua.create_function(|lua, actor: Table| {
                    capture_block_set_f32(lua, &actor, "x", 9.0)?;
                    Ok(())
                })
                .unwrap(),
            )
            .unwrap();
        actor
            .set(
                "FailMessageCommand",
                lua.create_function(|_, _: Table| -> mlua::Result<()> {
                    Err(mlua::Error::RuntimeError("boom".to_string()))
                })
                .unwrap(),
            )
            .unwrap();

        let captured = capture_actor_message_commands(&lua, &actor).unwrap();

        assert_eq!(captured.commands.len(), 1);
        assert_eq!(captured.commands[0].message, "Hit");
        assert_eq!(captured.commands[0].blocks[0].delta.x, Some(9.0));
        assert_eq!(captured.skipped.len(), 1);
        assert!(captured.skipped[0].contains("FailMessageCommand"));
        assert!(captured.skipped[0].contains("boom"));
    }

    #[test]
    fn capture_actor_message_commands_extracts_startup_sound_blocks() {
        let lua = Lua::new();
        let actor = lua.create_table().unwrap();
        reset_actor_capture(&lua, &actor).unwrap();
        capture_block_set_bool(&lua, &actor, "sound_play", true).unwrap();

        let captured = capture_actor_message_commands(&lua, &actor).unwrap();

        assert_eq!(captured.commands.len(), 1);
        assert_eq!(captured.commands[0].message, SONG_LUA_STARTUP_MESSAGE);
        assert_eq!(captured.commands[0].blocks[0].delta.sound_play, Some(true));
        assert!(captured.skipped.is_empty());
    }

    #[test]
    fn runtime_static_overlay_index_by_path_matches_static_texture_name() {
        let paths = [
            PathBuf::from("sprites/normal.png"),
            PathBuf::from("sprites/_STATIC 4x1.PNG"),
            PathBuf::from("sprites/other.png"),
        ];

        assert_eq!(
            runtime_static_overlay_index_by_path(paths.len(), |index| Some(paths[index].as_path())),
            Some(1)
        );
        assert_eq!(
            runtime_static_overlay_index_by_path(paths.len(), |index| (index == 0)
                .then_some(paths[index].as_path())),
            None
        );
    }

    #[test]
    fn overlay_model_layer_new_preserves_fields() {
        let draw = SongLuaOverlayModelDraw::new(
            [1.0, 2.0, 3.0],
            [4.0, 5.0, 6.0],
            [7.0, 8.0, 9.0],
            [0.1, 0.2, 0.3, 0.4],
            0.5,
            true,
            false,
        );
        let layer = SongLuaOverlayModelLayer::new(
            Arc::from("model.png"),
            Arc::from([11_u32, 12_u32]),
            [13.0, 14.0],
            [15.0, 16.0],
            [17.0, 18.0],
            [19.0, 20.0],
            [21.0, 22.0],
            Some(23.0),
            draw,
        );

        assert_eq!(layer.texture_key.as_ref(), "model.png");
        assert_eq!(layer.vertices.as_ref(), &[11, 12]);
        assert_eq!(layer.model_size, [13.0, 14.0]);
        assert_eq!(layer.uv_scale, [15.0, 16.0]);
        assert_eq!(layer.uv_offset, [17.0, 18.0]);
        assert_eq!(layer.uv_tex_shift, [19.0, 20.0]);
        assert_eq!(layer.uv_velocity, [21.0, 22.0]);
        assert_eq!(layer.uv_cycle_seconds, Some(23.0));
        assert_eq!(layer.draw, draw);
    }

    #[test]
    fn ensure_overlay_arrow_visual_adds_missing_child_visual() {
        fn dummy_actor(lua: &Lua, actor_type: &'static str) -> mlua::Result<Table> {
            let table = lua.create_table()?;
            table.set("__songlua_type", actor_type)?;
            Ok(table)
        }

        let lua = Lua::new();
        let root = dummy_actor(&lua, "ActorFrame").unwrap();
        let mut overlays = vec![SongLuaOverlayCompileActor {
            table: root,
            actor: SongLuaOverlayActor {
                kind: SongLuaOverlayKind::<(), (), ()>::ActorFrame,
                name: None,
                parent_index: None,
                initial_state: SongLuaOverlayState::default(),
                message_commands: Vec::new(),
            },
        }];

        ensure_overlay_arrow_visual(&lua, &mut overlays, 0, "default", dummy_actor, |_| {
            Some((
                SongLuaOverlayKind::Sprite {
                    texture_path: PathBuf::from("arrow.png"),
                    texture_key: Arc::from("arrow.png"),
                },
                SongLuaOverlayState::default(),
            ))
        })
        .unwrap();

        assert_eq!(overlays.len(), 2);
        assert_eq!(overlays[1].actor.parent_index, Some(0));
        assert!(matches!(
            overlays[1].actor.kind,
            SongLuaOverlayKind::Sprite { .. }
        ));

        ensure_overlay_arrow_visual(&lua, &mut overlays, 0, "default", dummy_actor, |_| {
            panic!("visual spec should not run when the arrow tree already has a visual")
        })
        .unwrap();
        assert_eq!(overlays.len(), 2);
    }

    #[test]
    fn message_command_lists_have_listener_matches_command_name() {
        let first = vec![SongLuaOverlayMessageCommand {
            message: "Alpha".to_string(),
            blocks: Vec::new(),
        }];
        let second = vec![SongLuaOverlayMessageCommand {
            message: "Beta".to_string(),
            blocks: Vec::new(),
        }];

        assert!(message_command_lists_have_listener(
            [first.as_slice(), second.as_slice()],
            "Beta"
        ));
        assert!(!message_command_lists_have_listener(
            [first.as_slice(), second.as_slice()],
            "Gamma"
        ));
    }

    #[test]
    fn push_startup_message_if_listened_adds_zero_beat_event() {
        let commands = vec![SongLuaOverlayMessageCommand {
            message: SONG_LUA_STARTUP_MESSAGE.to_string(),
            blocks: Vec::new(),
        }];
        let mut messages = Vec::new();

        push_startup_message_if_listened(&mut messages, [commands.as_slice()]);

        assert_eq!(
            messages,
            vec![SongLuaMessageEvent {
                beat: 0.0,
                message: SONG_LUA_STARTUP_MESSAGE.to_string(),
                persists: false,
            }]
        );
    }

    #[test]
    fn sort_compiled_song_lua_orders_timeline_dtos() {
        let mut compiled = CompiledSongLua::<()>::default();
        compiled.beat_mods = vec![
            mod_window(4.0, 1.0, "b"),
            mod_window(1.0, 2.0, "a"),
            mod_window(1.0, 1.0, "c"),
        ];
        compiled.time_mods = vec![mod_window(2.0, 1.0, "x"), mod_window(1.0, 1.0, "y")];
        compiled.eases = vec![ease_window(3.0, 1.0), ease_window(2.0, 2.0)];
        compiled.overlay_eases = vec![
            overlay_ease(2, 1.0, 1.0),
            overlay_ease(1, 1.0, 1.0),
            overlay_ease(0, 1.0, 0.5),
        ];
        compiled.messages = vec![
            SongLuaMessageEvent {
                beat: 2.0,
                message: "B".to_string(),
                persists: false,
            },
            SongLuaMessageEvent {
                beat: 1.0,
                message: "A".to_string(),
                persists: false,
            },
        ];

        sort_compiled_song_lua(&mut compiled);

        assert_eq!(
            compiled
                .beat_mods
                .iter()
                .map(|window| window.mods.as_str())
                .collect::<Vec<_>>(),
            vec!["c", "a", "b"]
        );
        assert_eq!(
            compiled
                .time_mods
                .iter()
                .map(|window| window.mods.as_str())
                .collect::<Vec<_>>(),
            vec!["y", "x"]
        );
        assert_eq!(
            compiled
                .eases
                .iter()
                .map(|window| window.start)
                .collect::<Vec<_>>(),
            vec![2.0, 3.0]
        );
        assert_eq!(
            compiled
                .overlay_eases
                .iter()
                .map(|window| window.overlay_index)
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert_eq!(
            compiled
                .messages
                .iter()
                .map(|event| event.beat)
                .collect::<Vec<_>>(),
            vec![1.0, 2.0]
        );
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
    fn scale_to_rect_plan_matches_actor_scale_policy() {
        let fit = scale_to_rect_plan([10.0, 20.0, 210.0, 120.0], [50.0, 100.0], [0.5, 0.0], false)
            .unwrap();
        assert_eq!(fit.pos, [110.0, 20.0]);
        assert_eq!(fit.zoom, 1.0);
        assert!(!fit.flip_x);
        assert!(!fit.flip_y);

        let cover = scale_to_rect_plan([10.0, 20.0, 210.0, 120.0], [50.0, 100.0], [0.0, 0.5], true)
            .unwrap();
        assert_eq!(cover.pos, [10.0, 70.0]);
        assert_eq!(cover.zoom, 4.0);

        let flipped =
            scale_to_rect_plan([210.0, 120.0, 10.0, 20.0], [50.0, 100.0], [0.5, 0.5], false)
                .unwrap();
        assert_eq!(flipped.pos, [110.0, 70.0]);
        assert!(flipped.flip_x);
        assert!(flipped.flip_y);

        assert!(scale_to_rect_plan([0.0, 0.0, 1.0, 1.0], [0.0, 1.0], [0.0, 0.0], false).is_none());
    }

    #[test]
    fn overlay_state_scale_uses_axis_fallbacks() {
        let state = SongLuaOverlayState {
            basezoom: 2.0,
            basezoom_x: 1.0,
            basezoom_y: 3.0,
            basezoom_z: 1.0,
            zoom: 4.0,
            zoom_x: 5.0,
            zoom_y: 1.0,
            zoom_z: 6.0,
            ..SongLuaOverlayState::default()
        };
        assert_eq!(overlay_state_axis_scale(state), [10.0, 12.0]);
        assert_eq!(overlay_state_z_scale(state), 12.0);
    }

    #[test]
    fn texture_rect_offsets_preserve_actor_host_math() {
        assert_eq!(
            offset_texture_rect([0.25, 0.5, 0.75, 1.0], [0.1, -0.2]),
            [0.35, 0.3, 0.85, 0.8]
        );
        assert_eq!(
            texture_pixel_offset_rect([0.0, 0.0, 0.5, 0.5], [200.0, 100.0], [10.0, 5.0]),
            Some([0.05, 0.05, 0.55, 0.55])
        );
        assert_eq!(
            texture_pixel_offset_rect([0.0, 0.0, 1.0, 1.0], [0.0, 100.0], [10.0, 5.0]),
            None
        );
    }

    #[test]
    fn sprite_texture_rect_prefers_custom_then_state_sheet() {
        assert_eq!(
            sprite_texture_rect(Some([0.1, 0.2, 0.3, 0.4]), Some(2), Some((4, 2))),
            [0.1, 0.2, 0.3, 0.4]
        );
        assert_eq!(
            sprite_texture_rect(None, Some(5), Some((4, 2))),
            [0.25, 0.5, 0.5, 1.0]
        );
        assert_eq!(
            sprite_texture_rect(None, Some(SONG_LUA_SPRITE_STATE_CLEAR), Some((4, 2))),
            [0.0, 0.0, 1.0, 1.0]
        );
        assert_eq!(
            sprite_texture_rect(None, Some(1), None),
            [0.0, 0.0, 1.0, 1.0]
        );
    }

    #[test]
    fn sprite_texture_rect_with_offset_preserves_optional_policy() {
        assert_eq!(
            sprite_texture_rect_with_offset(None, None, Some((4, 2)), None),
            None
        );
        assert_eq!(
            sprite_texture_rect_with_offset(None, None, Some((4, 2)), Some([0.25, -0.5])),
            Some([0.25, -0.5, 1.25, 0.5])
        );
        assert_eq!(
            sprite_texture_rect_with_offset(None, Some(5), Some((4, 2)), Some([0.25, -0.5])),
            Some([0.5, 0.0, 0.75, 0.5])
        );
        assert_eq!(
            sprite_texture_rect_with_offset(
                Some([0.1, 0.2, 0.3, 0.4]),
                Some(5),
                Some((4, 2)),
                Some([0.25, -0.5]),
            ),
            Some([0.1 + 0.25, 0.2 - 0.5, 0.3 + 0.25, 0.4 - 0.5])
        );
    }

    #[test]
    fn sprite_frame_count_saturates_empty_dims() {
        assert_eq!(sprite_frame_count(None), 1);
        assert_eq!(sprite_frame_count(Some((0, 0))), 1);
        assert_eq!(sprite_frame_count(Some((4, 3))), 12);
    }

    #[test]
    fn sprite_image_frame_size_divides_animated_or_stateful_sheets() {
        assert_eq!(
            sprite_image_frame_size(Some((256.0, 128.0)), false, None, Some((4, 2))),
            Some((256.0, 128.0))
        );
        assert_eq!(
            sprite_image_frame_size(Some((256.0, 128.0)), true, None, Some((4, 2))),
            Some((64.0, 64.0))
        );
        assert_eq!(
            sprite_image_frame_size(Some((256.0, 128.0)), false, Some(3), Some((4, 2))),
            Some((64.0, 64.0))
        );
        assert_eq!(
            sprite_image_frame_size(
                Some((256.0, 128.0)),
                false,
                Some(SONG_LUA_SPRITE_STATE_CLEAR),
                Some((4, 2)),
            ),
            Some((256.0, 128.0))
        );
        assert_eq!(
            sprite_image_frame_size(Some((256.0, 128.0)), true, None, Some((0, 0))),
            Some((256.0, 128.0))
        );
        assert_eq!(
            sprite_image_frame_size(None, true, Some(3), Some((4, 2))),
            None
        );
    }

    #[test]
    fn sprite_animation_state_at_matches_stepmania_timing() {
        assert_eq!(sprite_animation_state_at(-1.0, 0.1, 4), 0);
        assert_eq!(sprite_animation_state_at(0.39, 0.1, 4), 3);
        assert_eq!(sprite_animation_state_at(0.4, 0.1, 4), 0);
        assert_eq!(sprite_animation_state_at(5.0, 0.0, 4), 0);
        assert_eq!(sprite_animation_state_at(5.0, -1.0, 4), 0);
        assert_eq!(sprite_animation_state_at(5.0, 0.1, 0), 0);
    }

    #[test]
    fn sprite_animation_state_from_offsets_and_clamps() {
        assert_eq!(sprite_animation_state_from(2, 0.29, 1.0, 0.1, 4, true), 0);
        assert_eq!(sprite_animation_state_from(2, 0.29, 1.0, 0.1, 4, false), 3);
        assert_eq!(sprite_animation_state_from(2, -0.29, 1.0, 0.1, 4, false), 0);
        assert_eq!(sprite_animation_state_from(7, 0.29, 1.0, 0.0, 4, true), 3);
    }

    #[test]
    fn note_column_zoom_hide_policy_filters_unsupported_handlers() {
        assert_eq!(
            note_column_zoom_hide_beats_per_t("NoteColumnSplineMode_Offset", false, 0.5),
            Some(0.5)
        );
        assert_eq!(
            note_column_zoom_hide_beats_per_t("NoteColumnSplineMode_Disabled", false, 0.5),
            None
        );
        assert_eq!(
            note_column_zoom_hide_beats_per_t("NoteColumnSplineMode_Offset", true, 0.5),
            None
        );
        assert_eq!(
            note_column_zoom_hide_beats_per_t("NoteColumnSplineMode_Offset", false, 0.0),
            None
        );
        assert_eq!(
            note_column_zoom_hide_beats_per_t("NoteColumnSplineMode_Offset", false, f32::NAN),
            None
        );
    }

    #[test]
    fn sort_note_hide_windows_matches_actor_host_order() {
        let mut windows = vec![
            SongLuaNoteHideWindow {
                player: 1,
                column: 0,
                start_beat: 1.0,
                end_beat: 1.5,
            },
            SongLuaNoteHideWindow {
                player: 0,
                column: 1,
                start_beat: 2.0,
                end_beat: 2.5,
            },
            SongLuaNoteHideWindow {
                player: 0,
                column: 1,
                start_beat: 1.0,
                end_beat: 2.0,
            },
            SongLuaNoteHideWindow {
                player: 0,
                column: 0,
                start_beat: 4.0,
                end_beat: 4.5,
            },
        ];
        sort_note_hide_windows(&mut windows);
        assert_eq!(
            windows
                .iter()
                .map(|window| window.player)
                .collect::<Vec<_>>(),
            vec![0, 0, 0, 1]
        );
        assert_eq!(
            windows
                .iter()
                .map(|window| (window.column, window.start_beat))
                .collect::<Vec<_>>(),
            vec![(0, 4.0), (1, 1.0), (1, 2.0), (0, 1.0)]
        );
    }

    #[test]
    fn note_column_pos_offset_y_from_points_matches_actor_host_policy() {
        assert_eq!(
            note_column_pos_offset_y_from_points("NoteColumnSplineMode_Disabled", &[]),
            Some(0.0)
        );
        assert_eq!(
            note_column_pos_offset_y_from_points("NoteColumnSplineMode_Rotation", &[[0.0, 2.0]]),
            None
        );
        assert_eq!(
            note_column_pos_offset_y_from_points("NoteColumnSplineMode_Offset", &[]),
            Some(0.0)
        );
        assert_eq!(
            note_column_pos_offset_y_from_points(
                "NoteColumnSplineMode_Offset",
                &[[0.0, 2.0], [0.0005, 2.0005]],
            ),
            Some(2.0)
        );
        assert_eq!(
            note_column_pos_offset_y_from_points(
                "NoteColumnSplineMode_Offset",
                &[[0.0, 2.0], [0.0, 2.01]],
            ),
            None
        );
        assert_eq!(
            note_column_pos_offset_y_from_points("NoteColumnSplineMode_Offset", &[[0.01, 2.0]]),
            None
        );
    }

    #[test]
    fn column_offset_windows_from_samples_match_capture_policy() {
        let params = SongLuaColumnOffsetBuildParams {
            unit: SongLuaTimeUnit::Beat,
            start: 4.0,
            limit: 8.0,
            span_mode: SongLuaSpanMode::End,
            easing: Some("linear".to_string()),
            sustain: Some(1.0),
            opt1: Some(2.0),
            opt2: Some(3.0),
        };
        let from = [
            SongLuaColumnOffsetSample {
                player: 1,
                column: 2,
                y: 12.0,
            },
            SongLuaColumnOffsetSample {
                player: 0,
                column: 1,
                y: 0.0,
            },
            SongLuaColumnOffsetSample {
                player: 0,
                column: 0,
                y: 4.0,
            },
        ];
        let to = [
            SongLuaColumnOffsetSample {
                player: 0,
                column: 1,
                y: 0.0,
            },
            SongLuaColumnOffsetSample {
                player: 0,
                column: 0,
                y: 8.0,
            },
            SongLuaColumnOffsetSample {
                player: 1,
                column: 0,
                y: -2.0,
            },
        ];

        let out = column_offset_windows_from_samples(&from, &to, params);

        assert_eq!(out.len(), 3);
        assert_eq!(
            (out[0].player, out[0].column, out[0].from_y, out[0].to_y),
            (0, 0, 4.0, 8.0)
        );
        assert_eq!(
            (out[1].player, out[1].column, out[1].from_y, out[1].to_y),
            (1, 0, 0.0, -2.0)
        );
        assert_eq!(
            (out[2].player, out[2].column, out[2].from_y, out[2].to_y),
            (1, 2, 12.0, 0.0)
        );
        assert_eq!(out[0].easing.as_deref(), Some("linear"));
        assert_eq!(out[0].sustain, Some(1.0));
        assert_eq!(out[0].opt1, Some(2.0));
        assert_eq!(out[0].opt2, Some(3.0));
    }

    #[test]
    fn note_hide_windows_from_flags_preserve_spline_run_policy() {
        let out =
            note_hide_windows_from_flags(1, 2, 0.5, &[false, true, true, false, true, true, true]);

        assert_eq!(out.len(), 2);
        assert_eq!((out[0].player, out[0].column), (1, 2));
        assert_eq!((out[0].start_beat, out[0].end_beat), (0.5, 1.0));
        assert_eq!((out[1].start_beat, out[1].end_beat), (2.0, 3.0));
    }

    #[test]
    fn note_hide_window_index_policy_filters_invalid_ranges() {
        assert_eq!(note_hide_window_from_indices(0, 0, 1.0, 0, 1), None);
        assert_eq!(note_hide_window_from_indices(0, 0, 1.0, 3, 2), None);
        assert_eq!(
            note_hide_window_from_indices(0, 1, 0.25, 1, 1),
            Some(super::SongLuaNoteHideWindow {
                player: 0,
                column: 1,
                start_beat: 0.0,
                end_beat: 0.0,
            })
        );
        assert!(note_hide_windows_from_flags(0, 0, 0.0, &[true]).is_empty());
        assert!(note_hide_windows_from_flags(0, 0, f32::NAN, &[true]).is_empty());
    }

    #[test]
    fn overlay_eases_from_captures_intersects_common_delta_fields() {
        let params = SongLuaOverlayEaseBuildParams {
            unit: SongLuaTimeUnit::Beat,
            start: 2.0,
            limit: 4.0,
            span_mode: SongLuaSpanMode::Len,
            easing: Some("linear".to_string()),
            sustain: Some(0.5),
            opt1: Some(1.0),
            opt2: Some(2.0),
        };
        let block = |delta: SongLuaOverlayStateDelta| SongLuaOverlayCommandBlock {
            start: 0.0,
            duration: 0.0,
            easing: None,
            opt1: None,
            opt2: None,
            delta,
        };
        let from = vec![
            (
                2,
                vec![block(SongLuaOverlayStateDelta {
                    x: Some(10.0),
                    y: Some(20.0),
                    ..SongLuaOverlayStateDelta::default()
                })],
            ),
            (
                0,
                vec![block(SongLuaOverlayStateDelta {
                    x: Some(1.0),
                    ..SongLuaOverlayStateDelta::default()
                })],
            ),
        ];
        let to = vec![
            (
                2,
                vec![block(SongLuaOverlayStateDelta {
                    x: Some(30.0),
                    ..SongLuaOverlayStateDelta::default()
                })],
            ),
            (
                1,
                vec![block(SongLuaOverlayStateDelta {
                    x: Some(99.0),
                    ..SongLuaOverlayStateDelta::default()
                })],
            ),
        ];

        let out = overlay_eases_from_captures(3, &from, &to, params);

        assert_eq!(out.len(), 1);
        assert_eq!(out[0].overlay_index, 2);
        assert_eq!(out[0].from.x, Some(10.0));
        assert_eq!(out[0].from.y, None);
        assert_eq!(out[0].to.x, Some(30.0));
        assert_eq!(out[0].easing.as_deref(), Some("linear"));
        assert_eq!(out[0].sustain, Some(0.5));
        assert_eq!(out[0].opt1, Some(1.0));
        assert_eq!(out[0].opt2, Some(2.0));
    }

    #[test]
    fn parse_overlay_blend_mode_accepts_stepmania_add_name() {
        assert_eq!(
            parse_overlay_blend_mode("BlendMode_Add"),
            Some(SongLuaOverlayBlendMode::Add)
        );
        assert_eq!(
            parse_overlay_blend_mode("BlendMode_Multiply"),
            Some(SongLuaOverlayBlendMode::Multiply)
        );
        assert_eq!(
            parse_overlay_blend_mode("BlendMode_Subtract"),
            Some(SongLuaOverlayBlendMode::Subtract)
        );
    }

    #[test]
    fn parse_overlay_effect_mode_accepts_song_lua_effect_names() {
        assert_eq!(
            parse_overlay_effect_mode("DiffuseRamp"),
            Some(EffectMode::DiffuseRamp)
        );
        assert_eq!(
            parse_overlay_effect_mode("glowshift"),
            Some(EffectMode::GlowShift)
        );
        assert_eq!(
            parse_overlay_effect_mode("bounce"),
            Some(EffectMode::Bounce)
        );
        assert_eq!(parse_overlay_effect_mode("wag"), Some(EffectMode::Wag));
    }

    #[test]
    fn parse_overlay_effect_clock_accepts_music_and_bgm_aliases() {
        assert_eq!(parse_overlay_effect_clock("beat"), Some(EffectClock::Beat));
        assert_eq!(parse_overlay_effect_clock("bgm"), Some(EffectClock::Beat));
        assert_eq!(parse_overlay_effect_clock("music"), Some(EffectClock::Time));
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
