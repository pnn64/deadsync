use chrono::{Datelike, Local, Timelike};
use image::image_dimensions;
use log::{debug, info};
use mlua::{Function, Lua, MultiValue, Table, Value};
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use crate::engine::present::actors::{TextAlign, TextAttribute};
use crate::engine::present::anim::{EffectClock, EffectMode};

mod actor_host;
mod compat;
mod managers;
mod overlay;
mod runtime;
mod sl;
mod song_tables;
mod theme_colors;
mod types;
mod util;

use self::actor_host::{
    actor_overlay_initial_state, actor_tree_has_update_functions, broadcast_song_lua_message,
    compile_function_action, compile_overlay_function_ease, create_arrow_effects_table,
    create_top_screen_table, execute_script_file, install_def, install_file_loaders,
    probe_function_ease_target, read_note_column_zoom_hides, read_overlay_actors,
    read_tracked_compile_actors, read_update_function_actions, read_update_function_tables,
    reset_overlay_capture_tables, reset_tracked_capture_tables, run_actor_draw_functions,
    run_actor_init_commands, run_actor_startup_commands, run_actor_update_functions,
    run_actor_update_functions_with_delta,
};
use self::compat::{
    create_chunk_env_proxy, create_gameplay_layout, initial_chunk_environment,
    install_stdlib_compat,
};
use self::managers::{
    create_charman_table, create_conf_option_row, create_custom_option_row, create_display_table,
    create_gameman_table, create_hooks_table, create_life_record_table, create_memcardman_table,
    create_noteskin_table, create_operator_menu_option_rows_table, create_profileman_table,
    create_sl_custom_prefs_table, create_sound_table, create_statsman_table,
    create_theme_prefs_rows_table, create_theme_prefs_table, create_theme_table,
    create_unlockman_table, custom_option_default_text, graph_display_body_size,
    song_lua_human_player_count, theme_metric_number, theme_path, theme_string,
};
pub use self::overlay::{
    SongLuaOverlayActor, SongLuaOverlayBlendMode, SongLuaOverlayCommandBlock, SongLuaOverlayEase,
    SongLuaOverlayKind, SongLuaOverlayMeshVertex, SongLuaOverlayMessageCommand,
    SongLuaOverlayModelDraw, SongLuaOverlayModelLayer, SongLuaOverlayState,
    SongLuaOverlayStateDelta, SongLuaProxyTarget, SongLuaTextGlowMode,
};
use self::overlay::{
    overlay_delta_from_blocks, overlay_delta_intersection, overlay_state_after_blocks,
    parse_overlay_blend_mode, parse_overlay_effect_clock, parse_overlay_effect_mode,
    parse_overlay_text_align, parse_overlay_text_glow_mode,
};
use self::runtime::{
    SONG_LUA_BROADCASTS_KEY, SONG_LUA_RUNTIME_BEAT_KEY, SONG_LUA_RUNTIME_KEY,
    SONG_LUA_RUNTIME_SECONDS_KEY, SONG_LUA_SIDE_EFFECT_COUNT_KEY,
    compile_song_runtime_delta_values, compile_song_runtime_values, create_song_position_table,
    create_song_runtime_table, note_song_lua_side_effect, read_song_lua_broadcasts,
    record_song_lua_broadcast, set_compile_song_runtime_beat,
    set_compile_song_runtime_delta_values, set_compile_song_runtime_values, song_display_bps,
    song_elapsed_seconds_for_beat, song_lua_runtime_number, song_lua_side_effect_count,
    song_music_rate,
};
use self::sl::{create_sl_table, player_short_name};
use self::song_tables::{
    create_course_table, create_display_bpms_table, create_enabled_players_table,
    create_player_tables, create_song_options_table, create_song_table, create_songman_table,
    create_style_table, create_trail_table, display_bpms_for_args, display_bpms_text,
    format_song_options_text, set_string_method,
};
pub use self::types::{
    CompiledSongLua, SongLuaCapturedActor, SongLuaCompileContext, SongLuaCompileInfo,
    SongLuaDifficulty, SongLuaEaseTarget, SongLuaEaseWindow, SongLuaMessageEvent, SongLuaModWindow,
    SongLuaNoteHideWindow, SongLuaPlayerContext, SongLuaSpanMode, SongLuaSpeedMod, SongLuaTimeUnit,
};
use self::util::{
    SONG_LUA_EASING_NAME_KEY, create_owned_string_array, create_string_array, ease_window_cmp,
    file_path_string, is_song_lua_audio_path, is_song_lua_image_path, is_song_lua_media_path,
    is_song_lua_video_path, lua_format_text, lua_text_value, lua_values_equal, make_color_table,
    message_event_cmp, method_arg, mod_window_cmp, player_index_from_value, player_number_name,
    preprocess_lua_cmd_syntax, read_boolish, read_color_value, read_easing_name, read_f32,
    read_i32_value, read_player, read_song_lua_sound_paths, read_span_mode, read_string,
    read_u32_value, read_vertex_colors_value, song_dir_string, song_group_name, song_music_path,
    song_named_image_path, song_simfile_path, truthy,
};

const LUA_PLAYERS: usize = 2;
const SONG_LUA_NOTE_COLUMNS: usize = 4;
const SONG_LUA_DOUBLE_NOTE_COLUMNS: usize = 8;
const SONG_LUA_PRODUCT_FAMILY: &str = "ITGmania";
const SONG_LUA_PRODUCT_ID: &str = "ITGmania";
const SONG_LUA_PRODUCT_VERSION: &str = "1.2.0";
const THEME_RECEPTOR_Y_STD: f32 = -125.0;
const THEME_RECEPTOR_Y_REV: f32 = 145.0;
const SONG_LUA_COLUMN_X: [f32; SONG_LUA_NOTE_COLUMNS] = [-96.0, -32.0, 32.0, 96.0];
const SONG_LUA_DOUBLE_COLUMN_X: [f32; SONG_LUA_DOUBLE_NOTE_COLUMNS] =
    [-224.0, -160.0, -96.0, -32.0, 32.0, 96.0, 160.0, 224.0];
const SONG_LUA_COLUMN_NAMES: [&str; SONG_LUA_NOTE_COLUMNS] = ["Left", "Down", "Up", "Right"];
const SONG_LUA_SOUND_PATHS_KEY: &str = "__songlua_sound_paths";
const SONG_LUA_PROBE_METHODS_KEY: &str = "__songlua_probe_methods";
const SONG_LUA_PROBE_ACTORS_KEY: &str = "__songlua_probe_actors";
const SONG_LUA_PROBE_ACTOR_SET_KEY: &str = "__songlua_probe_actor_set";
const SONG_LUA_CAPTURE_ACTORS_KEY: &str = "__songlua_capture_scope_actors";
const SONG_LUA_CAPTURE_ACTOR_SET_KEY: &str = "__songlua_capture_scope_actor_set";
const SONG_LUA_CAPTURE_SNAPSHOTS_KEY: &str = "__songlua_capture_scope_snapshots";
const SONG_LUA_STARTUP_MESSAGE: &str = "__songlua_startup";
const SONG_LUA_THEME_PATH_PREFIX: &str = "__songlua_theme_path/";
const SONG_LUA_THEME_NAME: &str = "Simply Love";
const SONG_LUA_INITIAL_LIFE: f32 = 0.5;
const SONG_LUA_DANGER_LIFE: f32 = 0.2;
const GRAPH_DISPLAY_VALUE_RESOLUTION: usize = 100;
const SONG_LUA_SPRITE_STATE_CLEAR: u32 = u32::MAX;
const SONG_LUA_UPDATE_FUNCTION_MAX_SAMPLES: usize = 4096;
const MULTITAP_PREVISIBLE_BEATS: f32 = 8.0;
const MULTITAP_BASE_BOUNCE: f32 = 1.5;
const MULTITAP_ELASTICITY: f32 = 1.05;
const MULTITAP_SQUISHY: f32 = 0.2;
const MULTITAP_SAMPLE_STEP: f32 = 0.125;
const MULTITAP_LANE_ROTATION: [f32; SONG_LUA_DOUBLE_NOTE_COLUMNS] =
    [90.0, 0.0, 180.0, 270.0, 90.0, 0.0, 180.0, 270.0];
const EASING_NAMES: &[&str] = &[
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
const SONG_LUA_TIMING_WINDOW_NAMES: [&str; 5] = [
    "TimingWindow_W1",
    "TimingWindow_W2",
    "TimingWindow_W3",
    "TimingWindow_W4",
    "TimingWindow_W5",
];

#[derive(Clone, Copy)]
struct SongLuaStyleInfo {
    name: &'static str,
    steps_type: &'static str,
    style_type: &'static str,
    columns: usize,
    width: f32,
    x_offsets: &'static [f32],
}

fn song_lua_style_info(style_name: &str) -> SongLuaStyleInfo {
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
fn song_lua_style_column_x(style_name: &str, column_index: usize) -> f32 {
    song_lua_style_info(style_name)
        .x_offsets
        .get(column_index)
        .copied()
        .unwrap_or(0.0)
}

#[inline(always)]
fn song_lua_style_column_name(column_index: usize) -> &'static str {
    SONG_LUA_COLUMN_NAMES[column_index % SONG_LUA_COLUMN_NAMES.len()]
}
const SONG_LUA_PLAYER_OPTION_CAPABILITIES: &[&str] = &[
    "FromString",
    "IsEasierForSongAndSteps",
    "IsEasierForCourseAndTrail",
    "LifeSetting",
    "DrainSetting",
    "HideLightSetting",
    "ModTimerSetting",
    "BatteryLives",
    "XMod",
    "CMod",
    "MMod",
    "AMod",
    "CAMod",
    "DrawSize",
    "DrawSizeBack",
    "ModTimerMult",
    "ModTimerOffset",
    "TimeSpacing",
    "MaxScrollBPM",
    "ScrollSpeed",
    "ScrollBPM",
    "Boost",
    "Brake",
    "Wave",
    "WavePeriod",
    "Expand",
    "ExpandPeriod",
    "TanExpand",
    "TanExpandPeriod",
    "Boomerang",
    "Drunk",
    "DrunkSpeed",
    "DrunkOffset",
    "DrunkPeriod",
    "TanDrunk",
    "TanDrunkSpeed",
    "TanDrunkOffset",
    "TanDrunkPeriod",
    "DrunkZ",
    "DrunkZSpeed",
    "DrunkZOffset",
    "DrunkZPeriod",
    "TanDrunkZ",
    "TanDrunkZSpeed",
    "TanDrunkZOffset",
    "TanDrunkZPeriod",
    "Dizzy",
    "AttenuateX",
    "AttenuateY",
    "AttenuateZ",
    "ShrinkLinear",
    "ShrinkMult",
    "PulseInner",
    "PulseOuter",
    "PulsePeriod",
    "PulseOffset",
    "Confusion",
    "ConfusionOffset",
    "ConfusionX",
    "ConfusionXOffset",
    "ConfusionY",
    "ConfusionYOffset",
    "Bounce",
    "BouncePeriod",
    "BounceOffset",
    "BounceZ",
    "BounceZPeriod",
    "BounceZOffset",
    "Mini",
    "Tiny",
    "Flip",
    "Invert",
    "Tornado",
    "TornadoPeriod",
    "TornadoOffset",
    "TanTornado",
    "TanTornadoPeriod",
    "TanTornadoOffset",
    "TornadoZ",
    "TornadoZPeriod",
    "TornadoZOffset",
    "TanTornadoZ",
    "TanTornadoZPeriod",
    "TanTornadoZOffset",
    "Tipsy",
    "TipsySpeed",
    "TipsyOffset",
    "TanTipsy",
    "TanTipsySpeed",
    "TanTipsyOffset",
    "Bumpy",
    "BumpyOffset",
    "BumpyPeriod",
    "TanBumpy",
    "TanBumpyOffset",
    "TanBumpyPeriod",
    "BumpyX",
    "BumpyXOffset",
    "BumpyXPeriod",
    "TanBumpyX",
    "TanBumpyXOffset",
    "TanBumpyXPeriod",
    "Beat",
    "BeatOffset",
    "BeatPeriod",
    "BeatMult",
    "BeatY",
    "BeatYOffset",
    "BeatYPeriod",
    "BeatYMult",
    "BeatZ",
    "BeatZOffset",
    "BeatZPeriod",
    "BeatZMult",
    "Zigzag",
    "ZigzagPeriod",
    "ZigzagOffset",
    "ZigzagZ",
    "ZigzagZPeriod",
    "ZigzagZOffset",
    "Sawtooth",
    "SawtoothPeriod",
    "SawtoothZ",
    "SawtoothZPeriod",
    "Square",
    "SquareOffset",
    "SquarePeriod",
    "SquareZ",
    "SquareZOffset",
    "SquareZPeriod",
    "Digital",
    "DigitalSteps",
    "DigitalPeriod",
    "DigitalOffset",
    "TanDigital",
    "TanDigitalSteps",
    "TanDigitalPeriod",
    "TanDigitalOffset",
    "DigitalZ",
    "DigitalZSteps",
    "DigitalZPeriod",
    "DigitalZOffset",
    "TanDigitalZ",
    "TanDigitalZSteps",
    "TanDigitalZPeriod",
    "TanDigitalZOffset",
    "ParabolaX",
    "ParabolaY",
    "ParabolaZ",
    "Xmode",
    "Twirl",
    "Roll",
    "Hidden",
    "HiddenOffset",
    "Sudden",
    "SuddenOffset",
    "Stealth",
    "Blink",
    "RandomVanish",
    "Reverse",
    "Split",
    "Alternate",
    "Cross",
    "Centered",
    "Dark",
    "Blind",
    "Cover",
    "StealthType",
    "StealthPastReceptors",
    "DizzyHolds",
    "ZBuffer",
    "Cosecant",
    "RandAttack",
    "NoAttack",
    "PlayerAutoPlay",
    "Tilt",
    "Skew",
    "Passmark",
    "RandomSpeed",
    "TurnNone",
    "Mirror",
    "LRMirror",
    "UDMirror",
    "Backwards",
    "Left",
    "Right",
    "Shuffle",
    "SoftShuffle",
    "SuperShuffle",
    "HyperShuffle",
    "NoHolds",
    "NoRolls",
    "NoMines",
    "Little",
    "Wide",
    "Big",
    "Quick",
    "BMRize",
    "Skippy",
    "Mines",
    "AttackMines",
    "Echo",
    "Stomp",
    "Planted",
    "Floored",
    "Twister",
    "HoldRolls",
    "NoJumps",
    "NoHands",
    "NoLifts",
    "NoFakes",
    "NoQuads",
    "NoStretch",
    "MuteOnError",
    "Overhead",
    "Incoming",
    "Space",
    "Hallway",
    "Distant",
    "NoteSkin",
    "FailSetting",
    "MinTNSToHideNotes",
    "VisualDelay",
    "DisableTimingWindow",
    "ResetDisabledTimingWindows",
    "GetDisabledTimingWindows",
    "UsingReverse",
    "GetReversePercentForColumn",
    "GetStepAttacks",
];
const SONG_LUA_PLAYER_OPTION_MULTICOL_PREFIXES: &[&str] = &[
    "MoveX",
    "MoveY",
    "MoveZ",
    "ConfusionOffset",
    "ConfusionXOffset",
    "ConfusionYOffset",
    "Dark",
    "Stealth",
    "Tiny",
    "Bumpy",
    "Reverse",
];

fn song_lua_difficulty_from_value(value: Value) -> Option<SongLuaDifficulty> {
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

fn song_lua_steps_type_is_dance_single(value: Value) -> bool {
    let Some(raw) = read_string(value) else {
        return false;
    };
    let normalized = raw.trim().to_ascii_lowercase().replace(['_', '-', ' '], "");
    matches!(
        normalized.as_str(),
        "stepstypedancesingle" | "dancesingle" | "single"
    )
}

fn easiest_steps_difficulty(
    players: &[SongLuaPlayerContext; LUA_PLAYERS],
) -> Option<SongLuaDifficulty> {
    players
        .iter()
        .filter(|player| player.enabled)
        .map(|player| player.difficulty)
        .min_by_key(|difficulty| difficulty.sort_key())
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct SongLuaPerframePlayerState {
    x: Option<f32>,
    y: Option<f32>,
    z: Option<f32>,
    rotation_x: Option<f32>,
    rotation_z: Option<f32>,
    rotation_y: Option<f32>,
    zoom_x: Option<f32>,
    zoom_y: Option<f32>,
    zoom_z: Option<f32>,
    skew_x: Option<f32>,
    skew_y: Option<f32>,
}

struct SongLuaPerframeEntry {
    start: f32,
    end: f32,
    function: Function,
}

#[derive(Debug, Clone, Copy)]
struct SongLuaActorMultiVertexPoint {
    pos: [f32; 2],
    color: [f32; 4],
    uv: [f32; 2],
}

#[derive(Default)]
struct HostState {
    easing_names: HashMap<*const c_void, String>,
}

struct OverlayCompileActor {
    table: Table,
    actor: SongLuaOverlayActor,
}

#[derive(Clone, Copy)]
enum TrackedCompileActorTarget {
    Player(usize),
    SongForeground,
}

struct TrackedCompileActor {
    table: Table,
    actor: SongLuaCapturedActor,
    target: TrackedCompileActorTarget,
}

struct SongLuaCompileGlobals {
    prefix_globals: Value,
    mods: Value,
    mod_time: Value,
    mods_ease: Value,
    mod_perframes: Value,
    mod_actions: Value,
}

fn clone_lua_value(lua: &Lua, value: Value) -> mlua::Result<Value> {
    match value {
        Value::Table(table) => {
            let cloned = lua.create_table()?;
            for pair in table.pairs::<Value, Value>() {
                let (key, value) = pair?;
                cloned.set(clone_lua_value(lua, key)?, clone_lua_value(lua, value)?)?;
            }
            Ok(Value::Table(cloned))
        }
        other => Ok(other),
    }
}

fn snapshot_compile_globals(lua: &Lua, globals: &Table) -> mlua::Result<SongLuaCompileGlobals> {
    Ok(SongLuaCompileGlobals {
        prefix_globals: clone_lua_value(lua, globals.get::<Value>("prefix_globals")?)?,
        mods: clone_lua_value(lua, globals.get::<Value>("mods")?)?,
        mod_time: clone_lua_value(lua, globals.get::<Value>("mod_time")?)?,
        mods_ease: clone_lua_value(lua, globals.get::<Value>("mods_ease")?)?,
        mod_perframes: clone_lua_value(lua, globals.get::<Value>("mod_perframes")?)?,
        mod_actions: clone_lua_value(lua, globals.get::<Value>("mod_actions")?)?,
    })
}

fn restore_compile_globals(globals: &Table, snapshot: SongLuaCompileGlobals) -> mlua::Result<()> {
    globals.set("prefix_globals", snapshot.prefix_globals)?;
    globals.set("mods", snapshot.mods)?;
    globals.set("mod_time", snapshot.mod_time)?;
    globals.set("mods_ease", snapshot.mods_ease)?;
    globals.set("mod_perframes", snapshot.mod_perframes)?;
    globals.set("mod_actions", snapshot.mod_actions)?;
    Ok(())
}

fn push_unique_compile_detail(out: &mut Vec<String>, detail: String) {
    if !out.contains(&detail) {
        out.push(detail);
    }
}

fn merge_compile_info(out: &mut SongLuaCompileInfo, info: SongLuaCompileInfo) {
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

#[inline(always)]
fn is_compile_global_name(name: &str) -> bool {
    matches!(
        name,
        "prefix_globals" | "mods" | "mod_time" | "mods_ease" | "mod_perframes" | "mod_actions"
    )
}

pub fn compile_song_lua(
    entry_path: &Path,
    context: &SongLuaCompileContext,
) -> Result<CompiledSongLua, String> {
    let compile_started = Instant::now();
    let mut stage_started = compile_started;
    let mut stage_times = Vec::new();
    let entry_path = entry_file_path(entry_path)
        .ok_or_else(|| format!("song lua entry '{}' does not exist", entry_path.display()))?;
    let trace_entry_path = entry_path.clone();
    let lua = Lua::new();
    let mut host = HostState::default();
    install_host(&lua, context, &mut host).map_err(|err| err.to_string())?;
    push_song_lua_stage_time(&mut stage_times, "host", &mut stage_started);
    let root = execute_script_file(&lua, &entry_path, context.song_dir.as_path())
        .map_err(|err| format!("failed to execute '{}': {err}", entry_path.display()))?;
    push_song_lua_stage_time(&mut stage_times, "execute", &mut stage_started);
    run_actor_init_commands(&lua, &root).map_err(|err| {
        format!(
            "failed to run actor init commands for '{}': {err}",
            entry_path.display()
        )
    })?;
    push_song_lua_stage_time(&mut stage_times, "init_commands", &mut stage_started);
    run_actor_startup_commands(&lua, &root).map_err(|err| {
        format!(
            "failed to run actor startup commands for '{}': {err}",
            entry_path.display()
        )
    })?;
    push_song_lua_stage_time(&mut stage_times, "startup_commands", &mut stage_started);
    run_actor_update_functions(&lua, &root).map_err(|err| {
        format!(
            "failed to run actor update functions for '{}': {err}",
            entry_path.display()
        )
    })?;
    push_song_lua_stage_time(&mut stage_times, "update_functions", &mut stage_started);
    run_actor_draw_functions(&lua, &root);
    push_song_lua_stage_time(&mut stage_times, "draw_functions", &mut stage_started);
    register_loaded_easing_names(&lua, &mut host).map_err(|err| err.to_string())?;
    push_song_lua_stage_time(&mut stage_times, "easing_names", &mut stage_started);

    let globals = lua.globals();
    let mut out = CompiledSongLua {
        entry_path,
        screen_width: context.screen_width,
        screen_height: context.screen_height,
        ..CompiledSongLua::default()
    };
    // Overlay command capture replays actor commands. Restore the mod globals
    // afterwards so capture-time side effects do not rewrite compile inputs.
    let compile_globals =
        snapshot_compile_globals(&lua, &globals).map_err(|err| err.to_string())?;
    let overlays = read_overlay_actors(&lua, &root, context, &mut out.info);
    restore_compile_globals(&globals, compile_globals).map_err(|err| err.to_string())?;
    let mut overlays = overlays?;
    push_song_lua_stage_time(&mut stage_times, "read_overlays", &mut stage_started);
    let mut tracked_actors = read_tracked_compile_actors(&lua)?;
    let mut overlay_trigger_counter = 0usize;
    let hidden_players = std::array::from_fn(|player| {
        let key = if player == 0 {
            "__songlua_top_screen_player_1"
        } else {
            "__songlua_top_screen_player_2"
        };
        globals
            .get::<Option<Table>>(key)
            .ok()
            .flatten()
            .and_then(|actor| {
                actor
                    .get::<Option<bool>>("__songlua_visible")
                    .ok()
                    .flatten()
            })
            .is_some_and(|visible| !visible)
    });
    let prefix_perframes = globals
        .get::<Option<Table>>("prefix_globals")
        .map_err(|err| err.to_string())?
        .and_then(|table| table.get::<Option<Table>>("perframes").ok().flatten());
    let global_perframes = globals
        .get::<Option<Table>>("mod_perframes")
        .map_err(|err| err.to_string())?;
    push_song_lua_stage_time(&mut stage_times, "read_globals", &mut stage_started);

    if let Some(prefix_globals) = globals
        .get::<Option<Table>>("prefix_globals")
        .map_err(|err| err.to_string())?
    {
        out.beat_mods.extend(read_mod_windows(
            prefix_globals
                .get::<Option<Table>>("mods")
                .map_err(|err| err.to_string())?,
            SongLuaTimeUnit::Beat,
        )?);
        push_song_lua_stage_time(&mut stage_times, "prefix_mods", &mut stage_started);
        let (eases, overlay_eases, info) = read_eases(
            &lua,
            prefix_globals
                .get::<Option<Table>>("ease")
                .map_err(|err| err.to_string())?,
            SongLuaTimeUnit::Beat,
            &host.easing_names,
            &mut overlays,
        )?;
        out.eases.extend(eases);
        out.overlay_eases.extend(overlay_eases);
        merge_compile_info(&mut out.info, info);
        push_song_lua_stage_time(&mut stage_times, "prefix_eases", &mut stage_started);
        read_actions(
            &lua,
            prefix_globals
                .get::<Option<Table>>("actions")
                .map_err(|err| err.to_string())?,
            &mut overlays,
            &mut tracked_actors,
            &mut out.messages,
            &mut overlay_trigger_counter,
            &mut out.info,
        )?;
        push_song_lua_stage_time(&mut stage_times, "prefix_actions", &mut stage_started);
    }

    let global_mods = globals
        .get::<Option<Table>>("mods")
        .map_err(|err| err.to_string())?;
    out.beat_mods.extend(read_mod_windows(
        global_mods.clone(),
        SongLuaTimeUnit::Beat,
    )?);
    let (runtime_eases, runtime_overlay_eases) =
        read_runtime_mod_eases(global_mods, &host.easing_names, &mut overlays, context)?;
    out.eases.extend(runtime_eases);
    out.overlay_eases.extend(runtime_overlay_eases);
    out.time_mods.extend(read_mod_windows(
        globals
            .get::<Option<Table>>("mod_time")
            .map_err(|err| err.to_string())?,
        SongLuaTimeUnit::Second,
    )?);
    for table in read_update_function_tables(&lua, &root, &["mod_time"])? {
        out.time_mods
            .extend(read_mod_windows(Some(table), SongLuaTimeUnit::Second)?);
    }
    push_song_lua_stage_time(&mut stage_times, "global_mods", &mut stage_started);
    let (global_eases, global_overlay_eases, global_info) = read_eases(
        &lua,
        globals
            .get::<Option<Table>>("mods_ease")
            .map_err(|err| err.to_string())?,
        SongLuaTimeUnit::Beat,
        &host.easing_names,
        &mut overlays,
    )?;
    out.eases.extend(global_eases);
    out.overlay_eases.extend(global_overlay_eases);
    merge_compile_info(&mut out.info, global_info);
    push_song_lua_stage_time(&mut stage_times, "global_eases", &mut stage_started);
    read_actions(
        &lua,
        globals
            .get::<Option<Table>>("mod_actions")
            .map_err(|err| err.to_string())?,
        &mut overlays,
        &mut tracked_actors,
        &mut out.messages,
        &mut overlay_trigger_counter,
        &mut out.info,
    )?;
    push_song_lua_stage_time(&mut stage_times, "global_actions", &mut stage_started);
    read_update_function_actions(
        &lua,
        &root,
        &mut overlays,
        &mut tracked_actors,
        &mut out.messages,
        &mut overlay_trigger_counter,
        &mut out.info,
    )?;
    push_song_lua_stage_time(&mut stage_times, "update_actions", &mut stage_started);
    let (perframe_eases, perframe_overlay_eases, perframe_info) = compile_perframes(
        &lua,
        prefix_perframes,
        global_perframes,
        context,
        &mut overlays,
        &tracked_actors,
    )?;
    out.eases.extend(perframe_eases);
    out.overlay_eases.extend(perframe_overlay_eases);
    merge_compile_info(&mut out.info, perframe_info);
    push_song_lua_stage_time(&mut stage_times, "perframes", &mut stage_started);
    out.note_hides = read_note_column_zoom_hides(&lua)?;
    push_song_lua_stage_time(&mut stage_times, "note_hides", &mut stage_started);
    let update_overlay_eases = match compile_multitap_update_overlays(&lua, context, &mut overlays)?
    {
        Some(eases) => eases,
        None => {
            compile_update_function_overlays(&lua, &root, context, &mut overlays, &tracked_actors)?
        }
    };
    out.overlay_eases.extend(update_overlay_eases);
    push_song_lua_stage_time(&mut stage_times, "update_overlays", &mut stage_started);
    if overlays.iter().any(|overlay| {
        overlay
            .actor
            .message_commands
            .iter()
            .any(|command| command.message == SONG_LUA_STARTUP_MESSAGE)
    }) {
        out.messages.push(SongLuaMessageEvent {
            beat: 0.0,
            message: SONG_LUA_STARTUP_MESSAGE.to_string(),
            persists: false,
        });
    }
    out.overlays = overlays.into_iter().map(|overlay| overlay.actor).collect();
    for tracked in tracked_actors {
        match tracked.target {
            TrackedCompileActorTarget::Player(player) => out.player_actors[player] = tracked.actor,
            TrackedCompileActorTarget::SongForeground => out.song_foreground = tracked.actor,
        }
    }
    out.hidden_players = hidden_players;

    out.beat_mods.sort_by(mod_window_cmp);
    out.time_mods.sort_by(mod_window_cmp);
    out.eases.sort_by(ease_window_cmp);
    out.overlay_eases.sort_by(|left, right| {
        left.start
            .total_cmp(&right.start)
            .then_with(|| left.limit.total_cmp(&right.limit))
            .then_with(|| left.overlay_index.cmp(&right.overlay_index))
    });
    out.messages.sort_by(message_event_cmp);
    out.sound_paths = read_song_lua_sound_paths(&lua)?;
    push_song_lua_stage_time(&mut stage_times, "finalize", &mut stage_started);
    log_song_lua_compile_timing(&trace_entry_path, compile_started, &stage_times);
    Ok(out)
}

fn push_song_lua_stage_time(
    stage_times: &mut Vec<(&'static str, f64)>,
    stage: &'static str,
    stage_started: &mut Instant,
) {
    stage_times.push((stage, stage_started.elapsed().as_secs_f64() * 1000.0));
    *stage_started = Instant::now();
}

fn log_song_lua_compile_timing(
    entry_path: &Path,
    compile_started: Instant,
    stage_times: &[(&'static str, f64)],
) {
    let elapsed_ms = compile_started.elapsed().as_secs_f64() * 1000.0;
    if elapsed_ms < 1000.0 {
        return;
    }
    let mut stages = String::new();
    for (stage, ms) in stage_times {
        if !stages.is_empty() {
            stages.push(' ');
        }
        stages.push_str(stage);
        stages.push_str("_ms=");
        stages.push_str(format!("{ms:.3}").as_str());
    }
    info!(
        "Song lua compile timing: entry='{}' elapsed_ms={elapsed_ms:.3} {}",
        entry_path.display(),
        stages
    );
}

fn install_host(
    lua: &Lua,
    context: &SongLuaCompileContext,
    host: &mut HostState,
) -> mlua::Result<()> {
    install_stdlib_compat(lua, context.song_dir.as_path())?;
    install_ease_table(lua, host)?;
    install_globals(lua, context)?;
    install_cmd_helpers(lua)?;
    install_def(lua, context)?;
    install_file_loaders(lua, context.song_dir.clone())?;
    Ok(())
}

fn install_ease_table(lua: &Lua, host: &mut HostState) -> mlua::Result<()> {
    let globals = lua.globals();
    let ease = lua.create_table()?;
    for &name in EASING_NAMES {
        let function = lua.create_function(
            |_, (_t, b, c, d, _p1, _p2): (f32, f32, f32, f32, Value, Value)| {
                if d.abs() <= f32::EPSILON {
                    Ok(b + c)
                } else {
                    Ok(b + c * (f32::min(f32::max(_t / d, 0.0), 1.0)))
                }
            },
        )?;
        host.easing_names
            .insert(function.to_pointer(), name.to_string());
        ease.set(name, function)?;
    }
    globals.set("ease", ease)?;
    Ok(())
}

fn register_loaded_easing_names(lua: &Lua, host: &mut HostState) -> mlua::Result<()> {
    let globals = lua.globals();
    if let Some(ease) = globals.get::<Option<Table>>("ease")? {
        register_easing_table(&ease, host)?;
    }
    if let Some(xero) = globals.get::<Option<Table>>("xero")? {
        register_easing_table(&xero, host)?;
    }
    Ok(())
}

fn register_easing_table(table: &Table, host: &mut HostState) -> mlua::Result<()> {
    for &name in EASING_NAMES {
        match table.get::<Value>(name)? {
            Value::Function(function) => {
                host.easing_names
                    .insert(function.to_pointer(), name.to_string());
            }
            Value::Table(table) => {
                host.easing_names
                    .insert(table.to_pointer(), name.to_string());
                table.raw_set(SONG_LUA_EASING_NAME_KEY, name)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn install_cmd_helpers(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    for name in ["queuecommand", "playcommand"] {
        globals.set(name, name)?;
    }
    globals.set(
        "cmd",
        lua.create_function(move |lua, args: MultiValue| {
            let command_name = args.get(0).cloned().and_then(read_string);
            let command_args = args.into_iter().skip(1).collect::<Vec<_>>();
            lua.create_function(move |_, actor: Table| {
                let Some(command_name) = command_name.as_deref() else {
                    return Ok(Value::Table(actor));
                };
                let Value::Function(method) = actor.get::<Value>(command_name)? else {
                    return Ok(Value::Table(actor));
                };
                let mut call_args = MultiValue::new();
                call_args.push_back(Value::Table(actor.clone()));
                for arg in &command_args {
                    call_args.push_back(arg.clone());
                }
                let _ = method.call::<Value>(call_args)?;
                Ok(Value::Table(actor))
            })
        })?,
    )?;
    Ok(())
}

fn prefsmgr_default_value(
    lua: &Lua,
    key: &str,
    global_offset_seconds: f32,
    display_aspect_ratio: f32,
    display_width: i32,
    display_height: i32,
) -> mlua::Result<Value> {
    let lower = key.to_ascii_lowercase();
    if lower == "globaloffsetseconds" {
        Ok(Value::Number(global_offset_seconds as f64))
    } else if lower == "displayaspectratio" {
        Ok(Value::Number(display_aspect_ratio as f64))
    } else if lower == "displaywidth" {
        Ok(Value::Integer(display_width as i64))
    } else if lower == "displayheight" {
        Ok(Value::Integer(display_height as i64))
    } else if lower == "videorenderers" {
        Ok(Value::String(lua.create_string("opengl")?))
    } else if lower == "visualdelayseconds" {
        Ok(Value::Number(0.0))
    } else if lower == "bgbrightness" {
        Ok(Value::Number(1.0))
    } else if lower == "timingwindowscale" {
        Ok(Value::Number(1.0))
    } else if lower == "timingwindowadd" {
        Ok(Value::Number(0.0015))
    } else if lower.starts_with("timingwindowseconds") {
        Ok(Value::Number(0.0))
    } else if matches!(
        lower.as_str(),
        "autogengroupcourses"
            | "center1player"
            | "eastereggs"
            | "eventmode"
            | "harshhotlifepenalty"
            | "memorycards"
            | "menutimer"
            | "onlydedicatedmenubuttons"
            | "showbanners"
            | "shownativelanguage"
            | "threekeynavigation"
    ) {
        Ok(Value::Boolean(false))
    } else if matches!(
        lower.as_str(),
        "coinspercredit"
            | "customsongsloadtimeout"
            | "customsongsmaxmegabytes"
            | "customsongsmaxseconds"
            | "lifedifficultyscale"
            | "longversongseconds"
            | "marathonversongseconds"
            | "maxhighscoresperlistfor"
            | "maxhighscoresperlistformachine"
            | "maxregencomboaftermiss"
            | "mintnstoscorenotes"
            | "musicwheelswitchspeed"
            | "regencomboaftermiss"
            | "songsperplay"
            | "soundvolume"
            | "refreshrate"
    ) {
        let value = match lower.as_str() {
            "longversongseconds" => 150,
            "marathonversongseconds" => 300,
            "refreshrate" => 60,
            "maxhighscoresperlistfor" | "maxhighscoresperlistformachine" => 10,
            "songsperplay" | "soundvolume" => 1,
            _ => 0,
        };
        Ok(Value::Integer(value))
    } else if matches!(
        lower.as_str(),
        "coinmode"
            | "defaultmodifiers"
            | "editornoteskinp1"
            | "editornoteskinp2"
            | "displaymode"
            | "displayresolution"
            | "fullscreentype"
            | "httpallowhosts"
            | "premium"
    ) {
        let value = match lower.as_str() {
            "coinmode" => "CoinMode_Free",
            "displaymode" => "Windowed",
            "displayresolution" => "1920x1080",
            "editornoteskinp1" | "editornoteskinp2" => "default",
            "fullscreentype" => "Borderless",
            "premium" => "Premium_Off",
            _ => "",
        };
        Ok(Value::String(lua.create_string(value)?))
    } else if lower == "theme" {
        Ok(Value::String(lua.create_string(SONG_LUA_THEME_NAME)?))
    } else if lower.starts_with("defaultlocalprofileid") {
        Ok(Value::String(lua.create_string("")?))
    } else {
        Ok(Value::Nil)
    }
}

fn install_globals(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<()> {
    let globals = lua.globals();
    let screen_width = context.screen_width.max(1.0);
    let screen_height = context.screen_height.max(1.0);
    let screen_center_x = 0.5 * screen_width;
    let screen_center_y = 0.5 * screen_height;
    globals.set("PLAYER_1", player_number_name(0))?;
    globals.set("PLAYER_2", player_number_name(1))?;
    globals.set("PlayerNumber", create_player_number_table(lua)?)?;
    globals.set("OtherPlayer", create_other_player_table(lua)?)?;
    globals.set("Difficulty", create_difficulty_table(lua)?)?;
    globals.set(
        "EditState",
        create_string_enum_table(
            lua,
            &[
                "EditState_Edit",
                "EditState_Record",
                "EditState_RecordPaused",
                "EditState_Playing",
            ],
        )?,
    )?;
    globals.set(
        "ProfileSlot",
        create_string_enum_table(
            lua,
            &[
                "ProfileSlot_Player1",
                "ProfileSlot_Player2",
                "ProfileSlot_Machine",
            ],
        )?,
    )?;
    globals.set(
        "GameController",
        create_string_enum_table(lua, &["GameController_1", "GameController_2"])?,
    )?;
    globals.set(
        "PlayerController",
        create_string_enum_table(
            lua,
            &[
                "PlayerController_Human",
                "PlayerController_Autoplay",
                "PlayerController_Cpu",
            ],
        )?,
    )?;
    globals.set(
        "HealthState",
        create_string_enum_table(
            lua,
            &[
                "HealthState_Hot",
                "HealthState_Alive",
                "HealthState_Danger",
                "HealthState_Dead",
            ],
        )?,
    )?;
    globals.set(
        "SortOrder",
        create_string_enum_table(
            lua,
            &[
                "SortOrder_Group",
                "SortOrder_Title",
                "SortOrder_Artist",
                "SortOrder_BPM",
                "SortOrder_Popularity",
                "SortOrder_Preferred",
                "SortOrder_Recent",
            ],
        )?,
    )?;
    globals.set(
        "TapNoteScore",
        create_string_enum_table(
            lua,
            &[
                "TapNoteScore_W1",
                "TapNoteScore_W2",
                "TapNoteScore_W3",
                "TapNoteScore_W4",
                "TapNoteScore_W5",
                "TapNoteScore_Miss",
                "TapNoteScore_HitMine",
                "TapNoteScore_AvoidMine",
                "TapNoteScore_CheckpointMiss",
                "TapNoteScore_CheckpointHit",
                "TapNoteScore_None",
            ],
        )?,
    )?;
    globals.set(
        "TapNoteType",
        create_string_enum_table(
            lua,
            &[
                "TapNoteType_Empty",
                "TapNoteType_Tap",
                "TapNoteType_HoldHead",
                "TapNoteType_HoldTail",
                "TapNoteType_Mine",
                "TapNoteType_Lift",
                "TapNoteType_Attack",
                "TapNoteType_AutoKeysound",
                "TapNoteType_Fake",
            ],
        )?,
    )?;
    globals.set(
        "TapNoteSource",
        create_string_enum_table(
            lua,
            &[
                "TapNoteSource_Original",
                "TapNoteSource_Addition",
                "TapNoteSource_Mine",
            ],
        )?,
    )?;
    globals.set(
        "TapNoteSubType",
        create_string_enum_table(
            lua,
            &[
                "TapNoteSubType_Invalid",
                "TapNoteSubType_Hold",
                "TapNoteSubType_Roll",
            ],
        )?,
    )?;
    globals.set(
        "HoldNoteScore",
        create_string_enum_table(
            lua,
            &[
                "HoldNoteScore_Held",
                "HoldNoteScore_LetGo",
                "HoldNoteScore_MissedHold",
                "HoldNoteScore_None",
            ],
        )?,
    )?;
    globals.set(
        "StepsType",
        create_string_enum_table(lua, &["StepsType_Dance_Single", "StepsType_Dance_Double"])?,
    )?;
    globals.set(
        "StyleType",
        create_string_enum_table(
            lua,
            &[
                "StyleType_OnePlayerOneSide",
                "StyleType_OnePlayerTwoSides",
                "StyleType_TwoPlayersTwoSides",
            ],
        )?,
    )?;
    globals.set("SCREEN_WIDTH", screen_width.round() as i32)?;
    globals.set("SCREEN_HEIGHT", screen_height.round() as i32)?;
    globals.set("SCREEN_CENTER_X", screen_center_x)?;
    globals.set("SCREEN_CENTER_Y", screen_center_y)?;
    globals.set("scx", screen_center_x)?;
    globals.set("scy", screen_center_y)?;
    globals.set("SCREEN_LEFT", 0)?;
    globals.set("SCREEN_TOP", 0)?;
    globals.set("SCREEN_RIGHT", screen_width.round() as i32)?;
    globals.set("SCREEN_BOTTOM", screen_height.round() as i32)?;
    globals.set(
        "_screen",
        create_screen_table(
            lua,
            screen_width,
            screen_height,
            screen_center_x,
            screen_center_y,
        )?,
    )?;
    globals.set(
        "ProductFamily",
        lua.create_function(|_, _args: MultiValue| Ok(SONG_LUA_PRODUCT_FAMILY))?,
    )?;
    globals.set(
        "ProductID",
        lua.create_function(|_, _args: MultiValue| Ok(SONG_LUA_PRODUCT_ID))?,
    )?;
    globals.set(
        "ProductVersion",
        lua.create_function(|_, _args: MultiValue| Ok(SONG_LUA_PRODUCT_VERSION))?,
    )?;
    globals.set(
        "ToEnumShortString",
        lua.create_function(|lua, value: String| {
            let short = value
                .split_once('_')
                .map(|(_, short)| short)
                .unwrap_or(value.as_str());
            Ok(Value::String(lua.create_string(short)?))
        })?,
    )?;
    globals.set(
        "FormatPercentScore",
        lua.create_function(|lua, args: MultiValue| {
            let value = args.front().cloned().and_then(read_f32).unwrap_or(0.0);
            Ok(Value::String(
                lua.create_string(format!("{:.2}%", value * 100.0))?,
            ))
        })?,
    )?;
    globals.set(
        "clamp",
        lua.create_function(|_, (value, min, max): (f64, f64, f64)| Ok(value.clamp(min, max)))?,
    )?;
    globals.set(
        "GetScreenAspectRatio",
        lua.create_function(move |_, _args: MultiValue| Ok(screen_width / screen_height.max(1.0)))?,
    )?;
    globals.set(
        "WideScale",
        lua.create_function(move |_, (ar4_3, ar16_9): (f32, f32)| {
            Ok(scale_value(screen_width, 640.0, 854.0, ar4_3, ar16_9))
        })?,
    )?;
    globals.set(
        "SL_WideScale",
        lua.create_function(move |_, (ar4_3, ar16_9): (f32, f32)| {
            Ok(scale_value(screen_width, 640.0, 854.0, ar4_3, ar16_9))
        })?,
    )?;
    let is_widescreen = screen_width / screen_height.max(1.0) > 1.5;
    globals.set(
        "IsUsingWideScreen",
        lua.create_function(move |_, _args: MultiValue| Ok(is_widescreen))?,
    )?;
    globals.set(
        "DarkUI",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    globals.set(
        "IsServiceAllowed",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    globals.set(
        "round",
        lua.create_function(|_, args: MultiValue| {
            let value = args.front().cloned().and_then(read_f32).unwrap_or(0.0);
            let precision = args.get(1).cloned().and_then(read_f32).unwrap_or(0.0);
            let factor = 10.0_f32.powf(precision.max(0.0));
            Ok((value * factor).round() / factor)
        })?,
    )?;
    globals.set(
        "FindInTable",
        lua.create_function(|_, (needle, haystack): (Value, Value)| {
            let Value::Table(haystack) = haystack else {
                return Ok(Value::Nil);
            };
            for pair in haystack.pairs::<Value, Value>() {
                let (key, value) = pair?;
                if lua_values_equal(&needle, &value) {
                    return Ok(key);
                }
            }
            Ok(Value::Nil)
        })?,
    )?;
    globals.set(
        "DeepCopy",
        lua.create_function(|lua, args: MultiValue| {
            clone_lua_value(lua, args.front().cloned().unwrap_or(Value::Nil))
        })?,
    )?;
    globals.set(
        "SecondsToHHMMSS",
        lua.create_function(|lua, seconds: f64| {
            Ok(Value::String(
                lua.create_string(seconds_to_hhmmss(seconds))?,
            ))
        })?,
    )?;
    globals.set(
        "SecondsToMSS",
        lua.create_function(|lua, seconds: f64| {
            Ok(Value::String(lua.create_string(seconds_to_mss(seconds))?))
        })?,
    )?;
    globals.set(
        "SecondsToMMSS",
        lua.create_function(|lua, seconds: f64| {
            Ok(Value::String(lua.create_string(seconds_to_mmss(seconds))?))
        })?,
    )?;
    globals.set(
        "SecondsToMSSMsMs",
        lua.create_function(|lua, seconds: f64| {
            Ok(Value::String(
                lua.create_string(seconds_to_mss_ms_ms(seconds))?,
            ))
        })?,
    )?;
    globals.set(
        "SecondsToMMSSMsMs",
        lua.create_function(|lua, seconds: f64| {
            Ok(Value::String(
                lua.create_string(seconds_to_mmss_ms_ms(seconds))?,
            ))
        })?,
    )?;
    globals.set(
        "FormatNumberAndSuffix",
        lua.create_function(|lua, value: i64| {
            Ok(Value::String(
                lua.create_string(format_number_and_suffix(value))?,
            ))
        })?,
    )?;
    let now = Local::now();
    let year = now.year();
    let month_of_year = now.month0() as i32;
    let day_of_month = now.day() as i32;
    let hour = now.hour() as i32;
    let minute = now.minute() as i32;
    let second = now.second() as i32;
    globals.set(
        "Year",
        lua.create_function(move |_, _args: MultiValue| Ok(year))?,
    )?;
    globals.set(
        "MonthOfYear",
        lua.create_function(move |_, _args: MultiValue| Ok(month_of_year))?,
    )?;
    globals.set(
        "DayOfMonth",
        lua.create_function(move |_, _args: MultiValue| Ok(day_of_month))?,
    )?;
    globals.set(
        "Hour",
        lua.create_function(move |_, _args: MultiValue| Ok(hour))?,
    )?;
    globals.set(
        "Minute",
        lua.create_function(move |_, _args: MultiValue| Ok(minute))?,
    )?;
    globals.set(
        "Second",
        lua.create_function(move |_, _args: MultiValue| Ok(second))?,
    )?;
    globals.set(
        "GetTimeSinceStart",
        lua.create_function(|lua, _args: MultiValue| {
            let seconds = lua
                .globals()
                .get::<Option<Table>>(SONG_LUA_RUNTIME_KEY)?
                .and_then(|runtime| {
                    runtime
                        .get::<Option<f32>>(SONG_LUA_RUNTIME_SECONDS_KEY)
                        .ok()
                })
                .flatten()
                .unwrap_or(0.0);
            song_lua_runtime_number(seconds)
        })?,
    )?;
    let holiday_cheer = month_of_year == 11;
    globals.set(
        "HolidayCheer",
        lua.create_function(move |_, _args: MultiValue| Ok(holiday_cheer))?,
    )?;
    globals.set(
        "ASPECT_SCALE_FACTOR",
        screen_width / (640.0 * (screen_height / 480.0)),
    )?;
    globals.set(
        "scale",
        lua.create_function(
            |_, (value, from_low, from_high, to_low, to_high): (f32, f32, f32, f32, f32)| {
                Ok(scale_value(value, from_low, from_high, to_low, to_high))
            },
        )?,
    )?;
    globals.set(
        "__songlua_song_dir",
        song_dir_string(context.song_dir.as_path()),
    )?;
    globals.set("__songlua_script_dir", Value::Nil)?;
    globals.set("__songlua_current_script_path", Value::Nil)?;

    let player_options = lua.create_table()?;
    for capability in SONG_LUA_PLAYER_OPTION_CAPABILITIES {
        player_options.set(*capability, true)?;
    }
    for prefix in SONG_LUA_PLAYER_OPTION_MULTICOL_PREFIXES {
        for column in 1..=16 {
            player_options.set(format!("{prefix}{column}"), true)?;
        }
    }
    player_options.set("ConfusionOffset", context.confusion_offset_available)?;
    player_options.set("Confusion", context.confusion_available)?;
    player_options.set("AMod", context.amod_available)?;
    globals.set("PlayerOptions", player_options)?;

    let prefsmgr = lua.create_table()?;
    let global_offset_seconds = context.global_offset_seconds;
    let display_aspect_ratio = screen_width / screen_height.max(1.0);
    let display_width = screen_width.round() as i32;
    let display_height = screen_height.round() as i32;
    let pref_store = lua.create_table()?;
    let get_pref_store = pref_store.clone();
    prefsmgr.set(
        "GetPreference",
        lua.create_function(move |lua, args: MultiValue| {
            let Some(key) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            let stored = get_pref_store.get::<Value>(key.as_str())?;
            if !matches!(stored, Value::Nil) {
                return Ok(stored);
            }
            prefsmgr_default_value(
                lua,
                &key,
                global_offset_seconds,
                display_aspect_ratio,
                display_width,
                display_height,
            )
        })?,
    )?;
    let set_pref_store = pref_store.clone();
    let prefsmgr_for_set = prefsmgr.clone();
    prefsmgr.set(
        "SetPreference",
        lua.create_function(move |lua, args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            let Some(key) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(prefsmgr_for_set.clone());
            };
            if !matches!(
                prefsmgr_default_value(
                    lua,
                    &key,
                    global_offset_seconds,
                    display_aspect_ratio,
                    display_width,
                    display_height,
                )?,
                Value::Nil
            ) {
                let value = method_arg(&args, 1).cloned().unwrap_or(Value::Nil);
                set_pref_store.set(key, value)?;
            }
            Ok(prefsmgr_for_set.clone())
        })?,
    )?;
    prefsmgr.set(
        "PreferenceExists",
        lua.create_function(move |lua, args: MultiValue| {
            let Some(key) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(false);
            };
            Ok(!matches!(
                prefsmgr_default_value(
                    lua,
                    &key,
                    global_offset_seconds,
                    display_aspect_ratio,
                    display_width,
                    display_height,
                )?,
                Value::Nil
            ))
        })?,
    )?;
    let prefsmgr_for_save = prefsmgr.clone();
    prefsmgr.set(
        "SavePreferences",
        lua.create_function(move |lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(prefsmgr_for_save.clone())
        })?,
    )?;
    let reset_pref_store = pref_store;
    let prefsmgr_for_reset = prefsmgr.clone();
    prefsmgr.set(
        "SetPreferenceToDefault",
        lua.create_function(move |lua, args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            if let Some(key) = method_arg(&args, 0).cloned().and_then(read_string) {
                reset_pref_store.set(key, Value::Nil)?;
            }
            Ok(prefsmgr_for_reset.clone())
        })?,
    )?;
    globals.set("PREFSMAN", prefsmgr)?;
    globals.set("DISPLAY", create_display_table(lua, context)?)?;
    globals.set("THEME", create_theme_table(lua, context)?)?;
    globals.set("GAMEMAN", create_gameman_table(lua)?)?;
    globals.set("CHARMAN", create_charman_table(lua)?)?;
    globals.set("MEMCARDMAN", create_memcardman_table(lua)?)?;
    globals.set("UNLOCKMAN", create_unlockman_table(lua)?)?;
    globals.set("HOOKS", create_hooks_table(lua)?)?;
    globals.set("NOTESKIN", create_noteskin_table(lua, context)?)?;
    globals.set("SongUtil", create_song_util_table(lua)?)?;
    globals.set(
        "ScreenSystemLayerHelpers",
        create_screen_system_layer_helpers_table(lua)?,
    )?;
    globals.set("ArrowEffects", create_arrow_effects_table(lua)?)?;
    globals.set(
        "ScreenString",
        lua.create_function(|lua, args: MultiValue| {
            let name = method_arg(&args, 0)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            Ok(Value::String(lua.create_string(theme_string("", &name))?))
        })?,
    )?;
    globals.set(SONG_LUA_SIDE_EFFECT_COUNT_KEY, 0_i64)?;

    let song_runtime = create_song_runtime_table(lua, context)?;
    globals.set(SONG_LUA_RUNTIME_KEY, song_runtime.clone())?;
    let song = create_song_table(lua, context)?;
    let players = create_player_tables(lua, context, &song_runtime)?;
    let song_options = create_song_options_table(lua, context.song_music_rate)?;
    let display_bpms = context.song_display_bpms;
    let default_music_rate = song_music_rate(context);
    globals.set(
        "GetDisplayBPMs",
        lua.create_function(move |lua, args: MultiValue| {
            let (bpms, _) = display_bpms_for_args(&args, display_bpms, default_music_rate)?;
            create_display_bpms_table(lua, bpms)
        })?,
    )?;
    globals.set(
        "StringifyDisplayBPMs",
        lua.create_function(move |lua, args: MultiValue| {
            let (bpms, rate) = display_bpms_for_args(&args, display_bpms, default_music_rate)?;
            Ok(Value::String(
                lua.create_string(display_bpms_text(bpms, rate))?,
            ))
        })?,
    )?;
    let total_song_seconds = context.music_length_seconds.max(0.0);
    globals.set(
        "totalLengthSongOrCourse",
        lua.create_function(move |_, _args: MultiValue| {
            Ok(total_song_seconds / default_music_rate.max(f32::EPSILON))
        })?,
    )?;
    globals.set(
        "currentTimeSongOrCourse",
        lua.create_function({
            let song_runtime = song_runtime.clone();
            move |_, _args: MultiValue| Ok(song_runtime.get::<f32>(SONG_LUA_RUNTIME_SECONDS_KEY)?)
        })?,
    )?;
    let current_sort_order = lua.create_table()?;
    current_sort_order.raw_set(1, "SortOrder_Group")?;
    let gamestate = lua.create_table()?;
    let game_env = lua.create_table()?;
    gamestate.set(
        "Env",
        lua.create_function(move |_, _args: MultiValue| Ok(game_env.clone()))?,
    )?;
    let enabled_players = create_enabled_players_table(lua, context.players.clone())?;
    let human_players = enabled_players.clone();
    let current_song = lua.create_table()?;
    current_song.raw_set(1, song.clone())?;
    gamestate.set(
        "GetCurrentSong",
        lua.create_function({
            let current_song = current_song.clone();
            move |_, _args: MultiValue| {
                Ok(current_song
                    .raw_get::<Option<Table>>(1)?
                    .map_or(Value::Nil, Value::Table))
            }
        })?,
    )?;
    let current_style = lua.create_table()?;
    current_style.raw_set(1, create_style_table(lua, &context.style_name)?)?;
    gamestate.set(
        "GetCurrentStyle",
        lua.create_function({
            let current_style = current_style.clone();
            move |_, _args: MultiValue| {
                Ok(current_style
                    .raw_get::<Option<Table>>(1)?
                    .map_or(Value::Nil, Value::Table))
            }
        })?,
    )?;
    let players_enabled = context.players.clone();
    gamestate.set(
        "IsPlayerEnabled",
        lua.create_function(move |_, args: MultiValue| {
            let Some(player) = method_arg(&args, 0).and_then(player_index_from_value) else {
                return Ok(false);
            };
            Ok(players_enabled[player].enabled)
        })?,
    )?;
    let human_players_enabled = context.players.clone();
    gamestate.set(
        "IsHumanPlayer",
        lua.create_function(move |_, args: MultiValue| {
            let Some(player) = method_arg(&args, 0).and_then(player_index_from_value) else {
                return Ok(false);
            };
            Ok(human_players_enabled[player].enabled)
        })?,
    )?;
    let sides_joined = context.players.clone();
    gamestate.set(
        "IsSideJoined",
        lua.create_function(move |_, args: MultiValue| {
            let Some(player) = method_arg(&args, 0).and_then(player_index_from_value) else {
                return Ok(false);
            };
            Ok(sides_joined[player].enabled)
        })?,
    )?;
    let global_human_players_enabled = context.players.clone();
    globals.set(
        "IsHumanPlayer",
        lua.create_function(move |_, args: MultiValue| {
            let Some(player) = args.front().and_then(player_index_from_value) else {
                return Ok(false);
            };
            Ok(global_human_players_enabled[player].enabled)
        })?,
    )?;
    globals.set(
        "IsAutoplay",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    globals.set(
        "IsW0Judgment",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    globals.set(
        "IsW010Judgment",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    globals.set(
        "GetDefaultFailType",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string("FailType_Immediate")?))
        })?,
    )?;
    globals.set(
        "GetComboThreshold",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string("TapNoteScore_W3")?))
        })?,
    )?;
    let notefield_players = context.players.clone();
    globals.set(
        "GetNotefieldX",
        lua.create_function(move |_, args: MultiValue| {
            let Some(player) = method_arg(&args, 0).and_then(player_index_from_value) else {
                return Ok(Value::Nil);
            };
            Ok(Value::Number(notefield_players[player].screen_x.into()))
        })?,
    )?;
    globals.set(
        "GetNotefieldWidth",
        lua.create_function(|_, _args: MultiValue| Ok(256.0_f32))?,
    )?;
    globals.set(
        "GetGameplayLayout",
        lua.create_function(move |lua, args: MultiValue| {
            let reverse = method_arg(&args, 1).is_some_and(truthy);
            create_gameplay_layout(lua, screen_center_y, reverse)
        })?,
    )?;
    gamestate.set(
        "GetEnabledPlayers",
        lua.create_function(move |_, _args: MultiValue| Ok(enabled_players.clone()))?,
    )?;
    gamestate.set(
        "GetHumanPlayers",
        lua.create_function(move |_, _args: MultiValue| Ok(human_players.clone()))?,
    )?;
    let player_states = players.player_states.clone();
    gamestate.set(
        "GetPlayerState",
        lua.create_function(move |_, args: MultiValue| {
            let Some(player) = method_arg(&args, 0).and_then(player_index_from_value) else {
                return Ok(Value::Nil);
            };
            Ok(Value::Table(player_states[player].clone()))
        })?,
    )?;
    let global_player_states = players.player_states.clone();
    globals.set(
        "GetPlayerOptionsString",
        lua.create_function(move |_, args: MultiValue| {
            let Some(player) = args.front().and_then(player_index_from_value) else {
                return Ok(String::new());
            };
            Ok(global_player_states[player]
                .get::<Option<String>>("__songlua_player_options_string")?
                .unwrap_or_default())
        })?,
    )?;
    let current_steps = lua.create_table()?;
    for (player_index, steps) in players.steps.iter().enumerate() {
        current_steps.raw_set(player_index + 1, steps.clone())?;
    }
    let current_trail = lua.create_table()?;
    let primary_trail = create_trail_table(
        lua,
        song.clone(),
        players.steps[0].clone(),
        context.song_display_bpms,
    )?;
    for (player_index, steps) in players.steps.iter().enumerate() {
        let trail = if player_index == 0 {
            primary_trail.clone()
        } else {
            create_trail_table(lua, song.clone(), steps.clone(), context.song_display_bpms)?
        };
        current_trail.raw_set(player_index + 1, trail)?;
    }
    let course = create_course_table(lua, context, song.clone(), primary_trail.clone())?;
    globals.set(
        "SONGMAN",
        create_songman_table(
            lua,
            song.clone(),
            players.steps[0].clone(),
            course.clone(),
            context,
        )?,
    )?;
    gamestate.set(
        "GetCurrentSteps",
        lua.create_function({
            let current_steps = current_steps.clone();
            move |_, args: MultiValue| {
                let Some(player) = method_arg(&args, 0).and_then(player_index_from_value) else {
                    return Ok(Value::Nil);
                };
                Ok(current_steps
                    .raw_get::<Option<Table>>(player + 1)?
                    .map_or(Value::Nil, Value::Table))
            }
        })?,
    )?;
    globals.set(
        "GetSongAndSteps",
        lua.create_function({
            let current_song = current_song.clone();
            let current_steps = current_steps.clone();
            move |_, args: MultiValue| {
                let player = args.front().and_then(player_index_from_value).unwrap_or(0);
                let song = current_song
                    .raw_get::<Option<Table>>(1)?
                    .map_or(Value::Nil, Value::Table);
                let steps = current_steps
                    .raw_get::<Option<Table>>(player + 1)?
                    .map_or(Value::Nil, Value::Table);
                Ok((song, steps))
            }
        })?,
    )?;
    let easiest_steps_difficulty = easiest_steps_difficulty(&context.players);
    gamestate.set(
        "GetEasiestStepsDifficulty",
        lua.create_function(move |lua, _args: MultiValue| {
            let Some(difficulty) = easiest_steps_difficulty else {
                return Ok(Value::Nil);
            };
            Ok(Value::String(lua.create_string(difficulty.sm_name())?))
        })?,
    )?;
    gamestate.set(
        "SetCurrentSteps",
        lua.create_function({
            let current_steps = current_steps.clone();
            move |lua, args: MultiValue| {
                let Some(player) = method_arg(&args, 0).and_then(player_index_from_value) else {
                    return Ok(());
                };
                let Some(steps) = method_arg(&args, 1).and_then(|value| match value {
                    Value::Table(table) => Some(table.clone()),
                    _ => None,
                }) else {
                    return Ok(());
                };
                current_steps.raw_set(player + 1, steps)?;
                note_song_lua_side_effect(lua)?;
                Ok(())
            }
        })?,
    )?;
    gamestate.set(
        "GetSongBeat",
        lua.create_function({
            let song_runtime = song_runtime.clone();
            move |_, _args: MultiValue| Ok(song_runtime.get::<f32>(SONG_LUA_RUNTIME_BEAT_KEY)?)
        })?,
    )?;
    let song_bps = song_display_bps(context);
    gamestate.set(
        "GetSongBPS",
        lua.create_function(move |_, _args: MultiValue| Ok(song_bps))?,
    )?;
    gamestate.set(
        "GetCurMusicSeconds",
        lua.create_function({
            let song_runtime = song_runtime.clone();
            move |_, _args: MultiValue| Ok(song_runtime.get::<f32>(SONG_LUA_RUNTIME_SECONDS_KEY)?)
        })?,
    )?;
    let song_position = create_song_position_table(lua, &song_runtime)?;
    gamestate.set(
        "GetSongPosition",
        lua.create_function(move |_, _args: MultiValue| Ok(song_position.clone()))?,
    )?;
    gamestate.set(
        "GetSongOptionsObject",
        lua.create_function({
            let song_options = song_options.clone();
            move |_, _args: MultiValue| Ok(song_options.clone())
        })?,
    )?;
    gamestate.set(
        "GetSongOptions",
        lua.create_function({
            let song_options = song_options.clone();
            move |lua, _args: MultiValue| {
                let rate = song_options
                    .get::<Option<f32>>("__songlua_music_rate")?
                    .unwrap_or(1.0);
                Ok(Value::String(
                    lua.create_string(format_song_options_text(rate))?,
                ))
            }
        })?,
    )?;
    let master_player = context
        .players
        .iter()
        .position(|player| player.enabled)
        .unwrap_or(0);
    let master_player_name = player_number_name(master_player).to_string();
    gamestate.set(
        "GetMasterPlayerNumber",
        lua.create_function(move |lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string(&master_player_name)?))
        })?,
    )?;
    let joined_sides = context
        .players
        .iter()
        .filter(|player| player.enabled)
        .count() as i32;
    gamestate.set(
        "GetNumSidesJoined",
        lua.create_function(move |_, _args: MultiValue| Ok(joined_sides))?,
    )?;
    gamestate.set(
        "GetNumPlayersEnabled",
        lua.create_function(move |_, _args: MultiValue| Ok(joined_sides))?,
    )?;
    gamestate.set(
        "IsCourseMode",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    gamestate.set(
        "IsEventMode",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    gamestate.set(
        "GetCurrentTrail",
        lua.create_function({
            let current_trail = current_trail.clone();
            move |_, args: MultiValue| {
                let Some(player) = method_arg(&args, 0).and_then(player_index_from_value) else {
                    return Ok(Value::Nil);
                };
                Ok(current_trail
                    .raw_get::<Option<Table>>(player + 1)?
                    .map_or(Value::Nil, Value::Table))
            }
        })?,
    )?;
    gamestate.set(
        "GetCurrentCourse",
        lua.create_function(move |_, _args: MultiValue| Ok(course.clone()))?,
    )?;
    let current_game = create_game_table(lua)?;
    gamestate.set(
        "GetCurrentGame",
        lua.create_function(move |_, _args: MultiValue| Ok(current_game.clone()))?,
    )?;
    gamestate.set(
        "GetCoinMode",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string("CoinMode_Free")?))
        })?,
    )?;
    gamestate.set(
        "GetPremium",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string("Premium_Off")?))
        })?,
    )?;
    gamestate.set(
        "GetCoins",
        lua.create_function(|_, _args: MultiValue| Ok(0))?,
    )?;
    gamestate.set(
        "GetCoinsNeededToJoin",
        lua.create_function(|_, _args: MultiValue| Ok(0))?,
    )?;
    gamestate.set(
        "GetNumStagesLeft",
        lua.create_function(|_, _args: MultiValue| Ok(1))?,
    )?;
    gamestate.set(
        "GetCurrentStageIndex",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    gamestate.set(
        "GetCourseSongIndex",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    gamestate.set(
        "GetPlayMode",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string("PlayMode_Regular")?))
        })?,
    )?;
    gamestate.set(
        "GetSortOrder",
        lua.create_function({
            let current_sort_order = current_sort_order.clone();
            move |lua, _args: MultiValue| {
                let sort = current_sort_order
                    .raw_get::<Option<String>>(1)?
                    .unwrap_or_else(|| "SortOrder_Group".to_string());
                Ok(Value::String(lua.create_string(&sort)?))
            }
        })?,
    )?;
    gamestate.set(
        "GetPlayerFailType",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string("FailType_Immediate")?))
        })?,
    )?;
    gamestate.set(
        "SetCurrentSong",
        lua.create_function({
            let current_song = current_song.clone();
            move |lua, args: MultiValue| {
                if let Some(Value::Table(song)) = method_arg(&args, 0) {
                    current_song.raw_set(1, song.clone())?;
                }
                note_song_lua_side_effect(lua)?;
                Ok(())
            }
        })?,
    )?;
    gamestate.set(
        "SetCurrentStyle",
        lua.create_function({
            let current_style = current_style.clone();
            move |lua, args: MultiValue| {
                let style_name = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_else(|| "single".to_string());
                current_style.raw_set(1, create_style_table(lua, &style_name)?)?;
                note_song_lua_side_effect(lua)?;
                Ok(())
            }
        })?,
    )?;
    gamestate.set(
        "SetCurrentTrail",
        lua.create_function({
            let current_trail = current_trail.clone();
            move |lua, args: MultiValue| {
                let Some(player) = method_arg(&args, 0).and_then(player_index_from_value) else {
                    note_song_lua_side_effect(lua)?;
                    return Ok(());
                };
                if let Some(Value::Table(trail)) = method_arg(&args, 1) {
                    current_trail.raw_set(player + 1, trail.clone())?;
                }
                note_song_lua_side_effect(lua)?;
                Ok(())
            }
        })?,
    )?;
    gamestate.set(
        "InsertCoin",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    for name in [
        "AddStageToPlayer",
        "ResetPlayerOptions",
        "SetPreferredDifficulty",
        "UnjoinPlayer",
    ] {
        gamestate.set(
            name,
            lua.create_function(|lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(true)
            })?,
        )?;
    }
    gamestate.set(
        "JoinPlayer",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(true)
        })?,
    )?;
    gamestate.set(
        "ApplyGameCommand",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    gamestate.set(
        "SaveProfiles",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    globals.set("GAMESTATE", gamestate)?;

    let screenman = lua.create_table()?;
    let top_screen =
        create_top_screen_table(lua, context, current_sort_order, current_song.clone())?;
    globals.set(
        "__songlua_top_screen_player_1",
        top_screen.players[0].clone(),
    )?;
    globals.set(
        "__songlua_top_screen_player_2",
        top_screen.players[1].clone(),
    )?;
    globals.set("__songlua_top_screen", top_screen.top_screen.clone())?;
    let player_1_actor = top_screen.players[0].clone();
    let player_2_actor = top_screen.players[1].clone();
    globals.set(
        "GetPlayerAF",
        lua.create_function(move |_, args: MultiValue| {
            let Some(player) = method_arg(&args, 0).and_then(player_index_from_value) else {
                return Ok(Value::Nil);
            };
            Ok(Value::Table(match player {
                0 => player_1_actor.clone(),
                1 => player_2_actor.clone(),
                _ => return Ok(Value::Nil),
            }))
        })?,
    )?;
    let top_screen_table = top_screen.top_screen.clone();
    screenman.set(
        "GetTopScreen",
        lua.create_function(move |_, _args: MultiValue| Ok(top_screen_table.clone()))?,
    )?;
    screenman.set(
        "SystemMessage",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    screenman.set(
        "SetNewScreen",
        lua.create_function({
            let top_screen = top_screen.top_screen.clone();
            move |lua, args: MultiValue| {
                if let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) {
                    top_screen.set("Name", name)?;
                }
                note_song_lua_side_effect(lua)?;
                Ok(())
            }
        })?,
    )?;
    screenman.set(
        "AddNewScreenToTop",
        lua.create_function({
            let top_screen = top_screen.top_screen.clone();
            move |lua, args: MultiValue| {
                if let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) {
                    top_screen.set("Name", name)?;
                }
                note_song_lua_side_effect(lua)?;
                Ok(())
            }
        })?,
    )?;
    screenman.set(
        "set_input_redirected",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    globals.set("SCREENMAN", screenman)?;
    globals.set(SONG_LUA_SOUND_PATHS_KEY, lua.create_table()?)?;
    globals.set(
        "SOUND",
        create_sound_table(lua, context.song_dir.as_path())?,
    )?;

    let messageman = lua.create_table()?;
    let messageman_for_broadcast = messageman.clone();
    messageman.set(
        "Broadcast",
        lua.create_function(move |lua, args: MultiValue| {
            if let Some(message) = method_arg(&args, 0).cloned().and_then(read_string) {
                let params = method_arg(&args, 1).cloned();
                note_song_lua_side_effect(lua)?;
                record_song_lua_broadcast(lua, &message, params.is_some())?;
                broadcast_song_lua_message(lua, &message, params)?;
            }
            Ok(messageman_for_broadcast.clone())
        })?,
    )?;
    let messageman_for_logging = messageman.clone();
    messageman.set(
        "SetLogging",
        lua.create_function(move |lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(messageman_for_logging.clone())
        })?,
    )?;
    globals.set("MESSAGEMAN", messageman)?;
    globals.set("STATSMAN", create_statsman_table(lua, context)?)?;
    globals.set(
        "SM",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    globals.set("ThemePrefs", create_theme_prefs_table(lua)?)?;
    globals.set("ThemePrefsRows", create_theme_prefs_rows_table(lua)?)?;
    globals.set("SL_CustomPrefs", create_sl_custom_prefs_table(lua)?)?;
    globals.set(
        "CustomOptionRow",
        lua.create_function(|lua, value: Value| {
            let Some(name) = read_string(value) else {
                return Ok(Value::Boolean(false));
            };
            create_custom_option_row(lua, &name)
                .map(|row| row.map_or(Value::Boolean(false), Value::Table))
        })?,
    )?;
    for name in [
        "ConfAspectRatio",
        "ConfDisplayResolution",
        "ConfDisplayMode",
        "ConfRefreshRate",
        "ConfFullscreenType",
    ] {
        globals.set(
            name,
            lua.create_function({
                let name = name.to_string();
                move |lua, _args: MultiValue| create_conf_option_row(lua, &name).map(Value::Table)
            })?,
        )?;
    }
    globals.set(
        "OperatorMenuOptionRows",
        create_operator_menu_option_rows_table(lua)?,
    )?;
    globals.set("SL", create_sl_table(lua, context)?)?;
    globals.set("Branch", create_branch_table(lua)?)?;
    globals.set(
        "SelectMusicOrCourse",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string("ScreenSelectMusic")?))
        })?,
    )?;
    globals.set("PROFILEMAN", create_profileman_table(lua)?)?;
    Ok(())
}

fn create_difficulty_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let reverse = lua.create_table()?;
    for (idx, difficulty) in [
        SongLuaDifficulty::Beginner,
        SongLuaDifficulty::Easy,
        SongLuaDifficulty::Medium,
        SongLuaDifficulty::Hard,
        SongLuaDifficulty::Challenge,
        SongLuaDifficulty::Edit,
    ]
    .into_iter()
    .enumerate()
    {
        table.raw_set(idx + 1, difficulty.sm_name())?;
        reverse.raw_set(difficulty.sm_name(), idx)?;
    }
    table.set(
        "Reverse",
        lua.create_function(move |_, _args: MultiValue| Ok(reverse.clone()))?,
    )?;
    Ok(table)
}

fn create_string_enum_table(lua: &Lua, names: &[&str]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let reverse = lua.create_table()?;
    for (idx, name) in names.iter().enumerate() {
        table.raw_set(idx + 1, *name)?;
        reverse.raw_set(*name, idx)?;
    }
    table.set(
        "Reverse",
        lua.create_function(move |_, _args: MultiValue| Ok(reverse.clone()))?,
    )?;
    Ok(table)
}

fn create_player_number_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let reverse = lua.create_table()?;
    for player in 0..LUA_PLAYERS {
        let name = player_number_name(player);
        table.raw_set(player + 1, name)?;
        table.raw_set(name, name)?;
        reverse.raw_set(name, player)?;
    }
    table.set(
        "Reverse",
        lua.create_function(move |_, _args: MultiValue| Ok(reverse.clone()))?,
    )?;
    Ok(table)
}

fn create_other_player_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.raw_set(player_number_name(0), player_number_name(1))?;
    table.raw_set(player_number_name(1), player_number_name(0))?;
    Ok(table)
}

fn create_song_util_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "GetPlayableSteps",
        lua.create_function(|lua, args: MultiValue| {
            let Some(song) = song_util_song_arg(&args)? else {
                return Ok(Value::Table(lua.create_table()?));
            };
            call_song_steps_method(lua, &song, "GetAllSteps", Value::Nil)
        })?,
    )?;
    table.set(
        "GetPlayableStepsByStepsType",
        lua.create_function(|lua, args: MultiValue| {
            let Some(song) = song_util_song_arg(&args)? else {
                return Ok(Value::Table(lua.create_table()?));
            };
            let steps_type = match args
                .iter()
                .skip_while(|value| {
                    !matches!(value, Value::Table(table) if table.to_pointer() == song.to_pointer())
                })
                .nth(1)
                .cloned()
            {
                Some(value) => value,
                None => Value::String(lua.create_string("StepsType_Dance_Single")?),
            };
            call_song_steps_method(lua, &song, "GetStepsByStepsType", steps_type)
        })?,
    )?;
    Ok(table)
}

fn song_util_song_arg(args: &MultiValue) -> mlua::Result<Option<Table>> {
    for value in args {
        let Value::Table(table) = value else {
            continue;
        };
        if matches!(table.get::<Value>("GetAllSteps")?, Value::Function(_)) {
            return Ok(Some(table.clone()));
        }
    }
    Ok(None)
}

fn call_song_steps_method(
    lua: &Lua,
    song: &Table,
    method_name: &str,
    argument: Value,
) -> mlua::Result<Value> {
    let Value::Function(method) = song.get::<Value>(method_name)? else {
        return Ok(Value::Table(lua.create_table()?));
    };
    let mut args = MultiValue::new();
    args.push_back(Value::Table(song.clone()));
    if !matches!(argument, Value::Nil) {
        args.push_back(argument);
    }
    method.call::<Value>(args)
}

fn create_screen_system_layer_helpers_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    set_string_method(lua, &table, "GetCreditsMessage", "Free Play")?;
    Ok(table)
}

fn create_branch_table(lua: &Lua) -> mlua::Result<Table> {
    let branch = lua.create_table()?;
    for (name, screen) in [
        ("AfterScreenRankingDouble", "ScreenRainbow"),
        ("TitleMenu", "ScreenTitleMenu"),
        ("AfterInit", "ScreenTitleMenu"),
        ("AllowScreenSelectProfile", "ScreenSelectProfile"),
        ("AfterSelectProfile", "ScreenSelectColor"),
        ("AllowScreenSelectColor", "ScreenSelectColor"),
        ("AfterScreenSelectColor", "ScreenSelectStyle"),
        ("AllowScreenSelectPlayMode", "ScreenSelectPlayMode"),
        ("AllowScreenSelectPlayMode2", "ScreenProfileLoad"),
        ("AfterSelectPlayMode", "ScreenSelectMusic"),
        ("AfterEvaluationStage", "ScreenProfileSave"),
        ("AfterGameplay", "ScreenEvaluationStage"),
        ("AfterHeartEntry", "ScreenEvaluationStage"),
        ("AfterSelectMusic", "ScreenGameplay"),
        ("SSMCancel", "ScreenTitleMenu"),
        ("AllowScreenNameEntry", "ScreenNameEntryTraditional"),
        ("AllowScreenEvalSummary", "ScreenEvaluationSummary"),
        ("AfterProfileSave", "ScreenSelectMusic"),
        ("AfterProfileSaveSummary", "ScreenGameOver"),
        ("GameplayScreen", "ScreenGameplay"),
    ] {
        branch.set(
            name,
            lua.create_function(move |lua, _args: MultiValue| {
                Ok(Value::String(lua.create_string(screen)?))
            })?,
        )?;
    }
    Ok(branch)
}

fn scale_value(value: f32, from_low: f32, from_high: f32, to_low: f32, to_high: f32) -> f32 {
    let span = from_high - from_low;
    if span.abs() <= f32::EPSILON {
        to_low
    } else {
        (value - from_low) / span * (to_high - to_low) + to_low
    }
}

fn seconds_to_time_parts(seconds: f64) -> (i64, i64, i64) {
    let minutes = (seconds / 60.0).trunc() as i64;
    let secs = (seconds % 60.0).trunc() as i64;
    let centis = ((seconds - (minutes * 60 + secs) as f64) * 100.0).trunc() as i64;
    let centis = centis.clamp(0, 99);
    (minutes, secs, centis)
}

fn seconds_to_hhmmss(seconds: f64) -> String {
    let (minutes, seconds, _) = seconds_to_time_parts(seconds);
    format!("{:02}:{:02}:{seconds:02}", minutes / 60, minutes % 60)
}

fn seconds_to_mss(seconds: f64) -> String {
    let (minutes, seconds, _) = seconds_to_time_parts(seconds);
    format!("{minutes:01}:{seconds:02}")
}

fn seconds_to_mmss(seconds: f64) -> String {
    let (minutes, seconds, _) = seconds_to_time_parts(seconds);
    format!("{minutes:02}:{seconds:02}")
}

fn seconds_to_mss_ms_ms(seconds: f64) -> String {
    let (minutes, seconds, centis) = seconds_to_time_parts(seconds);
    format!("{minutes:01}:{seconds:02}.{centis:02}")
}

fn seconds_to_mmss_ms_ms(seconds: f64) -> String {
    let (minutes, seconds, centis) = seconds_to_time_parts(seconds);
    format!("{minutes:02}:{seconds:02}.{centis:02}")
}

fn format_number_and_suffix(value: i64) -> String {
    let suffix = if (value % 100) / 10 == 1 {
        "th"
    } else {
        match value % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        }
    };
    format!("{value}{suffix}")
}

fn create_game_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    set_string_method(lua, &table, "GetName", "dance")?;
    Ok(table)
}

fn create_screen_table(
    lua: &Lua,
    width: f32,
    height: f32,
    center_x: f32,
    center_y: f32,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("w", width)?;
    table.set("h", height)?;
    table.set("cx", center_x)?;
    table.set("cy", center_y)?;
    table.set("l", 0.0_f32)?;
    table.set("t", 0.0_f32)?;
    table.set("r", width)?;
    table.set("b", height)?;
    Ok(table)
}

fn entry_file_path(path: &Path) -> Option<PathBuf> {
    if path.is_file() {
        return Some(path.to_path_buf());
    }
    if path.is_dir() {
        let default_lua = path.join("default.lua");
        if default_lua.is_file() {
            return Some(default_lua);
        }
    }
    None
}

fn read_mod_windows(
    table: Option<Table>,
    unit: SongLuaTimeUnit,
) -> Result<Vec<SongLuaModWindow>, String> {
    let Some(table) = table else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for value in table.sequence_values::<Value>() {
        let Value::Table(entry) = value.map_err(|err| err.to_string())? else {
            continue;
        };
        let Some(start) = read_f32(entry.raw_get::<Value>(1).map_err(|err| err.to_string())?)
        else {
            continue;
        };
        let Some(limit) = read_f32(entry.raw_get::<Value>(2).map_err(|err| err.to_string())?)
        else {
            continue;
        };
        let Some(mods) = read_string(entry.raw_get::<Value>(3).map_err(|err| err.to_string())?)
        else {
            continue;
        };
        let span_mode = read_span_mode(entry.raw_get::<Value>(4).map_err(|err| err.to_string())?)
            .unwrap_or(SongLuaSpanMode::Len);
        let player = read_player(entry.raw_get::<Value>(5).map_err(|err| err.to_string())?);
        out.push(SongLuaModWindow {
            unit,
            start,
            limit,
            span_mode,
            mods,
            player,
        });
    }
    Ok(out)
}

#[derive(Clone)]
struct RuntimeModEaseEntry {
    start: f32,
    limit: f32,
    easing: String,
    to: f32,
    target: String,
    start_val: Option<f32>,
    opt1: Option<f32>,
    opt2: Option<f32>,
    player: Option<u8>,
    add: bool,
}

fn read_runtime_mod_eases(
    table: Option<Table>,
    easing_names: &HashMap<*const c_void, String>,
    overlays: &mut [OverlayCompileActor],
    context: &SongLuaCompileContext,
) -> Result<(Vec<SongLuaEaseWindow>, Vec<SongLuaOverlayEase>), String> {
    let Some(table) = table else {
        return Ok((Vec::new(), Vec::new()));
    };
    let mut entries = Vec::new();
    for value in table.sequence_values::<Value>() {
        let Value::Table(entry) = value.map_err(|err| err.to_string())? else {
            continue;
        };
        let Some(entry) = read_runtime_mod_ease_entry(entry, easing_names)? else {
            continue;
        };
        if !entries
            .iter()
            .any(|other| runtime_mod_entries_equal(other, &entry))
        {
            entries.push(entry);
        }
    }
    if entries.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    let mut current: [HashMap<String, f32>; LUA_PLAYERS] = std::array::from_fn(|_| HashMap::new());
    let mut eases = Vec::new();
    let mut overlay_eases = Vec::new();
    let static_overlay = runtime_static_overlay_index(overlays);
    let static_player = context
        .players
        .iter()
        .position(|player| player.enabled)
        .unwrap_or(0);

    for entry in entries {
        let key = runtime_mod_key(&entry.target);
        let players = runtime_mod_entry_players(entry.player);
        if key == "static" {
            let mut static_window = None;
            for player in players {
                let from = runtime_mod_start_value(&mut current[player], &key, &entry);
                let to = runtime_mod_end_value(from, &entry);
                current[player].insert(key.clone(), to);
                if player == static_player {
                    static_window = Some((from, to));
                }
            }
            if let (Some(overlay_index), Some((from, to))) = (static_overlay, static_window) {
                overlay_eases.push(runtime_static_overlay_ease(overlay_index, &entry, from, to));
            }
            continue;
        }

        let Some(target) = runtime_mod_ease_target(&key, &entry.target) else {
            continue;
        };
        for player in players {
            let from = runtime_mod_start_value(&mut current[player], &key, &entry);
            let to = runtime_mod_end_value(from, &entry);
            current[player].insert(key.clone(), to);
            eases.push(SongLuaEaseWindow {
                unit: SongLuaTimeUnit::Beat,
                start: entry.start,
                limit: entry.limit,
                span_mode: SongLuaSpanMode::Len,
                from,
                to,
                target: target.clone(),
                easing: Some(entry.easing.clone()),
                player: Some((player + 1) as u8),
                sustain: None,
                opt1: entry.opt1,
                opt2: entry.opt2,
            });
        }
    }
    extend_runtime_mod_sustains(&mut eases);
    Ok((eases, overlay_eases))
}

fn read_runtime_mod_ease_entry(
    entry: Table,
    easing_names: &HashMap<*const c_void, String>,
) -> Result<Option<RuntimeModEaseEntry>, String> {
    let Some(start) = read_f32(entry.raw_get::<Value>(1).map_err(|err| err.to_string())?) else {
        return Ok(None);
    };
    let Some(mut limit) = read_f32(entry.raw_get::<Value>(2).map_err(|err| err.to_string())?)
    else {
        return Ok(None);
    };
    let Some(easing) = read_easing_name(
        entry.raw_get::<Value>(3).map_err(|err| err.to_string())?,
        easing_names,
    ) else {
        return Ok(None);
    };
    let Some(to) = read_f32(entry.raw_get::<Value>(4).map_err(|err| err.to_string())?) else {
        return Ok(None);
    };
    let Some(target) = read_string(entry.raw_get::<Value>(5).map_err(|err| err.to_string())?)
    else {
        return Ok(None);
    };
    if read_string(
        entry
            .raw_get::<Value>("timing")
            .map_err(|err| err.to_string())?,
    )
    .is_some_and(|value| value.eq_ignore_ascii_case("end"))
    {
        limit -= start;
    }
    if !start.is_finite() || !limit.is_finite() || limit < 0.0 || !to.is_finite() {
        return Ok(None);
    }
    let player = read_player(
        entry
            .raw_get::<Value>("plr")
            .map_err(|err| err.to_string())?,
    );
    let player = match player {
        Some(player) => Some(player),
        None => read_player(
            entry
                .raw_get::<Value>("pn")
                .map_err(|err| err.to_string())?,
        ),
    };
    Ok(Some(RuntimeModEaseEntry {
        start,
        limit,
        easing,
        to,
        target,
        start_val: read_f32(
            entry
                .raw_get::<Value>("startVal")
                .map_err(|err| err.to_string())?,
        ),
        opt1: read_f32(
            entry
                .raw_get::<Value>("opt1")
                .map_err(|err| err.to_string())?,
        ),
        opt2: read_f32(
            entry
                .raw_get::<Value>("opt2")
                .map_err(|err| err.to_string())?,
        ),
        player,
        add: truthy(
            &entry
                .raw_get::<Value>("add")
                .map_err(|err| err.to_string())?,
        ),
    }))
}

fn runtime_mod_entries_equal(left: &RuntimeModEaseEntry, right: &RuntimeModEaseEntry) -> bool {
    left.start.to_bits() == right.start.to_bits()
        && left.limit.to_bits() == right.limit.to_bits()
        && left.to.to_bits() == right.to.to_bits()
        && left.target == right.target
        && left.easing == right.easing
        && left.start_val.map(f32::to_bits) == right.start_val.map(f32::to_bits)
        && left.opt1.map(f32::to_bits) == right.opt1.map(f32::to_bits)
        && left.opt2.map(f32::to_bits) == right.opt2.map(f32::to_bits)
        && left.player == right.player
        && left.add == right.add
}

fn runtime_mod_entry_players(player: Option<u8>) -> Vec<usize> {
    match player {
        Some(player) if (1..=LUA_PLAYERS as u8).contains(&player) => vec![(player - 1) as usize],
        _ => (0..LUA_PLAYERS).collect(),
    }
}

fn runtime_mod_key(target: &str) -> String {
    target.to_ascii_lowercase()
}

fn runtime_mod_initial_value(key: &str) -> f32 {
    if matches!(key, "zoom" | "zoomx" | "zoomy" | "zoomz") {
        1.0
    } else {
        0.0
    }
}

fn runtime_mod_start_value(
    current: &mut HashMap<String, f32>,
    key: &str,
    entry: &RuntimeModEaseEntry,
) -> f32 {
    entry.start_val.unwrap_or_else(|| {
        *current
            .entry(key.to_string())
            .or_insert_with(|| runtime_mod_initial_value(key))
    })
}

fn runtime_mod_end_value(from: f32, entry: &RuntimeModEaseEntry) -> f32 {
    if entry.add { from + entry.to } else { entry.to }
}

fn runtime_mod_ease_target(key: &str, original: &str) -> Option<SongLuaEaseTarget> {
    Some(match key {
        "z" => SongLuaEaseTarget::PlayerZ,
        "rotationx" => SongLuaEaseTarget::PlayerRotationX,
        "rotationy" => SongLuaEaseTarget::PlayerRotationY,
        "rotationz" => SongLuaEaseTarget::PlayerRotationZ,
        "zoom" => SongLuaEaseTarget::PlayerZoom,
        "zoomx" => SongLuaEaseTarget::PlayerZoomX,
        "zoomy" => SongLuaEaseTarget::PlayerZoomY,
        "zoomz" => SongLuaEaseTarget::PlayerZoomZ,
        "x" | "y" => return None,
        _ => SongLuaEaseTarget::Mod(original.to_string()),
    })
}

fn runtime_static_overlay_index(overlays: &[OverlayCompileActor]) -> Option<usize> {
    overlays.iter().position(|overlay| {
        let SongLuaOverlayKind::Sprite { texture_path, .. } = &overlay.actor.kind else {
            return false;
        };
        texture_path.file_name().is_some_and(|name| {
            name.to_string_lossy()
                .eq_ignore_ascii_case("_static 4x1.png")
        })
    })
}

fn runtime_static_overlay_ease(
    overlay_index: usize,
    entry: &RuntimeModEaseEntry,
    from: f32,
    to: f32,
) -> SongLuaOverlayEase {
    SongLuaOverlayEase {
        overlay_index,
        unit: SongLuaTimeUnit::Beat,
        start: entry.start,
        limit: entry.limit,
        span_mode: SongLuaSpanMode::Len,
        from: SongLuaOverlayStateDelta {
            diffuse: Some([1.0, 1.0, 1.0, from]),
            ..SongLuaOverlayStateDelta::default()
        },
        to: SongLuaOverlayStateDelta {
            diffuse: Some([1.0, 1.0, 1.0, to]),
            ..SongLuaOverlayStateDelta::default()
        },
        easing: Some(entry.easing.clone()),
        sustain: None,
        opt1: entry.opt1,
        opt2: entry.opt2,
    }
}

fn extend_runtime_mod_sustains(windows: &mut [SongLuaEaseWindow]) {
    const DEFAULT_SUSTAIN_BEATS: f32 = 1_000_000.0;
    const SAME_TICK_EPSILON: f32 = 0.001;

    for index in 0..windows.len() {
        let end = windows[index].start + windows[index].limit;
        let next_start = windows
            .iter()
            .enumerate()
            .filter_map(|(other_index, other)| {
                if other_index == index
                    || other.player != windows[index].player
                    || other.target != windows[index].target
                    || other.start <= windows[index].start + SAME_TICK_EPSILON
                {
                    None
                } else {
                    Some(other.start)
                }
            })
            .fold(None::<f32>, |acc, start| {
                Some(acc.map_or(start, |current| current.min(start)))
            })
            .unwrap_or(DEFAULT_SUSTAIN_BEATS);
        if next_start > end + SAME_TICK_EPSILON {
            windows[index].sustain = Some(next_start - end);
        }
    }
}

fn read_eases(
    lua: &Lua,
    table: Option<Table>,
    unit: SongLuaTimeUnit,
    easing_names: &HashMap<*const c_void, String>,
    overlays: &mut [OverlayCompileActor],
) -> Result<
    (
        Vec<SongLuaEaseWindow>,
        Vec<SongLuaOverlayEase>,
        SongLuaCompileInfo,
    ),
    String,
> {
    let Some(table) = table else {
        return Ok((Vec::new(), Vec::new(), SongLuaCompileInfo::default()));
    };
    let mut out = Vec::new();
    let mut overlay_eases = Vec::new();
    let mut info = SongLuaCompileInfo::default();
    let trace_started = Instant::now();
    let mut entry_count = 0usize;
    let mut function_targets = 0usize;
    let mut overlay_capture_attempts = 0usize;
    let mut overlay_capture_outputs = 0usize;
    let mut probe_ms = 0.0;
    let mut overlay_capture_ms = 0.0;
    for value in table.sequence_values::<Value>() {
        let Value::Table(entry) = value.map_err(|err| err.to_string())? else {
            continue;
        };
        entry_count += 1;
        let Some(start) = read_f32(entry.raw_get::<Value>(1).map_err(|err| err.to_string())?)
        else {
            continue;
        };
        let Some(limit) = read_f32(entry.raw_get::<Value>(2).map_err(|err| err.to_string())?)
        else {
            continue;
        };
        let Some(from) = read_f32(entry.raw_get::<Value>(3).map_err(|err| err.to_string())?) else {
            continue;
        };
        let Some(to) = read_f32(entry.raw_get::<Value>(4).map_err(|err| err.to_string())?) else {
            continue;
        };
        let field6 = entry.raw_get::<Value>(6).map_err(|err| err.to_string())?;
        let (span_mode, easing_value, player_value, sustain_value, opt1_value, opt2_value) =
            if let Some(span_mode) = read_span_mode(field6.clone()) {
                (
                    span_mode,
                    entry.raw_get::<Value>(7).map_err(|err| err.to_string())?,
                    entry.raw_get::<Value>(8).map_err(|err| err.to_string())?,
                    entry.raw_get::<Value>(9).map_err(|err| err.to_string())?,
                    entry.raw_get::<Value>(10).map_err(|err| err.to_string())?,
                    entry.raw_get::<Value>(11).map_err(|err| err.to_string())?,
                )
            } else {
                (
                    SongLuaSpanMode::Len,
                    field6,
                    entry.raw_get::<Value>(7).map_err(|err| err.to_string())?,
                    entry.raw_get::<Value>(8).map_err(|err| err.to_string())?,
                    entry.raw_get::<Value>(9).map_err(|err| err.to_string())?,
                    entry.raw_get::<Value>(10).map_err(|err| err.to_string())?,
                )
            };
        let easing = read_easing_name(easing_value, easing_names);
        let sustain = read_f32(sustain_value);
        let opt1 = read_f32(opt1_value);
        let opt2 = read_f32(opt2_value);
        let target_value = entry.raw_get::<Value>(5).map_err(|err| err.to_string())?;
        let (target, is_function_target) = match target_value {
            Value::String(text) => (
                SongLuaEaseTarget::Mod(text.to_str().map_err(|err| err.to_string())?.to_string()),
                false,
            ),
            Value::Function(function) => {
                function_targets += 1;
                let probe_started = Instant::now();
                let (probed_target, probe_methods, probe_actor_ptrs) =
                    probe_function_ease_target(lua, &function).map_err(|err| err.to_string())?;
                probe_ms += probe_started.elapsed().as_secs_f64() * 1000.0;
                let target = probed_target.unwrap_or(SongLuaEaseTarget::Function);
                if matches!(target, SongLuaEaseTarget::Function) {
                    overlay_capture_attempts += 1;
                    let capture_started = Instant::now();
                    let captured = compile_overlay_function_ease(
                        lua,
                        overlays,
                        &function,
                        unit,
                        start,
                        limit,
                        span_mode,
                        from,
                        to,
                        easing.clone(),
                        sustain,
                        opt1,
                        opt2,
                        &probe_actor_ptrs,
                    );
                    let capture_ms = capture_started.elapsed().as_secs_f64() * 1000.0;
                    overlay_capture_ms += capture_ms;
                    if capture_ms >= 1000.0 {
                        info!(
                            "Slow song lua function ease capture: unit={unit:?} start={start:.3} limit={limit:.3} span={span_mode:?} from={from:.3} to={to:.3} easing={easing:?} probe_actors={} overlays={} capture_ms={capture_ms:.3}",
                            probe_actor_ptrs.len(),
                            overlays.len(),
                        );
                    }
                    match captured {
                        Ok(compiled) if !compiled.is_empty() => {
                            overlay_capture_outputs += compiled.len();
                            overlay_eases.extend(compiled);
                            continue;
                        }
                        _ => {
                            info.unsupported_function_eases += 1;
                            let detail = format!(
                                "function ease unit={unit:?} start={start:.3} limit={limit:.3} \
                                 span={span_mode:?} from={from:.3} to={to:.3} easing={easing:?} \
                                 probe_methods={probe_methods:?}"
                            );
                            push_unique_compile_detail(
                                &mut info.unsupported_function_ease_captures,
                                detail.clone(),
                            );
                            debug!("Unsupported song lua function ease capture: {detail}");
                        }
                    }
                }
                (target, true)
            }
            _ => continue,
        };
        if is_function_target && matches!(target, SongLuaEaseTarget::Function) {
            continue;
        }

        out.push(SongLuaEaseWindow {
            unit,
            start,
            limit,
            span_mode,
            from,
            to,
            target,
            easing,
            player: read_player(player_value),
            sustain,
            opt1,
            opt2,
        });
    }
    let elapsed_ms = trace_started.elapsed().as_secs_f64() * 1000.0;
    if elapsed_ms >= 1000.0 {
        info!(
            "Song lua read_eases timing: unit={unit:?} entries={} function_targets={} overlay_capture_attempts={} overlay_capture_outputs={} player_eases={} overlay_eases={} unsupported_function_eases={} probe_ms={probe_ms:.3} overlay_capture_ms={overlay_capture_ms:.3} elapsed_ms={elapsed_ms:.3}",
            entry_count,
            function_targets,
            overlay_capture_attempts,
            overlay_capture_outputs,
            out.len(),
            overlay_eases.len(),
            info.unsupported_function_eases,
        );
    }
    Ok((out, overlay_eases, info))
}

fn read_perframe_entries(table: Option<Table>) -> Result<Vec<SongLuaPerframeEntry>, String> {
    let Some(table) = table else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for value in table.sequence_values::<Value>() {
        let Value::Table(entry) = value.map_err(|err| err.to_string())? else {
            continue;
        };
        let Some(start) = read_f32(entry.raw_get::<Value>(1).map_err(|err| err.to_string())?)
        else {
            continue;
        };
        let Some(end) = read_f32(entry.raw_get::<Value>(2).map_err(|err| err.to_string())?) else {
            continue;
        };
        let Value::Function(function) = entry.raw_get::<Value>(3).map_err(|err| err.to_string())?
        else {
            continue;
        };
        if !start.is_finite() || !end.is_finite() || end <= start {
            continue;
        }
        out.push(SongLuaPerframeEntry {
            start,
            end,
            function,
        });
    }
    Ok(out)
}

#[inline(always)]
fn perframe_segment_step(len: f32) -> f32 {
    (len / 96.0).clamp(1.0 / 192.0, 0.125)
}

#[inline(always)]
fn perframe_delta_seconds(context: &SongLuaCompileContext, delta_beats: f32) -> f32 {
    song_elapsed_seconds_for_beat(
        delta_beats,
        song_display_bps(context),
        song_music_rate(context),
    )
}

fn tracked_player_tables(tracked_actors: &[TrackedCompileActor]) -> [Option<Table>; LUA_PLAYERS] {
    let mut out = std::array::from_fn(|_| None);
    for tracked in tracked_actors {
        if let TrackedCompileActorTarget::Player(player) = tracked.target {
            out[player] = Some(tracked.table.clone());
        }
    }
    out
}

fn actor_perframe_player_state(actor: &Table) -> Result<SongLuaPerframePlayerState, String> {
    let zoom = actor
        .get::<Option<f32>>("__songlua_state_zoom")
        .map_err(|err| err.to_string())?;
    Ok(SongLuaPerframePlayerState {
        x: actor
            .get::<Option<f32>>("__songlua_state_x")
            .map_err(|err| err.to_string())?,
        y: actor
            .get::<Option<f32>>("__songlua_state_y")
            .map_err(|err| err.to_string())?,
        z: actor
            .get::<Option<f32>>("__songlua_state_z")
            .map_err(|err| err.to_string())?,
        rotation_x: actor
            .get::<Option<f32>>("__songlua_state_rot_x_deg")
            .map_err(|err| err.to_string())?,
        rotation_z: actor
            .get::<Option<f32>>("__songlua_state_rot_z_deg")
            .map_err(|err| err.to_string())?,
        rotation_y: actor
            .get::<Option<f32>>("__songlua_state_rot_y_deg")
            .map_err(|err| err.to_string())?,
        zoom_x: actor
            .get::<Option<f32>>("__songlua_state_zoom_x")
            .map_err(|err| err.to_string())?
            .or(zoom),
        zoom_y: actor
            .get::<Option<f32>>("__songlua_state_zoom_y")
            .map_err(|err| err.to_string())?
            .or(zoom),
        zoom_z: actor
            .get::<Option<f32>>("__songlua_state_zoom_z")
            .map_err(|err| err.to_string())?
            .or(zoom),
        skew_x: actor
            .get::<Option<f32>>("__songlua_state_skew_x")
            .map_err(|err| err.to_string())?,
        skew_y: actor
            .get::<Option<f32>>("__songlua_state_skew_y")
            .map_err(|err| err.to_string())?,
    })
}

#[inline(always)]
fn relative_player_target(value: Option<f32>, baseline: Option<f32>) -> Option<f32> {
    value.map(|value| value - baseline.unwrap_or(0.0))
}

fn current_perframe_player_states(
    player_tables: &[Option<Table>; LUA_PLAYERS],
) -> Result<[SongLuaPerframePlayerState; LUA_PLAYERS], String> {
    let mut out = [SongLuaPerframePlayerState::default(); LUA_PLAYERS];
    for player in 0..LUA_PLAYERS {
        let Some(actor) = player_tables[player].as_ref() else {
            continue;
        };
        out[player] = actor_perframe_player_state(actor)?;
    }
    Ok(out)
}

fn current_overlay_states(
    overlays: &[OverlayCompileActor],
) -> Result<Vec<SongLuaOverlayState>, String> {
    let mut out = Vec::with_capacity(overlays.len());
    for overlay in overlays {
        out.push(actor_overlay_initial_state(&overlay.table)?);
    }
    Ok(out)
}

fn active_perframe_entries<'a>(
    entries: &'a [SongLuaPerframeEntry],
    start: f32,
    end: f32,
) -> Vec<&'a SongLuaPerframeEntry> {
    let mid = start + 0.5 * (end - start);
    entries
        .iter()
        .filter(|entry| mid > entry.start && mid < entry.end)
        .collect()
}

fn call_perframe_entry(
    lua: &Lua,
    entry: &SongLuaPerframeEntry,
    beat: f32,
    delta_beats: f32,
    delta_seconds: f32,
) -> Result<bool, String> {
    let previous = compile_song_runtime_values(lua).map_err(|err| err.to_string())?;
    let previous_delta = compile_song_runtime_delta_values(lua).map_err(|err| err.to_string())?;
    let side_effect_before = song_lua_side_effect_count(lua).map_err(|err| err.to_string())?;
    set_compile_song_runtime_beat(lua, beat).map_err(|err| err.to_string())?;
    set_compile_song_runtime_delta_values(lua, delta_beats, delta_seconds)
        .map_err(|err| err.to_string())?;
    let result = entry
        .function
        .call::<Value>((beat, delta_seconds))
        .map(|_| ())
        .map_err(|err| err.to_string());
    set_compile_song_runtime_values(lua, previous.0, previous.1).map_err(|err| err.to_string())?;
    set_compile_song_runtime_delta_values(lua, previous_delta.0, previous_delta.1)
        .map_err(|err| err.to_string())?;
    let saw_side_effect =
        song_lua_side_effect_count(lua).map_err(|err| err.to_string())? > side_effect_before;
    result?;
    Ok(saw_side_effect)
}

fn update_function_end_beat(context: &SongLuaCompileContext) -> f32 {
    let seconds = context.music_length_seconds.max(0.0);
    let beats = seconds * song_display_bps(context) * song_music_rate(context);
    beats.max(0.0)
}

fn update_function_sample_step(len: f32) -> f32 {
    if len <= 0.0 {
        return 0.0;
    }
    let capped = len / SONG_LUA_UPDATE_FUNCTION_MAX_SAMPLES as f32;
    perframe_segment_step(len).max(capped)
}

fn call_update_functions_at(
    lua: &Lua,
    root: &Value,
    beat: f32,
    delta_beats: f32,
    delta_seconds: f32,
) -> Result<(), String> {
    let previous = compile_song_runtime_values(lua).map_err(|err| err.to_string())?;
    let previous_delta = compile_song_runtime_delta_values(lua).map_err(|err| err.to_string())?;
    set_compile_song_runtime_beat(lua, beat).map_err(|err| err.to_string())?;
    set_compile_song_runtime_delta_values(lua, delta_beats, delta_seconds)
        .map_err(|err| err.to_string())?;
    let result = run_actor_update_functions_with_delta(lua, root, delta_seconds as f64)
        .map_err(|err| err.to_string());
    set_compile_song_runtime_values(lua, previous.0, previous.1).map_err(|err| err.to_string())?;
    set_compile_song_runtime_delta_values(lua, previous_delta.0, previous_delta.1)
        .map_err(|err| err.to_string())?;
    result
}

#[derive(Clone)]
struct MultitapDesc {
    lane: usize,
    taps: Vec<f32>,
    peak: Option<f32>,
}

#[derive(Clone, Copy)]
struct MultitapPhase {
    pos: f32,
    squish: f32,
    lin: f32,
    visible: bool,
}

fn compile_multitap_update_overlays(
    lua: &Lua,
    context: &SongLuaCompileContext,
    overlays: &mut [OverlayCompileActor],
) -> Result<Option<Vec<SongLuaOverlayEase>>, String> {
    let Some(multitaps) = read_multitap_descs(lua, context)? else {
        return Ok(None);
    };
    if multitaps.is_empty() {
        return Ok(None);
    }
    let overlay_indices = named_overlay_indices(overlays);
    let mut out = Vec::new();
    for player in 0..LUA_PLAYERS {
        if !context.players[player].enabled {
            continue;
        }
        let pn = player + 1;
        let Some(&field_index) = overlay_indices.get(format!("MultitapFrameP{pn}").as_str()) else {
            return Ok(None);
        };
        apply_multitap_field_state(
            &mut overlays[field_index].actor.initial_state,
            context,
            player,
        );
        for (mti, desc) in multitaps.iter().enumerate() {
            let index = mti + 1;
            let Some(&frame_index) = overlay_indices.get(format!("MultitapP{pn}_{index}").as_str())
            else {
                return Ok(None);
            };
            let Some(&arrow_index) =
                overlay_indices.get(format!("MultitapArrowP{pn}_{index}").as_str())
            else {
                return Ok(None);
            };
            let Some(&deco_index) =
                overlay_indices.get(format!("MultitapDeco{pn}_{index}").as_str())
            else {
                return Ok(None);
            };
            push_multitap_actor_eases(
                &mut out,
                overlays,
                frame_index,
                arrow_index,
                deco_index,
                context,
                player,
                desc,
            );
        }
        for lane in 1..=SONG_LUA_NOTE_COLUMNS {
            let Some(&explosion_index) =
                overlay_indices.get(format!("MultitapExplosionP{pn}_{lane}").as_str())
            else {
                continue;
            };
            push_multitap_explosion_eases(
                &mut out,
                overlays,
                explosion_index,
                context,
                &multitaps,
                lane,
            );
        }
    }
    Ok(Some(out))
}

fn apply_multitap_field_state(
    state: &mut SongLuaOverlayState,
    context: &SongLuaCompileContext,
    player: usize,
) {
    state.visible = true;
    state.x = context.players[player].screen_x;
    state.y = context.players[player].screen_y;
    state.z = 0.0;
    state.zoom_x = 1.0;
    state.zoom_y = 1.0;
    state.zoom_z = 1.0;
}

fn named_overlay_indices(overlays: &[OverlayCompileActor]) -> HashMap<String, usize> {
    let mut out = HashMap::new();
    for (index, overlay) in overlays.iter().enumerate() {
        if let Some(name) = overlay.actor.name.as_ref() {
            out.insert(name.clone(), index);
        }
    }
    out
}

fn read_multitap_descs(
    lua: &Lua,
    context: &SongLuaCompileContext,
) -> Result<Option<Vec<MultitapDesc>>, String> {
    let globals = lua.globals();
    let Some(multitaps) = globals
        .get::<Option<Table>>("multitaps")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let difficulty = context.players[0]
        .difficulty
        .sm_name()
        .trim_start_matches("Difficulty_");
    let table = multitaps
        .get::<Option<Table>>(difficulty)
        .map_err(|err| err.to_string())?
        .or_else(|| multitaps.get::<Option<Table>>("Challenge").ok().flatten());
    let Some(table) = table else {
        return Ok(None);
    };
    let mut out = Vec::new();
    for value in table.sequence_values::<Value>() {
        let Value::Table(entry) = value.map_err(|err| err.to_string())? else {
            continue;
        };
        let Some(lane) = entry
            .get::<Option<i64>>("lane")
            .map_err(|err| err.to_string())?
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| (1..=SONG_LUA_DOUBLE_NOTE_COLUMNS).contains(value))
        else {
            continue;
        };
        let Some(taps_table) = entry
            .get::<Option<Table>>("taps")
            .map_err(|err| err.to_string())?
        else {
            continue;
        };
        let mut taps = Vec::new();
        for tap in taps_table.sequence_values::<Value>() {
            if let Some(tap) = read_f32(tap.map_err(|err| err.to_string())?)
                && tap.is_finite()
            {
                taps.push(tap);
            }
        }
        if taps.is_empty() {
            continue;
        }
        taps.sort_by(|left, right| left.total_cmp(right));
        let peak = entry
            .get::<Value>("peak")
            .map_err(|err| err.to_string())
            .ok()
            .and_then(read_f32)
            .filter(|value| value.is_finite());
        out.push(MultitapDesc { lane, taps, peak });
    }
    Ok(Some(out))
}

fn push_multitap_actor_eases(
    out: &mut Vec<SongLuaOverlayEase>,
    overlays: &[OverlayCompileActor],
    frame_index: usize,
    arrow_index: usize,
    deco_index: usize,
    context: &SongLuaCompileContext,
    player: usize,
    desc: &MultitapDesc,
) {
    let start = desc.taps[0] - MULTITAP_PREVISIBLE_BEATS;
    let end = desc.taps[desc.taps.len() - 1] + MULTITAP_SAMPLE_STEP;
    let mut frame_samples = Vec::new();
    let mut arrow_samples = Vec::new();
    let mut deco_samples = Vec::new();
    let mut beat = start;
    loop {
        let phase = calc_multitap_phase(desc, beat);
        frame_samples.push((
            beat,
            multitap_frame_state(
                overlays[frame_index].actor.initial_state,
                context,
                player,
                desc.lane,
                phase,
            ),
        ));
        arrow_samples.push((
            beat,
            multitap_arrow_state(overlays[arrow_index].actor.initial_state, desc.lane, phase),
        ));
        deco_samples.push((
            beat,
            multitap_deco_state(overlays[deco_index].actor.initial_state, phase),
        ));
        if beat >= end - f32::EPSILON {
            break;
        }
        beat = (beat + MULTITAP_SAMPLE_STEP).min(end);
    }
    push_overlay_sample_eases(
        out,
        frame_index,
        overlays[frame_index].actor.initial_state,
        &frame_samples,
    );
    push_overlay_sample_eases(
        out,
        arrow_index,
        overlays[arrow_index].actor.initial_state,
        &arrow_samples,
    );
    push_overlay_sample_eases(
        out,
        deco_index,
        overlays[deco_index].actor.initial_state,
        &deco_samples,
    );
}

fn push_multitap_explosion_eases(
    out: &mut Vec<SongLuaOverlayEase>,
    overlays: &[OverlayCompileActor],
    overlay_index: usize,
    context: &SongLuaCompileContext,
    descs: &[MultitapDesc],
    lane: usize,
) {
    let mut ranges = descs
        .iter()
        .filter(|desc| desc.lane == lane)
        .map(|desc| {
            (
                desc.taps[0] - MULTITAP_PREVISIBLE_BEATS,
                desc.taps[desc.taps.len() - 1] + MULTITAP_SAMPLE_STEP,
            )
        })
        .collect::<Vec<_>>();
    if ranges.is_empty() {
        return;
    }
    ranges.sort_by(|left, right| left.0.total_cmp(&right.0));
    let baseline = overlays[overlay_index].actor.initial_state;
    let mut samples = Vec::new();
    for (start, end) in ranges {
        let mut beat = start;
        loop {
            let visible = descs
                .iter()
                .any(|desc| desc.lane == lane && calc_multitap_phase(desc, beat).visible);
            samples.push((
                beat,
                multitap_explosion_state(baseline, context, lane, visible),
            ));
            if beat >= end - f32::EPSILON {
                break;
            }
            beat = (beat + MULTITAP_SAMPLE_STEP).min(end);
        }
    }
    samples.sort_by(|left, right| left.0.total_cmp(&right.0));
    samples.dedup_by(|left, right| (left.0 - right.0).abs() <= f32::EPSILON);
    push_overlay_sample_eases(out, overlay_index, baseline, &samples);
}

fn push_overlay_sample_eases(
    out: &mut Vec<SongLuaOverlayEase>,
    overlay_index: usize,
    baseline: SongLuaOverlayState,
    samples: &[(f32, SongLuaOverlayState)],
) {
    for window in samples.windows(2) {
        let [(start, from), (end, to)] = [window[0], window[1]];
        if end <= start || from == to {
            continue;
        }
        let Some((from, to)) = overlay_delta_pair_from_states(baseline, from, to) else {
            continue;
        };
        out.push(SongLuaOverlayEase {
            overlay_index,
            unit: SongLuaTimeUnit::Beat,
            start,
            limit: end - start,
            span_mode: SongLuaSpanMode::Len,
            from,
            to,
            easing: Some("linear".to_string()),
            sustain: None,
            opt1: None,
            opt2: None,
        });
    }
}

fn calc_multitap_phase(desc: &MultitapDesc, beat: f32) -> MultitapPhase {
    let mut out = MultitapPhase {
        pos: 0.0,
        squish: 0.0,
        lin: 0.0,
        visible: false,
    };
    if beat > desc.taps[desc.taps.len() - 1] {
        return out;
    }
    out.pos = desc.taps[0] - beat;
    out.visible = out.pos < MULTITAP_PREVISIBLE_BEATS;
    let mut elasticity = desc
        .peak
        .zip(desc.taps.get(1).copied())
        .map(|(peak, second)| peak / (second - desc.taps[0]))
        .unwrap_or(MULTITAP_BASE_BOUNCE);
    for index in 0..desc.taps.len() {
        if beat <= desc.taps[index] || index + 1 >= desc.taps.len() {
            break;
        }
        let gap = desc.taps[index + 1] - desc.taps[index];
        if gap <= f32::EPSILON {
            continue;
        }
        elasticity = desc
            .peak
            .map(|peak| peak / gap)
            .unwrap_or(elasticity * MULTITAP_ELASTICITY);
        let t = beat - desc.taps[index];
        out.pos = elasticity * t * (gap - t) / gap;
        let velocity = elasticity * (gap - 2.0 * t) / gap;
        out.squish = MULTITAP_SQUISHY * (velocity.abs() - 0.5);
        out.lin = t / gap;
        out.visible = true;
    }
    out
}

fn multitap_frame_state(
    baseline: SongLuaOverlayState,
    context: &SongLuaCompileContext,
    player: usize,
    lane: usize,
    phase: MultitapPhase,
) -> SongLuaOverlayState {
    if !phase.visible {
        return baseline;
    }
    let mut state = baseline;
    state.visible = true;
    state.x = song_lua_style_column_x(&context.style_name, lane - 1);
    state.y = THEME_RECEPTOR_Y_STD + multitap_y_offset(context, player, phase.pos);
    state.z = 0.0;
    state.zoom_x = 1.0;
    state.zoom_y = 1.0 + phase.squish;
    state.zoom_z = 1.0;
    state.diffuse[3] = 1.0;
    state
}

fn multitap_y_offset(context: &SongLuaCompileContext, player: usize, pos_beats: f32) -> f32 {
    pos_beats * 64.0 * song_lua_speedmod_multiplier(context, player)
}

fn song_lua_speedmod_multiplier(context: &SongLuaCompileContext, player: usize) -> f32 {
    let player = &context.players[player];
    let reference_bpm = player.display_bpms[1].max(player.display_bpms[0]).max(1.0);
    let music_rate = if context.song_music_rate.is_finite() && context.song_music_rate > 0.0 {
        context.song_music_rate
    } else {
        1.0
    };
    let multiplier = match player.speedmod {
        SongLuaSpeedMod::X(value) => value,
        SongLuaSpeedMod::C(value) | SongLuaSpeedMod::M(value) | SongLuaSpeedMod::A(value) => {
            value / reference_bpm / music_rate
        }
    };
    if multiplier.is_finite() && multiplier > 0.0 {
        multiplier
    } else {
        1.0
    }
}

fn multitap_arrow_state(
    baseline: SongLuaOverlayState,
    lane: usize,
    phase: MultitapPhase,
) -> SongLuaOverlayState {
    if !phase.visible {
        return baseline;
    }
    let mut state = baseline;
    state.visible = true;
    state.rot_z_deg = MULTITAP_LANE_ROTATION[lane - 1];
    state.diffuse = [0.4, 0.4, 0.4, 1.0];
    state
}

fn multitap_deco_state(baseline: SongLuaOverlayState, phase: MultitapPhase) -> SongLuaOverlayState {
    if !phase.visible {
        return baseline;
    }
    let mut state = baseline;
    state.visible = true;
    state.zoom = 1.0;
    state.z = 10.0;
    state.rot_z_deg = phase.lin * 180.0;
    state.effect_mode = EffectMode::DiffuseRamp;
    state.effect_clock = EffectClock::Beat;
    state.effect_color1 = [1.0, 1.0, 1.0, 1.0];
    state.effect_color2 = [0.8, 0.8, 0.8, 1.0];
    state.effect_period = 1.0;
    state
}

fn multitap_explosion_state(
    baseline: SongLuaOverlayState,
    context: &SongLuaCompileContext,
    lane: usize,
    visible: bool,
) -> SongLuaOverlayState {
    let mut state = baseline;
    state.visible = visible;
    state.x = song_lua_style_column_x(&context.style_name, lane - 1);
    state.y = THEME_RECEPTOR_Y_STD;
    state.z = 0.0;
    state.rot_z_deg = MULTITAP_LANE_ROTATION[lane - 1];
    state
}

fn compile_update_function_overlays(
    lua: &Lua,
    root: &Value,
    context: &SongLuaCompileContext,
    overlays: &mut [OverlayCompileActor],
    tracked_actors: &[TrackedCompileActor],
) -> Result<Vec<SongLuaOverlayEase>, String> {
    if overlays.is_empty()
        || !actor_tree_has_update_functions(lua, root).map_err(|err| err.to_string())?
    {
        return Ok(Vec::new());
    }
    let start = 0.0;
    let end = update_function_end_beat(context);
    if end <= start {
        return Ok(Vec::new());
    }

    reset_overlay_capture_tables(lua, overlays)?;
    reset_tracked_capture_tables(lua, tracked_actors)?;
    call_update_functions_at(lua, root, start, 0.0, 0.0)?;
    let baseline_overlays = current_overlay_states(overlays)?;
    let step = update_function_sample_step(end - start);
    let mut sample_beats = vec![start];
    let mut overlay_samples = vec![baseline_overlays.clone()];
    let mut beat = (start + step).min(end);
    let mut prev_eval = Some(start);

    loop {
        let eval_beat = beat;
        let delta_beats = prev_eval
            .map(|prev| (eval_beat - prev).abs())
            .unwrap_or(0.0);
        let delta_seconds = perframe_delta_seconds(context, delta_beats);
        reset_overlay_capture_tables(lua, overlays)?;
        reset_tracked_capture_tables(lua, tracked_actors)?;
        call_update_functions_at(lua, root, eval_beat, delta_beats, delta_seconds)?;
        sample_beats.push(beat);
        overlay_samples.push(current_overlay_states(overlays)?);
        prev_eval = Some(eval_beat);
        if beat >= end - f32::EPSILON {
            break;
        }
        beat = (beat + step).min(end);
    }

    let mut out = Vec::new();
    for index in 0..sample_beats.len() {
        let seg_start = sample_beats[index];
        let seg_end = sample_beats.get(index + 1).copied().unwrap_or(end);
        if seg_end <= seg_start {
            continue;
        }
        let from_overlays = &overlay_samples[index];
        let to_overlays = overlay_samples.get(index + 1).unwrap_or(from_overlays);
        for overlay_index in 0..from_overlays.len().min(to_overlays.len()) {
            if from_overlays[overlay_index] == to_overlays[overlay_index] {
                continue;
            }
            let Some((from, to)) = overlay_delta_pair_from_states(
                baseline_overlays[overlay_index],
                from_overlays[overlay_index],
                to_overlays[overlay_index],
            ) else {
                continue;
            };
            out.push(SongLuaOverlayEase {
                overlay_index,
                unit: SongLuaTimeUnit::Beat,
                start: seg_start,
                limit: seg_end - seg_start,
                span_mode: SongLuaSpanMode::Len,
                from,
                to,
                easing: Some("linear".to_string()),
                sustain: None,
                opt1: None,
                opt2: None,
            });
        }
    }
    Ok(out)
}

fn push_perframe_player_target(
    out: &mut Vec<SongLuaEaseWindow>,
    start: f32,
    end: f32,
    from: Option<f32>,
    to: Option<f32>,
    baseline: Option<f32>,
    neutral: f32,
    target: SongLuaEaseTarget,
    player: usize,
) {
    if end <= start {
        return;
    }
    let baseline = baseline.unwrap_or(neutral);
    let from = from.unwrap_or(baseline);
    let to = to.unwrap_or(baseline);
    if !from.is_finite() || !to.is_finite() {
        return;
    }
    if (from - baseline).abs() <= f32::EPSILON && (to - baseline).abs() <= f32::EPSILON {
        return;
    }
    out.push(SongLuaEaseWindow {
        unit: SongLuaTimeUnit::Beat,
        start,
        limit: end - start,
        span_mode: SongLuaSpanMode::Len,
        from,
        to,
        target,
        easing: Some("linear".to_string()),
        player: Some((player + 1) as u8),
        sustain: None,
        opt1: None,
        opt2: None,
    });
}

fn overlay_delta_pair_from_states(
    baseline: SongLuaOverlayState,
    from: SongLuaOverlayState,
    to: SongLuaOverlayState,
) -> Option<(SongLuaOverlayStateDelta, SongLuaOverlayStateDelta)> {
    let mut out_from = SongLuaOverlayStateDelta::default();
    let mut out_to = SongLuaOverlayStateDelta::default();
    macro_rules! copy_value_field {
        ($field:ident) => {
            if from.$field != baseline.$field || to.$field != baseline.$field {
                out_from.$field = Some(from.$field);
                out_to.$field = Some(to.$field);
            }
        };
    }
    macro_rules! copy_option_field {
        ($field:ident) => {
            if from.$field != baseline.$field || to.$field != baseline.$field {
                out_from.$field = from.$field;
                out_to.$field = to.$field;
            }
        };
    }
    copy_value_field!(x);
    copy_value_field!(y);
    copy_value_field!(z);
    copy_value_field!(z_bias);
    copy_value_field!(draw_order);
    copy_value_field!(draw_by_z_position);
    copy_value_field!(halign);
    copy_value_field!(valign);
    copy_value_field!(text_align);
    copy_value_field!(uppercase);
    copy_value_field!(shadow_len);
    copy_value_field!(shadow_color);
    copy_value_field!(glow);
    copy_option_field!(fov);
    copy_option_field!(vanishpoint);
    copy_value_field!(diffuse);
    copy_option_field!(vertex_colors);
    copy_value_field!(visible);
    copy_value_field!(cropleft);
    copy_value_field!(cropright);
    copy_value_field!(croptop);
    copy_value_field!(cropbottom);
    copy_value_field!(fadeleft);
    copy_value_field!(faderight);
    copy_value_field!(fadetop);
    copy_value_field!(fadebottom);
    copy_value_field!(mask_source);
    copy_value_field!(mask_dest);
    copy_value_field!(zoom);
    copy_value_field!(zoom_x);
    copy_value_field!(zoom_y);
    copy_value_field!(zoom_z);
    copy_value_field!(basezoom);
    copy_value_field!(basezoom_x);
    copy_value_field!(basezoom_y);
    copy_value_field!(basezoom_z);
    copy_value_field!(rot_x_deg);
    copy_value_field!(rot_y_deg);
    copy_value_field!(rot_z_deg);
    copy_value_field!(skew_x);
    copy_value_field!(skew_y);
    copy_value_field!(blend);
    copy_value_field!(vibrate);
    copy_value_field!(effect_magnitude);
    copy_value_field!(effect_clock);
    copy_value_field!(effect_mode);
    copy_value_field!(effect_color1);
    copy_value_field!(effect_color2);
    copy_value_field!(effect_period);
    copy_value_field!(effect_offset);
    copy_option_field!(effect_timing);
    copy_value_field!(rainbow);
    copy_value_field!(rainbow_scroll);
    copy_value_field!(text_jitter);
    copy_value_field!(text_distortion);
    copy_value_field!(text_glow_mode);
    copy_value_field!(mult_attrs_with_diffuse);
    copy_value_field!(sprite_animate);
    copy_value_field!(sprite_loop);
    copy_value_field!(sprite_playback_rate);
    copy_value_field!(sprite_state_delay);
    copy_option_field!(sprite_state_index);
    copy_option_field!(vert_spacing);
    copy_option_field!(wrap_width_pixels);
    copy_option_field!(max_width);
    copy_option_field!(max_height);
    copy_value_field!(max_w_pre_zoom);
    copy_value_field!(max_h_pre_zoom);
    copy_value_field!(max_dimension_uses_zoom);
    copy_value_field!(depth_test);
    copy_value_field!(texture_filtering);
    copy_value_field!(texture_wrapping);
    copy_option_field!(texcoord_offset);
    copy_option_field!(custom_texture_rect);
    copy_option_field!(texcoord_velocity);
    copy_option_field!(size);
    copy_option_field!(stretch_rect);
    overlay_delta_intersection(&out_from, &out_to)
}

fn compile_perframes(
    lua: &Lua,
    prefix_table: Option<Table>,
    global_table: Option<Table>,
    context: &SongLuaCompileContext,
    overlays: &mut [OverlayCompileActor],
    tracked_actors: &[TrackedCompileActor],
) -> Result<
    (
        Vec<SongLuaEaseWindow>,
        Vec<SongLuaOverlayEase>,
        SongLuaCompileInfo,
    ),
    String,
> {
    let mut entries = read_perframe_entries(prefix_table)?;
    entries.extend(read_perframe_entries(global_table)?);
    if entries.is_empty() {
        return Ok((Vec::new(), Vec::new(), SongLuaCompileInfo::default()));
    }

    let mut boundaries = entries
        .iter()
        .flat_map(|entry| [entry.start, entry.end])
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    boundaries.sort_by(|left, right| left.total_cmp(right));
    boundaries.dedup_by(|left, right| (*left - *right).abs() <= f32::EPSILON);
    if boundaries.len() < 2 {
        return Ok((Vec::new(), Vec::new(), SongLuaCompileInfo::default()));
    }

    let player_tables = tracked_player_tables(tracked_actors);
    let baseline_players = current_perframe_player_states(&player_tables)?;
    let baseline_overlays = current_overlay_states(overlays)?;
    let mut out_eases = Vec::new();
    let mut out_overlay_eases = Vec::new();
    let mut saw_recognized_side_effect = false;

    for window in boundaries.windows(2) {
        let [start, end] = [window[0], window[1]];
        if end <= start {
            continue;
        }
        let active = active_perframe_entries(&entries, start, end);
        if active.is_empty() {
            let current_players = current_perframe_player_states(&player_tables)?;
            let current_overlays = current_overlay_states(overlays)?;
            for player in 0..LUA_PLAYERS {
                push_perframe_player_target(
                    &mut out_eases,
                    start,
                    end,
                    current_players[player].x,
                    current_players[player].x,
                    baseline_players[player].x,
                    0.0,
                    SongLuaEaseTarget::PlayerX,
                    player,
                );
                push_perframe_player_target(
                    &mut out_eases,
                    start,
                    end,
                    current_players[player].y,
                    current_players[player].y,
                    baseline_players[player].y,
                    0.0,
                    SongLuaEaseTarget::PlayerY,
                    player,
                );
                push_perframe_player_target(
                    &mut out_eases,
                    start,
                    end,
                    relative_player_target(current_players[player].z, baseline_players[player].z),
                    relative_player_target(current_players[player].z, baseline_players[player].z),
                    Some(0.0),
                    0.0,
                    SongLuaEaseTarget::PlayerZ,
                    player,
                );
                push_perframe_player_target(
                    &mut out_eases,
                    start,
                    end,
                    current_players[player].rotation_x,
                    current_players[player].rotation_x,
                    baseline_players[player].rotation_x,
                    0.0,
                    SongLuaEaseTarget::PlayerRotationX,
                    player,
                );
                push_perframe_player_target(
                    &mut out_eases,
                    start,
                    end,
                    current_players[player].rotation_z,
                    current_players[player].rotation_z,
                    baseline_players[player].rotation_z,
                    0.0,
                    SongLuaEaseTarget::PlayerRotationZ,
                    player,
                );
                push_perframe_player_target(
                    &mut out_eases,
                    start,
                    end,
                    current_players[player].rotation_y,
                    current_players[player].rotation_y,
                    baseline_players[player].rotation_y,
                    0.0,
                    SongLuaEaseTarget::PlayerRotationY,
                    player,
                );
                push_perframe_player_target(
                    &mut out_eases,
                    start,
                    end,
                    current_players[player].zoom_x,
                    current_players[player].zoom_x,
                    baseline_players[player].zoom_x,
                    1.0,
                    SongLuaEaseTarget::PlayerZoomX,
                    player,
                );
                push_perframe_player_target(
                    &mut out_eases,
                    start,
                    end,
                    current_players[player].zoom_y,
                    current_players[player].zoom_y,
                    baseline_players[player].zoom_y,
                    1.0,
                    SongLuaEaseTarget::PlayerZoomY,
                    player,
                );
                push_perframe_player_target(
                    &mut out_eases,
                    start,
                    end,
                    current_players[player].zoom_z,
                    current_players[player].zoom_z,
                    baseline_players[player].zoom_z,
                    1.0,
                    SongLuaEaseTarget::PlayerZoomZ,
                    player,
                );
                push_perframe_player_target(
                    &mut out_eases,
                    start,
                    end,
                    current_players[player].skew_x,
                    current_players[player].skew_x,
                    baseline_players[player].skew_x,
                    0.0,
                    SongLuaEaseTarget::PlayerSkewX,
                    player,
                );
                push_perframe_player_target(
                    &mut out_eases,
                    start,
                    end,
                    current_players[player].skew_y,
                    current_players[player].skew_y,
                    baseline_players[player].skew_y,
                    0.0,
                    SongLuaEaseTarget::PlayerSkewY,
                    player,
                );
            }
            for (overlay_index, current) in current_overlays.iter().copied().enumerate() {
                let Some((from, to)) = overlay_delta_pair_from_states(
                    baseline_overlays[overlay_index],
                    current,
                    current,
                ) else {
                    continue;
                };
                out_overlay_eases.push(SongLuaOverlayEase {
                    overlay_index,
                    unit: SongLuaTimeUnit::Beat,
                    start,
                    limit: end - start,
                    span_mode: SongLuaSpanMode::Len,
                    from,
                    to,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                });
            }
            continue;
        }

        let step = perframe_segment_step(end - start);
        let eps = (0.5 * step).min(0.25 * (end - start)).max(1.0e-4_f32);
        let mut sample_beats = Vec::new();
        let mut player_samples = Vec::new();
        let mut overlay_samples = Vec::new();
        let mut beat = start;
        let mut prev_eval = None::<f32>;
        loop {
            let eval_beat = if beat <= start + f32::EPSILON {
                (start + eps).min(end - eps)
            } else if beat >= end - f32::EPSILON {
                (end - eps).max(start + eps)
            } else {
                beat
            };
            let delta_beats = prev_eval
                .map(|prev| (eval_beat - prev).abs())
                .unwrap_or(0.0);
            let delta_seconds = perframe_delta_seconds(context, delta_beats);
            reset_overlay_capture_tables(lua, overlays)?;
            reset_tracked_capture_tables(lua, tracked_actors)?;
            for entry in &active {
                saw_recognized_side_effect |=
                    call_perframe_entry(lua, entry, eval_beat, delta_beats, delta_seconds)?;
            }
            sample_beats.push(beat);
            player_samples.push(current_perframe_player_states(&player_tables)?);
            overlay_samples.push(current_overlay_states(overlays)?);
            prev_eval = Some(eval_beat);
            if beat >= end - f32::EPSILON {
                break;
            }
            beat = (beat + step).min(end);
            if beat > end {
                beat = end;
            }
        }

        for index in 0..sample_beats.len() {
            let seg_start = sample_beats[index];
            let seg_end = sample_beats.get(index + 1).copied().unwrap_or(end);
            if seg_end <= seg_start {
                continue;
            }
            let from_players = player_samples[index];
            let to_players = player_samples
                .get(index + 1)
                .copied()
                .unwrap_or(from_players);
            for player in 0..LUA_PLAYERS {
                push_perframe_player_target(
                    &mut out_eases,
                    seg_start,
                    seg_end,
                    from_players[player].x,
                    to_players[player].x,
                    baseline_players[player].x,
                    0.0,
                    SongLuaEaseTarget::PlayerX,
                    player,
                );
                push_perframe_player_target(
                    &mut out_eases,
                    seg_start,
                    seg_end,
                    from_players[player].y,
                    to_players[player].y,
                    baseline_players[player].y,
                    0.0,
                    SongLuaEaseTarget::PlayerY,
                    player,
                );
                push_perframe_player_target(
                    &mut out_eases,
                    seg_start,
                    seg_end,
                    relative_player_target(from_players[player].z, baseline_players[player].z),
                    relative_player_target(to_players[player].z, baseline_players[player].z),
                    Some(0.0),
                    0.0,
                    SongLuaEaseTarget::PlayerZ,
                    player,
                );
                push_perframe_player_target(
                    &mut out_eases,
                    seg_start,
                    seg_end,
                    from_players[player].rotation_x,
                    to_players[player].rotation_x,
                    baseline_players[player].rotation_x,
                    0.0,
                    SongLuaEaseTarget::PlayerRotationX,
                    player,
                );
                push_perframe_player_target(
                    &mut out_eases,
                    seg_start,
                    seg_end,
                    from_players[player].rotation_z,
                    to_players[player].rotation_z,
                    baseline_players[player].rotation_z,
                    0.0,
                    SongLuaEaseTarget::PlayerRotationZ,
                    player,
                );
                push_perframe_player_target(
                    &mut out_eases,
                    seg_start,
                    seg_end,
                    from_players[player].rotation_y,
                    to_players[player].rotation_y,
                    baseline_players[player].rotation_y,
                    0.0,
                    SongLuaEaseTarget::PlayerRotationY,
                    player,
                );
                push_perframe_player_target(
                    &mut out_eases,
                    seg_start,
                    seg_end,
                    from_players[player].zoom_x,
                    to_players[player].zoom_x,
                    baseline_players[player].zoom_x,
                    1.0,
                    SongLuaEaseTarget::PlayerZoomX,
                    player,
                );
                push_perframe_player_target(
                    &mut out_eases,
                    seg_start,
                    seg_end,
                    from_players[player].zoom_y,
                    to_players[player].zoom_y,
                    baseline_players[player].zoom_y,
                    1.0,
                    SongLuaEaseTarget::PlayerZoomY,
                    player,
                );
                push_perframe_player_target(
                    &mut out_eases,
                    seg_start,
                    seg_end,
                    from_players[player].zoom_z,
                    to_players[player].zoom_z,
                    baseline_players[player].zoom_z,
                    1.0,
                    SongLuaEaseTarget::PlayerZoomZ,
                    player,
                );
                push_perframe_player_target(
                    &mut out_eases,
                    seg_start,
                    seg_end,
                    from_players[player].skew_x,
                    to_players[player].skew_x,
                    baseline_players[player].skew_x,
                    0.0,
                    SongLuaEaseTarget::PlayerSkewX,
                    player,
                );
                push_perframe_player_target(
                    &mut out_eases,
                    seg_start,
                    seg_end,
                    from_players[player].skew_y,
                    to_players[player].skew_y,
                    baseline_players[player].skew_y,
                    0.0,
                    SongLuaEaseTarget::PlayerSkewY,
                    player,
                );
            }
            let from_overlays = &overlay_samples[index];
            let to_overlays = overlay_samples.get(index + 1).unwrap_or(from_overlays);
            for overlay_index in 0..from_overlays.len().min(to_overlays.len()) {
                let Some((from, to)) = overlay_delta_pair_from_states(
                    baseline_overlays[overlay_index],
                    from_overlays[overlay_index],
                    to_overlays[overlay_index],
                ) else {
                    continue;
                };
                out_overlay_eases.push(SongLuaOverlayEase {
                    overlay_index,
                    unit: SongLuaTimeUnit::Beat,
                    start: seg_start,
                    limit: seg_end - seg_start,
                    span_mode: SongLuaSpanMode::Len,
                    from,
                    to,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                });
            }
        }
    }

    let mut info = SongLuaCompileInfo::default();
    if out_eases.is_empty() && out_overlay_eases.is_empty() && !saw_recognized_side_effect {
        info.unsupported_perframes = entries.len();
        for entry in &entries {
            push_unique_compile_detail(
                &mut info.unsupported_perframe_captures,
                format!("perframe start={:.3} end={:.3}", entry.start, entry.end),
            );
        }
    }
    Ok((out_eases, out_overlay_eases, info))
}

fn read_actions(
    lua: &Lua,
    table: Option<Table>,
    overlays: &mut [OverlayCompileActor],
    tracked_actors: &mut [TrackedCompileActor],
    messages: &mut Vec<SongLuaMessageEvent>,
    counter: &mut usize,
    info: &mut SongLuaCompileInfo,
) -> Result<(), String> {
    let Some(table) = table else {
        return Ok(());
    };
    for value in table.sequence_values::<Value>() {
        let Value::Table(entry) = value.map_err(|err| err.to_string())? else {
            continue;
        };
        let Some(beat) = read_f32(entry.raw_get::<Value>(1).map_err(|err| err.to_string())?) else {
            continue;
        };
        let action = entry.raw_get::<Value>(2).map_err(|err| err.to_string())?;
        let persists = truthy(&entry.raw_get::<Value>(3).map_err(|err| err.to_string())?);
        match action {
            Value::String(text) => messages.push(SongLuaMessageEvent {
                beat,
                message: text.to_str().map_err(|err| err.to_string())?.to_string(),
                persists,
            }),
            Value::Function(function) => {
                if !matches!(
                    compile_function_action(
                        lua,
                        overlays,
                        tracked_actors,
                        &function,
                        beat,
                        persists,
                        counter,
                        messages,
                    ),
                    Ok(true)
                ) {
                    info.unsupported_function_actions += 1;
                    let detail = format!("function action beat={beat:.3} persists={persists}");
                    push_unique_compile_detail(
                        &mut info.unsupported_function_action_captures,
                        detail.clone(),
                    );
                    debug!("Unsupported song lua function action capture: {detail}");
                }
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
